use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Datelike, Duration, TimeZone, Utc};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli crate should live under the workspace root")
        .parent()
        .expect("workspace root should live under baseline/")
        .parent()
        .expect("baseline should live under the repo root")
        .to_path_buf()
}

fn run_cli_with_env(args: &[&str], envs: &[(&str, &str)]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_preprocessor-cli"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("failed to run preprocessor-cli");
    if !output.status.success() {
        panic!(
            "preprocessor-cli failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).expect("stdout should be utf-8")
}

#[test]
fn rust_source_url_emitter_matches_python_offline() {
    let repo_root = repo_root();
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let cache_root = temp_dir.path().join("fetch-cache");
    seed_fetch_cache(&cache_root).expect("fetch cache should be seeded");

    let work_dir = temp_dir.path().join("compare");
    let stdout = run_cli_with_env(
        &[
            "compare-source-url-emission",
            "--repo-root",
            &repo_root.display().to_string(),
            "--avare-source-root",
            &repo_root.join("avare-source").display().to_string(),
            "--work-dir",
            &work_dir.display().to_string(),
        ],
        &[
            ("FETCH_CACHE_ROOT", &cache_root.display().to_string()),
            ("FETCH_CACHE_MODE", "offline"),
        ],
    );

    for label in [
        "charts-sec",
        "charts-tac",
        "charts-enr-l",
        "charts-enr-h",
        "csup",
        "tpp-ne",
        "tpp-nw",
        "data",
    ] {
        assert!(
            stdout.contains(&format!("label {label} status=match")),
            "missing parity line for {label}\n{stdout}"
        );
    }
}

fn seed_fetch_cache(cache_root: &Path) -> anyhow::Result<()> {
    let charts_start = version_start(cycle_download_legacy56(), true);
    let current_start = version_start(cycle_download_current28(), false);
    let current_compact = current_start.replace('-', "");

    seed_cache_entry(
        cache_root,
        "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
        &format!(
            r#"
            <html><body>
              <a href="https://example.test/{charts_start}/sectional-files/alpha.zip">sec</a>
              <a href="https://example.test/{charts_start}/sectional-files/bravo.zip">sec</a>
              <a href="https://example.test/{charts_start}/Caribbean/carib.zip">carib</a>
              <a href="https://example.test/{charts_start}/metro_TAC.zip">tac</a>
              <a href="https://example.test/ignore/not-a-match.txt">ignore</a>
            </body></html>
            "#
        ),
    )?;
    seed_cache_entry(
        cache_root,
        "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
        &format!(
            r#"
            <html><body>
              <a href="https://example.test/{charts_start}/enr_l01.zip">enr-l</a>
              <a href="https://example.test/{charts_start}/enr_akl02.zip">enr-akl</a>
              <a href="https://example.test/{charts_start}/enr_p03.zip">enr-p</a>
              <a href="https://example.test/{charts_start}/enr_h04.zip">enr-h</a>
              <a href="https://example.test/{charts_start}/enr_akh05.zip">enr-akh</a>
            </body></html>
            "#
        ),
    )?;
    seed_cache_entry(
        cache_root,
        "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/",
        &format!(
            r#"<html><body><a href="https://example.test/DCS_{}.zip">csup</a></body></html>"#,
            current_start.replace('-', "")
        ),
    )?;
    seed_cache_entry(
        cache_root,
        "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/",
        &format!(
            r#"
            <html><body>
              <a href="https://example.test/DDTPPA_{}.zip">a</a>
              <a href="https://example.test/DDTPPE_{}.zip">e</a>
            </body></html>
            "#,
            &current_compact[2..],
            &current_compact[2..]
        ),
    )?;
    Ok(())
}

fn seed_cache_entry(cache_root: &Path, url: &str, body: &str) -> anyhow::Result<()> {
    let blobs_dir = cache_root.join("blobs");
    let http_dir = cache_root.join("http");
    fs::create_dir_all(&blobs_dir)?;
    fs::create_dir_all(&http_dir)?;
    let bytes = body.as_bytes();
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    fs::write(blobs_dir.join(&sha256), bytes)?;
    fs::write(
        http_dir.join(format!("{}.json", hash_text(url))),
        serde_json::to_vec(&serde_json::json!({
            "sha256": sha256,
            "size": bytes.len(),
            "url": url,
        }))?,
    )?;
    Ok(())
}

fn hash_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn cycle_download_legacy56() -> i32 {
    let (_, fs) = calculate_cycle(Utc::now(), 1);
    fs
}

fn cycle_download_current28() -> i32 {
    let (te, _) = calculate_cycle(Utc::now(), 1);
    te
}

fn calculate_cycle(now: chrono::DateTime<Utc>, future: i64) -> (i32, i32) {
    let mut start_utc = Utc.with_ymd_and_hms(2020, 1, 2, 9, 0, 0).unwrap();
    let mut cycle = 1_i32;
    let mut last_year = 2019_i32;
    let mut combined = 2001_i32;
    let mut is56 = true;
    let now_utc = now + Duration::days(28 * future);

    while start_utc < now_utc {
        if last_year != start_utc.year() {
            cycle = 1;
            last_year = start_utc.year();
        } else {
            cycle += 1;
        }
        combined = (start_utc.year() % 2000) * 100 + cycle;
        is56 = !is56;
        start_utc += Duration::days(28);
    }

    if is56 {
        (combined, combined)
    } else {
        let (previous, _) = calculate_cycle(now, future - 1);
        (combined, previous)
    }
}

fn version_start(cycle_name: i32, charts_format: bool) -> String {
    let cycle_upper = cycle_name / 100;
    let cycle_lower = cycle_name - (cycle_upper * 100);
    let year = 2000 + cycle_upper;
    let first_date = match year {
        2020 => 2,
        2021 => 28,
        2022 => 27,
        2023 => 26,
        2024 => 25,
        2025 => 23,
        2026 => 22,
        2027 => 21,
        2028 => 20,
        2029 => 18,
        _ => panic!("unsupported test year {year}"),
    };
    let epoch = Utc.with_ymd_and_hms(year, 1, first_date, 9, 0, 0).unwrap()
        + Duration::days(28 * i64::from(cycle_lower - 1));
    if charts_format {
        epoch.format("%m-%d-%Y").to_string()
    } else {
        epoch.format("%Y-%m-%d").to_string()
    }
}
