use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use crossbeam_channel::{self, RecvTimeoutError};
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, package_family_region_versioned, stage_work_dir,
    FULL_COVERAGE_ZOOM,
};
use preprocessor_core::nav_kv::{build_nav_kv_sorted, NavKvPair};
use preprocessor_core::{ChartFamily, Region};
use preprocessor_csup::{
    package_csup_region_versioned, prepare_csup_inputs, render_csup_region,
    stage_work_dir_for_product,
};
use preprocessor_data::{
    build_data_package, build_data_package_with_tpp_matches, DataBuildMode, DataBuildRequest,
    DataTppMatchRequest,
};
use preprocessor_fast::{
    build_geo_dataset, build_metar_dataset, build_nexrad_dataset, build_tfr_dataset,
    metar_content_fingerprint, BuildGeoRequest, BuildMetarRequest, BuildNexradRequest,
    BuildTfrRequest,
};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    prefetch_requests_with_provenance, read_source_urls_jsonl, write_package_outputs_jsonl,
    CacheLayout, FetchCacheConfig, FetchCacheMode, PackageOutputRecord, PrefetchRequest,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource, ResourceIndex,
    TileLevelRecord,
};
use preprocessor_tpp::{package_native_tpp_versioned, render_native_tpp, NativeTppRunRequest};
use preprocessor_vectors::{
    build_obstacle_dataset, build_vectors_dataset, BuildObstacleDatasetRequest, BuildVectorsRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::{
    write::SimpleFileOptions, CompressionMethod, DateTime as ZipDateTime, ZipArchive, ZipWriter,
};

use crate::emit_source_urls::{cycle_effective_date, discover_published_cycles, emit_source_urls};

const PACKAGE_CYCLE_VERSION: &str = "01";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductBuildProfile {
    Validation,
    Production,
}

impl ProductBuildProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "production" => Some(Self::Production),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Production => "production",
        }
    }

    fn tpp_regions(self) -> &'static [Region] {
        match self {
            Self::Validation => &[Region::Ne, Region::Nw],
            Self::Production => &Region::ALL,
        }
    }

    fn terrain_regions(self) -> &'static [Region] {
        match self {
            Self::Validation => &[Region::Nw],
            Self::Production => &Region::ALL,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProductBuildConfig {
    pub chart_cutline_root: PathBuf,
    pub build_root: PathBuf,
    pub profile: ProductBuildProfile,
    pub target_cycle: Option<String>,
    pub fetch_jobs: usize,
    pub cpu_jobs: usize,
    pub max_heavy_jobs: usize,
    pub fetch_cache_root: PathBuf,
    pub fetch_cache_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeRecord {
    name: String,
    fingerprint: String,
    started_at_utc: String,
    finished_at_utc: String,
    elapsed_ms: u64,
    cache_hit: bool,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    #[serde(default)]
    output_details: BTreeMap<String, NodeOutputDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeOutputDetail {
    path: String,
    sha256: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildManifest {
    schema_version: u32,
    profile: String,
    cycle: String,
    build_root: String,
    generated_at_utc: String,
    fetch_cache_root: String,
    fetch_cache_mode: String,
    nodes: Vec<NodeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcRootsManifest {
    schema_version: u32,
    profile: String,
    build_root: String,
    updated_at_utc: String,
    node_roots: BTreeMap<String, GcNodeRoot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcNodeRoot {
    scope: String,
    task_id: String,
    node_name: String,
    fingerprint: String,
    node_dir: String,
    record_path: String,
    cache_hit: bool,
    finished_at_utc: String,
    updated_at_utc: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildCacheGcMode {
    DryRun,
    Execute,
}

#[derive(Debug, Clone)]
pub struct BuildCacheGcConfig {
    pub build_root: PathBuf,
    pub profile: ProductBuildProfile,
    pub mode: BuildCacheGcMode,
    pub grace_hours: u64,
    pub bootstrap_from_build_manifests: bool,
}

#[derive(Debug, Clone)]
pub struct BuildCacheGcReport {
    pub roots_path: PathBuf,
    pub rooted_nodes: usize,
    pub scanned_nodes: usize,
    pub active_nodes: usize,
    pub stale_lock_nodes: usize,
    pub grace_nodes: usize,
    pub evictable_nodes: usize,
    pub reclaimed_bytes: u64,
    pub scratch_files: usize,
    pub scratch_bytes: u64,
    pub scratch_active_nodes: usize,
    pub private_scratch_files: usize,
    pub private_scratch_bytes: u64,
    pub private_scratch_active_nodes: usize,
    pub by_node_name: BTreeMap<String, BuildCacheGcBucket>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildCacheGcBucket {
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleManifest {
    schema_version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle_type: String,
    cycle: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    cycle_version: String,
    generated_at_utc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    effective_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    expiration_date: String,
    start_valid: String,
    end_valid: String,
    packages: Vec<BundlePackageArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ancillary: Vec<BundleArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FastBundleManifest {
    schema_version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle_type: String,
    published_at_utc: String,
    packages: Vec<BundlePackageArtifact>,
}

struct PublishedBundleRefs<'a> {
    packages: &'a [BundlePackageArtifact],
    ancillary: &'a [BundleArtifact],
}

enum BundleManifestLike {
    Cycle(BundleManifest),
    Fast(FastBundleManifest),
}

impl BundleManifestLike {
    fn bundle_refs(&self) -> PublishedBundleRefs<'_> {
        match self {
            BundleManifestLike::Cycle(bundle) => PublishedBundleRefs {
                packages: &bundle.packages,
                ancillary: &bundle.ancillary,
            },
            BundleManifestLike::Fast(bundle) => PublishedBundleRefs {
                packages: &bundle.packages,
                ancillary: &[],
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentArtifactsManifest {
    schema_version: u32,
    as_of_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    as_of_utc: String,
    bundles: Vec<CurrentBundleEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diagnostics: Option<CurrentDiagnosticsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentDiagnosticsEntry {
    filename: String,
    error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildDiagnosticsManifest {
    schema_version: u32,
    generated_at_utc: String,
    error_count: usize,
    errors: Vec<BuildDiagnosticEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildDiagnosticEntry {
    product: String,
    cycle: Option<String>,
    severity: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentBundleEntry {
    filename: String,
    #[serde(default)]
    relative_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    cycle: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    cycle_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    start_valid: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    end_valid: String,
    checksum_sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleArtifact {
    filename: String,
    relative_path: String,
    checksum_sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundlePackageArtifact {
    id: String,
    family_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    region_id: Option<String>,
    filename: String,
    relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cycle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cycle_version: Option<String>,
    checksum_sha256: String,
    size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_generated_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_fetched_at_utc: Option<String>,
    effective_date: Option<String>,
    expiration_date: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
struct BuiltNavDbArtifacts {
    node_record: NodeRecord,
    package: BundlePackageArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChartCoveragePolygonRecord {
    id: String,
    points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChartCoveragePolygonSetRecord {
    schema_version: u32,
    id: String,
    polygons: Vec<ChartCoveragePolygonRecord>,
}

#[derive(Debug, Clone)]
struct RawChartCutlinePolygon {
    points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductBuildResult {
    pub cycle_manifest_paths: Vec<PathBuf>,
    pub current_artifacts_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct FastSubsetBuildResult {
    pub current_artifacts_path: PathBuf,
    pub fast_products: Vec<PublishedFastProductResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublishedFastProductResult {
    pub id: String,
    pub source_zip_path: PathBuf,
    pub published_zip: PathBuf,
    pub checksum_sha256: String,
    pub size_bytes: u64,
    pub source_generated_at_utc: String,
}

fn build_fast_bundle_manifest(
    fast_products: &[PublishedFastProductResult],
    published_at_utc: &str,
) -> BundleManifestLike {
    BundleManifestLike::Fast(FastBundleManifest {
        schema_version: 1,
        bundle_id: "fast_current".to_string(),
        bundle_type: "fast".to_string(),
        published_at_utc: published_at_utc.to_string(),
        packages: fast_products
            .iter()
            .map(|product| BundlePackageArtifact {
                id: product.id.clone(),
                family_id: product.id.clone(),
                region_id: None,
                filename: product
                    .published_zip
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
                relative_path: product
                    .published_zip
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
                cycle: None,
                cycle_version: None,
                checksum_sha256: product.checksum_sha256.clone(),
                size_bytes: product.size_bytes,
                published_at_utc: Some(published_at_utc.to_string()),
                source_generated_at_utc: Some(product.source_generated_at_utc.clone()),
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some(product.source_generated_at_utc.clone()),
                expiration_date: None,
                metadata: BTreeMap::new(),
            })
            .collect(),
    })
}

fn static_product_task_ids(config: &ProductBuildConfig) -> Vec<String> {
    let mut task_ids = vec!["publish-geo".to_string()];
    if include_static_terrain_products() {
        task_ids.extend(
            config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| format!("publish-terrain-{}", region.code().to_ascii_lowercase())),
        );
        task_ids.extend(config.profile.terrain_regions().iter().map(|region| {
            format!(
                "publish-shaded-relief-{}",
                region.code().to_ascii_lowercase()
            )
        }));
    }
    task_ids
}

fn stable_product_family_region(id: &str) -> anyhow::Result<(String, Option<String>)> {
    if id == "geo" {
        return Ok(("geo".to_string(), None));
    }
    if let Some(region_id) = id.strip_prefix("terrain-") {
        return Ok(("terrain".to_string(), Some(region_id.to_string())));
    }
    if let Some(region_id) = id.strip_prefix("shaded-relief-") {
        return Ok(("shaded-relief".to_string(), Some(region_id.to_string())));
    }
    bail!("unrecognized stable product id: {id}")
}

fn stable_effective_date_from_published_file(path: &Path) -> anyhow::Result<(String, String)> {
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let published_at: DateTime<Utc> = modified.into();
    Ok((
        published_at.format("%Y-%m-%d").to_string(),
        published_at.to_rfc3339_opts(SecondsFormat::Secs, true),
    ))
}

fn build_stable_bundle_package_artifact(
    id: &str,
    published_zip: &Path,
    sha256: &str,
    size_bytes: u64,
    source_version: &str,
    source_fetched_at_utc: Option<String>,
) -> anyhow::Result<BundlePackageArtifact> {
    let (family_id, region_id) = stable_product_family_region(id)?;
    let filename = filename_string(published_zip)?;
    let (effective_date, published_at_utc) =
        stable_effective_date_from_published_file(published_zip)?;
    Ok(BundlePackageArtifact {
        id: id.to_string(),
        family_id,
        region_id,
        filename: filename.clone(),
        relative_path: filename,
        cycle: None,
        cycle_version: None,
        checksum_sha256: sha256.to_string(),
        size_bytes,
        published_at_utc: Some(published_at_utc),
        source_generated_at_utc: None,
        source_version: Some(source_version.to_string()),
        source_fetched_at_utc,
        effective_date: Some(effective_date),
        expiration_date: None,
        metadata: BTreeMap::new(),
    })
}

fn publish_fast_bundle_manifest(
    build_root: &Path,
    fast_products: &[PublishedFastProductResult],
    published_at_utc: &str,
) -> anyhow::Result<PathBuf> {
    let bundle_manifest = match build_fast_bundle_manifest(fast_products, published_at_utc) {
        BundleManifestLike::Fast(bundle) => bundle,
        BundleManifestLike::Cycle(_) => unreachable!("fast bundle builder returned cycle bundle"),
    };
    let bundle_manifest_path = write_hashed_fast_bundle_manifest(build_root, &bundle_manifest)?;
    validate_fast_bundle_manifest(build_root, &bundle_manifest_path)?;
    Ok(bundle_manifest_path)
}

fn sync_unpacked_fast_bundle_manifest(
    config: &ProductBuildConfig,
    bundle_manifest_path: &Path,
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(bundle_manifest_path, &unpacked_root)
}

fn current_bundle_path(
    current: &CurrentArtifactsManifest,
    build_root: &Path,
    bundle_type: &str,
) -> Option<PathBuf> {
    current
        .bundles
        .iter()
        .find(|bundle| bundle.bundle_type == bundle_type)
        .map(|bundle| build_root.join(&bundle.filename))
}

fn load_fast_bundle_products(
    bundle_path: &Path,
) -> anyhow::Result<Vec<PublishedFastProductResult>> {
    let bundle: FastBundleManifest = serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
    bundle
        .packages
        .into_iter()
        .map(|package| {
            Ok(PublishedFastProductResult {
                id: package.id,
                source_zip_path: bundle_path
                    .parent()
                    .context("fast bundle path missing parent")?
                    .join(&package.filename),
                published_zip: bundle_path
                    .parent()
                    .context("fast bundle path missing parent")?
                    .join(&package.filename),
                checksum_sha256: package.checksum_sha256,
                size_bytes: package.size_bytes,
                source_generated_at_utc: package
                    .source_generated_at_utc
                    .or(package.effective_date)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

#[derive(Debug)]
struct PreparedNode {
    name: String,
    fingerprint: String,
    dir: PathBuf,
    record_path: PathBuf,
    lock_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct PackageSummary {
    total: usize,
    cache_hits: usize,
    rebuilt: usize,
}

#[derive(Debug)]
struct BuildLockGuard {
    path: PathBuf,
    node_dir: PathBuf,
}

impl Drop for BuildLockGuard {
    fn drop(&mut self) {
        let _ = set_tree_readonly(&self.node_dir, false);
        let _ = fs::remove_file(&self.path);
        if self.node_dir.join("build-record.json").is_file()
            && !self.node_dir.join(".mutable-output-root").exists()
        {
            let _ = set_tree_readonly(&self.node_dir, true);
        }
    }
}

fn node_output_file_detail(record: &NodeRecord, key: &str) -> (Option<String>, Option<u64>) {
    record
        .output_details
        .get(key)
        .map(|detail| (detail.sha256.clone(), detail.size_bytes))
        .unwrap_or((None, None))
}

fn output_sha_or_hash(record: &NodeRecord, key: &str, path: &Path) -> anyhow::Result<String> {
    Ok(node_output_file_detail(record, key)
        .0
        .unwrap_or(hash_file(path)?))
}

fn sqlite_output_path(record: &NodeRecord) -> anyhow::Result<&str> {
    record
        .outputs
        .get("intermediate_sqlite_db")
        .or_else(|| record.outputs.get("main_db"))
        .map(String::as_str)
        .with_context(|| format!("node {} missing sqlite output", record.name))
}

enum NodeCacheState {
    CacheHit(NodeRecord),
    Build(BuildLockGuard),
}

#[derive(Debug, Clone)]
enum ScheduledTaskKind {
    ChartRender { family: ChartFamily },
    CsupStage,
    CsupRender { region: Region },
    TppRender { region: Region },
    DataBase,
    DataMatch,
    ChartPackage { family: ChartFamily },
    CsupPackage,
    TppPackage { region: Region },
    Vectors,
    ResourceIndex,
    ChartUnpack { family: ChartFamily, region: Region },
    CsupUnpack { region: Region },
    TppUnpack { region: Region },
    DataUnpack,
    VectorsUnpack,
}

#[derive(Debug, Clone)]
struct ScheduledTask {
    id: String,
    deps: Vec<String>,
    weight: usize,
    kind: ScheduledTaskKind,
}

#[derive(Debug, Clone)]
enum TaskValue {
    None,
    CsupStage {
        record: NodeRecord,
        work_dir: PathBuf,
    },
    ChartSource(ChartSource),
    CsupSource(AssetSource),
    FingerprintedData {
        intermediate_sqlite_db: PathBuf,
        source_input_dir: PathBuf,
        zip: PathBuf,
        fingerprint: String,
    },
    FingerprintedZip {
        zip: PathBuf,
    },
    FingerprintedTppSource {
        source: AssetSource,
        fingerprint: String,
    },
}

#[derive(Debug, Clone)]
enum ProductTaskValue {
    None,
    SourceUrls {
        dir: PathBuf,
        chart_versions: BTreeMap<String, String>,
        csup_version: String,
        tpp_versions: BTreeMap<String, String>,
        data_version: String,
        bundle_cycle: String,
    },
    CsupStage {
        record: NodeRecord,
        work_dir: PathBuf,
    },
    ChartSource(ChartSource),
    CsupSource(AssetSource),
    FingerprintedData {
        intermediate_sqlite_db: PathBuf,
        source_input_dir: PathBuf,
        zip: PathBuf,
        fingerprint: String,
    },
    FingerprintedZip {
        zip: PathBuf,
        errors: Option<PathBuf>,
    },
    FingerprintedTppSource {
        source: AssetSource,
        fingerprint: String,
    },
    CycleManifest {
        path: PathBuf,
    },
    PublishedNavDb {
        package: BundlePackageArtifact,
    },
    BuiltStandaloneProduct {
        zip_path: PathBuf,
        zip_sha256: Option<String>,
        zip_size_bytes: Option<u64>,
        source_version: String,
        source_fetched_at_utc: Option<String>,
    },
    BuiltStaticTileProduct {
        zip_path: PathBuf,
        zip_sha256: Option<String>,
        zip_size_bytes: Option<u64>,
        source_version: String,
        source_fetched_at_utc: Option<String>,
        tile_levels: Vec<TileLevelRecord>,
    },
    BuiltWaterMask {
        mask_tiles_dir: PathBuf,
        source_version: String,
    },
    TerrainDiscovery {
        index_path: PathBuf,
        source_fetched_at_utc: Option<String>,
    },
    PublishedStandaloneProduct {
        id: String,
        source_zip_path: PathBuf,
        published_zip: PathBuf,
        sha256: String,
        size_bytes: u64,
        source_version: String,
        source_fetched_at_utc: Option<String>,
    },
    CurrentArtifacts {
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
struct ProductTaskCompletion {
    node_records: Vec<NodeRecord>,
    value: ProductTaskValue,
    completion_detail: String,
}

#[derive(Debug, Clone)]
struct PublishedZipArtifact {
    source_zip_path: PathBuf,
    published_zip_path: PathBuf,
    checksum_sha256: String,
}

#[derive(Debug, Clone)]
struct TaskCompletion {
    node_records: Vec<NodeRecord>,
    value: TaskValue,
    completion_detail: String,
}

struct TaskCompletionGuard {
    tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<TaskCompletion>)>,
    task_id: String,
    task_weight: usize,
    sent: bool,
}

impl TaskCompletionGuard {
    fn new(
        tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<TaskCompletion>)>,
        task_id: String,
        task_weight: usize,
    ) -> Self {
        Self {
            tx,
            task_id,
            task_weight,
            sent: false,
        }
    }

    fn send(mut self, result: anyhow::Result<TaskCompletion>) {
        let _ = self
            .tx
            .send((self.task_id.clone(), self.task_weight, result));
        self.sent = true;
    }
}

impl Drop for TaskCompletionGuard {
    fn drop(&mut self) {
        if self.sent {
            return;
        }

        let _ = self.tx.send((
            self.task_id.clone(),
            self.task_weight,
            Err(anyhow::anyhow!(
                "task worker exited without delivering completion"
            )),
        ));
    }
}

struct ProductCompletionGuard {
    tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<ProductTaskCompletion>)>,
    task_id: String,
    task_weight: usize,
    sent: bool,
}

impl ProductCompletionGuard {
    fn new(
        tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<ProductTaskCompletion>)>,
        task_id: String,
        task_weight: usize,
    ) -> Self {
        Self {
            tx,
            task_id,
            task_weight,
            sent: false,
        }
    }

    fn send(mut self, result: anyhow::Result<ProductTaskCompletion>) {
        let _ = self
            .tx
            .send((self.task_id.clone(), self.task_weight, result));
        self.sent = true;
    }
}

impl Drop for ProductCompletionGuard {
    fn drop(&mut self) {
        if self.sent {
            return;
        }

        let _ = self.tx.send((
            self.task_id.clone(),
            self.task_weight,
            Err(anyhow::anyhow!(
                "product task worker exited without delivering completion"
            )),
        ));
    }
}

const PRODUCT_BUILD_CGROUP_ACTIVE_ENV: &str = "PRODUCT_BUILD_CGROUP_ACTIVE";
const DEFAULT_PRODUCT_BUILD_MEMORY_MAX: &str = "80G";
const TPP_RENDER_JOBS_PER_RUN: usize = 8;
const TPP_RENDER_WEIGHT: usize = 2;
const TPP_CACHE_LAYOUT_VERSION: &str = "v2-cache-nodes";
const TERRAIN_PIPELINE_VERSION: &str = "v4";
const SHADED_RELIEF_PIPELINE_VERSION: &str = "v6-chart-index-path";
const WATER_MASK_PIPELINE_VERSION: &str = "v2";
const TERRAIN_TILE_WORKERS: u32 = 16;
const SHADED_RELIEF_TILE_WORKERS: u32 = 16;
const WATER_MASK_FETCH_WORKERS: u32 = 2;
const WATER_MASK_TILE_WORKERS: u32 = 4;
const WATER_MASK_PAGE_SIZE: usize = 10;
const WATER_MASK_MAX_SPLIT_SOURCE_PAGES: usize = 64;
const WATER_MASK_MAX_OMITTED_OBJECTS: usize = 16;
const WATER_MASK_NHD_SERVICE: &str =
    "https://hydro.nationalmap.gov/arcgis/rest/services/nhd/MapServer";
const WATER_MASK_NHD_LAYERS: &[(u32, &str, &str)] = &[
    (
        9,
        "Area - Large Scale",
        "AREASQKM >= 1 AND FTYPE IN (312,445,460)",
    ),
    (
        12,
        "Waterbody - Large Scale",
        "AREASQKM >= 1 AND FTYPE IN (378,390,436,493)",
    ),
];
const TERRAIN_MIN_ZOOM: u32 = 0;
const TERRAIN_ZOOM: u32 = 10;
const TERRAIN_TILE_SIZE: u32 = 512;
const RASTER_BASEMAP_MAX_DISPLAY_ZOOM: f64 = 12.5;

pub fn explain_product_build(config: &ProductBuildConfig) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("profile {}", config.profile.as_str()));
    lines.push(format!("build_root {}", config.build_root.display()));
    lines.push(format!(
        "chart_cutline_root {}",
        config.chart_cutline_root.display()
    ));
    lines.push(format!(
        "fetch_cache_root {}",
        config.fetch_cache_root.display()
    ));
    lines.push(format!("fetch_cache_mode {}", config.fetch_cache_mode));
    lines.push(format!("max_heavy_jobs {}", config.max_heavy_jobs));
    lines.push("nodes".to_string());
    lines.push("  source-urls".to_string());
    for family in ["sec", "tac", "enr-l", "enr-h"] {
        lines.push(format!("  charts-{family}"));
    }
    lines.push("  csup".to_string());
    for region in config.profile.tpp_regions() {
        lines.push(format!("  tpp-{}", region.code().to_ascii_lowercase()));
    }
    lines.push("  data-input-staging".to_string());
    lines.push("  data-base".to_string());
    lines.push("  data".to_string());
    lines.push("  vectors".to_string());
    lines.push("  resource-index".to_string());
    lines.push("  nav-db".to_string());
    lines.push("  bundle-manifest".to_string());
    Ok(lines.join("\n") + "\n")
}

pub fn build_product(config: &ProductBuildConfig) -> anyhow::Result<ProductBuildResult> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let log_root = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("orchestrator-logs")
        .join(if config.profile == ProductBuildProfile::Production {
            "published-packaged"
        } else {
            "published-packaged-validation"
        });
    fs::create_dir_all(&log_root)
        .with_context(|| format!("failed to create {}", log_root.display()))?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin pid={} profile={} build_root={} scheduler=product_weighted_dag scheduler_version=2 fetch_jobs={} cpu_jobs={} max_heavy_jobs={} fetch_cache_mode={}",
        std::process::id(),
        config.profile.as_str(),
        config.build_root.display(),
        config.fetch_jobs,
        config.cpu_jobs,
        config.max_heavy_jobs,
        config.fetch_cache_mode,
    ))?;

    #[derive(Debug, Clone)]
    enum ProductScheduledTaskKind {
        SourceUrls { cycle: String },
        ChartRender { cycle: String, family: ChartFamily },
        ChartPackage { cycle: String, family: ChartFamily },
        CsupStage { cycle: String },
        CsupRender { cycle: String, region: Region },
        CsupPackage { cycle: String },
        TppRender { cycle: String, region: Region },
        TppPackage { cycle: String, region: Region },
        DataBase { cycle: String },
        DataMatch { cycle: String },
        Vectors { cycle: String },
        ResourceIndex { cycle: String },
        NavDb { cycle: String },
        BundleManifest { cycle: String },
        GeoBuild,
        GeoPublish,
        TerrainDiscovery,
        TerrainBuild { region: Region },
        TerrainPublish { region: Region },
        WaterMaskBuild { region: Region },
        ShadedReliefBuild { region: Region },
        ShadedReliefPublish { region: Region },
        CurrentArtifacts,
        ProductUnpack,
        ValidatePackagedContract,
        ValidateUnpackedContract,
    }

    #[derive(Debug, Clone)]
    struct ProductScheduledTask {
        id: String,
        deps: Vec<String>,
        weight: usize,
        kind: ProductScheduledTaskKind,
    }

    fn cycle_task_id(cycle: &str, name: &str) -> String {
        format!("{cycle}:{name}")
    }

    let result = (|| -> anyhow::Result<ProductBuildResult> {
        let cycles = product_cycles_to_build(config)?;
        let chart_families = [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ];
        let work_unit_budget = config.max_heavy_jobs.max(1) * 4 + 3;
        let mut pending_tasks = Vec::new();

        for cycle in &cycles {
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "source-urls"),
                deps: vec![],
                weight: 1,
                kind: ProductScheduledTaskKind::SourceUrls {
                    cycle: cycle.clone(),
                },
            });
            for family in chart_families {
                let family_id = family_slug(family);
                let render_id = cycle_task_id(cycle, &format!("charts-{family_id}-render"));
                let package_id = cycle_task_id(cycle, &format!("charts-{family_id}-package"));
                pending_tasks.push(ProductScheduledTask {
                    id: render_id.clone(),
                    deps: vec![cycle_task_id(cycle, "source-urls")],
                    weight: 4,
                    kind: ProductScheduledTaskKind::ChartRender {
                        cycle: cycle.clone(),
                        family,
                    },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: package_id.clone(),
                    deps: vec![render_id],
                    weight: 1,
                    kind: ProductScheduledTaskKind::ChartPackage {
                        cycle: cycle.clone(),
                        family,
                    },
                });
            }

            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "csup-stage"),
                deps: vec![cycle_task_id(cycle, "source-urls")],
                weight: 1,
                kind: ProductScheduledTaskKind::CsupStage {
                    cycle: cycle.clone(),
                },
            });
            let mut csup_render_ids = Vec::new();
            for region in Region::ALL {
                let task_id = cycle_task_id(
                    cycle,
                    &format!("csup-render-{}", region.code().to_ascii_lowercase()),
                );
                csup_render_ids.push(task_id.clone());
                pending_tasks.push(ProductScheduledTask {
                    id: task_id,
                    deps: vec![cycle_task_id(cycle, "csup-stage")],
                    weight: 2,
                    kind: ProductScheduledTaskKind::CsupRender {
                        cycle: cycle.clone(),
                        region,
                    },
                });
            }
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "csup-package"),
                deps: csup_render_ids.clone(),
                weight: 1,
                kind: ProductScheduledTaskKind::CsupPackage {
                    cycle: cycle.clone(),
                },
            });

            let mut tpp_package_ids = Vec::new();
            for region in config.profile.tpp_regions() {
                let region_id = region.code().to_ascii_lowercase();
                let render_id = cycle_task_id(cycle, &format!("tpp-{region_id}"));
                let package_id = cycle_task_id(cycle, &format!("tpp-{region_id}-package"));
                pending_tasks.push(ProductScheduledTask {
                    id: render_id.clone(),
                    deps: vec![cycle_task_id(cycle, "source-urls")],
                    weight: TPP_RENDER_WEIGHT,
                    kind: ProductScheduledTaskKind::TppRender {
                        cycle: cycle.clone(),
                        region: *region,
                    },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: package_id.clone(),
                    deps: vec![render_id],
                    weight: 1,
                    kind: ProductScheduledTaskKind::TppPackage {
                        cycle: cycle.clone(),
                        region: *region,
                    },
                });
                tpp_package_ids.push(package_id);
            }

            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "data-base"),
                deps: vec![cycle_task_id(cycle, "source-urls")],
                weight: 4,
                kind: ProductScheduledTaskKind::DataBase {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "data"),
                deps: {
                    let mut deps = vec![cycle_task_id(cycle, "data-base")];
                    deps.extend(tpp_package_ids.iter().cloned());
                    deps
                },
                weight: 1,
                kind: ProductScheduledTaskKind::DataMatch {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "vectors"),
                deps: vec![cycle_task_id(cycle, "data")],
                weight: 1,
                kind: ProductScheduledTaskKind::Vectors {
                    cycle: cycle.clone(),
                },
            });
            let mut resource_index_deps = chart_families
                .iter()
                .map(|family| {
                    cycle_task_id(cycle, &format!("charts-{}-package", family_slug(*family)))
                })
                .collect::<Vec<_>>();
            resource_index_deps.push(cycle_task_id(cycle, "csup-package"));
            resource_index_deps.extend(tpp_package_ids.iter().cloned());
            resource_index_deps.push(cycle_task_id(cycle, "data"));
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "resource-index"),
                deps: resource_index_deps,
                weight: 2,
                kind: ProductScheduledTaskKind::ResourceIndex {
                    cycle: cycle.clone(),
                },
            });
            let mut nav_db_deps = vec![
                cycle_task_id(cycle, "data"),
                cycle_task_id(cycle, "resource-index"),
                cycle_task_id(cycle, "vectors"),
            ];
            nav_db_deps.extend(static_product_task_ids(config));
            if include_static_terrain_products() {
                nav_db_deps.extend(config.profile.terrain_regions().iter().map(|region| {
                    format!("build-shaded-relief-{}", region.code().to_ascii_lowercase())
                }));
            }
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "nav-db"),
                deps: nav_db_deps,
                weight: 1,
                kind: ProductScheduledTaskKind::NavDb {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "bundle-manifest"),
                deps: vec![cycle_task_id(cycle, "nav-db")],
                weight: 1,
                kind: ProductScheduledTaskKind::BundleManifest {
                    cycle: cycle.clone(),
                },
            });
        }

        pending_tasks.push(ProductScheduledTask {
            id: "build-geo".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::GeoBuild,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "publish-geo".to_string(),
            deps: vec!["build-geo".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::GeoPublish,
        });
        if include_static_terrain_products() {
            pending_tasks.push(ProductScheduledTask {
                id: "terrain-discovery".to_string(),
                deps: vec![],
                weight: 1,
                kind: ProductScheduledTaskKind::TerrainDiscovery,
            });
            for region in config.profile.terrain_regions() {
                let region_id = region.code().to_ascii_lowercase();
                pending_tasks.push(ProductScheduledTask {
                    id: format!("build-terrain-{region_id}"),
                    deps: vec!["terrain-discovery".to_string()],
                    weight: 6,
                    kind: ProductScheduledTaskKind::TerrainBuild { region: *region },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: format!("publish-terrain-{region_id}"),
                    deps: vec![format!("build-terrain-{region_id}")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::TerrainPublish { region: *region },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: format!("build-water-mask-{region_id}"),
                    deps: vec![],
                    weight: 4,
                    kind: ProductScheduledTaskKind::WaterMaskBuild { region: *region },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: format!("build-shaded-relief-{region_id}"),
                    deps: vec![
                        "terrain-discovery".to_string(),
                        format!("build-water-mask-{region_id}"),
                    ],
                    weight: 6,
                    kind: ProductScheduledTaskKind::ShadedReliefBuild { region: *region },
                });
                pending_tasks.push(ProductScheduledTask {
                    id: format!("publish-shaded-relief-{region_id}"),
                    deps: vec![format!("build-shaded-relief-{region_id}")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::ShadedReliefPublish { region: *region },
                });
            }
        }
        let mut current_artifacts_deps = cycles
            .iter()
            .map(|cycle| cycle_task_id(cycle, "bundle-manifest"))
            .chain(std::iter::once("publish-geo".to_string()))
            .collect::<Vec<_>>();
        if include_static_terrain_products() {
            current_artifacts_deps.extend(
                config.profile.terrain_regions().iter().map(|region| {
                    format!("publish-terrain-{}", region.code().to_ascii_lowercase())
                }),
            );
            current_artifacts_deps.extend(config.profile.terrain_regions().iter().map(|region| {
                format!(
                    "publish-shaded-relief-{}",
                    region.code().to_ascii_lowercase()
                )
            }));
        }
        pending_tasks.push(ProductScheduledTask {
            id: "current-artifacts".to_string(),
            deps: current_artifacts_deps,
            weight: 1,
            kind: ProductScheduledTaskKind::CurrentArtifacts,
        });
        let mut product_unpack_deps =
            vec!["current-artifacts".to_string(), "publish-geo".to_string()];
        if include_static_terrain_products() {
            product_unpack_deps.extend(
                config.profile.terrain_regions().iter().map(|region| {
                    format!("publish-terrain-{}", region.code().to_ascii_lowercase())
                }),
            );
            product_unpack_deps.extend(config.profile.terrain_regions().iter().map(|region| {
                format!(
                    "publish-shaded-relief-{}",
                    region.code().to_ascii_lowercase()
                )
            }));
        }
        pending_tasks.push(ProductScheduledTask {
            id: "product-unpack".to_string(),
            deps: product_unpack_deps,
            weight: 1,
            kind: ProductScheduledTaskKind::ProductUnpack,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "validate-packaged-contract".to_string(),
            deps: vec!["current-artifacts".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::ValidatePackagedContract,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "validate-unpacked-contract".to_string(),
            deps: vec![
                "product-unpack".to_string(),
                "validate-packaged-contract".to_string(),
            ],
            weight: 1,
            kind: ProductScheduledTaskKind::ValidateUnpackedContract,
        });

        let total_tasks = pending_tasks.len();
        master_log.log(format!(
            "product-scheduler-ready tasks={} work_unit_budget={} chart_and_data_weight=4 csup_weight=2 tpp_weight={} tpp_render_jobs_per_run={} light_weight=1 resource_index_weight=2",
            total_tasks, work_unit_budget, TPP_RENDER_WEIGHT, TPP_RENDER_JOBS_PER_RUN
        ))?;

        let (tx, rx) =
            crossbeam_channel::unbounded::<(String, usize, anyhow::Result<ProductTaskCompletion>)>(
            );
        let mut running_jobs = 0_usize;
        let mut running_units = 0_usize;
        let mut launched_tasks = 0_usize;
        let mut completed_tasks = 0_usize;
        let mut completed_ids = std::collections::BTreeSet::<String>::new();
        let mut task_values = BTreeMap::<String, ProductTaskValue>::new();
        let mut task_node_records = BTreeMap::<String, Vec<NodeRecord>>::new();
        let mut worker_threads = BTreeMap::<String, thread::JoinHandle<anyhow::Result<()>>>::new();

        while running_jobs > 0 || !pending_tasks.is_empty() {
            let mut launched_any = false;
            let mut index = 0_usize;
            while index < pending_tasks.len() {
                let task = &pending_tasks[index];
                let deps_ready = task.deps.iter().all(|dep| completed_ids.contains(dep));
                let fits_budget = running_units + task.weight <= work_unit_budget;
                if !deps_ready || !fits_budget {
                    index += 1;
                    continue;
                }

                let task = pending_tasks.remove(index);
                let task_id = task.id.clone();
                let task_weight = task.weight;
                launched_tasks += 1;
                master_log.log(format!(
                    "launch {} launched={}/{} completed={}/{} weight={} running_units={}/{}",
                    task_id,
                    launched_tasks,
                    total_tasks,
                    completed_tasks,
                    total_tasks,
                    task_weight,
                    running_units + task_weight,
                    work_unit_budget,
                ))?;
                let tx = tx.clone();
                let config = config.clone();
                let task_values_snapshot = task_values.clone();
                let task_node_records_snapshot = task_node_records.clone();
                let worker_task_id = task_id.clone();
                let join_handle = thread::spawn(move || -> anyhow::Result<()> {
                    let task_label = worker_task_id.clone();
                    let completion_guard =
                        ProductCompletionGuard::new(tx, worker_task_id.clone(), task_weight);
                    let result = panic::catch_unwind(AssertUnwindSafe(|| match task.kind {
                        ProductScheduledTaskKind::SourceUrls { cycle } => {
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle.clone());
                            let (source_urls_dir, source_urls_record) =
                                build_source_urls_node(&cycle_config)?;
                            let chart_versions = [
                                ("sec".to_string(), chart_family_version_label(&source_urls_dir, ChartFamily::Sec)?),
                                ("tac".to_string(), chart_family_version_label(&source_urls_dir, ChartFamily::Tac)?),
                                ("enr-l".to_string(), chart_family_version_label(&source_urls_dir, ChartFamily::EnrL)?),
                                ("enr-h".to_string(), chart_family_version_label(&source_urls_dir, ChartFamily::EnrH)?),
                            ]
                            .into_iter()
                            .collect::<BTreeMap<_, _>>();
                            let csup_version = csup_version_label(&source_urls_dir)?;
                            let tpp_versions = config
                                .profile
                                .tpp_regions()
                                .iter()
                                .map(|region| {
                                    Ok((
                                        region.code().to_ascii_lowercase(),
                                        tpp_region_version_label(&source_urls_dir, *region)?,
                                    ))
                                })
                                .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
                            let data_version = data_version_label(&source_urls_dir)?;
                            let bundle_cycle = data_manifest_cycle(&source_urls_dir)?;
                            let completion_detail = format!(
                                "cycle bundle={} charts=sec:{} tac:{} enr-l:{} enr-h:{} csup:{} tpp={} data:{}",
                                bundle_cycle,
                                chart_versions["sec"],
                                chart_versions["tac"],
                                chart_versions["enr-l"],
                                chart_versions["enr-h"],
                                csup_version,
                                config
                                    .profile
                                    .tpp_regions()
                                    .iter()
                                    .map(|region| {
                                        let key = region.code().to_ascii_lowercase();
                                        format!("{}:{}", key, tpp_versions[&key])
                                    })
                                    .collect::<Vec<_>>()
                                    .join(","),
                                data_version,
                            );
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(
                                    source_urls_record,
                                    &cycle_config.build_root,
                                )],
                                value: ProductTaskValue::SourceUrls {
                                    dir: source_urls_dir,
                                    chart_versions,
                                    csup_version,
                                    tpp_versions,
                                    data_version,
                                    bundle_cycle: bundle_cycle.clone(),
                                },
                                completion_detail,
                            })
                        }
                        ProductScheduledTaskKind::ChartRender { cycle, family } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let family_id = family_slug(family).to_string();
                            let record = build_chart_render_node(
                                &cycle_config,
                                family,
                                &cycle_config.chart_cutline_root,
                                &source_urls.join(format!("charts-{family_id}/source_urls.jsonl")),
                                cycle_config.fetch_jobs,
                                cycle_config.cpu_jobs.min(8).max(1),
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::CsupStage { cycle } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let record = build_csup_stage_node(
                                &cycle_config,
                                Path::new(""),
                                &source_urls.join("csup/source_urls.jsonl"),
                                cycle_config.fetch_jobs,
                            )?;
                            let work_dir = resolve_artifact_path(&cycle_config, output_path(&record, "work_dir")?);
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record.clone(), &cycle_config.build_root)],
                                value: ProductTaskValue::CsupStage { record, work_dir },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::CsupRender { cycle, region } => {
                            let source_urls_key = cycle_task_id(&cycle, "source-urls");
                            let source_urls = match task_values_snapshot.get(&source_urls_key) {
                                Some(ProductTaskValue::SourceUrls { csup_version, .. }) => csup_version.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let stage = match task_values_snapshot.get(&cycle_task_id(&cycle, "csup-stage")) {
                                Some(ProductTaskValue::CsupStage { record, work_dir }) => (record.clone(), work_dir.clone()),
                                _ => bail!("missing csup stage for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let record = build_csup_render_node(
                                &cycle_config,
                                region,
                                &stage.1,
                                &stage.0.fingerprint,
                                &source_urls,
                                cycle_config.cpu_jobs.max(1),
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::TppRender { cycle, region } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let region_id = region.code().to_ascii_lowercase();
                            let request = NativeTppRunRequest {
                                region,
                                source_repo: PathBuf::new(),
                                run_root: PathBuf::new(),
                                prefetch_source_urls: Some(
                                    source_urls.join(format!("tpp-{region_id}/source_urls.jsonl")),
                                ),
                                fetch_jobs: cycle_config.fetch_jobs,
                                render_jobs: TPP_RENDER_JOBS_PER_RUN,
                                fetch_cache: Some(static_source_fetch_cache_config(&cycle_config)?),
                            };
                            let record = build_tpp_render_node(&cycle_config, &request)?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::DataBase { cycle } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let records = build_data_nodes(&cycle_config, &source_urls, "data-base")?;
                            let data_record = records
                                .iter()
                                .find(|record| record.name == "data-base")
                                .cloned()
                                .context("data-base task missing data node record")?;
                            let staging_record = records
                                .iter()
                                .find(|record| record.name == "data-input-staging")
                                .cloned()
                                .context("data-base task missing data input staging node record")?;
                            let zip = resolve_artifact_path(&cycle_config, output_path(&data_record, "zip")?);
                            let intermediate_sqlite_db = resolve_artifact_path(
                                &cycle_config,
                                sqlite_output_path(&data_record)?,
                            );
                            let source_input_dir = resolve_artifact_path(&cycle_config, output_path(&staging_record, "staged_input_dir")?);
                            Ok(ProductTaskCompletion {
                                node_records: records
                                    .into_iter()
                                    .map(|record| normalize_node_record_paths(record, &cycle_config.build_root))
                                    .collect(),
                                value: ProductTaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    zip,
                                    fingerprint: data_record.fingerprint,
                                },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ChartPackage { cycle, family } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls {
                                    dir,
                                    chart_versions,
                                    ..
                                }) => (dir.clone(), chart_versions.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let family_id = family_slug(family).to_string();
                            let started = Instant::now();
                            let (records, source) = build_chart_package_nodes(
                                &cycle_config,
                                family,
                                &source_urls.0,
                                source_urls
                                    .1
                                    .get(&family_id)
                                    .expect("chart family version should exist"),
                            )?;
                            let summary = summarize_package_records(&records);
                            Ok(ProductTaskCompletion {
                                node_records: records
                                    .into_iter()
                                    .map(|record| normalize_node_record_paths(record, &cycle_config.build_root))
                                    .collect(),
                                value: ProductTaskValue::ChartSource(source),
                                completion_detail: format!(
                                    "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                    started.elapsed().as_millis(),
                                    summary.total,
                                    summary.cache_hits,
                                    summary.rebuilt,
                                ),
                            })
                        }
                        ProductScheduledTaskKind::CsupPackage { cycle } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls {
                                    dir,
                                    csup_version,
                                    ..
                                }) => (dir.clone(), csup_version.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let started = Instant::now();
                            let (records, source) = build_csup_package_nodes(
                                &cycle_config,
                                &source_urls.0,
                                &source_urls.1,
                            )?;
                            let summary = summarize_package_records(&records);
                            Ok(ProductTaskCompletion {
                                node_records: records
                                    .into_iter()
                                    .map(|record| normalize_node_record_paths(record, &cycle_config.build_root))
                                    .collect(),
                                value: ProductTaskValue::CsupSource(source),
                                completion_detail: format!(
                                    "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                    started.elapsed().as_millis(),
                                    summary.total,
                                    summary.cache_hits,
                                    summary.rebuilt,
                                ),
                            })
                        }
                        ProductScheduledTaskKind::TppPackage { cycle, region } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls {
                                    dir,
                                    tpp_versions,
                                    ..
                                }) => (dir.clone(), tpp_versions.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let region_id = region.code().to_ascii_lowercase();
                            let started = Instant::now();
                            let (record, source) = build_tpp_package_node(
                                &cycle_config,
                                region,
                                &source_urls.0.join(format!("tpp-{region_id}/source_urls.jsonl")),
                                source_urls
                                    .1
                                    .get(&region_id)
                                    .expect("tpp region version should exist"),
                            )?;
                            let cache_hit = record.cache_hit;
                            let fingerprint = record.fingerprint.clone();
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::FingerprintedTppSource {
                                    source,
                                    fingerprint,
                                },
                                completion_detail: format!(
                                    "elapsed_ms={} cache_hit={}",
                                    started.elapsed().as_millis(),
                                    cache_hit,
                                ),
                            })
                        }
                        ProductScheduledTaskKind::DataMatch { cycle } => {
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { data_version, .. }) => {
                                    data_version.clone()
                                }
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let raw_data = match task_values_snapshot.get(&cycle_task_id(&cycle, "data-base")) {
                                Some(ProductTaskValue::FingerprintedData { intermediate_sqlite_db, source_input_dir, zip, fingerprint }) => {
                                    (intermediate_sqlite_db.clone(), source_input_dir.clone(), zip.clone(), fingerprint.clone())
                                }
                                _ => bail!("missing data-base output for cycle {cycle}"),
                            };
                            let tpp_sources = config
                                .profile
                                .tpp_regions()
                                .iter()
                                .map(|region| {
                                    let key = cycle_task_id(
                                        &cycle,
                                        &format!("tpp-{}-package", region.code().to_ascii_lowercase()),
                                    );
                                    match task_values_snapshot.get(&key) {
                                        Some(ProductTaskValue::FingerprintedTppSource { source, fingerprint }) => {
                                            Ok((*region, source.clone(), fingerprint.clone()))
                                        }
                                        _ => bail!("missing tpp package source for {}", region.code()),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let record = build_data_match_node(
                                &cycle_config,
                                &raw_data.0,
                                &raw_data.2,
                                &source_urls,
                                &raw_data.3,
                                &tpp_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&cycle_config, output_path(&record, "zip")?);
                            let intermediate_sqlite_db = resolve_artifact_path(
                                &cycle_config,
                                sqlite_output_path(&record)?,
                            );
                            let fingerprint = record.fingerprint.clone();
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir: raw_data.1,
                                    zip,
                                    fingerprint,
                                },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ProductScheduledTaskKind::Vectors { cycle } => {
                            let (data, source_input_dir, data_fingerprint) = match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData { intermediate_sqlite_db, source_input_dir, fingerprint, .. }) => {
                                    (intermediate_sqlite_db.clone(), source_input_dir.clone(), fingerprint.clone())
                                }
                                _ => bail!("missing data output for cycle {cycle}"),
                            };
                            let data_version = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { data_version, .. }) => {
                                    data_version.clone()
                                }
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let record = build_vectors_node(&cycle_config, &data, &source_input_dir, &data_fingerprint, &data_version)?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&cycle_config, output_path(&record, "zip")?);
                            let errors = Some(resolve_artifact_path(&cycle_config, output_path(&record, "errors")?));
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::FingerprintedZip {
                                    zip,
                                    errors,
                                },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ProductScheduledTaskKind::ResourceIndex { cycle } => {
                            let data_zip = match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData { zip, .. }) => zip.clone(),
                                _ => bail!("missing data output for cycle {cycle}"),
                            };
                            let chart_sources = ["sec", "tac", "enr-l", "enr-h"]
                                .iter()
                                .map(|family_id| {
                                    let key = cycle_task_id(&cycle, &format!("charts-{family_id}-package"));
                                    match task_values_snapshot.get(&key) {
                                        Some(ProductTaskValue::ChartSource(source)) => Ok(source.clone()),
                                        _ => bail!("missing chart source for cycle {cycle} family {family_id}"),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let csup_sources = vec![match task_values_snapshot.get(&cycle_task_id(&cycle, "csup-package")) {
                                Some(ProductTaskValue::CsupSource(source)) => source.clone(),
                                _ => bail!("missing csup package source for cycle {cycle}"),
                            }];
                            let tpp_sources = config
                                .profile
                                .tpp_regions()
                                .iter()
                                .map(|region| {
                                    let key = cycle_task_id(
                                        &cycle,
                                        &format!("tpp-{}-package", region.code().to_ascii_lowercase()),
                                    );
                                    match task_values_snapshot.get(&key) {
                                        Some(ProductTaskValue::FingerprintedTppSource { source, .. }) => Ok(source.clone()),
                                        _ => bail!("missing tpp package source for cycle {cycle} {}", region.code()),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let record = build_resource_index_node(
                                &cycle_config,
                                &data_zip,
                                chart_sources,
                                tpp_sources,
                                csup_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::None,
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ProductScheduledTaskKind::NavDb { cycle } => {
                            let resource_index_path = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "resource-index"))
                            {
                                Some(ProductTaskValue::None) => {
                                    let resource_index_record = task_node_records_snapshot
                                        .get(&cycle_task_id(&cycle, "resource-index"))
                                        .and_then(|records| records.iter().find(|record| record.name == "resource-index"))
                                        .cloned()
                                        .context("missing resource-index node record")?;
                                    resolve_artifact_path(
                                        &config,
                                        output_path(&resource_index_record, "resource_index")?,
                                    )
                                }
                                _ => bail!("missing resource-index output for cycle {cycle}"),
                            };
                            let intermediate_sqlite_db = match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData { intermediate_sqlite_db, .. }) => intermediate_sqlite_db.clone(),
                                _ => bail!("missing data output for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle.clone());
                            let shaded_relief_tile_levels = if include_static_terrain_products() {
                                collect_shaded_relief_tile_levels(&task_values_snapshot, &cycle_config)?
                            } else {
                                Vec::new()
                            };
                            let stable_packages = static_product_task_ids(&cycle_config)
                                .iter()
                                .map(|task_id| match task_values_snapshot.get(task_id) {
                                    Some(ProductTaskValue::PublishedStandaloneProduct {
                                        id,
                                        published_zip,
                                        sha256,
                                        size_bytes,
                                        source_version,
                                        source_fetched_at_utc,
                                        ..
                                    }) => build_stable_bundle_package_artifact(
                                        id,
                                        published_zip,
                                        sha256,
                                        *size_bytes,
                                        source_version,
                                        source_fetched_at_utc.clone(),
                                    ),
                                    _ => bail!(
                                        "missing published stable product output for {}",
                                        task_id
                                    ),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let vectors_record = task_node_records_snapshot
                                .get(&cycle_task_id(&cycle, "vectors"))
                                .and_then(|records| records.iter().find(|record| record.name == "vectors"))
                                .cloned()
                                .context("missing vectors node record")?;
                            let vectors_zip_path =
                                resolve_artifact_path(&cycle_config, output_path(&vectors_record, "zip")?);
                            let vectors_sha256 =
                                output_sha_or_hash(&vectors_record, "zip", &vectors_zip_path)?;
                            let vectors_filename = format!(
                                "vectors_data_{cycle}_{PACKAGE_CYCLE_VERSION}_{vectors_sha256}.zip"
                            );
                            publish_flat_artifact(
                                &vectors_zip_path,
                                &cycle_config.build_root.join(&vectors_filename),
                            )?;
                            let resource_index: ResourceIndex = serde_json::from_slice(
                                &fs::read(&resource_index_path).with_context(|| {
                                    format!("failed to read {}", resource_index_path.display())
                                })?,
                            )
                            .with_context(|| {
                                format!("failed to parse {}", resource_index_path.display())
                            })?;
                            let start_valid = resource_index
                                .temporal_summary
                                .uniform_good_beyond_date
                                .clone()
                                .or_else(|| {
                                    resource_index.temporal_summary.uniform_effective_date.clone()
                                })
                                .context("resource-index missing start-valid date")?;
                            let end_valid = resource_index
                                .temporal_summary
                                .uniform_expiration_date
                                .clone()
                                .or_else(|| {
                                    resource_index
                                        .temporal_summary
                                        .expiration_dates
                                        .first()
                                        .cloned()
                                })
                                .context("resource-index missing end-valid date")?;
                            let vectors_package = BundlePackageArtifact {
                                id: format!("VECTORS_DATA_{cycle}_{PACKAGE_CYCLE_VERSION}"),
                                family_id: "vectors".to_string(),
                                region_id: None,
                                filename: vectors_filename.clone(),
                                relative_path: vectors_filename,
                                cycle: Some(cycle.clone()),
                                cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
                                checksum_sha256: vectors_sha256,
                                size_bytes: fs::metadata(&vectors_zip_path)
                                    .with_context(|| {
                                        format!("failed to stat {}", vectors_zip_path.display())
                                    })?
                                    .len(),
                                published_at_utc: None,
                                source_generated_at_utc: None,
                                source_version: None,
                                source_fetched_at_utc: None,
                                effective_date: Some(start_valid),
                                expiration_date: Some(end_valid),
                                metadata: BTreeMap::new(),
                            };
                            let built = build_nav_kv_artifact(
                                &cycle_config,
                                &resource_index_path,
                                &intermediate_sqlite_db,
                                &cycle,
                                &vectors_package,
                                &stable_packages,
                                &shaded_relief_tile_levels,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(
                                    built.node_record,
                                    &cycle_config.build_root,
                                )],
                                value: ProductTaskValue::PublishedNavDb {
                                    package: built.package,
                                },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::BundleManifest { cycle } => {
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle.clone());
                            let source_urls = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { bundle_cycle, .. }) => bundle_cycle.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut node_records = task_node_records_snapshot
                                .iter()
                                .filter(|(task_id, _)| task_id.starts_with(&format!("{cycle}:")))
                                .flat_map(|(_, records)| records.clone())
                                .collect::<Vec<_>>();
                            node_records.sort_by(|left, right| left.name.cmp(&right.name));
                            let build_manifest = BuildManifest {
                                schema_version: 1,
                                profile: cycle_config.profile.as_str().to_string(),
                                cycle: source_urls.clone(),
                                build_root: relative_product_build_path(&cycle_config.build_root),
                                generated_at_utc: manifest_generated_at(&node_records),
                                fetch_cache_root: relative_artifact_path(
                                    &cycle_config.fetch_cache_root,
                                    &cycle_config.build_root,
                                ),
                                fetch_cache_mode: cycle_config.fetch_cache_mode.clone(),
                                nodes: node_records,
                            };
                            let build_manifest_path =
                                internal_build_manifest_path(&cycle_config, &source_urls)?;
                            fs::write(
                                &build_manifest_path,
                                serde_json::to_vec_pretty(&build_manifest)
                                    .context("failed to encode product build manifest")?,
                            )
                            .with_context(|| {
                                format!("failed to write {}", build_manifest_path.display())
                            })?;
                            let stable_packages = static_product_task_ids(&cycle_config)
                                .iter()
                                .map(|task_id| match task_values_snapshot.get(task_id) {
                                    Some(ProductTaskValue::PublishedStandaloneProduct {
                                        id,
                                        published_zip,
                                        sha256,
                                        size_bytes,
                                        source_version,
                                        source_fetched_at_utc,
                                        ..
                                    }) => build_stable_bundle_package_artifact(
                                        id,
                                        published_zip,
                                        sha256,
                                        *size_bytes,
                                        source_version,
                                        source_fetched_at_utc.clone(),
                                    ),
                                    _ => bail!(
                                        "missing published stable product output for {}",
                                        task_id
                                    ),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let nav_db_package = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "nav-db"))
                            {
                                Some(ProductTaskValue::PublishedNavDb { package }) => package.clone(),
                                _ => bail!("missing nav-db output for cycle {cycle}"),
                            };
                            let bundle_manifest = build_bundle_manifest(
                                &cycle_config,
                                &build_manifest,
                                &stable_packages,
                                &nav_db_package,
                            )?;
                            let bundle_manifest_path =
                                write_hashed_bundle_manifest(&cycle_config.build_root, &bundle_manifest)?;
                            validate_bundle_manifest(&cycle_config.build_root, &bundle_manifest_path)?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::CycleManifest {
                                    path: bundle_manifest_path,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::GeoBuild => {
                            let (zip_path, source_version, record) = build_geo_product(&config)?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc: None,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::TerrainDiscovery => {
                            let (index_path, source_fetched_at_utc, record) =
                                build_terrain_discovery_index(&config)?;
                            let cache_hit = record.cache_hit;
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::TerrainDiscovery {
                                    index_path,
                                    source_fetched_at_utc,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::TerrainBuild { region } => {
                            let (index_path, source_fetched_at_utc) =
                                match task_values_snapshot.get("terrain-discovery") {
                                    Some(ProductTaskValue::TerrainDiscovery {
                                        index_path,
                                        source_fetched_at_utc,
                                    }) => (index_path.clone(), source_fetched_at_utc.clone()),
                                    _ => bail!("missing terrain discovery output"),
                                };
                            let (zip_path, source_version, source_fetched_at_utc, record) =
                                build_terrain_product(
                                    &config,
                                    region,
                                    &index_path,
                                    source_fetched_at_utc,
                                )?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::WaterMaskBuild { region } => {
                            let (_zip_path, mask_tiles_dir, source_version, _source_fetched_at_utc, record) =
                                build_water_mask_product(&config, region)?;
                            let cache_hit = record.cache_hit;
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltWaterMask {
                                    mask_tiles_dir,
                                    source_version,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::ShadedReliefBuild { region } => {
                            let (index_path, source_fetched_at_utc) =
                                match task_values_snapshot.get("terrain-discovery") {
                                    Some(ProductTaskValue::TerrainDiscovery {
                                        index_path,
                                        source_fetched_at_utc,
                                    }) => (index_path.clone(), source_fetched_at_utc.clone()),
                                    _ => bail!("missing terrain discovery output"),
                                };
                            let region_id = region.code().to_ascii_lowercase();
                            let (water_mask_dir, water_mask_version) = match task_values_snapshot
                                .get(&format!("build-water-mask-{region_id}"))
                            {
                                Some(ProductTaskValue::BuiltWaterMask {
                                    mask_tiles_dir,
                                    source_version,
                                }) => (mask_tiles_dir.clone(), source_version.clone()),
                                _ => bail!("missing water mask output for {}", region.code()),
                            };
                            let (
                                zip_path,
                                source_version,
                                source_fetched_at_utc,
                                tile_levels,
                                record,
                            ) =
                                build_shaded_relief_product(
                                    &config,
                                    region,
                                    &index_path,
                                    source_fetched_at_utc,
                                    &water_mask_dir,
                                    &water_mask_version,
                                )?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStaticTileProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    tile_levels,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::GeoPublish => {
                            let built = match task_values_snapshot.get("build-geo") {
                                Some(ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => (
                                    zip_path.clone(),
                                    zip_sha256.clone(),
                                    *zip_size_bytes,
                                    source_version.clone(),
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => bail!("missing geo build output"),
                            };
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_zip(
                                    &config.build_root,
                                    &built.0,
                                    "geo",
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: "geo".to_string(),
                                    source_zip_path: built.0,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    source_version: built.3,
                                    source_fetched_at_utc: built.4,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::TerrainPublish { region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let task_id = format!("build-terrain-{region_id}");
                            let built = match task_values_snapshot.get(&task_id) {
                                Some(ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => (
                                    zip_path.clone(),
                                    zip_sha256.clone(),
                                    *zip_size_bytes,
                                    source_version.clone(),
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => bail!("missing terrain build output for {}", region.code()),
                            };
                            let product_id = format!("terrain-{region_id}");
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_zip(
                                    &config.build_root,
                                    &built.0,
                                    &product_id,
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: product_id,
                                    source_zip_path: built.0,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    source_version: built.3,
                                    source_fetched_at_utc: built.4,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ShadedReliefPublish { region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let task_id = format!("build-shaded-relief-{region_id}");
                            let built = match task_values_snapshot.get(&task_id) {
                                Some(ProductTaskValue::BuiltStaticTileProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => (
                                    zip_path.clone(),
                                    zip_sha256.clone(),
                                    *zip_size_bytes,
                                    source_version.clone(),
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => bail!(
                                    "missing shaded relief build output for {}",
                                    region.code()
                                ),
                            };
                            let product_id = format!("shaded-relief-{region_id}");
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_zip(
                                    &config.build_root,
                                    &built.0,
                                    &product_id,
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: product_id,
                                    source_zip_path: built.0,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    source_version: built.3,
                                    source_fetched_at_utc: built.4,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::CurrentArtifacts => {
                            let as_of_utc = Utc::now();
                            let diagnostics = write_product_build_diagnostics(
                                &config.build_root,
                                as_of_utc.date_naive(),
                                &task_values_snapshot,
                            )?;
                            let current_artifacts_path = write_current_artifacts_manifest(
                                &config.build_root,
                                as_of_utc,
                                diagnostics.clone(),
                            )?;
                            cleanup_published_packaged_root(
                                &config.build_root,
                                &current_artifacts_path,
                            )?;
                            let diagnostic_error_count =
                                diagnostics.as_ref().map(|value| value.error_count).unwrap_or(0);
                            let completion_detail = if diagnostic_error_count > 0 {
                                format!(
                                    "published ERROR diagnostic_errors={diagnostic_error_count}"
                                )
                            } else {
                                "published diagnostic_errors=0".to_string()
                            };
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::CurrentArtifacts {
                                    path: current_artifacts_path,
                                },
                                completion_detail,
                            })
                        }
                        ProductScheduledTaskKind::ProductUnpack => {
                            let current_artifacts_path = match task_values_snapshot.get("current-artifacts") {
                                Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
                                _ => bail!("missing current artifacts output"),
                            };
                            let current_artifacts = load_current_artifacts_manifest(&current_artifacts_path)?;
                            for bundle_ref in current_artifacts
                                .bundles
                                .iter()
                                .filter(|bundle| bundle.bundle_type == "cycle")
                            {
                                let bundle_manifest_path = config.build_root.join(&bundle_ref.filename);
                                let bundle_manifest = match load_bundle_manifest_like(&bundle_manifest_path)? {
                                    BundleManifestLike::Cycle(bundle) => bundle,
                                    BundleManifestLike::Fast(_) => {
                                        bail!(
                                            "expected cycle bundle in current_artifacts, found fast bundle {}",
                                            bundle_ref.filename
                                        )
                                    }
                                };
                                let cycle = bundle_manifest.cycle.clone();
                                let mut cycle_config = config.clone();
                                cycle_config.target_cycle = Some(cycle.clone());
                                let build_manifest_path =
                                    internal_build_manifest_path(&cycle_config, &cycle)?;
                                let _: BuildManifest = serde_json::from_slice(
                                    &fs::read(&build_manifest_path).with_context(|| {
                                        format!("failed to read {}", build_manifest_path.display())
                                    })?,
                                )
                                .with_context(|| {
                                    format!("failed to parse {}", build_manifest_path.display())
                                })?;
                                sync_unpacked_metadata(
                                    &cycle_config,
                                    &bundle_manifest,
                                    &bundle_manifest_path,
                                    Some(&task_values_snapshot),
                                )?;
                            }
                            let fast_products = match current_bundle_path(
                                &current_artifacts,
                                &config.build_root,
                                "fast",
                            ) {
                                Some(path) => load_fast_bundle_products(&path)?
                                    .into_iter()
                                    .map(|product| PublishedZipArtifact {
                                        source_zip_path: product.source_zip_path,
                                        published_zip_path: product.published_zip,
                                        checksum_sha256: product.checksum_sha256,
                                    })
                                    .collect::<Vec<_>>(),
                                None => Vec::new(),
                            };
                            let static_products = static_product_task_ids(&config)
                                .iter()
                                .map(|task_id| match task_values_snapshot.get(task_id) {
                                    Some(ProductTaskValue::PublishedStandaloneProduct {
                                        source_zip_path,
                                        published_zip,
                                        sha256,
                                        ..
                                    }) => Ok(PublishedZipArtifact {
                                        source_zip_path: source_zip_path.clone(),
                                        published_zip_path: published_zip.clone(),
                                        checksum_sha256: sha256.clone(),
                                    }),
                                    _ => bail!("missing published static product output for {}", task_id),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let mut zip_artifacts = Vec::new();
                            zip_artifacts.extend(fast_products);
                            zip_artifacts.extend(static_products);
                            sync_product_level_unpacked(
                                &config.build_root,
                                &current_artifacts_path,
                                &zip_artifacts,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: "synced".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ValidatePackagedContract => {
                            let current_artifacts_path = match task_values_snapshot.get("current-artifacts") {
                                Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
                                _ => bail!("missing current artifacts output"),
                            };
                            validate_packaged_contract(&config.build_root, &current_artifacts_path)?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: "validated".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ValidateUnpackedContract => {
                            let current_artifacts_path = match task_values_snapshot.get("current-artifacts") {
                                Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
                                _ => bail!("missing current artifacts output"),
                            };
                            let unpacked_root = published_unpacked_root(&config)?;
                            validate_unpacked_contract(
                                &config.build_root,
                                &unpacked_root,
                                &current_artifacts_path,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: "validated".to_string(),
                            })
                        }
                    }))
                    .unwrap_or_else(|panic_payload| {
                        let panic_text = if let Some(text) = panic_payload.downcast_ref::<&str>() {
                            (*text).to_string()
                        } else if let Some(text) = panic_payload.downcast_ref::<String>() {
                            text.clone()
                        } else {
                            "unknown panic payload".to_string()
                        };
                        Err(anyhow::anyhow!("product task thread panicked: {task_label}: {panic_text}"))
                    });
                    completion_guard.send(result);
                    Ok(())
                });
                worker_threads.insert(task_id.clone(), join_handle);
                running_jobs += 1;
                running_units += task_weight;
                launched_any = true;
            }

            if running_jobs == 0 {
                if pending_tasks.is_empty() {
                    break;
                }
                bail!("product scheduler deadlock: no runnable tasks remain");
            }
            if !launched_any {
                // wait for a running task to free capacity or satisfy dependencies
            }

            let (task_id, task_weight, result) = loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(message) => break message,
                    Err(RecvTimeoutError::Timeout) => {
                        master_log.log(format!(
                            "product-scheduler-wait running_jobs={} pending_tasks={} running_units={}/{}",
                            running_jobs,
                            pending_tasks.len(),
                            running_units,
                            work_unit_budget,
                        ))?;
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        bail!("product scheduler channel closed unexpectedly");
                    }
                }
            };
            running_jobs -= 1;
            running_units = running_units.saturating_sub(task_weight);
            if let Some(handle) = worker_threads.remove(&task_id) {
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("failed to join product worker {task_id}"))??;
            }
            match result {
                Ok(completion) => {
                    completed_tasks += 1;
                    task_node_records.insert(task_id.clone(), completion.node_records.clone());
                    completed_ids.insert(task_id.clone());
                    task_values.insert(task_id.clone(), completion.value);
                    master_log.log(format!(
                        "complete {} completed={}/{} running_units={}/{} {}",
                        task_id,
                        completed_tasks,
                        total_tasks,
                        running_units,
                        work_unit_budget,
                        completion.completion_detail,
                    ))?;
                }
                Err(err) => {
                    master_log.log(format!("complete {task_id} FAIL error={err}"))?;
                    return Err(err);
                }
            }
        }

        let mut cycle_manifest_paths = cycles
            .iter()
            .map(
                |cycle| match task_values.get(&cycle_task_id(cycle, "bundle-manifest")) {
                    Some(ProductTaskValue::CycleManifest { path }) => Ok(path.clone()),
                    _ => bail!("missing cycle manifest for {cycle}"),
                },
            )
            .collect::<anyhow::Result<Vec<_>>>()?;
        cycle_manifest_paths.sort();
        let current_artifacts_path = match task_values.get("current-artifacts") {
            Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
            _ => bail!("missing current artifacts output"),
        };
        record_gc_roots(config, "full", &task_node_records)?;

        Ok(ProductBuildResult {
            cycle_manifest_paths,
            current_artifacts_path,
        })
    })();

    match result {
        Ok(result) => {
            master_log.log(format!(
                "complete PASS current_artifacts={}",
                result.current_artifacts_path.display()
            ))?;
            Ok(result)
        }
        Err(err) => {
            master_log.log(format!("complete FAIL error={err}"))?;
            Err(err)
        }
    }
}

pub fn build_fast_subset(config: &ProductBuildConfig) -> anyhow::Result<FastSubsetBuildResult> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let current_artifacts_path = current_artifacts_path_for_fast_subset(config)?;
    let mut current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(&current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;
    let previous_fast_products = match current_bundle_path(&current, &config.build_root, "fast") {
        Some(path) => load_fast_bundle_products(&path)?,
        None => Vec::new(),
    };
    let previous_fast_products_by_id = previous_fast_products
        .iter()
        .map(|product| (product.id.clone(), product.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut gc_records = BTreeMap::new();
    let mut fast_products = Vec::new();
    for product in [
        build_or_reuse_fast_product(
            config,
            "obstacles",
            &previous_fast_products_by_id,
            &mut gc_records,
            build_obstacles_product,
        )?,
        build_or_reuse_fast_product(
            config,
            "tfrs",
            &previous_fast_products_by_id,
            &mut gc_records,
            build_tfrs_product,
        )?,
        build_or_reuse_fast_product(
            config,
            "metars",
            &previous_fast_products_by_id,
            &mut gc_records,
            build_metars_product,
        )?,
        build_or_reuse_fast_product(
            config,
            "nexrad",
            &previous_fast_products_by_id,
            &mut gc_records,
            build_nexrad_product,
        )?,
    ] {
        if let Some(product) = product {
            fast_products.push(product);
        }
    }
    let fast_bundle_products = fast_products.clone();
    let published_at_utc = utc_now_string();
    let fast_bundle_manifest_path =
        publish_fast_bundle_manifest(&config.build_root, &fast_bundle_products, &published_at_utc)?;
    sync_unpacked_fast_bundle_manifest(config, &fast_bundle_manifest_path)?;
    let as_of_utc = Utc::now();
    current.as_of_date = as_of_utc.date_naive().format("%Y-%m-%d").to_string();
    current.as_of_utc = as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true);
    current.bundles = build_current_bundle_entries(&config.build_root, as_of_utc.date_naive())?;

    let output_path = config
        .build_root
        .join(current_artifacts_latest_alias_filename());
    let immutable_path = config
        .build_root
        .join(current_artifacts_immutable_filename(as_of_utc));
    write_current_artifacts_json(&immutable_path, &current)?;
    write_current_artifacts_json(&output_path, &current)?;
    cleanup_published_packaged_root(&config.build_root, &output_path)?;

    sync_fast_subset_unpacked(
        &config.build_root,
        &output_path,
        &previous_fast_products,
        &fast_products,
    )?;
    validate_packaged_contract(&config.build_root, &output_path)?;
    let unpacked_root = published_unpacked_root(config)?;
    validate_unpacked_contract(&config.build_root, &unpacked_root, &output_path)?;
    record_gc_roots(config, "fast", &gc_records)?;

    Ok(FastSubsetBuildResult {
        current_artifacts_path: output_path,
        fast_products,
    })
}

pub fn publish_discovery_manifest(
    config: &ProductBuildConfig,
    as_of_utc: DateTime<Utc>,
    bundle_filenames: &[String],
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    if bundle_filenames.is_empty() {
        bail!("publish-discovery-manifest requires at least one --bundle");
    }
    let latest_alias_path = config
        .build_root
        .join(current_artifacts_latest_alias_filename());
    if !latest_alias_path.is_file() {
        bail!(
            "missing current artifacts alias {}; build-product first",
            latest_alias_path.display()
        );
    }
    let bundles = bundle_filenames
        .iter()
        .map(|filename| current_bundle_entry_from_path(&config.build_root.join(filename)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        as_of_date: as_of_utc.date_naive().format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        diagnostics: None,
    };
    let immutable_path = config
        .build_root
        .join(current_artifacts_immutable_filename(as_of_utc));
    write_current_artifacts_json(&immutable_path, &manifest)?;
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(&config.build_root, &latest_alias_path, &unpacked_root)?;
    cleanup_published_packaged_root(&config.build_root, &latest_alias_path)?;
    cleanup_published_unpacked_root(&unpacked_root, &latest_alias_path)?;
    validate_packaged_contract(&config.build_root, &latest_alias_path)?;
    validate_unpacked_contract(&config.build_root, &unpacked_root, &latest_alias_path)?;
    Ok(immutable_path)
}

fn obstacle_snapshot_label(value: &str) -> anyhow::Result<String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("failed to parse obstacle snapshot date {value}"))?;
    Ok(date.format("%Y.%m.%d").to_string())
}

fn build_obstacles_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let snapshot_date = Utc::now().date_naive();
    let snapshot_label = obstacle_snapshot_label(&snapshot_date.format("%Y-%m-%d").to_string())?;
    let source_generated_at_utc = format!("{}T00:00:00Z", snapshot_date.format("%Y-%m-%d"));
    let logical_url = format!(
        "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_{snapshot_label}.zip"
    );
    let inputs = BTreeMap::from([
        ("product_id".to_string(), "obstacles".to_string()),
        ("source_url".to_string(), logical_url.clone()),
        (
            "vectors_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-vectors/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "fast-obstacles")?,
        "fast-obstacles",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join(format!("obstacles_{snapshot_label}.manifest"));
    let stats_path = output_dir.join("stats.json");
    let zip_path = output_dir.join(format!("obstacles_{snapshot_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[manifest_path.clone(), stats_path.clone(), zip_path.clone()],
    )? {
        NodeCacheState::CacheHit(record) => return Ok((zip_path, source_generated_at_utc, record)),
        NodeCacheState::Build(lock) => lock,
    };

    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let work_dir = prepared.dir.join("work");
    let input_dir = work_dir.join("input");
    let provenance_dir = prepared
        .dir
        .join("meta")
        .join("provenance")
        .join("obstacles");
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"obstacles\",\"url\":\"{}\"}}\n",
            logical_url
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    prefetch_archives_with_provenance(
        std::slice::from_ref(&logical_url),
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "obstacles",
    )?;
    let result = build_obstacle_dataset(&BuildObstacleDatasetRequest {
        input_dir,
        output_dir,
        version_label: snapshot_label,
    })?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "stats".to_string(),
            relative_artifact_path(&result.stats_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((result.zip_path, source_generated_at_utc, record))
}

fn publish_built_fast_product(
    config: &ProductBuildConfig,
    id: &str,
    built: (PathBuf, String, NodeRecord),
) -> anyhow::Result<PublishedFastProductResult> {
    let (source_zip_path, source_generated_at_utc, record) = built;
    let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
    let (published_zip, checksum_sha256, size_bytes) = publish_content_addressed_fast_product_zip(
        &config.build_root,
        id,
        &source_zip_path,
        zip_sha256.as_deref(),
        zip_size_bytes,
    )?;
    Ok(PublishedFastProductResult {
        id: id.to_string(),
        source_zip_path,
        published_zip,
        checksum_sha256,
        size_bytes,
        source_generated_at_utc,
    })
}

fn build_or_reuse_fast_product<F>(
    config: &ProductBuildConfig,
    id: &str,
    previous_fast_products_by_id: &BTreeMap<String, PublishedFastProductResult>,
    gc_records: &mut BTreeMap<String, Vec<NodeRecord>>,
    build_product: F,
) -> anyhow::Result<Option<PublishedFastProductResult>>
where
    F: FnOnce(&ProductBuildConfig) -> anyhow::Result<(PathBuf, String, NodeRecord)>,
{
    match build_product(config).and_then(|built| {
        gc_records.insert(format!("fast:{id}"), vec![built.2.clone()]);
        publish_built_fast_product(config, id, built)
    }) {
        Ok(product) => Ok(Some(product)),
        Err(error) => {
            if let Some(previous) = previous_fast_products_by_id.get(id) {
                eprintln!(
                    "WARNING fast product {id} failed; reusing previous package {}: {error:#}",
                    previous.published_zip.display()
                );
                Ok(Some(previous.clone()))
            } else {
                eprintln!(
                    "WARNING fast product {id} failed and no previous package exists; omitting it from fast bundle: {error:#}"
                );
                Ok(None)
            }
        }
    }
}

fn current_artifacts_path_for_fast_subset(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    let latest_alias = config
        .build_root
        .join(current_artifacts_latest_alias_filename());
    if latest_alias.is_file() {
        return Ok(latest_alias);
    }

    let mut candidates = fs::read_dir(&config.build_root)
        .with_context(|| format!("failed to read {}", config.build_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", config.build_root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("current_artifacts_")
                        && name.ends_with(".json")
                        && name
                            .strip_prefix("current_artifacts_")
                            .is_some_and(|suffix| suffix.contains('T'))
                })
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop().with_context(|| {
        format!(
            "no current_artifacts discovery manifest exists in {}; run build-product first",
            config.build_root.display()
        )
    })
}

fn sync_fast_subset_unpacked(
    build_root: &Path,
    current_artifacts_path: &Path,
    previous_fast_products: &[PublishedFastProductResult],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_build_root(build_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(build_root, current_artifacts_path, &unpacked_root)?;
    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;
    if let Some(diagnostics) = &current.diagnostics {
        sync_unpacked_file(&build_root.join(&diagnostics.filename), &unpacked_root)?;
    }
    sync_referenced_fast_bundle_unpacked_zips(
        build_root,
        &unpacked_root,
        current_artifacts_path,
        previous_fast_products,
        fast_products,
    )?;
    cleanup_published_unpacked_root(&unpacked_root, current_artifacts_path)?;
    Ok(())
}

fn sync_referenced_fast_bundle_unpacked_zips(
    build_root: &Path,
    unpacked_root: &Path,
    current_artifacts_path: &Path,
    previous_fast_products: &[PublishedFastProductResult],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let mut products_by_filename = BTreeMap::<String, PublishedFastProductResult>::new();
    for product in previous_fast_products.iter().chain(fast_products.iter()) {
        let Some(filename) = product
            .published_zip
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            bail!("failed to determine published fast filename");
        };
        products_by_filename.insert(filename, product.clone());
    }
    for entry in fs::read_dir(build_root)
        .with_context(|| format!("failed to read {}", build_root.display()))?
    {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !(name.starts_with("bundle_fast_") && name.ends_with(".json")) {
            continue;
        }
        for product in load_fast_bundle_products(&path)? {
            let Some(filename) = product
                .published_zip
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
            else {
                bail!("failed to determine published fast filename");
            };
            products_by_filename.entry(filename).or_insert(product);
        }
    }
    for discovery_path in discovery_manifest_paths(build_root, current_artifacts_path)? {
        let current: CurrentArtifactsManifest = serde_json::from_slice(
            &fs::read(&discovery_path)
                .with_context(|| format!("failed to read {}", discovery_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", discovery_path.display()))?;
        for bundle in current
            .bundles
            .iter()
            .filter(|bundle| bundle.bundle_type == "fast")
        {
            let bundle_path = build_root.join(&bundle.filename);
            let fast_bundle: FastBundleManifest = serde_json::from_slice(
                &fs::read(&bundle_path)
                    .with_context(|| format!("failed to read {}", bundle_path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
            for package in &fast_bundle.packages {
                let unpack_dir = unpacked_target_dir(unpacked_root, &package.filename)?;
                let marker_path = unpacked_marker_path(unpacked_root, &package.filename)?;
                if unpack_dir.is_dir()
                    && fs::read_to_string(&marker_path)
                        .ok()
                        .as_deref()
                        .map(str::trim)
                        == Some(package.checksum_sha256.as_str())
                {
                    continue;
                }
                let Some(product) = products_by_filename.get(&package.filename) else {
                    bail!(
                        "no source mapping available to mirror historical fast package {}",
                        package.filename
                    );
                };
                if product.source_zip_path == product.published_zip {
                    sync_unpacked_zip_by_extract(
                        &product.published_zip,
                        unpacked_root,
                        &package.filename,
                        Some(&package.checksum_sha256),
                    )?;
                } else {
                    sync_unpacked_zip_from_source(
                        &product.published_zip,
                        product
                            .source_zip_path
                            .parent()
                            .unwrap_or_else(|| Path::new("/")),
                        unpacked_root,
                        &package.filename,
                        Some(&package.checksum_sha256),
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub fn build_cycle(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let log_root = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("orchestrator-logs")
        .join(if config.profile == ProductBuildProfile::Production {
            "published-packaged"
        } else {
            "published-packaged-validation"
        });
    fs::create_dir_all(&log_root)
        .with_context(|| format!("failed to create {}", log_root.display()))?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin pid={} profile={} build_root={} scheduler=weighted_dag scheduler_version=2 max_heavy_jobs={} cpu_jobs={} fetch_jobs={} fetch_cache_mode={}",
        std::process::id(),
        config.profile.as_str(),
        config.build_root.display(),
        config.max_heavy_jobs,
        config.cpu_jobs,
        config.fetch_jobs,
        config.fetch_cache_mode
    ))?;

    let result = (|| -> anyhow::Result<PathBuf> {
        let mut node_records = Vec::new();
        let (source_urls_dir, source_urls_record) = build_source_urls_node(config)?;
        master_log.log(format!(
            "complete source-urls cache_hit={}",
            source_urls_record.cache_hit
        ))?;
        node_records.push(normalize_node_record_paths(
            source_urls_record,
            &config.build_root,
        ));

        let chart_versions = [
            (
                "sec".to_string(),
                chart_family_version_label(&source_urls_dir, ChartFamily::Sec)?,
            ),
            (
                "tac".to_string(),
                chart_family_version_label(&source_urls_dir, ChartFamily::Tac)?,
            ),
            (
                "enr-l".to_string(),
                chart_family_version_label(&source_urls_dir, ChartFamily::EnrL)?,
            ),
            (
                "enr-h".to_string(),
                chart_family_version_label(&source_urls_dir, ChartFamily::EnrH)?,
            ),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let csup_version = csup_version_label(&source_urls_dir)?;
        let tpp_versions = config
            .profile
            .tpp_regions()
            .iter()
            .map(|region| {
                Ok((
                    region.code().to_ascii_lowercase(),
                    tpp_region_version_label(&source_urls_dir, *region)?,
                ))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
        let data_version = data_version_label(&source_urls_dir)?;
        let bundle_cycle = data_manifest_cycle(&source_urls_dir)?;
        master_log.log(format!(
            "cycle bundle={} charts=sec:{} tac:{} enr-l:{} enr-h:{} csup:{} tpp={} data:{}",
            bundle_cycle,
            chart_versions["sec"],
            chart_versions["tac"],
            chart_versions["enr-l"],
            chart_versions["enr-h"],
            csup_version,
            config
                .profile
                .tpp_regions()
                .iter()
                .map(|region| {
                    let key = region.code().to_ascii_lowercase();
                    format!("{}:{}", key, tpp_versions[&key])
                })
                .collect::<Vec<_>>()
                .join(","),
            data_version,
        ))?;

        let chart_families = [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ];
        let work_unit_budget = config.max_heavy_jobs.max(1) * 4 + 2;
        let mut pending_tasks = Vec::new();
        for family in chart_families {
            let family_id = family_slug(family).to_string();
            pending_tasks.push(ScheduledTask {
                id: format!("charts-{family_id}-render"),
                deps: vec![],
                weight: 4,
                kind: ScheduledTaskKind::ChartRender { family },
            });
            pending_tasks.push(ScheduledTask {
                id: format!("charts-{family_id}-package"),
                deps: vec![format!("charts-{family_id}-render")],
                weight: 1,
                kind: ScheduledTaskKind::ChartPackage { family },
            });
            for region in Region::ALL {
                pending_tasks.push(ScheduledTask {
                    id: format!(
                        "charts-{}-unpack-{}",
                        family_id,
                        region.code().to_ascii_lowercase()
                    ),
                    deps: vec![format!("charts-{family_id}-package")],
                    weight: 1,
                    kind: ScheduledTaskKind::ChartUnpack { family, region },
                });
            }
        }
        pending_tasks.push(ScheduledTask {
            id: "csup-stage".to_string(),
            deps: vec![],
            weight: 1,
            kind: ScheduledTaskKind::CsupStage,
        });
        let mut csup_render_ids = Vec::new();
        for region in Region::ALL {
            let region_id = region.code().to_ascii_lowercase();
            let task_id = format!("csup-render-{region_id}");
            csup_render_ids.push(task_id.clone());
            pending_tasks.push(ScheduledTask {
                id: task_id,
                deps: vec!["csup-stage".to_string()],
                weight: 2,
                kind: ScheduledTaskKind::CsupRender { region },
            });
        }
        pending_tasks.push(ScheduledTask {
            id: "csup-package".to_string(),
            deps: csup_render_ids.clone(),
            weight: 1,
            kind: ScheduledTaskKind::CsupPackage,
        });
        for region in Region::ALL {
            pending_tasks.push(ScheduledTask {
                id: format!("csup-unpack-{}", region.code().to_ascii_lowercase()),
                deps: vec!["csup-package".to_string()],
                weight: 1,
                kind: ScheduledTaskKind::CsupUnpack { region },
            });
        }
        let mut tpp_package_ids = Vec::new();
        for region in config.profile.tpp_regions() {
            let region_id = region.code().to_ascii_lowercase();
            let render_id = format!("tpp-{region_id}");
            let package_id = format!("tpp-{region_id}-package");
            pending_tasks.push(ScheduledTask {
                id: render_id.clone(),
                deps: vec![],
                weight: TPP_RENDER_WEIGHT,
                kind: ScheduledTaskKind::TppRender { region: *region },
            });
            pending_tasks.push(ScheduledTask {
                id: package_id.clone(),
                deps: vec![render_id],
                weight: 1,
                kind: ScheduledTaskKind::TppPackage { region: *region },
            });
            pending_tasks.push(ScheduledTask {
                id: format!("tpp-{region_id}-unpack"),
                deps: vec![package_id.clone()],
                weight: 1,
                kind: ScheduledTaskKind::TppUnpack { region: *region },
            });
            tpp_package_ids.push(package_id);
        }
        pending_tasks.push(ScheduledTask {
            id: "data-base".to_string(),
            deps: vec![],
            weight: 4,
            kind: ScheduledTaskKind::DataBase,
        });
        pending_tasks.push(ScheduledTask {
            id: "data".to_string(),
            deps: {
                let mut deps = vec!["data-base".to_string()];
                deps.extend(tpp_package_ids.iter().cloned());
                deps
            },
            weight: 1,
            kind: ScheduledTaskKind::DataMatch,
        });
        pending_tasks.push(ScheduledTask {
            id: "vectors".to_string(),
            deps: vec!["data".to_string()],
            weight: 1,
            kind: ScheduledTaskKind::Vectors,
        });
        pending_tasks.push(ScheduledTask {
            id: "data-unpack".to_string(),
            deps: vec!["data".to_string()],
            weight: 1,
            kind: ScheduledTaskKind::DataUnpack,
        });
        pending_tasks.push(ScheduledTask {
            id: "vectors-unpack".to_string(),
            deps: vec!["vectors".to_string()],
            weight: 1,
            kind: ScheduledTaskKind::VectorsUnpack,
        });
        let mut resource_index_deps = chart_families
            .iter()
            .map(|family| format!("charts-{}-package", family_slug(*family)))
            .collect::<Vec<_>>();
        resource_index_deps.push("csup-package".to_string());
        resource_index_deps.extend(tpp_package_ids.iter().cloned());
        resource_index_deps.push("data".to_string());
        pending_tasks.push(ScheduledTask {
            id: "resource-index".to_string(),
            deps: resource_index_deps,
            weight: 2,
            kind: ScheduledTaskKind::ResourceIndex,
        });

        let total_tasks = pending_tasks.len();
        master_log.log(format!(
            "scheduler-ready tasks={} work_unit_budget={} chart_and_data_weight=4 csup_weight=2 tpp_weight={} tpp_render_jobs_per_run={} light_weight=1 resource_index_weight=2",
            total_tasks, work_unit_budget, TPP_RENDER_WEIGHT, TPP_RENDER_JOBS_PER_RUN
        ))?;
        let (tx, rx) =
            crossbeam_channel::unbounded::<(String, usize, anyhow::Result<TaskCompletion>)>();
        let mut running_jobs = 0_usize;
        let mut running_units = 0_usize;
        let mut launched_tasks = 0_usize;
        let mut completed_tasks = 0_usize;
        let mut completed_ids = std::collections::BTreeSet::<String>::new();
        let mut task_values = BTreeMap::<String, TaskValue>::new();
        let mut worker_threads = BTreeMap::<String, thread::JoinHandle<anyhow::Result<()>>>::new();

        while running_jobs > 0 || !pending_tasks.is_empty() {
            let mut launched_any = false;
            let mut index = 0_usize;
            while index < pending_tasks.len() {
                let task = &pending_tasks[index];
                let deps_ready = task.deps.iter().all(|dep| completed_ids.contains(dep));
                let fits_budget = running_units + task.weight <= work_unit_budget;
                if !deps_ready || !fits_budget {
                    index += 1;
                    continue;
                }

                let task = pending_tasks.remove(index);
                let task_id = task.id.clone();
                let task_weight = task.weight;
                launched_tasks += 1;
                master_log.log(format!(
                    "launch {} launched={}/{} completed={}/{} weight={} running_units={}/{}",
                    task_id,
                    launched_tasks,
                    total_tasks,
                    completed_tasks,
                    total_tasks,
                    task_weight,
                    running_units + task_weight,
                    work_unit_budget,
                ))?;
                let tx = tx.clone();
                let config = config.clone();
                let source_urls_dir = source_urls_dir.clone();
                let chart_versions = chart_versions.clone();
                let csup_version = csup_version.clone();
                let tpp_versions = tpp_versions.clone();
                let data_version = data_version.clone();
                let bundle_cycle = bundle_cycle.clone();
                let task_values_snapshot = task_values.clone();
                let worker_task_id = task_id.clone();
                let join_handle = thread::spawn(move || -> anyhow::Result<()> {
                    let task_label = worker_task_id.clone();
                    let completion_guard =
                        TaskCompletionGuard::new(tx, worker_task_id.clone(), task_weight);
                    let result = panic::catch_unwind(AssertUnwindSafe(|| match task.kind {
                        ScheduledTaskKind::ChartRender { family } => {
                            let family_id = family_slug(family).to_string();
                            let record = build_chart_render_node(
                                &config,
                                family,
                                &config.chart_cutline_root,
                                &source_urls_dir
                                    .join(format!("charts-{family_id}/source_urls.jsonl")),
                                config.fetch_jobs,
                                config.cpu_jobs.min(8).max(1),
                            )
                            .map(|record| TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            });
                            record
                        }
                        ScheduledTaskKind::CsupStage => {
                            let record = build_csup_stage_node(
                                &config,
                                Path::new(""),
                                &source_urls_dir.join("csup/source_urls.jsonl"),
                                config.fetch_jobs,
                            )
                            .and_then(|record| {
                                let work_dir = resolve_artifact_path(
                                    &config,
                                    output_path(&record, "work_dir")?,
                                );
                                Ok(TaskCompletion {
                                    node_records: vec![record.clone()],
                                    value: TaskValue::CsupStage { record, work_dir },
                                    completion_detail: "cache_or_rebuild".to_string(),
                                })
                            });
                            record
                        }
                        ScheduledTaskKind::CsupRender { region } => {
                            let stage = match task_values_snapshot.get("csup-stage") {
                                Some(TaskValue::CsupStage { record, work_dir }) => {
                                    (record, work_dir)
                                }
                                _ => unreachable!("csup-stage dependency should have completed"),
                            };
                            build_csup_render_node(
                                &config,
                                region,
                                stage.1,
                                &stage.0.fingerprint,
                                &csup_version,
                                config.cpu_jobs.max(1),
                            )
                            .map(|record| TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ScheduledTaskKind::TppRender { region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let request = NativeTppRunRequest {
                                region,
                                source_repo: PathBuf::new(),
                                run_root: PathBuf::new(),
                                prefetch_source_urls: Some(
                                    source_urls_dir
                                        .join(format!("tpp-{region_id}/source_urls.jsonl")),
                                ),
                                fetch_jobs: config.fetch_jobs,
                                render_jobs: TPP_RENDER_JOBS_PER_RUN,
                                fetch_cache: Some(static_source_fetch_cache_config(&config)?),
                            };
                            build_tpp_render_node(&config, &request).map(|record| TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ScheduledTaskKind::DataBase => build_data_nodes(
                            &config,
                            &source_urls_dir,
                            "data-base",
                        )
                        .and_then(|records| {
                            let data_record = records
                                .iter()
                                .find(|record| record.name == "data-base")
                                .cloned()
                                .context("data-base task missing data node record")?;
                            let staging_record = records
                                .iter()
                                .find(|record| record.name == "data-input-staging")
                                .cloned()
                                .context("data-base task missing data input staging node record")?;
                            let zip =
                                resolve_artifact_path(&config, output_path(&data_record, "zip")?);
                            let intermediate_sqlite_db =
                                resolve_artifact_path(&config, sqlite_output_path(&data_record)?);
                            let source_input_dir = resolve_artifact_path(
                                &config,
                                output_path(&staging_record, "staged_input_dir")?,
                            );
                            Ok(TaskCompletion {
                                node_records: records,
                                value: TaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    zip,
                                    fingerprint: data_record.fingerprint,
                                },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }),
                        ScheduledTaskKind::DataMatch => {
                            let raw_data = match task_values_snapshot.get("data-base") {
                                Some(TaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    zip,
                                    fingerprint,
                                }) => (
                                    intermediate_sqlite_db.clone(),
                                    source_input_dir.clone(),
                                    zip.clone(),
                                    fingerprint.clone(),
                                ),
                                _ => unreachable!("data-base dependency should have completed"),
                            };
                            let tpp_sources = config
                                .profile
                                .tpp_regions()
                                .iter()
                                .map(|region| {
                                    let region_id = region.code().to_ascii_lowercase();
                                    let key = format!("tpp-{region_id}-package");
                                    match task_values_snapshot.get(&key) {
                                        Some(TaskValue::FingerprintedTppSource {
                                            source,
                                            fingerprint,
                                        }) => Ok((*region, source.clone(), fingerprint.clone())),
                                        _ => bail!("missing tpp package source for {region_id}"),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let record = build_data_match_node(
                                &config,
                                &raw_data.0,
                                &raw_data.2,
                                &data_version,
                                &raw_data.3,
                                &tpp_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&config, output_path(&record, "zip")?);
                            let intermediate_sqlite_db =
                                resolve_artifact_path(&config, sqlite_output_path(&record)?);
                            let fingerprint = record.fingerprint.clone();
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir: raw_data.1,
                                    zip,
                                    fingerprint,
                                },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ScheduledTaskKind::ChartPackage { family } => {
                            let family_id = family_slug(family).to_string();
                            let started = Instant::now();
                            let (records, source) = build_chart_package_nodes(
                                &config,
                                family,
                                &source_urls_dir,
                                chart_versions
                                    .get(&family_id)
                                    .expect("chart family version should exist"),
                            )?;
                            let summary = summarize_package_records(&records);
                            Ok(TaskCompletion {
                                node_records: records,
                                value: TaskValue::ChartSource(source),
                                completion_detail: format!(
                                    "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                    started.elapsed().as_millis(),
                                    summary.total,
                                    summary.cache_hits,
                                    summary.rebuilt,
                                ),
                            })
                        }
                        ScheduledTaskKind::CsupPackage => {
                            let started = Instant::now();
                            let (records, source) =
                                build_csup_package_nodes(&config, &source_urls_dir, &csup_version)?;
                            let summary = summarize_package_records(&records);
                            Ok(TaskCompletion {
                                node_records: records,
                                value: TaskValue::CsupSource(source),
                                completion_detail: format!(
                                    "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                    started.elapsed().as_millis(),
                                    summary.total,
                                    summary.cache_hits,
                                    summary.rebuilt,
                                ),
                            })
                        }
                        ScheduledTaskKind::TppPackage { region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let started = Instant::now();
                            let (record, source) = build_tpp_package_node(
                                &config,
                                region,
                                &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                                tpp_versions
                                    .get(&region_id)
                                    .expect("tpp region version should exist"),
                            )?;
                            let cache_hit = record.cache_hit;
                            let fingerprint = record.fingerprint.clone();
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::FingerprintedTppSource {
                                    source,
                                    fingerprint,
                                },
                                completion_detail: format!(
                                    "elapsed_ms={} cache_hit={}",
                                    started.elapsed().as_millis(),
                                    cache_hit,
                                ),
                            })
                        }
                        ScheduledTaskKind::Vectors => {
                            let (data, source_input_dir, data_fingerprint) =
                                match task_values_snapshot.get("data") {
                                    Some(TaskValue::FingerprintedData {
                                        intermediate_sqlite_db,
                                        source_input_dir,
                                        fingerprint,
                                        ..
                                    }) => (intermediate_sqlite_db, source_input_dir, fingerprint),
                                    _ => unreachable!("data dependency should have completed"),
                                };
                            let record = build_vectors_node(
                                &config,
                                data,
                                source_input_dir,
                                data_fingerprint,
                                &data_version,
                            )?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&config, output_path(&record, "zip")?);
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::FingerprintedZip { zip },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ScheduledTaskKind::ResourceIndex => {
                            let data_zip = match task_values_snapshot.get("data") {
                                Some(TaskValue::FingerprintedData {
                                    intermediate_sqlite_db: _,
                                    zip,
                                    ..
                                }) => zip.clone(),
                                _ => unreachable!("data dependency should have completed"),
                            };
                            let chart_sources = ["sec", "tac", "enr-l", "enr-h"]
                                .iter()
                                .map(|family_id| {
                                    let key = format!("charts-{family_id}-package");
                                    match task_values_snapshot.get(&key) {
                                        Some(TaskValue::ChartSource(source)) => Ok(source.clone()),
                                        _ => bail!("missing chart source for {family_id}"),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let csup_sources =
                                vec![match task_values_snapshot.get("csup-package") {
                                    Some(TaskValue::CsupSource(source)) => source.clone(),
                                    _ => bail!("missing csup package source"),
                                }];
                            let tpp_sources = config
                                .profile
                                .tpp_regions()
                                .iter()
                                .map(|region| {
                                    let region_id = region.code().to_ascii_lowercase();
                                    let key = format!("tpp-{region_id}-package");
                                    match task_values_snapshot.get(&key) {
                                        Some(TaskValue::FingerprintedTppSource {
                                            source, ..
                                        }) => Ok(source.clone()),
                                        _ => bail!("missing tpp package source for {region_id}"),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let record = build_resource_index_node(
                                &config,
                                &data_zip,
                                chart_sources,
                                tpp_sources,
                                csup_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::None,
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ScheduledTaskKind::ChartUnpack { family, region } => {
                            let family_id = family_slug(family).to_string();
                            let key = format!("charts-{family_id}-package");
                            let source = match task_values_snapshot.get(&key) {
                                Some(TaskValue::ChartSource(source)) => source.clone(),
                                _ => bail!("missing chart source for {family_id}"),
                            };
                            let package =
                                package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config)?;
                            let published_filename = canonical_package_filename(
                                &family_id,
                                &region.code().to_ascii_lowercase(),
                                &package.zip,
                            )?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip_path,
                                &source.package_root,
                                &unpacked_root,
                                &published_filename,
                                Some(&package.zip_sha256),
                            )?;
                            Ok(TaskCompletion {
                                node_records: vec![],
                                value: TaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ScheduledTaskKind::CsupUnpack { region } => {
                            let source = match task_values_snapshot.get("csup-package") {
                                Some(TaskValue::CsupSource(source)) => source.clone(),
                                _ => bail!("missing csup package source"),
                            };
                            let package =
                                package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config)?;
                            let published_filename = canonical_package_filename(
                                "csup",
                                &region.code().to_ascii_lowercase(),
                                &package.zip,
                            )?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip_path,
                                &source.package_root,
                                &unpacked_root,
                                &published_filename,
                                Some(&package.zip_sha256),
                            )?;
                            Ok(TaskCompletion {
                                node_records: vec![],
                                value: TaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ScheduledTaskKind::TppUnpack { region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let key = format!("tpp-{region_id}-package");
                            let source = match task_values_snapshot.get(&key) {
                                Some(TaskValue::FingerprintedTppSource { source, .. }) => {
                                    source.clone()
                                }
                                _ => bail!("missing tpp package source for {region_id}"),
                            };
                            let package =
                                package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config)?;
                            let published_filename = canonical_package_filename(
                                "tpp",
                                &region.code().to_ascii_lowercase(),
                                &package.zip,
                            )?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip_path,
                                &source.package_root,
                                &unpacked_root,
                                &published_filename,
                                Some(&package.zip_sha256),
                            )?;
                            Ok(TaskCompletion {
                                node_records: vec![],
                                value: TaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ScheduledTaskKind::DataUnpack => {
                            let zip = match task_values_snapshot.get("data") {
                                Some(TaskValue::FingerprintedData {
                                    intermediate_sqlite_db: _,
                                    zip,
                                    ..
                                }) => zip.clone(),
                                _ => bail!("missing data zip"),
                            };
                            let unpacked_root = published_unpacked_root(&config)?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip,
                                zip.parent().unwrap_or_else(|| Path::new("/")),
                                &unpacked_root,
                                &format!("data_{bundle_cycle}.zip"),
                                None,
                            )?;
                            Ok(TaskCompletion {
                                node_records: vec![],
                                value: TaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ScheduledTaskKind::VectorsUnpack => {
                            let zip = match task_values_snapshot.get("vectors") {
                                Some(TaskValue::FingerprintedZip { zip, .. }) => zip.clone(),
                                _ => bail!("missing vectors zip"),
                            };
                            let unpacked_root = published_unpacked_root(&config)?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip,
                                zip.parent().unwrap_or_else(|| Path::new("/")),
                                &unpacked_root,
                                &format!("vectors_data_{bundle_cycle}.zip"),
                                None,
                            )?;
                            Ok(TaskCompletion {
                                node_records: vec![],
                                value: TaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                    }))
                    .unwrap_or_else(|panic_payload| {
                        let panic_text = if let Some(text) = panic_payload.downcast_ref::<&str>() {
                            (*text).to_string()
                        } else if let Some(text) = panic_payload.downcast_ref::<String>() {
                            text.clone()
                        } else {
                            "unknown panic payload".to_string()
                        };
                        Err(anyhow::anyhow!(
                            "task thread panicked: {task_label}: {panic_text}"
                        ))
                    });
                    completion_guard.send(result);
                    Ok(())
                });
                worker_threads.insert(task_id.clone(), join_handle);
                running_jobs += 1;
                running_units += task_weight;
                launched_any = true;
            }

            if running_jobs == 0 {
                if pending_tasks.is_empty() {
                    break;
                }
                bail!("scheduler deadlock: no runnable tasks remain");
            }
            if !launched_any {
                // wait for a running task to free capacity or satisfy dependencies
            }

            let (task_id, task_weight, result) = loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(message) => break message,
                    Err(RecvTimeoutError::Timeout) => {
                        let finished_count = worker_threads
                            .values()
                            .filter(|handle| handle.is_finished())
                            .count();
                        master_log.log(format!(
                            "scheduler-wait running_jobs={} worker_threads={} finished_threads={} pending_tasks={} running_units={}/{}",
                            running_jobs,
                            worker_threads.len(),
                            finished_count,
                            pending_tasks.len(),
                            running_units,
                            work_unit_budget,
                        ))?;
                        if running_jobs > 0 && worker_threads.is_empty() {
                            bail!(
                                "scheduler invariant violated: running_jobs={} but no worker threads remain",
                                running_jobs
                            );
                        }
                        if running_jobs > 0
                            && !worker_threads.is_empty()
                            && finished_count == worker_threads.len()
                        {
                            bail!(
                                "scheduler invariant violated: all {} worker threads are finished but no completion messages arrived",
                                worker_threads.len()
                            );
                        }
                        let finished_threads = worker_threads
                            .iter()
                            .filter(|(_, handle)| handle.is_finished())
                            .map(|(task_id, _)| task_id.clone())
                            .collect::<Vec<_>>();
                        for finished_task_id in finished_threads {
                            let join_result = worker_threads
                                .remove(&finished_task_id)
                                .expect("finished worker thread handle should exist")
                                .join();
                            match join_result {
                                Ok(Ok(())) => {}
                                Ok(Err(err)) => {
                                    master_log.log(format!(
                                        "complete {finished_task_id} FAIL error=worker thread join returned error: {err}"
                                    ))?;
                                    return Err(err);
                                }
                                Err(panic_payload) => {
                                    let panic_text =
                                        if let Some(text) = panic_payload.downcast_ref::<&str>() {
                                            (*text).to_string()
                                        } else if let Some(text) =
                                            panic_payload.downcast_ref::<String>()
                                        {
                                            text.clone()
                                        } else {
                                            "unknown panic payload".to_string()
                                        };
                                    let err = anyhow::anyhow!(
                                        "worker thread join observed panic: {finished_task_id}: {panic_text}"
                                    );
                                    master_log.log(format!(
                                        "complete {finished_task_id} FAIL error={err}"
                                    ))?;
                                    return Err(err);
                                }
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        bail!("scheduler channel closed unexpectedly");
                    }
                }
            };
            running_jobs -= 1;
            running_units = running_units.saturating_sub(task_weight);
            if let Some(handle) = worker_threads.remove(&task_id) {
                match handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        master_log.log(format!(
                            "complete {task_id} FAIL error=worker thread join returned error: {err}"
                        ))?;
                        return Err(err);
                    }
                    Err(panic_payload) => {
                        let panic_text = if let Some(text) = panic_payload.downcast_ref::<&str>() {
                            (*text).to_string()
                        } else if let Some(text) = panic_payload.downcast_ref::<String>() {
                            text.clone()
                        } else {
                            "unknown panic payload".to_string()
                        };
                        let err = anyhow::anyhow!(
                            "worker thread join observed panic: {task_id}: {panic_text}"
                        );
                        master_log.log(format!("complete {task_id} FAIL error={err}"))?;
                        return Err(err);
                    }
                }
            }
            match result {
                Ok(completion) => {
                    completed_tasks += 1;
                    for record in completion.node_records {
                        node_records.push(normalize_node_record_paths(record, &config.build_root));
                    }
                    completed_ids.insert(task_id.clone());
                    task_values.insert(task_id.clone(), completion.value);
                    master_log.log(format!(
                        "complete {} completed={}/{} running_units={}/{} {}",
                        task_id,
                        completed_tasks,
                        total_tasks,
                        running_units,
                        work_unit_budget,
                        completion.completion_detail,
                    ))?;
                }
                Err(err) => {
                    completed_ids.insert(task_id.clone());
                    master_log.log(format!("complete {task_id} FAIL error={err}"))?;
                    return Err(err);
                }
            }
        }

        node_records.sort_by(|left, right| left.name.cmp(&right.name));
        node_records.sort_by(|left, right| left.name.cmp(&right.name));

        let build_manifest = BuildManifest {
            schema_version: 1,
            profile: config.profile.as_str().to_string(),
            cycle: bundle_cycle.clone(),
            build_root: relative_product_build_path(&config.build_root),
            generated_at_utc: manifest_generated_at(&node_records),
            fetch_cache_root: relative_artifact_path(&config.fetch_cache_root, &config.build_root),
            fetch_cache_mode: config.fetch_cache_mode.clone(),
            nodes: node_records,
        };
        let build_manifest_path = internal_build_manifest_path(config, &bundle_cycle)?;
        fs::write(
            &build_manifest_path,
            serde_json::to_vec_pretty(&build_manifest)
                .context("failed to encode product build manifest")?,
        )
        .with_context(|| format!("failed to write {}", build_manifest_path.display()))?;

        let resource_index_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "resource-index")
            .context("build manifest missing resource-index node")?;
        let data_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "data")
            .context("build manifest missing data node")?;
        let vectors_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "vectors")
            .context("build manifest missing vectors node")?;
        let resource_index_path = resolve_artifact_path(
            config,
            output_path(resource_index_record, "resource_index")?,
        );
        let intermediate_sqlite_db =
            resolve_artifact_path(config, sqlite_output_path(data_record)?);
        let vectors_zip_path = resolve_artifact_path(config, output_path(vectors_record, "zip")?);
        let vectors_sha256 = output_sha_or_hash(vectors_record, "zip", &vectors_zip_path)?;
        let vectors_filename =
            format!("vectors_data_{bundle_cycle}_{PACKAGE_CYCLE_VERSION}_{vectors_sha256}.zip");
        publish_flat_artifact(
            &vectors_zip_path,
            &config.build_root.join(&vectors_filename),
        )?;
        let resource_index: ResourceIndex = serde_json::from_slice(
            &fs::read(&resource_index_path)
                .with_context(|| format!("failed to read {}", resource_index_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", resource_index_path.display()))?;
        let start_valid = resource_index
            .temporal_summary
            .uniform_good_beyond_date
            .clone()
            .or_else(|| {
                resource_index
                    .temporal_summary
                    .uniform_effective_date
                    .clone()
            })
            .context("resource-index missing start-valid date")?;
        let end_valid = resource_index
            .temporal_summary
            .uniform_expiration_date
            .clone()
            .or_else(|| {
                resource_index
                    .temporal_summary
                    .expiration_dates
                    .first()
                    .cloned()
            })
            .context("resource-index missing end-valid date")?;
        let vectors_package = BundlePackageArtifact {
            id: format!("VECTORS_DATA_{bundle_cycle}_{PACKAGE_CYCLE_VERSION}"),
            family_id: "vectors".to_string(),
            region_id: None,
            filename: vectors_filename.clone(),
            relative_path: vectors_filename,
            cycle: Some(bundle_cycle.clone()),
            cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
            checksum_sha256: vectors_sha256,
            size_bytes: fs::metadata(&vectors_zip_path)
                .with_context(|| format!("failed to stat {}", vectors_zip_path.display()))?
                .len(),
            published_at_utc: None,
            source_generated_at_utc: None,
            source_version: None,
            source_fetched_at_utc: None,
            effective_date: Some(start_valid),
            expiration_date: Some(end_valid),
            metadata: BTreeMap::new(),
        };
        let nav_db = build_nav_kv_artifact(
            config,
            &resource_index_path,
            &intermediate_sqlite_db,
            &bundle_cycle,
            &vectors_package,
            &[],
            &[],
        )?;
        let bundle_manifest = build_bundle_manifest(config, &build_manifest, &[], &nav_db.package)?;
        let bundle_manifest_path =
            write_hashed_bundle_manifest(&config.build_root, &bundle_manifest)?;
        validate_bundle_manifest(&config.build_root, &bundle_manifest_path)?;
        sync_unpacked_metadata(config, &bundle_manifest, &bundle_manifest_path, None)?;
        record_gc_roots_from_build_manifest(
            config,
            &format!("cycle:{bundle_cycle}"),
            &build_manifest,
        )?;
        Ok(bundle_manifest_path)
    })();

    match result {
        Ok(manifest_path) => {
            master_log.log(format!(
                "complete PASS manifest={}",
                manifest_path.display()
            ))?;
            Ok(manifest_path)
        }
        Err(err) => {
            master_log.log(format!("complete FAIL error={err}"))?;
            Err(err)
        }
    }
}

fn collect_shaded_relief_tile_levels(
    task_values: &BTreeMap<String, ProductTaskValue>,
    config: &ProductBuildConfig,
) -> anyhow::Result<Vec<(Region, Vec<TileLevelRecord>)>> {
    config
        .profile
        .terrain_regions()
        .iter()
        .map(|region| {
            let region_id = region.code().to_ascii_lowercase();
            let task_id = format!("build-shaded-relief-{region_id}");
            let tile_levels = match task_values.get(&task_id) {
                Some(ProductTaskValue::BuiltStaticTileProduct { tile_levels, .. }) => {
                    tile_levels.clone()
                }
                _ => bail!("missing shaded relief build output for {}", region.code()),
            };
            Ok((*region, tile_levels))
        })
        .collect()
}

fn build_bundle_manifest(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    stable_packages: &[BundlePackageArtifact],
    nav_db_package: &BundlePackageArtifact,
) -> anyhow::Result<BundleManifest> {
    let resource_index_record = build_manifest
        .nodes
        .iter()
        .find(|node| node.name == "resource-index")
        .context("build manifest missing resource-index node")?;
    let vectors_record = build_manifest
        .nodes
        .iter()
        .find(|node| node.name == "vectors")
        .context("build manifest missing vectors node")?;

    let resource_index_path = resolve_artifact_path(
        config,
        output_path(resource_index_record, "resource_index")?,
    );
    let vectors_zip_path = resolve_artifact_path(config, output_path(vectors_record, "zip")?);
    let index: ResourceIndex = serde_json::from_slice(
        &fs::read(&resource_index_path)
            .with_context(|| format!("failed to read {}", resource_index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", resource_index_path.display()))?;
    let start_valid = index
        .temporal_summary
        .uniform_good_beyond_date
        .clone()
        .or_else(|| index.temporal_summary.uniform_effective_date.clone())
        .context("resource-index missing start-valid date")?;
    let end_valid = index
        .temporal_summary
        .uniform_expiration_date
        .clone()
        .or_else(|| index.temporal_summary.expiration_dates.first().cloned())
        .context("resource-index missing end-valid date")?;
    let cycle = build_manifest.cycle.clone();
    let vectors_sha256 = output_sha_or_hash(vectors_record, "zip", &vectors_zip_path)?;
    let vectors_filename =
        format!("vectors_data_{cycle}_{PACKAGE_CYCLE_VERSION}_{vectors_sha256}.zip");
    publish_flat_artifact(
        &vectors_zip_path,
        &config.build_root.join(&vectors_filename),
    )?;

    let mut package_artifacts = index
        .packages
        .iter()
        .map(|package| {
            let package_path = resolve_bundle_package_source_path(config, build_manifest, package)?;
            let filename = canonical_package_filename_hashed(
                &package.family_id,
                &package.region_id,
                Path::new(&package_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
                &package.checksum_sha256,
            )?;
            publish_flat_artifact(&package_path, &config.build_root.join(&filename))?;
            Ok(BundlePackageArtifact {
                id: package.id.clone(),
                family_id: package.family_id.clone(),
                region_id: Some(package.region_id.clone()),
                filename: filename.clone(),
                relative_path: filename,
                cycle: package_version_from_filename(
                    Path::new(&package_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default(),
                )
                .ok(),
                cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
                checksum_sha256: package.checksum_sha256.clone(),
                size_bytes: fs::metadata(&package_path)
                    .with_context(|| format!("failed to stat {}", package_path.display()))?
                    .len(),
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: package.effective_date.clone(),
                expiration_date: package.expiration_date.clone(),
                metadata: package.metadata.clone(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    package_artifacts.push(BundlePackageArtifact {
        id: format!("VECTORS_DATA_{cycle}_{PACKAGE_CYCLE_VERSION}"),
        family_id: "vectors".to_string(),
        region_id: None,
        filename: vectors_filename.clone(),
        relative_path: vectors_filename.clone(),
        cycle: Some(cycle.clone()),
        cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
        checksum_sha256: vectors_sha256.clone(),
        size_bytes: fs::metadata(&vectors_zip_path)
            .with_context(|| format!("failed to stat {}", vectors_zip_path.display()))?
            .len(),
        published_at_utc: None,
        source_generated_at_utc: None,
        source_version: None,
        source_fetched_at_utc: None,
        effective_date: Some(start_valid.clone()),
        expiration_date: Some(end_valid.clone()),
        metadata: BTreeMap::new(),
    });
    package_artifacts.extend(stable_packages.iter().cloned());
    package_artifacts.push(nav_db_package.clone());

    let ancillary = vec![];

    Ok(BundleManifest {
        schema_version: 2,
        bundle_id: format!("cycle_{cycle}_{PACKAGE_CYCLE_VERSION}"),
        bundle_type: "cycle".to_string(),
        cycle: cycle.clone(),
        cycle_version: PACKAGE_CYCLE_VERSION.to_string(),
        generated_at_utc: build_manifest.generated_at_utc.clone(),
        effective_date: start_valid.clone(),
        expiration_date: end_valid.clone(),
        start_valid: start_valid.clone(),
        end_valid: end_valid.clone(),
        packages: package_artifacts,
        ancillary,
    })
}

fn build_nav_kv_artifact(
    config: &ProductBuildConfig,
    resource_index_path: &Path,
    intermediate_sqlite_db_path: &Path,
    cycle: &str,
    vectors_package: &BundlePackageArtifact,
    stable_packages: &[BundlePackageArtifact],
    shaded_relief_tile_levels: &[(Region, Vec<TileLevelRecord>)],
) -> anyhow::Result<BuiltNavDbArtifacts> {
    let resource_index: ResourceIndex = serde_json::from_slice(
        &fs::read(resource_index_path)
            .with_context(|| format!("failed to read {}", resource_index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", resource_index_path.display()))?;
    let mut package_artifacts = bundle_package_artifacts_from_resource_index(&resource_index)?;
    package_artifacts.push(vectors_package.clone());
    package_artifacts.extend(stable_packages.iter().cloned());
    let package_index_json = serde_json::to_string(&package_artifacts)
        .context("failed to encode nav-db package inputs")?;
    let shaded_relief_json = shaded_relief_tile_levels
        .iter()
        .map(|(region, levels)| {
            format!(
                "{}:{}",
                region.code().to_ascii_lowercase(),
                serde_json::to_string(levels).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let inputs = BTreeMap::from([
        (
            "resource_index".to_string(),
            hash_file(resource_index_path)?,
        ),
        (
            "intermediate_sqlite_db".to_string(),
            hash_file(intermediate_sqlite_db_path)?,
        ),
        ("cycle".to_string(), cycle.to_string()),
        (
            "package_artifacts".to_string(),
            hash_text(&package_index_json),
        ),
        (
            "shaded_relief_tile_levels".to_string(),
            hash_text(&shaded_relief_json),
        ),
        ("nav_kv_page_bytes".to_string(), (64 * 1024).to_string()),
        (
            "nav_kv_builder".to_string(),
            hash_file(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/product_build.rs"))?,
        ),
    ]);
    let prepared = prepare_node_at(&build_shared_node_dir(config, "nav-db")?, "nav-db", &inputs)?;
    let output_dir = prepared.dir.join("output");
    let source_dir = output_dir.join("nav_db");
    let root_filename = "root";
    let nav_db_zip_source_path = output_dir.join(format!("nav_db_{cycle}.zip"));
    let record =
        match claim_or_wait_for_node(&prepared, std::slice::from_ref(&nav_db_zip_source_path))? {
            NodeCacheState::CacheHit(record) => record,
            NodeCacheState::Build(_lock) => {
                if output_dir.exists() {
                    fs::remove_dir_all(&output_dir)
                        .with_context(|| format!("failed to remove {}", output_dir.display()))?;
                }
                fs::create_dir_all(&source_dir)
                    .with_context(|| format!("failed to create {}", source_dir.display()))?;
                let started_at_utc = utc_now_string();
                let started = Instant::now();
                let chart_coverage_polygon_sets =
                    build_chart_coverage_polygon_sets(&config.chart_cutline_root, &resource_index)?;
                let chart_catalog = build_nav_kv_chart_catalog(
                    &resource_index,
                    shaded_relief_tile_levels,
                    &chart_coverage_polygon_sets,
                );
                let chart_catalog_bytes = serde_json::to_vec(&chart_catalog)
                    .context("failed to encode nav_kv chart/catalog value")?;
                let mut pairs = vec![NavKvPair {
                    key: "chart/catalog".to_string(),
                    value: chart_catalog_bytes,
                }];
                pairs.extend(build_nav_kv_chart_coverage_pairs(
                    &chart_coverage_polygon_sets,
                )?);
                pairs.extend(build_nav_kv_resource_summary_pairs(&resource_index)?);
                pairs.extend(build_nav_kv_plate_pairs(&resource_index)?);
                pairs.extend(build_nav_kv_package_pairs(&package_artifacts)?);
                pairs.extend(build_nav_kv_navref_pairs(intermediate_sqlite_db_path)?);
                let built = build_nav_kv_sorted(pairs, 64 * 1024)
                    .map_err(|err| anyhow::anyhow!("failed to build nav_kv: {err}"))?;
                let root_source_path = source_dir.join(root_filename);
                fs::write(&root_source_path, &built.root_bytes)
                    .with_context(|| format!("failed to write {}", root_source_path.display()))?;

                let mut page_filenames = Vec::new();
                for (index, page) in built.value_pages.iter().enumerate() {
                    let page_filename = format!("values_{index:04}");
                    let page_source_path = source_dir.join(&page_filename);
                    fs::write(&page_source_path, page).with_context(|| {
                        format!("failed to write {}", page_source_path.display())
                    })?;
                    page_filenames.push(page_filename);
                }
                let published_source_dir = artifact_root_from_build_root(&config.build_root)
                    .join("private-work")
                    .join("nav-kv")
                    .join(config.profile.as_str())
                    .join(cycle);
                if published_source_dir.exists() {
                    fs::remove_dir_all(&published_source_dir).with_context(|| {
                        format!("failed to remove {}", published_source_dir.display())
                    })?;
                }
                hardlink_dir_recursive(&source_dir, &published_source_dir)?;
                let mut zip_entries = vec![root_filename];
                let page_entry_names = page_filenames
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                zip_entries.extend(page_entry_names.iter().copied());
                zip_directory_deterministic(&nav_db_zip_source_path, &source_dir, &zip_entries)?;
                let outputs = BTreeMap::from([(
                    "nav_db_zip".to_string(),
                    relative_artifact_path(&nav_db_zip_source_path, &config.build_root),
                )]);
                write_node_record(
                    prepared,
                    inputs,
                    outputs,
                    false,
                    started_at_utc,
                    utc_now_string(),
                    started.elapsed().as_millis() as u64,
                )?
            }
        };
    let nav_db_sha256 = output_sha_or_hash(&record, "nav_db_zip", &nav_db_zip_source_path)?;
    let nav_db_published_filename =
        format!("nav_db_{cycle}_{PACKAGE_CYCLE_VERSION}_{nav_db_sha256}.zip");
    let nav_db_package_artifact =
        publish_bundle_artifact(config, &nav_db_zip_source_path, &nav_db_published_filename)?;
    Ok(BuiltNavDbArtifacts {
        node_record: record,
        package: BundlePackageArtifact {
            id: format!("NAV_DB_{cycle}_{PACKAGE_CYCLE_VERSION}"),
            family_id: "nav-db".to_string(),
            region_id: None,
            filename: nav_db_package_artifact.filename.clone(),
            relative_path: nav_db_package_artifact.relative_path.clone(),
            cycle: Some(cycle.to_string()),
            cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
            checksum_sha256: nav_db_package_artifact.checksum_sha256.clone(),
            size_bytes: nav_db_package_artifact.size_bytes,
            published_at_utc: None,
            source_generated_at_utc: None,
            source_version: None,
            source_fetched_at_utc: None,
            effective_date: resource_index
                .temporal_summary
                .uniform_good_beyond_date
                .clone()
                .or_else(|| {
                    resource_index
                        .temporal_summary
                        .uniform_effective_date
                        .clone()
                }),
            expiration_date: resource_index
                .temporal_summary
                .uniform_expiration_date
                .clone()
                .or_else(|| {
                    resource_index
                        .temporal_summary
                        .expiration_dates
                        .first()
                        .cloned()
                }),
            metadata: BTreeMap::new(),
        },
    })
}

fn bundle_package_artifacts_from_resource_index(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<BundlePackageArtifact>> {
    resource_index
        .packages
        .iter()
        .map(bundle_package_artifact_from_resource_package)
        .collect()
}

fn bundle_package_artifact_from_resource_package(
    package: &preprocessor_resource_index::ResourcePackage,
) -> anyhow::Result<BundlePackageArtifact> {
    let artifact_path = package
        .artifact_path
        .as_deref()
        .with_context(|| format!("package {} missing artifact_path", package.id))?;
    let source_filename = Path::new(artifact_path)
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("package {} artifact_path has no filename", package.id))?;
    let filename = canonical_package_filename_hashed(
        &package.family_id,
        &package.region_id,
        source_filename,
        &package.checksum_sha256,
    )?;
    Ok(BundlePackageArtifact {
        id: package.id.clone(),
        family_id: package.family_id.clone(),
        region_id: Some(package.region_id.clone()),
        filename: filename.clone(),
        relative_path: filename,
        cycle: package_version_from_filename(source_filename).ok(),
        cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
        checksum_sha256: package.checksum_sha256.clone(),
        size_bytes: package.size_bytes,
        published_at_utc: None,
        source_generated_at_utc: None,
        source_version: None,
        source_fetched_at_utc: None,
        effective_date: package.effective_date.clone(),
        expiration_date: package.expiration_date.clone(),
        metadata: package.metadata.clone(),
    })
}

fn build_nav_kv_chart_catalog(
    resource_index: &ResourceIndex,
    shaded_relief_tile_levels: &[(Region, Vec<TileLevelRecord>)],
    chart_coverage_polygon_sets: &BTreeMap<String, ChartCoveragePolygonSetRecord>,
) -> serde_json::Value {
    let mut collections = resource_index
        .chart_collections
        .iter()
        .filter(|collection| {
            matches!(
                collection.family_id.as_str(),
                "sec" | "tac" | "enr-l" | "enr-h"
            )
        })
        .map(|collection| {
            let levels = collection
                .levels
                .iter()
                .map(|level| {
                    serde_json::json!({
                        "zoom": level.zoom,
                        "x_min": level.x_min,
                        "x_max": level.x_max,
                        "y_tms_min": level.y_tms_min,
                        "y_tms_max": level.y_tms_max,
                    })
                })
                .collect::<Vec<_>>();
            let coverage = chart_coverage_polygon_sets
                .get(&collection.id)
                .map(|polygon_set| {
                    serde_json::json!({
                        "kind": "polygon_set_ref",
                        "value": {
                            "polygon_set_id": polygon_set.id,
                        },
                    })
                })
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "id": collection.id,
                "label": format!(
                    "{} {}",
                    region_display_name(resource_index, &collection.region_id),
                    family_display_name(resource_index, &collection.family_id),
                ),
                "region_id": collection.region_id,
                "coverage": coverage,
                "map_view": {
                    "chart_family": collection.family_id,
                    "chart_name": format!(
                        "{} {}",
                        region_display_name(resource_index, &collection.region_id),
                        family_display_name(resource_index, &collection.family_id),
                    ),
                    "chart_index": collection.chart_index,
                    "tile_root": "tiles",
                    "tile_url_root": format!("/sectional-packages/{}/tiles", collection.package_id),
                    "tile_path_template": collection.tile_path_template.strip_prefix("tiles/").unwrap_or(&collection.tile_path_template),
                    "tile_size": 512,
                    "min_zoom": min_zoom_for_levels(collection),
                    "max_zoom": max_zoom_for_levels(collection),
                    "storage_kind": "sectional_package",
                    "package_name": collection.package_id,
                    "initial_viewport": {
                        "lat": collection.default_view.lat,
                        "lon": collection.default_view.lon,
                        "zoom": collection.default_view.zoom,
                    },
                    "levels": levels,
                },
            })
        })
        .collect::<Vec<_>>();
    collections.extend(build_nav_kv_shaded_relief_catalog_entries(
        resource_index,
        shaded_relief_tile_levels,
    ));
    serde_json::Value::Array(collections)
}

fn build_nav_kv_chart_coverage_pairs(
    chart_coverage_polygon_sets: &BTreeMap<String, ChartCoveragePolygonSetRecord>,
) -> anyhow::Result<Vec<NavKvPair>> {
    chart_coverage_polygon_sets
        .values()
        .map(|polygon_set| {
            Ok(NavKvPair {
                key: format!(
                    "geometry/polygon-set/{}",
                    had_key_component(&polygon_set.id)
                ),
                value: serde_json::to_vec(polygon_set)
                    .context("failed to encode chart coverage polygon set")?,
            })
        })
        .collect()
}

fn build_nav_kv_shaded_relief_catalog_entries(
    resource_index: &ResourceIndex,
    shaded_relief_tile_levels: &[(Region, Vec<TileLevelRecord>)],
) -> Vec<serde_json::Value> {
    shaded_relief_tile_levels
        .iter()
        .map(|(region, tile_levels)| {
            let region_id = region.code().to_ascii_lowercase();
            let product_id = format!("shaded-relief-{region_id}");
            let region_display_name = region_display_name(resource_index, &region_id);
            let initial_viewport = default_view_for_static_region(resource_index, *region);
            let levels = tile_levels
                .iter()
                .map(|level| {
                    serde_json::json!({
                        "zoom": level.zoom,
                        "x_min": level.x_min,
                        "x_max": level.x_max,
                        "y_tms_min": level.y_tms_min,
                        "y_tms_max": level.y_tms_max,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "id": product_id,
                "label": format!("{region_display_name} Shaded Relief"),
                "region_id": region_id,
                "map_view": {
                    "chart_family": "shaded-relief",
                    "chart_name": format!("{region_display_name} Shaded Relief"),
                    "chart_index": 0,
                    "tile_root": "tiles",
                    "tile_url_root": format!("/shaded-relief-products/{product_id}/tiles"),
                    "tile_path_template": "0/{z}/{x}/{y}.webp",
                    "tile_size": TERRAIN_TILE_SIZE,
                    "min_zoom": TERRAIN_MIN_ZOOM,
                    "max_zoom": RASTER_BASEMAP_MAX_DISPLAY_ZOOM,
                    "storage_kind": "static_product",
                    "package_name": product_id,
                    "initial_viewport": {
                        "lat": initial_viewport.lat,
                        "lon": initial_viewport.lon,
                        "zoom": initial_viewport.zoom,
                    },
                    "levels": levels,
                },
            })
        })
        .collect()
}

fn build_chart_coverage_polygon_sets(
    chart_cutline_root: &Path,
    resource_index: &ResourceIndex,
) -> anyhow::Result<BTreeMap<String, ChartCoveragePolygonSetRecord>> {
    let mut sets = BTreeMap::new();
    for family_id in ["sec", "tac", "enr-l", "enr-h"] {
        let Some(cutline_dir_name) = chart_cutline_dir_name(family_id) else {
            continue;
        };
        let family_collections = resource_index
            .chart_collections
            .iter()
            .filter(|collection| collection.family_id == family_id)
            .collect::<Vec<_>>();
        if family_collections.is_empty() {
            continue;
        }
        let polygons = read_chart_cutline_polygons(&chart_cutline_root.join(cutline_dir_name))?;
        for cutline in polygons {
            for target_collection in
                collections_for_cutline_polygon(&cutline.points, &family_collections)
            {
                let polygon_set = sets.entry(target_collection.id.clone()).or_insert_with(|| {
                    ChartCoveragePolygonSetRecord {
                        schema_version: 1,
                        id: format!("chart-coverage:{}", target_collection.id),
                        polygons: Vec::new(),
                    }
                });
                let polygon_index = polygon_set.polygons.len();
                polygon_set.polygons.push(ChartCoveragePolygonRecord {
                    id: format!("{}:{}", polygon_set.id, polygon_index),
                    points: cutline.points.clone(),
                });
            }
        }
    }
    Ok(sets)
}

fn chart_cutline_dir_name(family_id: &str) -> Option<&'static str> {
    match family_id {
        "sec" => Some("SEC"),
        "tac" => Some("TAC"),
        "enr-l" => Some("ENR_L"),
        "enr-h" => Some("ENR_H"),
        _ => None,
    }
}

fn read_chart_cutline_polygons(dir: &Path) -> anyhow::Result<Vec<RawChartCutlinePolygon>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate {}", dir.display()))?;
    paths.sort();
    let mut polygons = Vec::new();
    for path in paths {
        if path.extension().and_then(|ext| ext.to_str()) != Some("geojson") {
            continue;
        }
        polygons.extend(read_chart_cutline_polygons_from_file(&path)?);
    }
    Ok(polygons)
}

fn read_chart_cutline_polygons_from_file(
    path: &Path,
) -> anyhow::Result<Vec<RawChartCutlinePolygon>> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let feature_values = match value.get("type").and_then(|value| value.as_str()) {
        Some("FeatureCollection") => value
            .get("features")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default(),
        Some("Feature") => vec![value],
        Some(other) => bail!(
            "unsupported geojson root type {other} in {}",
            path.display()
        ),
        None => bail!("geojson root missing type in {}", path.display()),
    };

    let mut polygons = Vec::new();
    for feature in feature_values {
        let geometry = feature
            .get("geometry")
            .context("geojson feature missing geometry")?;
        let geometry_type = geometry
            .get("type")
            .and_then(|value| value.as_str())
            .context("geojson geometry missing type")?;
        match geometry_type {
            "Polygon" => polygons.push(RawChartCutlinePolygon {
                points: polygon_points_from_geojson_coordinates(
                    geometry
                        .get("coordinates")
                        .context("polygon missing coordinates")?,
                )?,
            }),
            other => bail!(
                "unsupported cutline geometry type {other} in {}",
                path.display()
            ),
        }
    }
    Ok(polygons)
}

fn polygon_points_from_geojson_coordinates(
    coordinates: &serde_json::Value,
) -> anyhow::Result<Vec<[f64; 2]>> {
    let rings = coordinates
        .as_array()
        .context("polygon coordinates were not an array")?;
    let exterior = rings
        .first()
        .and_then(|ring| ring.as_array())
        .context("polygon had no exterior ring")?;
    exterior
        .iter()
        .map(|point| {
            let point = point.as_array().context("polygon point was not an array")?;
            let x = point
                .first()
                .and_then(|value| value.as_f64())
                .context("polygon point missing x/lon")?;
            let y = point
                .get(1)
                .and_then(|value| value.as_f64())
                .context("polygon point missing y/lat")?;
            Ok(if x.abs() > 180.0 || y.abs() > 90.0 {
                web_mercator_to_lon_lat(x, y)
            } else {
                [x, y]
            })
        })
        .collect()
}

fn web_mercator_to_lon_lat(x: f64, y: f64) -> [f64; 2] {
    let origin_shift = 20_037_508.342_789_244_f64;
    let lon = (x / origin_shift) * 180.0;
    let lat = (y / origin_shift) * 180.0;
    let lat = 180.0 / std::f64::consts::PI
        * (2.0 * ((lat * std::f64::consts::PI / 180.0).exp()).atan() - std::f64::consts::PI / 2.0);
    [lon, lat]
}

fn collections_for_cutline_polygon<'a>(
    points: &[[f64; 2]],
    collections: &[&'a preprocessor_resource_index::ChartCollectionRecord],
) -> Vec<&'a preprocessor_resource_index::ChartCollectionRecord> {
    let Some(polygon_bounds) = polygon_bounds(points) else {
        return Vec::new();
    };
    let overlapping = collections
        .iter()
        .copied()
        .filter(|collection| overlap_area(&polygon_bounds, &collection.coverage_bounds) > 0.0)
        .collect::<Vec<_>>();
    if !overlapping.is_empty() {
        return overlapping;
    }
    collections
        .iter()
        .copied()
        .max_by(|left, right| {
            overlap_area(&polygon_bounds, &left.coverage_bounds)
                .partial_cmp(&overlap_area(&polygon_bounds, &right.coverage_bounds))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .into_iter()
        .collect()
}

fn overlap_area(
    left: &preprocessor_resource_index::CoverageBounds,
    right: &preprocessor_resource_index::CoverageBounds,
) -> f64 {
    let lon_overlap = (left.lon_max.min(right.lon_max) - left.lon_min.max(right.lon_min)).max(0.0);
    let lat_overlap = (left.lat_max.min(right.lat_max) - left.lat_min.max(right.lat_min)).max(0.0);
    lon_overlap * lat_overlap
}

fn polygon_bounds(points: &[[f64; 2]]) -> Option<preprocessor_resource_index::CoverageBounds> {
    let first = points.first()?;
    let mut lon_min = first[0];
    let mut lon_max = first[0];
    let mut lat_min = first[1];
    let mut lat_max = first[1];
    for point in points.iter().skip(1) {
        lon_min = lon_min.min(point[0]);
        lon_max = lon_max.max(point[0]);
        lat_min = lat_min.min(point[1]);
        lat_max = lat_max.max(point[1]);
    }
    Some(preprocessor_resource_index::CoverageBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    })
}

fn default_view_for_static_region(
    resource_index: &ResourceIndex,
    region: Region,
) -> preprocessor_resource_index::DefaultView {
    let region_id = region.code().to_ascii_lowercase();
    if let Some(reference) = resource_index
        .chart_collections
        .iter()
        .find(|collection| collection.region_id == region_id && collection.family_id == "sec")
        .or_else(|| {
            resource_index
                .chart_collections
                .iter()
                .find(|collection| collection.region_id == region_id)
        })
    {
        return reference.default_view.clone();
    }

    let bounds = region.bounds();
    preprocessor_resource_index::DefaultView {
        lat: (bounds.lat_min + bounds.lat_max) / 2.0,
        lon: (bounds.lon_min + bounds.lon_max) / 2.0,
        zoom: 4.0,
    }
}

fn build_nav_kv_plate_pairs(resource_index: &ResourceIndex) -> anyhow::Result<Vec<NavKvPair>> {
    let airports = build_nav_kv_plate_airports(resource_index);
    let airport_index = airports
        .iter()
        .map(|airport| {
            serde_json::json!({
                "id": airport.get("id").and_then(|value| value.as_str()).unwrap_or_default(),
                "label": airport.get("label").and_then(|value| value.as_str()).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let mut pairs = vec![NavKvPair {
        key: "plate/airport-index".to_string(),
        value: serde_json::to_vec(&airport_index)
            .context("failed to encode nav_kv plate/airport-index value")?,
    }];
    for airport in airports {
        let Some(airport_id) = airport.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        pairs.push(NavKvPair {
            key: format!("plate/airport/{}", had_upper_key_component(airport_id)),
            value: serde_json::to_vec(&airport).with_context(|| {
                format!("failed to encode nav_kv plate/airport/{airport_id} value")
            })?,
        });
        if let Some(charts) = airport.get("charts").and_then(|value| value.as_array()) {
            for chart in charts {
                let Some(plate_id) = chart.get("id").and_then(|value| value.as_str()) else {
                    continue;
                };
                pairs.push(NavKvPair {
                    key: format!("plate/by-id/{}", had_key_component(plate_id)),
                    value: serde_json::to_vec(chart).with_context(|| {
                        format!("failed to encode nav_kv plate/by-id/{plate_id} value")
                    })?,
                });
            }
        }
    }
    Ok(pairs)
}

fn build_nav_kv_resource_summary_pairs(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<NavKvPair>> {
    let families = resource_index
        .families
        .iter()
        .map(|family| {
            serde_json::json!({
                "id": family.id,
                "display_name": family.display_name,
                "kind": family.kind,
            })
        })
        .collect::<Vec<_>>();
    let regions = resource_index
        .regions
        .iter()
        .map(|region| {
            serde_json::json!({
                "id": region.id,
                "display_name": region.display_name,
                "sort_order": region.sort_order,
            })
        })
        .collect::<Vec<_>>();
    let temporal_summary = serde_json::json!({
        "cycle_codes": resource_index.temporal_summary.cycle_codes,
        "effective_dates": resource_index.temporal_summary.effective_dates,
        "expiration_dates": resource_index.temporal_summary.expiration_dates,
        "uniform_cycle_code": resource_index.temporal_summary.uniform_cycle_code,
        "uniform_effective_date": resource_index.temporal_summary.uniform_effective_date,
        "uniform_expiration_date": resource_index.temporal_summary.uniform_expiration_date,
        "uniform_good_beyond_date": resource_index.temporal_summary.uniform_good_beyond_date,
    });
    Ok(vec![
        json_pair(
            "resource/families".to_string(),
            &serde_json::Value::Array(families),
            "resource/families",
        )?,
        json_pair(
            "resource/regions".to_string(),
            &serde_json::Value::Array(regions),
            "resource/regions",
        )?,
        json_pair(
            "resource/temporal-summary".to_string(),
            &temporal_summary,
            "resource/temporal-summary",
        )?,
    ])
}

fn build_nav_kv_package_pairs(
    package_artifacts: &[BundlePackageArtifact],
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut package_index = Vec::with_capacity(package_artifacts.len());
    let mut pairs = Vec::with_capacity(package_artifacts.len());
    for package in package_artifacts {
        let value = serde_json::json!({
            "id": package.id,
            "family_id": package.family_id,
            "region_id": package.region_id,
            "relative_path": package.relative_path,
            "size_bytes": package.size_bytes,
            "checksum_sha256": package.checksum_sha256,
            "cycle": package.cycle,
            "cycle_version": package.cycle_version,
            "effective_date": package.effective_date,
            "expiration_date": package.expiration_date,
            "metadata": package.metadata,
        });
        package_index.push(serde_json::json!({
            "id": package.id,
            "family_id": package.family_id,
            "region_id": package.region_id,
            "metadata": &package.metadata,
        }));
        pairs.push(json_pair(
            format!("package/by-id/{}", had_key_component(&package.id)),
            &value,
            &format!("package/by-id/{}", package.id),
        )?);
    }
    pairs.push(json_pair(
        "package/index".to_string(),
        &serde_json::Value::Array(package_index),
        "package/index",
    )?);
    Ok(pairs)
}

fn build_nav_kv_plate_airports(resource_index: &ResourceIndex) -> Vec<serde_json::Value> {
    let airport_by_id = resource_index
        .airports
        .iter()
        .map(|airport| (airport.id.as_str(), airport))
        .collect::<BTreeMap<_, _>>();
    let plate_by_id = resource_index
        .plates
        .iter()
        .map(|plate| (plate.id.as_str(), plate))
        .collect::<BTreeMap<_, _>>();
    let csup_by_id = resource_index
        .csups
        .iter()
        .map(|csup| (csup.id.as_str(), csup))
        .collect::<BTreeMap<_, _>>();
    resource_index
        .airport_resources
        .iter()
        .filter_map(|airport_resources| {
            let airport_id = &airport_resources.airport_id;
            let mut charts = Vec::new();
            for plate_id in &airport_resources.plate_ids {
                if let Some(plate) = plate_by_id.get(plate_id.as_str()) {
                    charts.push(nav_kv_plate_asset(airport_id, plate));
                }
            }
            for csup_id in &airport_resources.csup_ids {
                if let Some(csup) = csup_by_id.get(csup_id.as_str()) {
                    charts.push(nav_kv_csup_asset(airport_id, csup));
                }
            }
            charts.sort_by(|left, right| {
                let left_category = left
                    .get("folder_category")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let right_category = right
                    .get("folder_category")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let left_label = left
                    .get("label")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let right_label = right
                    .get("label")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                folder_category_rank(left_category)
                    .cmp(&folder_category_rank(right_category))
                    .then_with(|| left_label.cmp(right_label))
            });
            if charts.is_empty() {
                return None;
            }
            let airport = airport_by_id.get(airport_id.as_str());
            Some(serde_json::json!({
                "id": airport_id,
                "label": airport
                    .map(|airport| airport.facility_name.as_str())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(airport_id),
                "facility_name": airport.map(|airport| airport.facility_name.as_str()),
                "lat": airport.map(|airport| airport.lat),
                "lon": airport.map(|airport| airport.lon),
                "airport_type": airport.map(|airport| airport.airport_type.as_str()),
                "package_ids": airport_resources.package_ids.clone(),
                "charts": charts,
            }))
        })
        .collect::<Vec<_>>()
}

fn build_nav_kv_navref_pairs(main_db_path: &Path) -> anyhow::Result<Vec<NavKvPair>> {
    let connection = rusqlite::Connection::open(main_db_path)
        .with_context(|| format!("failed to open {}", main_db_path.display()))?;
    let mut pairs = Vec::new();
    pairs.extend(build_nav_kv_airport_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_navaid_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_arinc_navaid_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_fix_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_runway_position_pairs(&connection)?);
    pairs.extend(build_nav_kv_waypoint_lookup_pairs(&connection)?);
    pairs.extend(build_nav_kv_procedure_pairs(&connection)?);
    pairs.extend(build_nav_kv_airway_pairs(&connection)?);
    let mut deduped = BTreeMap::<String, Vec<u8>>::new();
    for pair in pairs {
        deduped.entry(pair.key).or_insert(pair.value);
    }
    validate_airway_navrefs_resolve(&deduped)?;
    Ok(deduped
        .into_iter()
        .map(|(key, value)| NavKvPair { key, value })
        .collect())
}

fn validate_airway_navrefs_resolve(pairs: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<()> {
    for (key, value) in pairs {
        if !key.starts_with("airway/") {
            continue;
        }
        let json: serde_json::Value = serde_json::from_slice(value)
            .with_context(|| format!("failed to parse nav_kv airway value {key}"))?;
        validate_airway_navrefs_in_value(pairs, key, &json)?;
    }
    Ok(())
}

fn validate_airway_navrefs_in_value(
    pairs: &BTreeMap<String, Vec<u8>>,
    source_key: &str,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                validate_airway_navrefs_in_value(pairs, source_key, value)?;
            }
        }
        serde_json::Value::Object(object) => {
            if let Some(nav_ref) = object.get("nav_ref") {
                validate_airway_nav_ref_resolves(pairs, source_key, nav_ref)?;
            }
            for value in object.values() {
                validate_airway_navrefs_in_value(pairs, source_key, value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_airway_nav_ref_resolves(
    pairs: &BTreeMap<String, Vec<u8>>,
    source_key: &str,
    nav_ref: &serde_json::Value,
) -> anyhow::Result<()> {
    let required_key = if let Some(id) = nav_ref.get("Airport").and_then(|value| value.as_str()) {
        Some(format!(
            "navref/position/airport/{}",
            id.trim().to_ascii_uppercase()
        ))
    } else if let Some(arinc) = nav_ref
        .get("ArincNavaid")
        .and_then(|value| value.as_object())
    {
        match (
            arinc.get("identifier").and_then(|value| value.as_str()),
            arinc.get("icao_code").and_then(|value| value.as_str()),
            arinc.get("section_code").and_then(|value| value.as_str()),
            arinc
                .get("subsection_code")
                .and_then(|value| value.as_str()),
        ) {
            (Some(identifier), Some(icao_code), Some(section_code), Some(subsection_code)) => {
                Some(format!(
                    "navref/position/arinc-navaid/{}",
                    arinc_navaid_had_key(identifier, icao_code, section_code, subsection_code)
                ))
            }
            _ => None,
        }
    } else if let Some(id) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
        Some(format!(
            "navref/position/navaid/{}",
            id.trim().to_ascii_uppercase()
        ))
    } else {
        nav_ref
            .get("Fix")
            .and_then(|value| value.as_str())
            .map(|id| format!("navref/position/fix/{}", id.trim().to_ascii_uppercase()))
    };

    let Some(required_key) = required_key else {
        return Ok(());
    };
    anyhow::ensure!(
        pairs.contains_key(&required_key),
        "nav_kv airway value {source_key} emits unresolved nav_ref {nav_ref}; missing {required_key}"
    );
    Ok(())
}

fn build_nav_kv_airport_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type), trim(ATCT), trim(FuelTypes)
        FROM airports
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let runway_info = airport_runway_symbol_info_by_airport(connection)?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind, atct, fuel_types) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/airport/{key_id}"),
            &serde_json::json!({ "lat": lat, "lon": lon }),
            "navref airport position",
        )?);
        let info = runway_info.get(&id.trim().to_ascii_uppercase());
        let has_water_runway = info.map(|info| info.has_water_runway).unwrap_or(false)
            || kind.trim().eq_ignore_ascii_case("SEAPLANE BAS");
        pairs.push(json_pair(
            format!("navref/symbol/airport/{key_id}"),
            &serde_json::json!({
                "kind": kind.to_ascii_lowercase(),
                "label": airport_display_label(&id),
                "style_class": "airport",
                "towered": atct.trim().eq_ignore_ascii_case("Y"),
                "fuel_available": !fuel_types.trim().is_empty(),
                "has_paved_runway": info.map(|info| info.has_paved_runway),
                "heliport": kind.trim().to_ascii_uppercase().contains("HELIPORT"),
                "has_water_runway": has_water_runway,
                "runway_length_ratio": runway_length_ratio(info.map(|info| info.length_ft)),
                "longest_runway_heading_true_deg": info.map(|info| info.heading_true_deg),
            }),
            "navref airport symbol",
        )?);
        let _ = facility_name;
    }
    Ok(pairs)
}

fn build_nav_kv_navaid_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type)
        FROM nav
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/navaid/{key_id}"),
            &serde_json::json!({ "lat": lat, "lon": lon }),
            "navref navaid position",
        )?);
        if matches!(
            kind.trim().to_ascii_uppercase().as_str(),
            "VOR" | "VOR/DME" | "VORTAC"
        ) {
            pairs.push(json_pair(
                format!("navref/symbol/navaid/{key_id}"),
                &serde_json::json!({
                    "kind": kind.to_ascii_lowercase(),
                    "label": navaid_display_label(&id, &facility_name),
                    "style_class": "nav",
                    "towered": false,
                    "fuel_available": false,
                    "runway_length_ratio": 0.0,
                    "longest_runway_heading_true_deg": serde_json::Value::Null,
                }),
                "navref navaid symbol",
            )?);
        }
    }
    Ok(pairs)
}

fn build_nav_kv_arinc_navaid_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
          AND trim(subsection_code) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (identifier, icao_code, section_code, subsection_code, lat, lon) = row?;
        pairs.push(json_pair(
            format!(
                "navref/position/arinc-navaid/{}",
                arinc_navaid_had_key(&identifier, &icao_code, &section_code, &subsection_code)
            ),
            &serde_json::json!({ "lat": lat, "lon": lon }),
            "navref ARINC navaid position",
        )?);
    }
    Ok(pairs)
}

fn build_nav_kv_fix_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type)
        FROM fix
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/fix/{key_id}"),
            &serde_json::json!({ "lat": lat, "lon": lon }),
            "navref fix position",
        )?);
        pairs.push(json_pair(
            format!("navref/symbol/fix/{key_id}"),
            &serde_json::json!({
                "kind": kind.to_ascii_lowercase(),
                "label": titlecase_nav_label(&facility_name).to_ascii_uppercase(),
                "style_class": "fix",
                "towered": false,
                "fuel_available": false,
                "runway_length_ratio": 0.0,
                "longest_runway_heading_true_deg": serde_json::Value::Null,
            }),
            "navref fix symbol",
        )?);
    }
    Ok(pairs)
}

fn build_nav_kv_runway_position_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(LEIdent), CAST(LELatitude AS REAL), CAST(LELongitude AS REAL),
               trim(HEIdent), CAST(HELatitude AS REAL), CAST(HELongitude AS REAL)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (airport_id, le_ident, le_lat, le_lon, he_ident, he_lat, he_lon) = row?;
        for (ident, lat, lon) in [(le_ident, le_lat, le_lon), (he_ident, he_lat, he_lon)] {
            let ident = ident.trim();
            if ident.is_empty() {
                continue;
            }
            pairs.push(json_pair(
                format!(
                    "navref/position/runway/{}/{}",
                    had_upper_key_component(&airport_id),
                    had_upper_key_component(&format!("RW{ident}")),
                ),
                &serde_json::json!({ "lat": lat, "lon": lon }),
                "navref runway position",
            )?);
        }
    }
    Ok(pairs)
}

fn build_nav_kv_waypoint_lookup_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut candidates = Vec::<serde_json::Value>::new();
    let mut exists_by_identifier = BTreeMap::<String, (bool, bool, bool)>::new();
    collect_waypoint_candidates(
        connection,
        "airports",
        "airport",
        &mut candidates,
        &mut exists_by_identifier,
    )?;
    collect_waypoint_candidates(
        connection,
        "nav",
        "navaid",
        &mut candidates,
        &mut exists_by_identifier,
    )?;
    collect_waypoint_candidates(
        connection,
        "fix",
        "fix",
        &mut candidates,
        &mut exists_by_identifier,
    )?;

    let mut pairs = Vec::new();
    for (identifier, (exists_as_airport, exists_as_navaid, exists_as_fix)) in exists_by_identifier {
        let nav_ref = if is_runway_identifier(&identifier) {
            Some(serde_json::json!({ "Fix": identifier }))
        } else if exists_as_navaid {
            Some(serde_json::json!({ "Navaid": identifier }))
        } else if exists_as_airport {
            Some(serde_json::json!({ "Airport": identifier }))
        } else if exists_as_fix {
            Some(serde_json::json!({ "Fix": identifier }))
        } else {
            None
        };
        pairs.push(json_pair(
            format!(
                "waypoint/identifier/{}",
                had_upper_key_component(&identifier)
            ),
            &nav_ref.unwrap_or(serde_json::Value::Null),
            "waypoint identifier",
        )?);
    }

    candidates.sort_by(|left, right| {
        let left_identifier = left
            .get("identifier")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let right_identifier = right
            .get("identifier")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let left_kind = left
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let right_kind = right
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        left_identifier
            .len()
            .cmp(&right_identifier.len())
            .then_with(|| left_identifier.cmp(right_identifier))
            .then_with(|| waypoint_kind_rank(left_kind).cmp(&waypoint_kind_rank(right_kind)))
    });

    let mut by_prefix = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for candidate in candidates {
        let Some(identifier) = candidate.get("identifier").and_then(|value| value.as_str()) else {
            continue;
        };
        let chars = identifier.chars().collect::<Vec<_>>();
        for length in 1..=2.min(chars.len()) {
            let prefix = chars.iter().take(length).collect::<String>();
            by_prefix.entry(prefix).or_default().push(candidate.clone());
        }
    }
    for (prefix, candidates) in by_prefix {
        pairs.push(json_pair(
            format!("waypoint/prefix/{}", had_upper_key_component(&prefix)),
            &serde_json::Value::Array(candidates),
            "waypoint prefix",
        )?);
    }

    Ok(pairs)
}

fn collect_waypoint_candidates(
    connection: &rusqlite::Connection,
    table: &str,
    kind: &str,
    candidates: &mut Vec<serde_json::Value>,
    exists_by_identifier: &mut BTreeMap<String, (bool, bool, bool)>,
) -> anyhow::Result<()> {
    let sql = if kind == "airport" {
        format!(
            "
        SELECT trim(LocationID), trim(City), trim(State), trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
        )
    } else {
        format!(
            "
        SELECT trim(LocationID), '', '', trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
        )
    };
    let mut stmt = connection.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(),
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    for row in rows {
        let (identifier, city, state, facility_name, lat, lon) = row?;
        let identifier = identifier.trim().to_ascii_uppercase();
        if identifier.is_empty() {
            continue;
        }
        let entry = exists_by_identifier.entry(identifier.clone()).or_default();
        match kind {
            "airport" => entry.0 = true,
            "navaid" => entry.1 = true,
            "fix" => entry.2 = true,
            _ => {}
        }
        let nav_ref = match kind {
            "airport" => serde_json::json!({ "Airport": identifier }),
            "navaid" => serde_json::json!({ "Navaid": identifier }),
            _ => serde_json::json!({ "Fix": identifier }),
        };
        candidates.push(serde_json::json!({
            "identifier": identifier,
            "nav_ref": nav_ref,
            "kind": kind,
            "city": city,
            "state": state,
            "facility_name": facility_name,
            "position": { "lat": lat, "lon": lon },
        }));
    }
    Ok(())
}

fn waypoint_kind_rank(kind: &str) -> usize {
    match kind {
        "navaid" => 0,
        "airport" => 1,
        "fix" => 2,
        _ => 3,
    }
}

fn build_nav_kv_procedure_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut pairs = Vec::new();
    let airport_ids = load_nav_kv_airport_ids(connection)?;
    let cifp_matches = load_nav_kv_cifp_tpp_matches(connection)?;
    let mut matches_by_procedure = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut matches_by_plate = BTreeMap::<String, Vec<serde_json::Value>>::new();
    let mut approach_lists = BTreeMap::<String, BTreeSet<String>>::new();
    for row in cifp_matches {
        let airport_id = row
            .get("airport_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let cifp_id = row
            .get("cifp_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let plate_id = row
            .get("plate_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if !airport_id.is_empty() && !cifp_id.is_empty() {
            matches_by_procedure
                .entry((airport_id.clone(), cifp_id.clone()))
                .or_default()
                .push(row.clone());
            approach_lists
                .entry(airport_id)
                .or_default()
                .insert(cifp_id);
        }
        if !plate_id.is_empty() {
            matches_by_plate.entry(plate_id).or_default().push(row);
        }
    }
    for ((airport_id, cifp_id), rows) in matches_by_procedure {
        pairs.push(json_pair(
            format!(
                "plate/cifp/{}/{}",
                had_upper_key_component(&airport_id),
                had_upper_key_component(&cifp_id)
            ),
            &serde_json::Value::Array(rows),
            "plate cifp matches",
        )?);
    }
    for (plate_id, rows) in matches_by_plate {
        pairs.push(json_pair(
            format!(
                "plate/procedure-candidates/{}",
                had_key_component(&plate_id)
            ),
            &serde_json::Value::Array(rows),
            "plate procedure candidates",
        )?);
    }
    for airport_id in &airport_ids {
        let procedure_ids = approach_lists.remove(airport_id).unwrap_or_default();
        let rows = procedure_ids
            .into_iter()
            .map(|procedure_id| {
                serde_json::json!({
                    "airport_id": airport_id,
                    "procedure_id": procedure_id,
                    "kind": "approach",
                })
            })
            .collect::<Vec<_>>();
        pairs.push(json_pair(
            format!(
                "procedure/list/{}/APPROACH",
                had_upper_key_component(&airport_id)
            ),
            &serde_json::Value::Array(rows),
            "approach procedure list",
        )?);
    }

    let mut sid_lists = BTreeMap::<String, BTreeSet<String>>::new();
    let mut star_lists = BTreeMap::<String, BTreeSet<String>>::new();
    let mut distinct_by_procedure = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut materialization_by_procedure =
        BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    load_nav_kv_procedure_rows(
        connection,
        &mut sid_lists,
        &mut star_lists,
        &mut distinct_by_procedure,
        &mut materialization_by_procedure,
    )?;
    for airport_id in &airport_ids {
        pairs.push(nav_kv_procedure_list_pair(
            airport_id,
            "SID",
            "sid",
            sid_lists.remove(airport_id).unwrap_or_default(),
        )?);
        pairs.push(nav_kv_procedure_list_pair(
            airport_id,
            "STAR",
            "star",
            star_lists.remove(airport_id).unwrap_or_default(),
        )?);
    }
    for ((airport_id, procedure_id), rows) in distinct_by_procedure {
        pairs.push(json_pair(
            format!(
                "procedure/distinct-rows/{}/{}",
                had_upper_key_component(&airport_id),
                had_upper_key_component(&procedure_id)
            ),
            &serde_json::Value::Array(rows),
            "procedure distinct rows",
        )?);
    }
    for ((airport_id, procedure_id), rows) in materialization_by_procedure {
        pairs.push(json_pair(
            format!(
                "procedure/materialization-rows/{}/{}",
                had_upper_key_component(&airport_id),
                had_upper_key_component(&procedure_id)
            ),
            &serde_json::Value::Array(rows),
            "procedure materialization rows",
        )?);
    }

    Ok(pairs)
}

fn load_nav_kv_airport_ids(connection: &rusqlite::Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID)
        FROM airports
        WHERE trim(LocationID) <> ''
        ORDER BY trim(LocationID)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(row.get::<_, String>(0)?.trim().to_ascii_uppercase())
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn load_nav_kv_cifp_tpp_matches(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(airport_id), trim(cifp_id), trim(plate_id), trim(plate_label),
               trim(package_id), CAST(public AS INTEGER), CAST(priority AS INTEGER),
               trim(match_kind), CAST(is_primary AS INTEGER)
        FROM cifp_tpp_matches
        ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "airport_id": row.get::<_, String>(0)?,
            "cifp_id": row.get::<_, String>(1)?,
            "plate_id": row.get::<_, String>(2)?,
            "plate_label": row.get::<_, String>(3)?,
            "package_id": row.get::<_, String>(4)?,
            "public": row.get::<_, i64>(5)?,
            "priority": row.get::<_, i64>(6)?,
            "match_kind": row.get::<_, String>(7)?,
            "is_primary": row.get::<_, i64>(8)?,
        }))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn nav_kv_procedure_list_pair(
    airport_id: &str,
    kind_key: &str,
    kind_value: &str,
    procedure_ids: BTreeSet<String>,
) -> anyhow::Result<NavKvPair> {
    let rows = procedure_ids
        .into_iter()
        .map(|procedure_id| {
            serde_json::json!({
                "airport_id": airport_id,
                "procedure_id": procedure_id,
                "kind": kind_value,
            })
        })
        .collect::<Vec<_>>();
    json_pair(
        format!(
            "procedure/list/{}/{}",
            had_upper_key_component(airport_id),
            kind_key
        ),
        &serde_json::Value::Array(rows),
        "procedure list",
    )
}

fn load_nav_kv_procedure_rows(
    connection: &rusqlite::Connection,
    sid_lists: &mut BTreeMap<String, BTreeSet<String>>,
    star_lists: &mut BTreeMap<String, BTreeSet<String>>,
    distinct_by_procedure: &mut BTreeMap<(String, String), Vec<serde_json::Value>>,
    materialization_by_procedure: &mut BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<()> {
    let nav_context = NavLookupContext::load(connection)?;
    let mut distinct_seen = BTreeSet::<(String, String, String, String)>::new();
    let mut stmt = connection.prepare(
        "
        SELECT
          trim(airport_identifier),
          trim(sid_star_approach_identifier),
          trim(route_type),
          trim(transition_identifier),
          CAST(sequence_number AS INTEGER),
          trim(fix_identifier),
          trim(icao_code_2),
          trim(section_code_2),
          trim(subsection_code_2),
          trim(recommended_navaid),
          trim(icao_code_3),
          trim(recd_nav_section),
          trim(recd_nav_subsection),
          trim(altitude_1),
          trim(altitude_2),
          trim(path_and_termination),
          trim(turn_direction),
          trim(theta),
          trim(magnetic_course),
          trim(route_distance_holding_distance_or_time)
        FROM cifp_sid_star_app
        WHERE trim(airport_identifier) <> ''
          AND trim(sid_star_approach_identifier) <> ''
        ORDER BY trim(route_type), trim(transition_identifier), CAST(sequence_number AS INTEGER)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, String>(16)?,
            row.get::<_, String>(17)?,
            row.get::<_, String>(18)?,
            row.get::<_, String>(19)?,
        ))
    })?;
    for row in rows {
        let (
            airport_id,
            procedure_id,
            route_type,
            transition_id,
            sequence,
            fix_identifier,
            fix_icao_code,
            fix_section_code,
            fix_subsection_code,
            recommended_navaid,
            recommended_nav_icao_code,
            recommended_nav_section,
            recommended_nav_subsection,
            altitude_1,
            altitude_2,
            path_termination,
            turn_direction,
            theta,
            magnetic_course,
            route_distance_or_time,
        ) = row?;
        match infer_nav_kv_procedure_kind(&route_type) {
            "sid" => {
                sid_lists
                    .entry(airport_id.clone())
                    .or_default()
                    .insert(procedure_id.clone());
            }
            "star" => {
                star_lists
                    .entry(airport_id.clone())
                    .or_default()
                    .insert(procedure_id.clone());
            }
            _ => {}
        }
        if distinct_seen.insert((
            airport_id.clone(),
            procedure_id.clone(),
            route_type.clone(),
            transition_id.clone(),
        )) {
            distinct_by_procedure
                .entry((airport_id.clone(), procedure_id.clone()))
                .or_default()
                .push(serde_json::json!({
                    "route_type": route_type,
                    "transition_id": transition_id,
                }));
        }
        if path_termination.trim().is_empty() {
            continue;
        }
        let nav_ref = nav_context.classify_cifp_reference_json(
            &fix_identifier,
            &fix_icao_code,
            &fix_section_code,
            &fix_subsection_code,
        );
        let defining_nav_ref = nav_context.classify_cifp_reference_json(
            &recommended_navaid,
            &recommended_nav_icao_code,
            &recommended_nav_section,
            &recommended_nav_subsection,
        );
        let nav_position = nav_context.resolve_position_json(&nav_ref, Some(&airport_id));
        let defining_nav_position =
            nav_context.resolve_position_json(&defining_nav_ref, Some(&airport_id));
        materialization_by_procedure
            .entry((airport_id.clone(), procedure_id.clone()))
            .or_default()
            .push(serde_json::json!({
                "key": {
                    "airport_id": airport_id,
                    "procedure_id": procedure_id,
                    "route_type": route_type,
                    "transition_id": transition_id,
                },
                "sequence": sequence,
                "nav_ref": nav_ref,
                "nav_position": nav_position,
                "nav_magnetic_variation_deg": nav_context.variation_for_nav_ref(&nav_ref),
                "defining_nav_ref": defining_nav_ref,
                "defining_nav_position": defining_nav_position,
                "defining_nav_magnetic_variation_deg": nav_context.variation_for_nav_ref(&defining_nav_ref),
                "airport_magnetic_variation_deg": nav_context.airport_variation.get(&airport_id.trim().to_ascii_uppercase()).copied().flatten(),
                "altitude_1_ft": parse_nav_kv_cifp_altitude_ft(&altitude_1),
                "altitude_2_ft": parse_nav_kv_cifp_altitude_ft(&altitude_2),
                "path_termination": path_termination,
                "turn_direction": non_empty_json_string(turn_direction),
                "theta_deg": parse_nav_kv_cifp_tenths_value(&theta),
                "magnetic_course_deg": parse_nav_kv_cifp_tenths_value(&magnetic_course),
                "route_distance_or_time": non_empty_json_string(route_distance_or_time),
            }));
    }
    Ok(())
}

fn infer_nav_kv_procedure_kind(route_type: &str) -> &'static str {
    match route_type.trim() {
        "1" | "2" | "3" => "star",
        "4" | "5" | "6" => "sid",
        _ => "approach",
    }
}

struct NavLookupContext {
    airport_positions: BTreeMap<String, serde_json::Value>,
    navaid_positions: BTreeMap<String, serde_json::Value>,
    arinc_navaid_positions: BTreeMap<ArincNavaidKey, serde_json::Value>,
    fix_positions: BTreeMap<String, serde_json::Value>,
    airport_positions_by_coord: BTreeMap<(i64, i64), String>,
    navaid_positions_by_coord: BTreeMap<(i64, i64), String>,
    fix_positions_by_coord: BTreeMap<(i64, i64), String>,
    runway_positions: BTreeMap<(String, String), serde_json::Value>,
    navaid_variation: BTreeMap<String, Option<f64>>,
    arinc_navaid_variation: BTreeMap<ArincNavaidKey, Option<f64>>,
    airport_variation: BTreeMap<String, Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ArincNavaidKey {
    identifier: String,
    icao_code: String,
    section_code: String,
    subsection_code: String,
}

impl ArincNavaidKey {
    fn new(identifier: &str, icao_code: &str, section_code: &str, subsection_code: &str) -> Self {
        Self {
            identifier: identifier.trim().to_ascii_uppercase(),
            icao_code: icao_code.trim().to_ascii_uppercase(),
            section_code: section_code.trim().to_ascii_uppercase(),
            subsection_code: subsection_code.trim().to_ascii_uppercase(),
        }
    }

    fn is_complete(&self) -> bool {
        !self.identifier.is_empty()
            && !self.icao_code.is_empty()
            && !self.section_code.is_empty()
            && !self.subsection_code.is_empty()
    }
}

fn is_runway_identifier(identifier: &str) -> bool {
    let trimmed = identifier.trim().to_ascii_uppercase();
    let suffix = match trimmed.strip_prefix("RW") {
        Some(suffix) => suffix,
        None => return false,
    };
    if suffix.is_empty() {
        return false;
    }
    let mut chars = suffix.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric())
}

impl NavLookupContext {
    fn load(connection: &rusqlite::Connection) -> anyhow::Result<Self> {
        let airport_positions =
            load_nav_position_map(connection, "airports", "ARPLatitude", "ARPLongitude")?;
        let navaid_positions =
            load_nav_position_map(connection, "nav", "ARPLatitude", "ARPLongitude")?;
        let arinc_navaid_positions = load_arinc_navaid_position_map(connection)?;
        let arinc_navaid_variation = load_arinc_navaid_variation_map(connection)?;
        let fix_positions =
            load_nav_position_map(connection, "fix", "ARPLatitude", "ARPLongitude")?;
        Ok(Self {
            airport_positions_by_coord: build_position_lookup(&airport_positions),
            navaid_positions_by_coord: build_position_lookup(&navaid_positions),
            fix_positions_by_coord: build_position_lookup(&fix_positions),
            airport_positions,
            navaid_positions,
            arinc_navaid_positions,
            fix_positions,
            runway_positions: load_runway_position_map(connection)?,
            navaid_variation: load_variation_map(connection, "nav", "Variation", false)?,
            arinc_navaid_variation,
            airport_variation: load_variation_map(
                connection,
                "airports",
                "MagneticVariation",
                true,
            )?,
        })
    }

    fn classify_json(&self, identifier: &str) -> serde_json::Value {
        let trimmed = identifier.trim().to_ascii_uppercase();
        if trimmed.is_empty() {
            return serde_json::Value::Null;
        }
        if is_runway_identifier(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }
        if self.navaid_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Navaid": trimmed });
        }
        if self.airport_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Airport": trimmed });
        }
        if self.fix_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }
        serde_json::Value::Null
    }

    fn classify_cifp_reference_json(
        &self,
        identifier: &str,
        icao_code: &str,
        section_code: &str,
        subsection_code: &str,
    ) -> serde_json::Value {
        let trimmed = identifier.trim().to_ascii_uppercase();
        if trimmed.is_empty() {
            return serde_json::Value::Null;
        }
        if is_runway_identifier(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }

        match section_code.trim().to_ascii_uppercase().as_str() {
            "D" => {
                let key = ArincNavaidKey::new(&trimmed, icao_code, section_code, subsection_code);
                if key.is_complete() && self.arinc_navaid_positions.contains_key(&key) {
                    return serde_json::json!({
                        "ArincNavaid": {
                            "identifier": key.identifier,
                            "icao_code": key.icao_code,
                            "section_code": key.section_code,
                            "subsection_code": key.subsection_code,
                        }
                    });
                }
                if self.navaid_positions.contains_key(&trimmed) {
                    return serde_json::json!({ "Navaid": trimmed });
                }
            }
            "A" => {
                if self.airport_positions.contains_key(&trimmed) {
                    return serde_json::json!({ "Airport": trimmed });
                }
            }
            "P" => {
                let subsection = subsection_code.trim().to_ascii_uppercase();
                if subsection == "C" || subsection.is_empty() {
                    if self.fix_positions.contains_key(&trimmed) {
                        return serde_json::json!({ "Fix": trimmed });
                    }
                }
            }
            _ => {}
        }

        self.classify_json(&trimmed)
    }

    fn classify_airway_point_json(
        &self,
        identifier: &str,
        lat: f64,
        lon: f64,
    ) -> serde_json::Value {
        if let Some(nav_ref) = self.classify_by_position_json(lat, lon) {
            return nav_ref;
        }

        let nav_ref = self.classify_json(identifier);
        if !nav_ref.is_null() {
            return nav_ref;
        }

        serde_json::json!({ "LatLon": { "lat": lat, "lon": lon } })
    }

    fn classify_by_position_json(&self, lat: f64, lon: f64) -> Option<serde_json::Value> {
        let key = position_lookup_key(lat, lon);
        if let Some(id) = self.fix_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Fix": id }));
        }
        if let Some(id) = self.navaid_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Navaid": id }));
        }
        if let Some(id) = self.airport_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Airport": id }));
        }
        None
    }

    fn resolve_position_json(
        &self,
        nav_ref: &serde_json::Value,
        procedure_airport_id: Option<&str>,
    ) -> serde_json::Value {
        if let Some(key) = arinc_navaid_key_from_nav_ref(nav_ref) {
            return self
                .arinc_navaid_positions
                .get(&key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Airport").and_then(|value| value.as_str()) {
            return self
                .airport_positions
                .get(&code.trim().to_ascii_uppercase())
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
            return self
                .navaid_positions
                .get(&code.trim().to_ascii_uppercase())
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Fix").and_then(|value| value.as_str()) {
            let code = code.trim().to_ascii_uppercase();
            if let Some(airport_id) = procedure_airport_id {
                if is_runway_identifier(&code) {
                    if let Some(position) = self
                        .runway_positions
                        .get(&(airport_id.trim().to_ascii_uppercase(), code.clone()))
                    {
                        return position.clone();
                    }
                }
            }
            return self
                .fix_positions
                .get(&code)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        serde_json::Value::Null
    }

    fn variation_for_nav_ref(&self, nav_ref: &serde_json::Value) -> serde_json::Value {
        if let Some(key) = arinc_navaid_key_from_nav_ref(nav_ref) {
            return self
                .arinc_navaid_variation
                .get(&key)
                .copied()
                .flatten()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
            return self
                .navaid_variation
                .get(&code.trim().to_ascii_uppercase())
                .copied()
                .flatten()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);
        }
        serde_json::Value::Null
    }
}

fn arinc_navaid_key_from_nav_ref(nav_ref: &serde_json::Value) -> Option<ArincNavaidKey> {
    let arinc = nav_ref.get("ArincNavaid")?.as_object()?;
    let key = ArincNavaidKey::new(
        arinc.get("identifier")?.as_str()?,
        arinc.get("icao_code")?.as_str()?,
        arinc.get("section_code")?.as_str()?,
        arinc.get("subsection_code")?.as_str()?,
    );
    key.is_complete().then_some(key)
}

fn load_nav_position_map(
    connection: &rusqlite::Connection,
    table: &str,
    lat_column: &str,
    lon_column: &str,
) -> anyhow::Result<BTreeMap<String, serde_json::Value>> {
    let mut stmt = connection.prepare(&format!(
        "
        SELECT trim(LocationID), CAST({lat_column} AS REAL), CAST({lon_column} AS REAL)
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?.trim().to_ascii_uppercase(),
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, lat, lon) = row?;
        map.entry(id)
            .or_insert_with(|| serde_json::json!({ "lat": lat, "lon": lon }));
    }
    Ok(map)
}

fn load_arinc_navaid_position_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<ArincNavaidKey, serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
          AND trim(subsection_code) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            ArincNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ),
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, lat, lon) = row?;
        map.entry(key)
            .or_insert_with(|| serde_json::json!({ "lat": lat, "lon": lon }));
    }
    Ok(map)
}

fn load_arinc_navaid_variation_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<ArincNavaidKey, Option<f64>>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(Variation AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
          AND trim(subsection_code) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            ArincNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ),
            row.get::<_, Option<f64>>(4)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, variation) = row?;
        map.entry(key).or_insert(variation);
    }
    Ok(map)
}

fn position_lookup_key(lat: f64, lon: f64) -> (i64, i64) {
    (
        (lat * 1_000_000.0).round() as i64,
        (lon * 1_000_000.0).round() as i64,
    )
}

fn build_position_lookup(
    positions: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<(i64, i64), String> {
    let mut lookup = BTreeMap::new();
    for (id, position) in positions {
        let Some(lat) = position.get("lat").and_then(|value| value.as_f64()) else {
            continue;
        };
        let Some(lon) = position.get("lon").and_then(|value| value.as_f64()) else {
            continue;
        };
        lookup
            .entry(position_lookup_key(lat, lon))
            .or_insert_with(|| id.clone());
    }
    lookup
}

fn load_runway_position_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<(String, String), serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(LEIdent), CAST(LELatitude AS REAL), CAST(LELongitude AS REAL),
               trim(HEIdent), CAST(HELatitude AS REAL), CAST(HELongitude AS REAL)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (airport_id, le_ident, le_lat, le_lon, he_ident, he_lat, he_lon) = row?;
        let airport_id = airport_id.trim().to_ascii_uppercase();
        for (ident, lat, lon) in [(le_ident, le_lat, le_lon), (he_ident, he_lat, he_lon)] {
            let ident = ident.trim();
            if ident.is_empty() {
                continue;
            }
            map.insert(
                (
                    airport_id.clone(),
                    format!("RW{}", ident.to_ascii_uppercase()),
                ),
                serde_json::json!({ "lat": lat, "lon": lon }),
            );
        }
    }
    Ok(map)
}

fn load_variation_map(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    airport_format: bool,
) -> anyhow::Result<BTreeMap<String, Option<f64>>> {
    let mut stmt = connection.prepare(&format!(
        "
        SELECT trim(LocationID), trim({column})
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?.trim().to_ascii_uppercase(),
            row.get::<_, String>(1)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, raw) = row?;
        let variation = if airport_format {
            parse_nav_kv_airport_magnetic_variation(&raw)
        } else {
            raw.trim().parse::<f64>().ok()
        };
        map.entry(id).or_insert(variation);
    }
    Ok(map)
}

fn parse_nav_kv_cifp_tenths_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    Some(parsed / 10.0)
}

fn parse_nav_kv_cifp_altitude_ft(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

fn parse_nav_kv_airport_magnetic_variation(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (magnitude_text, suffix) = trimmed.split_at(trimmed.len().saturating_sub(1));
    match suffix {
        "E" => magnitude_text.parse::<f64>().ok(),
        "W" => magnitude_text.parse::<f64>().ok().map(|degrees| -degrees),
        _ => trimmed.parse::<f64>().ok(),
    }
}

fn non_empty_json_string(value: String) -> serde_json::Value {
    if value.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(value)
    }
}

fn build_nav_kv_airway_pairs(connection: &rusqlite::Connection) -> anyhow::Result<Vec<NavKvPair>> {
    let nav_context = NavLookupContext::load(connection)?;
    let mut stmt = connection.prepare(
        "
        SELECT trim(name), trim(branch_key), CAST(sequence_number AS INTEGER),
               trim(point_name), Latitude, Longitude
        FROM airways_branch
        WHERE trim(name) <> ''
        ORDER BY trim(name), trim(branch_key), CAST(sequence_number AS INTEGER)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut branch_points = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut spatial_points = BTreeMap::<(i32, i32), Vec<serde_json::Value>>::new();
    for row in rows {
        let (name, branch_key, sequence, point_name, lat, lon) = row?;
        let position = serde_json::json!({ "lat": lat, "lon": lon });
        let nav_ref = nav_context.classify_airway_point_json(&point_name, lat, lon);
        let point = serde_json::json!({
            "airway_name": name,
            "sequence": sequence,
            "position": position,
            "nav_ref": nav_ref.clone(),
        });
        branch_points
            .entry((name.clone(), branch_key.clone()))
            .or_default()
            .push(point);
        let spatial_point = serde_json::json!({
            "airway_name": name,
            "branch_key": branch_key,
            "sequence": sequence,
            "position": { "lat": lat, "lon": lon },
            "nav_ref": nav_ref,
        });
        spatial_points
            .entry((lat.floor() as i32, lon.floor() as i32))
            .or_default()
            .push(spatial_point);
    }

    let mut branches_by_airway = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for ((name, branch_key), points) in branch_points {
        branches_by_airway
            .entry(name.clone())
            .or_default()
            .push(serde_json::json!({
                "display_name": name,
                "branch_key": branch_key,
                "points": points,
            }));
    }

    let mut pairs = Vec::new();
    for (airway_name, branches) in branches_by_airway {
        pairs.push(json_pair(
            format!("airway/{}", had_upper_key_component(&airway_name)),
            &serde_json::Value::Array(branches),
            "airway branches",
        )?);
    }
    for ((lat_tile, lon_tile), points) in spatial_points {
        pairs.push(json_pair(
            format!("airway/spatial/{lat_tile}/{lon_tile}"),
            &serde_json::Value::Array(points),
            "airway spatial tile",
        )?);
    }
    Ok(pairs)
}

#[derive(Clone, Copy)]
struct AirportRunwaySymbolInfo {
    length_ft: f64,
    heading_true_deg: f64,
    has_paved_runway: bool,
    has_water_runway: bool,
}

fn airport_runway_symbol_info_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, AirportRunwaySymbolInfo>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(Length), trim(Surface), trim(LEHeadingT),
               trim(LELatitude), trim(LELongitude), trim(HELatitude), trim(HELongitude)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, AirportRunwaySymbolInfo>::new();
    for row in rows {
        let (airport_id, length, surface, heading, le_lat, le_lon, he_lat, he_lon) = row?;
        let length = parse_float(&length);
        if length <= 0.0 {
            continue;
        }
        let surface = surface.trim().to_ascii_uppercase();
        let has_paved_runway = surface_is_paved(&surface);
        let has_water_runway = surface.contains("WATER");
        let heading = parse_float(&heading);
        let heading = if heading > 0.0 {
            normalize_heading(heading)
        } else {
            let le_lat = parse_float(&le_lat);
            let le_lon = parse_float(&le_lon);
            let he_lat = parse_float(&he_lat);
            let he_lon = parse_float(&he_lon);
            if !valid_lat_lon(le_lat, le_lon) || !valid_lat_lon(he_lat, he_lon) {
                continue;
            }
            bearing_true_deg(le_lat, le_lon, he_lat, he_lon)
        };
        let key = airport_id.trim().to_ascii_uppercase();
        match by_airport.get_mut(&key) {
            Some(existing) if existing.length_ft >= length => {
                existing.has_paved_runway |= has_paved_runway;
                existing.has_water_runway |= has_water_runway;
            }
            _ => {
                by_airport.insert(
                    key,
                    AirportRunwaySymbolInfo {
                        length_ft: length,
                        heading_true_deg: heading,
                        has_paved_runway,
                        has_water_runway,
                    },
                );
            }
        }
    }
    Ok(by_airport)
}

fn json_pair(key: String, value: &serde_json::Value, context: &str) -> anyhow::Result<NavKvPair> {
    Ok(NavKvPair {
        key,
        value: serde_json::to_vec(value)
            .with_context(|| format!("failed to encode nav_kv {context} value"))?,
    })
}

fn nav_kv_plate_asset(
    airport_id: &str,
    plate: &preprocessor_resource_index::PlateRecord,
) -> serde_json::Value {
    let filename = plate
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&plate.asset_path);
    let thumbnail_path = non_empty_string(&plate.thumbnail_path);
    serde_json::json!({
        "id": format!("plate:{airport_id}:{filename}"),
        "airport_id": airport_id,
        "package_id": plate.package_id,
        "label": plate.label,
        "kind": "plate",
        "folder_category": folder_category_for_document_type(&plate.document_type),
        "source_asset_path": plate.asset_path,
        "asset_path": plate.asset_path,
        "asset_url": format!("/{}", plate.asset_path),
        "thumbnail_source_path": thumbnail_path,
        "thumbnail_path": thumbnail_path,
        "thumbnail_url": thumbnail_path.map(|path| format!("/{path}")),
        "georef": plate.georef,
    })
}

fn nav_kv_csup_asset(
    airport_id: &str,
    csup: &preprocessor_resource_index::CsupRecord,
) -> serde_json::Value {
    let filename = csup
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&csup.asset_path);
    let thumbnail_path = non_empty_string(&csup.thumbnail_path);
    serde_json::json!({
        "id": format!("csup:{airport_id}:{filename}"),
        "airport_id": airport_id,
        "package_id": csup.package_id,
        "label": csup.label,
        "kind": "csup",
        "folder_category": "csup",
        "source_asset_path": csup.asset_path,
        "asset_path": csup.asset_path,
        "asset_url": format!("/{}", csup.asset_path),
        "thumbnail_source_path": thumbnail_path,
        "thumbnail_path": thumbnail_path,
        "thumbnail_url": thumbnail_path.map(|path| format!("/{path}")),
        "georef": serde_json::Value::Null,
    })
}

fn non_empty_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn had_upper_key_component(value: &str) -> String {
    had_key_component(&value.trim().to_ascii_uppercase())
}

fn had_key_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.trim().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

fn arinc_navaid_had_key(
    identifier: &str,
    icao_code: &str,
    section_code: &str,
    subsection_code: &str,
) -> String {
    [
        had_upper_key_component(section_code),
        had_upper_key_component(subsection_code),
        had_upper_key_component(icao_code),
        had_upper_key_component(identifier),
    ]
    .join("/")
}

fn airport_display_label(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.len() == 4 && trimmed.starts_with('K') {
        trimmed[1..].to_ascii_uppercase()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn navaid_display_label(id: &str, facility_name: &str) -> String {
    let frequency = facility_name
        .split_whitespace()
        .last()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(frequency) = frequency {
        format!("{} {frequency}", id.trim()).to_ascii_uppercase()
    } else {
        id.trim().to_ascii_uppercase()
    }
}

fn titlecase_nav_label(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut normalized = first.to_uppercase().collect::<String>();
                    normalized.push_str(&chars.as_str().to_ascii_lowercase());
                    normalized
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn runway_length_ratio(longest_runway_length_ft: Option<f64>) -> f64 {
    (longest_runway_length_ft.unwrap_or(0.0) / 5000.0).clamp(0.0, 1.0)
}

fn surface_is_paved(surface: &str) -> bool {
    surface
        .split('-')
        .any(|part| matches!(part.trim(), "ASPH" | "CONC" | "BIT" | "PEM"))
}

fn parse_float(value: &str) -> f64 {
    value.trim().parse::<f64>().unwrap_or(0.0)
}

fn valid_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
}

fn bearing_true_deg(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64) -> f64 {
    let start_lat_rad = start_lat.to_radians();
    let end_lat_rad = end_lat.to_radians();
    let delta_lon_rad = (end_lon - start_lon).to_radians();
    let y = delta_lon_rad.sin() * end_lat_rad.cos();
    let x = start_lat_rad.cos() * end_lat_rad.sin()
        - start_lat_rad.sin() * end_lat_rad.cos() * delta_lon_rad.cos();
    normalize_heading(y.atan2(x).to_degrees())
}

fn normalize_heading(heading: f64) -> f64 {
    let normalized = heading.rem_euclid(360.0);
    if normalized == 0.0 {
        360.0
    } else {
        normalized
    }
}

fn folder_category_for_document_type(document_type: &str) -> &'static str {
    match document_type {
        "airport_diagram" => "airport-diagram",
        "takeoff_minimums" | "alternate_minimums" | "minimums" => "takeoff-mins",
        "departure" => "departure",
        "star" => "star",
        "csup" => "csup",
        "hotspot" => "hotspot",
        "other" => "other",
        "approach" => "approach",
        _ => "other",
    }
}

fn folder_category_rank(category: &str) -> usize {
    match category {
        "approach" => 0,
        "departure" => 1,
        "star" => 2,
        "airport-diagram" => 3,
        "csup" => 4,
        "takeoff-mins" => 5,
        "other" => 6,
        "hotspot" => 7,
        _ => 8,
    }
}

fn family_display_name(resource_index: &ResourceIndex, family_id: &str) -> String {
    resource_index
        .families
        .iter()
        .find(|family| family.id == family_id)
        .map(|family| family.display_name.clone())
        .unwrap_or_else(|| family_id.to_string())
}

fn region_display_name(resource_index: &ResourceIndex, region_id: &str) -> String {
    resource_index
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .map(|region| region.display_name.clone())
        .unwrap_or_else(|| region_id.to_ascii_uppercase())
}

fn min_zoom_for_levels(collection: &preprocessor_resource_index::ChartCollectionRecord) -> f64 {
    let min_level = collection
        .levels
        .iter()
        .map(|level| level.zoom)
        .min()
        .unwrap_or(0);
    (min_level as f64 - 2.8).max(1.5)
}

fn max_zoom_for_levels(_collection: &preprocessor_resource_index::ChartCollectionRecord) -> f64 {
    RASTER_BASEMAP_MAX_DISPLAY_ZOOM
}

fn resolve_bundle_package_source_path(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    package: &preprocessor_resource_index::ResourcePackage,
) -> anyhow::Result<PathBuf> {
    let region_id = package.region_id.to_ascii_lowercase();
    let node_name = match package.family_id.as_str() {
        "csup" => format!("csup-package-{region_id}"),
        "tpp" => format!("tpp-{region_id}-package"),
        family_id => format!("charts-{family_id}-package-{region_id}"),
    };
    let record = build_manifest
        .nodes
        .iter()
        .find(|node| node.name == node_name)
        .with_context(|| format!("build manifest missing package node {node_name}"))?;
    Ok(resolve_artifact_path(config, output_path(record, "zip")?))
}

fn output_path<'a>(record: &'a NodeRecord, key: &str) -> anyhow::Result<&'a str> {
    record
        .outputs
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("node {} missing outputs.{key}", record.name))
}

fn resolve_artifact_path(config: &ProductBuildConfig, relative_path: &str) -> PathBuf {
    artifact_root_from_build_root(&config.build_root).join(relative_path)
}

fn published_unpacked_root(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    published_unpacked_root_from_build_root(&config.build_root)
}

fn internal_build_manifest_path(
    config: &ProductBuildConfig,
    bundle_cycle: &str,
) -> anyhow::Result<PathBuf> {
    let artifact_root = artifact_root_from_build_root(&config.build_root);
    let build_root_name = config
        .build_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("build root has no final path component"))?;
    let dir = artifact_root
        .join("private-work")
        .join("build-manifests")
        .join(build_root_name);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir.join(format!("build-manifest_{bundle_cycle}.json")))
}

pub fn published_unpacked_root_from_build_root(build_root: &Path) -> anyhow::Result<PathBuf> {
    let artifact_root = artifact_root_from_build_root(build_root);
    let unpacked_dir_name = match build_root.file_name().and_then(|name| name.to_str()) {
        Some("published-packaged") => "published-unpacked",
        Some("published-packaged-validation") => "published-unpacked-validation",
        Some(other) => {
            return Err(anyhow::anyhow!(
                "unsupported build root for unpacked publication: {}",
                other
            ))
        }
        None => return Err(anyhow::anyhow!("build root has no final path component")),
    };
    Ok(artifact_root.join(unpacked_dir_name))
}

fn unpacked_target_dir(unpacked_root: &Path, published_filename: &str) -> anyhow::Result<PathBuf> {
    let stem = Path::new(published_filename)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("failed to derive unpacked target from {published_filename}")
        })?;
    Ok(unpacked_root.join(stem))
}

fn unpacked_marker_path(unpacked_root: &Path, published_filename: &str) -> anyhow::Result<PathBuf> {
    let artifact_root = artifact_root_from_build_root(unpacked_root);
    let unpacked_dir_name = unpacked_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("unpacked root has no final path component"))?;
    let marker_dir = artifact_root
        .join("private-work")
        .join("published-unpacked-state")
        .join(unpacked_dir_name);
    fs::create_dir_all(&marker_dir)
        .with_context(|| format!("failed to create {}", marker_dir.display()))?;
    Ok(marker_dir.join(format!("{published_filename}.source-zip-sha256")))
}

fn sync_unpacked_zip_from_source(
    zip_path: &Path,
    source_root: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: Option<&str>,
) -> anyhow::Result<(bool, PathBuf)> {
    let unpack_dir = unpacked_target_dir(unpacked_root, published_filename)?;
    let marker_path = unpacked_marker_path(unpacked_root, published_filename)?;
    let zip_sha256 = match known_sha256 {
        Some(value) => value.to_string(),
        None => hash_file(zip_path)?,
    };
    if unpack_dir.is_dir()
        && fs::read_to_string(&marker_path)
            .ok()
            .as_deref()
            .map(str::trim)
            == Some(zip_sha256.as_str())
    {
        return Ok((true, unpack_dir));
    }
    if unpack_dir.exists() {
        fs::remove_dir_all(&unpack_dir)
            .with_context(|| format!("failed to remove {}", unpack_dir.display()))?;
    }
    fs::create_dir_all(&unpack_dir)
        .with_context(|| format!("failed to create {}", unpack_dir.display()))?;
    hardlink_zip_members_from_source_root(zip_path, source_root, &unpack_dir)?;
    fs::write(&marker_path, format!("{zip_sha256}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))?;
    Ok((false, unpack_dir))
}

fn sync_unpacked_dir_from_existing(
    source_dir: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: &str,
) -> anyhow::Result<(bool, PathBuf)> {
    let unpack_dir = unpacked_target_dir(unpacked_root, published_filename)?;
    let marker_path = unpacked_marker_path(unpacked_root, published_filename)?;
    if unpack_dir.is_dir()
        && fs::read_to_string(&marker_path)
            .ok()
            .as_deref()
            .map(str::trim)
            == Some(known_sha256)
    {
        return Ok((true, unpack_dir));
    }
    if unpack_dir.exists() {
        fs::remove_dir_all(&unpack_dir)
            .with_context(|| format!("failed to remove {}", unpack_dir.display()))?;
    }
    hardlink_dir_recursive(source_dir, &unpack_dir)?;
    fs::write(&marker_path, format!("{known_sha256}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))?;
    Ok((false, unpack_dir))
}

fn sync_unpacked_zip_by_extract(
    zip_path: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: Option<&str>,
) -> anyhow::Result<(bool, PathBuf)> {
    let unpack_dir = unpacked_target_dir(unpacked_root, published_filename)?;
    let marker_path = unpacked_marker_path(unpacked_root, published_filename)?;
    let zip_sha256 = match known_sha256 {
        Some(value) => value.to_string(),
        None => hash_file(zip_path)?,
    };
    if unpack_dir.is_dir()
        && fs::read_to_string(&marker_path)
            .ok()
            .as_deref()
            .map(str::trim)
            == Some(zip_sha256.as_str())
    {
        return Ok((true, unpack_dir));
    }
    if unpack_dir.exists() {
        fs::remove_dir_all(&unpack_dir)
            .with_context(|| format!("failed to remove {}", unpack_dir.display()))?;
    }
    fs::create_dir_all(&unpack_dir)
        .with_context(|| format!("failed to create {}", unpack_dir.display()))?;
    let file =
        File::open(zip_path).with_context(|| format!("failed to open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", zip_path.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).with_context(|| {
            format!(
                "failed to read zip member #{index} from {}",
                zip_path.display()
            )
        })?;
        let member = entry.name().to_string();
        let outpath = unpack_dir.join(&member);
        if member.ends_with('/') || entry.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("failed to create {}", outpath.display()))?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut out = File::create(&outpath)
            .with_context(|| format!("failed to create {}", outpath.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("failed to extract {}", outpath.display()))?;
    }
    fs::write(&marker_path, format!("{zip_sha256}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))?;
    Ok((false, unpack_dir))
}

fn hardlink_dir_recursive(source_dir: &Path, output_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let mut entries = fs::read_dir(source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", source_dir.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let source = entry.path();
        let output = output_dir.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", source.display()))?;
        if file_type.is_dir() {
            hardlink_dir_recursive(&source, &output)?;
        } else if file_type.is_file() {
            fs::hard_link(&source, &output).with_context(|| {
                format!(
                    "failed to hardlink {} to {}",
                    source.display(),
                    output.display()
                )
            })?;
        }
    }
    Ok(())
}

fn hardlink_zip_members_from_source_root(
    zip_path: &Path,
    source_root: &Path,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let file =
        File::open(zip_path).with_context(|| format!("failed to open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", zip_path.display()))?;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).with_context(|| {
            format!(
                "failed to read zip member #{index} from {}",
                zip_path.display()
            )
        })?;
        let member = entry.name().to_string();
        let outpath = output_dir.join(&member);
        if member.ends_with('/') || entry.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("failed to create {}", outpath.display()))?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let source = resolve_source_member_path(source_root, &member)?;
        if !source.is_file() {
            bail!(
                "missing source member {} for {} under {}",
                member,
                zip_path.display(),
                source_root.display()
            );
        }
        fs::hard_link(&source, &outpath).with_context(|| {
            format!(
                "failed to hardlink {} to {}",
                source.display(),
                outpath.display()
            )
        })?;
    }
    Ok(())
}

fn resolve_source_member_path(source_root: &Path, member: &str) -> anyhow::Result<PathBuf> {
    let direct = source_root.join(member);
    if direct.is_file() {
        return Ok(direct);
    }

    let member_path = Path::new(member);
    if member_path.components().count() == 1 && member_path.extension().is_none() {
        let mut manifests = fs::read_dir(source_root)
            .with_context(|| format!("failed to read {}", source_root.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to iterate {}", source_root.display()))?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("manifest"))
            .collect::<Vec<_>>();
        manifests.sort();
        if manifests.len() == 1 {
            return Ok(manifests.remove(0));
        }
    }

    Ok(direct)
}

fn sync_unpacked_metadata(
    config: &ProductBuildConfig,
    bundle_manifest: &BundleManifest,
    bundle_manifest_path: &Path,
    task_values: Option<&BTreeMap<String, ProductTaskValue>>,
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root(config)?;
    remove_legacy_unpacked_subtree(&unpacked_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(bundle_manifest_path, &unpacked_root)?;
    for artifact in &bundle_manifest.ancillary {
        if artifact.filename.ends_with(".zip") {
            continue;
        }
        sync_unpacked_file(&config.build_root.join(&artifact.filename), &unpacked_root)?;
    }
    sync_cycle_bundle_unpacked_zips(config, bundle_manifest, &unpacked_root, task_values)?;
    Ok(())
}

fn sync_cycle_bundle_unpacked_zips(
    config: &ProductBuildConfig,
    bundle_manifest: &BundleManifest,
    unpacked_root: &Path,
    task_values: Option<&BTreeMap<String, ProductTaskValue>>,
) -> anyhow::Result<()> {
    let build_manifest = if task_values.is_none() {
        let path = internal_build_manifest_path(config, &bundle_manifest.cycle)?;
        Some(
            serde_json::from_slice::<BuildManifest>(
                &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", path.display()))?,
        )
    } else {
        None
    };
    for package in &bundle_manifest.packages {
        if package.family_id == "nav-db" {
            let cycle = package
                .cycle
                .as_deref()
                .context("nav-db package missing cycle")?;
            let source_dir = task_values
                .and_then(|values| values.get(&format!("{cycle}:nav-db")))
                .and_then(|value| match value {
                    ProductTaskValue::FingerprintedZip { zip, .. } => {
                        Some(zip.parent().map(|parent| parent.join("nav_db")))
                    }
                    _ => None,
                })
                .flatten()
                .unwrap_or_else(|| {
                    artifact_root_from_build_root(&config.build_root)
                        .join("private-work")
                        .join("nav-kv")
                        .join(config.profile.as_str())
                        .join(cycle)
                });
            sync_unpacked_zip_from_source(
                &config.build_root.join(&package.filename),
                &source_dir,
                unpacked_root,
                &package.filename,
                Some(&package.checksum_sha256),
            )
            .with_context(|| format!("failed to unpack package {}", package.id))?;
            continue;
        }
        if package.cycle.is_none() {
            continue;
        }
        if let Some(cycle) = &package.cycle {
            let legacy_filename = format!(
                "{}_{}_{}.zip",
                package.family_id.replace('-', "_"),
                package
                    .region_id
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                cycle
            );
            let legacy_dir = unpacked_target_dir(unpacked_root, &legacy_filename)?;
            let legacy_marker_path = unpacked_marker_path(unpacked_root, &legacy_filename)?;
            if legacy_dir.is_dir()
                && fs::read_to_string(&legacy_marker_path)
                    .ok()
                    .as_deref()
                    .map(str::trim)
                    == Some(package.checksum_sha256.as_str())
            {
                sync_unpacked_dir_from_existing(
                    &legacy_dir,
                    unpacked_root,
                    &package.filename,
                    &package.checksum_sha256,
                )
                .with_context(|| {
                    format!(
                        "failed to sync hashed unpacked package {} from existing {}",
                        package.id,
                        legacy_dir.display()
                    )
                })?;
                continue;
            }
        }
        let package_root = if let Some(task_values) = task_values {
            resolve_cycle_bundle_package_root(task_values, &bundle_manifest.cycle, package)?
        } else {
            resolve_cycle_bundle_package_root_from_build_manifest(
                config,
                build_manifest
                    .as_ref()
                    .expect("build manifest fallback should exist for standalone cycle unpack"),
                package,
            )?
        }
        .with_context(|| format!("failed to resolve source root for package {}", package.id))?;
        sync_unpacked_zip_from_source(
            &config.build_root.join(&package.filename),
            &package_root,
            unpacked_root,
            &package.filename,
            Some(&package.checksum_sha256),
        )
        .with_context(|| format!("failed to unpack package {}", package.id))?;
    }
    Ok(())
}

fn resolve_cycle_bundle_package_root_from_build_manifest(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    package: &BundlePackageArtifact,
) -> anyhow::Result<Option<PathBuf>> {
    if package.cycle.is_none() {
        return Ok(None);
    }
    let region_id = package
        .region_id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let node_name = match package.family_id.as_str() {
        "csup" => format!("csup-package-{region_id}"),
        "tpp" => format!("tpp-{region_id}-package"),
        "sec" | "tac" | "enr-l" | "enr-h" => {
            format!("charts-{}-package-{region_id}", package.family_id)
        }
        "vectors" => "vectors".to_string(),
        _ => return Ok(None),
    };
    let record = match build_manifest
        .nodes
        .iter()
        .find(|node| node.name == node_name)
    {
        Some(record) => record,
        None => return Ok(None),
    };
    let root = match package.family_id.as_str() {
        "tpp" => record
            .outputs
            .get("package_root")
            .map(|path| resolve_artifact_path(config, path))
            .or_else(|| {
                record
                    .outputs
                    .get("zip")
                    .map(|path| resolve_artifact_path(config, path))
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            }),
        _ => record
            .outputs
            .get("zip")
            .map(|path| resolve_artifact_path(config, path))
            .and_then(|path| path.parent().map(Path::to_path_buf)),
    };
    Ok(root)
}

fn resolve_cycle_bundle_package_root(
    task_values: &BTreeMap<String, ProductTaskValue>,
    bundle_cycle: &str,
    package: &BundlePackageArtifact,
) -> anyhow::Result<Option<PathBuf>> {
    fn task_id(cycle: &str, name: &str) -> String {
        format!("{cycle}:{name}")
    }

    if package.cycle.is_none() {
        return Ok(None);
    }
    let region_id = package
        .region_id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let task_id = match package.family_id.as_str() {
        "csup" => task_id(bundle_cycle, "csup-package"),
        "tpp" => task_id(bundle_cycle, &format!("tpp-{region_id}-package")),
        "sec" | "tac" | "enr-l" | "enr-h" => task_id(
            bundle_cycle,
            &format!("charts-{}-package", package.family_id),
        ),
        "vectors" => task_id(bundle_cycle, "vectors"),
        _ => return Ok(None),
    };

    let root = match task_values.get(&task_id) {
        Some(ProductTaskValue::ChartSource(source)) => source.package_root.clone(),
        Some(ProductTaskValue::CsupSource(source)) => source.package_root.clone(),
        Some(ProductTaskValue::FingerprintedTppSource { source, .. }) => {
            source.package_root.clone()
        }
        Some(ProductTaskValue::FingerprintedZip { zip, .. }) => {
            zip.parent().unwrap_or_else(|| Path::new("/")).to_path_buf()
        }
        _ => return Ok(None),
    };
    Ok(Some(root))
}

fn sync_unpacked_file(source_path: &Path, unpacked_root: &Path) -> anyhow::Result<()> {
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("failed to determine filename for {}", source_path.display())
        })?;
    let published_path = unpacked_root.join(filename);
    publish_flat_artifact(source_path, &published_path)
}

fn sync_unpacked_discovery_manifests(
    packaged_root: &Path,
    current_artifacts_path: &Path,
    unpacked_root: &Path,
) -> anyhow::Result<()> {
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        sync_unpacked_file(&discovery_path, unpacked_root)?;
    }
    Ok(())
}

fn sync_product_level_unpacked(
    build_root: &Path,
    current_artifacts_path: &Path,
    zip_artifacts: &[PublishedZipArtifact],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_build_root(build_root)?;
    let packaged_root = build_root.join("published-packaged");
    remove_legacy_unpacked_subtree(&unpacked_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(build_root, current_artifacts_path, &unpacked_root)?;
    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;
    if let Some(diagnostics) = &current.diagnostics {
        sync_unpacked_file(&build_root.join(&diagnostics.filename), &unpacked_root)?;
    }
    for artifact in zip_artifacts {
        let published_filename = artifact
            .published_zip_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("failed to determine published filename"))?;
        let source_root = artifact
            .source_zip_path
            .parent()
            .unwrap_or_else(|| Path::new("/"));
        if source_root == packaged_root {
            let unpack_dir = unpacked_target_dir(&unpacked_root, published_filename)?;
            let marker_path = unpacked_marker_path(&unpacked_root, published_filename)?;
            let marker_matches = fs::read_to_string(&marker_path)
                .ok()
                .as_deref()
                .map(str::trim)
                == Some(artifact.checksum_sha256.as_str());
            if unpack_dir.is_dir() && marker_matches {
                continue;
            }
            bail!(
                "missing unpacked source tree for preserved published package {}",
                artifact.published_zip_path.display()
            );
        }
        sync_unpacked_zip_from_source(
            &artifact.published_zip_path,
            source_root,
            &unpacked_root,
            published_filename,
            Some(&artifact.checksum_sha256),
        )?;
    }
    cleanup_published_unpacked_root(&unpacked_root, current_artifacts_path)?;
    Ok(())
}

fn product_cycles_to_build(config: &ProductBuildConfig) -> anyhow::Result<Vec<String>> {
    if let Some(cycle) = &config.target_cycle {
        return Ok(vec![cycle.clone()]);
    }
    let as_of_date = Utc::now().date_naive();
    let mut cycles = discover_published_cycles(Some(&FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    }))?
    .into_iter()
    .filter(|cycle| match cycle_effective_date(cycle) {
        Ok(effective) => effective + chrono::Duration::days(28) >= as_of_date,
        Err(_) => false,
    })
    .collect::<Vec<_>>();
    cycles.sort();
    cycles.dedup();
    if cycles.is_empty() {
        anyhow::bail!("no published FAA cycles are currently buildable");
    }
    Ok(cycles)
}

fn build_tfrs_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let artifact_root = artifact_root_from_build_root(&config.build_root).to_path_buf();
    let generated_at_utc = Utc::now()
        .with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid");
    let version_label = generated_at_utc.format("%Y%m%dT%H%MZ").to_string();
    let build_root = artifact_root
        .join("private-work")
        .join("tfrs")
        .join(&version_label);
    let input_dir = build_root.join("input");

    if build_root.exists() {
        fs::remove_dir_all(&build_root)
            .with_context(|| format!("failed to clear {}", build_root.display()))?;
    }
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;

    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    let provenance_dir = build_root.join("meta").join("provenance").join("tfrs");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;

    let list_url = "https://tfr.faa.gov/tfrapi/exportTfrList";
    let graphics_url = concat!(
        "https://tfr.faa.gov/geoserver/TFR/ows?",
        "service=WFS&version=1.1.0&request=GetFeature&typeName=TFR:V_TFR_LOC&",
        "maxFeatures=300&outputFormat=application/json&srsname=EPSG:4326"
    );
    let source_requests = vec![
        PrefetchRequest::new(list_url)
            .with_logical_file_name("list.json")
            .with_http1(),
        PrefetchRequest::new(graphics_url)
            .with_logical_file_name("graphics.geojson")
            .with_http1(),
    ];
    let mut source_urls_jsonl = String::new();
    for request in &source_requests {
        source_urls_jsonl.push_str(&format!(
            "{{\"event\":\"source_url\",\"http_version\":\"1.1\",\"label\":\"tfrs\",\"url\":\"{}\"}}\n",
            request.url
        ));
    }
    fs::write(provenance_dir.join("source_urls.jsonl"), source_urls_jsonl).with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_requests_with_provenance(
        &source_requests,
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "tfrs",
    )?;

    let source_fingerprint = hash_tree(&input_dir)?;
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = fast_product_node_inputs("tfrs", &source_fingerprint)?;
    let build_version_label = version_label.clone();
    run_fast_structured_product_node(
        config,
        "tfrs",
        "fast-tfrs",
        &version_label,
        inputs,
        move |output_dir| {
            let result = build_tfr_dataset(&BuildTfrRequest {
                input_dir,
                output_dir,
                version_label: build_version_label,
                generated_at_utc,
            })?;
            Ok(FastStructuredProductOutputs {
                manifest_path: result.manifest_path,
                structured_json_path: result.structured_json_path,
                zip_path: result.zip_path,
            })
        },
    )
}

fn build_metars_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let artifact_root = artifact_root_from_build_root(&config.build_root).to_path_buf();
    let generated_at_utc = Utc::now()
        .with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid");
    let version_label = generated_at_utc.format("%Y%m%dT%H%MZ").to_string();
    let build_root = artifact_root
        .join("private-work")
        .join("metars")
        .join(&version_label);
    let input_dir = build_root.join("input");

    if build_root.exists() {
        fs::remove_dir_all(&build_root)
            .with_context(|| format!("failed to clear {}", build_root.display()))?;
    }
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;

    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    let provenance_dir = build_root.join("meta").join("provenance").join("metars");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;

    let url =
        "https://aviationweather.gov/data/cache/metars.cache.xml.gz#logical_name=metars.cache.xml.gz"
            .to_string();
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"metars\",\"url\":\"{}\"}}\n",
            url
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_archives_with_provenance(
        std::slice::from_ref(&url),
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "metars",
    )?;

    let gz_path = input_dir.join("metars.cache.xml.gz");
    run_status_command("gzip", &["-d", gz_path.to_str().unwrap()])?;
    let input_xml_path = input_dir.join("metars.cache.xml");
    let source_fingerprint = hash_tree(&input_dir)?;
    let content_fingerprint = metar_content_fingerprint(&input_xml_path)?;
    let version_label = fast_product_version_label(&content_fingerprint);
    let inputs = fast_product_node_inputs("metars", &source_fingerprint)?;
    let build_version_label = version_label.clone();
    run_fast_structured_product_node(
        config,
        "metars",
        "fast-metars",
        &version_label,
        inputs,
        move |output_dir| {
            let result = build_metar_dataset(&BuildMetarRequest {
                input_xml_path,
                output_dir,
                version_label: build_version_label,
                generated_at_utc,
            })?;
            Ok(FastStructuredProductOutputs {
                manifest_path: result.manifest_path,
                structured_json_path: result.structured_json_path,
                zip_path: result.zip_path,
            })
        },
    )
}

fn build_nexrad_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let artifact_root = artifact_root_from_build_root(&config.build_root).to_path_buf();
    let generated_at_utc = Utc::now()
        .with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid");
    let version_label = generated_at_utc.format("%Y%m%dT%H%MZ").to_string();
    let build_root = artifact_root
        .join("private-work")
        .join("nexrad")
        .join(&version_label);
    let input_dir = build_root.join("input");

    if build_root.exists() {
        fs::remove_dir_all(&build_root)
            .with_context(|| format!("failed to clear {}", build_root.display()))?;
    }
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;

    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    let provenance_dir = build_root.join("meta").join("provenance").join("nexrad");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;

    let index_url =
        "https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/#logical_name=index.html"
            .to_string();
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"nexrad-index\",\"url\":\"{}\"}}\n",
            index_url
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_archives_with_provenance(
        std::slice::from_ref(&index_url),
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "nexrad-index",
    )?;

    let listings = parse_nexrad_index_for_product(&input_dir.join("index.html"))?;
    if listings.len() < 11 {
        bail!(
            "expected at least 11 radar listings, found {}",
            listings.len()
        );
    }
    let selected_urls = [0usize, 5usize, 10usize]
        .into_iter()
        .map(|index| {
            let file_name = &listings[index];
            format!(
                "https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/{}#logical_name={}",
                file_name, file_name
            )
        })
        .collect::<Vec<_>>();
    let mut source_urls_jsonl =
        String::from("{\"event\":\"source_url\",\"label\":\"nexrad-index\",\"url\":\"https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/#logical_name=index.html\"}\n");
    for url in &selected_urls {
        source_urls_jsonl.push_str(&format!(
            "{{\"event\":\"source_url\",\"label\":\"nexrad-frame\",\"url\":\"{}\"}}\n",
            url
        ));
    }
    fs::write(provenance_dir.join("source_urls.jsonl"), source_urls_jsonl).with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_archives_with_provenance(
        &selected_urls,
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "nexrad-frame",
    )?;

    let source_fingerprint = hash_tree(&input_dir)?;
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = fast_product_node_inputs("nexrad", &source_fingerprint)?;
    let build_version_label = version_label.clone();
    run_fast_structured_product_node(
        config,
        "nexrad",
        "fast-nexrad",
        &version_label,
        inputs,
        move |output_dir| {
            let result = build_nexrad_dataset(&BuildNexradRequest {
                input_dir,
                output_dir,
                version_label: build_version_label,
                generated_at_utc,
            })?;
            Ok(FastStructuredProductOutputs {
                manifest_path: result.manifest_path,
                structured_json_path: result.structured_json_path,
                zip_path: result.zip_path,
            })
        },
    )
}

fn build_geo_product(config: &ProductBuildConfig) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let source_csv_path = static_geo_source_path();
    let source_fingerprint = hash_file(&source_csv_path)?;
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = fast_product_node_inputs("geo", &source_fingerprint)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-geo")?,
        "static-geo",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let csv_path = output_dir.join("geo.csv");
    let manifest_path = output_dir.join(format!("geo_{version_label}.manifest.json"));
    let zip_path = output_dir.join(format!("geo_{version_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[csv_path.clone(), manifest_path, zip_path.clone()],
    )? {
        NodeCacheState::CacheHit(record) => {
            return Ok((zip_path, version_label, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let generated_at_utc = Utc::now()
        .with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid");
    let result = build_geo_dataset(&BuildGeoRequest {
        source_csv_path,
        output_dir,
        version_label: version_label.clone(),
        generated_at_utc,
    })?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "csv".to_string(),
            relative_artifact_path(&result.csv_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((result.zip_path, version_label, record))
}

struct FastStructuredProductOutputs {
    manifest_path: PathBuf,
    structured_json_path: PathBuf,
    zip_path: PathBuf,
}

fn run_fast_structured_product_node<BuildProduct>(
    config: &ProductBuildConfig,
    product_id: &str,
    node_name: &str,
    version_label: &str,
    inputs: BTreeMap<String, String>,
    build_product: BuildProduct,
) -> anyhow::Result<(PathBuf, String, NodeRecord)>
where
    BuildProduct: FnOnce(PathBuf) -> anyhow::Result<FastStructuredProductOutputs>,
{
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, node_name)?,
        node_name,
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let structured_json_path = output_dir.join(format!("{product_id}.json"));
    let manifest_path = output_dir.join(format!("{product_id}_{version_label}.manifest.json"));
    let zip_path = output_dir.join(format!("{product_id}_{version_label}.zip"));
    let expected_outputs = [
        structured_json_path.clone(),
        manifest_path.clone(),
        zip_path.clone(),
    ];
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
        NodeCacheState::CacheHit(record) => {
            let source_generated_at_utc = fast_product_source_generated_at(
                product_id,
                &structured_json_path,
                &manifest_path,
            )?;
            return Ok((zip_path, source_generated_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_product(output_dir)?;
    let source_generated_at_utc = fast_product_source_generated_at(
        product_id,
        &result.structured_json_path,
        &result.manifest_path,
    )?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "structured_json".to_string(),
            relative_artifact_path(&result.structured_json_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((result.zip_path, source_generated_at_utc, record))
}

fn build_terrain_product(
    config: &ProductBuildConfig,
    region: Region,
    terrain_index_path: &Path,
    source_fetched_at_utc: Option<String>,
) -> anyhow::Result<(PathBuf, String, Option<String>, NodeRecord)> {
    let region_id = region.code().to_ascii_lowercase();
    let input_dir = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("terrain")
        .join(&region_id)
        .join("input");
    let dem_dir = input_dir.join("dems");
    fs::create_dir_all(&dem_dir)
        .with_context(|| format!("failed to create {}", dem_dir.display()))?;

    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("terrain-{region_id}"));
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let mut dem_candidates = terrain_dem_candidates_for_region(terrain_index_path, region)?;
    if dem_candidates.is_empty() {
        bail!(
            "terrain discovery index has no DEM URLs for {}",
            region.code()
        );
    }
    if let Some(cached_selection) = cached_terrain_dem_selection(&dem_candidates, &fetch_cache)? {
        let source_fingerprint = terrain_source_fingerprint_from_cached(
            &cached_selection.selection.urls,
            &cached_selection.sources,
            &cached_selection.selection.missing_cells,
        );
        let version_label = fast_product_version_label(&source_fingerprint);
        let inputs = terrain_product_inputs(region, &source_fingerprint)?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &format!("static-terrain-{region_id}"))?,
            &format!("static-terrain-{region_id}"),
            &inputs,
        )?;
        let output_dir = prepared.dir.join("output");
        let zip_path = output_dir.join(format!("terrain_{region_id}_{version_label}.zip"));
        let manifest_path = output_dir.join("manifest.json");
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path])? {
            return Ok((zip_path, version_label, source_fetched_at_utc, record));
        }
    }
    let dem_selection = prefetch_terrain_dems_with_fallback(
        &mut dem_candidates,
        &dem_dir,
        config.fetch_jobs,
        &fetch_cache,
        &provenance_dir,
        &format!("terrain-{region_id}-dem"),
    )?;
    let dem_paths = terrain_dem_paths_from_urls(&dem_dir, &dem_selection.urls)?;
    let source_fingerprint = if let Some(sources) =
        cached_terrain_dem_sources_for_urls(&fetch_cache, &dem_selection.urls)?
    {
        terrain_source_fingerprint_from_cached(
            &dem_selection.urls,
            &sources,
            &dem_selection.missing_cells,
        )
    } else {
        terrain_source_fingerprint(
            &dem_selection.urls,
            &dem_paths,
            &dem_selection.missing_cells,
        )?
    };
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = terrain_product_inputs(region, &source_fingerprint)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-terrain-{region_id}"))?,
        &format!("static-terrain-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let zip_path = output_dir.join(format!("terrain_{region_id}_{version_label}.zip"));
    let manifest_path = output_dir.join("manifest.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path])? {
        NodeCacheState::CacheHit(record) => {
            return Ok((zip_path, version_label, source_fetched_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let vrt_path = output_dir.join(format!("terrain_{region_id}.vrt"));
    build_terrain_vrt(&vrt_path, &dem_paths)?;
    build_terrain_region_tiles(
        region,
        &vrt_path,
        &static_geo_source_path(),
        &output_dir,
        &version_label,
        &dem_selection,
    )?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&output_dir.join("manifest.json"), &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((zip_path, version_label, source_fetched_at_utc, record))
}

fn terrain_product_inputs(
    region: Region,
    source_fingerprint: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        ("product_id".to_string(), format!("terrain-{region_id}")),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "terrain_pipeline".to_string(),
            TERRAIN_PIPELINE_VERSION.to_string(),
        ),
        ("geo_csv".to_string(), hash_file(static_geo_source_path())?),
    ]))
}

fn build_water_mask_product(
    config: &ProductBuildConfig,
    region: Region,
) -> anyhow::Result<(PathBuf, PathBuf, String, Option<String>, NodeRecord)> {
    let region_id = region.code().to_ascii_lowercase();
    let inputs = water_mask_product_inputs(region)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-water-mask-{region_id}"))?,
        &format!("static-water-mask-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    if let Some(record) = try_load_node_record(&prepared, &[manifest_path.clone()])? {
        let (source_version, source_fetched_at_utc) = water_mask_manifest_versions(&manifest_path)?;
        let zip_path = water_mask_record_zip_path(&prepared.dir, &record)?;
        return Ok((
            zip_path,
            output_dir.join("tiles"),
            source_version,
            source_fetched_at_utc,
            record,
        ));
    }
    let _build_lock = match claim_or_wait_for_node(&prepared, &[manifest_path.clone()])? {
        NodeCacheState::CacheHit(record) => {
            let (source_version, source_fetched_at_utc) =
                water_mask_manifest_versions(&manifest_path)?;
            let zip_path = water_mask_record_zip_path(&prepared.dir, &record)?;
            return Ok((
                zip_path,
                output_dir.join("tiles"),
                source_version,
                source_fetched_at_utc,
                record,
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let source_dir = water_mask_cached_source_dir(config, region, &output_dir)?;
    build_water_mask_region_tiles(region, &output_dir, &source_dir)?;
    let (source_version, source_fetched_at_utc) = water_mask_manifest_versions(&manifest_path)?;
    let zip_path = output_dir.join(format!(
        "water_mask_{region_id}_{}.zip",
        fast_product_version_label(&source_version)
    ));
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((
        zip_path,
        output_dir.join("tiles"),
        source_version,
        source_fetched_at_utc,
        record,
    ))
}

fn water_mask_record_zip_path(node_dir: &Path, record: &NodeRecord) -> anyhow::Result<PathBuf> {
    let value = record
        .outputs
        .get("zip")
        .context("water mask node record missing zip output")?;
    resolve_recorded_output_path(node_dir, value)
        .with_context(|| format!("failed to resolve water mask zip output {value}"))
}

fn water_mask_manifest_versions(path: &Path) -> anyhow::Result<(String, Option<String>)> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let source_fingerprint = value
        .get("source_fingerprint")
        .and_then(|value| value.as_str())
        .context("water mask manifest missing source_fingerprint")?
        .to_string();
    let source_fetched_at_utc = value
        .get("source_fetched_at_utc")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    Ok((source_fingerprint, source_fetched_at_utc))
}

fn water_mask_product_inputs(region: Region) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        ("product_id".to_string(), format!("water-mask-{region_id}")),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "water_mask_pipeline".to_string(),
            WATER_MASK_PIPELINE_VERSION.to_string(),
        ),
        (
            "water_mask_source_fetch".to_string(),
            format!(
                "nhd-object-ids-v1-precision-6-page-size-{}-fetch-workers-{}-layers-{}",
                WATER_MASK_PAGE_SIZE,
                WATER_MASK_FETCH_WORKERS,
                WATER_MASK_NHD_LAYERS
                    .iter()
                    .map(|(layer, _name, where_clause)| format!("{layer}:{where_clause}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        (
            "water_mask_script".to_string(),
            hash_file(water_mask_tile_script_path())?,
        ),
    ]))
}

fn build_shaded_relief_product(
    config: &ProductBuildConfig,
    region: Region,
    terrain_index_path: &Path,
    source_fetched_at_utc: Option<String>,
    water_mask_tiles_dir: &Path,
    water_mask_version: &str,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<String>,
    Vec<TileLevelRecord>,
    NodeRecord,
)> {
    let region_id = region.code().to_ascii_lowercase();
    let input_dir = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("shaded-relief")
        .join(&region_id)
        .join("input");
    let dem_dir = input_dir.join("dems");
    fs::create_dir_all(&dem_dir)
        .with_context(|| format!("failed to create {}", dem_dir.display()))?;

    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("shaded-relief-{region_id}"));
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let mut dem_candidates = terrain_dem_candidates_for_region(terrain_index_path, region)?;
    if dem_candidates.is_empty() {
        bail!(
            "terrain discovery index has no DEM URLs for shaded relief {}",
            region.code()
        );
    }
    if let Some(cached_selection) = cached_terrain_dem_selection(&dem_candidates, &fetch_cache)? {
        let source_fingerprint = terrain_source_fingerprint_from_cached(
            &cached_selection.selection.urls,
            &cached_selection.sources,
            &cached_selection.selection.missing_cells,
        );
        let version_label = fast_product_version_label(&source_fingerprint);
        let inputs = shaded_relief_product_inputs(region, &source_fingerprint, water_mask_version)?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &format!("static-shaded-relief-{region_id}"))?,
            &format!("static-shaded-relief-{region_id}"),
            &inputs,
        )?;
        let output_dir = prepared.dir.join("output");
        let zip_path = output_dir.join(format!("shaded_relief_{region_id}_{version_label}.zip"));
        let manifest_path = output_dir.join("manifest.json");
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path])? {
            let tile_levels = read_static_tile_manifest_levels(&output_dir.join("manifest.json"))?;
            return Ok((
                zip_path,
                version_label,
                source_fetched_at_utc,
                tile_levels,
                record,
            ));
        }
    }
    let dem_selection = prefetch_terrain_dems_with_fallback(
        &mut dem_candidates,
        &dem_dir,
        config.fetch_jobs,
        &fetch_cache,
        &provenance_dir,
        &format!("shaded-relief-{region_id}-dem"),
    )?;
    let dem_paths = terrain_dem_paths_from_urls(&dem_dir, &dem_selection.urls)?;
    let source_fingerprint = if let Some(sources) =
        cached_terrain_dem_sources_for_urls(&fetch_cache, &dem_selection.urls)?
    {
        terrain_source_fingerprint_from_cached(
            &dem_selection.urls,
            &sources,
            &dem_selection.missing_cells,
        )
    } else {
        terrain_source_fingerprint(
            &dem_selection.urls,
            &dem_paths,
            &dem_selection.missing_cells,
        )?
    };
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = shaded_relief_product_inputs(region, &source_fingerprint, water_mask_version)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-shaded-relief-{region_id}"))?,
        &format!("static-shaded-relief-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let zip_path = output_dir.join(format!("shaded_relief_{region_id}_{version_label}.zip"));
    let manifest_path = output_dir.join("manifest.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path])? {
        NodeCacheState::CacheHit(record) => {
            let tile_levels = read_static_tile_manifest_levels(&output_dir.join("manifest.json"))?;
            return Ok((
                zip_path,
                version_label,
                source_fetched_at_utc,
                tile_levels,
                record,
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let vrt_path = output_dir.join(format!("shaded_relief_{region_id}.vrt"));
    build_terrain_vrt(&vrt_path, &dem_paths)?;
    build_shaded_relief_region_tiles(
        region,
        &vrt_path,
        &output_dir,
        &version_label,
        &dem_selection,
        water_mask_tiles_dir,
    )?;
    move_static_tile_tree_under_chart_index(&output_dir, 0)?;
    let tile_levels = read_static_tile_manifest_levels(&output_dir.join("manifest.json"))?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&output_dir.join("manifest.json"), &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((
        zip_path,
        version_label,
        source_fetched_at_utc,
        tile_levels,
        record,
    ))
}

fn shaded_relief_product_inputs(
    region: Region,
    source_fingerprint: &str,
    water_mask_version: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        (
            "product_id".to_string(),
            format!("shaded-relief-{region_id}"),
        ),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "water_mask_version".to_string(),
            water_mask_version.to_string(),
        ),
        (
            "shaded_relief_pipeline".to_string(),
            SHADED_RELIEF_PIPELINE_VERSION.to_string(),
        ),
        (
            "shaded_relief_script".to_string(),
            hash_file(shaded_relief_tile_script_path())?,
        ),
    ]))
}

#[derive(Debug, Deserialize)]
struct StaticTileManifest {
    levels: Vec<StaticTileManifestLevel>,
}

#[derive(Debug, Deserialize)]
struct StaticTileManifestLevel {
    zoom: u32,
    x_min: u32,
    x_max: u32,
    y_tms_min: u32,
    y_tms_max: u32,
}

fn read_static_tile_manifest_levels(manifest_path: &Path) -> anyhow::Result<Vec<TileLevelRecord>> {
    let manifest: StaticTileManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let levels = manifest
        .levels
        .into_iter()
        .map(|level| TileLevelRecord {
            zoom: level.zoom,
            x_min: level.x_min,
            x_max: level.x_max,
            y_tms_min: level.y_tms_min,
            y_tms_max: level.y_tms_max,
        })
        .collect::<Vec<_>>();
    if levels.is_empty() {
        bail!(
            "static tile manifest {} had no levels",
            manifest_path.display()
        );
    }
    Ok(levels)
}

fn move_static_tile_tree_under_chart_index(
    output_dir: &Path,
    chart_index: u32,
) -> anyhow::Result<()> {
    let tiles_dir = output_dir.join("tiles");
    let chart_index_dir = tiles_dir.join(chart_index.to_string());
    if chart_index_dir.exists() {
        fs::remove_dir_all(&chart_index_dir)
            .with_context(|| format!("failed to remove {}", chart_index_dir.display()))?;
    }
    let tmp_dir = output_dir.join(format!(".tiles-chart-index-{chart_index}"));
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)
            .with_context(|| format!("failed to remove {}", tmp_dir.display()))?;
    }
    fs::create_dir_all(&tmp_dir)
        .with_context(|| format!("failed to create {}", tmp_dir.display()))?;
    for entry in fs::read_dir(&tiles_dir)
        .with_context(|| format!("failed to read {}", tiles_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') || name == chart_index.to_string() {
            continue;
        }
        fs::rename(&path, tmp_dir.join(&file_name)).with_context(|| {
            format!(
                "failed to move {} under chart-index staging",
                path.display()
            )
        })?;
    }
    fs::rename(&tmp_dir, &chart_index_dir).with_context(|| {
        format!(
            "failed to install chart-index tile tree at {}",
            chart_index_dir.display()
        )
    })?;
    Ok(())
}

fn static_geo_source_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .parent()
        .expect("workspace root should live under product")
        .parent()
        .expect("product should live under repo root")
        .join("avare-assets")
        .join("geo")
        .join("geo.csv")
}

fn terrain_tnmaccess_url(region: Region) -> String {
    let bounds = region.bounds();
    let bbox = format!(
        "{},{},{},{}",
        bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
    );
    format!(
        "https://tnmaccess.nationalmap.gov/api/v1/products?bbox={bbox}&datasets=National%20Elevation%20Dataset%20(NED)%201%20arc-second%20Current&prodFormats=GeoTIFF&max=3000#logical_name=terrain_{}_tnmaccess.json",
        region.code().to_ascii_lowercase()
    )
}

fn build_terrain_discovery_index(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, Option<String>, NodeRecord)> {
    let discovery_dir = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("terrain")
        .join("global-discovery")
        .join("input");
    fs::create_dir_all(&discovery_dir)
        .with_context(|| format!("failed to create {}", discovery_dir.display()))?;
    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join("terrain-discovery");
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let discovery_urls = config
        .profile
        .terrain_regions()
        .iter()
        .map(|region| terrain_tnmaccess_url(*region))
        .collect::<Vec<_>>();
    prefetch_archives_with_provenance(
        &discovery_urls,
        &discovery_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "terrain-discovery",
    )?;

    let mut by_cell = BTreeMap::<String, Vec<TerrainDemCandidate>>::new();
    let mut discovery_hashes = BTreeMap::new();
    for region in config.profile.terrain_regions() {
        let region_id = region.code().to_ascii_lowercase();
        let path = discovery_dir.join(format!("terrain_{region_id}_tnmaccess.json"));
        discovery_hashes.insert(region_id, hash_file(&path)?);
        for (cell, mut candidates) in terrain_dem_candidates_from_tnmaccess(&path)? {
            by_cell.entry(cell).or_default().append(&mut candidates);
        }
    }
    normalize_terrain_candidates(&mut by_cell);
    let source_fetched_at_utc = terrain_source_fetched_at_utc(&fetch_cache, &discovery_urls, &[])?;
    let source_fingerprint = terrain_discovery_fingerprint(&by_cell, &discovery_hashes);
    let inputs = BTreeMap::from([
        ("product_id".to_string(), "terrain-discovery".to_string()),
        (
            "regions".to_string(),
            config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| region.code())
                .collect::<Vec<_>>()
                .join(","),
        ),
        ("source_fingerprint".to_string(), source_fingerprint.clone()),
        (
            "product_build".to_string(),
            hash_file(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/product_build.rs"))?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-terrain-discovery")?,
        "static-terrain-discovery",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let index_path = output_dir.join("terrain_dem_index.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[index_path.clone()])? {
        NodeCacheState::CacheHit(record) => {
            return Ok((index_path, source_fetched_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let index = TerrainDemIndex {
        schema_version: 1,
        regions: config
            .profile
            .terrain_regions()
            .iter()
            .map(|region| region.code().to_string())
            .collect(),
        source_fetched_at_utc: source_fetched_at_utc.clone(),
        cells: by_cell,
    };
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).context("failed to encode terrain DEM index")?,
    )
    .with_context(|| format!("failed to write {}", index_path.display()))?;
    let outputs = BTreeMap::from([(
        "index".to_string(),
        relative_artifact_path(&index_path, &config.build_root),
    )]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((index_path, source_fetched_at_utc, record))
}

fn terrain_dem_candidates_from_tnmaccess(
    path: &Path,
) -> anyhow::Result<BTreeMap<String, Vec<TerrainDemCandidate>>> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let items = value
        .get("items")
        .and_then(|value| value.as_array())
        .context("TNMAccess response missing items[]")?;
    let mut by_cell = BTreeMap::<String, Vec<TerrainDemCandidate>>::new();
    for item in items {
        let Some(url) = item.get("downloadURL").and_then(|value| value.as_str()) else {
            continue;
        };
        if !url.ends_with(".tif") {
            continue;
        }
        let filename = url.rsplit('/').next().unwrap_or("dem.tif").to_string();
        let Some(cell) = terrain_dem_cell_from_filename(&filename) else {
            continue;
        };
        let candidate = TerrainDemCandidate {
            url: format!("{url}#logical_name={filename}"),
            publication_date: item
                .get("publicationDate")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            last_updated: item
                .get("lastUpdated")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            filename,
        };
        by_cell.entry(cell).or_default().push(candidate);
    }
    normalize_terrain_candidates(&mut by_cell);
    Ok(by_cell)
}

fn normalize_terrain_candidates(
    candidates_by_cell: &mut BTreeMap<String, Vec<TerrainDemCandidate>>,
) {
    for candidates in candidates_by_cell.values_mut() {
        candidates.sort_by(|left, right| right.sort_key().cmp(&left.sort_key()));
        candidates.dedup_by(|left, right| left.url == right.url);
    }
}

fn terrain_discovery_fingerprint(
    candidates: &BTreeMap<String, Vec<TerrainDemCandidate>>,
    discovery_hashes: &BTreeMap<String, String>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-discovery-v1");
    for (region, hash) in discovery_hashes {
        hasher.update(region.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0xff]);
    }
    for (cell, cell_candidates) in candidates {
        hasher.update(cell.as_bytes());
        hasher.update([0]);
        for candidate in cell_candidates {
            hasher.update(candidate.url.as_bytes());
            hasher.update([0]);
        }
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TerrainDemIndex {
    schema_version: u32,
    regions: Vec<String>,
    source_fetched_at_utc: Option<String>,
    cells: BTreeMap<String, Vec<TerrainDemCandidate>>,
}

#[derive(Debug, Clone)]
struct TerrainCellCandidates {
    cell: String,
    candidates: Vec<TerrainDemCandidate>,
    selected: usize,
    missing: bool,
}

impl TerrainCellCandidates {
    fn selected_candidate(&self) -> anyhow::Result<&TerrainDemCandidate> {
        self.candidates
            .get(self.selected)
            .with_context(|| format!("terrain cell {} has no selected DEM candidate", self.cell))
    }

    fn selected_url(&self) -> anyhow::Result<String> {
        Ok(self.selected_candidate()?.url.clone())
    }

    fn selected_url_if_available(&self) -> anyhow::Result<Option<String>> {
        if self.missing {
            return Ok(None);
        }
        Ok(Some(self.selected_url()?))
    }

    fn advance_after_failed_url(&mut self, failed_url: &str) -> anyhow::Result<TerrainCellAction> {
        if self.selected_candidate()?.url != failed_url {
            return Ok(TerrainCellAction::Unaffected);
        }
        if self.selected + 1 >= self.candidates.len() {
            self.missing = true;
            return Ok(TerrainCellAction::MarkedMissing);
        }
        self.selected += 1;
        Ok(TerrainCellAction::Advanced)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerrainCellAction {
    Unaffected,
    Advanced,
    MarkedMissing,
}

#[derive(Debug, Clone)]
struct TerrainDemSelection {
    urls: Vec<String>,
    missing_cells: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedTerrainDemSelection {
    selection: TerrainDemSelection,
    sources: Vec<CachedTerrainDemSource>,
}

#[derive(Debug, Clone)]
struct CachedTerrainDemSource {
    filename: String,
    sha256: String,
}

fn terrain_dem_candidates_for_region(
    index_path: &Path,
    region: Region,
) -> anyhow::Result<Vec<TerrainCellCandidates>> {
    let index: TerrainDemIndex = serde_json::from_slice(
        &fs::read(index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", index_path.display()))?;
    Ok(index
        .cells
        .into_iter()
        .filter(|(cell, _)| terrain_cell_intersects_region(cell, region))
        .map(|(cell, candidates)| TerrainCellCandidates {
            cell,
            candidates,
            selected: 0,
            missing: false,
        })
        .collect())
}

fn cached_terrain_dem_selection(
    cells: &[TerrainCellCandidates],
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<Option<CachedTerrainDemSelection>> {
    let mut urls = Vec::new();
    let mut sources = Vec::new();
    let mut missing_cells = Vec::new();
    for cell in cells {
        let mut cached_candidates = Vec::new();
        for (index, candidate) in cell.candidates.iter().enumerate() {
            if let Some(source) = cached_terrain_dem_source(fetch_cache, candidate)? {
                cached_candidates.push((index, candidate, source));
            }
        }
        match cached_candidates.as_slice() {
            [(0, candidate, source), ..] => {
                urls.push(candidate.url.clone());
                sources.push(source.clone());
            }
            [] => missing_cells.push(cell.cell.clone()),
            _ => {
                // A later cached candidate may be an intentional fallback, or it may be stale
                // relative to a newly-discovered newer DEM. Fetching is the only safe way to
                // distinguish those cases, so do not use the early cache-hit path.
                return Ok(None);
            }
        }
    }
    Ok(Some(CachedTerrainDemSelection {
        selection: TerrainDemSelection {
            urls,
            missing_cells,
        },
        sources,
    }))
}

fn cached_terrain_dem_source(
    fetch_cache: &FetchCacheConfig,
    candidate: &TerrainDemCandidate,
) -> anyhow::Result<Option<CachedTerrainDemSource>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let metadata_path = layout.http_metadata_path(&candidate.url);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
    let Some(sha256) = value.get("sha256").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    if !layout.blob_path(sha256).is_file() {
        return Ok(None);
    }
    Ok(Some(CachedTerrainDemSource {
        filename: candidate.filename.clone(),
        sha256: sha256.to_string(),
    }))
}

fn cached_terrain_dem_sources_for_urls(
    fetch_cache: &FetchCacheConfig,
    urls: &[String],
) -> anyhow::Result<Option<Vec<CachedTerrainDemSource>>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let mut sources = Vec::new();
    for url in urls {
        let metadata_path = layout.http_metadata_path(url);
        if !metadata_path.is_file() {
            return Ok(None);
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("failed to read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        let Some(sha256) = value.get("sha256").and_then(|value| value.as_str()) else {
            return Ok(None);
        };
        if !layout.blob_path(sha256).is_file() {
            return Ok(None);
        }
        sources.push(CachedTerrainDemSource {
            filename: terrain_dem_filename_from_url(url)?,
            sha256: sha256.to_string(),
        });
    }
    Ok(Some(sources))
}

fn terrain_dem_filename_from_url(url: &str) -> anyhow::Result<String> {
    url.split("#logical_name=")
        .nth(1)
        .or_else(|| url.rsplit('/').next())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .context("terrain DEM URL has no filename")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TerrainDemCandidate {
    url: String,
    publication_date: String,
    last_updated: String,
    filename: String,
}

impl TerrainDemCandidate {
    fn sort_key(&self) -> (&str, &str, &str) {
        (&self.publication_date, &self.last_updated, &self.filename)
    }
}

fn terrain_dem_cell_from_filename(filename: &str) -> Option<String> {
    filename
        .split('_')
        .find(|part| {
            let bytes = part.as_bytes();
            matches!(bytes.first(), Some(b'n' | b's')) && (part.contains('w') || part.contains('e'))
        })
        .map(ToOwned::to_owned)
}

fn terrain_cell_intersects_region(cell: &str, region: Region) -> bool {
    let Some((lat_min, lon_min)) = terrain_cell_origin(cell) else {
        return false;
    };
    let bounds = region.bounds();
    let lat_max = lat_min + 1.0;
    let lon_max = lon_min + 1.0;
    lon_min < bounds.lon_max
        && lon_max > bounds.lon_min
        && lat_min < bounds.lat_max
        && lat_max > bounds.lat_min
}

fn terrain_cell_origin(cell: &str) -> Option<(f64, f64)> {
    let lon_start = cell.find('w').or_else(|| cell.find('e'))?;
    let (lat_part, lon_part_with_dir) = cell.split_at(lon_start);
    let (lon_dir, lon_part) = lon_part_with_dir.split_at(1);
    let lat_abs = lat_part.get(1..)?.parse::<f64>().ok()?;
    let lon_abs = lon_part.parse::<f64>().ok()?;
    let lat_north_edge = if lat_part.starts_with('s') {
        -lat_abs
    } else {
        lat_abs
    };
    let lat = lat_north_edge - 1.0;
    let lon = if lon_dir == "w" { -lon_abs } else { lon_abs };
    Some((lat, lon))
}

fn prefetch_terrain_dems_with_fallback(
    cells: &mut [TerrainCellCandidates],
    dem_dir: &Path,
    fetch_jobs: usize,
    fetch_cache: &FetchCacheConfig,
    provenance_dir: &Path,
    label: &str,
) -> anyhow::Result<TerrainDemSelection> {
    loop {
        let urls = cells
            .iter()
            .map(TerrainCellCandidates::selected_url_if_available)
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        match prefetch_archives_with_provenance(
            &urls,
            dem_dir,
            fetch_jobs,
            Some(fetch_cache),
            provenance_dir,
            label,
        ) {
            Ok(()) => {
                let missing_cells = cells
                    .iter()
                    .filter(|cell| cell.missing)
                    .map(|cell| cell.cell.clone())
                    .collect::<Vec<_>>();
                return Ok(TerrainDemSelection {
                    urls,
                    missing_cells,
                });
            }
            Err(error) => {
                let message = error.to_string();
                let Some(failed_url) = terrain_failed_fetch_url(&message) else {
                    return Err(error);
                };
                let Some(failed_logical_url) = urls
                    .iter()
                    .find(|url| terrain_urls_match(url, &failed_url))
                    .cloned()
                else {
                    return Err(error);
                };
                let mut handled = false;
                for cell in cells.iter_mut() {
                    match cell.advance_after_failed_url(&failed_logical_url)? {
                        TerrainCellAction::Unaffected => {}
                        TerrainCellAction::Advanced => {
                            eprintln!(
                                "terrain DEM fetch failed for {}; falling back to next candidate for cell {}",
                                failed_logical_url, cell.cell
                            );
                            handled = true;
                            break;
                        }
                        TerrainCellAction::MarkedMissing => {
                            eprintln!(
                                "terrain DEM fetch failed for {}; marking cell {} as nodata",
                                failed_logical_url, cell.cell
                            );
                            handled = true;
                            break;
                        }
                    }
                }
                if !handled {
                    return Err(error);
                }
            }
        }
    }
}

fn terrain_failed_fetch_url(message: &str) -> Option<String> {
    let start = message.find("curl failed for ")? + "curl failed for ".len();
    let rest = &message[start..];
    let end = rest.find(" with HTTP").or_else(|| rest.find('\n'))?;
    Some(rest[..end].to_string())
}

fn terrain_urls_match(logical_url: &str, failed_url: &str) -> bool {
    logical_url == failed_url
        || logical_url
            .split_once("#logical_name=")
            .map(|(network_url, _)| network_url == failed_url)
            .unwrap_or(false)
}

fn terrain_dem_paths_from_urls(dem_dir: &Path, urls: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    urls.iter()
        .map(|url| {
            let parsed_name = url
                .split("#logical_name=")
                .nth(1)
                .or_else(|| url.rsplit('/').next())
                .context("terrain DEM URL has no filename")?;
            let path = dem_dir.join(parsed_name);
            if !path.is_file() {
                bail!("terrain DEM download missing {}", path.display());
            }
            Ok(path)
        })
        .collect()
}

fn terrain_source_fetched_at_utc(
    fetch_cache: &FetchCacheConfig,
    discovery_urls: &[String],
    dem_urls: &[String],
) -> anyhow::Result<Option<String>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let mut fetched_times = Vec::new();
    for url in discovery_urls
        .iter()
        .map(String::as_str)
        .chain(dem_urls.iter().map(String::as_str))
    {
        let metadata_path = layout.http_metadata_path(url);
        if !metadata_path.is_file() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("failed to read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        if let Some(fetched_at) = value.get("fetched_at_utc").and_then(|value| value.as_str()) {
            fetched_times.push(fetched_at.to_string());
            continue;
        }
        if let Ok(modified) = fs::metadata(&metadata_path).and_then(|metadata| metadata.modified())
        {
            fetched_times.push(
                DateTime::<Utc>::from(modified)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            );
        }
    }
    fetched_times.sort();
    Ok(fetched_times.into_iter().max())
}

fn terrain_source_fingerprint(
    dem_urls: &[String],
    dem_paths: &[PathBuf],
    missing_cells: &[String],
) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-v1");
    hasher.update(TERRAIN_ZOOM.to_string().as_bytes());
    for url in dem_urls {
        hasher.update(url.as_bytes());
        hasher.update([0]);
    }
    for path in dem_paths {
        hasher.update(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(hash_file(path)?.as_bytes());
    }
    for cell in missing_cells {
        hasher.update(b"missing");
        hasher.update([0]);
        hasher.update(cell.as_bytes());
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn terrain_source_fingerprint_from_cached(
    dem_urls: &[String],
    sources: &[CachedTerrainDemSource],
    missing_cells: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-v1");
    hasher.update(TERRAIN_ZOOM.to_string().as_bytes());
    for url in dem_urls {
        hasher.update(url.as_bytes());
        hasher.update([0]);
    }
    for source in sources {
        hasher.update(source.filename.as_bytes());
        hasher.update(source.sha256.as_bytes());
    }
    for cell in missing_cells {
        hasher.update(b"missing");
        hasher.update([0]);
        hasher.update(cell.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

fn build_terrain_vrt(vrt_path: &Path, dem_paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut command = Command::new("gdalbuildvrt");
    command.arg("-overwrite").arg(vrt_path);
    for path in dem_paths {
        command.arg(path);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run gdalbuildvrt for {}", vrt_path.display()))?;
    if !status.success() {
        bail!("gdalbuildvrt failed for {}", vrt_path.display());
    }
    Ok(())
}

fn build_terrain_region_tiles(
    region: Region,
    vrt_path: &Path,
    geo_csv_path: &Path,
    output_dir: &Path,
    version_label: &str,
    dem_selection: &TerrainDemSelection,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_terrain_tiles.py");
    fs::write(&script_path, TERRAIN_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let bounds = region.bounds();
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--vrt")
        .arg(vrt_path)
        .arg("--geo-csv")
        .arg(geo_csv_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-count")
        .arg(dem_selection.urls.len().to_string())
        .arg("--missing-cells")
        .arg(dem_selection.missing_cells.join(","))
        .arg("--workers")
        .arg(TERRAIN_TILE_WORKERS.to_string())
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "terrain tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn water_mask_query_url(layer: u32, params: &[(&str, String)]) -> String {
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode_query_value(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{WATER_MASK_NHD_SERVICE}/{layer}/query?{query}")
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn water_mask_ids_url(layer: u32, bbox: &str, where_clause: &str) -> String {
    format!(
        "{}#logical_name=layer_{layer}_ids.json",
        water_mask_query_url(
            layer,
            &[
                ("where", where_clause.to_string()),
                ("geometry", bbox.to_string()),
                ("geometryType", "esriGeometryEnvelope".to_string()),
                ("inSR", "4326".to_string()),
                ("spatialRel", "esriSpatialRelIntersects".to_string()),
                ("returnIdsOnly", "true".to_string()),
                ("f", "json".to_string()),
            ],
        )
    )
}

#[derive(Debug, Clone)]
struct WaterMaskPageRequest {
    layer: u32,
    label: String,
    object_ids: Vec<u64>,
}

impl WaterMaskPageRequest {
    fn file_name(&self) -> String {
        format!("layer_{}_chunk_{}.geojson", self.layer, self.label)
    }

    fn url(&self) -> String {
        water_mask_page_url(self.layer, &self.label, &self.object_ids)
    }
}

fn water_mask_page_url(layer: u32, page_label: &str, object_ids: &[u64]) -> String {
    format!(
        "{}#logical_name=layer_{layer}_chunk_{page_label}.geojson",
        water_mask_query_url(
            layer,
            &[
                (
                    "objectIds",
                    object_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                ("outFields", "FTYPE,FCODE,GNIS_NAME".to_string()),
                ("outSR", "4326".to_string()),
                ("returnGeometry", "true".to_string()),
                ("geometryPrecision", "6".to_string()),
                ("f", "geojson".to_string()),
                ("orderByFields", "OBJECTID".to_string()),
            ],
        )
    )
}

fn water_mask_cached_source_dir(
    config: &ProductBuildConfig,
    region: Region,
    output_dir: &Path,
) -> anyhow::Result<PathBuf> {
    let region_id = region.code().to_ascii_lowercase();
    let bounds = region.bounds();
    let bbox = format!(
        "{},{},{},{}",
        bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
    );
    let source_dir = output_dir.join("source-pages");
    fs::create_dir_all(&source_dir)
        .with_context(|| format!("failed to create {}", source_dir.display()))?;
    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("water-mask-{region_id}"));
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let ids_urls = WATER_MASK_NHD_LAYERS
        .iter()
        .map(|(layer, _name, where_clause)| water_mask_ids_url(*layer, &bbox, where_clause))
        .collect::<Vec<_>>();
    prefetch_water_mask_source_urls(
        &ids_urls,
        &source_dir,
        &provenance_dir,
        &format!("water-mask-{region_id}-ids"),
        &fetch_cache,
    )?;

    let mut page_requests = Vec::new();
    for (layer, _name, _where_clause) in WATER_MASK_NHD_LAYERS {
        let ids_path = source_dir.join(format!("layer_{layer}_ids.json"));
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&ids_path)
                .with_context(|| format!("failed to read {}", ids_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", ids_path.display()))?;
        let mut object_ids = value
            .get("objectIds")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_u64())
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        for (chunk_index, chunk) in object_ids.chunks(WATER_MASK_PAGE_SIZE).enumerate() {
            page_requests.push(WaterMaskPageRequest {
                layer: *layer,
                label: format!("{chunk_index:05}"),
                object_ids: chunk.to_vec(),
            });
        }
    }
    prefetch_water_mask_source_pages(
        &page_requests,
        &source_dir,
        &provenance_dir,
        &format!("water-mask-{region_id}-page"),
        &fetch_cache,
    )?;
    Ok(source_dir)
}

fn prefetch_water_mask_source_urls(
    urls: &[String],
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<()> {
    prefetch_archives_with_provenance(
        urls,
        source_dir,
        WATER_MASK_FETCH_WORKERS as usize,
        Some(fetch_cache),
        provenance_dir,
        label,
    )
}

fn prefetch_water_mask_source_pages(
    pages: &[WaterMaskPageRequest],
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<()> {
    let mut split_page_fetches = 0usize;
    let mut omitted_objects = Vec::new();
    let urls = pages
        .iter()
        .map(WaterMaskPageRequest::url)
        .collect::<Vec<_>>();
    if prefetch_water_mask_source_urls(&urls, source_dir, provenance_dir, label, fetch_cache)
        .is_err()
    {
        for page in pages {
            prefetch_water_mask_source_page_split(
                page,
                source_dir,
                provenance_dir,
                label,
                fetch_cache,
                &mut split_page_fetches,
                &mut omitted_objects,
            )?;
        }
    }
    if !omitted_objects.is_empty() {
        eprintln!(
            "water mask omitted {} persistent failing NHD object(s): {:?}",
            omitted_objects.len(),
            omitted_objects
        );
    }
    Ok(())
}

fn prefetch_water_mask_source_page_split(
    page: &WaterMaskPageRequest,
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
    split_page_fetches: &mut usize,
    omitted_objects: &mut Vec<u64>,
) -> anyhow::Result<()> {
    let urls = [page.url()];
    match prefetch_water_mask_source_urls(&urls, source_dir, provenance_dir, label, fetch_cache) {
        Ok(()) => return Ok(()),
        Err(error) => {
            if page.object_ids.len() > 1 {
                if *split_page_fetches >= WATER_MASK_MAX_SPLIT_SOURCE_PAGES {
                    bail!(
                        "water mask source page splitting exceeded {} split pages after failure: {error}",
                        WATER_MASK_MAX_SPLIT_SOURCE_PAGES
                    );
                }
                let midpoint = page.object_ids.len() / 2;
                let split_pages = [
                    WaterMaskPageRequest {
                        layer: page.layer,
                        label: format!("{}_a", page.label),
                        object_ids: page.object_ids[..midpoint].to_vec(),
                    },
                    WaterMaskPageRequest {
                        layer: page.layer,
                        label: format!("{}_b", page.label),
                        object_ids: page.object_ids[midpoint..].to_vec(),
                    },
                ];
                *split_page_fetches += split_pages.len();
                for split_page in split_pages {
                    prefetch_water_mask_source_page_split(
                        &split_page,
                        source_dir,
                        provenance_dir,
                        label,
                        fetch_cache,
                        split_page_fetches,
                        omitted_objects,
                    )
                    .with_context(|| {
                        format!(
                            "failed while splitting water mask page {} after: {error}",
                            page.file_name()
                        )
                    })?;
                }
                return Ok(());
            }
            if omitted_objects.len() >= WATER_MASK_MAX_OMITTED_OBJECTS {
                bail!(
                    "water mask source omitted object cap exceeded after persistent failure for {}: {error}",
                    page.file_name()
                );
            }
            omitted_objects.push(page.object_ids[0]);
            write_empty_water_mask_page(source_dir, page).with_context(|| {
                format!(
                    "wrote empty water mask page for persistent failing object {} after: {error}",
                    page.object_ids[0]
                )
            })?;
            Ok(())
        }
    }
}

fn write_empty_water_mask_page(
    source_dir: &Path,
    page: &WaterMaskPageRequest,
) -> anyhow::Result<()> {
    let path = source_dir.join(page.file_name());
    let value = serde_json::json!({
        "type": "FeatureCollection",
        "features": [],
    });
    fs::write(
        &path,
        serde_json::to_vec(&value).context("failed to encode empty water mask page")?,
    )
    .with_context(|| format!("failed to write empty water mask page {}", path.display()))?;
    Ok(())
}

fn build_water_mask_region_tiles(
    region: Region,
    output_dir: &Path,
    source_dir: &Path,
) -> anyhow::Result<()> {
    let script_path = water_mask_tile_script_path();
    let bounds = region.bounds();
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--source-dir")
        .arg(source_dir)
        .arg("--fetch-workers")
        .arg(WATER_MASK_FETCH_WORKERS.to_string())
        .arg("--tile-workers")
        .arg(WATER_MASK_TILE_WORKERS.to_string())
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "water mask tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn build_shaded_relief_region_tiles(
    region: Region,
    vrt_path: &Path,
    output_dir: &Path,
    version_label: &str,
    dem_selection: &TerrainDemSelection,
    water_mask_tiles_dir: &Path,
) -> anyhow::Result<()> {
    let script_path = shaded_relief_tile_script_path();
    let bounds = region.bounds();
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--vrt")
        .arg(vrt_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-count")
        .arg(dem_selection.urls.len().to_string())
        .arg("--missing-cells")
        .arg(dem_selection.missing_cells.join(","))
        .arg("--water-mask-dir")
        .arg(water_mask_tiles_dir)
        .arg("--workers")
        .arg(SHADED_RELIEF_TILE_WORKERS.to_string())
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "shaded relief tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn shaded_relief_tile_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("build_shaded_relief_tiles.py")
}

fn water_mask_tile_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("build_water_mask_tiles.py")
}

fn zip_directory_deterministic(
    zip_path: &Path,
    root: &Path,
    entries: &[&str],
) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for entry in entries {
        collect_zip_files(root, &root.join(entry), &mut files)?;
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let file = File::create(zip_path)
        .with_context(|| format!("failed to create {}", zip_path.display()))?;
    let mut writer = ZipWriter::new(file);
    for (name, path) in files {
        let compression =
            if name.ends_with(".terrain") || name.ends_with(".png") || name.ends_with(".webp") {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .last_modified_time(ZipDateTime::default());
        writer.start_file(name, options).with_context(|| {
            format!("failed to add {} to {}", path.display(), zip_path.display())
        })?;
        let mut input =
            File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut bytes = Vec::new();
        input
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read {}", path.display()))?;
        writer
            .write_all(&bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    writer
        .finish()
        .with_context(|| format!("failed to finish {}", zip_path.display()))?;
    Ok(())
}

fn collect_zip_files(
    root: &Path,
    path: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    if path.is_file() {
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to relativize {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, path.to_path_buf()));
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        collect_zip_files(root, &entry.path(), files)?;
    }
    Ok(())
}

const TERRAIN_TILE_SCRIPT: &str = r#"
import argparse, gzip, json, math, struct
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
import numpy as np
from osgeo import gdal

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS

WORKER_DS = None
WORKER_GEO = None
WORKER_TILES_ROOT = None
WORKER_ZOOM = None
WORKER_TILE_SIZE = None

def mercator(lon, lat):
    lat = max(min(lat, 85.05112878), -85.05112878)
    mx = lon * ORIGIN_SHIFT / 180.0
    my = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * RADIUS
    return mx, my

def lonlat(mx, my):
    lon = (mx / ORIGIN_SHIFT) * 180.0
    lat = (2.0 * math.atan(math.exp(my / RADIUS)) - math.pi / 2.0) * 180.0 / math.pi
    return lon, lat

def tile_bounds(x, y, z, tile_size):
    initial_resolution = (2.0 * math.pi * RADIUS) / tile_size
    resolution = initial_resolution / (2 ** z)
    minx = x * tile_size * resolution - ORIGIN_SHIFT
    maxx = (x + 1) * tile_size * resolution - ORIGIN_SHIFT
    miny = y * tile_size * resolution - ORIGIN_SHIFT
    maxy = (y + 1) * tile_size * resolution - ORIGIN_SHIFT
    return minx, miny, maxx, maxy

def tile_range(west, south, east, north, z, tile_size):
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2 ** z)
    west_m, south_m = mercator(west, south)
    east_m, north_m = mercator(east, north)
    x0 = math.floor((west_m + ORIGIN_SHIFT) / resolution / tile_size)
    x1 = math.floor((east_m + ORIGIN_SHIFT) / resolution / tile_size)
    y0 = math.floor((south_m + ORIGIN_SHIFT) / resolution / tile_size)
    y1 = math.floor((north_m + ORIGIN_SHIFT) / resolution / tile_size)
    return range(x0, x1 + 1), range(y0, y1 + 1)

def load_geo(path):
    values = {}
    with open(path) as f:
        for line in f:
            lat, lon, height, _decl = [int(x) for x in line.strip().split(',')]
            values[(lat, lon)] = height
    return values

def geoid(values, lat, lon):
    lon = ((lon + 180.0) % 360.0) - 180.0
    lat = max(min(lat, 89.0), -90.0)
    lat0 = math.floor(lat)
    lat1 = min(lat0 + 1, 89)
    lon0 = math.floor(lon)
    lon1 = lon0 + 1
    if lon1 >= 180:
        lon1 -= 360
    lt = lat - lat0
    ln = lon - lon0
    sw = values[(lat0, lon0)]
    se = values[(lat0, lon1)]
    nw = values[(lat1, lon0)]
    ne = values[(lat1, lon1)]
    return (sw * (1-ln) + se * ln) * (1-lt) + (nw * (1-ln) + ne * ln) * lt

def write_tile(path, payload, tile_size):
    path.parent.mkdir(parents=True, exist_ok=True)
    raw = b'ABT1' + struct.pack('<HHhhff', tile_size, tile_size, -32768, 0, 1.0, 0.0) + payload
    with open(path, 'wb') as f:
        f.write(gzip.compress(raw, mtime=0))

def read_tile(path, tile_size):
    with gzip.open(path, 'rb') as f:
        raw = f.read()
    if raw[:4] != b'ABT1':
        raise ValueError(f'{path} is not an ABT1 terrain tile')
    width, height, nodata, _reserved, _scale, _offset = struct.unpack('<HHhhff', raw[4:20])
    if width != tile_size or height != tile_size or nodata != -32768:
        raise ValueError(f'{path} has unexpected terrain header')
    return np.frombuffer(raw[20:], dtype='<i2').reshape((tile_size, tile_size))

def max_downsample_2x2(samples):
    nodata = -32768
    blocks = samples.reshape((samples.shape[0] // 2, 2, samples.shape[1] // 2, 2))
    valid = blocks != nodata
    safe = np.where(valid, blocks, -32768)
    reduced = safe.max(axis=(1, 3)).astype('<i2')
    reduced[~valid.any(axis=(1, 3))] = nodata
    return reduced

def build_parent_tile(tiles_root, z, x, y, tile_size):
    half = tile_size // 2
    parent = np.full((tile_size, tile_size), -32768, dtype='<i2')
    children = [
        (x * 2, y * 2 + 1, 0, half, 0, half),
        (x * 2 + 1, y * 2 + 1, 0, half, half, tile_size),
        (x * 2, y * 2, half, tile_size, 0, half),
        (x * 2 + 1, y * 2, half, tile_size, half, tile_size),
    ]
    for child_x, child_y, row0, row1, col0, col1 in children:
        child_path = tiles_root / str(z + 1) / str(child_x) / f'{child_y}.terrain'
        if child_path.exists():
            parent[row0:row1, col0:col1] = max_downsample_2x2(read_tile(child_path, tile_size))
    write_tile(tiles_root / str(z) / str(x) / f'{y}.terrain', parent.tobytes(), tile_size)

def build_parent_pyramid(tiles_root, max_zoom, tile_size):
    counts = {max_zoom: sum(1 for _ in (tiles_root / str(max_zoom)).glob('*/*.terrain'))}
    for z in range(max_zoom - 1, -1, -1):
        child_root = tiles_root / str(z + 1)
        parents = set()
        for child_path in child_root.glob('*/*.terrain'):
            child_x = int(child_path.parent.name)
            child_y = int(child_path.stem)
            parents.add((child_x // 2, child_y // 2))
        for x, y in sorted(parents):
            build_parent_tile(tiles_root, z, x, y, tile_size)
        counts[z] = len(parents)
    return counts

def init_worker(vrt_path, geo_csv_path, tiles_root, zoom, tile_size):
    global WORKER_DS, WORKER_GEO, WORKER_TILES_ROOT, WORKER_ZOOM, WORKER_TILE_SIZE
    WORKER_DS = gdal.Open(vrt_path)
    if WORKER_DS is None:
        raise RuntimeError(f'failed to open {vrt_path}')
    WORKER_GEO = load_geo(geo_csv_path)
    WORKER_TILES_ROOT = Path(tiles_root)
    WORKER_ZOOM = zoom
    WORKER_TILE_SIZE = tile_size

def render_tile(task):
    x, y = task
    minx, miny, maxx, maxy = tile_bounds(x, y, WORKER_ZOOM, WORKER_TILE_SIZE)
    warped = gdal.Warp(
        '', WORKER_DS, format='MEM', dstSRS='EPSG:3857',
        outputBounds=[minx, miny, maxx, maxy],
        width=WORKER_TILE_SIZE, height=WORKER_TILE_SIZE,
        resampleAlg='bilinear', dstNodata=-999999.0,
    )
    arr = warped.ReadAsArray()
    center_lon, center_lat = lonlat((minx + maxx) / 2.0, (miny + maxy) / 2.0)
    tile_geoid_ft = geoid(WORKER_GEO, center_lat, center_lon)
    invalid = (arr <= -999998.0) | np.isnan(arr)
    samples = np.rint(arr.astype(np.float64) * 3.280839895 + tile_geoid_ft)
    samples = np.clip(samples, -32767, 32767).astype('<i2')
    samples[invalid] = -32768
    write_tile(
        WORKER_TILES_ROOT / str(WORKER_ZOOM) / str(x) / f'{y}.terrain',
        samples.tobytes(),
        WORKER_TILE_SIZE,
    )
    return 1

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--vrt', required=True)
    ap.add_argument('--geo-csv', required=True)
    ap.add_argument('--output-dir', required=True)
    ap.add_argument('--region', required=True)
    ap.add_argument('--bbox', required=True)
    ap.add_argument('--zoom', required=True, type=int)
    ap.add_argument('--tile-size', required=True, type=int)
    ap.add_argument('--version-label', required=True)
    ap.add_argument('--source-count', required=True, type=int)
    ap.add_argument('--missing-cells', default='')
    ap.add_argument('--workers', required=True, type=int)
    args = ap.parse_args()
    west, south, east, north = [float(x) for x in args.bbox.split(',')]
    root = Path(args.output_dir)
    tiles_root = root / 'tiles'
    x_range, y_range = tile_range(west, south, east, north, args.zoom, args.tile_size)
    tasks = [(x, y) for x in x_range for y in y_range]
    workers = max(1, args.workers)
    if workers == 1:
        init_worker(args.vrt, args.geo_csv, str(tiles_root), args.zoom, args.tile_size)
        count = sum(render_tile(task) for task in tasks)
    else:
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=init_worker,
            initargs=(args.vrt, args.geo_csv, str(tiles_root), args.zoom, args.tile_size),
        ) as pool:
            count = sum(pool.map(render_tile, tasks, chunksize=8))
    level_counts = build_parent_pyramid(tiles_root, args.zoom, args.tile_size)
    manifest = {
        'schema_version': 1,
        'product': 'terrain',
        'region': args.region,
        'version_label': args.version_label,
        'min_zoom': 0,
        'max_zoom': args.zoom,
        'base_zoom': args.zoom,
        'tile_size': args.tile_size,
        'tile_format': 'ABT1',
        'tile_content_encoding': 'gzip',
        'zip_member_compression': 'stored',
        'parent_tile_policy': 'max valid elevation over child samples; all-nodata children remain nodata',
        'sample_encoding': 'int16_le',
        'sample_units': 'feet',
        'sample_vertical_datum': 'WGS84 ellipsoid',
        'source_dem': 'USGS 3DEP 1 arc-second DEM',
        'source_dem_vertical_datum': 'source tile metadata; generally NAVD88 in CONUS',
        'geoid_model': 'avare geo.csv one-degree grid, applied once per tile at tile center (temporary approximation)',
        'worker_count': workers,
        'refresh_policy': {
            'identity': 'published filename is content-addressed by ZIP bytes',
            'source_fetched_at_utc': 'reported in the cycle bundle package row',
            'refresh_interval': 'producer policy; not embedded in artifact metadata'
        },
        'source_dem_count': args.source_count,
        'missing_dem_cells': [cell for cell in args.missing_cells.split(',') if cell],
        'nodata': -32768,
        'base_tile_count': count,
        'tile_count': sum(level_counts.values()),
        'levels': [{'zoom': z, 'tile_count': level_counts[z]} for z in sorted(level_counts)],
        'files': {'tiles': 'tiles'}
    }
    with open(root / 'manifest.json', 'w') as f:
        json.dump(manifest, f, indent=2, sort_keys=True)

if __name__ == '__main__':
    main()
"#;

fn fast_product_version_label(source_fingerprint: &str) -> String {
    source_fingerprint.chars().take(16).collect()
}

fn fast_product_node_inputs(
    product_id: &str,
    source_fingerprint: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("product_id".to_string(), product_id.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "fast_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fast/src/lib.rs"),
            )?,
        ),
    ]))
}

fn fast_product_source_generated_at(
    product_id: &str,
    structured_json_path: &Path,
    manifest_path: &Path,
) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(structured_json_path)
            .with_context(|| format!("failed to read {}", structured_json_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", structured_json_path.display()))?;
    match product_id {
        "metars" => value
            .get("metars")
            .and_then(|value| value.as_array())
            .and_then(|records| {
                records
                    .iter()
                    .filter_map(|record| {
                        record
                            .get("observation_time_utc")
                            .and_then(|value| value.as_str())
                    })
                    .max()
            })
            .map(ToOwned::to_owned)
            .context("METAR product had no observation_time_utc values"),
        "nexrad" => value
            .get("frames")
            .and_then(|value| value.as_array())
            .and_then(|frames| {
                frames
                    .iter()
                    .filter_map(|frame| {
                        frame
                            .get("observed_at_utc")
                            .and_then(|value| value.as_str())
                    })
                    .max()
            })
            .map(ToOwned::to_owned)
            .context("NEXRAD product had no observed_at_utc values"),
        "tfrs" => {
            let manifest: serde_json::Value = serde_json::from_slice(
                &fs::read(manifest_path)
                    .with_context(|| format!("failed to read {}", manifest_path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            manifest
                .get("generated_at_utc")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .context("TFR product manifest had no generated_at_utc")
        }
        other => bail!("unsupported fast product id {other}"),
    }
}

fn publish_content_addressed_fast_product_zip(
    build_root: &Path,
    fast_product_id: &str,
    zip_path: &Path,
    known_sha256: Option<&str>,
    known_size_bytes: Option<u64>,
) -> anyhow::Result<(PathBuf, String, u64)> {
    publish_content_addressed_zip(
        build_root,
        zip_path,
        fast_product_id,
        known_sha256,
        known_size_bytes,
    )
}

fn publish_content_addressed_zip(
    build_root: &Path,
    zip_path: &Path,
    file_prefix: &str,
    known_sha256: Option<&str>,
    known_size_bytes: Option<u64>,
) -> anyhow::Result<(PathBuf, String, u64)> {
    let sha256 = match known_sha256 {
        Some(value) => value.to_string(),
        None => hash_file(zip_path)?,
    };
    let size_bytes = match known_size_bytes {
        Some(value) => value,
        None => fs::metadata(zip_path)
            .with_context(|| format!("failed to stat {}", zip_path.display()))?
            .len(),
    };
    let published_path = build_root.join(format!("{file_prefix}_{sha256}.zip"));
    if !published_path.is_file() {
        fs::hard_link(zip_path, &published_path).with_context(|| {
            format!(
                "failed to hardlink {} to {}",
                zip_path.display(),
                published_path.display()
            )
        })?;
    }
    Ok((published_path, sha256, size_bytes))
}

fn run_status_command(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {program}"))?;
    if !status.success() {
        bail!("{program} exited with status {status}");
    }
    Ok(())
}

fn parse_nexrad_index_for_product(path: &Path) -> anyhow::Result<Vec<String>> {
    let html =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut entries = html
        .lines()
        .filter_map(|line| {
            let start = line.find("CONUS_L2_CREF_QCD_")?;
            let tail = &line[start..];
            let end = tail.find(".tif.gz")?;
            Some(tail[..end + ".tif.gz".len()].to_string())
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.reverse();
    entries.dedup();
    Ok(entries)
}

fn build_current_bundle_entries(
    build_root: &Path,
    as_of_date: NaiveDate,
) -> anyhow::Result<Vec<CurrentBundleEntry>> {
    let mut bundle_paths = fs::read_dir(build_root)
        .with_context(|| format!("failed to read {}", build_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", build_root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    (name.starts_with("bundle_cycle_") || name.starts_with("bundle_fast_"))
                        && name.ends_with(".json")
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    bundle_paths.sort();

    let mut cycle_bundles_by_cycle =
        BTreeMap::<String, (u32, String, SystemTime, CurrentBundleEntry)>::new();
    let mut latest_fast_bundle: Option<(String, SystemTime, CurrentBundleEntry)> = None;
    for bundle_path in bundle_paths {
        let metadata = fs::metadata(&bundle_path)
            .with_context(|| format!("failed to stat {}", bundle_path.display()))?;
        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let entry = current_bundle_entry_from_path(&bundle_path)?;
        let filename = entry.filename.clone();
        if filename.starts_with("bundle_cycle_") {
            let end_valid_date = NaiveDate::parse_from_str(&entry.end_valid, "%Y-%m-%d")
                .with_context(|| format!("failed to parse bundle end_valid {}", entry.end_valid))?;
            if end_valid_date < as_of_date {
                continue;
            }
            let bundle_manifest: serde_json::Value = serde_json::from_slice(
                &fs::read(&bundle_path)
                    .with_context(|| format!("failed to read {}", bundle_path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
            let generated_at_utc = bundle_manifest
                .get("generated_at_utc")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let cycle_version_rank = entry.cycle_version.parse::<u32>().unwrap_or(0);
            let should_replace = match cycle_bundles_by_cycle.get(&entry.cycle) {
                Some((
                    existing_version_rank,
                    existing_generated_at_utc,
                    existing_modified_at,
                    _,
                )) => {
                    cycle_version_rank > *existing_version_rank
                        || (cycle_version_rank == *existing_version_rank
                            && generated_at_utc > *existing_generated_at_utc)
                        || (cycle_version_rank == *existing_version_rank
                            && generated_at_utc == *existing_generated_at_utc
                            && modified_at > *existing_modified_at)
                }
                None => true,
            };
            if should_replace {
                cycle_bundles_by_cycle.insert(
                    entry.cycle.clone(),
                    (cycle_version_rank, generated_at_utc, modified_at, entry),
                );
            }
            continue;
        }
        if filename.starts_with("bundle_fast_") {
            let bundle_manifest: FastBundleManifest = serde_json::from_slice(
                &fs::read(&bundle_path)
                    .with_context(|| format!("failed to read {}", bundle_path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
            let published_at_utc = bundle_manifest.published_at_utc.clone();
            let should_replace = match &latest_fast_bundle {
                Some((existing_published_at_utc, existing_modified_at, _)) => {
                    published_at_utc > *existing_published_at_utc
                        || (published_at_utc == *existing_published_at_utc
                            && modified_at > *existing_modified_at)
                }
                None => true,
            };
            if should_replace {
                latest_fast_bundle = Some((published_at_utc, modified_at, entry));
            }
            continue;
        }
    }
    let mut bundles = cycle_bundles_by_cycle
        .into_values()
        .map(|(_, _, _, entry)| entry)
        .collect::<Vec<_>>();
    if let Some((_, _, entry)) = latest_fast_bundle {
        bundles.push(entry);
    }
    bundles.sort_by(|left, right| {
        let left_key = (
            left.bundle_type != "cycle",
            left.cycle.as_str(),
            left.id.as_str(),
        );
        let right_key = (
            right.bundle_type != "cycle",
            right.cycle.as_str(),
            right.id.as_str(),
        );
        left_key.cmp(&right_key)
    });
    Ok(bundles)
}

fn current_bundle_entry_from_path(bundle_path: &Path) -> anyhow::Result<CurrentBundleEntry> {
    let metadata = fs::metadata(bundle_path)
        .with_context(|| format!("failed to stat {}", bundle_path.display()))?;
    let filename = filename_string(bundle_path)?;
    if filename.starts_with("bundle_cycle_") {
        let bundle_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(bundle_path)
                .with_context(|| format!("failed to read {}", bundle_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
        let bundle_cycle = bundle_manifest
            .get("cycle")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing top-level cycle"))?;
        let bundle_cycle_version = bundle_manifest
            .get("cycle_version")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let (file_cycle, file_cycle_version, file_hash) = parse_cycle_bundle_filename(bundle_path)?;
        if bundle_cycle != file_cycle || bundle_cycle_version != file_cycle_version {
            anyhow::bail!(
                "bundle cycle mismatch for {}: payload cycle {}_{} != filename cycle {}_{}",
                bundle_path.display(),
                bundle_cycle,
                bundle_cycle_version,
                file_cycle,
                file_cycle_version
            );
        }
        let bundle_sha256 = hash_file(bundle_path)?;
        if bundle_sha256 != file_hash {
            anyhow::bail!(
                "bundle hash mismatch for {}: filename hash {} != content hash {}",
                bundle_path.display(),
                file_hash,
                bundle_sha256
            );
        }
        let start_valid = bundle_manifest
            .get("start_valid")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing start_valid"))?;
        let end_valid = bundle_manifest
            .get("end_valid")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing end_valid"))?;
        return Ok(CurrentBundleEntry {
            filename: filename.clone(),
            relative_path: filename,
            id: format!("cycle_{bundle_cycle}_{bundle_cycle_version}"),
            bundle_type: "cycle".to_string(),
            cycle: bundle_cycle.to_string(),
            cycle_version: bundle_cycle_version.to_string(),
            start_valid: start_valid.to_string(),
            end_valid: end_valid.to_string(),
            checksum_sha256: bundle_sha256,
            size_bytes: metadata.len(),
        });
    }
    if filename.starts_with("bundle_fast_") {
        let bundle_manifest: FastBundleManifest = serde_json::from_slice(
            &fs::read(bundle_path)
                .with_context(|| format!("failed to read {}", bundle_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
        let file_hash = parse_fast_bundle_filename(bundle_path)?;
        let bundle_sha256 = hash_file(bundle_path)?;
        if bundle_sha256 != file_hash {
            anyhow::bail!(
                "fast bundle hash mismatch for {}: filename hash {} != content hash {}",
                bundle_path.display(),
                file_hash,
                bundle_sha256
            );
        }
        return Ok(CurrentBundleEntry {
            filename: filename.clone(),
            relative_path: filename,
            id: bundle_manifest.bundle_id.clone(),
            bundle_type: "fast".to_string(),
            cycle: String::new(),
            cycle_version: String::new(),
            start_valid: String::new(),
            end_valid: String::new(),
            checksum_sha256: bundle_sha256,
            size_bytes: metadata.len(),
        });
    }
    bail!("unsupported bundle filename {}", bundle_path.display());
}

fn parse_cycle_bundle_filename(path: &Path) -> anyhow::Result<(String, String, String)> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("bundle path has no filename: {}", path.display()))?;
    let stem = filename
        .strip_suffix(".json")
        .ok_or_else(|| anyhow::anyhow!("bundle filename does not end in .json: {filename}"))?;
    let rest = stem.strip_prefix("bundle_cycle_").ok_or_else(|| {
        anyhow::anyhow!("bundle filename must start with bundle_cycle_: {filename}")
    })?;
    let mut parts = rest.rsplitn(3, '_').collect::<Vec<_>>();
    if parts.len() != 3 {
        anyhow::bail!("bundle filename must be bundle_cycle_YYCC_VV_<sha256>.json: {filename}");
    }
    let hash = parts.remove(0).to_string();
    let version = parts.remove(0).to_string();
    let cycle = parts.remove(0).to_string();
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        anyhow::bail!("bundle filename has invalid sha256 suffix: {filename}");
    }
    Ok((cycle, version, hash))
}

fn parse_fast_bundle_filename(path: &Path) -> anyhow::Result<String> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("bundle path has no filename: {}", path.display()))?;
    let stem = filename
        .strip_suffix(".json")
        .ok_or_else(|| anyhow::anyhow!("bundle filename does not end in .json: {filename}"))?;
    let hash = stem.strip_prefix("bundle_fast_").ok_or_else(|| {
        anyhow::anyhow!("bundle filename must start with bundle_fast_: {filename}")
    })?;
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        anyhow::bail!("bundle filename has invalid sha256 suffix: {filename}");
    }
    Ok(hash.to_string())
}

fn current_artifacts_timestamp_string(as_of_utc: DateTime<Utc>) -> String {
    as_of_utc.format("%Y%m%dT%H%M%SZ").to_string()
}

fn current_artifacts_immutable_filename(as_of_utc: DateTime<Utc>) -> String {
    format!(
        "current_artifacts_{}.json",
        current_artifacts_timestamp_string(as_of_utc)
    )
}

fn current_artifacts_latest_alias_filename() -> &'static str {
    "current_artifacts.json"
}

fn write_current_artifacts_json(
    path: &Path,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(manifest)
            .context("failed to encode current artifacts manifest")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn write_current_artifacts_manifest(
    build_root: &Path,
    as_of_utc: DateTime<Utc>,
    diagnostics: Option<CurrentDiagnosticsEntry>,
) -> anyhow::Result<PathBuf> {
    let as_of_date = as_of_utc.date_naive();
    let bundles = build_current_bundle_entries(build_root, as_of_date)?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        as_of_date: as_of_date.format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        diagnostics,
    };
    let immutable_path = build_root.join(current_artifacts_immutable_filename(as_of_utc));
    let latest_alias_path = build_root.join(current_artifacts_latest_alias_filename());
    write_current_artifacts_json(&immutable_path, &manifest)?;
    write_current_artifacts_json(&latest_alias_path, &manifest)?;
    Ok(latest_alias_path)
}

fn write_product_build_diagnostics(
    build_root: &Path,
    as_of_date: NaiveDate,
    task_values: &BTreeMap<String, ProductTaskValue>,
) -> anyhow::Result<Option<CurrentDiagnosticsEntry>> {
    let mut errors = Vec::new();
    for (task_id, task_value) in task_values {
        if !task_id.ends_with(":vectors") {
            continue;
        }
        let cycle = task_id.trim_end_matches(":vectors").to_string();
        let ProductTaskValue::FingerprintedZip {
            errors: Some(errors_path),
            ..
        } = task_value
        else {
            continue;
        };
        let payload: serde_json::Value = serde_json::from_slice(
            &fs::read(errors_path)
                .with_context(|| format!("failed to read {}", errors_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", errors_path.display()))?;
        let product = payload
            .get("product")
            .and_then(|value| value.as_str())
            .unwrap_or("vectors")
            .to_string();
        for error in payload
            .get("errors")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            errors.push(BuildDiagnosticEntry {
                product: product.clone(),
                cycle: Some(cycle.clone()),
                severity: error
                    .get("severity")
                    .and_then(|value| value.as_str())
                    .unwrap_or("ERROR")
                    .to_string(),
                code: error
                    .get("code")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                message: error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unspecified build diagnostic")
                    .to_string(),
                expected: error
                    .get("expected")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize),
                actual: error
                    .get("actual")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize),
            });
        }
    }
    let error_count = errors
        .iter()
        .filter(|error| error.severity == "ERROR")
        .count();
    let filename = format!("build_errors_{}.json", as_of_date.format("%Y%m%d"));
    let path = build_root.join(&filename);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&BuildDiagnosticsManifest {
            schema_version: 1,
            generated_at_utc: utc_now_string(),
            error_count,
            errors,
        })
        .context("failed to encode build diagnostics manifest")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(Some(CurrentDiagnosticsEntry {
        filename,
        error_count,
    }))
}

fn cleanup_published_packaged_root(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let keep = collect_reachable_packaged_entries(packaged_root, current_artifacts_path)?;
    prune_root_to_keep_set(packaged_root, &keep)
}

fn cleanup_published_unpacked_root(
    unpacked_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let keep = collect_reachable_unpacked_entries(current_artifacts_path)?;
    prune_root_to_keep_set(unpacked_root, &keep)
}

fn collect_reachable_packaged_entries(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        let current = load_current_artifacts_manifest(&discovery_path)?;
        keep.insert(filename_string(&discovery_path)?);
        if let Some(diagnostics) = &current.diagnostics {
            keep.insert(diagnostics.filename.clone());
        }
        for bundle_ref in &current.bundles {
            keep.insert(bundle_ref.filename.clone());
            let bundle = load_bundle_manifest_like(&packaged_root.join(&bundle_ref.filename))?;
            let bundle_refs = bundle.bundle_refs();
            for artifact in bundle_refs.ancillary {
                keep.insert(artifact.filename.clone());
            }
            for package in bundle_refs.packages {
                keep.insert(package.filename.clone());
            }
        }
    }
    Ok(keep)
}

fn collect_reachable_unpacked_entries(
    current_artifacts_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    let unpacked_root = current_artifacts_path
        .parent()
        .context("current artifacts path missing parent")?;
    for discovery_path in discovery_manifest_paths(unpacked_root, current_artifacts_path)? {
        let current = load_current_artifacts_manifest(&discovery_path)?;
        keep.insert(filename_string(&discovery_path)?);
        if let Some(diagnostics) = &current.diagnostics {
            keep.insert(diagnostics.filename.clone());
        }
        for bundle_ref in &current.bundles {
            keep.insert(bundle_ref.filename.clone());
            let bundle_path = unpacked_root.join(&bundle_ref.filename);
            let bundle = load_bundle_manifest_like(&bundle_path)?;
            let bundle_refs = bundle.bundle_refs();
            for artifact in bundle_refs.ancillary {
                if artifact.filename.ends_with(".zip") {
                    keep.insert(zip_stem(&artifact.filename)?);
                } else {
                    keep.insert(artifact.filename.clone());
                }
            }
            for package in bundle_refs.packages {
                keep.insert(zip_stem(&package.filename)?);
            }
        }
    }
    Ok(keep)
}

fn discovery_manifest_paths(
    root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = vec![current_artifacts_path.to_path_buf()];
    let mut seen = BTreeSet::from([current_artifacts_path.to_path_buf()]);
    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", root.display()))?
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_discovery = name == current_artifacts_latest_alias_filename()
            || (name.starts_with("current_artifacts_")
                && name.contains('T')
                && name.ends_with(".json"));
        if is_discovery && seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn prune_root_to_keep_set(root: &Path, keep: &BTreeSet<String>) -> anyhow::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if keep.contains(&name) {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove stale directory {}", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale file {}", path.display()))?;
        }
    }
    Ok(())
}

fn load_current_artifacts_manifest(path: &Path) -> anyhow::Result<CurrentArtifactsManifest> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_bundle_manifest_like(path: &Path) -> anyhow::Result<BundleManifestLike> {
    let filename = filename_string(path)?;
    if filename.starts_with("bundle_cycle_") {
        let bundle: BundleManifest = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        return Ok(BundleManifestLike::Cycle(bundle));
    }
    if filename.starts_with("bundle_fast_") {
        let bundle: FastBundleManifest = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        return Ok(BundleManifestLike::Fast(bundle));
    }
    bail!("unrecognized bundle filename: {filename}")
}

fn filename_string(path: &Path) -> anyhow::Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .context("path has no filename")
}

fn validate_packaged_contract(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        validate_no_internal_paths_in_json(&discovery_path)?;
        let current = load_current_artifacts_manifest(&discovery_path)?;

        for bundle in &current.bundles {
            validate_public_filename(&bundle.filename, "current_artifacts.bundles[].filename")?;
            if !bundle.relative_path.is_empty() {
                validate_public_filename(
                    &bundle.relative_path,
                    "current_artifacts.bundles[].relative_path",
                )?;
                if bundle.filename != bundle.relative_path {
                    bail!(
                        "bundle filename/relative_path mismatch in current_artifacts: {} != {}",
                        bundle.filename,
                        bundle.relative_path
                    );
                }
            }
            let bundle_path = packaged_root.join(&bundle.filename);
            ensure_public_file_exists(&bundle_path)?;
            validate_embedded_sha256_filename(&bundle.filename, &bundle.checksum_sha256)?;
            validate_bundle_manifest(packaged_root, &bundle_path)?;
        }
        if let Some(diagnostics) = &current.diagnostics {
            validate_public_filename(
                &diagnostics.filename,
                "current_artifacts.diagnostics.filename",
            )?;
            let diagnostics_path = packaged_root.join(&diagnostics.filename);
            ensure_public_file_exists(&diagnostics_path)?;
            validate_no_internal_paths_in_json(&diagnostics_path)?;
        }
    }

    Ok(())
}

fn validate_bundle_manifest(packaged_root: &Path, bundle_path: &Path) -> anyhow::Result<()> {
    let filename = filename_string(bundle_path)?;
    if filename.starts_with("bundle_fast_") {
        return validate_fast_bundle_manifest(packaged_root, bundle_path);
    }
    validate_no_internal_paths_in_json(bundle_path)?;
    let (_, _, filename_hash) = parse_cycle_bundle_filename(bundle_path)?;
    let bundle_hash = hash_file(bundle_path)?;
    if bundle_hash != filename_hash {
        bail!(
            "bundle filename hash mismatch for {}: filename {} != content {}",
            bundle_path.display(),
            filename_hash,
            bundle_hash
        );
    }
    let bundle: BundleManifest = serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))?;

    for package in &bundle.packages {
        validate_public_filename(&package.filename, "bundle.packages[].filename")?;
        validate_public_filename(&package.relative_path, "bundle.packages[].relative_path")?;
        validate_embedded_sha256_filename(&package.filename, &package.checksum_sha256)?;
        if package.cycle.is_some()
            && package.cycle_version.as_deref() != Some(PACKAGE_CYCLE_VERSION)
        {
            bail!(
                "package {} has unexpected cycle_version {:?}",
                package.id,
                package.cycle_version
            );
        }
        if package.filename != package.relative_path {
            bail!(
                "package filename/relative_path mismatch in {}: {} != {}",
                bundle_path.display(),
                package.filename,
                package.relative_path
            );
        }
        if package.cycle.is_none() {
            if package.cycle_version.is_some() {
                bail!(
                    "stable package {} unexpectedly carries cycle_version {:?}",
                    package.id,
                    package.cycle_version
                );
            }
            if package.effective_date.is_none() {
                bail!("stable package {} is missing effective_date", package.id);
            }
            if package.expiration_date.is_some() {
                bail!(
                    "stable package {} unexpectedly carries expiration_date {:?}",
                    package.id,
                    package.expiration_date
                );
            }
        }
        ensure_public_file_exists(&packaged_root.join(&package.filename))?;
    }
    for artifact in &bundle.ancillary {
        validate_bundle_artifact_ref(packaged_root, artifact)?;
    }
    validate_bundle_contract_split(&bundle, bundle_path)?;
    Ok(())
}

fn validate_fast_bundle_manifest(packaged_root: &Path, bundle_path: &Path) -> anyhow::Result<()> {
    validate_no_internal_paths_in_json(bundle_path)?;
    let filename_hash = parse_fast_bundle_filename(bundle_path)?;
    let bundle_hash = hash_file(bundle_path)?;
    if bundle_hash != filename_hash {
        bail!(
            "fast bundle filename hash mismatch for {}: filename {} != content {}",
            bundle_path.display(),
            filename_hash,
            bundle_hash
        );
    }
    let bundle: FastBundleManifest = serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
    if bundle.bundle_type != "fast" {
        bail!(
            "fast bundle {} has unexpected bundle_type {}",
            bundle_path.display(),
            bundle.bundle_type
        );
    }
    for package in &bundle.packages {
        validate_public_filename(&package.filename, "fast_bundle.packages[].filename")?;
        validate_public_filename(
            &package.relative_path,
            "fast_bundle.packages[].relative_path",
        )?;
        validate_embedded_sha256_filename(&package.filename, &package.checksum_sha256)?;
        if package.filename != package.relative_path {
            bail!(
                "fast bundle filename/relative_path mismatch in {}: {} != {}",
                bundle_path.display(),
                package.filename,
                package.relative_path
            );
        }
        ensure_public_file_exists(&packaged_root.join(&package.filename))?;
    }
    Ok(())
}

fn validate_unpacked_contract(
    packaged_root: &Path,
    unpacked_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    validate_packaged_contract(packaged_root, current_artifacts_path)?;
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        let current_filename = discovery_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("current artifacts path has no filename")?;
        let unpacked_current_path = unpacked_root.join(current_filename);
        ensure_public_file_exists(&unpacked_current_path)?;
        validate_no_internal_paths_in_json(&unpacked_current_path)?;

        let current = load_current_artifacts_manifest(&discovery_path)?;

        for bundle in &current.bundles {
            let unpacked_bundle_path = unpacked_root.join(&bundle.filename);
            ensure_public_file_exists(&unpacked_bundle_path)?;
            validate_no_internal_paths_in_json(&unpacked_bundle_path)?;
            let bundle = load_bundle_manifest_like(&unpacked_bundle_path)?;
            let bundle_refs = bundle.bundle_refs();
            for artifact in bundle_refs.ancillary {
                if artifact.filename.ends_with(".zip") {
                    ensure_public_dir_exists(&unpacked_root.join(zip_stem(&artifact.filename)?))?;
                } else {
                    ensure_public_file_exists(&unpacked_root.join(&artifact.filename))?;
                }
            }
            for package in bundle_refs.packages {
                ensure_public_dir_exists(&unpacked_root.join(zip_stem(&package.filename)?))?;
            }
        }
    }

    Ok(())
}

fn validate_bundle_artifact_ref(
    packaged_root: &Path,
    artifact: &BundleArtifact,
) -> anyhow::Result<()> {
    validate_public_filename(&artifact.filename, "bundle artifact filename")?;
    validate_public_filename(&artifact.relative_path, "bundle artifact relative_path")?;
    validate_embedded_sha256_filename(&artifact.filename, &artifact.checksum_sha256)?;
    if artifact.filename != artifact.relative_path {
        bail!(
            "bundle artifact filename/relative_path mismatch: {} != {}",
            artifact.filename,
            artifact.relative_path
        );
    }
    ensure_public_file_exists(&packaged_root.join(&artifact.filename))
}

fn validate_bundle_contract_split(
    bundle: &BundleManifest,
    bundle_path: &Path,
) -> anyhow::Result<()> {
    let has_vectors_package = bundle
        .packages
        .iter()
        .any(|package| package.family_id == "vectors" && package.region_id.is_none());
    if !has_vectors_package {
        bail!(
            "bundle {} missing vectors package row in packages[]",
            bundle_path.display()
        );
    }
    let has_nav_db_package = bundle
        .packages
        .iter()
        .any(|package| package.family_id == "nav-db" && package.region_id.is_none());
    if !has_nav_db_package {
        bail!(
            "bundle {} missing nav-db package row in packages[]",
            bundle_path.display()
        );
    }

    for package in &bundle.packages {
        if bundle
            .ancillary
            .iter()
            .any(|artifact| artifact.filename == package.filename)
        {
            bail!(
                "bundle {} lists {} in both packages[] and ancillary[]",
                bundle_path.display(),
                package.filename
            );
        }
    }
    for forbidden in ["resource_index_", "catalog_", "data_"] {
        if bundle
            .packages
            .iter()
            .any(|package| package.filename.starts_with(forbidden))
        {
            bail!(
                "bundle {} contains transitional artifact prefix {} in packages[]",
                bundle_path.display(),
                forbidden
            );
        }
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("data_"))
    {
        bail!(
            "bundle {} still publishes data zip in ancillary[]",
            bundle_path.display()
        );
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("catalog_"))
    {
        bail!(
            "bundle {} still publishes catalog in ancillary[]",
            bundle_path.display()
        );
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("resource_index_"))
    {
        bail!(
            "bundle {} still publishes resource_index in ancillary[]",
            bundle_path.display()
        );
    }
    for forbidden in ["nav_kv_"] {
        if bundle
            .ancillary
            .iter()
            .any(|artifact| artifact.filename.starts_with(forbidden))
        {
            bail!(
                "bundle {} contains unpacked-only artifact prefix {} in ancillary[]",
                bundle_path.display(),
                forbidden
            );
        }
    }
    Ok(())
}

fn validate_embedded_sha256_filename(filename: &str, checksum_sha256: &str) -> anyhow::Result<()> {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow::anyhow!("filename has no stem: {filename}"))?;
    if let Some(suffix) = stem.rsplit('_').next() {
        if suffix.len() == 64 && suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
            if suffix != checksum_sha256 {
                bail!(
                    "embedded sha256 mismatch for {filename}: filename {suffix} != checksum {checksum_sha256}"
                );
            }
        }
    }
    Ok(())
}

fn validate_public_filename(value: &str, field: &str) -> anyhow::Result<()> {
    if value
        != Path::new(value)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    {
        bail!("{field} must be a basename, got {value}");
    }
    if value.contains('/') || value.contains('\\') {
        bail!("{field} must not contain path separators: {value}");
    }
    Ok(())
}

fn ensure_public_file_exists(path: &Path) -> anyhow::Result<()> {
    let meta =
        fs::metadata(path).with_context(|| format!("missing published file {}", path.display()))?;
    if !meta.is_file() {
        bail!(
            "expected published file, found non-file at {}",
            path.display()
        );
    }
    Ok(())
}

fn ensure_public_dir_exists(path: &Path) -> anyhow::Result<()> {
    let meta =
        fs::metadata(path).with_context(|| format!("missing published dir {}", path.display()))?;
    if !meta.is_dir() {
        bail!(
            "expected published dir, found non-dir at {}",
            path.display()
        );
    }
    Ok(())
}

fn zip_stem(filename: &str) -> anyhow::Result<String> {
    let path = Path::new(filename);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    if extension != "zip" {
        bail!("expected zip filename, got {filename}");
    }
    Ok(path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("zip filename missing stem")?
        .to_string())
}

fn validate_no_internal_paths_in_json(path: &Path) -> anyhow::Result<()> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_no_internal_paths_in_value(path, "$", &value)
}

fn validate_no_internal_paths_in_value(
    path: &Path,
    json_path: &str,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::String(text) => {
            for forbidden in [
                "cache/",
                "private-work/",
                "work/",
                "published-packaged/production",
            ] {
                if text.contains(forbidden) {
                    bail!(
                        "{} contains forbidden internal path fragment at {}: {}",
                        path.display(),
                        json_path,
                        text
                    );
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_no_internal_paths_in_value(path, &format!("{json_path}[{index}]"), item)?;
            }
            Ok(())
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                validate_no_internal_paths_in_value(path, &format!("{json_path}.{key}"), item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn remove_legacy_unpacked_subtree(unpacked_root: &Path) -> anyhow::Result<()> {
    let legacy = unpacked_root.join("production");
    if legacy.exists() {
        fs::remove_dir_all(&legacy)
            .with_context(|| format!("failed to remove legacy {}", legacy.display()))?;
    }
    Ok(())
}

fn manifest_generated_at(node_records: &[NodeRecord]) -> String {
    node_records
        .iter()
        .map(|record| record.finished_at_utc.as_str())
        .max()
        .unwrap_or_else(|| panic!("build manifest should include at least one node"))
        .to_string()
}

fn gc_roots_path(config: &ProductBuildConfig) -> PathBuf {
    artifact_root_from_build_root(&config.build_root)
        .join("cache")
        .join("gc_roots")
        .join(format!("{}_build_roots.json", config.profile.as_str()))
}

fn load_gc_roots(path: &Path, config: &ProductBuildConfig) -> anyhow::Result<GcRootsManifest> {
    if path.is_file() {
        return serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()));
    }
    Ok(GcRootsManifest {
        schema_version: 1,
        profile: config.profile.as_str().to_string(),
        build_root: relative_product_build_path(&config.build_root),
        updated_at_utc: utc_now_string(),
        node_roots: BTreeMap::new(),
    })
}

fn write_gc_roots(path: &Path, roots: &GcRootsManifest) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(
        &temp_path,
        serde_json::to_vec_pretty(roots).context("failed to encode GC roots")?,
    )
    .with_context(|| format!("failed to write {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })
}

fn record_gc_roots(
    config: &ProductBuildConfig,
    scope: &str,
    task_records: &BTreeMap<String, Vec<NodeRecord>>,
) -> anyhow::Result<PathBuf> {
    let roots_path = gc_roots_path(config);
    let mut roots = load_gc_roots(&roots_path, config)?;
    let now = utc_now_string();
    roots.schema_version = 1;
    roots.profile = config.profile.as_str().to_string();
    roots.build_root = relative_product_build_path(&config.build_root);
    roots.updated_at_utc = now.clone();
    let prefix = format!("{scope}:");
    roots.node_roots.retain(|key, _| !key.starts_with(&prefix));
    let cache_nodes_root = artifact_root_from_build_root(&config.build_root)
        .join("cache")
        .join("nodes");
    for (task_id, records) in task_records {
        for record in records {
            let key = format!("{scope}:{task_id}:{}:{}", record.name, record.fingerprint);
            let node_dir = cache_nodes_root
                .join(&record.name)
                .join(&record.fingerprint);
            let record_path = node_dir.join("build-record.json");
            roots.node_roots.insert(
                key,
                GcNodeRoot {
                    scope: scope.to_string(),
                    task_id: task_id.clone(),
                    node_name: record.name.clone(),
                    fingerprint: record.fingerprint.clone(),
                    node_dir: relative_artifact_path(&node_dir, &config.build_root),
                    record_path: relative_artifact_path(&record_path, &config.build_root),
                    cache_hit: record.cache_hit,
                    finished_at_utc: record.finished_at_utc.clone(),
                    updated_at_utc: now.clone(),
                },
            );
        }
    }
    write_gc_roots(&roots_path, &roots)?;
    Ok(roots_path)
}

fn record_gc_roots_from_build_manifest(
    config: &ProductBuildConfig,
    scope: &str,
    build_manifest: &BuildManifest,
) -> anyhow::Result<PathBuf> {
    let mut task_records = BTreeMap::<String, Vec<NodeRecord>>::new();
    for record in &build_manifest.nodes {
        task_records
            .entry(record.name.clone())
            .or_default()
            .push(record.clone());
    }
    record_gc_roots(config, scope, &task_records)
}

fn bootstrap_gc_roots_from_build_manifests(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    let manifest_dir = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("build-manifests")
        .join(
            config
                .build_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(config.profile.as_str()),
        );
    let mut records = BTreeMap::<String, Vec<NodeRecord>>::new();
    if manifest_dir.is_dir() {
        for entry in fs::read_dir(&manifest_dir)
            .with_context(|| format!("failed to read {}", manifest_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("build-manifest_") && name.ends_with(".json"))
            {
                continue;
            }
            let manifest: BuildManifest = serde_json::from_slice(
                &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", path.display()))?;
            records.insert(format!("{}:build-manifest", manifest.cycle), manifest.nodes);
        }
    }
    if records.is_empty() {
        bail!(
            "no build-manifest_*.json files found in {}",
            manifest_dir.display()
        );
    }
    record_gc_roots(config, "full", &records)
}

pub fn gc_build_cache(config: &BuildCacheGcConfig) -> anyhow::Result<BuildCacheGcReport> {
    let product_config = ProductBuildConfig {
        chart_cutline_root: PathBuf::new(),
        build_root: config.build_root.clone(),
        profile: config.profile,
        target_cycle: None,
        fetch_jobs: 1,
        cpu_jobs: 1,
        max_heavy_jobs: 1,
        fetch_cache_root: artifact_root_from_build_root(&config.build_root)
            .join("cache")
            .join("fetch"),
        fetch_cache_mode: "cache-first".to_string(),
    };
    if config.bootstrap_from_build_manifests {
        bootstrap_gc_roots_from_build_manifests(&product_config)?;
    }
    let roots_path = gc_roots_path(&product_config);
    let roots = load_gc_roots(&roots_path, &product_config)?;
    let cache_nodes_root = artifact_root_from_build_root(&config.build_root)
        .join("cache")
        .join("nodes");
    let rooted = roots
        .node_roots
        .values()
        .map(|root| (root.node_name.clone(), root.fingerprint.clone()))
        .collect::<BTreeSet<_>>();
    let mut report = BuildCacheGcReport {
        roots_path,
        rooted_nodes: rooted.len(),
        scanned_nodes: 0,
        active_nodes: 0,
        stale_lock_nodes: 0,
        grace_nodes: 0,
        evictable_nodes: 0,
        reclaimed_bytes: 0,
        scratch_files: 0,
        scratch_bytes: 0,
        scratch_active_nodes: 0,
        private_scratch_files: 0,
        private_scratch_bytes: 0,
        private_scratch_active_nodes: 0,
        by_node_name: BTreeMap::new(),
    };
    if !cache_nodes_root.is_dir() {
        return Ok(report);
    }
    let grace = Duration::from_secs(config.grace_hours.saturating_mul(3600));
    let now = SystemTime::now();
    for node_entry in fs::read_dir(&cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let fingerprint = fingerprint_entry.file_name().to_string_lossy().to_string();
            report.scanned_nodes += 1;
            if rooted.contains(&(node_name.clone(), fingerprint.clone())) {
                continue;
            }
            let node_dir = fingerprint_entry.path();
            let lock_path = node_dir.join(".build-lock");
            if lock_path.exists() {
                if lock_is_live(&lock_path)? {
                    report.active_nodes += 1;
                    continue;
                }
                report.stale_lock_nodes += 1;
            }
            if is_younger_than(&node_dir, now, grace)? {
                report.grace_nodes += 1;
                continue;
            }
            let bytes = directory_size(&node_dir)?;
            report.evictable_nodes += 1;
            report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
            let bucket = report.by_node_name.entry(node_name.clone()).or_default();
            bucket.count += 1;
            bucket.bytes = bucket.bytes.saturating_add(bytes);
            if config.mode == BuildCacheGcMode::Execute {
                set_tree_readonly(&node_dir, false)?;
                fs::remove_dir_all(&node_dir)
                    .with_context(|| format!("failed to remove {}", node_dir.display()))?;
            }
        }
    }
    scrub_rooted_tpp_render_scratch(&cache_nodes_root, &rooted, config.mode, &mut report)?;
    scrub_terrain_private_work_scratch(
        &config.build_root,
        &cache_nodes_root,
        config.mode,
        &mut report,
    )?;
    Ok(report)
}

fn scrub_terrain_private_work_scratch(
    build_root: &Path,
    cache_nodes_root: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    let private_terrain_root = artifact_root_from_build_root(build_root)
        .join("private-work")
        .join("terrain");
    if !private_terrain_root.exists() {
        return Ok(());
    }
    if terrain_node_build_is_active(cache_nodes_root)? {
        report.private_scratch_active_nodes += 1;
        return Ok(());
    }
    report.private_scratch_files += count_files_in_dir(&private_terrain_root)?;
    report.private_scratch_bytes = report
        .private_scratch_bytes
        .saturating_add(directory_size(&private_terrain_root)?);
    if mode == BuildCacheGcMode::Execute {
        fs::remove_dir_all(&private_terrain_root)
            .with_context(|| format!("failed to remove {}", private_terrain_root.display()))?;
    }
    Ok(())
}

fn terrain_node_build_is_active(cache_nodes_root: &Path) -> anyhow::Result<bool> {
    if !cache_nodes_root.is_dir() {
        return Ok(false);
    }
    for node_entry in fs::read_dir(cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        if !node_name.starts_with("static-terrain-") {
            continue;
        }
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let lock_path = fingerprint_entry.path().join(".build-lock");
            if lock_path.exists() && lock_is_live(&lock_path)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn count_files_in_dir(dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            count += count_files_in_dir(&entry.path())?;
        } else {
            count += 1;
        }
    }
    Ok(count)
}

fn scrub_rooted_tpp_render_scratch(
    cache_nodes_root: &Path,
    rooted: &BTreeSet<(String, String)>,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for (node_name, fingerprint) in rooted {
        if !is_tpp_render_node_name(node_name) {
            continue;
        }
        let node_dir = cache_nodes_root.join(node_name).join(fingerprint);
        if !node_dir.is_dir() {
            continue;
        }
        let lock_path = node_dir.join(".build-lock");
        if lock_path.exists() && lock_is_live(&lock_path)? {
            report.scratch_active_nodes += 1;
            continue;
        }
        scrub_tpp_render_scratch_dir(&node_dir, mode, report)?;
    }
    Ok(())
}

fn is_tpp_render_node_name(node_name: &str) -> bool {
    node_name.starts_with("tpp-") && node_name.ends_with("-render")
}

fn scrub_tpp_render_scratch_dir(
    dir: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scrub_tpp_render_scratch_dir(&path, mode, report)?;
            continue;
        }
        if !is_tpp_render_scratch_file(&path) {
            continue;
        }
        let bytes = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", path.display()))?
            .len();
        report.scratch_files += 1;
        report.scratch_bytes = report.scratch_bytes.saturating_add(bytes);
        if mode == BuildCacheGcMode::Execute {
            set_path_readonly(&path, false)?;
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn is_tpp_render_scratch_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    if extension.eq_ignore_ascii_case("pdf") {
        return true;
    }
    if !(extension.eq_ignore_ascii_case("tif") || extension.eq_ignore_ascii_case("tiff")) {
        return false;
    }
    path.components()
        .any(|component| component.as_os_str() == "plates")
}

fn is_younger_than(path: &Path, now: SystemTime, grace: Duration) -> anyhow::Result<bool> {
    if grace.is_zero() {
        return Ok(false);
    }
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .modified()
        .with_context(|| format!("failed to read mtime for {}", path.display()))?;
    Ok(now
        .duration_since(modified)
        .map(|age| age < grace)
        .unwrap_or(true))
}

fn lock_is_live(lock_path: &Path) -> anyhow::Result<bool> {
    let Some(pid) = read_lock_pid(lock_path)? else {
        return Ok(true);
    };
    Ok(process_is_alive(pid))
}

fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        total = total.saturating_add(directory_size(&entry.path())?);
    }
    Ok(total)
}

fn bundle_artifact(
    absolute_path: &Path,
    published_filename: &str,
) -> anyhow::Result<BundleArtifact> {
    Ok(BundleArtifact {
        filename: published_filename.to_string(),
        relative_path: published_filename.to_string(),
        checksum_sha256: hash_file(absolute_path)?,
        size_bytes: fs::metadata(absolute_path)
            .with_context(|| format!("failed to stat {}", absolute_path.display()))?
            .len(),
    })
}

fn write_hashed_bundle_manifest(
    build_root: &Path,
    bundle_manifest: &BundleManifest,
) -> anyhow::Result<PathBuf> {
    let bytes =
        serde_json::to_vec_pretty(bundle_manifest).context("failed to encode bundle manifest")?;
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let bundle_manifest_path = build_root.join(format!(
        "bundle_cycle_{}_{}_{sha256}.json",
        bundle_manifest.cycle, bundle_manifest.cycle_version
    ));
    fs::write(&bundle_manifest_path, bytes)
        .with_context(|| format!("failed to write {}", bundle_manifest_path.display()))?;
    Ok(bundle_manifest_path)
}

fn write_hashed_fast_bundle_manifest(
    build_root: &Path,
    bundle_manifest: &FastBundleManifest,
) -> anyhow::Result<PathBuf> {
    let bytes = serde_json::to_vec_pretty(bundle_manifest)
        .context("failed to encode fast bundle manifest")?;
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let bundle_manifest_path = build_root.join(format!("bundle_fast_{sha256}.json"));
    fs::write(&bundle_manifest_path, bytes)
        .with_context(|| format!("failed to write {}", bundle_manifest_path.display()))?;
    Ok(bundle_manifest_path)
}

fn publish_bundle_artifact(
    config: &ProductBuildConfig,
    absolute_path: &Path,
    published_filename: &str,
) -> anyhow::Result<BundleArtifact> {
    let published_path = config.build_root.join(published_filename);
    publish_flat_artifact(absolute_path, &published_path)?;
    bundle_artifact(absolute_path, published_filename)
}

fn publish_flat_artifact(source_path: &Path, published_path: &Path) -> anyhow::Result<()> {
    if published_path.exists() {
        fs::remove_file(published_path)
            .with_context(|| format!("failed to remove {}", published_path.display()))?;
    }
    fs::hard_link(source_path, published_path).with_context(|| {
        format!(
            "failed to hardlink {} to {}",
            source_path.display(),
            published_path.display()
        )
    })?;
    Ok(())
}

fn canonical_package_filename(
    family_id: &str,
    region_id: &str,
    original_filename: &str,
) -> anyhow::Result<String> {
    let cycle = package_version_from_filename(original_filename)?;
    Ok(format!(
        "{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
        cycle
    ))
}

fn canonical_package_filename_hashed(
    family_id: &str,
    region_id: &str,
    original_filename: &str,
    checksum_sha256: &str,
) -> anyhow::Result<String> {
    let cycle = package_version_from_filename(original_filename)?;
    Ok(format!(
        "{}_{}_{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
        cycle,
        PACKAGE_CYCLE_VERSION,
        checksum_sha256
    ))
}

fn package_version_from_filename(original_filename: &str) -> anyhow::Result<String> {
    Path::new(original_filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit('_').next())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("failed to derive cycle from package filename {original_filename}")
        })
}

impl ProductBuildConfig {
    pub fn from_env_and_args(args: &[String]) -> anyhow::Result<Self> {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("preprocessor-cli crate should live under the workspace root")
            .to_path_buf();
        let repo_root = workspace_root
            .parent()
            .expect("workspace root should live under product/")
            .parent()
            .expect("product should live under the repo root")
            .to_path_buf();
        let artifact_root = default_artifact_write_path(&repo_root);

        let mut profile = ProductBuildProfile::Production;
        let mut chart_cutline_root = repo_root.join("avare-assets").join("chart-cutlines");
        let mut build_root = match profile {
            ProductBuildProfile::Production => artifact_root.join("published-packaged"),
            ProductBuildProfile::Validation => artifact_root.join("published-packaged-validation"),
        };
        let mut target_cycle = None;
        let mut fetch_jobs = env_usize("FETCH_JOBS").unwrap_or(4);
        let mut cpu_jobs = env_usize("CPU_JOBS").unwrap_or_else(default_cpu_jobs);
        let mut max_heavy_jobs = env_usize("MAX_HEAVY_JOBS").unwrap_or(4).max(1);
        let fetch_cache_root = env_path("FETCH_CACHE_ROOT")
            .unwrap_or_else(|| artifact_root.join("cache").join("fetch"));
        let fetch_cache_mode = env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string());

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--profile" => {
                    let value = args.get(index + 1).context("missing value for --profile")?;
                    profile = ProductBuildProfile::parse(value)
                        .ok_or_else(|| anyhow::anyhow!("unsupported profile: {value}"))?;
                    build_root = match profile {
                        ProductBuildProfile::Production => artifact_root.join("published-packaged"),
                        ProductBuildProfile::Validation => {
                            artifact_root.join("published-packaged-validation")
                        }
                    };
                    index += 2;
                }
                "--chart-cutline-root" => {
                    chart_cutline_root = PathBuf::from(
                        args.get(index + 1)
                            .context("missing value for --chart-cutline-root")?,
                    );
                    index += 2;
                }
                "--build-root" => {
                    build_root = PathBuf::from(
                        args.get(index + 1)
                            .context("missing value for --build-root")?,
                    );
                    index += 2;
                }
                "--cycle" => {
                    target_cycle = Some(
                        args.get(index + 1)
                            .context("missing value for --cycle")?
                            .to_string(),
                    );
                    index += 2;
                }
                "--fetch-jobs" => {
                    fetch_jobs = args
                        .get(index + 1)
                        .context("missing value for --fetch-jobs")?
                        .parse()
                        .context("failed to parse --fetch-jobs")?;
                    index += 2;
                }
                "--cpu-jobs" => {
                    cpu_jobs = args
                        .get(index + 1)
                        .context("missing value for --cpu-jobs")?
                        .parse()
                        .context("failed to parse --cpu-jobs")?;
                    index += 2;
                }
                "--max-heavy-jobs" => {
                    max_heavy_jobs = args
                        .get(index + 1)
                        .context("missing value for --max-heavy-jobs")?
                        .parse()
                        .context("failed to parse --max-heavy-jobs")?;
                    max_heavy_jobs = max_heavy_jobs.max(1);
                    index += 2;
                }
                "--as-of-utc" | "--bundle" => {
                    index += 2;
                }
                other => bail!("unknown cycle-build argument: {other}"),
            }
        }

        Ok(Self {
            chart_cutline_root,
            build_root,
            profile,
            target_cycle,
            fetch_jobs,
            cpu_jobs,
            max_heavy_jobs,
            fetch_cache_root,
            fetch_cache_mode,
        })
    }
}

fn build_source_urls_node(config: &ProductBuildConfig) -> anyhow::Result<(PathBuf, NodeRecord)> {
    if let Some(override_root) = env_path("AEROBAG_SOURCE_URLS_ROOT") {
        return build_overridden_source_urls_node(config, &override_root);
    }
    let resolved_cycle = match &config.target_cycle {
        Some(cycle) => cycle.clone(),
        None => discover_published_cycles(Some(&fetch_cache_config(config)?))?
            .into_iter()
            .last()
            .context("no published FAA cycles discovered")?,
    };
    let emit_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/emit_source_urls.rs");
    let mut inputs = BTreeMap::from([("emit_source".to_string(), hash_file(&emit_source)?)]);
    inputs.insert("target_cycle".to_string(), hash_text(&resolved_cycle));
    let shared_root = build_shared_node_dir(config, "source-urls")?;
    let prepared = prepare_node_at(&shared_root, "source-urls", &inputs)?;
    let output_dir = prepared.dir.join("out");
    let expected = vec![
        output_dir.join("charts-sec/source_urls.jsonl"),
        output_dir.join("charts-tac/source_urls.jsonl"),
        output_dir.join("charts-enr-l/source_urls.jsonl"),
        output_dir.join("charts-enr-h/source_urls.jsonl"),
        output_dir.join("csup/source_urls.jsonl"),
        output_dir.join("tpp-ak/source_urls.jsonl"),
        output_dir.join("tpp-pac/source_urls.jsonl"),
        output_dir.join("tpp-sw/source_urls.jsonl"),
        output_dir.join("tpp-nc/source_urls.jsonl"),
        output_dir.join("tpp-ec/source_urls.jsonl"),
        output_dir.join("tpp-sc/source_urls.jsonl"),
        output_dir.join("tpp-ne/source_urls.jsonl"),
        output_dir.join("tpp-nw/source_urls.jsonl"),
        output_dir.join("tpp-se/source_urls.jsonl"),
        output_dir.join("data/source_urls.jsonl"),
    ];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        fs::create_dir_all(&output_dir)?;
        emit_source_urls(
            &output_dir,
            Some(&resolved_cycle),
            Some(&fetch_cache_config(config)?),
        )?;
        Ok(BTreeMap::from([(
            "output_dir".to_string(),
            relative_artifact_path(&output_dir, &config.build_root),
        )]))
    })?;
    Ok((output_dir, record))
}

fn fetch_cache_config(config: &ProductBuildConfig) -> anyhow::Result<FetchCacheConfig> {
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    })
}

fn static_source_fetch_cache_config(
    config: &ProductBuildConfig,
) -> anyhow::Result<FetchCacheConfig> {
    let mode =
        env::var("STATIC_SOURCE_FETCH_CACHE_MODE").unwrap_or_else(|_| "cache-first".to_string());
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&mode)?,
    })
}

fn terrain_fetch_cache_config(config: &ProductBuildConfig) -> anyhow::Result<FetchCacheConfig> {
    let mode = env::var("TERRAIN_FETCH_CACHE_MODE").unwrap_or_else(|_| "cache-first".to_string());
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&mode)?,
    })
}

fn include_static_terrain_products() -> bool {
    env::var("AEROBAG_SKIP_STATIC_TERRAIN_PRODUCTS")
        .map(|value| value != "1" && !value.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
}

fn build_overridden_source_urls_node(
    config: &ProductBuildConfig,
    override_root: &Path,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    let inputs = BTreeMap::from([("source_urls_root".to_string(), hash_tree(override_root)?)]);
    let shared_root = build_shared_node_dir(config, "source-urls")?;
    let prepared = prepare_node_at(&shared_root, "source-urls", &inputs)?;
    let output_dir = prepared.dir.join("out");
    let expected = vec![
        output_dir.join("charts-sec/source_urls.jsonl"),
        output_dir.join("charts-tac/source_urls.jsonl"),
        output_dir.join("charts-enr-l/source_urls.jsonl"),
        output_dir.join("charts-enr-h/source_urls.jsonl"),
        output_dir.join("csup/source_urls.jsonl"),
        output_dir.join("tpp-ak/source_urls.jsonl"),
        output_dir.join("tpp-pac/source_urls.jsonl"),
        output_dir.join("tpp-sw/source_urls.jsonl"),
        output_dir.join("tpp-nc/source_urls.jsonl"),
        output_dir.join("tpp-ec/source_urls.jsonl"),
        output_dir.join("tpp-sc/source_urls.jsonl"),
        output_dir.join("tpp-ne/source_urls.jsonl"),
        output_dir.join("tpp-nw/source_urls.jsonl"),
        output_dir.join("tpp-se/source_urls.jsonl"),
        output_dir.join("data/source_urls.jsonl"),
    ];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir)
                .with_context(|| format!("failed to remove {}", output_dir.display()))?;
        }
        copy_dir_recursive(override_root, &output_dir)?;
        Ok(BTreeMap::from([(
            "output_dir".to_string(),
            relative_artifact_path(&output_dir, &config.build_root),
        )]))
    })?;
    Ok((output_dir, record))
}

fn build_chart_render_node(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
    cpu_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(family).to_string();
    let node_name = format!("charts-{family_id}-render");
    let inputs = chart_render_inputs(family, source_repo, source_urls, fetch_jobs, cpu_jobs)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let work_dir = prepared.dir.join("work").join(family.capture_label());
    let tiles_root = work_dir.join("tiles");
    run_cached_node(prepared, inputs, &[tiles_root.clone()], |prepared| {
        let work_dir = stage_work_dir(family, source_repo, &prepared.dir)?;
        let provenance_dir = prepared
            .dir
            .join("meta")
            .join("provenance")
            .join(format!("charts-{family_id}"));
        fs::create_dir_all(&provenance_dir)?;
        copy_source_urls_provenance(source_urls, &provenance_dir)?;
        let urls = read_source_urls_jsonl(source_urls)?;
        prefetch_archives_with_provenance(
            &urls,
            &work_dir,
            fetch_jobs,
            Some(&static_source_fetch_cache_config(config)?),
            &provenance_dir,
            family.capture_label(),
        )?;
        build_family_vrts(family, &work_dir, cpu_jobs)?;
        build_family_tiles(family, &work_dir, cpu_jobs)?;
        Ok(BTreeMap::from([
            (
                "work_dir".to_string(),
                relative_artifact_path(&work_dir, &config.build_root),
            ),
            (
                "tiles_root".to_string(),
                relative_artifact_path(&tiles_root, &config.build_root),
            ),
        ]))
    })
}

fn build_chart_package_nodes(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, ChartSource)> {
    let family_id = family_slug(family).to_string();
    let source_urls_path = source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl"));
    let render_node_name = format!("charts-{family_id}-render");
    let render_inputs = chart_render_inputs(
        family,
        &config.chart_cutline_root,
        &source_urls_path,
        config.fetch_jobs,
        config.cpu_jobs.min(8).max(1),
    )?;
    let render_prepared = prepare_node_at(
        &build_shared_node_dir(config, &render_node_name)?,
        &render_node_name,
        &render_inputs,
    )?;
    let render_record = load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
    let work_dir = resolve_artifact_path(config, output_path(&render_record, "work_dir")?);
    let provenance_dir = render_prepared
        .dir
        .join("meta")
        .join("provenance")
        .join(format!("charts-{family_id}"));
    let aggregate_path = provenance_dir.join("package_outputs.jsonl");
    let node_records = build_regional_package_nodes(
        config,
        &aggregate_path,
        "chart",
        |region| {
            let node_name = format!(
                "charts-{family_id}-package-{}",
                region.code().to_ascii_lowercase()
            );
            let inputs = BTreeMap::from([
                (
                    "render_fingerprint".to_string(),
                    render_record.fingerprint.clone(),
                ),
                ("region".to_string(), region.code().to_string()),
                ("version_label".to_string(), version_label.to_string()),
                (
                    "chart_package_lib".to_string(),
                    hash_file(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .expect("preprocessor-cli should live under workspace root")
                            .join("preprocessor-charts/src/lib.rs"),
                    )?,
                ),
            ]);
            Ok(RegionalPackageSpec {
                region,
                node_name,
                inputs,
                zip_path: work_dir.join(format!(
                    "{}_{}_{}.zip",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                )),
                manifest_path: work_dir.join(format!(
                    "{}_{}_{}.manifest",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                )),
            })
        },
        |region, manifest_path, zip_path| {
            Ok(PackageOutputRecord {
                label: family.capture_label().to_string(),
                chart: Some(manifest_chart_name(family).to_string()),
                region: region.code().to_string(),
                manifest: format!(
                    "{}_{}_{}.manifest",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                ),
                manifest_sha256: hash_file(manifest_path)?,
                zip: format!(
                    "{}_{}_{}.zip",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                ),
                zip_sha256: hash_file(zip_path)?,
                metadata: BTreeMap::from([(
                    "full_coverage_zoom".to_string(),
                    serde_json::Value::from(FULL_COVERAGE_ZOOM),
                )]),
            })
        },
        |region| {
            package_family_region_versioned(family, &work_dir, region, version_label, version_label)
        },
    )?;
    Ok((
        node_records,
        ChartSource {
            family_id,
            package_outputs_path: aggregate_path,
            package_root: work_dir,
            source_urls_path: Some(source_urls_path),
        },
    ))
}

struct RegionalPackageSpec {
    region: Region,
    node_name: String,
    inputs: BTreeMap<String, String>,
    zip_path: PathBuf,
    manifest_path: PathBuf,
}

fn build_regional_package_nodes<MakeSpec, FallbackRecord, BuildPackage>(
    config: &ProductBuildConfig,
    aggregate_path: &Path,
    aggregate_label: &str,
    mut make_spec: MakeSpec,
    mut fallback_record: FallbackRecord,
    mut build_package: BuildPackage,
) -> anyhow::Result<Vec<NodeRecord>>
where
    MakeSpec: FnMut(Region) -> anyhow::Result<RegionalPackageSpec>,
    FallbackRecord: FnMut(Region, &Path, &Path) -> anyhow::Result<PackageOutputRecord>,
    BuildPackage: FnMut(Region) -> anyhow::Result<PackageOutputRecord>,
{
    let existing_package_records = read_package_outputs_by_region(aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let spec = make_spec(region)?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &spec.node_name)?,
            &spec.node_name,
            &spec.inputs,
        )?;
        let expected_outputs = [spec.zip_path.clone(), spec.manifest_path.clone()];
        if let Some(record) = try_load_node_record(&prepared, &expected_outputs)? {
            node_records.push(record);
            package_records.push(package_record_for_cached_region(
                &existing_package_records,
                &mut fallback_record,
                spec.region,
                &spec.manifest_path,
                &spec.zip_path,
            )?);
            continue;
        }
        let _build_lock = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
            NodeCacheState::CacheHit(record) => {
                node_records.push(record);
                package_records.push(package_record_for_cached_region(
                    &existing_package_records,
                    &mut fallback_record,
                    spec.region,
                    &spec.manifest_path,
                    &spec.zip_path,
                )?);
                continue;
            }
            NodeCacheState::Build(lock) => lock,
        };
        let started_at_utc = utc_now_string();
        let started = Instant::now();
        let package_record = build_package(spec.region)?;
        let outputs = BTreeMap::from([
            (
                "zip".to_string(),
                relative_artifact_path(&spec.zip_path, &config.build_root),
            ),
            (
                "manifest".to_string(),
                relative_artifact_path(&spec.manifest_path, &config.build_root),
            ),
        ]);
        let record = write_node_record(
            prepared,
            spec.inputs,
            outputs,
            false,
            started_at_utc,
            utc_now_string(),
            started.elapsed().as_millis() as u64,
        )?;
        node_records.push(record);
        package_records.push(package_record);
    }
    let parent = aggregate_path
        .parent()
        .with_context(|| format!("{aggregate_label} aggregate path missing parent"))?;
    fs::create_dir_all(parent)?;
    write_package_outputs_jsonl(parent, &package_records)?;
    Ok(node_records)
}

fn package_record_for_cached_region<FallbackRecord>(
    existing_package_records: &BTreeMap<String, PackageOutputRecord>,
    fallback_record: &mut FallbackRecord,
    region: Region,
    manifest_path: &Path,
    zip_path: &Path,
) -> anyhow::Result<PackageOutputRecord>
where
    FallbackRecord: FnMut(Region, &Path, &Path) -> anyhow::Result<PackageOutputRecord>,
{
    existing_package_records
        .get(region.code())
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| fallback_record(region, manifest_path, zip_path))
}

fn build_csup_render_node(
    config: &ProductBuildConfig,
    region: Region,
    work_dir: &Path,
    stage_fingerprint: &str,
    version_label: &str,
    render_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
    let inputs = csup_render_inputs(stage_fingerprint, region, render_jobs, version_label)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let marker = work_dir.join(format!(
        ".render-complete-{}",
        region.code().to_ascii_lowercase()
    ));
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&marker),
        |_prepared| {
            render_csup_region(work_dir, region, render_jobs)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(work_dir, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

fn build_csup_stage_node(
    config: &ProductBuildConfig,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let inputs = csup_stage_inputs(source_urls, fetch_jobs)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-stage")?,
        "csup-stage",
        &inputs,
    )?;
    let work_root = prepared.dir.clone();
    let marker = work_root.join(".stage-complete");
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&marker),
        |_prepared| {
            let work_dir = stage_work_dir_for_product(source_repo, &work_root)?;
            let provenance_dir = work_root.join("meta").join("provenance").join("csup");
            fs::create_dir_all(&provenance_dir)?;
            copy_source_urls_provenance(source_urls, &provenance_dir)?;
            let urls = read_source_urls_jsonl(source_urls)?;
            prefetch_archives_with_provenance(
                &urls,
                &work_dir,
                fetch_jobs,
                Some(&static_source_fetch_cache_config(config)?),
                &provenance_dir,
                "csup",
            )?;
            prepare_csup_inputs(&work_dir)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "provenance_dir".to_string(),
                    relative_artifact_path(&provenance_dir, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

fn build_csup_package_nodes(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, AssetSource)> {
    let source_urls_path = source_urls_dir.join("csup/source_urls.jsonl");
    let stage_inputs = csup_stage_inputs(&source_urls_path, config.fetch_jobs)?;
    let stage_prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-stage")?,
        "csup-stage",
        &stage_inputs,
    )?;
    let stage_record = load_existing_node_record(&stage_prepared.record_path, "csup-stage")?;
    let work_dir = resolve_artifact_path(config, output_path(&stage_record, "work_dir")?);
    let provenance_dir =
        resolve_artifact_path(config, output_path(&stage_record, "provenance_dir")?);
    let aggregate_path = provenance_dir.join("package_outputs.jsonl");
    let node_records = build_regional_package_nodes(
        config,
        &aggregate_path,
        "csup",
        |region| {
            let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
            let render_inputs = csup_render_inputs(
                &stage_record.fingerprint,
                region,
                config.cpu_jobs.max(1),
                version_label,
            )?;
            let render_prepared = prepare_node_at(
                &build_shared_node_dir(config, &render_node_name)?,
                &render_node_name,
                &render_inputs,
            )?;
            let render_record =
                load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
            let node_name = format!("csup-package-{}", region.code().to_ascii_lowercase());
            let inputs = BTreeMap::from([
                (
                    "render_fingerprint".to_string(),
                    render_record.fingerprint.clone(),
                ),
                ("region".to_string(), region.code().to_string()),
                ("version_label".to_string(), version_label.to_string()),
                (
                    "csup_package".to_string(),
                    hash_file(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .expect("preprocessor-cli should live under workspace root")
                            .join("preprocessor-csup/src/package.rs"),
                    )?,
                ),
                (
                    "tools_lib".to_string(),
                    hash_file(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .expect("preprocessor-cli should live under workspace root")
                            .join("preprocessor-tools/src/lib.rs"),
                    )?,
                ),
            ]);
            Ok(RegionalPackageSpec {
                region,
                node_name,
                inputs,
                zip_path: work_dir.join(format!("{}_CSUP_{}.zip", region.code(), version_label)),
                manifest_path: work_dir.join(format!(
                    "{}_CSUP_{}.manifest",
                    region.code(),
                    version_label
                )),
            })
        },
        |region, manifest_path, zip_path| {
            Ok(PackageOutputRecord {
                label: "csup".to_string(),
                chart: None,
                region: region.code().to_string(),
                manifest: format!("{}_CSUP_{}.manifest", region.code(), version_label),
                manifest_sha256: hash_file(manifest_path)?,
                zip: format!("{}_CSUP_{}.zip", region.code(), version_label),
                zip_sha256: hash_file(zip_path)?,
                metadata: BTreeMap::new(),
            })
        },
        |region| package_csup_region_versioned(&work_dir, region, version_label, version_label),
    )?;
    Ok((
        node_records,
        AssetSource {
            package_outputs_path: aggregate_path,
            asset_root: work_dir.clone(),
            package_root: work_dir.clone(),
            source_urls_path: Some(source_urls_path),
        },
    ))
}

fn build_tpp_render_node(
    config: &ProductBuildConfig,
    request: &NativeTppRunRequest,
) -> anyhow::Result<NodeRecord> {
    let region_id = request.region.code().to_ascii_lowercase();
    let source_urls = request
        .prefetch_source_urls
        .as_ref()
        .context("tpp build requires source urls")?;
    let node_name = format!("tpp-{region_id}-render");
    let inputs = tpp_render_inputs(request, source_urls, &region_id)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let run_root = prepared.dir.clone();
    let plates_root = run_root.join(format!("work/tpp-{region_id}/plates"));
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&plates_root),
        |_prepared| {
            let mut request = request.clone();
            request.run_root = run_root;
            let result = render_native_tpp(&request)?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&result.work_dir, &config.build_root),
                ),
                (
                    "provenance_dir".to_string(),
                    relative_artifact_path(&result.provenance_dir, &config.build_root),
                ),
                (
                    "plates_root".to_string(),
                    relative_artifact_path(&plates_root, &config.build_root),
                ),
            ]))
        },
    )
}

fn build_tpp_package_node(
    config: &ProductBuildConfig,
    region: Region,
    source_urls_path: &Path,
    version_label: &str,
) -> anyhow::Result<(NodeRecord, AssetSource)> {
    let region_id = region.code().to_ascii_lowercase();
    let render_request = NativeTppRunRequest {
        region,
        source_repo: PathBuf::new(),
        run_root: PathBuf::new(),
        prefetch_source_urls: Some(source_urls_path.to_path_buf()),
        fetch_jobs: config.fetch_jobs,
        render_jobs: TPP_RENDER_JOBS_PER_RUN,
        fetch_cache: Some(static_source_fetch_cache_config(config)?),
    };
    let render_node_name = format!("tpp-{region_id}-render");
    let render_inputs = tpp_render_inputs(&render_request, source_urls_path, &region_id)?;
    let render_prepared = prepare_node_at(
        &build_shared_node_dir(config, &render_node_name)?,
        &render_node_name,
        &render_inputs,
    )?;
    let render_record = load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
    let asset_root = resolve_artifact_path(config, output_path(&render_record, "work_dir")?);
    let inputs = BTreeMap::from([
        (
            "render_fingerprint".to_string(),
            render_record.fingerprint.clone(),
        ),
        ("region".to_string(), region.code().to_string()),
        ("version_label".to_string(), version_label.to_string()),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_package".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tpp/src/package.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]);
    let node_name = format!("tpp-{region_id}-package");
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let package_root = prepared.dir.join("output");
    let provenance_dir = prepared
        .dir
        .join("meta")
        .join("provenance")
        .join(format!("tpp-{region_id}"));
    let package_outputs_path = provenance_dir.join("package_outputs.jsonl");
    let zip_path = package_root.join(format!("{}_TPP_{}.zip", region.code(), version_label));
    let manifest_path =
        package_root.join(format!("{}_TPP_{}.manifest", region.code(), version_label));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            package_outputs_path.clone(),
            zip_path.clone(),
            manifest_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => {
            return Ok((
                record,
                AssetSource {
                    package_outputs_path,
                    asset_root: asset_root.clone(),
                    package_root: package_root.clone(),
                    source_urls_path: Some(source_urls_path.to_path_buf()),
                },
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = package_native_tpp_versioned(
        &asset_root,
        &package_root,
        &provenance_dir,
        region,
        version_label,
        version_label,
    )?;
    let outputs = BTreeMap::from([
        (
            "asset_root".to_string(),
            relative_artifact_path(&asset_root, &config.build_root),
        ),
        (
            "package_root".to_string(),
            relative_artifact_path(&package_root, &config.build_root),
        ),
        (
            "package_outputs".to_string(),
            relative_artifact_path(&package_outputs_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "package_count".to_string(),
            result.package_count.to_string(),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((
        record,
        AssetSource {
            package_outputs_path,
            asset_root,
            package_root,
            source_urls_path: Some(source_urls_path.to_path_buf()),
        },
    ))
}

fn chart_render_inputs(
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
    cpu_jobs: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("family".to_string(), family_slug(family).to_string()),
        ("source_repo".to_string(), hash_tree(source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("cpu_jobs".to_string(), cpu_jobs.to_string()),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
    ]))
}

fn csup_stage_inputs(
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
        (
            "csup_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/lib.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]))
}

fn csup_render_inputs(
    stage_fingerprint: &str,
    region: Region,
    render_jobs: usize,
    version_label: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        (
            "stage_fingerprint".to_string(),
            stage_fingerprint.to_string(),
        ),
        ("region".to_string(), region.code().to_string()),
        ("render_jobs".to_string(), render_jobs.to_string()),
        ("version_label".to_string(), version_label.to_string()),
        (
            "csup_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/lib.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]))
}

fn tpp_render_inputs(
    request: &NativeTppRunRequest,
    source_urls: &Path,
    region_id: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("region".to_string(), region_id.to_string()),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), request.fetch_jobs.to_string()),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tpp/src/lib.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]))
}

fn build_data_nodes(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
    node_name: &str,
) -> anyhow::Result<Vec<NodeRecord>> {
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    let data_version = data_version_label(source_urls_dir)?;
    let data_manifest_version = data_manifest_cycle(source_urls_dir)?;
    let (staged_input_dir, staging_record) = build_data_input_node(config, &source_urls)?;

    let artifact_stem = data_version.clone();
    let inputs = BTreeMap::from([
        (
            "staged_input_dir".to_string(),
            relative_artifact_path(&staged_input_dir, &config.build_root),
        ),
        (
            "staged_input_fingerprint".to_string(),
            staging_record.fingerprint.clone(),
        ),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        (
            "manifest_version".to_string(),
            data_manifest_version.clone(),
        ),
        ("artifact_stem".to_string(), artifact_stem.clone()),
        (
            "data_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, node_name)?,
        node_name,
        &inputs,
    )?;
    let provenance_dir = prepared.dir.join(format!("meta/provenance/{node_name}"));
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;

    let request = DataBuildRequest {
        input_dir: staged_input_dir.clone(),
        output_dir: prepared.dir.join("output"),
        mode: DataBuildMode::Production,
        manifest_version: data_manifest_version.clone(),
        artifact_stem: Some(artifact_stem),
    };
    let manifest_path = request.output_dir.join(format!(
        "{}.manifest",
        request.artifact_stem.as_deref().unwrap_or("databases")
    ));
    let zip_path = request.output_dir.join(format!(
        "{}.zip",
        request.artifact_stem.as_deref().unwrap_or("databases")
    ));
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[manifest_path.clone(), zip_path.clone()])? {
            NodeCacheState::CacheHit(record) => return Ok(vec![staging_record, record]),
            NodeCacheState::Build(lock) => lock,
        };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package(&request)?;
    let outputs = BTreeMap::from([
        (
            "intermediate_sqlite_db".to_string(),
            relative_artifact_path(&result.main_db, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let build_record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok(vec![staging_record, build_record])
}

fn build_data_match_node(
    config: &ProductBuildConfig,
    raw_intermediate_sqlite_db: &Path,
    raw_zip: &Path,
    artifact_stem: &str,
    raw_data_fingerprint: &str,
    tpp_sources: &[(Region, AssetSource, String)],
) -> anyhow::Result<NodeRecord> {
    let mut inputs = BTreeMap::from([
        (
            "raw_data_fingerprint".to_string(),
            raw_data_fingerprint.to_string(),
        ),
        ("artifact_stem".to_string(), artifact_stem.to_string()),
        (
            "matching_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/tpp_cifp_matching.rs"),
            )?,
        ),
        (
            "data_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/lib.rs"),
            )?,
        ),
        (
            "core_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-core/src/lib.rs"),
            )?,
        ),
    ]);
    let mut tpp_zips = Vec::new();
    for (region, source, fingerprint) in tpp_sources {
        let package = package_record_for_region(&source.package_outputs_path, *region)?;
        let zip_path = source.package_root.join(&package.zip);
        inputs.insert(
            format!("tpp_{}_fingerprint", region.code().to_ascii_lowercase()),
            fingerprint.clone(),
        );
        tpp_zips.push(zip_path);
    }
    let prepared = prepare_node_at(&build_shared_node_dir(config, "data")?, "data", &inputs)?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join(format!("{artifact_stem}.manifest"));
    let zip_path = output_dir.join(format!("{artifact_stem}.zip"));
    let intermediate_sqlite_db_path = output_dir.join("intermediate-sqlite.db");
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            intermediate_sqlite_db_path.clone(),
            manifest_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package_with_tpp_matches(&DataTppMatchRequest {
        input_main_db: raw_intermediate_sqlite_db.to_path_buf(),
        input_zip: raw_zip.to_path_buf(),
        output_dir: output_dir.clone(),
        artifact_stem: artifact_stem.to_string(),
        tpp_package_zips: tpp_zips,
    })?;
    let outputs = BTreeMap::from([
        (
            "intermediate_sqlite_db".to_string(),
            relative_artifact_path(&result.main_db, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

fn build_vectors_node(
    config: &ProductBuildConfig,
    intermediate_sqlite_db: &Path,
    source_input_dir: &Path,
    data_fingerprint: &str,
    version_label: &str,
) -> anyhow::Result<NodeRecord> {
    let inputs = BTreeMap::from([
        ("data_fingerprint".to_string(), data_fingerprint.to_string()),
        ("include_class_e_airspace".to_string(), "false".to_string()),
        (
            "source_input_dir".to_string(),
            relative_artifact_path(source_input_dir, &config.build_root),
        ),
        ("version_label".to_string(), version_label.to_string()),
        (
            "vectors_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-vectors/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "vectors")?,
        "vectors",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let request = BuildVectorsRequest {
        main_db: intermediate_sqlite_db.to_path_buf(),
        data_input_dir: Some(source_input_dir.to_path_buf()),
        output_dir: output_dir.clone(),
        version_label: version_label.to_string(),
        include_class_e_airspace: false,
    };
    let zip_path = output_dir.join(format!("vectors_{version_label}.zip"));
    let stats_path = output_dir.join("stats.json");
    let errors_path = output_dir.join("errors.json");
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[zip_path.clone(), stats_path.clone(), errors_path.clone()],
    )? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_vectors_dataset(&request)?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "stats".to_string(),
            relative_artifact_path(&result.stats_path, &config.build_root),
        ),
        (
            "errors".to_string(),
            relative_artifact_path(&result.errors_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

fn build_data_input_node(
    config: &ProductBuildConfig,
    source_urls: &Path,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    let urls = cycle_data_urls(read_source_urls_jsonl(source_urls)?);
    let inputs = BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
        ("cycle_urls".to_string(), hash_text(&urls.join("\n"))),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "data-input-staging")?,
        "data-input-staging",
        &inputs,
    )?;
    let staged_root = prepared.dir.join("out");
    let marker = staged_root.join(".staged-complete");
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&marker))? {
        NodeCacheState::CacheHit(record) => return Ok((staged_root, record)),
        NodeCacheState::Build(lock) => lock,
    };

    if staged_root.exists() {
        fs::remove_dir_all(&staged_root)
            .with_context(|| format!("failed to remove {}", staged_root.display()))?;
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&staged_root)
        .with_context(|| format!("failed to create {}", staged_root.display()))?;
    let provenance_dir = prepared.dir.join("meta/provenance/data-input-staging");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(source_urls, &provenance_dir)?;
    prefetch_archives_with_provenance(
        &urls,
        &staged_root,
        config.fetch_jobs,
        Some(&static_source_fetch_cache_config(config)?),
        &provenance_dir,
        "data",
    )?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "staged_input_dir".to_string(),
            relative_artifact_path(&staged_root, &config.build_root),
        ),
        (
            "provenance_dir".to_string(),
            relative_artifact_path(&provenance_dir, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((staged_root, record))
}

fn build_resource_index_node(
    config: &ProductBuildConfig,
    nav_db_zip: &Path,
    chart_sources: Vec<ChartSource>,
    tpp_sources: Vec<AssetSource>,
    csup_sources: Vec<AssetSource>,
) -> anyhow::Result<NodeRecord> {
    let node_root = build_node_root(config, "resource-index")?;
    let chart_json = chart_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.family_id,
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let tpp_json = tpp_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let csup_json = csup_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let inputs = BTreeMap::from([
        ("nav_db_zip".to_string(), hash_file(nav_db_zip)?),
        ("chart_sources".to_string(), hash_text(&chart_json)),
        ("tpp_sources".to_string(), hash_text(&tpp_json)),
        ("csup_sources".to_string(), hash_text(&csup_json)),
        (
            "resource_index_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-resource-index/src/lib.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(&node_root, "resource-index", &inputs)?;
    let output_path = prepared.dir.join("resource-index.json");
    let catalog_output_path = prepared.dir.join("catalog.json");
    let thumbnail_root = prepared.dir.join("thumbnails");
    if let Some(record) =
        try_load_node_record(&prepared, &[output_path.clone(), thumbnail_root.clone()])?
    {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let request = BuildResourceIndexRequest {
        nav_db_zip: nav_db_zip.to_path_buf(),
        output_path: output_path.clone(),
        catalog_output_path: Some(catalog_output_path.clone()),
        chart_sources,
        tpp_sources,
        csup_sources,
    };
    write_resource_index(&request)?;
    let outputs = BTreeMap::from([
        (
            "resource_index".to_string(),
            relative_artifact_path(&output_path, &config.build_root),
        ),
        (
            "catalog".to_string(),
            relative_artifact_path(&catalog_output_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

fn prepare_node_at(
    root: &Path,
    name: &str,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let fingerprint = fingerprint_for_node(name, inputs)?;
    let dir = root.join(&fingerprint);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(PreparedNode {
        name: name.to_string(),
        fingerprint,
        record_path: dir.join("build-record.json"),
        lock_path: dir.join(".build-lock"),
        dir,
    })
}

fn summarize_package_records(records: &[NodeRecord]) -> PackageSummary {
    let total = records.len();
    let cache_hits = records.iter().filter(|record| record.cache_hit).count();
    PackageSummary {
        total,
        cache_hits,
        rebuilt: total.saturating_sub(cache_hits),
    }
}

fn read_package_outputs_by_region(
    path: &Path,
) -> anyhow::Result<BTreeMap<String, PackageOutputRecord>> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut records = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse package output json")?;
        if value.get("event").and_then(|v| v.as_str()) != Some("package_output") {
            continue;
        }
        let record = PackageOutputRecord {
            label: value
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            chart: value
                .get("chart")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            region: value
                .get("region")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            manifest: value
                .get("manifest")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            manifest_sha256: value
                .get("manifest_sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            zip: value
                .get("zip")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            zip_sha256: value
                .get("zip_sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            metadata: value
                .get("metadata")
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        };
        records.insert(record.region.clone(), record);
    }
    Ok(records)
}

fn package_record_for_region(path: &Path, region: Region) -> anyhow::Result<PackageOutputRecord> {
    read_package_outputs_by_region(path)?
        .remove(region.code())
        .ok_or_else(|| anyhow::anyhow!("missing package output for region {}", region.code()))
}

fn try_load_node_record(
    prepared: &PreparedNode,
    expected_outputs: &[PathBuf],
) -> anyhow::Result<Option<NodeRecord>> {
    if !prepared.record_path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&prepared.record_path)
        .with_context(|| format!("failed to read {}", prepared.record_path.display()))?;
    let record: NodeRecord =
        serde_json::from_slice(&bytes).context("failed to parse node record")?;
    if record.fingerprint != prepared.fingerprint {
        return Ok(None);
    }
    if expected_outputs.iter().all(|path| path.exists()) {
        let mut cached = record;
        cached.cache_hit = true;
        return Ok(Some(cached));
    }
    Ok(None)
}

fn claim_or_wait_for_node(
    prepared: &PreparedNode,
    expected_outputs: &[PathBuf],
) -> anyhow::Result<NodeCacheState> {
    loop {
        if let Some(record) = try_load_node_record(prepared, expected_outputs)? {
            return Ok(NodeCacheState::CacheHit(record));
        }

        set_tree_readonly(&prepared.dir, false)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&prepared.lock_path)
        {
            Ok(mut file) => {
                let pid = std::process::id();
                let now = utc_now_string();
                use std::io::Write as _;
                writeln!(file, "pid={pid}").ok();
                writeln!(file, "started_at_utc={now}").ok();
                reset_node_dir_for_rebuild(prepared)?;
                return Ok(NodeCacheState::Build(BuildLockGuard {
                    path: prepared.lock_path.clone(),
                    node_dir: prepared.dir.clone(),
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock_if_needed(&prepared.lock_path)?;
                thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to acquire {}", prepared.lock_path.display())
                });
            }
        }
    }
}

fn run_cached_node<F>(
    prepared: PreparedNode,
    inputs: BTreeMap<String, String>,
    expected_outputs: &[PathBuf],
    build: F,
) -> anyhow::Result<NodeRecord>
where
    F: FnOnce(&PreparedNode) -> anyhow::Result<BTreeMap<String, String>>,
{
    let _build_lock = match claim_or_wait_for_node(&prepared, expected_outputs)? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let outputs = build(&prepared)?;
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

fn reset_node_dir_for_rebuild(prepared: &PreparedNode) -> anyhow::Result<()> {
    set_tree_readonly(&prepared.dir, false)?;
    for entry in fs::read_dir(&prepared.dir)
        .with_context(|| format!("failed to read {}", prepared.dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path == prepared.lock_path {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn set_tree_readonly(root: &Path, readonly: bool) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if readonly {
        for entry in
            fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                set_tree_readonly(&path, true)?;
            } else {
                set_path_readonly(&path, true)?;
            }
        }
        set_path_readonly(root, true)?;
    } else {
        set_path_readonly(root, false)?;
        for entry in
            fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                set_tree_readonly(&path, false)?;
            } else {
                set_path_readonly(&path, false)?;
            }
        }
    }
    Ok(())
}

fn set_path_readonly(path: &Path, readonly: bool) -> anyhow::Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let mut permissions = metadata.permissions();
    let mut mode = permissions.mode();
    if readonly {
        mode &= !0o222;
    } else if metadata.is_dir() {
        mode |= 0o700;
    } else {
        mode |= 0o600;
    }
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to chmod {}", path.display()))
}

fn remove_stale_lock_if_needed(lock_path: &Path) -> anyhow::Result<()> {
    if !lock_path.is_file() {
        return Ok(());
    }
    let Some(pid) = read_lock_pid(lock_path)? else {
        return Ok(());
    };
    if process_is_alive(pid) {
        return Ok(());
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to remove stale {}", lock_path.display()))
        }
    }
}

fn read_lock_pid(lock_path: &Path) -> anyhow::Result<Option<u32>> {
    let text = fs::read_to_string(lock_path)
        .with_context(|| format!("failed to read {}", lock_path.display()))?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("pid=") {
            return Ok(value.trim().parse::<u32>().ok());
        }
    }
    Ok(None)
}

fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn normalize_node_record_paths(mut record: NodeRecord, build_root: &Path) -> NodeRecord {
    record.outputs = record
        .outputs
        .into_iter()
        .map(|(key, value)| {
            let normalized = if value.starts_with('/') {
                relative_artifact_path(Path::new(&value), build_root)
            } else {
                value
            };
            (key, normalized)
        })
        .collect();
    record.output_details = record
        .output_details
        .into_iter()
        .map(|(key, mut detail)| {
            if detail.path.starts_with('/') {
                detail.path = relative_artifact_path(Path::new(&detail.path), build_root);
            }
            (key, detail)
        })
        .collect();
    record
}

fn write_node_record(
    prepared: PreparedNode,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    cache_hit: bool,
    started_at_utc: String,
    finished_at_utc: String,
    elapsed_ms: u64,
) -> anyhow::Result<NodeRecord> {
    let finalize_readonly = !legacy_mutable_output_node(&prepared.name);
    let output_details = node_output_details(&prepared.dir, &outputs)?;
    let record = NodeRecord {
        name: prepared.name,
        fingerprint: prepared.fingerprint,
        started_at_utc,
        finished_at_utc,
        elapsed_ms,
        cache_hit,
        inputs,
        outputs,
        output_details,
    };
    fs::write(
        &prepared.record_path,
        serde_json::to_vec_pretty(&record).context("failed to encode node record")?,
    )
    .with_context(|| format!("failed to write {}", prepared.record_path.display()))?;
    if !finalize_readonly {
        fs::write(
            prepared.dir.join(".mutable-output-root"),
            b"legacy mutable output root\n",
        )
        .with_context(|| format!("failed to mark {} as mutable", prepared.dir.display()))?;
    }
    Ok(record)
}

fn legacy_mutable_output_node(name: &str) -> bool {
    // Chart packaging still writes region zips/manifests into chart render work dirs, and
    // CSUP render/package still writes markers/thumbnails/manifests into the CSUP stage dir.
    // Those are legacy glue boundaries; keep them writable until those package outputs move
    // into their own node dirs.
    (name.starts_with("charts-") && name.ends_with("-render")) || name == "csup-stage"
}

fn node_output_details(
    node_dir: &Path,
    outputs: &BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, NodeOutputDetail>> {
    outputs
        .iter()
        .map(|(key, value)| {
            let resolved = resolve_recorded_output_path(node_dir, value);
            let (sha256, size_bytes) = match resolved.as_deref() {
                Some(path) if path.is_file() => {
                    let metadata = fs::metadata(path)
                        .with_context(|| format!("failed to stat {}", path.display()))?;
                    (Some(hash_file(path)?), Some(metadata.len()))
                }
                _ => (None, None),
            };
            Ok((
                key.clone(),
                NodeOutputDetail {
                    path: value.clone(),
                    sha256,
                    size_bytes,
                },
            ))
        })
        .collect()
}

fn resolve_recorded_output_path(node_dir: &Path, value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }
    for ancestor in node_dir.ancestors() {
        let candidate = ancestor.join(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn artifact_root_from_build_root(build_root: &Path) -> &Path {
    if build_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "published-packaged" || name == "published-packaged-validation")
    {
        return build_root.parent().unwrap_or(build_root);
    }
    if build_root
        .parent()
        .and_then(|value| value.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "published-packaged" || name == "published-packaged-validation")
    {
        return build_root
            .parent()
            .and_then(|value| value.parent())
            .unwrap_or(build_root);
    }
    build_root.parent().unwrap_or(build_root)
}

fn normalize_absolute_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn relative_artifact_path(path: &Path, build_root: &Path) -> String {
    let artifact_root = normalize_absolute_path(artifact_root_from_build_root(build_root));
    let normalized_path = normalize_absolute_path(path);
    normalized_path
        .strip_prefix(&artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn relative_product_build_path(path: &Path) -> String {
    let artifact_root = artifact_root_from_build_root(path);
    path.strip_prefix(artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn build_node_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("publish-nodes")
        .join(config.profile.as_str())
        .join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn build_shared_node_dir(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = artifact_root_from_build_root(&config.build_root)
        .join("cache")
        .join("nodes")
        .join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn load_existing_node_record(
    record_path: &Path,
    expected_name: &str,
) -> anyhow::Result<NodeRecord> {
    let bytes = fs::read(record_path)
        .with_context(|| format!("failed to read {}", record_path.display()))?;
    let record: NodeRecord =
        serde_json::from_slice(&bytes).context("failed to parse node record")?;
    if record.name != expected_name {
        bail!(
            "node record {} had unexpected name {}",
            record_path.display(),
            record.name
        );
    }
    Ok(record)
}

fn manifest_chart_name(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "SEC",
        ChartFamily::Tac => "TAC",
        ChartFamily::EnrL => "ENR_L",
        ChartFamily::EnrH => "ENR_H",
    }
}

fn hash_tree(root: &Path) -> anyhow::Result<String> {
    let mut entries = Vec::new();
    collect_files(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (relative, path) in entries {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher
            .update(fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(current)
        .with_context(|| format!("failed to read {}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", current.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to relativize {}", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, path));
        }
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(to).with_context(|| format!("failed to create {}", to.display()))?;
    let mut entries = fs::read_dir(from)
        .with_context(|| format!("failed to read {}", from.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", from.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let source = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", source.display()))?;
        if file_type.is_dir() {
            copy_dir_recursive(&source, &dest)?;
        } else if file_type.is_file() {
            fs::copy(&source, &dest).with_context(|| {
                format!("failed to copy {} to {}", source.display(), dest.display())
            })?;
        }
    }
    Ok(())
}

fn fingerprint_for_node(name: &str, inputs: &BTreeMap<String, String>) -> anyhow::Result<String> {
    let value = serde_json::json!({
        "schema_version": 1,
        "node": name,
        "inputs": inputs,
    });
    Ok(hash_text(
        &serde_json::to_string(&value).context("fingerprint json")?,
    ))
}

fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn utc_now_string() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(unix)]
fn current_nofile_limit() -> anyhow::Result<u64> {
    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
    if result != 0 {
        anyhow::bail!(
            "failed to read RLIMIT_NOFILE: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(limits.rlim_cur)
}

#[cfg(not(unix))]
fn current_nofile_limit() -> anyhow::Result<u64> {
    Ok(4096)
}

pub fn maybe_reexec_build_cycle_under_cgroup(args: &[String]) -> anyhow::Result<bool> {
    if env::var_os(PRODUCT_BUILD_CGROUP_ACTIVE_ENV).is_some() {
        return Ok(false);
    }
    if !command_exists("systemd-run") {
        return Ok(false);
    }
    let memory_max = env::var("PRODUCT_BUILD_MEMORY_MAX")
        .unwrap_or_else(|_| DEFAULT_PRODUCT_BUILD_MEMORY_MAX.to_string());
    let nofile_limit = env::var("PRODUCT_BUILD_NOFILE_LIMIT")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("invalid PRODUCT_BUILD_NOFILE_LIMIT={value}"))
        })
        .transpose()?
        .unwrap_or(current_nofile_limit()?);
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let status = Command::new("systemd-run")
        .args(["--quiet", "--wait", "--collect"])
        .args(["-p", &format!("MemoryMax={memory_max}")])
        .args(["-p", &format!("LimitNOFILE={nofile_limit}")])
        .args(["-p", "MemorySwapMax=0"])
        .args(["-p", "OOMPolicy=kill"])
        .arg("env")
        .arg(format!("{PRODUCT_BUILD_CGROUP_ACTIVE_ENV}=1"))
        .args(
            env::var("AEROBAG_ARTIFACT_WRITE_PATH")
                .ok()
                .into_iter()
                .map(|value| format!("AEROBAG_ARTIFACT_WRITE_PATH={value}")),
        )
        .arg(current_exe)
        .arg("build-cycle")
        .args(args)
        .status()
        .context("failed to re-exec product build under systemd-run")?;
    let exit_code = status.code().unwrap_or(1);
    if exit_code == 0 {
        return Ok(true);
    }
    bail!("cycle build cgroup wrapper exited with code {exit_code}");
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

struct MasterLog {
    start: Instant,
    file: File,
}

impl MasterLog {
    fn create(path: &Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self {
            start: Instant::now(),
            file,
        })
    }

    fn log(&mut self, message: impl AsRef<str>) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let line = format!(
            "{} {} {}",
            now,
            format_elapsed(self.start.elapsed().as_secs()),
            message.as_ref()
        );
        self.file
            .write_all(line.as_bytes())
            .and_then(|_| self.file.write_all(b"\n"))
            .context("failed to write master log")?;
        self.file.flush().context("failed to flush master log")?;
        Ok(())
    }
}

fn format_elapsed(elapsed_secs: u64) -> String {
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    if hours > 0 {
        format!("+{}:{minutes:02}:{seconds:02}", hours)
    } else {
        format!("+{minutes}:{seconds:02}")
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
}

pub(crate) fn default_artifact_write_path(repo_root: &Path) -> PathBuf {
    if let Some(path) = env_path("AEROBAG_ARTIFACT_WRITE_PATH") {
        return if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        };
    }
    {
        let config_path = repo_root.join(".aerobag-artifact-write-path");
        let raw = fs::read_to_string(&config_path).unwrap_or_else(|error| {
            panic!(
                "artifact write-path config missing at {} and AEROBAG_ARTIFACT_WRITE_PATH is unset: {error}",
                config_path.display()
            )
        });
        let configured = raw.trim();
        assert!(
            !configured.is_empty(),
            "artifact write-path config at {} is empty",
            config_path.display()
        );
        let path = PathBuf::from(configured);
        if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        }
    }
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn default_cpu_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(8)
}

fn family_slug(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "sec",
        ChartFamily::Tac => "tac",
        ChartFamily::EnrL => "enr-l",
        ChartFamily::EnrH => "enr-h",
    }
}

fn chart_family_version_label(
    source_urls_dir: &Path,
    family: ChartFamily,
) -> anyhow::Result<String> {
    let source_urls =
        source_urls_dir.join(format!("charts-{}/source_urls.jsonl", family_slug(family)));
    let effective = find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
        .with_context(|| format!("missing chart effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn csup_version_label(source_urls_dir: &Path) -> anyhow::Result<String> {
    let source_urls = source_urls_dir.join("csup/source_urls.jsonl");
    let effective = find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
        .with_context(|| format!("missing csup effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn tpp_region_version_label(source_urls_dir: &Path, region: Region) -> anyhow::Result<String> {
    let region_id = region.code().to_ascii_lowercase();
    let source_urls = source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
    // FAA d-TPP is a 28-day digital product. That differs from the printed TPP
    // books and from our chart/CSUP 56-day windows, so TPP artifacts are labeled
    // from the DDTPP effective date in the source URLs rather than from the
    // surrounding 56-day bundle window.
    let effective = find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
        .with_context(|| format!("missing tpp effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn data_version_label(source_urls_dir: &Path) -> anyhow::Result<String> {
    Ok(format!("data_{}", data_manifest_cycle(source_urls_dir)?))
}

fn data_manifest_cycle(source_urls_dir: &Path) -> anyhow::Result<String> {
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    let effective = find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
        .with_context(|| format!("missing data effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn cycle_data_urls(urls: Vec<String>) -> Vec<String> {
    urls.into_iter()
        .filter(|url| {
            !url.split('#')
                .next()
                .unwrap_or(url)
                .ends_with("/DAILY_DOF_DAT.ZIP")
        })
        .collect()
}

fn find_effective_date_from_urls(urls: &[String]) -> Option<NaiveDate> {
    urls.iter().find_map(|url| {
        let url = url.split('#').next().unwrap_or(url);
        extract_between(url, "/visual/", "/")
            .and_then(|value| parse_date(&value, "%m-%d-%Y").ok())
            .or_else(|| {
                extract_between(url, "/enroute/", "/")
                    .and_then(|value| parse_date(&value, "%m-%d-%Y").ok())
            })
            .or_else(|| {
                extract_suffix_between(url, "DCS_", ".zip")
                    .and_then(|value| parse_date(&value, "%Y%m%d").ok())
            })
            .or_else(|| {
                extract_between(url, "28DaySubscription_Effective_", ".zip")
                    .and_then(|value| parse_date(&value, "%Y-%m-%d").ok())
            })
            .or_else(|| {
                extract_between(url, "/28DaySub/", "/aixm5.0.zip")
                    .and_then(|value| parse_date(&value, "%Y-%m-%d").ok())
            })
            .or_else(|| {
                extract_suffix_between(url, "CIFP_", ".zip").and_then(|compact| {
                    if compact.len() == 6 && compact.chars().all(|ch| ch.is_ascii_digit()) {
                        parse_date(
                            &format!("20{}-{}-{}", &compact[0..2], &compact[2..4], &compact[4..6]),
                            "%Y-%m-%d",
                        )
                        .ok()
                    } else {
                        None
                    }
                })
            })
            .or_else(|| {
                url.split('/')
                    .next_back()
                    .and_then(|name| name.strip_suffix(".zip"))
                    .and_then(|name| name.rsplit('_').next())
                    .and_then(|compact| {
                        if compact.len() == 6
                            && compact.chars().all(|ch| ch.is_ascii_digit())
                            && url.contains("DDTPP")
                        {
                            parse_date(
                                &format!(
                                    "20{}-{}-{}",
                                    &compact[0..2],
                                    &compact[2..4],
                                    &compact[4..6]
                                ),
                                "%Y-%m-%d",
                            )
                            .ok()
                        } else {
                            None
                        }
                    })
            })
    })
}

fn extract_between(value: &str, prefix: &str, suffix: &str) -> Option<String> {
    let tail = value.split_once(prefix)?.1;
    Some(tail.split_once(suffix)?.0.to_string())
}

fn extract_suffix_between(value: &str, prefix: &str, suffix: &str) -> Option<String> {
    let tail = value.rsplit_once(prefix)?.1;
    Some(tail.split_once(suffix)?.0.to_string())
}

fn parse_date(value: &str, format: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(value, format)
        .with_context(|| format!("failed to parse FAA date {value} with {format}"))
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
    use preprocessor_resource_index::{
        AirportRecord, AirportResourcesRecord, ChartCollectionRecord, CoverageBounds, CsupRecord,
        DefaultView, NavDbRef, PlateRecord, ResourceFamily, ResourcePackage, ResourceRegion,
        TemporalSummary, TileLevelRecord,
    };
    use tempfile::tempdir;

    fn write_source_urls(root: &Path, relative: &str, lines: &[&str]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    fn minimal_resource_index() -> ResourceIndex {
        ResourceIndex {
            schema_version: 1,
            cycle: Some("2604".to_string()),
            generated_at_utc: "2026-04-20T00:00:00Z".to_string(),
            temporal_summary: TemporalSummary {
                cycle_codes: vec![],
                effective_dates: vec![],
                expiration_dates: vec![],
                uniform_cycle_code: None,
                uniform_effective_date: None,
                uniform_expiration_date: None,
                uniform_good_beyond_date: None,
            },
            nav_db: NavDbRef {
                artifact_path: None,
                sqlite_entry: "data.db".to_string(),
                cycle_code: None,
                version_label: None,
                effective_date: None,
                expiration_date: None,
            },
            families: vec![
                ResourceFamily {
                    id: "sec".to_string(),
                    display_name: "Sectional".to_string(),
                    kind: "tiled_raster".to_string(),
                },
                ResourceFamily {
                    id: "tac".to_string(),
                    display_name: "TAC".to_string(),
                    kind: "tiled_raster".to_string(),
                },
            ],
            regions: vec![ResourceRegion {
                id: "nw".to_string(),
                display_name: "Northwest".to_string(),
                sort_order: 0,
            }],
            packages: vec![ResourcePackage {
                id: "NW_SEC".to_string(),
                family_id: "sec".to_string(),
                region_id: "nw".to_string(),
                artifact_path: None,
                size_bytes: 0,
                checksum_sha256: String::new(),
                cycle_code: None,
                version_label: None,
                effective_date: None,
                expiration_date: None,
                metadata: BTreeMap::from([(
                    "full_coverage_zoom".to_string(),
                    serde_json::Value::from(7_u32),
                )]),
            }],
            chart_collections: vec![ChartCollectionRecord {
                id: "sec:nw".to_string(),
                family_id: "sec".to_string(),
                region_id: "nw".to_string(),
                package_id: "NW_SEC".to_string(),
                chart_index: 0,
                tile_path_template: "tiles/0/{z}/{x}/{y}.webp".to_string(),
                levels: vec![TileLevelRecord {
                    zoom: 10,
                    x_min: 1,
                    x_max: 2,
                    y_tms_min: 3,
                    y_tms_max: 4,
                }],
                coverage_bounds: CoverageBounds {
                    lat_min: 40.0,
                    lat_max: 50.0,
                    lon_min: -125.0,
                    lon_max: -103.0,
                },
                default_view: DefaultView {
                    lat: 45.0,
                    lon: -122.0,
                    zoom: 8.0,
                },
            }],
            airports: Vec::<AirportRecord>::new(),
            airport_resources: Vec::<AirportResourcesRecord>::new(),
            plates: Vec::<PlateRecord>::new(),
            csups: Vec::<CsupRecord>::new(),
        }
    }

    fn test_product_build_config(root: &Path) -> ProductBuildConfig {
        ProductBuildConfig {
            chart_cutline_root: root.join("cutlines"),
            build_root: root.join("published"),
            profile: ProductBuildProfile::Validation,
            target_cycle: None,
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: root.join("fetch-cache"),
            fetch_cache_mode: "cache-first".to_string(),
        }
    }

    #[test]
    fn fast_subset_reuses_previous_product_when_rebuild_fails() {
        let temp = tempdir().expect("tempdir");
        let config = test_product_build_config(temp.path());
        let previous = PublishedFastProductResult {
            id: "tfrs".to_string(),
            source_zip_path: temp.path().join("tfrs_old.zip"),
            published_zip: temp.path().join("published").join("tfrs_old.zip"),
            checksum_sha256: "oldsha".to_string(),
            size_bytes: 42,
            source_generated_at_utc: "2026-04-29T00:00:00Z".to_string(),
        };
        let previous_by_id = BTreeMap::from([("tfrs".to_string(), previous.clone())]);
        let mut gc_records = BTreeMap::new();

        let product = build_or_reuse_fast_product(
            &config,
            "tfrs",
            &previous_by_id,
            &mut gc_records,
            |_config| anyhow::bail!("HTTP 403"),
        )
        .expect("fallback should not fail");

        assert_eq!(product, Some(previous));
        assert!(gc_records.is_empty());
    }

    #[test]
    fn fast_subset_omits_failed_product_when_no_previous_exists() {
        let temp = tempdir().expect("tempdir");
        let config = test_product_build_config(temp.path());
        let mut gc_records = BTreeMap::new();

        let product = build_or_reuse_fast_product(
            &config,
            "tfrs",
            &BTreeMap::new(),
            &mut gc_records,
            |_config| anyhow::bail!("HTTP 403"),
        )
        .expect("missing fallback should not fail");

        assert_eq!(product, None);
        assert!(gc_records.is_empty());
    }

    #[test]
    fn nav_kv_chart_catalog_includes_shaded_relief_static_products() {
        let shaded_relief_levels = vec![(
            Region::Nw,
            vec![TileLevelRecord {
                zoom: 10,
                x_min: 156,
                x_max: 219,
                y_tms_min: 636,
                y_tms_max: 676,
            }],
        )];
        let catalog = build_nav_kv_chart_catalog(
            &minimal_resource_index(),
            &shaded_relief_levels,
            &BTreeMap::new(),
        );
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let shaded = entries
            .iter()
            .find(|entry| entry["id"] == "shaded-relief-nw")
            .expect("shaded relief entry");

        assert_eq!(shaded["label"], "Northwest Shaded Relief");
        assert_eq!(shaded["map_view"]["chart_family"], "shaded-relief");
        assert_eq!(
            shaded["map_view"]["tile_url_root"],
            "/shaded-relief-products/shaded-relief-nw/tiles"
        );
        assert_eq!(
            shaded["map_view"]["tile_path_template"],
            "0/{z}/{x}/{y}.webp"
        );
        assert_eq!(shaded["map_view"]["storage_kind"], "static_product");
        assert_eq!(
            shaded["map_view"]["max_zoom"],
            RASTER_BASEMAP_MAX_DISPLAY_ZOOM
        );
        assert_eq!(shaded["map_view"]["initial_viewport"]["lat"], 45.0);
        let levels = shaded["map_view"]["levels"]
            .as_array()
            .expect("levels should be an array");
        assert_eq!(levels.len(), 1);
        let z10 = levels
            .iter()
            .find(|level| level["zoom"] == 10)
            .expect("z10 level");
        assert_eq!(z10["x_min"], 156);
        assert_eq!(z10["x_max"], 219);
        assert_eq!(z10["y_tms_min"], 636);
        assert_eq!(z10["y_tms_max"], 676);
    }

    #[test]
    fn nav_kv_chart_catalog_emits_tile_path_templates_for_chart_packages() {
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &[], &BTreeMap::new());
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let sectional = entries
            .iter()
            .find(|entry| entry["id"] == "sec:nw")
            .expect("sectional entry");

        assert_eq!(
            sectional["map_view"]["tile_path_template"],
            "0/{z}/{x}/{y}.webp"
        );
    }

    #[test]
    fn nav_kv_chart_catalog_emits_polygon_set_coverage_for_chart_packages() {
        let cutline_root = tempdir().expect("tempdir");
        let sec_dir = cutline_root.path().join("SEC");
        fs::create_dir_all(&sec_dir).expect("create cutline dir");
        fs::write(
            sec_dir.join("Northwest SEC.geojson"),
            serde_json::json!({
                "type": "FeatureCollection",
                "features": [{
                    "type": "Feature",
                    "properties": {"location": "Northwest SEC.tif"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[
                            [-124.0, 41.0],
                            [-124.0, 49.0],
                            [-104.0, 49.0],
                            [-104.0, 41.0],
                            [-124.0, 41.0]
                        ]]
                    }
                }]
            })
            .to_string(),
        )
        .expect("write cutline");
        let polygon_sets =
            build_chart_coverage_polygon_sets(cutline_root.path(), &minimal_resource_index())
                .expect("polygon sets");
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &[], &polygon_sets);
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let sectional = entries
            .iter()
            .find(|entry| entry["id"] == "sec:nw")
            .expect("sectional entry");

        assert_eq!(sectional["coverage"]["kind"], "polygon_set_ref");
        assert_eq!(
            sectional["coverage"]["value"]["polygon_set_id"],
            "chart-coverage:sec:nw"
        );
        let pairs = build_nav_kv_chart_coverage_pairs(&polygon_sets).expect("coverage pairs");
        let pair = pairs
            .iter()
            .find(|pair| {
                pair.key
                    == format!(
                        "geometry/polygon-set/{}",
                        had_key_component("chart-coverage:sec:nw")
                    )
            })
            .expect("coverage pair");
        let polygon_set: ChartCoveragePolygonSetRecord =
            serde_json::from_slice(&pair.value).expect("decode polygon set");
        assert_eq!(polygon_set.id, "chart-coverage:sec:nw");
        assert_eq!(polygon_set.polygons.len(), 1);
        assert_eq!(polygon_set.polygons[0].points[0], [-124.0, 41.0]);
    }

    #[test]
    fn nav_kv_package_pairs_publish_bundle_package_rows() {
        let pairs = build_nav_kv_package_pairs(&[
            BundlePackageArtifact {
                id: "NW_SEC_2604_01".to_string(),
                family_id: "sec".to_string(),
                region_id: Some("nw".to_string()),
                filename: "sec_nw_2604_01_deadbeef.zip".to_string(),
                relative_path: "sec_nw_2604_01_deadbeef.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "deadbeef".to_string(),
                size_bytes: 123,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                metadata: BTreeMap::from([(
                    "full_coverage_zoom".to_string(),
                    serde_json::Value::from(7_u32),
                )]),
            },
            BundlePackageArtifact {
                id: "VECTORS_DATA_2604_01".to_string(),
                family_id: "vectors".to_string(),
                region_id: None,
                filename: "vectors_data_2604_01_cafebabe.zip".to_string(),
                relative_path: "vectors_data_2604_01_cafebabe.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "cafebabe".to_string(),
                size_bytes: 456,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                metadata: BTreeMap::new(),
            },
        ])
        .unwrap();

        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let index = pair_value("package/index");
        assert_eq!(index.as_array().unwrap().len(), 2);
        assert_eq!(index[0]["id"], "NW_SEC_2604_01");
        assert_eq!(index[0]["metadata"]["full_coverage_zoom"], 7);
        assert_eq!(index[1]["id"], "VECTORS_DATA_2604_01");

        let sectional = pair_value("package/by-id/NW_SEC_2604_01");
        assert_eq!(sectional["metadata"]["full_coverage_zoom"], 7);

        let vectors = pair_value("package/by-id/VECTORS_DATA_2604_01");
        assert_eq!(vectors["family_id"], "vectors");
        assert_eq!(vectors["region_id"], serde_json::Value::Null);
        assert_eq!(
            vectors["relative_path"],
            "vectors_data_2604_01_cafebabe.zip"
        );
        assert_eq!(vectors["size_bytes"], 456);
        assert_eq!(vectors["checksum_sha256"], "cafebabe");
        assert_eq!(vectors["cycle"], "2604");
        assert_eq!(vectors["cycle_version"], "01");
        let sec = pair_value("package/by-id/NW_SEC_2604_01");
        assert_eq!(sec["metadata"]["full_coverage_zoom"], 7);
    }

    #[test]
    fn nav_kv_package_inputs_include_resource_index_chart_metadata() {
        let mut resource_index = minimal_resource_index();
        resource_index.packages[0].id = "NW_SEC_2604_01".to_string();
        resource_index.packages[0].artifact_path = Some("products/sec_nw_2604.zip".to_string());
        resource_index.packages[0].size_bytes = 123;
        resource_index.packages[0].checksum_sha256 = "deadbeef".to_string();
        resource_index.packages[0].cycle_code = Some("2604".to_string());
        resource_index.packages[0].version_label = Some("01".to_string());
        resource_index.packages[0].metadata = BTreeMap::from([(
            "full_coverage_zoom".to_string(),
            serde_json::Value::from(7_u32),
        )]);

        let artifacts = bundle_package_artifacts_from_resource_index(&resource_index)
            .expect("resource index packages should convert");
        let pairs = build_nav_kv_package_pairs(&artifacts).expect("package pairs");
        let pair = pairs
            .iter()
            .find(|pair| pair.key == "package/by-id/NW_SEC_2604_01")
            .expect("sectional package by-id row");
        let value: serde_json::Value = serde_json::from_slice(&pair.value).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(value["metadata"]["full_coverage_zoom"], 7);
        assert_eq!(value["relative_path"], "sec_nw_2604_01_deadbeef.zip");
        assert_eq!(value["size_bytes"], 123);
    }

    #[test]
    fn nav_kv_resource_summary_pairs_publish_family_region_and_temporal_tables() {
        let pairs = build_nav_kv_resource_summary_pairs(&minimal_resource_index()).unwrap();

        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let families = pair_value("resource/families");
        assert!(families
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["id"] == "sec"));

        let regions = pair_value("resource/regions");
        assert_eq!(regions.as_array().unwrap()[0]["id"], "nw");

        let temporal = pair_value("resource/temporal-summary");
        assert_eq!(temporal["uniform_cycle_code"], serde_json::Value::Null);
    }

    #[test]
    fn nav_kv_airway_pairs_preserve_empty_branch_keys() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE airports (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    MagneticVariation TEXT
                );
                CREATE TABLE nav (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    Variation TEXT
                );
                CREATE TABLE arinc_navaids (
                    identifier TEXT,
                    icao_code TEXT,
                    section_code TEXT,
                    subsection_code TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    Variation TEXT
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    LEIdent TEXT,
                    LELatitude REAL,
                    LELongitude REAL,
                    HEIdent TEXT,
                    HELatitude REAL,
                    HELongitude REAL
                );
                CREATE TABLE airways_branch (
                    name TEXT,
                    branch_key TEXT,
                    sequence_number INTEGER,
                    sequence_token TEXT,
                    point_name TEXT,
                    Latitude REAL,
                    Longitude REAL
                );
                INSERT INTO fix VALUES ('RAWER', 45.235644444444446, -122.79431666666666);
                INSERT INTO fix VALUES ('CANBY', 45.31056944444444, -122.76489166666667);
                INSERT INTO fix VALUES ('HARPR', 42.480555555555554, -122.88376111111111);
                INSERT INTO nav VALUES ('ILA', 39.0711736111111, -122.027269722222, '14.0');
                INSERT INTO nav VALUES ('OAK', 37.7259255555556, -122.223591944444, '14.0');
                INSERT INTO airways_branch VALUES ('V23', '', 690, '690', 'RAWER', 45.235644444444446, -122.79431666666666);
                INSERT INTO airways_branch VALUES ('V23', '', 700, '700', 'CANBY', 45.31056944444444, -122.76489166666667);
                INSERT INTO airways_branch VALUES ('V23', '', 710, '710', 'NAMEDBUTMISSING', 45.4, -122.7);
                INSERT INTO airways_branch VALUES ('V23', '', 720, '720', '', 45.5, -122.6);
                INSERT INTO airways_branch VALUES ('Q801', 'A', 10, '10', 'HARPR', 42.480555555555554, -122.88376111111111);
                INSERT INTO airways_branch VALUES ('V195', 'RAGGS-JINGO', 220, '220', 'OAKLAND', 37.7259255555556, -122.223591944444);
                INSERT INTO airways_branch VALUES ('V195', 'RAGGS-JINGO', 300, '300', 'WILLIAMS', 39.0711736111111, -122.027269722222);
                ",
            )
            .unwrap();

        let pairs = build_nav_kv_airway_pairs(&connection).unwrap();
        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let v23 = pair_value("airway/V23");
        assert_eq!(v23[0]["branch_key"], "");
        assert_eq!(
            v23[0]["points"][0]["nav_ref"],
            serde_json::json!({ "Fix": "RAWER" })
        );
        assert_eq!(
            v23[0]["points"][1]["nav_ref"],
            serde_json::json!({ "Fix": "CANBY" })
        );
        assert_eq!(
            v23[0]["points"][2]["nav_ref"],
            serde_json::json!({ "LatLon": { "lat": 45.4, "lon": -122.7 } })
        );
        assert_eq!(
            v23[0]["points"][3]["nav_ref"],
            serde_json::json!({ "LatLon": { "lat": 45.5, "lon": -122.6 } })
        );

        let q801 = pair_value("airway/Q801");
        assert_eq!(q801[0]["branch_key"], "A");

        let v195 = pair_value("airway/V195");
        assert_eq!(
            v195[0]["points"][0]["nav_ref"],
            serde_json::json!({ "Navaid": "OAK" })
        );
        assert_eq!(
            v195[0]["points"][1]["nav_ref"],
            serde_json::json!({ "Navaid": "ILA" })
        );

        let rawer_tile = pair_value("airway/spatial/45/-123");
        assert!(rawer_tile
            .as_array()
            .unwrap()
            .iter()
            .any(|point| point["airway_name"] == "V23" && point["branch_key"] == ""));
    }

    #[test]
    fn nav_kv_airway_navref_validation_rejects_missing_position_keys() {
        let mut pairs = BTreeMap::new();
        pairs.insert(
            "airway/V195".to_string(),
            serde_json::to_vec(&serde_json::json!([{
                "branch_key": "",
                "points": [{
                    "nav_ref": { "Navaid": "ILA" }
                }]
            }]))
            .unwrap(),
        );

        let error = validate_airway_navrefs_resolve(&pairs).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing navref/position/navaid/ILA"));

        pairs.insert(
            "navref/position/navaid/ILA".to_string(),
            serde_json::to_vec(&serde_json::json!({ "lat": 39.0, "lon": -122.0 })).unwrap(),
        );
        validate_airway_navrefs_resolve(&pairs).unwrap();
    }

    #[test]
    fn derives_distinct_vintage_labels_from_source_urls() {
        let temp = tempdir().unwrap();
        write_source_urls(
            temp.path(),
            "charts-sec/source_urls.jsonl",
            &[
                r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/visual/03-19-2026/sectional-files/Seattle.zip"]}"#,
            ],
        );
        write_source_urls(
            temp.path(),
            "charts-enr-l/source_urls.jsonl",
            &[
                r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/enroute/03-19-2026/enr_l01.zip"]}"#,
            ],
        );
        write_source_urls(
            temp.path(),
            "csup/source_urls.jsonl",
            &[
                r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/Upload_313-d/supplements/DCS_20260319.zip"]}"#,
            ],
        );
        write_source_urls(
            temp.path(),
            "tpp-ne/source_urls.jsonl",
            &[
                r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/upload_313-d/terminal/DDTPPA_260416.zip"]}"#,
            ],
        );
        write_source_urls(
            temp.path(),
            "data/source_urls.jsonl",
            &[
                r#"{"event":"source_url","url":"https://nfdc.faa.gov/webContent/28DaySub/28DaySubscription_Effective_2026-04-16.zip"}"#,
            ],
        );

        assert_eq!(
            chart_family_version_label(temp.path(), ChartFamily::Sec).unwrap(),
            "2603"
        );
        assert_eq!(
            chart_family_version_label(temp.path(), ChartFamily::EnrL).unwrap(),
            "2603"
        );
        assert_eq!(csup_version_label(temp.path()).unwrap(), "2603");
        assert_eq!(
            tpp_region_version_label(temp.path(), Region::Ne).unwrap(),
            "2604"
        );
        assert_eq!(data_manifest_cycle(temp.path()).unwrap(), "2604");
        assert_eq!(data_version_label(temp.path()).unwrap(), "data_2604");
    }

    #[test]
    fn excludes_daily_obstacle_url_from_cycle_data_inputs() {
        let urls = vec![
            "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP".to_string(),
            "https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260416.zip".to_string(),
        ];
        let filtered = cycle_data_urls(urls);
        assert_eq!(
            filtered,
            vec!["https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260416.zip"]
        );
    }

    #[test]
    fn interprets_usgs_dem_cell_names_as_northwest_corners() {
        assert_eq!(terrain_cell_origin("n36w102"), Some((35.0, -102.0)));
        assert_eq!(terrain_cell_origin("s01e123"), Some((-2.0, 123.0)));

        assert!(terrain_cell_intersects_region("n37w102", Region::Sc));
        assert!(terrain_cell_intersects_region("n40w107", Region::Sw));
    }

    #[test]
    fn folder_categories_keep_other_out_of_approach_and_sort_hotspots_last() {
        assert_eq!(folder_category_for_document_type("approach"), "approach");
        assert_eq!(folder_category_for_document_type("other"), "other");
        assert_eq!(folder_category_for_document_type("hotspot"), "hotspot");
        assert!(folder_category_rank("takeoff-mins") < folder_category_rank("other"));
        assert!(folder_category_rank("other") < folder_category_rank("hotspot"));
    }

    #[test]
    fn cifp_declared_navaid_section_overrides_fix_like_procedure_identifier() {
        let context = NavLookupContext {
            airport_positions: BTreeMap::new(),
            navaid_positions: BTreeMap::from([(
                "RWF".to_string(),
                serde_json::json!({ "lat": 44.46727361111111, "lon": -95.12823 }),
            )]),
            arinc_navaid_positions: BTreeMap::new(),
            fix_positions: BTreeMap::new(),
            airport_positions_by_coord: BTreeMap::new(),
            navaid_positions_by_coord: BTreeMap::new(),
            fix_positions_by_coord: BTreeMap::new(),
            runway_positions: BTreeMap::new(),
            navaid_variation: BTreeMap::new(),
            arinc_navaid_variation: BTreeMap::new(),
            airport_variation: BTreeMap::new(),
        };

        assert_eq!(
            context.classify_cifp_reference_json("RWF", "", "D", ""),
            serde_json::json!({ "Navaid": "RWF" })
        );
        assert_eq!(
            context.classify_json("RWF"),
            serde_json::json!({ "Navaid": "RWF" })
        );
        assert_eq!(
            context.classify_json("RW17"),
            serde_json::json!({ "Fix": "RW17" })
        );
    }

    #[test]
    fn cifp_qualified_navaid_reference_uses_arinc_scope() {
        let key = ArincNavaidKey::new("JN", "K7", "D", "B");
        let context = NavLookupContext {
            airport_positions: BTreeMap::new(),
            navaid_positions: BTreeMap::from([(
                "JN".to_string(),
                serde_json::json!({ "lat": 40.1809228, "lon": -85.3209822 }),
            )]),
            arinc_navaid_positions: BTreeMap::from([(
                key.clone(),
                serde_json::json!({ "lat": 35.4749992, "lon": -78.4252856 }),
            )]),
            fix_positions: BTreeMap::new(),
            airport_positions_by_coord: BTreeMap::new(),
            navaid_positions_by_coord: BTreeMap::new(),
            fix_positions_by_coord: BTreeMap::new(),
            runway_positions: BTreeMap::new(),
            navaid_variation: BTreeMap::new(),
            arinc_navaid_variation: BTreeMap::from([(key, Some(-9.0))]),
            airport_variation: BTreeMap::new(),
        };

        let nav_ref = context.classify_cifp_reference_json("JN", "K7", "D", "B");
        assert_eq!(
            nav_ref,
            serde_json::json!({
                "ArincNavaid": {
                    "identifier": "JN",
                    "icao_code": "K7",
                    "section_code": "D",
                    "subsection_code": "B",
                }
            })
        );
        assert_eq!(
            context.resolve_position_json(&nav_ref, Some("KJNX")),
            serde_json::json!({ "lat": 35.4749992, "lon": -78.4252856 })
        );
        assert_eq!(
            context.variation_for_nav_ref(&nav_ref),
            serde_json::json!(-9.0)
        );
    }
}
