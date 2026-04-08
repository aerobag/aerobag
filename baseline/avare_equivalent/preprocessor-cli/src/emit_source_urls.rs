use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use preprocessor_fetch::CacheLayout;
use serde_json::Value;
use sha2::{Digest, Sha256};

const VFR_URL: &str = "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/";
const IFR_URL: &str = "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/";
const DAFD_URL: &str = "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/";
const DTPP_URL: &str = "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/";

#[derive(Clone, Copy)]
enum CycleFormat {
    Charts,
    Iso,
}

#[derive(Clone, Copy)]
enum DownloadCycleKind {
    Legacy56,
    Current28,
}

#[derive(Debug, Clone)]
pub struct SourceUrlEmitResult {
    pub label: String,
    pub path: PathBuf,
}

pub fn emit_source_urls(output_dir: &Path) -> anyhow::Result<Vec<SourceUrlEmitResult>> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let records_by_label = build_records()?;
    let mut results = Vec::new();
    for (label, records) in records_by_label {
        let path = write_source_urls(output_dir, &label, &records)?;
        results.push(SourceUrlEmitResult { label, path });
    }
    Ok(results)
}

pub fn compare_source_url_emission(
    repo_root: &Path,
    avare_source_root: &Path,
    work_dir: &Path,
) -> anyhow::Result<()> {
    let python_dir = work_dir.join("python");
    let rust_dir = work_dir.join("rust");
    if python_dir.exists() {
        fs::remove_dir_all(&python_dir)
            .with_context(|| format!("failed to remove {}", python_dir.display()))?;
    }
    if rust_dir.exists() {
        fs::remove_dir_all(&rust_dir)
            .with_context(|| format!("failed to remove {}", rust_dir.display()))?;
    }
    fs::create_dir_all(&python_dir)
        .with_context(|| format!("failed to create {}", python_dir.display()))?;
    fs::create_dir_all(&rust_dir)
        .with_context(|| format!("failed to create {}", rust_dir.display()))?;

    let status = Command::new("python3")
        .arg(repo_root.join("legacy-capture/emit_source_urls.py"))
        .args(["--avare-source-root", &avare_source_root.display().to_string()])
        .args(["--output-dir", &python_dir.display().to_string()])
        .status()
        .with_context(|| {
            format!(
                "failed to launch {}",
                repo_root.join("legacy-capture/emit_source_urls.py").display()
            )
        })?;
    if !status.success() {
        bail!("legacy-capture/emit_source_urls.py failed");
    }

    emit_source_urls(&rust_dir)?;
    compare_source_url_output_dirs(&python_dir, &rust_dir)
}

pub fn compare_source_url_output_dirs(left_dir: &Path, right_dir: &Path) -> anyhow::Result<()> {
    let left_labels = read_label_names(left_dir)?;
    let right_labels = read_label_names(right_dir)?;
    let all_labels = left_labels
        .union(&right_labels)
        .cloned()
        .collect::<Vec<_>>();

    let mut mismatch = false;
    for label in all_labels {
        let left_path = left_dir.join(&label).join("source_urls.jsonl");
        let right_path = right_dir.join(&label).join("source_urls.jsonl");
        if !left_path.is_file() || !right_path.is_file() {
            mismatch = true;
            let status = if left_path.is_file() { "missing_right" } else { "missing_left" };
            println!("label {label} status={status}");
            continue;
        }
        let left_bytes = fs::read(&left_path)
            .with_context(|| format!("failed to read {}", left_path.display()))?;
        let right_bytes = fs::read(&right_path)
            .with_context(|| format!("failed to read {}", right_path.display()))?;
        let status = if left_bytes == right_bytes { "match" } else { "mismatch" };
        println!("label {label} status={status}");
        if left_bytes != right_bytes {
            mismatch = true;
        }
    }

    if mismatch {
        bail!("source url emission mismatch");
    }
    Ok(())
}

fn read_label_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut labels = BTreeSet::new();
    if !root.is_dir() {
        return Ok(labels);
    }
    let mut entries = fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate directory {}", root.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if entry
            .file_type()
            .with_context(|| format!("failed to stat {}", entry.path().display()))?
            .is_dir()
        {
            labels.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(labels)
}

