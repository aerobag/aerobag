// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use std::io::{self, Read};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc;
use std::time::Instant;

// This bounds a broken test, not HTTP latency. curl owns the 200ms transfer
// deadline; process scheduling and filesystem cleanup have no latency contract.
const WATCHDOG: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq)]
enum Stop {
    FetchCompleted,
    WatchdogExpired,
    TestAborted,
}

#[derive(Debug)]
struct EndpointReport {
    stop: Stop,
    connections: usize,
}

struct StalledEndpoint {
    address: SocketAddr,
    stop: mpsc::Sender<Stop>,
    worker: Option<thread::JoinHandle<io::Result<EndpointReport>>>,
}

impl StalledEndpoint {
    fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (stop, received) = mpsc::channel();
        let worker = thread::spawn(move || {
            // TCP connects into the backlog; no HTTP headers are ever sent.
            // Releasing the listening socket AND accepted streams unblocks a
            // curl whose deadline was accidentally removed, before we join it.
            let stop = match received.recv_timeout(WATCHDOG) {
                Ok(stop) => stop,
                Err(mpsc::RecvTimeoutError::Timeout) => Stop::WatchdogExpired,
                Err(mpsc::RecvTimeoutError::Disconnected) => Stop::TestAborted,
            };
            let mut connections = 0;
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        connections += 1;
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error),
                }
            }
            let report = EndpointReport { stop, connections };
            eprintln!("stalled HTTP endpoint: {report:?}");
            Ok(report)
        });
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }

    fn finish(mut self) -> io::Result<EndpointReport> {
        let _ = self.stop.send(Stop::FetchCompleted);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("stalled endpoint panicked")
    }
}

impl Drop for StalledEndpoint {
    fn drop(&mut self) {
        let _ = self.stop.send(Stop::TestAborted);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn check_stalled_fetch(cached: bool, authenticated: bool) -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let endpoint = StalledEndpoint::start()?;
    let url = format!("http://{}/stalled.json", endpoint.address);
    let headers = if authenticated {
        BTreeMap::from([("Authorization".to_string(), "test-only-token".to_string())])
    } else {
        BTreeMap::new()
    };
    let timeouts = NetworkTimeouts {
        connect: Duration::from_secs(1),
        total: Duration::from_millis(200),
    };
    let archive_path = temp.path().join("stalled.json");
    eprintln!(
        "fetch starting: cached={cached} authenticated={authenticated} timeouts={timeouts:?}"
    );
    let started = Instant::now();
    let result = if cached {
        fetch_network_with_cache_once(&NetworkFetchRequest {
            layout: &CacheLayout::new(temp.path().join("cache")),
            cache_key: &url,
            network_url: &url,
            headers: &headers,
            force_http1: false,
            allow_html: false,
            timeouts,
            file_name: "stalled.json",
            dest_dir: temp.path(),
            archive_path: &archive_path,
        })
        .map(|_| ())
    } else {
        fetch_network_once(&url, &headers, false, timeouts, "stalled.json", temp.path())
    };
    let report = endpoint.finish()?;
    eprintln!(
        "fetch and cleanup returned after {:?}: {result:?}",
        started.elapsed()
    );
    assert_eq!(
        report.stop,
        Stop::FetchCompleted,
        "test watchdog fired; {report:?}; {result:?}"
    );
    assert_eq!(
        report.connections, 1,
        "curl must connect once to the stalled endpoint"
    );
    let error = result.expect_err("stalled response succeeded");
    let failure = error
        .downcast_ref::<CurlFailure>()
        .expect("expected curl exit failure");
    assert_eq!(
        failure.status.code(),
        Some(28),
        "expected curl transfer timeout: {error:#}"
    );
    let diagnostic = error.to_string();
    assert!(diagnostic.contains(&url));
    assert!(
        diagnostic.contains(failure.stderr.trim()),
        "plain Display lost curl diagnostics"
    );
    assert!(
        !archive_path.exists(),
        "timed-out response became a valid download"
    );
    assert_eq!(
        fs::read_dir(temp.path())?.count(),
        0,
        "failed attempt leaked partial files"
    );
    assert!(!format!("{error:#}").contains("test-only-token"));
    Ok(())
}

#[test]
fn uncached_stalled_http_times_out() -> anyhow::Result<()> {
    check_stalled_fetch(false, false)
}

#[test]
fn uncached_authenticated_stalled_http_times_out() -> anyhow::Result<()> {
    check_stalled_fetch(false, true)
}

#[test]
fn cached_stalled_http_times_out() -> anyhow::Result<()> {
    check_stalled_fetch(true, false)
}

#[test]
fn cached_authenticated_stalled_http_times_out() -> anyhow::Result<()> {
    check_stalled_fetch(true, true)
}

#[test]
fn watchdog_releases_stalled_connections_and_reports_failure() -> anyhow::Result<()> {
    let endpoint = StalledEndpoint::start()?;
    let mut stream = TcpStream::connect(endpoint.address)?;
    stream.set_read_timeout(Some(WATCHDOG))?;
    // Force the watchdog's event, not a slow wall-clock wait. Completion after
    // expiry must not overwrite that failure with FetchCompleted.
    endpoint.stop.send(Stop::WatchdogExpired)?;
    let report = endpoint.finish()?;
    assert_eq!(report.stop, Stop::WatchdogExpired);
    assert_eq!(report.connections, 1);
    assert_eq!(stream.read(&mut [0])?, 0);
    Ok(())
}

#[test]
fn abandoning_endpoint_releases_connections() -> anyhow::Result<()> {
    let endpoint = StalledEndpoint::start()?;
    let mut stream = TcpStream::connect(endpoint.address)?;
    stream.set_read_timeout(Some(WATCHDOG))?;
    drop(endpoint);
    assert_eq!(stream.read(&mut [0])?, 0);
    Ok(())
}
