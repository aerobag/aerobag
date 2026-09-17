// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(addr: SocketAddr, path: &str) -> anyhow::Result<(String, serde_json::Value)> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    Ok((headers.into(), serde_json::from_str(body)?))
}

#[test]
fn bad_publication_keeps_real_daemon_serving_other_products_and_reporting_failure(
) -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let live = temp.path().join("live");
    fs::create_dir_all(live.join("v3/versions/metars"))?;
    fs::write(
        live.join("v3/versions/metars/cached.json"),
        b"{\"cached\":true}",
    )?;
    let credentials = temp.path().join("nms.json");
    fs::write(
        &credentials,
        serde_json::to_vec(&serde_json::json!({
            "sourceEnvironment":"staging", "clientId":"test", "clientSecret":"test",
            "apiBaseUrl":"https://api-staging.cgifederal-aim.com/nmsapi/v1",
            "tokenUrl":"https://api-staging.cgifederal-aim.com/v1/auth/token"
        }))?,
    )?;
    let publication = temp.path().join("product_artifacts.json");
    fs::write(&publication, b"broken publication")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);
    let log_path = temp.path().join("daemon.log");
    let log = fs::File::create(&log_path)?;
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_aerobag-live-feedsd"))
            .args([
                "--listen",
                &addr.to_string(),
                "--fetch-cache-mode",
                "offline",
            ])
            .arg("--live-root")
            .arg(&live)
            .arg("--scratch-root")
            .arg(temp.path().join("scratch"))
            .arg("--fetch-cache-root")
            .arg(temp.path().join("cache"))
            .arg("--tfr-detail-backfill-state-root")
            .arg(temp.path().join("tfr-state"))
            .arg("--nms-notams-state-root")
            .arg(temp.path().join("nms-state"))
            .arg("--nms-notams-config")
            .arg(&credentials)
            .arg("--product-artifacts")
            .arg(&publication)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(exit) = daemon.0.try_wait()? {
            anyhow::bail!("daemon exited {exit}: {}", fs::read_to_string(&log_path)?);
        }
        if let Ok((headers, value)) = request(addr, "/live-feeds/status.json") {
            assert!(headers.starts_with("HTTP/1.1 200 OK"));
            break value;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "daemon unavailable: {}",
            fs::read_to_string(&log_path)?
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status["products"]["notams"]["last_error"]
        .as_str()
        .unwrap()
        .contains("failed to parse product artifacts"));
    assert!(
        status["products"]["notams"]["consecutive_failure_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    for product in ["metars", "tafs", "tfrs", "nexrad", "obstacles"] {
        assert!(
            status["products"].get(product).is_some(),
            "missing {product}: {status}"
        );
    }
    let (headers, cached) = request(addr, "/live-feeds/v3/versions/metars/cached.json")?;
    assert!(headers.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(cached["cached"], true);
    let (_, compatibility) = request(addr, "/live-feeds/compatibility.json")?;
    assert_eq!(compatibility["ready"], false);
    assert_eq!(compatibility["projection_ready"], false);
    assert!(compatibility["configured_products"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "notams"));
    assert!(daemon.0.try_wait()?.is_none());
    Ok(())
}
