use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context};
use chrono::{Datelike, Duration, NaiveDate};
use preprocessor_fetch::{CacheLayout, FetchCacheConfig, FetchCacheMode};
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

pub fn emit_source_urls(
    output_dir: &Path,
    target_cycle: Option<&str>,
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let records_by_label = build_records(target_cycle, fetch_cache)?;
    let mut results = Vec::new();
    for (label, records) in records_by_label {
        let path = write_source_urls(output_dir, &label, &records)?;
        results.push(path);
    }
    Ok(results)
}

pub fn discover_published_cycles(
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<Vec<String>> {
    let published_chart_effective_dates = discover_published_chart_effective_dates(fetch_cache)?;
    let mut cycles = discover_published_tpp_cycles(fetch_cache)?
        .into_iter()
        .filter(|cycle| {
            chart_effective_date_for_cycle(cycle, &published_chart_effective_dates).is_ok()
        })
        .collect::<Vec<_>>();
    cycles.sort();
    cycles.dedup();
    Ok(cycles)
}

pub fn cycle_effective_date(cycle_code: &str) -> anyhow::Result<NaiveDate> {
    let cycle_name: i32 = cycle_code
        .parse()
        .with_context(|| format!("failed to parse cycle code {cycle_code}"))?;
    let cycle_upper = cycle_name / 100;
    let cycle_lower = cycle_name - (cycle_upper * 100);
    let year = 2000 + cycle_upper;
    let first_date =
        first_cycle_day(year).ok_or_else(|| anyhow::anyhow!("unsupported cycle year {year}"))?;
    let first = NaiveDate::from_ymd_opt(year, 1, first_date)
        .ok_or_else(|| anyhow::anyhow!("invalid first cycle day for {year}"))?;
    Ok(first + Duration::days(28 * i64::from(cycle_lower - 1)))
}

fn build_records(
    target_cycle: Option<&str>,
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<Vec<(String, Vec<BTreeMap<String, Value>>)>> {
    let cycle = match target_cycle {
        Some(cycle) => cycle.to_string(),
        None => discover_published_cycles(fetch_cache)?
            .into_iter()
            .last()
            .context("no published FAA cycles discovered")?,
    };
    let published_chart_effective_dates = discover_published_chart_effective_dates(fetch_cache)?;
    let charts_effective =
        chart_effective_date_for_cycle(&cycle, &published_chart_effective_dates)?;
    let charts_start = format_effective_date(charts_effective, CycleFormat::Charts);
    let iso_start = format_effective_date(charts_effective, CycleFormat::Iso);
    let current_start = format_effective_date(cycle_effective_date(&cycle)?, CycleFormat::Iso);
    let current_compact = current_start.replace('-', "");

    let mut records = vec![
        (
            "charts-sec".to_string(),
            vec![
                list_crawl_record(
                    "charts-sec",
                    VFR_URL,
                    format!("^http.*{charts_start}/sectional-files/.*.zip$"),
                    fetch_cache,
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
                    fetch_cache,
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
                fetch_cache,
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
                    fetch_cache,
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
                    fetch_cache,
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
                    fetch_cache,
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
                    fetch_cache,
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
                    fetch_cache,
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
                    fetch_cache,
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
                fetch_cache,
                &|href| href.starts_with("http") && href.ends_with(&format!("DCS_{}.zip", iso_start.replace('-', ""))),
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
                source_url_record(
                    "data",
                    format!("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_{}.zip", &current_compact[2..]),
                ),
            ],
        ),
    ];

    for label in [
        "tpp-ak", "tpp-pac", "tpp-nw", "tpp-sw", "tpp-nc", "tpp-ec", "tpp-sc", "tpp-ne", "tpp-se",
    ] {
        records.push((
            label.to_string(),
            vec![list_crawl_record(
                label,
                DTPP_URL,
                format!("^http.*DDTPP[A-E]+_{}.zip$", &current_compact[2..]),
                fetch_cache,
                &|href| {
                    href.starts_with("http")
                        && href.ends_with(".zip")
                        && href.contains("DDTPP")
                        && href.contains(&format!("_{}", &current_compact[2..]))
                },
            )?],
        ));
    }

    Ok(records)
}

fn list_crawl_record(
    label: &str,
    url: &str,
    pattern: String,
    fetch_cache: Option<&FetchCacheConfig>,
    predicate: &dyn Fn(&str) -> bool,
) -> anyhow::Result<BTreeMap<String, Value>> {
    let hrefs = extract_href_links(&fetch_url_bytes(url, fetch_cache)?)?;
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

fn fetch_url_bytes(url: &str, fetch_cache: Option<&FetchCacheConfig>) -> anyhow::Result<Vec<u8>> {
    if let Some(fetch_cache) = fetch_cache {
        let layout = CacheLayout::new(&fetch_cache.root);
        match fetch_cache.mode {
            FetchCacheMode::Offline => {
                if let Some(bytes) = load_cached_bytes(&layout, url)? {
                    return Ok(bytes);
                }
                bail!("cache miss in offline mode for crawl {url}");
            }
            FetchCacheMode::CacheFirst => {
                if let Some(bytes) = load_cached_bytes(&layout, url)? {
                    return Ok(bytes);
                }
            }
            FetchCacheMode::Fill => {}
        }
        let output = run_curl_fetch(url).with_context(|| format!("failed to fetch {url}"))?;
        if !output.status.success() {
            bail!("curl failed for {url}");
        }
        store_cached_bytes(&layout, url, &output.stdout)?;
        return Ok(output.stdout);
    }
    let output = run_curl_fetch(url).with_context(|| format!("failed to fetch {url}"))?;
    if !output.status.success() {
        bail!("curl failed for {url}");
    }
    Ok(output.stdout)
}

fn run_curl_fetch(url: &str) -> anyhow::Result<std::process::Output> {
    let mut command = Command::new("curl");
    command
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error");
    command.arg(url).output().context("curl execution failed")
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
    Ok(Some(fs::read(&blob_path).with_context(|| {
        format!("failed to read {}", blob_path.display())
    })?))
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

fn discover_published_tpp_cycles(
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<BTreeSet<String>> {
    let hrefs = extract_href_links(&fetch_url_bytes(DTPP_URL, fetch_cache)?)?;
    let mut cycles = BTreeSet::new();
    for href in hrefs {
        if !href.starts_with("http") || !href.ends_with(".zip") || !href.contains("DDTPP") {
            continue;
        }
        let Some(compact) = extract_suffix_date_token(&href, "DDTPP", ".zip") else {
            continue;
        };
        let effective = parse_compact_yy_mm_dd(&compact)?;
        cycles.insert(cycle_code_from_effective_date(effective)?);
    }
    Ok(cycles)
}

fn discover_published_chart_effective_dates(
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<BTreeSet<NaiveDate>> {
    let mut dates = BTreeSet::new();
    for url in [VFR_URL, IFR_URL] {
        let hrefs = extract_href_links(&fetch_url_bytes(url, fetch_cache)?)?;
        for href in hrefs {
            for segment in href.split('/') {
                if let Ok(date) = NaiveDate::parse_from_str(segment, "%m-%d-%Y") {
                    dates.insert(date);
                }
            }
        }
    }
    let hrefs = extract_href_links(&fetch_url_bytes(DAFD_URL, fetch_cache)?)?;
    for href in hrefs {
        if let Some(compact) = extract_prefix_date_token(&href, "DCS_", ".zip") {
            if let Ok(date) = NaiveDate::parse_from_str(&compact, "%Y%m%d") {
                dates.insert(date);
            }
        }
    }
    Ok(dates)
}

fn chart_effective_date_for_cycle(
    cycle: &str,
    published_chart_effective_dates: &BTreeSet<NaiveDate>,
) -> anyhow::Result<NaiveDate> {
    let cycle_effective = cycle_effective_date(cycle)?;
    if published_chart_effective_dates.contains(&cycle_effective) {
        return Ok(cycle_effective);
    }
    let previous = cycle_effective - Duration::days(28);
    if published_chart_effective_dates.contains(&previous) {
        return Ok(previous);
    }
    bail!("no published chart/csup window found for cycle {cycle}")
}

fn format_effective_date(effective: NaiveDate, format: CycleFormat) -> String {
    match format {
        CycleFormat::Charts => effective.format("%m-%d-%Y").to_string(),
        CycleFormat::Iso => effective.format("%Y-%m-%d").to_string(),
    }
}

fn extract_suffix_date_token(href: &str, marker: &str, suffix: &str) -> Option<String> {
    let marker_index = href.find(marker)?;
    let suffix_index = href[marker_index..].find(suffix)? + marker_index;
    let between = &href[marker_index..suffix_index];
    let underscore = between.rfind('_')?;
    let token = &between[underscore + 1..];
    if token.len() == 6 && token.chars().all(|ch| ch.is_ascii_digit()) {
        Some(token.to_string())
    } else {
        None
    }
}

fn extract_prefix_date_token(href: &str, prefix: &str, suffix: &str) -> Option<String> {
    let start = href.find(prefix)? + prefix.len();
    let end = href[start..].find(suffix)? + start;
    let token = &href[start..end];
    if token.len() == 8 && token.chars().all(|ch| ch.is_ascii_digit()) {
        Some(token.to_string())
    } else {
        None
    }
}

fn parse_compact_yy_mm_dd(value: &str) -> anyhow::Result<NaiveDate> {
    let year: i32 = format!("20{}", &value[0..2]).parse()?;
    let month: u32 = value[2..4].parse()?;
    let day: u32 = value[4..6].parse()?;
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("invalid compact date {value}"))
}

fn cycle_code_from_effective_date(effective: NaiveDate) -> anyhow::Result<String> {
    let year = effective.year();
    let first_date =
        first_cycle_day(year).ok_or_else(|| anyhow::anyhow!("unsupported cycle year {year}"))?;
    let first = NaiveDate::from_ymd_opt(year, 1, first_date)
        .ok_or_else(|| anyhow::anyhow!("invalid first cycle day for {year}"))?;
    let delta_days = effective.signed_duration_since(first).num_days();
    if delta_days < 0 || delta_days % 28 != 0 {
        bail!("effective date {effective} does not align to a 28-day FAA cycle");
    }
    let cycle = (delta_days / 28) + 1;
    Ok(format!("{:02}{:02}", year % 100, cycle))
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
    use super::*;

    #[test]
    fn parses_tpp_compact_suffix_date() {
        let href = "https://aeronav.faa.gov/upload_313-d/terminal/DDTPPA_260416.zip";
        assert_eq!(
            extract_suffix_date_token(href, "DDTPP", ".zip").as_deref(),
            Some("260416")
        );
        assert_eq!(
            parse_compact_yy_mm_dd("260416").unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 16).unwrap()
        );
        assert_eq!(
            cycle_code_from_effective_date(NaiveDate::from_ymd_opt(2026, 4, 16).unwrap()).unwrap(),
            "2604"
        );
    }

    #[test]
    fn resolves_chart_window_from_current_or_previous_cycle_start() {
        let published = BTreeSet::from([
            NaiveDate::from_ymd_opt(2026, 3, 19).unwrap(),
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap(),
        ]);
        assert_eq!(
            chart_effective_date_for_cycle("2604", &published).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 19).unwrap()
        );
        assert_eq!(
            chart_effective_date_for_cycle("2605", &published).unwrap(),
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap()
        );
    }
}
