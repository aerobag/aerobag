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
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const NETWORK_FETCH_OUTER_ATTEMPTS: u32 = 3;
const NETWORK_FETCH_OUTER_RETRY_DELAY: Duration = Duration::from_secs(2);

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DownloadRecord {
    pub url: String,
    pub file: String,
    pub sha256: String,
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
    let mut rows = BTreeSet::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse downloads jsonl line")?;
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
        rows.insert(DownloadRecord {
            url: url.to_string(),
            file: file.to_string(),
            sha256: sha256.to_string(),
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
    urls: &[String],
    dest_dir: impl AsRef<Path>,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
) -> anyhow::Result<()> {
    let requests = urls
        .iter()
        .map(|url| PrefetchRequest::from_legacy_url(url))
        .collect::<anyhow::Result<Vec<_>>>()?;
    prefetch_archives_inner(&requests, dest_dir.as_ref(), fetch_jobs, fetch_cache, None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefetchRequest {
    pub url: String,
    pub cache_key: String,
    pub logical_file_name: Option<String>,
    pub force_http1: bool,
    pub allow_html: bool,
}

impl PrefetchRequest {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            cache_key: url.clone(),
            url,
            logical_file_name: None,
            force_http1: false,
            allow_html: false,
        }
    }

    pub fn with_logical_file_name(mut self, logical_file_name: impl Into<String>) -> Self {
        self.logical_file_name = Some(logical_file_name.into());
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

    fn from_legacy_url(url: &str) -> anyhow::Result<Self> {
        let parsed = parse_logical_download(url)?;
        Ok(Self {
            cache_key: url.to_string(),
            url: parsed.network_url,
            logical_file_name: parsed.logical_file_name,
            force_http1: false,
            allow_html: false,
        })
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
    urls: &[String],
    dest_dir: impl AsRef<Path>,
    fetch_jobs: usize,
    fetch_cache: Option<&FetchCacheConfig>,
    provenance_dir: impl AsRef<Path>,
    label: &str,
) -> anyhow::Result<()> {
    let requests = urls
        .iter()
        .map(|url| PrefetchRequest::from_legacy_url(url))
        .collect::<anyhow::Result<Vec<_>>>()?;
    prefetch_requests_with_provenance(
        &requests,
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

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("prefetch worker panicked"))??;
    }

    Ok(())
}

#[derive(Clone)]
struct PrefetchProvenanceRecorder {
    label: String,
    file: Arc<Mutex<File>>,
}

impl PrefetchProvenanceRecorder {
    fn record_download(
        &self,
        url: &str,
        file_name: &str,
        sha256: &str,
        size: u64,
        source: &str,
    ) -> anyhow::Result<()> {
        let value = serde_json::json!({
            "downloaded": source == "network",
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
                source = fetch_network_with_cache(
                    &layout,
                    &request.cache_key,
                    &request.url,
                    request.force_http1,
                    request.allow_html,
                    file_name,
                    dest_dir,
                    &archive_path,
                )?;
            }
        } else {
            fetch_network(&request.url, request.force_http1, file_name, dest_dir)?;
            source = "network";
        }
    }

    let sha256 = hash_file(&archive_path)?;
    if let Some(fetch_cache) = fetch_cache {
        let layout = CacheLayout::new(&fetch_cache.root);
        relink_existing_cached_download(&layout, &sha256, file_name, &archive_path)?;
    }
    let size = fs::metadata(&archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?
        .len();
    if let Some(recorder) = recorder {
        recorder.record_download(&request.url, file_name, &sha256, size, source)?;
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

fn fetch_network_with_cache(
    layout: &CacheLayout,
    cache_key: &str,
    network_url: &str,
    force_http1: bool,
    allow_html: bool,
    file_name: &str,
    dest_dir: &Path,
    archive_path: &Path,
) -> anyhow::Result<&'static str> {
    let mut last_error = None;
    for attempt in 1..=NETWORK_FETCH_OUTER_ATTEMPTS {
        match fetch_network_with_cache_once(
            layout,
            cache_key,
            network_url,
            force_http1,
            allow_html,
            file_name,
            dest_dir,
            archive_path,
        ) {
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
    layout: &CacheLayout,
    cache_key: &str,
    network_url: &str,
    force_http1: bool,
    allow_html: bool,
    file_name: &str,
    dest_dir: &Path,
    archive_path: &Path,
) -> anyhow::Result<&'static str> {
    let metadata = read_cache_metadata(layout, cache_key)?;
    let temp_path = temporary_download_path(archive_path);
    let headers_path = temp_path.with_extension("headers");
    let cookies_path = temp_path.with_extension("cookies");
    let mut result = curl_download_with_status(
        network_url,
        force_http1,
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
            force_http1,
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
    store_cached_download_with_headers(
        layout,
        cache_key,
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
    force_http1: bool,
    dest_dir: &Path,
    temp_path: &Path,
    headers_path: &Path,
    cookies_path: &Path,
    metadata: Option<&serde_json::Value>,
) -> anyhow::Result<CurlDownloadResult> {
    let mut command = Command::new("curl");
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
    let output = command
        .output()
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogicalDownload {
    network_url: String,
    logical_file_name: Option<String>,
}

fn parse_logical_download(url: &str) -> anyhow::Result<LogicalDownload> {
    let (network_url, fragment) = match url.split_once('#') {
        Some((base, fragment)) => (base.to_string(), Some(fragment)),
        None => (url.to_string(), None),
    };
    let mut logical_file_name = None;
    if let Some(fragment) = fragment {
        for part in fragment.split('&') {
            if let Some(value) = part.strip_prefix("logical_name=") {
                logical_file_name = Some(value.to_string());
            }
        }
    }
    let file_name_source = logical_file_name.as_deref().unwrap_or(&network_url);
    file_name_source
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("failed to derive filename from {url}"))?;
    Ok(LogicalDownload {
        network_url,
        logical_file_name,
    })
}

fn fetch_network(
    url: &str,
    force_http1: bool,
    file_name: &str,
    dest_dir: &Path,
) -> anyhow::Result<()> {
    let mut last_error = None;
    for attempt in 1..=NETWORK_FETCH_OUTER_ATTEMPTS {
        match fetch_network_once(url, force_http1, file_name, dest_dir) {
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
    force_http1: bool,
    file_name: &str,
    dest_dir: &Path,
) -> anyhow::Result<()> {
    let archive_path = dest_dir.join(file_name);
    let temp_path = temporary_download_path(&archive_path);
    let cookies_path = temp_path.with_extension("cookies");
    let mut command = Command::new("curl");
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
    let status = command
        .status()
        .with_context(|| format!("failed to fetch {url}"))?;
    if !status.success() {
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&cookies_path);
        bail!("curl failed for {url}");
    }
    fs::rename(&temp_path, &archive_path).with_context(|| {
        format!(
            "failed to move downloaded file {} into place at {}",
            temp_path.display(),
            archive_path.display()
        )
    })?;
    let _ = fs::remove_file(&cookies_path);
    Ok(())
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
    if let Some(expected_name) = metadata.get("file").and_then(|value| value.as_str()) {
        if expected_name != file_name {
            bail!("cache filename mismatch for {url}: expected {expected_name}, got {file_name}");
        }
    }
    Ok(true)
}

fn store_cached_download_with_headers(
    layout: &CacheLayout,
    url: &str,
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
        "etag": validators.etag,
        "file": file_name,
        "fetched_at_utc": Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "last_modified": validators.last_modified,
        "sha256": sha256,
        "size": size,
        "url": url,
    });
    fs::write(
        layout.http_metadata_path(url),
        serde_json::to_vec_pretty(&metadata).context("failed to encode cache metadata")?,
    )
    .with_context(|| format!("failed to write cache metadata for {url}"))?;
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
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        return list_zip_members(path).is_ok();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn logical_download_uses_snapshot_name_for_cache_identity() -> anyhow::Result<()> {
        let parsed = parse_logical_download(
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
        )
        .unwrap();
        assert_eq!(
            parsed.network_url,
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP"
        );
        assert_eq!(
            parsed.logical_file_name.as_deref(),
            Some("obstacle_2026.04.10.zip")
        );
        assert_eq!(
            PrefetchRequest::from_legacy_url(
                "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
            )?
            .cache_key,
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_2026.04.10.zip",
        );
        Ok(())
    }

    #[test]
    fn prefetch_request_carries_transport_outside_url() {
        let request = PrefetchRequest::new("https://tfr.faa.gov/tfrapi/exportTfrList")
            .with_logical_file_name("list.json")
            .with_http1();
        assert_eq!(request.url, "https://tfr.faa.gov/tfrapi/exportTfrList");
        assert_eq!(
            request.cache_key,
            "https://tfr.faa.gov/tfrapi/exportTfrList"
        );
        assert_eq!(request.logical_file_name.as_deref(), Some("list.json"));
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
}
