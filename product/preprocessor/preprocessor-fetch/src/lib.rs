use anyhow::{bail, Context};
use preprocessor_core::CaptureManifest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::{
    collections::VecDeque,
    env, fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
};

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
) -> anyhow::Result<()> {
    prefetch_archives_inner(urls, dest_dir.as_ref(), fetch_jobs, None)
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
    provenance_dir: impl AsRef<Path>,
    label: &str,
) -> anyhow::Result<()> {
    fs::create_dir_all(provenance_dir.as_ref())
        .with_context(|| format!("failed to create {}", provenance_dir.as_ref().display()))?;
    let downloads_path = provenance_dir.as_ref().join("downloads.jsonl");
    let file = File::create(&downloads_path)
        .with_context(|| format!("failed to create {}", downloads_path.display()))?;
    let recorder = PrefetchProvenanceRecorder {
        label: label.to_string(),
        file: Arc::new(Mutex::new(file)),
    };
    prefetch_archives_inner(urls, dest_dir.as_ref(), fetch_jobs, Some(recorder))
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
        serde_json::to_writer(&mut file, &value)
            .context("failed to encode package output jsonl")?;
        file.write_all(b"\n")
            .context("failed to write package output newline")?;
    }
    Ok(path)
}

fn prefetch_archives_inner(
    urls: &[String],
    dest_dir: &Path,
    fetch_jobs: usize,
    recorder: Option<PrefetchProvenanceRecorder>,
) -> anyhow::Result<()> {
    let dest_dir = dest_dir.to_path_buf();
    fs::create_dir_all(&dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;

    let queue = Arc::new(Mutex::new(VecDeque::from(urls.to_vec())));
    let job_count = fetch_jobs.max(1);
    let mut handles = Vec::with_capacity(job_count);

    for _ in 0..job_count {
        let queue = Arc::clone(&queue);
        let dest_dir = dest_dir.clone();
        let recorder = recorder.clone();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let url = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("queue poisoned"))?;
                    guard.pop_front()
                };
                let Some(url) = url else {
                    break;
                };
                prefetch_one(&url, &dest_dir, recorder.as_ref())?;
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
    url: &str,
    dest_dir: &Path,
    recorder: Option<&PrefetchProvenanceRecorder>,
) -> anyhow::Result<()> {
    let file_name = url
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("failed to derive filename from {url}"))?;
    let archive_path = dest_dir.join(file_name);
    let mut source = "local";

    if !archive_path.is_file() {
        if let Some(cache_root) = env::var_os("FETCH_CACHE_ROOT") {
            let layout = CacheLayout::new(cache_root);
            if restore_cached_download(&layout, url, file_name, &archive_path)? {
                source = "cache";
            } else {
                if env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string()) == "offline"
                {
                    bail!("cache miss in offline mode for {url}");
                }
                fetch_network(url, file_name, dest_dir)?;
                store_cached_download(&layout, url, file_name, &archive_path)?;
                source = "network";
            }
        } else {
            fetch_network(url, file_name, dest_dir)?;
            source = "network";
        }
    }

    let sha256 = hash_file(&archive_path)?;
    let size = fs::metadata(&archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?
        .len();
    if let Some(recorder) = recorder {
        recorder.record_download(url, file_name, &sha256, size, source)?;
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

fn fetch_network(url: &str, file_name: &str, dest_dir: &Path) -> anyhow::Result<()> {
    let status = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--output")
        .arg(file_name)
        .arg(url)
        .current_dir(dest_dir)
        .status()
        .with_context(|| format!("failed to fetch {url}"))?;
    if !status.success() {
        bail!("curl failed for {url}");
    }
    Ok(())
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
    fs::copy(&blob_path, archive_path).with_context(|| {
        format!(
            "failed to copy cached blob {} to {}",
            blob_path.display(),
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

fn store_cached_download(
    layout: &CacheLayout,
    url: &str,
    file_name: &str,
    archive_path: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(layout.blobs_dir())
        .with_context(|| format!("failed to create {}", layout.blobs_dir().display()))?;
    fs::create_dir_all(layout.http_dir())
        .with_context(|| format!("failed to create {}", layout.http_dir().display()))?;
    let sha256 = hash_file(archive_path)?;
    let blob_path = layout.blob_path(&sha256);
    if !blob_path.is_file() {
        fs::copy(archive_path, &blob_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                archive_path.display(),
                blob_path.display()
            )
        })?;
    }
    let size = fs::metadata(archive_path)
        .with_context(|| format!("failed to stat {}", archive_path.display()))?
        .len();
    let metadata = serde_json::json!({
        "file": file_name,
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
