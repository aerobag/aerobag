// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::CaptureManifest;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::{
    collections::VecDeque,
    fs,
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const NETWORK_FETCH_OUTER_ATTEMPTS: u32 = 3;
const NETWORK_FETCH_OUTER_RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkTimeouts {
    pub connect: Duration,
    pub total: Duration,
}

impl Default for NetworkTimeouts {
    fn default() -> Self {
        // Bulk cycle archives can be large. Live products use a tighter budget.
        Self {
            connect: Duration::from_secs(15),
            total: Duration::from_secs(1800),
        }
    }
}

impl NetworkTimeouts {
    fn apply(self, command: &mut Command) -> anyhow::Result<()> {
        if self.connect.is_zero() || self.total.is_zero() {
            bail!("network deadlines must be positive");
        }
        command
            .arg("--connect-timeout")
            .arg(self.connect.as_secs_f64().to_string())
            .arg("--max-time")
            .arg(self.total.as_secs_f64().to_string());
        Ok(())
    }
}

pub fn manifest_path_for_run(run_root: &str) -> String {
    format!("{run_root}/meta/manifest.json")
}

pub fn manifest_summary(manifest: &CaptureManifest) -> String {
    format!(
        "run {} with {} capture stages",
        manifest.run_id,
        manifest.captures.len()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheLayout {
    pub root: PathBuf,
}

impl CacheLayout {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs")
    }

    pub fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    pub fn http_dir(&self) -> PathBuf {
        self.root.join("http")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    pub fn blob_path(&self, sha256: &str) -> PathBuf {
        self.blobs_dir().join(sha256)
    }

    pub fn object_metadata_path(&self, logical_name: &str) -> PathBuf {
        self.objects_dir().join(format!("{logical_name}.json"))
    }

    pub fn http_metadata_path(&self, url: &str) -> PathBuf {
        self.http_dir().join(format!("{}.json", hash_text(url)))
    }

    pub fn run_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(run_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchCacheMode {
    CacheFirst,
    Fill,
    Offline,
}

impl FetchCacheMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "cache-first" => Ok(Self::CacheFirst),
            "fill" => Ok(Self::Fill),
            "offline" => Ok(Self::Offline),
            other => bail!("unsupported fetch cache mode: {other}"),
        }
    }

    fn is_offline(&self) -> bool {
        matches!(self, Self::Offline)
    }

    fn is_cache_first(&self) -> bool {
        matches!(self, Self::CacheFirst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchCacheConfig {
    pub root: PathBuf,
    pub mode: FetchCacheMode,
}

pub fn hash_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn hash_file(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let bytes = fs::read(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn read_source_urls_jsonl(path: impl AsRef<Path>) -> anyhow::Result<Vec<String>> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    let mut urls = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse source url jsonl line")?;
        if value.get("event").and_then(|value| value.as_str()) == Some("source_url") {
            if let Some(url) = value.get("url").and_then(|value| value.as_str()) {
                urls.push(url.to_string());
            }
        }
        if let Some(results) = value.get("results").and_then(|value| value.as_array()) {
            for result in results {
                if let Some(url) = result.as_str() {
                    urls.push(url.to_string());
                }
            }
        }
    }
    Ok(urls)
}

pub fn read_source_prefetch_requests_jsonl(
    path: impl AsRef<Path>,
) -> anyhow::Result<Vec<PrefetchRequest>> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    let mut requests = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse source url jsonl line")?;
        if value.get("event").and_then(|value| value.as_str()) == Some("source_url") {
            requests.push(source_prefetch_request_from_json(&value)?);
        }
        if let Some(results) = value.get("results").and_then(|value| value.as_array()) {
            for result in results {
                requests.push(source_prefetch_request_from_json(result)?);
            }
        }
    }
    Ok(requests)
}

fn source_prefetch_request_from_json(value: &serde_json::Value) -> anyhow::Result<PrefetchRequest> {
    let (url, logical_file_name, cache_key) = if let Some(url) = value.as_str() {
        (url, None, None)
    } else {
        let url = value
            .get("url")
            .and_then(|value| value.as_str())
            .context("source url record missing url")?;
        let logical_file_name = value
            .get("logical_file_name")
            .or_else(|| value.get("file_name"))
            .and_then(|value| value.as_str());
        let cache_key = value.get("cache_key").and_then(|value| value.as_str());
        (url, logical_file_name, cache_key)
    };
    let mut request = PrefetchRequest::new(url);
    if let Some(logical_file_name) = logical_file_name {
        request = request.with_logical_file_name(logical_file_name);
    }
    if let Some(cache_key) = cache_key {
        request = request.with_cache_key(cache_key);
    }
    Ok(request)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DownloadRecord {
    pub cache_key: String,
    pub url: String,
    pub file: String,
    pub sha256: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExtractRecord {
    pub archive: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageOutputRecord {
    pub label: String,
    pub chart: Option<String>,
    pub region: String,
    pub manifest: String,
    pub manifest_sha256: String,
    pub zip: String,
    pub zip_sha256: String,
    pub metadata: BTreeMap<String, serde_json::Value>,
}

pub fn read_source_url_set(path: impl AsRef<Path>) -> anyhow::Result<BTreeSet<String>> {
    Ok(read_source_urls_jsonl(path)?.into_iter().collect())
}

pub fn read_download_records(path: impl AsRef<Path>) -> anyhow::Result<BTreeSet<DownloadRecord>> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    read_download_records_from_text(&text, false)
        .with_context(|| format!("failed to parse {}", path.as_ref().display()))
}

pub fn read_download_records_lossy(
    path: impl AsRef<Path>,
) -> anyhow::Result<BTreeSet<DownloadRecord>> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    read_download_records_from_text(&text, true)
}

fn read_download_records_from_text(
    text: &str,
    skip_malformed: bool,
) -> anyhow::Result<BTreeSet<DownloadRecord>> {
    let mut rows = BTreeSet::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) if skip_malformed => {
                eprintln!(
                    "warning: skipping malformed downloads jsonl line {}: {error}",
                    line_index + 1
                );
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to parse downloads jsonl line {}", line_index + 1)
                });
            }
        };
        if value.get("event").and_then(|value| value.as_str()) != Some("download") {
            continue;
        }
        let Some(url) = value.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(file) = value.get("file").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(sha256) = value.get("sha256").and_then(|value| value.as_str()) else {
            continue;
        };
        let cache_key = value
            .get("cache_key")
            .and_then(|value| value.as_str())
            .unwrap_or(url);
        rows.insert(DownloadRecord {
            cache_key: cache_key.to_string(),
            url: url.to_string(),
            file: file.to_string(),
            sha256: sha256.to_string(),
            size: value.get("size").and_then(|value| value.as_u64()),
        });
    }
    Ok(rows)
}

