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
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use crossbeam_channel::{self, RecvTimeoutError};
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, package_family_region_versioned, stage_work_dir,
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
    load_tfr_notam_ids, metar_content_fingerprint, sanitize_notam_id, BuildGeoRequest,
    BuildMetarRequest, BuildNexradRequest, BuildTfrRequest,
};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    read_source_urls_jsonl, write_package_outputs_jsonl, CacheLayout, FetchCacheConfig,
    FetchCacheMode, PackageOutputRecord,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource, ResourceIndex,
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
    cycle: String,
    generated_at_utc: String,
    start_valid: String,
    end_valid: String,
    catalog: BundleArtifact,
    resource_index: BundleArtifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nav_kv: Option<BundleNavKvArtifact>,
    data: BundleArtifact,
    vectors: BundleArtifact,
    packages: Vec<BundlePackageArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentArtifactsManifest {
    schema_version: u32,
    as_of_date: String,
    bundles: Vec<CurrentBundleEntry>,
    obstacles: CurrentObstacleEntry,
    #[serde(default)]
    static_products: Vec<CurrentStaticProductEntry>,
    #[serde(default)]
    fast_products: Vec<CurrentFastProductEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentBundleEntry {
    filename: String,
    cycle: String,
    start_valid: String,
    end_valid: String,
    checksum_sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentObstacleEntry {
    filename: String,
    published_date: String,
    checksum_sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentStaticProductEntry {
    id: String,
    filename: String,
    published_at_utc: String,
    source_version: String,
    checksum_sha256: String,
    size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_fetched_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentFastProductEntry {
    id: String,
    filename: String,
    published_at_utc: String,
    source_generated_at_utc: String,
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
struct BundleNavKvArtifact {
    root: BundleArtifact,
    value_pages: Vec<BundleArtifact>,
    page_size: u32,
    value_bytes_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundlePackageArtifact {
    id: String,
    family_id: String,
    region_id: String,
    filename: String,
    relative_path: String,
    checksum_sha256: String,
    size_bytes: u64,
    effective_date: Option<String>,
    expiration_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductBuildResult {
    pub cycle_manifest_paths: Vec<PathBuf>,
    pub current_artifacts_path: PathBuf,
    pub obstacle_manifest_path: PathBuf,
    pub obstacle_stats_path: PathBuf,
    pub obstacle_zip_path: PathBuf,
    pub published_obstacle_zip: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct FastSubsetBuildResult {
    pub current_artifacts_path: PathBuf,
    pub fast_products: Vec<PublishedFastProductResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishedFastProductResult {
    pub id: String,
    pub source_zip_path: PathBuf,
    pub published_zip: PathBuf,
    pub checksum_sha256: String,
    pub size_bytes: u64,
    pub source_generated_at_utc: String,
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
        main_db: PathBuf,
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
        main_db: PathBuf,
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
    CycleManifest {
        path: PathBuf,
    },
    ObstaclesBuilt {
        manifest_path: PathBuf,
        stats_path: PathBuf,
        zip_path: PathBuf,
    },
    BuiltStandaloneProduct {
        zip_path: PathBuf,
        zip_sha256: Option<String>,
        zip_size_bytes: Option<u64>,
        source_version: String,
        source_fetched_at_utc: Option<String>,
    },
    TerrainDiscovery {
        index_path: PathBuf,
        source_fetched_at_utc: Option<String>,
    },
    PublishedObstacle {
        source_zip_path: PathBuf,
        published_zip: PathBuf,
        sha256: String,
        size_bytes: u64,
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
const TERRAIN_PIPELINE_VERSION: &str = "v3";
const SHADED_RELIEF_PIPELINE_VERSION: &str = "v3";
const SHADED_RELIEF_TILE_WORKERS: u32 = 4;
const TERRAIN_MIN_ZOOM: u32 = 0;
const TERRAIN_ZOOM: u32 = 10;
const TERRAIN_TILE_SIZE: u32 = 512;

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
        SourceUrls {
            cycle: String,
        },
        ChartRender {
            cycle: String,
            family: ChartFamily,
        },
        ChartPackage {
            cycle: String,
            family: ChartFamily,
        },
        ChartUnpack {
            cycle: String,
            family: ChartFamily,
            region: Region,
        },
        CsupStage {
            cycle: String,
        },
        CsupRender {
            cycle: String,
            region: Region,
        },
        CsupPackage {
            cycle: String,
        },
        CsupUnpack {
            cycle: String,
            region: Region,
        },
        TppRender {
            cycle: String,
            region: Region,
        },
        TppPackage {
            cycle: String,
            region: Region,
        },
        TppUnpack {
            cycle: String,
            region: Region,
        },
        DataBase {
            cycle: String,
        },
        DataMatch {
            cycle: String,
        },
        Vectors {
            cycle: String,
        },
        DataUnpack {
            cycle: String,
        },
        VectorsUnpack {
            cycle: String,
        },
        ResourceIndex {
            cycle: String,
        },
        BundleManifest {
            cycle: String,
        },
        ObstaclesBuild,
        ObstaclesPublish,
        TfrsBuild,
        TfrsPublish,
        MetarsBuild,
        MetarsPublish,
        NexradBuild,
        NexradPublish,
        GeoBuild,
        GeoPublish,
        TerrainDiscovery,
        TerrainBuild {
            region: Region,
        },
        TerrainPublish {
            region: Region,
        },
        ShadedReliefBuild {
            region: Region,
        },
        ShadedReliefPublish {
            region: Region,
        },
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
                for region in Region::ALL {
                    pending_tasks.push(ProductScheduledTask {
                        id: cycle_task_id(
                            cycle,
                            &format!(
                                "charts-{}-unpack-{}",
                                family_id,
                                region.code().to_ascii_lowercase()
                            ),
                        ),
                        deps: vec![package_id.clone()],
                        weight: 1,
                        kind: ProductScheduledTaskKind::ChartUnpack {
                            cycle: cycle.clone(),
                            family,
                            region,
                        },
                    });
                }
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
            for region in Region::ALL {
                pending_tasks.push(ProductScheduledTask {
                    id: cycle_task_id(
                        cycle,
                        &format!("csup-unpack-{}", region.code().to_ascii_lowercase()),
                    ),
                    deps: vec![cycle_task_id(cycle, "csup-package")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::CsupUnpack {
                        cycle: cycle.clone(),
                        region,
                    },
                });
            }

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
                pending_tasks.push(ProductScheduledTask {
                    id: cycle_task_id(cycle, &format!("tpp-{region_id}-unpack")),
                    deps: vec![package_id.clone()],
                    weight: 1,
                    kind: ProductScheduledTaskKind::TppUnpack {
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
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "data-unpack"),
                deps: vec![cycle_task_id(cycle, "data")],
                weight: 1,
                kind: ProductScheduledTaskKind::DataUnpack {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "vectors-unpack"),
                deps: vec![cycle_task_id(cycle, "vectors")],
                weight: 1,
                kind: ProductScheduledTaskKind::VectorsUnpack {
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
            pending_tasks.push(ProductScheduledTask {
                id: cycle_task_id(cycle, "bundle-manifest"),
                deps: vec![
                    cycle_task_id(cycle, "resource-index"),
                    cycle_task_id(cycle, "vectors"),
                ],
                weight: 1,
                kind: ProductScheduledTaskKind::BundleManifest {
                    cycle: cycle.clone(),
                },
            });
        }

        pending_tasks.push(ProductScheduledTask {
            id: "build-obstacles".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::ObstaclesBuild,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "publish-obstacles".to_string(),
            deps: vec!["build-obstacles".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::ObstaclesPublish,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "build-tfrs".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::TfrsBuild,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "publish-tfrs".to_string(),
            deps: vec!["build-tfrs".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::TfrsPublish,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "build-metars".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::MetarsBuild,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "publish-metars".to_string(),
            deps: vec!["build-metars".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::MetarsPublish,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "build-nexrad".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::NexradBuild,
        });
        pending_tasks.push(ProductScheduledTask {
            id: "publish-nexrad".to_string(),
            deps: vec!["build-nexrad".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::NexradPublish,
        });
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
                weight: 2,
                kind: ProductScheduledTaskKind::TerrainBuild { region: *region },
            });
            pending_tasks.push(ProductScheduledTask {
                id: format!("publish-terrain-{region_id}"),
                deps: vec![format!("build-terrain-{region_id}")],
                weight: 1,
                kind: ProductScheduledTaskKind::TerrainPublish { region: *region },
            });
            pending_tasks.push(ProductScheduledTask {
                id: format!("build-shaded-relief-{region_id}"),
                deps: vec!["terrain-discovery".to_string()],
                weight: 2,
                kind: ProductScheduledTaskKind::ShadedReliefBuild { region: *region },
            });
            pending_tasks.push(ProductScheduledTask {
                id: format!("publish-shaded-relief-{region_id}"),
                deps: vec![format!("build-shaded-relief-{region_id}")],
                weight: 1,
                kind: ProductScheduledTaskKind::ShadedReliefPublish { region: *region },
            });
        }
        pending_tasks.push(ProductScheduledTask {
            id: "current-artifacts".to_string(),
            deps: cycles
                .iter()
                .map(|cycle| cycle_task_id(cycle, "bundle-manifest"))
                .chain(std::iter::once("publish-obstacles".to_string()))
                .chain(std::iter::once("publish-tfrs".to_string()))
                .chain(std::iter::once("publish-metars".to_string()))
                .chain(std::iter::once("publish-nexrad".to_string()))
                .chain(std::iter::once("publish-geo".to_string()))
                .chain(config.profile.terrain_regions().iter().map(|region| {
                    format!("publish-terrain-{}", region.code().to_ascii_lowercase())
                }))
                .chain(config.profile.terrain_regions().iter().map(|region| {
                    format!(
                        "publish-shaded-relief-{}",
                        region.code().to_ascii_lowercase()
                    )
                }))
                .collect(),
            weight: 1,
            kind: ProductScheduledTaskKind::CurrentArtifacts,
        });
        let mut product_unpack_deps = vec![
            "current-artifacts".to_string(),
            "publish-obstacles".to_string(),
            "publish-tfrs".to_string(),
            "publish-metars".to_string(),
            "publish-nexrad".to_string(),
            "publish-geo".to_string(),
        ];
        product_unpack_deps.extend(
            config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| format!("publish-terrain-{}", region.code().to_ascii_lowercase())),
        );
        product_unpack_deps.extend(config.profile.terrain_regions().iter().map(|region| {
            format!(
                "publish-shaded-relief-{}",
                region.code().to_ascii_lowercase()
            )
        }));
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
                            let zip = resolve_artifact_path(&cycle_config, output_path(&data_record, "zip")?);
                            let main_db = resolve_artifact_path(&cycle_config, output_path(&data_record, "main_db")?);
                            Ok(ProductTaskCompletion {
                                node_records: records
                                    .into_iter()
                                    .map(|record| normalize_node_record_paths(record, &cycle_config.build_root))
                                    .collect(),
                                value: ProductTaskValue::FingerprintedData {
                                    main_db,
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
                                Some(ProductTaskValue::FingerprintedData { main_db, zip, fingerprint }) => {
                                    (main_db.clone(), zip.clone(), fingerprint.clone())
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
                                &raw_data.1,
                                &source_urls,
                                &raw_data.2,
                                &tpp_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&cycle_config, output_path(&record, "zip")?);
                            let main_db = resolve_artifact_path(&cycle_config, output_path(&record, "main_db")?);
                            let fingerprint = record.fingerprint.clone();
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::FingerprintedData {
                                    main_db,
                                    zip,
                                    fingerprint,
                                },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ProductScheduledTaskKind::Vectors { cycle } => {
                            let (data, data_fingerprint) = match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData { main_db, fingerprint, .. }) => {
                                    (main_db.clone(), fingerprint.clone())
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
                            let record = build_vectors_node(&cycle_config, &data, &data_fingerprint, &data_version)?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&cycle_config, output_path(&record, "zip")?);
                            Ok(ProductTaskCompletion {
                                node_records: vec![normalize_node_record_paths(record, &cycle_config.build_root)],
                                value: ProductTaskValue::FingerprintedZip { zip },
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
                        ProductScheduledTaskKind::ChartUnpack {
                            cycle,
                            family,
                            region,
                        } => {
                            let key = cycle_task_id(&cycle, &format!("charts-{}-package", family_slug(family)));
                            let source = match task_values_snapshot.get(&key) {
                                Some(ProductTaskValue::ChartSource(source)) => source.clone(),
                                _ => bail!("missing chart source for cycle {cycle}"),
                            };
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let unpacked_root = published_unpacked_root(&cycle_config)?;
                            let published_filename = canonical_package_filename(
                                family_slug(family),
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
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ProductScheduledTaskKind::CsupUnpack { cycle, region } => {
                            let source = match task_values_snapshot.get(&cycle_task_id(&cycle, "csup-package")) {
                                Some(ProductTaskValue::CsupSource(source)) => source.clone(),
                                _ => bail!("missing csup source for cycle {cycle}"),
                            };
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let unpacked_root = published_unpacked_root(&cycle_config)?;
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
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ProductScheduledTaskKind::TppUnpack { cycle, region } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let source = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, &format!("tpp-{region_id}-package")))
                            {
                                Some(ProductTaskValue::FingerprintedTppSource { source, .. }) => source.clone(),
                                _ => bail!("missing tpp source for cycle {cycle}"),
                            };
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let unpacked_root = published_unpacked_root(&cycle_config)?;
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
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ProductScheduledTaskKind::DataUnpack { cycle } => {
                            let zip = match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData { zip, .. }) => zip.clone(),
                                _ => bail!("missing data zip for cycle {cycle}"),
                            };
                            let bundle_cycle = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { bundle_cycle, .. }) => bundle_cycle.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let unpacked_root = published_unpacked_root(&cycle_config)?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip,
                                zip.parent().unwrap_or_else(|| Path::new("/")),
                                &unpacked_root,
                                &format!("data_{bundle_cycle}.zip"),
                                None,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
                            })
                        }
                        ProductScheduledTaskKind::VectorsUnpack { cycle } => {
                            let zip = match task_values_snapshot.get(&cycle_task_id(&cycle, "vectors")) {
                                Some(ProductTaskValue::FingerprintedZip { zip, .. }) => zip.clone(),
                                _ => bail!("missing vectors zip for cycle {cycle}"),
                            };
                            let bundle_cycle = match task_values_snapshot
                                .get(&cycle_task_id(&cycle, "source-urls"))
                            {
                                Some(ProductTaskValue::SourceUrls { bundle_cycle, .. }) => bundle_cycle.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                            let mut cycle_config = config.clone();
                            cycle_config.target_cycle = Some(cycle);
                            let unpacked_root = published_unpacked_root(&cycle_config)?;
                            let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                                &zip,
                                zip.parent().unwrap_or_else(|| Path::new("/")),
                                &unpacked_root,
                                &format!("vectors_data_{bundle_cycle}.zip"),
                                None,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::None,
                                completion_detail: format!(
                                    "cache_hit={} unpack_dir={}",
                                    cache_hit,
                                    unpack_dir.display()
                                ),
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
                            let bundle_manifest = build_bundle_manifest(&cycle_config, &build_manifest)?;
                            let bundle_manifest_path =
                                cycle_config.build_root.join(format!("bundle_{source_urls}.json"));
                            fs::write(
                                &bundle_manifest_path,
                                serde_json::to_vec_pretty(&bundle_manifest)
                                    .context("failed to encode bundle manifest")?,
                            )
                            .with_context(|| {
                                format!("failed to write {}", bundle_manifest_path.display())
                            })?;
                            validate_bundle_manifest(&cycle_config.build_root, &bundle_manifest_path)?;
                            sync_unpacked_metadata(
                                &cycle_config,
                                &bundle_manifest,
                                &build_manifest,
                                &bundle_manifest_path,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::CycleManifest {
                                    path: bundle_manifest_path,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ObstaclesBuild => {
                            let (manifest_path, stats_path, zip_path) = build_obstacles_product(&config)?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::ObstaclesBuilt {
                                    manifest_path,
                                    stats_path,
                                    zip_path,
                                },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ObstaclesPublish => {
                            let built = match task_values_snapshot.get("build-obstacles") {
                                Some(ProductTaskValue::ObstaclesBuilt { zip_path, .. }) => zip_path.clone(),
                                _ => bail!("missing obstacle build output"),
                            };
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_obstacle_zip(&config.build_root, &built)?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedObstacle {
                                    source_zip_path: built,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::TfrsBuild => {
                            let (zip_path, source_generated_at_utc, record) =
                                build_tfrs_product(&config)?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version: source_generated_at_utc,
                                    source_fetched_at_utc: None,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::MetarsBuild => {
                            let (zip_path, source_generated_at_utc, record) =
                                build_metars_product(&config)?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version: source_generated_at_utc,
                                    source_fetched_at_utc: None,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
                            })
                        }
                        ProductScheduledTaskKind::NexradBuild => {
                            let (zip_path, source_generated_at_utc, record) =
                                build_nexrad_product(&config)?;
                            let cache_hit = record.cache_hit;
                            let (zip_sha256, zip_size_bytes) =
                                node_output_file_detail(&record, "zip");
                            Ok(ProductTaskCompletion {
                                node_records: vec![record],
                                value: ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    zip_size_bytes,
                                    source_version: source_generated_at_utc,
                                    source_fetched_at_utc: None,
                                },
                                completion_detail: format!("cache_hit={cache_hit}"),
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
                        ProductScheduledTaskKind::ShadedReliefBuild { region } => {
                            let (index_path, source_fetched_at_utc) =
                                match task_values_snapshot.get("terrain-discovery") {
                                    Some(ProductTaskValue::TerrainDiscovery {
                                        index_path,
                                        source_fetched_at_utc,
                                    }) => (index_path.clone(), source_fetched_at_utc.clone()),
                                    _ => bail!("missing terrain discovery output"),
                                };
                            let (zip_path, source_version, source_fetched_at_utc, record) =
                                build_shaded_relief_product(
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
                        ProductScheduledTaskKind::TfrsPublish => {
                            let built = match task_values_snapshot.get("build-tfrs") {
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
                                _ => bail!("missing TFR build output"),
                            };
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_fast_product_zip(
                                    &config.build_root,
                                    "tfrs",
                                    &built.0,
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: "tfrs".to_string(),
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
                        ProductScheduledTaskKind::MetarsPublish => {
                            let built = match task_values_snapshot.get("build-metars") {
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
                                _ => bail!("missing METAR build output"),
                            };
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_fast_product_zip(
                                    &config.build_root,
                                    "metars",
                                    &built.0,
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: "metars".to_string(),
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
                        ProductScheduledTaskKind::NexradPublish => {
                            let built = match task_values_snapshot.get("build-nexrad") {
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
                                _ => bail!("missing NEXRAD build output"),
                            };
                            let (published_zip, sha256, size_bytes) =
                                publish_content_addressed_fast_product_zip(
                                    &config.build_root,
                                    "nexrad",
                                    &built.0,
                                    built.1.as_deref(),
                                    built.2,
                                )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::PublishedStandaloneProduct {
                                    id: "nexrad".to_string(),
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
                            let published_obstacle = match task_values_snapshot.get("publish-obstacles") {
                                Some(ProductTaskValue::PublishedObstacle {
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    ..
                                }) => (published_zip.clone(), sha256.clone(), *size_bytes),
                                _ => bail!("missing published obstacle output"),
                            };
                            let fast_products = ["publish-tfrs", "publish-metars", "publish-nexrad"]
                                .iter()
                                .map(|task_id| match task_values_snapshot.get(*task_id) {
                                    Some(ProductTaskValue::PublishedStandaloneProduct {
                                        id,
                                        published_zip,
                                        sha256,
                                        size_bytes,
                                        source_version,
                                        ..
                                    }) => Ok(CurrentFastProductEntry {
                                        id: id.clone(),
                                        filename: published_zip
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or_default()
                                            .to_string(),
                                        published_at_utc: utc_now_string(),
                                        source_generated_at_utc: source_version.clone(),
                                        checksum_sha256: sha256.clone(),
                                        size_bytes: *size_bytes,
                                    }),
                                    _ => bail!("missing published fast product output for {}", task_id),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let static_product_task_ids = std::iter::once("publish-geo".to_string())
                                .chain(config.profile.terrain_regions().iter().map(|region| {
                                    format!(
                                        "publish-terrain-{}",
                                        region.code().to_ascii_lowercase()
                                    )
                                }))
                                .chain(config.profile.terrain_regions().iter().map(|region| {
                                    format!(
                                        "publish-shaded-relief-{}",
                                        region.code().to_ascii_lowercase()
                                    )
                                }))
                                .collect::<Vec<_>>();
                            let static_products = static_product_task_ids
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
                                    }) => Ok(CurrentStaticProductEntry {
                                        id: id.clone(),
                                        filename: published_zip
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or_default()
                                            .to_string(),
                                        published_at_utc: utc_now_string(),
                                        source_version: source_version.clone(),
                                        checksum_sha256: sha256.clone(),
                                        size_bytes: *size_bytes,
                                        source_fetched_at_utc: source_fetched_at_utc.clone(),
                                    }),
                                    _ => bail!("missing published static product output for {}", task_id),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let current_artifacts_path = write_current_artifacts_manifest(
                                &config.build_root,
                                Utc::now().date_naive(),
                                &published_obstacle.0,
                                &published_obstacle.1,
                                published_obstacle.2,
                                static_products,
                                fast_products,
                            )?;
                            Ok(ProductTaskCompletion {
                                node_records: vec![],
                                value: ProductTaskValue::CurrentArtifacts {
                                    path: current_artifacts_path,
                                },
                                completion_detail: "published".to_string(),
                            })
                        }
                        ProductScheduledTaskKind::ProductUnpack => {
                            let current_artifacts_path = match task_values_snapshot.get("current-artifacts") {
                                Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
                                _ => bail!("missing current artifacts output"),
                            };
                            let obstacle = match task_values_snapshot.get("publish-obstacles") {
                                Some(ProductTaskValue::PublishedObstacle {
                                    source_zip_path,
                                    published_zip,
                                    sha256,
                                    ..
                                }) => PublishedZipArtifact {
                                    source_zip_path: source_zip_path.clone(),
                                    published_zip_path: published_zip.clone(),
                                    checksum_sha256: sha256.clone(),
                                },
                                _ => bail!("missing published obstacle output"),
                            };
                            let fast_products = ["publish-tfrs", "publish-metars", "publish-nexrad"]
                                .iter()
                                .map(|task_id| match task_values_snapshot.get(*task_id) {
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
                                    _ => bail!("missing published fast product output for {}", task_id),
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let static_product_task_ids = std::iter::once("publish-geo".to_string())
                                .chain(config.profile.terrain_regions().iter().map(|region| {
                                    format!(
                                        "publish-terrain-{}",
                                        region.code().to_ascii_lowercase()
                                    )
                                }))
                                .chain(config.profile.terrain_regions().iter().map(|region| {
                                    format!(
                                        "publish-shaded-relief-{}",
                                        region.code().to_ascii_lowercase()
                                    )
                                }))
                                .collect::<Vec<_>>();
                            let static_products = static_product_task_ids
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
                            let mut zip_artifacts = vec![obstacle];
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
        let (obstacle_manifest_path, obstacle_stats_path, obstacle_zip_path) =
            match task_values.get("build-obstacles") {
                Some(ProductTaskValue::ObstaclesBuilt {
                    manifest_path,
                    stats_path,
                    zip_path,
                }) => (manifest_path.clone(), stats_path.clone(), zip_path.clone()),
                _ => bail!("missing obstacle build outputs"),
            };
        let published_obstacle_zip = match task_values.get("publish-obstacles") {
            Some(ProductTaskValue::PublishedObstacle { published_zip, .. }) => {
                published_zip.clone()
            }
            _ => bail!("missing published obstacle output"),
        };
        let current_artifacts_path = match task_values.get("current-artifacts") {
            Some(ProductTaskValue::CurrentArtifacts { path }) => path.clone(),
            _ => bail!("missing current artifacts output"),
        };
        record_gc_roots(config, "full", &task_node_records)?;

        Ok(ProductBuildResult {
            cycle_manifest_paths,
            current_artifacts_path,
            obstacle_manifest_path,
            obstacle_stats_path,
            obstacle_zip_path,
            published_obstacle_zip,
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
    let previous_fast_products = current.fast_products.clone();

    let built_tfrs = build_tfrs_product(config)?;
    let built_metars = build_metars_product(config)?;
    let built_nexrad = build_nexrad_product(config)?;
    let mut gc_records = BTreeMap::new();
    gc_records.insert("fast:tfrs".to_string(), vec![built_tfrs.2.clone()]);
    gc_records.insert("fast:metars".to_string(), vec![built_metars.2.clone()]);
    gc_records.insert("fast:nexrad".to_string(), vec![built_nexrad.2.clone()]);
    let tfrs = publish_built_fast_product(config, "tfrs", built_tfrs)?;
    let metars = publish_built_fast_product(config, "metars", built_metars)?;
    let nexrad = publish_built_fast_product(config, "nexrad", built_nexrad)?;
    let fast_products = vec![tfrs, metars, nexrad];
    let published_at_utc = utc_now_string();
    current.fast_products = fast_products
        .iter()
        .map(|product| CurrentFastProductEntry {
            id: product.id.clone(),
            filename: product
                .published_zip
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            published_at_utc: published_at_utc.clone(),
            source_generated_at_utc: product.source_generated_at_utc.clone(),
            checksum_sha256: product.checksum_sha256.clone(),
            size_bytes: product.size_bytes,
        })
        .collect();
    current.as_of_date = Utc::now().date_naive().format("%Y-%m-%d").to_string();

    let output_path = config.build_root.join(format!(
        "current_artifacts_{}.json",
        Utc::now().date_naive().format("%Y%m%d")
    ));
    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&current)
            .context("failed to encode current artifacts manifest")?,
    )
    .with_context(|| format!("failed to write {}", output_path.display()))?;

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

fn current_artifacts_path_for_fast_subset(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    let today_path = config.build_root.join(format!(
        "current_artifacts_{}.json",
        Utc::now().date_naive().format("%Y%m%d")
    ));
    if today_path.is_file() {
        return Ok(today_path);
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
                    name.starts_with("current_artifacts_") && name.ends_with(".json")
                })
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop().with_context(|| {
        format!(
            "no current_artifacts_YYYYMMDD.json exists in {}; run build-product first",
            config.build_root.display()
        )
    })
}

fn sync_fast_subset_unpacked(
    build_root: &Path,
    current_artifacts_path: &Path,
    previous_fast_products: &[CurrentFastProductEntry],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_build_root(build_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(current_artifacts_path, &unpacked_root)?;
    remove_stale_fast_unpacked_dirs(&unpacked_root, previous_fast_products, fast_products)?;
    for product in fast_products {
        let published_filename = product
            .published_zip
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("failed to determine published fast filename"))?;
        sync_unpacked_zip_from_source(
            &product.published_zip,
            product
                .source_zip_path
                .parent()
                .unwrap_or_else(|| Path::new("/")),
            &unpacked_root,
            published_filename,
            Some(&product.checksum_sha256),
        )?;
    }
    Ok(())
}

fn remove_stale_fast_unpacked_dirs(
    unpacked_root: &Path,
    previous_fast_products: &[CurrentFastProductEntry],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let current_dirs = fast_products
        .iter()
        .map(|product| {
            let filename = product
                .published_zip
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("failed to determine published fast filename"))?;
            zip_stem(filename)
        })
        .collect::<anyhow::Result<std::collections::BTreeSet<_>>>()?;
    for product in previous_fast_products {
        let previous_dir = zip_stem(&product.filename)?;
        if current_dirs.contains(&previous_dir) {
            continue;
        }
        let path = unpacked_root.join(&previous_dir);
        if path.exists() {
            fs::remove_dir_all(&path).with_context(|| {
                format!("failed to remove stale fast product {}", path.display())
            })?;
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
                            let zip =
                                resolve_artifact_path(&config, output_path(&data_record, "zip")?);
                            let main_db = resolve_artifact_path(
                                &config,
                                output_path(&data_record, "main_db")?,
                            );
                            Ok(TaskCompletion {
                                node_records: records,
                                value: TaskValue::FingerprintedData {
                                    main_db,
                                    zip,
                                    fingerprint: data_record.fingerprint,
                                },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }),
                        ScheduledTaskKind::DataMatch => {
                            let raw_data = match task_values_snapshot.get("data-base") {
                                Some(TaskValue::FingerprintedData {
                                    main_db,
                                    zip,
                                    fingerprint,
                                }) => (main_db.clone(), zip.clone(), fingerprint.clone()),
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
                                &raw_data.1,
                                &data_version,
                                &raw_data.2,
                                &tpp_sources,
                            )?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&config, output_path(&record, "zip")?);
                            let main_db =
                                resolve_artifact_path(&config, output_path(&record, "main_db")?);
                            let fingerprint = record.fingerprint.clone();
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::FingerprintedData {
                                    main_db,
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
                            let (data, data_fingerprint) = match task_values_snapshot.get("data") {
                                Some(TaskValue::FingerprintedData {
                                    main_db,
                                    fingerprint,
                                    ..
                                }) => (main_db, fingerprint),
                                _ => unreachable!("data dependency should have completed"),
                            };
                            let record =
                                build_vectors_node(&config, data, data_fingerprint, &data_version)?;
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
                                    main_db: _, zip, ..
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
                                    main_db: _, zip, ..
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

        let bundle_manifest = build_bundle_manifest(config, &build_manifest)?;
        let bundle_manifest_path = config
            .build_root
            .join(format!("bundle_{bundle_cycle}.json"));
        fs::write(
            &bundle_manifest_path,
            serde_json::to_vec_pretty(&bundle_manifest)
                .context("failed to encode bundle manifest")?,
        )
        .with_context(|| format!("failed to write {}", bundle_manifest_path.display()))?;
        validate_bundle_manifest(&config.build_root, &bundle_manifest_path)?;
        sync_unpacked_metadata(
            config,
            &bundle_manifest,
            &build_manifest,
            &bundle_manifest_path,
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

fn build_bundle_manifest(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
) -> anyhow::Result<BundleManifest> {
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
    let catalog_path =
        resolve_artifact_path(config, output_path(resource_index_record, "catalog")?);
    let data_zip_path = resolve_artifact_path(config, output_path(data_record, "zip")?);
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
    let data_filename = format!("data_{cycle}.zip");
    let vectors_filename = format!("vectors_data_{cycle}.zip");

    let package_artifacts = index
        .packages
        .iter()
        .map(|package| {
            let package_path = resolve_bundle_package_source_path(config, build_manifest, package)?;
            let filename = canonical_package_filename(
                &package.family_id,
                &package.region_id,
                Path::new(&package_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
            )?;
            publish_flat_artifact(&package_path, &config.build_root.join(&filename))?;
            Ok(BundlePackageArtifact {
                id: package.id.clone(),
                family_id: package.family_id.clone(),
                region_id: package.region_id.clone(),
                filename: filename.clone(),
                relative_path: filename,
                checksum_sha256: package.checksum_sha256.clone(),
                size_bytes: fs::metadata(&package_path)
                    .with_context(|| format!("failed to stat {}", package_path.display()))?
                    .len(),
                effective_date: package.effective_date.clone(),
                expiration_date: package.expiration_date.clone(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let public_resource_index =
        rewrite_public_resource_index(&index, &data_filename, &package_artifacts);
    let published_resource_index_path = write_published_json(
        &config
            .build_root
            .join(format!("resource_index_{cycle}.json")),
        &public_resource_index,
    )?;
    let data_db_path = resolve_artifact_path(config, output_path(data_record, "main_db")?);
    let nav_kv = write_nav_kv_artifact(
        config,
        &public_resource_index,
        &cycle,
        &data_db_path,
        config.profile.terrain_regions(),
    )?;

    Ok(BundleManifest {
        schema_version: 1,
        cycle: cycle.clone(),
        generated_at_utc: build_manifest.generated_at_utc.clone(),
        start_valid,
        end_valid,
        catalog: publish_bundle_artifact(config, &catalog_path, &format!("catalog_{cycle}.json"))?,
        resource_index: bundle_artifact(
            &published_resource_index_path,
            &format!("resource_index_{cycle}.json"),
        )?,
        nav_kv: Some(nav_kv),
        data: publish_bundle_artifact(config, &data_zip_path, &data_filename)?,
        vectors: publish_bundle_artifact(config, &vectors_zip_path, &vectors_filename)?,
        packages: package_artifacts,
    })
}

fn write_nav_kv_artifact(
    config: &ProductBuildConfig,
    resource_index: &ResourceIndex,
    cycle: &str,
    main_db_path: &Path,
    shaded_relief_regions: &[Region],
) -> anyhow::Result<BundleNavKvArtifact> {
    let chart_catalog = build_nav_kv_chart_catalog(resource_index, shaded_relief_regions);
    let chart_catalog_bytes = serde_json::to_vec(&chart_catalog)
        .context("failed to encode nav_kv chart/catalog value")?;
    let mut pairs = vec![NavKvPair {
        key: "chart/catalog".to_string(),
        value: chart_catalog_bytes,
    }];
    pairs.extend(build_nav_kv_plate_pairs(resource_index)?);
    pairs.extend(build_nav_kv_navref_pairs(main_db_path)?);
    let built = build_nav_kv_sorted(pairs, 64 * 1024)
        .map_err(|err| anyhow::anyhow!("failed to build nav_kv: {err}"))?;

    let source_dir = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("nav-kv")
        .join(config.profile.as_str())
        .join(cycle);
    fs::create_dir_all(&source_dir)
        .with_context(|| format!("failed to create {}", source_dir.display()))?;

    let root_filename = format!("nav_kv_{cycle}.root");
    let root_source_path = source_dir.join(&root_filename);
    fs::write(&root_source_path, &built.root_bytes)
        .with_context(|| format!("failed to write {}", root_source_path.display()))?;
    let root = publish_bundle_artifact(config, &root_source_path, &root_filename)?;

    let mut value_pages = Vec::new();
    for (index, page) in built.value_pages.iter().enumerate() {
        let page_filename = format!("nav_kv_{cycle}.values_{index:04}");
        let page_source_path = source_dir.join(&page_filename);
        fs::write(&page_source_path, page)
            .with_context(|| format!("failed to write {}", page_source_path.display()))?;
        value_pages.push(publish_bundle_artifact(
            config,
            &page_source_path,
            &page_filename,
        )?);
    }

    Ok(BundleNavKvArtifact {
        root,
        value_pages,
        page_size: built.page_size,
        value_bytes_len: built.value_bytes_len,
    })
}

fn build_nav_kv_chart_catalog(
    resource_index: &ResourceIndex,
    shaded_relief_regions: &[Region],
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
            serde_json::json!({
                "id": collection.id,
                "label": format!(
                    "{} {}",
                    region_display_name(resource_index, &collection.region_id),
                    family_display_name(resource_index, &collection.family_id),
                ),
                "region_id": collection.region_id,
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
        shaded_relief_regions,
    ));
    serde_json::Value::Array(collections)
}

fn build_nav_kv_shaded_relief_catalog_entries(
    resource_index: &ResourceIndex,
    regions: &[Region],
) -> Vec<serde_json::Value> {
    regions
        .iter()
        .map(|region| {
            let region_id = region.code().to_ascii_lowercase();
            let product_id = format!("shaded-relief-{region_id}");
            let region_display_name = region_display_name(resource_index, &region_id);
            let initial_viewport = default_view_for_static_region(resource_index, *region);
            let levels = (TERRAIN_MIN_ZOOM..=TERRAIN_ZOOM)
                .map(|zoom| {
                    let max_tile = (1_u32 << zoom) - 1;
                    serde_json::json!({
                        "zoom": zoom,
                        "x_min": 0,
                        "x_max": max_tile,
                        "y_tms_min": 0,
                        "y_tms_max": max_tile,
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
                    "tile_path_template": "{z}/{x}/{y}.webp",
                    "tile_size": TERRAIN_TILE_SIZE,
                    "min_zoom": TERRAIN_MIN_ZOOM,
                    "max_zoom": f64::from(TERRAIN_ZOOM) + 0.8,
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

fn build_nav_kv_plate_airports(resource_index: &ResourceIndex) -> Vec<serde_json::Value> {
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
            Some(serde_json::json!({
                "id": airport_id,
                "label": airport_id,
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
    pairs.extend(build_nav_kv_fix_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_runway_position_pairs(&connection)?);
    pairs.extend(build_nav_kv_waypoint_lookup_pairs(&connection)?);
    pairs.extend(build_nav_kv_procedure_pairs(&connection)?);
    pairs.extend(build_nav_kv_airway_pairs(&connection)?);
    let mut deduped = BTreeMap::<String, Vec<u8>>::new();
    for pair in pairs {
        deduped.entry(pair.key).or_insert(pair.value);
    }
    Ok(deduped
        .into_iter()
        .map(|(key, value)| NavKvPair { key, value })
        .collect())
}

fn build_nav_kv_airport_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type), trim(ATCT), trim(FuelTypes), trim(Use)
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
            row.get::<_, String>(7)?,
        ))
    })?;
    let runway_info = airport_runway_symbol_info_by_airport(connection)?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind, atct, fuel_types, use_code) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/airport/{key_id}"),
            &serde_json::json!({ "lat": lat, "lon": lon }),
            "navref airport position",
        )?);
        let info = runway_info.get(&id.trim().to_ascii_uppercase());
        let kind_upper = kind.trim().to_ascii_uppercase();
        let private_use = use_code.trim().eq_ignore_ascii_case("PR");
        let heliport = kind_upper.contains("HELIPORT");
        let has_water_runway = info.map(|info| info.has_water_runway).unwrap_or(false)
            || kind.trim().eq_ignore_ascii_case("SEAPLANE BAS");
        if private_use || heliport || has_water_runway {
            continue;
        }
        pairs.push(json_pair(
            format!("navref/symbol/airport/{key_id}"),
            &serde_json::json!({
                "kind": kind.to_ascii_lowercase(),
                "label": airport_display_label(&id),
                "style_class": "airport",
                "towered": atct.trim().eq_ignore_ascii_case("Y"),
                "fuel_available": !fuel_types.trim().is_empty(),
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
        let nav_ref = if identifier.starts_with("RW") {
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
          trim(recommended_navaid),
          trim(altitude_1),
          trim(altitude_2),
          trim(path_and_termination),
          trim(turn_direction),
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
            recommended_navaid,
            altitude_1,
            altitude_2,
            path_termination,
            turn_direction,
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
        let nav_ref = nav_context.classify_json(&fix_identifier);
        let defining_nav_ref = nav_context.classify_json(&recommended_navaid);
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
    fix_positions: BTreeMap<String, serde_json::Value>,
    runway_positions: BTreeMap<(String, String), serde_json::Value>,
    navaid_variation: BTreeMap<String, Option<f64>>,
    airport_variation: BTreeMap<String, Option<f64>>,
}

impl NavLookupContext {
    fn load(connection: &rusqlite::Connection) -> anyhow::Result<Self> {
        Ok(Self {
            airport_positions: load_nav_position_map(
                connection,
                "airports",
                "ARPLatitude",
                "ARPLongitude",
            )?,
            navaid_positions: load_nav_position_map(
                connection,
                "nav",
                "ARPLatitude",
                "ARPLongitude",
            )?,
            fix_positions: load_nav_position_map(connection, "fix", "ARPLatitude", "ARPLongitude")?,
            runway_positions: load_runway_position_map(connection)?,
            navaid_variation: load_variation_map(connection, "nav", "Variation", false)?,
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
        if trimmed.starts_with("RW") {
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

    fn resolve_position_json(
        &self,
        nav_ref: &serde_json::Value,
        procedure_airport_id: Option<&str>,
    ) -> serde_json::Value {
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
                if code.starts_with("RW") {
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
          AND trim(branch_key) <> ''
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
        let nav_ref = nav_context.classify_json(&point_name);
        let nav_ref = if nav_ref.is_null() {
            serde_json::json!({ "LatLon": { "lat": lat, "lon": lon } })
        } else {
            nav_ref
        };
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
        "approach" | "other" => "approach",
        _ => "approach",
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
        "hotspot" => 6,
        _ => 7,
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

fn max_zoom_for_levels(collection: &preprocessor_resource_index::ChartCollectionRecord) -> f64 {
    collection
        .levels
        .iter()
        .map(|level| level.zoom)
        .max()
        .unwrap_or(0) as f64
        + 0.8
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
        match fs::hard_link(&source, &outpath) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(&source, &outpath).with_context(|| {
                    format!(
                        "failed to copy {} to {}",
                        source.display(),
                        outpath.display()
                    )
                })?;
            }
        }
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
    _build_manifest: &BuildManifest,
    bundle_manifest_path: &Path,
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root(config)?;
    remove_legacy_unpacked_subtree(&unpacked_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(bundle_manifest_path, &unpacked_root)?;
    sync_unpacked_file(
        &config.build_root.join(&bundle_manifest.catalog.filename),
        &unpacked_root,
    )?;
    sync_unpacked_file(
        &config
            .build_root
            .join(&bundle_manifest.resource_index.filename),
        &unpacked_root,
    )?;
    if let Some(nav_kv) = &bundle_manifest.nav_kv {
        sync_unpacked_file(
            &config.build_root.join(&nav_kv.root.filename),
            &unpacked_root,
        )?;
        for page in &nav_kv.value_pages {
            sync_unpacked_file(&config.build_root.join(&page.filename), &unpacked_root)?;
        }
    }
    Ok(())
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

fn sync_product_level_unpacked(
    build_root: &Path,
    current_artifacts_path: &Path,
    zip_artifacts: &[PublishedZipArtifact],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_build_root(build_root)?;
    remove_legacy_unpacked_subtree(&unpacked_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(current_artifacts_path, &unpacked_root)?;
    for artifact in zip_artifacts {
        let published_filename = artifact
            .published_zip_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("failed to determine published filename"))?;
        sync_unpacked_zip_from_source(
            &artifact.published_zip_path,
            artifact
                .source_zip_path
                .parent()
                .unwrap_or_else(|| Path::new("/")),
            &unpacked_root,
            published_filename,
            Some(&artifact.checksum_sha256),
        )?;
    }
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

fn obstacle_snapshot_label(value: &str) -> anyhow::Result<String> {
    Ok(NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("failed to parse obstacle snapshot date {value}"))?
        .format("%Y.%m.%d")
        .to_string())
}

fn build_obstacles_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let artifact_root = artifact_root_from_build_root(&config.build_root).to_path_buf();
    let snapshot_date = env::var("AEROBAG_OBSTACLE_SNAPSHOT_DATE")
        .unwrap_or_else(|_| Utc::now().format("%Y-%m-%d").to_string());
    let snapshot_label = obstacle_snapshot_label(&snapshot_date)?;
    let build_root = artifact_root
        .join("private-work")
        .join("obstacles")
        .join(&snapshot_label);
    let output_dir = build_root.join("output");
    let manifest_path = output_dir.join(format!("obstacles_{snapshot_label}.manifest"));
    let stats_path = output_dir.join("stats.json");
    let zip_path = output_dir.join(format!("obstacles_{snapshot_label}.zip"));
    if manifest_path.is_file() && stats_path.is_file() && zip_path.is_file() {
        return Ok((manifest_path, stats_path, zip_path));
    }

    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    let work_dir = build_root.join("work");
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    let provenance_dir = build_root.join("meta").join("provenance").join("obstacles");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    let logical_url = format!(
        "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP#logical_name=obstacle_{snapshot_label}.zip"
    );
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
    prefetch_archives_with_provenance(
        &[logical_url],
        &work_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "obstacles",
    )?;
    let result = build_obstacle_dataset(&BuildObstacleDatasetRequest {
        input_dir: work_dir,
        output_dir,
        version_label: snapshot_label,
    })?;
    Ok((result.manifest_path, result.stats_path, result.zip_path))
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
    let details_dir = input_dir.join("details");

    if build_root.exists() {
        fs::remove_dir_all(&build_root)
            .with_context(|| format!("failed to clear {}", build_root.display()))?;
    }
    fs::create_dir_all(&details_dir)
        .with_context(|| format!("failed to create {}", details_dir.display()))?;

    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    let provenance_dir = build_root.join("meta").join("provenance").join("tfrs");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;

    let list_url = "https://tfr.faa.gov/tfrapi/getTfrList#logical_name=list.json".to_string();
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"tfrs\",\"url\":\"{}\"}}\n",
            list_url
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_archives_with_provenance(
        std::slice::from_ref(&list_url),
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "tfrs-list",
    )?;

    let notam_ids = load_tfr_notam_ids(&input_dir)?;
    let detail_urls = notam_ids
        .iter()
        .map(|notam_id| {
            let sanitized = sanitize_notam_id(notam_id);
            format!(
                "https://tfr.faa.gov/download/detail_{}.xml#logical_name={}.xml",
                sanitized, sanitized
            )
        })
        .collect::<Vec<_>>();
    let mut source_urls_jsonl =
        String::from("{\"event\":\"source_url\",\"label\":\"tfrs-list\",\"url\":\"https://tfr.faa.gov/tfrapi/getTfrList#logical_name=list.json\"}\n");
    for url in &detail_urls {
        source_urls_jsonl.push_str(&format!(
            "{{\"event\":\"source_url\",\"label\":\"tfrs-detail\",\"url\":\"{}\"}}\n",
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
        &detail_urls,
        &details_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "tfrs-detail",
    )?;

    let source_fingerprint = hash_tree(&input_dir)?;
    let version_label = fast_product_version_label(&source_fingerprint);
    let inputs = fast_product_node_inputs("tfrs", &source_fingerprint)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "fast-tfrs")?,
        "fast-tfrs",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let structured_json_path = output_dir.join("tfrs.json");
    let manifest_path = output_dir.join(format!("tfrs_{version_label}.manifest.json"));
    let zip_path = output_dir.join(format!("tfrs_{version_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            structured_json_path.clone(),
            manifest_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => {
            let source_generated_at_utc =
                fast_product_source_generated_at("tfrs", &structured_json_path, &manifest_path)?;
            return Ok((zip_path, source_generated_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_tfr_dataset(&BuildTfrRequest {
        input_dir,
        output_dir,
        version_label,
        generated_at_utc,
    })?;
    let source_generated_at_utc = fast_product_source_generated_at(
        "tfrs",
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
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "fast-metars")?,
        "fast-metars",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let structured_json_path = output_dir.join("metars.json");
    let manifest_path = output_dir.join(format!("metars_{version_label}.manifest.json"));
    let zip_path = output_dir.join(format!("metars_{version_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            structured_json_path.clone(),
            manifest_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => {
            let source_generated_at_utc =
                fast_product_source_generated_at("metars", &structured_json_path, &manifest_path)?;
            return Ok((zip_path, source_generated_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_metar_dataset(&BuildMetarRequest {
        input_xml_path,
        output_dir,
        version_label,
        generated_at_utc,
    })?;
    let source_generated_at_utc = fast_product_source_generated_at(
        "metars",
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
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "fast-nexrad")?,
        "fast-nexrad",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let structured_json_path = output_dir.join("nexrad.json");
    let manifest_path = output_dir.join(format!("nexrad_{version_label}.manifest.json"));
    let zip_path = output_dir.join(format!("nexrad_{version_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            structured_json_path.clone(),
            manifest_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => {
            let source_generated_at_utc =
                fast_product_source_generated_at("nexrad", &structured_json_path, &manifest_path)?;
            return Ok((zip_path, source_generated_at_utc, record));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_nexrad_dataset(&BuildNexradRequest {
        input_dir,
        output_dir,
        version_label,
        generated_at_utc,
    })?;
    let source_generated_at_utc = fast_product_source_generated_at(
        "nexrad",
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

fn build_shaded_relief_product(
    config: &ProductBuildConfig,
    region: Region,
    terrain_index_path: &Path,
    source_fetched_at_utc: Option<String>,
) -> anyhow::Result<(PathBuf, String, Option<String>, NodeRecord)> {
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
        let inputs = shaded_relief_product_inputs(region, &source_fingerprint)?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &format!("static-shaded-relief-{region_id}"))?,
            &format!("static-shaded-relief-{region_id}"),
            &inputs,
        )?;
        let output_dir = prepared.dir.join("output");
        let zip_path = output_dir.join(format!("shaded_relief_{region_id}_{version_label}.zip"));
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
    let inputs = shaded_relief_product_inputs(region, &source_fingerprint)?;
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
    let vrt_path = output_dir.join(format!("shaded_relief_{region_id}.vrt"));
    build_terrain_vrt(&vrt_path, &dem_paths)?;
    build_shaded_relief_region_tiles(
        region,
        &vrt_path,
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

fn shaded_relief_product_inputs(
    region: Region,
    source_fingerprint: &str,
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
            "shaded_relief_pipeline".to_string(),
            SHADED_RELIEF_PIPELINE_VERSION.to_string(),
        ),
        (
            "shaded_relief_workers".to_string(),
            SHADED_RELIEF_TILE_WORKERS.to_string(),
        ),
        (
            "shaded_relief_script".to_string(),
            hash_file(shaded_relief_tile_script_path())?,
        ),
    ]))
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
    let lat = if lat_part.starts_with('s') {
        -lat_abs
    } else {
        lat_abs
    };
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

fn build_shaded_relief_region_tiles(
    region: Region,
    vrt_path: &Path,
    output_dir: &Path,
    version_label: &str,
    dem_selection: &TerrainDemSelection,
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
        let compression = if name.ends_with(".terrain") || name.ends_with(".png") {
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
from pathlib import Path
import numpy as np
from osgeo import gdal

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS

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
    args = ap.parse_args()
    west, south, east, north = [float(x) for x in args.bbox.split(',')]
    root = Path(args.output_dir)
    tiles_root = root / 'tiles'
    ds = gdal.Open(args.vrt)
    if ds is None:
        raise SystemExit(f'failed to open {args.vrt}')
    geo = load_geo(args.geo_csv)
    x_range, y_range = tile_range(west, south, east, north, args.zoom, args.tile_size)
    count = 0
    for x in x_range:
        for y in y_range:
            minx, miny, maxx, maxy = tile_bounds(x, y, args.zoom, args.tile_size)
            warped = gdal.Warp(
                '', ds, format='MEM', dstSRS='EPSG:3857',
                outputBounds=[minx, miny, maxx, maxy],
                width=args.tile_size, height=args.tile_size,
                resampleAlg='bilinear', dstNodata=-999999.0,
            )
            arr = warped.ReadAsArray()
            center_lon, center_lat = lonlat((minx + maxx) / 2.0, (miny + maxy) / 2.0)
            tile_geoid_ft = geoid(geo, center_lat, center_lon)
            invalid = (arr <= -999998.0) | np.isnan(arr)
            samples = np.rint(arr.astype(np.float64) * 3.280839895 + tile_geoid_ft)
            samples = np.clip(samples, -32767, 32767).astype('<i2')
            samples[invalid] = -32768
            write_tile(tiles_root / str(args.zoom) / str(x) / f'{y}.terrain', samples.tobytes(), args.tile_size)
            count += 1
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
        'refresh_policy': {
            'identity': 'published filename is content-addressed by ZIP bytes',
            'source_fetched_at_utc': 'reported in current_artifacts.static_products[]',
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

fn publish_content_addressed_obstacle_zip(
    build_root: &Path,
    obstacle_zip_path: &Path,
) -> anyhow::Result<(PathBuf, String, u64)> {
    publish_content_addressed_zip(build_root, obstacle_zip_path, "obstacles", None, None)
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
        match fs::hard_link(zip_path, &published_path) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(zip_path, &published_path).with_context(|| {
                    format!(
                        "failed to copy {} to {}",
                        zip_path.display(),
                        published_path.display()
                    )
                })?;
            }
        }
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
                .map(|name| name.starts_with("bundle_") && name.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    bundle_paths.sort();

    let mut bundles = Vec::new();
    for bundle_path in bundle_paths {
        let bundle_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&bundle_path)
                .with_context(|| format!("failed to read {}", bundle_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
        let bundle_cycle = bundle_manifest
            .get("cycle")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing top-level cycle"))?;
        let file_cycle = bundle_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.strip_prefix("bundle_"))
            .unwrap_or("unknown");
        if bundle_cycle != file_cycle {
            anyhow::bail!(
                "bundle cycle mismatch for {}: payload cycle {} != filename cycle {}",
                bundle_path.display(),
                bundle_cycle,
                file_cycle
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
        let end_valid_date = NaiveDate::parse_from_str(end_valid, "%Y-%m-%d")
            .with_context(|| format!("failed to parse bundle end_valid {end_valid}"))?;
        if end_valid_date < as_of_date {
            continue;
        }
        bundles.push(CurrentBundleEntry {
            filename: bundle_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            cycle: bundle_cycle.to_string(),
            start_valid: start_valid.to_string(),
            end_valid: end_valid.to_string(),
            checksum_sha256: hash_file(&bundle_path)?,
            size_bytes: fs::metadata(&bundle_path)
                .with_context(|| format!("failed to stat {}", bundle_path.display()))?
                .len(),
        });
    }
    Ok(bundles)
}

fn write_current_artifacts_manifest(
    build_root: &Path,
    as_of_date: NaiveDate,
    published_obstacle_zip: &Path,
    obstacle_sha256: &str,
    obstacle_size_bytes: u64,
    static_products: Vec<CurrentStaticProductEntry>,
    fast_products: Vec<CurrentFastProductEntry>,
) -> anyhow::Result<PathBuf> {
    let bundles = build_current_bundle_entries(build_root, as_of_date)?;
    let published_date = as_of_date.format("%Y-%m-%d").to_string();
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        as_of_date: published_date.clone(),
        bundles,
        obstacles: CurrentObstacleEntry {
            filename: published_obstacle_zip
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            published_date: published_date.clone(),
            checksum_sha256: obstacle_sha256.to_string(),
            size_bytes: obstacle_size_bytes,
        },
        static_products,
        fast_products,
    };
    let manifest_path = build_root.join(format!(
        "current_artifacts_{}.json",
        as_of_date.format("%Y%m%d")
    ));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest)
            .context("failed to encode current artifacts manifest")?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(manifest_path)
}

fn validate_packaged_contract(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    validate_no_internal_paths_in_json(current_artifacts_path)?;
    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;

    for bundle in &current.bundles {
        validate_public_filename(&bundle.filename, "current_artifacts.bundles[].filename")?;
        let bundle_path = packaged_root.join(&bundle.filename);
        ensure_public_file_exists(&bundle_path)?;
        validate_bundle_manifest(packaged_root, &bundle_path)?;
    }

    validate_public_filename(
        &current.obstacles.filename,
        "current_artifacts.obstacles.filename",
    )?;
    ensure_public_file_exists(&packaged_root.join(&current.obstacles.filename))?;
    for product in &current.static_products {
        validate_public_filename(
            &product.filename,
            "current_artifacts.static_products[].filename",
        )?;
        ensure_public_file_exists(&packaged_root.join(&product.filename))?;
    }
    for product in &current.fast_products {
        validate_public_filename(
            &product.filename,
            "current_artifacts.fast_products[].filename",
        )?;
        ensure_public_file_exists(&packaged_root.join(&product.filename))?;
    }
    Ok(())
}

fn validate_bundle_manifest(packaged_root: &Path, bundle_path: &Path) -> anyhow::Result<()> {
    validate_no_internal_paths_in_json(bundle_path)?;
    let bundle: BundleManifest = serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))?;

    for artifact in [
        &bundle.catalog,
        &bundle.resource_index,
        &bundle.data,
        &bundle.vectors,
    ] {
        validate_bundle_artifact_ref(packaged_root, artifact)?;
    }
    if let Some(nav_kv) = &bundle.nav_kv {
        validate_bundle_artifact_ref(packaged_root, &nav_kv.root)?;
        for page in &nav_kv.value_pages {
            validate_bundle_artifact_ref(packaged_root, page)?;
        }
    }
    for package in &bundle.packages {
        validate_public_filename(&package.filename, "bundle.packages[].filename")?;
        validate_public_filename(&package.relative_path, "bundle.packages[].relative_path")?;
        if package.filename != package.relative_path {
            bail!(
                "package filename/relative_path mismatch in {}: {} != {}",
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
    let current_filename = current_artifacts_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("current artifacts path has no filename")?;
    let unpacked_current_path = unpacked_root.join(current_filename);
    ensure_public_file_exists(&unpacked_current_path)?;
    validate_no_internal_paths_in_json(&unpacked_current_path)?;

    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;

    for bundle in &current.bundles {
        let unpacked_bundle_path = unpacked_root.join(&bundle.filename);
        ensure_public_file_exists(&unpacked_bundle_path)?;
        validate_no_internal_paths_in_json(&unpacked_bundle_path)?;
        let bundle: BundleManifest = serde_json::from_slice(
            &fs::read(&unpacked_bundle_path)
                .with_context(|| format!("failed to read {}", unpacked_bundle_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", unpacked_bundle_path.display()))?;

        for artifact in [&bundle.catalog, &bundle.resource_index] {
            ensure_public_file_exists(&unpacked_root.join(&artifact.filename))?;
        }
        if let Some(nav_kv) = &bundle.nav_kv {
            ensure_public_file_exists(&unpacked_root.join(&nav_kv.root.filename))?;
            for page in &nav_kv.value_pages {
                ensure_public_file_exists(&unpacked_root.join(&page.filename))?;
            }
        }
        for artifact in [&bundle.data, &bundle.vectors] {
            ensure_public_dir_exists(&unpacked_root.join(zip_stem(&artifact.filename)?))?;
        }
        for package in &bundle.packages {
            ensure_public_dir_exists(&unpacked_root.join(zip_stem(&package.filename)?))?;
        }
    }

    ensure_public_dir_exists(&unpacked_root.join(zip_stem(&current.obstacles.filename)?))?;
    for product in &current.static_products {
        ensure_public_dir_exists(&unpacked_root.join(zip_stem(&product.filename)?))?;
    }
    for product in &current.fast_products {
        ensure_public_dir_exists(&unpacked_root.join(zip_stem(&product.filename)?))?;
    }
    Ok(())
}

fn validate_bundle_artifact_ref(
    packaged_root: &Path,
    artifact: &BundleArtifact,
) -> anyhow::Result<()> {
    validate_public_filename(&artifact.filename, "bundle artifact filename")?;
    validate_public_filename(&artifact.relative_path, "bundle artifact relative_path")?;
    if artifact.filename != artifact.relative_path {
        bail!(
            "bundle artifact filename/relative_path mismatch: {} != {}",
            artifact.filename,
            artifact.relative_path
        );
    }
    ensure_public_file_exists(&packaged_root.join(&artifact.filename))
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

fn write_published_json<T: Serialize>(published_path: &Path, value: &T) -> anyhow::Result<PathBuf> {
    fs::write(
        published_path,
        serde_json::to_vec_pretty(value).context("failed to encode published json")?,
    )
    .with_context(|| format!("failed to write {}", published_path.display()))?;
    Ok(published_path.to_path_buf())
}

fn rewrite_public_resource_index(
    index: &ResourceIndex,
    data_filename: &str,
    package_artifacts: &[BundlePackageArtifact],
) -> ResourceIndex {
    let mut public_index = index.clone();
    let _ = data_filename;
    let _ = package_artifacts;
    public_index.nav_db.artifact_path = None;
    for package in &mut public_index.packages {
        package.artifact_path = None;
    }
    public_index
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
    match fs::hard_link(source_path, published_path) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source_path, published_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    published_path.display()
                )
            })?;
            Ok(())
        }
    }
}

fn canonical_package_filename(
    family_id: &str,
    region_id: &str,
    original_filename: &str,
) -> anyhow::Result<String> {
    let cycle = Path::new(original_filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit('_').next())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("failed to derive cycle from package filename {original_filename}")
        })?;
    Ok(format!(
        "{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
        cycle
    ))
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
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected)? {
        NodeCacheState::CacheHit(record) => return Ok((output_dir, record)),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&output_dir)?;
    emit_source_urls(
        &output_dir,
        Some(&resolved_cycle),
        Some(&fetch_cache_config(config)?),
    )?;
    let outputs = BTreeMap::from([(
        "output_dir".to_string(),
        relative_artifact_path(&output_dir, &config.build_root),
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
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected)? {
        NodeCacheState::CacheHit(record) => return Ok((output_dir, record)),
        NodeCacheState::Build(lock) => lock,
    };
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to remove {}", output_dir.display()))?;
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    copy_dir_recursive(override_root, &output_dir)?;
    let outputs = BTreeMap::from([(
        "output_dir".to_string(),
        relative_artifact_path(&output_dir, &config.build_root),
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
    let _build_lock = match claim_or_wait_for_node(&prepared, &[tiles_root.clone()])? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
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
    let outputs = BTreeMap::from([
        (
            "work_dir".to_string(),
            relative_artifact_path(&work_dir, &config.build_root),
        ),
        (
            "tiles_root".to_string(),
            relative_artifact_path(&tiles_root, &config.build_root),
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
    let existing_package_records = read_package_outputs_by_region(&aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
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
        ]);
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &node_name)?,
            &node_name,
            &inputs,
        )?;
        let zip_path = work_dir.join(format!(
            "{}_{}_{}.zip",
            region.code(),
            manifest_chart_name(family),
            version_label
        ));
        let manifest_path = work_dir.join(format!(
            "{}_{}_{}.manifest",
            region.code(),
            manifest_chart_name(family),
            version_label
        ));
        if let Some(record) =
            try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])?
        {
            node_records.push(record);
        } else {
            let _build_lock = match claim_or_wait_for_node(
                &prepared,
                &[zip_path.clone(), manifest_path.clone()],
            )? {
                NodeCacheState::CacheHit(record) => {
                    node_records.push(record);
                    if let Some(existing) = existing_package_records.get(region.code()) {
                        package_records.push(existing.clone());
                    } else {
                        package_records.push(PackageOutputRecord {
                            label: family.capture_label().to_string(),
                            chart: Some(manifest_chart_name(family).to_string()),
                            region: region.code().to_string(),
                            manifest: format!(
                                "{}_{}_{}.manifest",
                                region.code(),
                                manifest_chart_name(family),
                                version_label
                            ),
                            manifest_sha256: hash_file(&manifest_path)?,
                            zip: format!(
                                "{}_{}_{}.zip",
                                region.code(),
                                manifest_chart_name(family),
                                version_label
                            ),
                            zip_sha256: hash_file(&zip_path)?,
                        });
                    }
                    continue;
                }
                NodeCacheState::Build(lock) => lock,
            };
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            let package_record = package_family_region_versioned(
                family,
                &work_dir,
                region,
                version_label,
                version_label,
            )?;
            let outputs = BTreeMap::from([
                (
                    "zip".to_string(),
                    relative_artifact_path(&zip_path, &config.build_root),
                ),
                (
                    "manifest".to_string(),
                    relative_artifact_path(&manifest_path, &config.build_root),
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
            node_records.push(record);
            package_records.push(package_record);
            continue;
        }
        if let Some(existing) = existing_package_records.get(region.code()) {
            package_records.push(existing.clone());
        } else {
            package_records.push(PackageOutputRecord {
                label: family.capture_label().to_string(),
                chart: Some(manifest_chart_name(family).to_string()),
                region: region.code().to_string(),
                manifest: format!(
                    "{}_{}_{}.manifest",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                ),
                manifest_sha256: hash_file(&manifest_path)?,
                zip: format!(
                    "{}_{}_{}.zip",
                    region.code(),
                    manifest_chart_name(family),
                    version_label
                ),
                zip_sha256: hash_file(&zip_path)?,
            });
        }
    }
    if let Some(parent) = aggregate_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_package_outputs_jsonl(
        aggregate_path
            .parent()
            .context("chart aggregate path missing parent")?,
        &package_records,
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
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&marker))? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    render_csup_region(work_dir, region, render_jobs)?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "work_dir".to_string(),
            relative_artifact_path(work_dir, &config.build_root),
        ),
        (
            "marker".to_string(),
            relative_artifact_path(&marker, &config.build_root),
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
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&marker))? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
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
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
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
    let existing_package_records = read_package_outputs_by_region(&aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
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
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &node_name)?,
            &node_name,
            &inputs,
        )?;
        let zip_path = work_dir.join(format!("{}_CSUP_{}.zip", region.code(), version_label));
        let manifest_path =
            work_dir.join(format!("{}_CSUP_{}.manifest", region.code(), version_label));
        if let Some(record) =
            try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])?
        {
            node_records.push(record);
        } else {
            let _build_lock = match claim_or_wait_for_node(
                &prepared,
                &[zip_path.clone(), manifest_path.clone()],
            )? {
                NodeCacheState::CacheHit(record) => {
                    node_records.push(record);
                    if let Some(existing) = existing_package_records.get(region.code()) {
                        package_records.push(existing.clone());
                    } else {
                        package_records.push(PackageOutputRecord {
                            label: "csup".to_string(),
                            chart: None,
                            region: region.code().to_string(),
                            manifest: format!("{}_CSUP_{}.manifest", region.code(), version_label),
                            manifest_sha256: hash_file(&manifest_path)?,
                            zip: format!("{}_CSUP_{}.zip", region.code(), version_label),
                            zip_sha256: hash_file(&zip_path)?,
                        });
                    }
                    continue;
                }
                NodeCacheState::Build(lock) => lock,
            };
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            let package_record =
                package_csup_region_versioned(&work_dir, region, version_label, version_label)?;
            let outputs = BTreeMap::from([
                (
                    "zip".to_string(),
                    relative_artifact_path(&zip_path, &config.build_root),
                ),
                (
                    "manifest".to_string(),
                    relative_artifact_path(&manifest_path, &config.build_root),
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
            node_records.push(record);
            package_records.push(package_record);
            continue;
        }
        if let Some(existing) = existing_package_records.get(region.code()) {
            package_records.push(existing.clone());
        } else {
            package_records.push(PackageOutputRecord {
                label: "csup".to_string(),
                chart: None,
                region: region.code().to_string(),
                manifest: format!("{}_CSUP_{}.manifest", region.code(), version_label),
                manifest_sha256: hash_file(&manifest_path)?,
                zip: format!("{}_CSUP_{}.zip", region.code(), version_label),
                zip_sha256: hash_file(&zip_path)?,
            });
        }
    }
    if let Some(parent) = aggregate_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_package_outputs_jsonl(
        aggregate_path
            .parent()
            .context("csup aggregate path missing parent")?,
        &package_records,
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
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&plates_root))? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let mut request = request.clone();
    request.run_root = run_root;
    let result = render_native_tpp(&request)?;
    let outputs = BTreeMap::from([
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
            "main_db".to_string(),
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
    raw_main_db: &Path,
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
    let main_db_path = output_dir.join("main.db");
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            main_db_path.clone(),
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
        input_main_db: raw_main_db.to_path_buf(),
        input_zip: raw_zip.to_path_buf(),
        output_dir: output_dir.clone(),
        artifact_stem: artifact_stem.to_string(),
        tpp_package_zips: tpp_zips,
    })?;
    let outputs = BTreeMap::from([
        (
            "main_db".to_string(),
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
    main_db: &Path,
    data_fingerprint: &str,
    version_label: &str,
) -> anyhow::Result<NodeRecord> {
    let inputs = BTreeMap::from([
        ("data_fingerprint".to_string(), data_fingerprint.to_string()),
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
        main_db: main_db.to_path_buf(),
        output_dir: output_dir.clone(),
        version_label: version_label.to_string(),
    };
    let zip_path = output_dir.join(format!("vectors_{version_label}.zip"));
    let stats_path = output_dir.join("stats.json");
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[zip_path.clone(), stats_path.clone()])? {
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
            format!(
                "{}:{}:{}:{}",
                source.family_id,
                source.package_outputs_path.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tpp_json = tpp_sources
        .iter()
        .map(|source| {
            format!(
                "{}:{}:{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let csup_json = csup_sources
        .iter()
        .map(|source| {
            format!(
                "{}:{}:{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
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

    #[test]
    fn nav_kv_chart_catalog_includes_shaded_relief_static_products() {
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &[Region::Nw]);
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
        assert_eq!(shaded["map_view"]["tile_path_template"], "{z}/{x}/{y}.webp");
        assert_eq!(shaded["map_view"]["storage_kind"], "static_product");
        assert_eq!(shaded["map_view"]["initial_viewport"]["lat"], 45.0);
        assert_eq!(
            shaded["map_view"]["levels"]
                .as_array()
                .expect("levels should be an array")
                .len(),
            11
        );
    }

    #[test]
    fn nav_kv_chart_catalog_emits_tile_path_templates_for_chart_packages() {
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &[]);
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
}