fn build_records() -> anyhow::Result<Vec<(String, Vec<BTreeMap<String, Value>>)>> {
    let now = Utc::now();
    let charts_start = version_start(
        cycle_download(now, DownloadCycleKind::Legacy56),
        CycleFormat::Charts,
    )?;
    let iso_start = version_start(
        cycle_download(now, DownloadCycleKind::Legacy56),
        CycleFormat::Iso,
    )?;
    let current_start = version_start(
        cycle_download(now, DownloadCycleKind::Current28),
        CycleFormat::Iso,
    )?;
    let current_compact = current_start.replace('-', "");

    Ok(vec![
        (
            "charts-sec".to_string(),
            vec![
                list_crawl_record(
                    "charts-sec",
                    VFR_URL,
                    format!("^http.*{charts_start}/sectional-files/.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/sectional-files/"))
                            && href.ends_with(".zip")
                    },
                )?,
                list_crawl_record(
                    "charts-sec",
                    VFR_URL,
                    format!("^http.*{charts_start}/Caribbean/.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/Caribbean/"))
                            && href.ends_with(".zip")
                    },
                )?,
            ],
        ),
        (
            "charts-tac".to_string(),
            vec![list_crawl_record(
                "charts-tac",
                VFR_URL,
                format!("^http.*{charts_start}.*_TAC.zip$"),
                &|href| href.starts_with("http") && href.contains(&charts_start) && href.ends_with("_TAC.zip"),
            )?],
        ),
        (
            "charts-enr-l".to_string(),
            vec![
                list_crawl_record(
                    "charts-enr-l",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_l.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_l"))
                            && href.ends_with(".zip")
                    },
                )?,
                list_crawl_record(
                    "charts-enr-l",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_akl.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_akl"))
                            && href.ends_with(".zip")
                    },
                )?,
                list_crawl_record(
                    "charts-enr-l",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_p.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_p"))
                            && href.ends_with(".zip")
                    },
                )?,
            ],
        ),
        (
            "charts-enr-h".to_string(),
            vec![
                list_crawl_record(
                    "charts-enr-h",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_h.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_h"))
                            && href.ends_with(".zip")
                    },
                )?,
                list_crawl_record(
                    "charts-enr-h",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_akh.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_akh"))
                            && href.ends_with(".zip")
                    },
                )?,
                list_crawl_record(
                    "charts-enr-h",
                    IFR_URL,
                    format!("^http.*{charts_start}/enr_p.*.zip$"),
                    &|href| {
                        href.starts_with("http")
                            && href.contains(&format!("{charts_start}/enr_p"))
                            && href.ends_with(".zip")
                    },
                )?,
            ],
        ),
        (
            "csup".to_string(),
            vec![list_crawl_record(
                "csup",
                DAFD_URL,
                format!("^http.*DCS_{}.zip$", iso_start.replace('-', "")),
                &|href| href.starts_with("http") && href.ends_with(&format!("DCS_{}.zip", iso_start.replace('-', ""))),
            )?],
        ),
        (
            "tpp-ne".to_string(),
            vec![list_crawl_record(
                "tpp-ne",
                DTPP_URL,
                format!("^http.*DDTPP[A-E]+_{}.zip$", current_compact[2..].to_string()),
                &|href| {
                    href.starts_with("http")
                        && href.ends_with(".zip")
                        && href.contains("DDTPP")
                        && href.contains(&format!("_{}", &current_compact[2..]))
                },
            )?],
        ),
        (
            "tpp-nw".to_string(),
            vec![list_crawl_record(
                "tpp-nw",
                DTPP_URL,
                format!("^http.*DDTPP[A-E]+_{}.zip$", current_compact[2..].to_string()),
                &|href| {
                    href.starts_with("http")
                        && href.ends_with(".zip")
                        && href.contains("DDTPP")
                        && href.contains(&format!("_{}", &current_compact[2..]))
                },
            )?],
        ),
        (
            "data".to_string(),
            vec![
                source_url_record(
                    "data",
                    format!(
                        "https://nfdc.faa.gov/webContent/28DaySub/28DaySubscription_Effective_{current_start}.zip"
                    ),
                ),
                source_url_record(
                    "data",
                    format!("https://nfdc.faa.gov/webContent/28DaySub/{current_start}/aixm5.0.zip"),
                ),
                source_url_record("data", "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP".to_string()),
                source_url_record(
                    "data",
                    format!("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_{}.zip", &current_compact[2..]),
                ),
            ],
        ),
    ])
}