pub fn read_extract_records(path: impl AsRef<Path>) -> anyhow::Result<BTreeSet<ExtractRecord>> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    let mut rows = BTreeSet::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse downloads jsonl line")?;
        if value.get("event").and_then(|value| value.as_str()) != Some("extract_zip") {
            continue;
        }
        let Some(archive) = value.get("archive").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(members) = value.get("members").and_then(|value| value.as_array()) else {
            continue;
        };
        let members = members
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        rows.insert(ExtractRecord {
            archive: archive.to_string(),
            members,
        });
    }
    Ok(rows)
}

pub fn prefetch_archives(
    requests: &[PrefetchRequest],
    dest_dir: impl AsRef<Path>,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<()> {
    prefetch_archives_inner(requests, dest_dir.as_ref(), fetch_jobs, fetch_cache, None)
}

#[derive(Clone, PartialEq, Eq)]
pub struct PrefetchRequest {
    pub url: String,
    pub cache_key: String,
    pub logical_file_name: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub force_http1: bool,
    pub allow_html: bool,
    pub timeouts: NetworkTimeouts,
}

impl std::fmt::Debug for PrefetchRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrefetchRequest")
            .field("url", &self.url)
            .field("cache_key", &self.cache_key)
            .field("logical_file_name", &self.logical_file_name)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("force_http1", &self.force_http1)
            .field("allow_html", &self.allow_html)
            .field("timeouts", &self.timeouts)
            .finish()
    }
}

impl PrefetchRequest {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            cache_key: url.clone(),
            url,
            logical_file_name: None,
            headers: BTreeMap::new(),
            force_http1: false,
            allow_html: false,
            timeouts: NetworkTimeouts::default(),
        }
    }

    pub fn with_logical_file_name(mut self, logical_file_name: impl Into<String>) -> Self {
        self.logical_file_name = Some(logical_file_name.into());
        self
    }

    pub fn with_timeouts(mut self, timeouts: NetworkTimeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    pub fn with_cache_key(mut self, cache_key: impl Into<String>) -> Self {
        self.cache_key = cache_key.into();
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn with_http1(mut self) -> Self {
        self.force_http1 = true;
        self
    }

    pub fn allow_html(mut self) -> Self {
        self.allow_html = true;
        self
    }
}

pub fn copy_source_urls_provenance(
    source_urls_path: impl AsRef<Path>,
    provenance_dir: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    let provenance_dir = provenance_dir.as_ref();
    fs::create_dir_all(provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    let destination = provenance_dir.join("source_urls.jsonl");
    fs::copy(source_urls_path.as_ref(), &destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source_urls_path.as_ref().display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

pub fn prefetch_archives_with_provenance(
    requests: &[PrefetchRequest],
    dest_dir: impl AsRef<Path>,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
    provenance_dir: impl AsRef<Path>,
    label: &str,
) -> anyhow::Result<()> {
    prefetch_requests_with_provenance(
        requests,
        dest_dir,
        fetch_jobs,
        fetch_cache,
        provenance_dir,
        label,
    )
}

pub fn prefetch_requests_with_provenance(
    requests: &[PrefetchRequest],
    dest_dir: impl AsRef<Path>,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
    provenance_dir: impl AsRef<Path>,
    label: &str,
) -> anyhow::Result<()> {
    fs::create_dir_all(provenance_dir.as_ref())
        .with_context(|| format!("failed to create {}", provenance_dir.as_ref().display()))?;
    let downloads_path = provenance_dir.as_ref().join("downloads.jsonl");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&downloads_path)
        .with_context(|| format!("failed to open {}", downloads_path.display()))?;
    let recorder = PrefetchProvenanceRecorder {
        label: label.to_string(),
        file: Arc::new(Mutex::new(file)),
    };
    prefetch_archives_inner(
        requests,
        dest_dir.as_ref(),
        fetch_jobs,
        fetch_cache,
        Some(recorder),
    )
}

pub fn write_package_outputs_jsonl(
    provenance_dir: impl AsRef<Path>,
    records: &[PackageOutputRecord],
) -> anyhow::Result<PathBuf> {
    let provenance_dir = provenance_dir.as_ref();
    fs::create_dir_all(provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    let path = provenance_dir.join("package_outputs.jsonl");
    let mut file =
        File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    for record in records {
        let mut value = serde_json::json!({
            "event": "package_output",
            "label": record.label,
            "manifest": record.manifest,
            "manifest_sha256": record.manifest_sha256,
            "region": record.region,
            "zip": record.zip,
            "zip_sha256": record.zip_sha256,
        });
        if let Some(chart) = &record.chart {
            value["chart"] = serde_json::Value::String(chart.clone());
        }
        if !record.metadata.is_empty() {
            value["metadata"] =
                serde_json::Value::Object(record.metadata.clone().into_iter().collect());
        }
        serde_json::to_writer(&mut file, &value)
            .context("failed to encode package output jsonl")?;
        file.write_all(b"\n")
            .context("failed to write package output newline")?;
    }
    Ok(path)
}

fn prefetch_archives_inner(
    requests: &[PrefetchRequest],
    dest_dir: &Path,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
    recorder: Option<PrefetchProvenanceRecorder>,
) -> anyhow::Result<()> {
    let dest_dir = dest_dir.to_path_buf();
    fs::create_dir_all(&dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;

    let queue = Arc::new(Mutex::new(VecDeque::from(requests.to_vec())));
    let fetch_cache = fetch_cache.cloned();
    let job_count = fetch_jobs.max(1);
    let mut handles = Vec::with_capacity(job_count);

    for _ in 0..job_count {
        let queue = Arc::clone(&queue);
        let dest_dir = dest_dir.clone();
        let fetch_cache = fetch_cache.clone();
        let recorder = recorder.clone();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let request = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("queue poisoned"))?;
                    guard.pop_front()
                };
                let Some(request) = request else {
                    break;
                };
                prefetch_one(&request, &dest_dir, fetch_cache.as_ref(), recorder.as_ref())?;
            }
            Ok(())
        }));
    }

    join_prefetch_workers(handles)
}

