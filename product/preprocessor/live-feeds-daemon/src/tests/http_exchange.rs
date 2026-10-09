// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Functional loopback HTTP tests: explicit progress, one hang watchdog, and
//! joined teardown. Socket read deadlines are not server-throughput assertions.

use super::*;
use std::net::Shutdown;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver};

const WATCHDOG: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    ServerEntered,
    ClientSending,
    ClientReading,
    ResponseStarted,
}

#[derive(Debug)]
enum Event {
    Progress(Phase),
    Finished(&'static str, Option<String>),
}

enum Watchdog {
    Elapsed,
    AtPhase(Phase),
}

struct CloseSockets(Vec<TcpStream>);

impl Drop for CloseSockets {
    fn drop(&mut self) {
        for socket in &self.0 {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

pub(super) struct HttpExchange {
    pub(super) address: SocketAddr,
    client: TcpStream,
    server: TcpStream,
}

impl HttpExchange {
    pub(super) fn connect() -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let client = TcpStream::connect_timeout(&address, WATCHDOG)?;
        let (server, _) = listener.accept()?;
        Ok(Self {
            address,
            client,
            server,
        })
    }

    pub(super) fn request(
        self,
        request: &str,
        server: impl FnOnce(TcpStream) -> anyhow::Result<()> + Send,
    ) -> anyhow::Result<String> {
        self.run(
            server,
            |mut client, events| {
                events.send(Event::Progress(Phase::ClientSending))?;
                client
                    .write_all(request.as_bytes())
                    .context("sending request")?;
                client.shutdown(Shutdown::Write)?;
                events.send(Event::Progress(Phase::ClientReading))?;
                let mut response = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = client.read(&mut buffer).context("reading response")?;
                    if count == 0 {
                        break;
                    }
                    if response.is_empty() {
                        events.send(Event::Progress(Phase::ResponseStarted))?;
                    }
                    response.extend_from_slice(&buffer[..count]);
                }
                String::from_utf8(response).context("response is not UTF-8")
            },
            Watchdog::Elapsed,
        )
    }

    fn run(
        self,
        server: impl FnOnce(TcpStream) -> anyhow::Result<()> + Send,
        client: impl FnOnce(TcpStream, &Sender<Event>) -> anyhow::Result<String> + Send,
        watchdog: Watchdog,
    ) -> anyhow::Result<String> {
        let close = CloseSockets(vec![self.client.try_clone()?, self.server.try_clone()?]);
        let server_close = CloseSockets(vec![self.server.try_clone()?]);
        let client_close = CloseSockets(vec![self.client.try_clone()?]);
        let (events, received) = mpsc::channel();
        let server_events = events.clone();
        thread::scope(|scope| {
            // Declare the guard inside the scope so unwinding closes sockets
            // BEFORE scope's implicit joins, not after a blocked reader's join.
            let close = close;
            let server = scope.spawn(move || {
                // The supervisor holds cancellation clones. Explicit shutdown
                // still delivers EOF when this worker finishes normally.
                let _close = server_close;
                worker("server", &server_events, || {
                    server_events.send(Event::Progress(Phase::ServerEntered))?;
                    server(self.server)
                })
            });
            let client = scope.spawn(move || {
                let _close = client_close;
                worker("client", &events, || client(self.client, &events))
            });
            // Cancellation stops observation, not reporting: keep the receiver
            // alive through both joins so late workers cannot fail at send().
            let (completion, trace) = supervise(&received, watchdog);
            if completion.is_err() {
                drop(close);
            }
            // Always collect both outcomes. An early read/write error must not
            // detach a server panic and hide the initiating failure.
            let server_result = server.join().expect("worker catches panics");
            let client_result = client.join().expect("worker catches panics");
            if let Err(reason) = completion {
                bail!("{reason}; {trace}; server={server_result:?}; client={client_result:?}");
            }
            server_result?;
            client_result
        })
    }
}

fn worker<T>(
    name: &'static str,
    events: &Sender<Event>,
    action: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let result = catch_unwind(AssertUnwindSafe(action)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic");
        Err(anyhow::anyhow!("{name} panicked: {message}"))
    });
    let _ = events.send(Event::Finished(
        name,
        result.as_ref().err().map(|error| format!("{error:#}")),
    ));
    result
}

fn supervise(events: &Receiver<Event>, watchdog: Watchdog) -> (anyhow::Result<()>, String) {
    let started = Instant::now();
    let mut completed = 0;
    let mut trace = Vec::new();
    let result = (|| {
        while completed < 2 {
            let remaining = WATCHDOG.saturating_sub(started.elapsed());
            let event = events
                .recv_timeout(remaining)
                .context("HTTP exchange watchdog (not a throughput limit)")?;
            trace.push(format!("{:?}: {event:?}", started.elapsed()));
            if matches!((&watchdog, &event), (Watchdog::AtPhase(expected), Event::Progress(actual)) if expected == actual)
            {
                bail!("HTTP exchange watchdog injected at {event:?}");
            }
            if let Event::Finished(side, error) = event {
                if let Some(error) = error {
                    bail!("{side} failed: {error}");
                }
                completed += 1;
            }
        }
        Ok(())
    })();
    (result, trace.join("; "))
}

#[test]
fn response_waits_for_explicit_request_progress_not_a_sleep() -> anyhow::Result<()> {
    let exchange = HttpExchange::connect()?;
    let (sent, received) = mpsc::channel();
    let result = exchange.run(
        move |mut server| {
            received.recv_timeout(WATCHDOG)?;
            let mut request = String::new();
            server.read_to_string(&mut request)?;
            assert_eq!(request, "GET / HTTP/1.1\r\n\r\n");
            server.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")?;
            Ok(())
        },
        |mut client, events| {
            client.write_all(b"GET / HTTP/1.1\r\n\r\n")?;
            client.shutdown(Shutdown::Write)?;
            events.send(Event::Progress(Phase::ClientReading))?;
            sent.send(())?;
            let mut response = String::new();
            client.read_to_string(&mut response)?;
            Ok(response)
        },
        Watchdog::Elapsed,
    )?;
    assert!(result.ends_with("\r\n\r\nok"));
    Ok(())
}

#[test]
fn client_failure_cancels_and_joins_server() -> anyhow::Result<()> {
    let finished = AtomicUsize::new(0);
    let error = HttpExchange::connect()?
        .run(
            |mut server| {
                let mut buffer = Vec::new();
                let result = server.read_to_end(&mut buffer);
                finished.fetch_add(1, Ordering::SeqCst);
                result?;
                Ok(())
            },
            |_, _| anyhow::bail!("injected client failure"),
            Watchdog::Elapsed,
        )
        .unwrap_err();
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert!(
        error.to_string().contains("injected client failure"),
        "{error:#}"
    );
    Ok(())
}

#[test]
fn server_panic_cancels_and_joins_client() -> anyhow::Result<()> {
    let finished = AtomicUsize::new(0);
    let error = HttpExchange::connect()?
        .run(
            |_| panic!("injected server panic"),
            |mut client, _| {
                let mut response = String::new();
                let result = client.read_to_string(&mut response);
                finished.fetch_add(1, Ordering::SeqCst);
                result?;
                Ok(response)
            },
            Watchdog::Elapsed,
        )
        .unwrap_err();
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert!(
        error.to_string().contains("injected server panic"),
        "{error:#}"
    );
    Ok(())
}

#[test]
fn cancellation_preserves_reporting_until_late_workers_finish() -> anyhow::Result<()> {
    let (events, received) = mpsc::channel();
    events.send(Event::Progress(Phase::ClientReading))?;
    let (completion, _) = supervise(&received, Watchdog::AtPhase(Phase::ClientReading));
    assert!(completion
        .unwrap_err()
        .to_string()
        .contains("watchdog injected"));

    // Force the ordering from the preflight failure: the server starts only
    // AFTER the supervisor has decided to cancel and stopped observing events.
    let entered = AtomicUsize::new(0);
    let result = thread::scope(|scope| {
        scope
            .spawn(|| {
                worker("server", &events, || {
                    events.send(Event::Progress(Phase::ServerEntered))?;
                    entered.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            })
            .join()
            .unwrap()
    });
    assert_eq!(entered.load(Ordering::SeqCst), 1, "{result:?}");
    result
}

#[test]
fn watchdog_cancels_blocked_io_on_both_sides_without_waiting_for_real_time() -> anyhow::Result<()> {
    let finished = AtomicUsize::new(0);
    let error = HttpExchange::connect()?
        .run(
            |mut server| {
                let mut buffer = Vec::new();
                let result = server.read_to_end(&mut buffer);
                finished.fetch_add(1, Ordering::SeqCst);
                result?;
                Ok(())
            },
            |mut client, events| {
                events.send(Event::Progress(Phase::ClientReading))?;
                let mut response = String::new();
                let result = client.read_to_string(&mut response);
                finished.fetch_add(1, Ordering::SeqCst);
                result?;
                Ok(response)
            },
            Watchdog::AtPhase(Phase::ClientReading),
        )
        .unwrap_err();
    assert_eq!(finished.load(Ordering::SeqCst), 2);
    assert!(error.to_string().contains("watchdog injected"), "{error:#}");
    assert!(error.to_string().contains("ClientReading"));
    Ok(())
}