fn list_crawl_record(
    label: &str,
    url: &str,
    pattern: String,
    predicate: &dyn Fn(&str) -> bool,
) -> anyhow::Result<BTreeMap<String, Value>> {
    let hrefs = extract_href_links(&fetch_url_bytes(url)?)?;
    let mut results = hrefs
        .into_iter()
        .filter(|href| predicate(href))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(Value::String)
        .collect::<Vec<_>>();
    results.sort_by(|a, b| a.as_str().cmp(&b.as_str()));

    let mut record = BTreeMap::new();
    record.insert("event".to_string(), Value::String("list_crawl".to_string()));
    record.insert("label".to_string(), Value::String(label.to_string()));
    record.insert("match".to_string(), Value::String(pattern));
    record.insert("results".to_string(), Value::Array(results));
    record.insert("url".to_string(), Value::String(url.to_string()));
    Ok(record)
}

fn source_url_record(label: &str, url: String) -> BTreeMap<String, Value> {
    let mut record = BTreeMap::new();
    record.insert("event".to_string(), Value::String("source_url".to_string()));
    record.insert("label".to_string(), Value::String(label.to_string()));
    record.insert("url".to_string(), Value::String(url));
    record
}

fn write_source_urls(
    output_dir: &Path,
    label: &str,
    records: &[BTreeMap<String, Value>],
) -> anyhow::Result<PathBuf> {
    let target_dir = output_dir.join(label);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;
    let path = target_dir.join("source_urls.jsonl");
    let mut output = String::new();
    for record in records {
        output.push_str(&render_object(record)?);
        output.push('\n');
    }
    fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn render_object(record: &BTreeMap<String, Value>) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for (key, value) in record {
        parts.push(format!(
            "{}: {}",
            serde_json::to_string(key).context("failed to encode json key")?,
            render_value(value)?
        ));
    }
    Ok(format!("{{{}}}", parts.join(", ")))
}

fn render_value(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::Array(items) => {
            let mut rendered = Vec::new();
            for item in items {
                rendered.push(render_value(item)?);
            }
            Ok(format!("[{}]", rendered.join(", ")))
        }
        _ => serde_json::to_string(value).context("failed to encode json value"),
    }
}

fn fetch_url_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    if let Some(cache_root) = env::var_os("FETCH_CACHE_ROOT") {
        let layout = CacheLayout::new(cache_root);
        if let Some(bytes) = load_cached_bytes(&layout, url)? {
            return Ok(bytes);
        }
        if env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string()) == "offline" {
            bail!("cache miss in offline mode for crawl {url}");
        }
        let output = Command::new("curl")
            .arg("-L")
            .arg("--fail")
            .arg("--silent")
            .arg("--show-error")
            .arg(url)
            .output()
            .with_context(|| format!("failed to fetch {url}"))?;
        if !output.status.success() {
            bail!("curl failed for {url}");
        }
        store_cached_bytes(&layout, url, &output.stdout)?;
        return Ok(output.stdout);
    }
    let output = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg(url)
        .output()
        .with_context(|| format!("failed to fetch {url}"))?;
    if !output.status.success() {
        bail!("curl failed for {url}");
    }
    Ok(output.stdout)
}