fn join_prefetch_workers(
    handles: Vec<thread::JoinHandle<anyhow::Result<()>>>,
) -> anyhow::Result<()> {
    // A failed request must not detach its siblings. Otherwise they outlive the
    // product tick, race scratch cleanup, and overlap the next scheduled retry.
    let mut first_error = None;
    for handle in handles {
        if let Err(error) = handle
            .join()
            .map_err(|_| anyhow::anyhow!("prefetch worker panicked"))
            .and_then(|result| result)
        {
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Clone)]
struct PrefetchProvenanceRecorder {
    label: String,
    file: Arc<Mutex<File>>,
}

impl PrefetchProvenanceRecorder {
    fn record_download(
        &self,
        cache_key: &str,
        url: &str,
        file_name: &str,
        sha256: &str,
        size: u64,
        source: &str,
    ) -> anyhow::Result<()> {
        let value = serde_json::json!({
            "downloaded": source == "network",
            "cache_key": cache_key,
            "event": "download",
            "file": file_name,
            "label": self.label,
            "sha256": sha256,
            "size": size,
            "source": source,
            "url": url,
        });
        self.write_line(&value)
    }

    fn record_extract(&self, archive: &str, members: &[String]) -> anyhow::Result<()> {
        let value = serde_json::json!({
            "archive": archive,
            "event": "extract_zip",
            "label": self.label,
            "members": members,
        });
        self.write_line(&value)
    }

    fn write_line(&self, value: &serde_json::Value) -> anyhow::Result<()> {
        let mut guard = self
            .file
            .lock()
            .map_err(|_| anyhow::anyhow!("provenance recorder poisoned"))?;
        serde_json::to_writer(&mut *guard, value).context("failed to encode provenance jsonl")?;
        guard
            .write_all(b"\n")
            .context("failed to write provenance newline")?;
        Ok(())
    }
}

fn prefetch_one(
    request: &PrefetchRequest,
    dest_dir: &Path,
    fetch_cache: Option<&FetchCacheConfig>,
    recorder: Option<&PrefetchProvenanceRecorder>,
) -> anyhow::Result<()> {
    let file_name = request.logical_file_name.as_deref().unwrap_or_else(|| {
        request
            .url
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .expect("network url should have a filename")
    });
    let archive_path = dest_dir.join(file_name);
    let mut source = "local";

    if archive_path.is_file() && !existing_download_is_usable(&archive_path) {
        fs::remove_file(&archive_path).with_context(|| {
            format!(
                "failed to remove corrupted partial download {}",
                archive_path.display()
            )
        })?;
    }

    if !archive_path.is_file() {
        if let Some(fetch_cache) = fetch_cache {
            let layout = CacheLayout::new(&fetch_cache.root);
            if fetch_cache.mode.is_offline() {
                if restore_cached_download(&layout, &request.cache_key, file_name, &archive_path)? {
                    source = "cache";
                } else {
                    bail!("cache miss in offline mode for {}", request.url);
                }
            } else if fetch_cache.mode.is_cache_first()
                && restore_cached_download(&layout, &request.cache_key, file_name, &archive_path)?
            {
                source = "cache";
            } else {
                source = fetch_network_with_cache(&NetworkFetchRequest {
                    layout: &layout,
                    cache_key: &request.cache_key,
                    network_url: &request.url,
                    headers: &request.headers,
                    force_http1: request.force_http1,
                    allow_html: request.allow_html,
                    timeouts: request.timeouts,
                    file_name,
                    dest_dir,
                    archive_path: &archive_path,
                })?;
            }
        } else {
            fetch_network(
                &request.url,
                &request.headers,
                request.force_http1,
                request.timeouts,
                file_name,
                dest_dir,
            )?;
            source = "network";
        }
    }

    validate_download_is_usable(&archive_path)?;
    let sha256 = hash_file(&archive_path)?;
    if let Some(fetch_cache) = fetch_cache {
        let layout = CacheLayout::new(&fetch_cache.root);
        relink_existing_cached_download(&layout, &sha256, file_name, &archive_path)?;
    }
    let size = fs::metadata(&archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?
        .len();
    if let Some(recorder) = recorder {
        recorder.record_download(
            &request.cache_key,
            &request.url,
            file_name,
            &sha256,
            size,
            source,
        )?;
    }

    if archive_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        let members = list_zip_members(&archive_path)?;
        if let Some(recorder) = recorder {
            recorder.record_extract(file_name, &members)?;
        }
        let status = Command::new("unzip")
            .arg("-q")
            .arg("-o")
            .arg(file_name)
            .current_dir(dest_dir)
            .status()
            .with_context(|| format!("failed to unzip {}", archive_path.display()))?;
        if !status.success() {
            bail!("unzip failed for {}", archive_path.display());
        }
    }

    Ok(())
}

struct NetworkFetchRequest<'a> {
    layout: &'a CacheLayout,
    cache_key: &'a str,
    network_url: &'a str,
    headers: &'a BTreeMap<String, String>,
    force_http1: bool,
    allow_html: bool,
    timeouts: NetworkTimeouts,
    file_name: &'a str,
    dest_dir: &'a Path,
    archive_path: &'a Path,
}

fn fetch_network_with_cache(request: &NetworkFetchRequest<'_>) -> anyhow::Result<&'static str> {
    let mut last_error = None;
    for attempt in 1..=NETWORK_FETCH_OUTER_ATTEMPTS {
        match fetch_network_with_cache_once(request) {
            Ok(source) => return Ok(source),
            Err(error) => {
                last_error = Some(error);
                if attempt < NETWORK_FETCH_OUTER_ATTEMPTS {
                    thread::sleep(NETWORK_FETCH_OUTER_RETRY_DELAY);
                }
            }
        }
    }
    Err(last_error.expect("network fetch should have run at least once"))
}

fn fetch_network_with_cache_once(
    request: &NetworkFetchRequest<'_>,
) -> anyhow::Result<&'static str> {
    let NetworkFetchRequest {
        layout,
        cache_key,
        network_url,
        headers,
        force_http1,
        allow_html,
        timeouts,
        file_name,
        dest_dir,
        archive_path,
    } = *request;
    let metadata = read_cache_metadata(layout, cache_key)?;
    let temp_path = temporary_download_path(archive_path);
    let headers_path = temp_path.with_extension("headers");
    let cookies_path = temp_path.with_extension("cookies");
    let mut result = curl_download_with_status(
        network_url,
        headers,
        force_http1,
        timeouts,
        dest_dir,
        &temp_path,
        &headers_path,
        &cookies_path,
        metadata.as_ref(),
    )?;
    if !allow_html && result.http_status == 200 && looks_like_html(&temp_path)? {
        // The first request can legitimately end at the FAA banner page while
        // setting cookies. Re-issue the original request once with the same jar.
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&headers_path);
        result = curl_download_with_status(
            network_url,
            headers,
            force_http1,
            timeouts,
            dest_dir,
            &temp_path,
            &headers_path,
            &cookies_path,
            None,
        )?;
    }
    if !allow_html && result.http_status == 200 && looks_like_html(&temp_path)? {
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&headers_path);
        let _ = fs::remove_file(&cookies_path);
        bail!("server returned HTML instead of data for {network_url}");
    }
    if !result.success || !(result.http_status == 304 || (200..300).contains(&result.http_status)) {
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&headers_path);
        let _ = fs::remove_file(&cookies_path);
        bail!(
            "curl failed for {network_url} with HTTP {}: {}",
            result.http_status,
            result.stderr
        );
    }
    if result.http_status == 304 {
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&headers_path);
        let _ = fs::remove_file(&cookies_path);
        if restore_cached_download(layout, cache_key, file_name, archive_path)? {
            return Ok("validated-cache");
        }
        bail!("HTTP 304 for {network_url}, but cached blob was unavailable");
    }
    fs::rename(&temp_path, archive_path).with_context(|| {
        format!(
            "failed to move downloaded file {} into place at {}",
            temp_path.display(),
            archive_path.display()
        )
    })?;
    validate_download_is_usable(archive_path)
        .with_context(|| format!("downloaded invalid data from {network_url}"))?;
    store_cached_download_with_headers(
        layout,
        cache_key,
        network_url,
        file_name,
        archive_path,
        parse_http_validators(&headers_path)?,
    )?;
    let _ = fs::remove_file(&headers_path);
    let _ = fs::remove_file(&cookies_path);
    Ok("network")
}

struct CurlDownloadResult {
    http_status: u16,
    success: bool,
    stderr: String,
}