fn load_cached_bytes(layout: &CacheLayout, url: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let metadata_path = layout.http_metadata_path(url);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let metadata_bytes = fs::read(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    let metadata: Value =
        serde_json::from_slice(&metadata_bytes).context("failed to parse cache metadata")?;
    let Some(sha256) = metadata.get("sha256").and_then(Value::as_str) else {
        return Ok(None);
    };
    let blob_path = layout.blob_path(sha256);
    if !blob_path.is_file() {
        return Ok(None);
    }
    Ok(Some(
        fs::read(&blob_path)
            .with_context(|| format!("failed to read {}", blob_path.display()))?,
    ))
}

fn store_cached_bytes(layout: &CacheLayout, url: &str, bytes: &[u8]) -> anyhow::Result<()> {
    fs::create_dir_all(layout.blobs_dir())
        .with_context(|| format!("failed to create {}", layout.blobs_dir().display()))?;
    fs::create_dir_all(layout.http_dir())
        .with_context(|| format!("failed to create {}", layout.http_dir().display()))?;
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let blob_path = layout.blob_path(&sha256);
    if !blob_path.is_file() {
        fs::write(&blob_path, bytes)
            .with_context(|| format!("failed to write {}", blob_path.display()))?;
    }
    let metadata = serde_json::json!({
        "sha256": sha256,
        "size": bytes.len(),
        "url": url,
    });
    fs::write(
        layout.http_metadata_path(url),
        serde_json::to_vec(&metadata).context("failed to encode cache metadata")?,
    )
    .with_context(|| format!("failed to write cache metadata for {url}"))?;
    Ok(())
}

fn extract_href_links(html: &[u8]) -> anyhow::Result<Vec<String>> {
    let text = String::from_utf8(html.to_vec()).context("crawl page was not utf-8")?;
    let bytes = text.as_bytes();
    let mut links = Vec::new();
    let mut index = 0;
    while let Some(offset) = text[index..].find("href") {
        let mut pos = index + offset + 4;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] != b'=' {
            index = pos.min(bytes.len());
            continue;
        }
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start;
        let end;
        if pos < bytes.len() && (bytes[pos] == b'"' || bytes[pos] == b'\'') {
            let quote = bytes[pos];
            pos += 1;
            start = pos;
            while pos < bytes.len() && bytes[pos] != quote {
                pos += 1;
            }
            end = pos;
        } else {
            start = pos;
            while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'>' {
                pos += 1;
            }
            end = pos;
        }
        if end <= bytes.len() {
            links.push(text[start..end].to_string());
        }
        index = pos.saturating_add(1);
    }
    Ok(links)
}

fn cycle_download(now: DateTime<Utc>, kind: DownloadCycleKind) -> i32 {
    let (te, fs) = calculate_cycle(now, 1);
    match kind {
        DownloadCycleKind::Legacy56 => fs,
        DownloadCycleKind::Current28 => te,
    }
}

fn calculate_cycle(now: DateTime<Utc>, future: i64) -> (i32, i32) {
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

fn version_start(cycle_name: i32, format: CycleFormat) -> anyhow::Result<String> {
    let cycle_upper = cycle_name / 100;
    let cycle_lower = cycle_name - (cycle_upper * 100);
    let year = 2000 + cycle_upper;
    let first_date = first_cycle_day(year).ok_or_else(|| anyhow::anyhow!("unsupported cycle year {year}"))?;
    let mut epoch = Utc.with_ymd_and_hms(year, 1, first_date, 9, 0, 0).unwrap();
    epoch += Duration::days(28 * i64::from(cycle_lower - 1));
    let formatted = match format {
        CycleFormat::Charts => epoch.format("%m-%d-%Y").to_string(),
        CycleFormat::Iso => epoch.format("%Y-%m-%d").to_string(),
    };
    Ok(formatted)
}

fn first_cycle_day(year: i32) -> Option<u32> {
    match year {
        2020 => Some(2),
        2021 => Some(28),
        2022 => Some(27),
        2023 => Some(26),
        2024 => Some(25),
        2025 => Some(23),
        2026 => Some(22),
        2027 => Some(21),
        2028 => Some(20),
        2029 => Some(18),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::extract_href_links;

    #[test]
    fn extracts_single_and_double_quoted_hrefs() {
        let html = br#"<a href="https://example.com/a.zip">a</a><a href='https://example.com/b.zip'>b</a>"#;
        let links = extract_href_links(html).expect("href parse should succeed");
        assert_eq!(
            links,
            vec![
                "https://example.com/a.zip".to_string(),
                "https://example.com/b.zip".to_string()
            ]
        );
    }
}