fn curl_download_with_status(
    network_url: &str,
    request_headers: &BTreeMap<String, String>,
    force_http1: bool,
    timeouts: NetworkTimeouts,
    dest_dir: &Path,
    temp_path: &Path,
    headers_path: &Path,
    cookies_path: &Path,
    metadata: Option<&serde_json::Value>,
) -> anyhow::Result<CurlDownloadResult> {
    let mut command = Command::new("curl");
    timeouts.apply(&mut command)?;
    command
        .arg("-L")
        .arg("--silent")
        .arg("--show-error")
        .arg("--cookie-jar")
        .arg(cookies_path)
        .arg("--cookie")
        .arg(cookies_path)
        .arg("--dump-header")
        .arg(headers_path)
        .arg("--output")
        .arg(temp_path)
        .arg("--write-out")
        .arg("%{http_code}");
    if force_http1 {
        command.arg("--http1.1");
    }
    command.arg(network_url).current_dir(dest_dir);
    if let Some(etag) = metadata
        .and_then(|value| value.get("etag"))
        .and_then(|value| value.as_str())
    {
        command.arg("-H").arg(format!("If-None-Match: {etag}"));
    }
    if let Some(last_modified) = metadata
        .and_then(|value| value.get("last_modified"))
        .and_then(|value| value.as_str())
    {
        command
            .arg("-H")
            .arg(format!("If-Modified-Since: {last_modified}"));
    }
    let output = run_curl_with_headers(&mut command, request_headers)
        .with_context(|| format!("failed to fetch {network_url}"))?;
    let status_text = String::from_utf8_lossy(&output.stdout);
    let http_status = status_text.trim().parse::<u16>().with_context(|| {
        format!("curl returned non-numeric HTTP status for {network_url}: {status_text:?}")
    })?;
    Ok(CurlDownloadResult {
        http_status,
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn fetch_network(
    url: &str,
    request_headers: &BTreeMap<String, String>,
    force_http1: bool,
    timeouts: NetworkTimeouts,
    file_name: &str,
    dest_dir: &Path,
) -> anyhow::Result<()> {
    let mut last_error = None;
    for attempt in 1..=NETWORK_FETCH_OUTER_ATTEMPTS {
        match fetch_network_once(
            url,
            request_headers,
            force_http1,
            timeouts,
            file_name,
            dest_dir,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < NETWORK_FETCH_OUTER_ATTEMPTS {
                    thread::sleep(NETWORK_FETCH_OUTER_RETRY_DELAY);
                }
            }
        }
    }
    Err(last_error.expect("network fetch should have run at least once"))
}

fn fetch_network_once(
    url: &str,
    request_headers: &BTreeMap<String, String>,
    force_http1: bool,
    timeouts: NetworkTimeouts,
    file_name: &str,
    dest_dir: &Path,
) -> anyhow::Result<()> {
    let archive_path = dest_dir.join(file_name);
    let temp_path = temporary_download_path(&archive_path);
    let cookies_path = temp_path.with_extension("cookies");
    let mut command = Command::new("curl");
    timeouts.apply(&mut command)?;
    command
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--cookie-jar")
        .arg(&cookies_path)
        .arg("--cookie")
        .arg(&cookies_path)
        .arg("--output")
        .arg(&temp_path);
    if force_http1 {
        command.arg("--http1.1");
    }
    command.arg(url).current_dir(dest_dir);
    let output = run_curl_with_headers(&mut command, request_headers)
        .with_context(|| format!("failed to fetch {url}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&cookies_path);
        bail!(
            "curl failed for {url}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::rename(&temp_path, &archive_path).with_context(|| {
        format!(
            "failed to move downloaded file {} into place at {}",
            temp_path.display(),
            archive_path.display()
        )
    })?;
    validate_download_is_usable(&archive_path)
        .with_context(|| format!("downloaded invalid data from {url}"))?;
    let _ = fs::remove_file(&cookies_path);
    Ok(())
}

fn run_curl_with_headers(
    command: &mut Command,
    headers: &BTreeMap<String, String>,
) -> anyhow::Result<Output> {
    if headers.is_empty() {
        return command.output().context("failed to run curl");
    }
    let input = encode_request_headers(headers)?;
    // Header values can contain credentials. Pass them through stdin so they
    // never appear in process listings, command diagnostics, or temporary files.
    command.arg("--header").arg("@-");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run curl")?;
    let written = child
        .stdin
        .take()
        .expect("curl has piped stdin")
        .write_all(&input);
    let output = child
        .wait_with_output()
        .context("failed to wait for curl")?;
    written.context("failed to supply curl request headers")?;
    Ok(output)
}

fn encode_request_headers(headers: &BTreeMap<String, String>) -> anyhow::Result<Vec<u8>> {
    let mut input = Vec::new();
    for (name, value) in headers {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_graphic() && byte != b':')
            || value.contains(['\r', '\n', '\0'])
        {
            bail!("invalid HTTP request header");
        }
        writeln!(input, "{name}: {value}")?;
    }
    Ok(input)
}

fn looks_like_html(path: &Path) -> anyhow::Result<bool> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| byte == b'<'))
}

fn restore_cached_download(
    layout: &CacheLayout,
    url: &str,
    file_name: &str,
    archive_path: &Path,
) -> anyhow::Result<bool> {
    let metadata_path = layout.http_metadata_path(url);
    if !metadata_path.is_file() {
        return Ok(false);
    }
    let metadata_bytes = fs::read(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata_bytes).context("failed to parse cache metadata")?;
    let Some(sha256) = metadata.get("sha256").and_then(|value| value.as_str()) else {
        return Ok(false);
    };
    let blob_path = layout.blob_path(sha256);
    if !blob_path.is_file() {
        return Ok(false);
    }
    let temp_path = temporary_download_path(archive_path);
    restore_cached_blob(&blob_path, &temp_path, file_name).with_context(|| {
        format!(
            "failed to restore cached blob {} to {}",
            blob_path.display(),
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, archive_path).with_context(|| {
        format!(
            "failed to move cached blob {} into place at {}",
            temp_path.display(),
            archive_path.display()
        )
    })?;
    if validate_download_is_usable(archive_path).is_err() {
        let _ = fs::remove_file(archive_path);
        let _ = fs::remove_file(&metadata_path);
        let _ = fs::remove_file(&blob_path);
        return Ok(false);
    }
    if let Some(expected_name) = metadata.get("file").and_then(|value| value.as_str()) {
        if expected_name != file_name {
            bail!("cache filename mismatch for {url}: expected {expected_name}, got {file_name}");
        }
    }
    Ok(true)
}

fn store_cached_download_with_headers(
    layout: &CacheLayout,
    cache_key: &str,
    source_url: &str,
    file_name: &str,
    archive_path: &Path,
    validators: HttpValidators,
) -> anyhow::Result<()> {
    fs::create_dir_all(layout.blobs_dir())
        .with_context(|| format!("failed to create {}", layout.blobs_dir().display()))?;
    fs::create_dir_all(layout.http_dir())
        .with_context(|| format!("failed to create {}", layout.http_dir().display()))?;
    let sha256 = hash_file(archive_path)?;
    let blob_path = layout.blob_path(&sha256);
    if !blob_path.is_file() {
        store_cached_blob(archive_path, &blob_path, file_name).with_context(|| {
            format!(
                "failed to store {} at {}",
                archive_path.display(),
                blob_path.display()
            )
        })?;
    }
    let size = fs::metadata(archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?
        .len();
    let metadata = serde_json::json!({
        "cache_key": cache_key,
        "etag": validators.etag,
        "file": file_name,
        "fetched_at_utc": Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "last_modified": validators.last_modified,
        "sha256": sha256,
        "size": size,
        "url": source_url,
    });
    fs::write(
        layout.http_metadata_path(cache_key),
        serde_json::to_vec_pretty(&metadata).context("failed to encode cache metadata")?,
    )
    .with_context(|| format!("failed to write cache metadata for {cache_key}"))?;
    Ok(())
}

fn restore_cached_blob(source: &Path, destination: &Path, file_name: &str) -> anyhow::Result<()> {
    if is_hardlink_safe_cached_name(file_name) {
        link_or_copy_file(source, destination)
    } else {
        copy_file(source, destination)
    }
}

fn store_cached_blob(source: &Path, destination: &Path, file_name: &str) -> anyhow::Result<()> {
    if is_hardlink_safe_cached_name(file_name) {
        link_or_copy_file(source, destination)
    } else {
        copy_file(source, destination)
    }
}

fn link_or_copy_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    match fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(link_error) => {
            fs::copy(source, destination).with_context(|| {
                format!(
                    "failed to hardlink {} to {} ({link_error}); copy fallback also failed",
                    source.display(),
                    destination.display()
                )
            })?;
            Ok(())
        }
    }
}

fn relink_existing_cached_download(
    layout: &CacheLayout,
    sha256: &str,
    file_name: &str,
    archive_path: &Path,
) -> anyhow::Result<()> {
    if !is_hardlink_safe_cached_name(file_name) {
        return Ok(());
    }
    let blob_path = layout.blob_path(sha256);
    if !blob_path.is_file() {
        return Ok(());
    }
    let archive_metadata = fs::metadata(archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?;
    let blob_metadata = fs::metadata(&blob_path)
        .with_context(|| format!("failed to stat {}", blob_path.display()))?;
    if archive_metadata.dev() == blob_metadata.dev()
        && archive_metadata.ino() == blob_metadata.ino()
    {
        return Ok(());
    }
    let temp_path = temporary_download_path(archive_path);
    let _ = fs::remove_file(&temp_path);
    fs::hard_link(&blob_path, &temp_path).with_context(|| {
        format!(
            "failed to hardlink cached blob {} to {}",
            blob_path.display(),
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, archive_path).with_context(|| {
        format!(
            "failed to replace {} with hardlink {}",
            archive_path.display(),
            blob_path.display()
        )
    })?;
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn is_hardlink_safe_cached_name(file_name: &str) -> bool {
    let Some(extension) = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    extension.eq_ignore_ascii_case("zip")
        || extension.eq_ignore_ascii_case("tif")
        || extension.eq_ignore_ascii_case("tiff")
}

fn read_cache_metadata(
    layout: &CacheLayout,
    url: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let metadata_path = layout.http_metadata_path(url);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let metadata_bytes = fs::read(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    serde_json::from_slice(&metadata_bytes)
        .map(Some)
        .context("failed to parse cache metadata")
}

#[derive(Debug, Default, Clone)]
struct HttpValidators {
    etag: Option<String>,
    last_modified: Option<String>,
}

fn parse_http_validators(headers_path: &Path) -> anyhow::Result<HttpValidators> {
    if !headers_path.is_file() {
        return Ok(HttpValidators::default());
    }
    let text = fs::read_to_string(headers_path)
        .with_context(|| format!("failed to read {}", headers_path.display()))?;
    let mut validators = HttpValidators::default();
    for line in text.lines() {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("etag") {
                validators.etag = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("last-modified") {
                validators.last_modified = Some(value.to_string());
            }
        }
    }
    Ok(validators)
}

fn list_zip_members(path: &Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("unzip")
        .arg("-Z1")
        .arg(path)
        .output()
        .with_context(|| format!("failed to list zip members for {}", path.display()))?;
    if !output.status.success() {
        bail!("unzip -Z1 failed for {}", path.display());
    }
    let text = String::from_utf8(output.stdout).context("zip member output was not utf-8")?;
    let mut members = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    members.sort();
    Ok(members)
}

fn temporary_download_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    path.with_file_name(format!(
        ".{file_name}.part-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ))
}

fn existing_download_is_usable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if metadata.len() == 0 {
        return false;
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        return list_zip_members(path).is_ok();
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("gz"))
        .unwrap_or(false)
    {
        return gzip_test(path).is_ok();
    }
    true
}

fn validate_download_is_usable(path: &Path) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.len() == 0 {
        bail!("downloaded file is empty: {}", path.display());
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        let _ = list_zip_members(path)?;
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("gz"))
        .unwrap_or(false)
    {
        gzip_test(path)?;
    }
    Ok(())
}

fn gzip_test(path: &Path) -> anyhow::Result<()> {
    let output = Command::new("gzip")
        .arg("-t")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run gzip -t on {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("gzip validation failed for {}", path.display());
        }
        bail!("gzip validation failed for {}: {}", path.display(), stderr);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn failed_prefetch_waits_for_sibling_workers_before_returning() -> anyhow::Result<()> {
        use std::sync::mpsc;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let workers = vec![
            thread::spawn(|| anyhow::bail!("first fetch failed")),
            thread::spawn(move || {
                entered_tx.send(())?;
                release_rx.recv_timeout(Duration::from_secs(10))?;
                Ok(())
            }),
        ];
        let joiner = thread::spawn(move || {
            done_tx.send(join_prefetch_workers(workers)).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(5))?;
        // It is incorrect to return the first error while its sibling is held.
        let premature = done_rx.recv_timeout(Duration::from_millis(50));
        release_tx.send(())?;
        joiner.join().expect("joiner panicked");
        assert!(
            matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)),
            "a fetch worker was detached"
        );
        let error = done_rx.recv_timeout(Duration::from_secs(5))?.unwrap_err();
        assert_eq!(error.to_string(), "first fetch failed");
        Ok(())
    }

    #[test]
    fn stalled_http_has_a_deadline_with_and_without_cache_or_headers() -> anyhow::Result<()> {
        for cached in [false, true] {
            for authenticated in [false, true] {
                let temp = tempfile::tempdir()?;
                // The listening socket accepts TCP into its backlog but never
                // supplies an HTTP response. No sleeps or external server needed.
                let listener = TcpListener::bind("127.0.0.1:0")?;
                listener.set_nonblocking(true)?;
                let url = format!("http://{}/stalled.json", listener.local_addr()?);
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
                let started = std::time::Instant::now();
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
                let error = format!("{:#}", result.expect_err("stalled response succeeded"));
                assert!(error.contains("timed out"), "{error}");
                assert!(
                    started.elapsed() < Duration::from_secs(5),
                    "HTTP deadline was not enforced"
                );
                assert!(
                    listener.accept().is_ok(),
                    "curl never connected to the stalled endpoint"
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
                assert!(!error.contains("test-only-token"));
            }
        }
        Ok(())
    }

    #[test]
    fn network_deadlines_are_explicit_and_positive() -> anyhow::Result<()> {
        let mut command = Command::new("curl");
        NetworkTimeouts::default().apply(&mut command)?;
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--connect-timeout", "15", "--max-time", "1800"]
        );
        assert!(NetworkTimeouts {
            connect: Duration::ZERO,
            total: Duration::from_secs(1)
        }
        .apply(&mut command)
        .is_err());
        Ok(())
    }

    fn serve_inventory() -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/sites", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "no inventory request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("inventory server: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let body = r#"{"success":true,"count":0,"payload":[]}"#;
            write!(stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ).unwrap();
            request
        });
        (url, server)
    }

    #[test]
    fn authenticated_headers_reach_server_without_entering_command_arguments() {
        let (url, server) = serve_inventory();
        let headers = BTreeMap::from([
            (
                "Authorization".to_string(),
                "Bearer unit-test-token".to_string(),
            ),
            (
                "Referer".to_string(),
                "https://weathercams.faa.gov/".to_string(),
            ),
        ]);
        let mut command = Command::new("curl");
        command.args(["--silent", "--show-error", "--max-time", "5", &url]);
        let output = run_curl_with_headers(&mut command, &headers).unwrap();
        assert!(output.status.success());
        let request = server.join().unwrap();
        assert!(request.contains("Authorization: Bearer unit-test-token\r\n"));
        assert!(request.contains("Referer: https://weathercams.faa.gov/\r\n"));
        assert!(!format!("{command:?}").contains("unit-test-token"));
        assert!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["success"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn authenticated_inventory_uses_normal_cache_and_provenance_without_persisting_token() {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-authenticated-inventory-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let cache = FetchCacheConfig {
            root: root.join("cache"),
            mode: FetchCacheMode::Fill,
        };
        for (name, cache) in [("uncached", None), ("cached", Some(&cache))] {
            let (url, server) = serve_inventory();
            let request = PrefetchRequest::new(&url)
                .with_logical_file_name("sites.json")
                .with_header("Authorization", "Bearer unit-test-token");
            assert!(!format!("{request:?}").contains("unit-test-token"));
            let destination = root.join(name);
            let provenance = destination.join("provenance");
            prefetch_requests_with_provenance(
                &[request],
                &destination,
                1,
                cache,
                &provenance,
                "weather-camera-inventory",
            )
            .unwrap();
            assert!(server
                .join()
                .unwrap()
                .contains("Authorization: Bearer unit-test-token\r\n"));
            let data = fs::read(destination.join("sites.json")).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&data).unwrap()["success"],
                true
            );
            let provenance = fs::read_to_string(provenance.join("downloads.jsonl")).unwrap();
            assert!(provenance.contains(&url));
            assert!(!provenance.contains("unit-test-token"));
            if let Some(cache) = cache {
                let metadata =
                    fs::read_to_string(CacheLayout::new(&cache.root).http_metadata_path(&url))
                        .unwrap();
                assert!(!metadata.contains("unit-test-token"));
                let offline = FetchCacheConfig {
                    root: cache.root.clone(),
                    mode: FetchCacheMode::Offline,
                };
                // No token and no running server are needed to replay an already fetched input.
                prefetch_requests_with_provenance(
                    &[PrefetchRequest::new(url).with_logical_file_name("sites.json")],
                    root.join("offline"),
                    1,
                    Some(&offline),
                    root.join("offline-provenance"),
                    "test",
                )
                .unwrap();
                assert_eq!(fs::read(root.join("offline/sites.json")).unwrap(), data);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_headers_reject_line_injection_without_echoing_values() {
        for value in ["test-only-token\r\nX: injected", "test-only-token\0"] {
            let headers = BTreeMap::from([("Authorization".to_string(), value.to_string())]);
            let error = encode_request_headers(&headers).unwrap_err();
            assert!(!error.to_string().contains("test-only-token"));
        }
    }

    #[test]
    fn structured_request_uses_snapshot_name_for_cache_identity() {
        let request = PrefetchRequest::new("https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP")
            .with_logical_file_name("obstacle_2026.04.10.zip")
            .with_cache_key(
                "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
            );
        assert_eq!(
            request.url,
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP"
        );
        assert_eq!(
            request.logical_file_name.as_deref(),
            Some("obstacle_2026.04.10.zip")
        );
        assert_eq!(
            request.cache_key,
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
        );
    }

    #[test]
    fn prefetch_request_carries_transport_outside_url() {
        let request = PrefetchRequest::new("https://tfr.faa.gov/tfrapi/exportTfrList")
            .with_logical_file_name("list.json")
            .with_header("Referer", "https://tfr.faa.gov/")
            .with_http1();
        assert_eq!(request.url, "https://tfr.faa.gov/tfrapi/exportTfrList");
        assert_eq!(
            request.cache_key,
            "https://tfr.faa.gov/tfrapi/exportTfrList"
        );
        assert_eq!(request.logical_file_name.as_deref(), Some("list.json"));
        assert_eq!(
            request.headers.get("Referer").map(String::as_str),
            Some("https://tfr.faa.gov/")
        );
        assert!(request.force_http1);
    }

    #[test]
    fn different_obstacle_snapshots_produce_distinct_cache_metadata_paths() {
        let layout = CacheLayout::new("/tmp/fetch-cache-test");
        let a = layout.http_metadata_path(
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
        );
        let b = layout.http_metadata_path(
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.11.zip",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn source_jsonl_emits_structured_prefetch_requests() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-source-jsonl-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let path = root.join("source_urls.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"event\":\"source_url\",\"url\":\"https://example.invalid/source\",\"logical_file_name\":\"source.zip\",\"cache_key\":\"source-cache\"}\n",
                "{\"event\":\"list_crawl\",\"results\":[\"https://example.invalid/listed.zip\"]}\n"
            ),
        )?;

        let requests = read_source_prefetch_requests_jsonl(&path)?;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url, "https://example.invalid/source");
        assert_eq!(requests[0].logical_file_name.as_deref(), Some("source.zip"));
        assert_eq!(requests[0].cache_key, "source-cache");
        assert_eq!(
            requests[1],
            PrefetchRequest::new("https://example.invalid/listed.zip")
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn zip_cache_restore_prefers_hardlinks() {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-link-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let destination = root.join("destination.bin");
        fs::write(&source, b"payload").unwrap();

        restore_cached_blob(&source, &destination, "payload.zip").unwrap();

        let source_metadata = fs::metadata(&source).unwrap();
        let destination_metadata = fs::metadata(&destination).unwrap();
        assert_eq!(source_metadata.ino(), destination_metadata.ino());
        assert_eq!(source_metadata.nlink(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tif_cache_restore_prefers_hardlinks() {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-tif-link-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.tif");
        let destination = root.join("destination.tif");
        fs::write(&source, b"payload").unwrap();

        restore_cached_blob(&source, &destination, "payload.tif").unwrap();

        let source_metadata = fs::metadata(&source).unwrap();
        let destination_metadata = fs::metadata(&destination).unwrap();
        assert_eq!(source_metadata.ino(), destination_metadata.ino());
        assert_eq!(source_metadata.nlink(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gz_cache_restore_uses_private_copy() {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-copy-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let destination = root.join("destination.bin");
        fs::write(&source, b"payload").unwrap();

        restore_cached_blob(&source, &destination, "payload.xml.gz").unwrap();

        let source_metadata = fs::metadata(&source).unwrap();
        let destination_metadata = fs::metadata(&destination).unwrap();
        assert_ne!(source_metadata.ino(), destination_metadata.ino());
        assert_eq!(source_metadata.nlink(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gz_download_must_be_nonempty_and_valid() {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-gz-validation-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let empty = root.join("empty.xml.gz");
        let invalid = root.join("invalid.xml.gz");
        fs::write(&empty, b"").unwrap();
        fs::write(&invalid, b"not gzip").unwrap();

        assert!(!existing_download_is_usable(&empty));
        assert!(!existing_download_is_usable(&invalid));
        assert!(validate_download_is_usable(&empty).is_err());
        assert!(validate_download_is_usable(&invalid).is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_gz_cache_restore_is_evicted_as_cache_miss() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "preprocessor-fetch-invalid-gz-cache-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = CacheLayout::new(root.join("cache"));
        fs::create_dir_all(layout.blobs_dir())?;
        fs::create_dir_all(layout.http_dir())?;
        let url = "https://example.invalid/metars.cache.xml.gz";
        let sha256 = hash_text("");
        let blob_path = layout.blob_path(&sha256);
        let metadata_path = layout.http_metadata_path(url);
        fs::write(&blob_path, b"")?;
        fs::write(
            &metadata_path,
            serde_json::to_vec(&serde_json::json!({
                "file": "metars.cache.xml.gz",
                "sha256": sha256,
                "url": url,
            }))?,
        )?;
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir)?;
        let output_path = output_dir.join("metars.cache.xml.gz");

        assert!(!restore_cached_download(
            &layout,
            url,
            "metars.cache.xml.gz",
            &output_path,
        )?);
        assert!(!output_path.exists());
        assert!(!metadata_path.exists());
        assert!(!blob_path.exists());

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
