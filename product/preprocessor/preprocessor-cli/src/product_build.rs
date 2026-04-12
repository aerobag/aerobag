use std::{
    collections::BTreeMap,
    env, fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::Instant,
};

use anyhow::{bail, Context};
use chrono::{Datelike, NaiveDate, Utc};
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, package_family_region_versioned, stage_work_dir,
};
use preprocessor_core::{ChartFamily, Region};
use preprocessor_csup::{
    package_csup_region_versioned, prepare_csup_inputs, render_csup_region, stage_work_dir_for_product,
};
use preprocessor_data::{build_data_package, DataBuildMode, DataBuildRequest};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    read_source_urls_jsonl, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource, ResourceIndex,
};
use preprocessor_tpp::{
    package_native_tpp_versioned, render_native_tpp, NativeTppRunRequest,
};
use preprocessor_vectors::{build_vectors_dataset, BuildVectorsRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::emit_source_urls::{discover_published_cycles, emit_source_urls};

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
struct BundleManifest {
    schema_version: u32,
    cycle: String,
    generated_at_utc: String,
    start_valid: String,
    end_valid: String,
    catalog: BundleArtifact,
    resource_index: BundleArtifact,
    data: BundleArtifact,
    vectors: BundleArtifact,
    packages: Vec<BundlePackageArtifact>,
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
    region_id: String,
    filename: String,
    relative_path: String,
    checksum_sha256: String,
    size_bytes: u64,
    effective_date: Option<String>,
    expiration_date: Option<String>,
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
}

impl Drop for BuildLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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
    TppRender { region: Region, run_root: PathBuf },
    Data,
    ChartPackage { family: ChartFamily },
    CsupPackage,
    TppPackage { region: Region, run_root: PathBuf },
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
    CsupStage { record: NodeRecord, work_dir: PathBuf },
    ChartSource(ChartSource),
    CsupSource(AssetSource),
    TppSource(AssetSource),
    Data { main_db: PathBuf, zip: PathBuf },
    ZipArtifact { zip: PathBuf },
}

#[derive(Debug, Clone)]
struct TaskCompletion {
    node_records: Vec<NodeRecord>,
    value: TaskValue,
    completion_detail: String,
}

const PRODUCT_BUILD_CGROUP_ACTIVE_ENV: &str = "PRODUCT_BUILD_CGROUP_ACTIVE";
const DEFAULT_PRODUCT_BUILD_MEMORY_MAX: &str = "80G";

pub fn explain_product_build(config: &ProductBuildConfig) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("profile {}", config.profile.as_str()));
    lines.push(format!("build_root {}", config.build_root.display()));
    lines.push(format!("chart_cutline_root {}", config.chart_cutline_root.display()));
    lines.push(format!("fetch_cache_root {}", config.fetch_cache_root.display()));
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
    lines.push("  data".to_string());
    lines.push("  vectors".to_string());
    lines.push("  resource-index".to_string());
    Ok(lines.join("\n") + "\n")
}

pub fn build_cycle(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let log_root = config.build_root.join("orchestrator-logs");
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
        node_records.push(normalize_node_record_paths(source_urls_record, &config.build_root));

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

        let chart_families = [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ];
        let work_unit_budget = config.max_heavy_jobs.max(1) * 4;
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
            let version = tpp_versions
                .get(&region_id)
                .expect("tpp region version should exist");
            let run_root = build_shared_work_root(config, &format!("tpp-{region_id}-{version}"))?;
            let render_id = format!("tpp-{region_id}");
            let package_id = format!("tpp-{region_id}-package");
            pending_tasks.push(ScheduledTask {
                id: render_id.clone(),
                deps: vec![],
                weight: 2,
                kind: ScheduledTaskKind::TppRender {
                    region: *region,
                    run_root: run_root.clone(),
                },
            });
            pending_tasks.push(ScheduledTask {
                id: package_id.clone(),
                deps: vec![render_id],
                weight: 1,
                kind: ScheduledTaskKind::TppPackage {
                    region: *region,
                    run_root,
                },
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
            id: "data".to_string(),
            deps: vec![],
            weight: 4,
            kind: ScheduledTaskKind::Data,
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
            "scheduler-ready tasks={} work_unit_budget={} heavy_task_weight=4 light_task_weight=1 resource_index_weight=2",
            total_tasks, work_unit_budget
        ))?;
        let (tx, rx) = mpsc::channel::<(String, usize, anyhow::Result<TaskCompletion>)>();
        let mut running_jobs = 0_usize;
        let mut running_units = 0_usize;
        let mut launched_tasks = 0_usize;
        let mut completed_tasks = 0_usize;
        let mut completed_ids = std::collections::BTreeSet::<String>::new();
        let mut task_values = BTreeMap::<String, TaskValue>::new();

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
                thread::spawn(move || -> anyhow::Result<()> {
                    let result = match task.kind {
                        ScheduledTaskKind::ChartRender { family } => {
                            let family_id = family_slug(family).to_string();
                            let record = build_chart_render_node(
                                &config,
                                family,
                                &config.chart_cutline_root,
                                &source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
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
                                let work_dir = resolve_artifact_path(&config, output_path(&record, "work_dir")?);
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
                                Some(TaskValue::CsupStage { record, work_dir }) => (record, work_dir),
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
                        ScheduledTaskKind::TppRender { region, run_root } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let request = NativeTppRunRequest {
                                region,
                                source_repo: PathBuf::new(),
                                run_root,
                                prefetch_source_urls: Some(
                                    source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                                ),
                                fetch_jobs: config.fetch_jobs,
                                render_jobs: config.cpu_jobs.max(1),
                            };
                            build_tpp_render_node(&config, &request).map(|record| TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::None,
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }
                        ScheduledTaskKind::Data => build_data_nodes(&config, &source_urls_dir).and_then(|records| {
                            let data_record = records
                                .iter()
                                .find(|record| record.name == "data")
                                .cloned()
                                .context("data task missing data node record")?;
                            let zip = resolve_artifact_path(&config, output_path(&data_record, "zip")?);
                            let main_db = resolve_artifact_path(&config, output_path(&data_record, "main_db")?);
                            Ok(TaskCompletion {
                                node_records: records,
                                value: TaskValue::Data { main_db, zip },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        }),
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
                        ScheduledTaskKind::TppPackage { region, run_root } => {
                            let region_id = region.code().to_ascii_lowercase();
                            let started = Instant::now();
                            let (record, source) = build_tpp_package_node(
                                &config,
                                region,
                                &run_root,
                                &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                                tpp_versions
                                    .get(&region_id)
                                    .expect("tpp region version should exist"),
                            )?;
                            let cache_hit = record.cache_hit;
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::TppSource(source),
                                completion_detail: format!(
                                    "elapsed_ms={} cache_hit={}",
                                    started.elapsed().as_millis(),
                                    cache_hit,
                                ),
                            })
                        }
                        ScheduledTaskKind::Vectors => {
                            let data = match task_values_snapshot.get("data") {
                                Some(TaskValue::Data { main_db, zip: _ }) => main_db,
                                _ => unreachable!("data dependency should have completed"),
                            };
                            let record = build_vectors_node(&config, data, &data_version)?;
                            let cache_hit = record.cache_hit;
                            let zip = resolve_artifact_path(&config, output_path(&record, "zip")?);
                            Ok(TaskCompletion {
                                node_records: vec![record],
                                value: TaskValue::ZipArtifact { zip },
                                completion_detail: format!("cache_hit={}", cache_hit),
                            })
                        }
                        ScheduledTaskKind::ResourceIndex => {
                            let data_zip = match task_values_snapshot.get("data") {
                                Some(TaskValue::Data { main_db: _, zip }) => zip.clone(),
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
                            let csup_sources = vec![match task_values_snapshot.get("csup-package") {
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
                                        Some(TaskValue::TppSource(source)) => Ok(source.clone()),
                                        _ => bail!("missing tpp package source for {region_id}"),
                                    }
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            let record =
                                build_resource_index_node(&config, &data_zip, chart_sources, tpp_sources, csup_sources)?;
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
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config, &bundle_cycle)?;
                            let (cache_hit, unpack_dir) =
                                sync_unpacked_zip(&config, &zip_path, &unpacked_root)?;
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
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config, &bundle_cycle)?;
                            let (cache_hit, unpack_dir) =
                                sync_unpacked_zip(&config, &zip_path, &unpacked_root)?;
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
                                Some(TaskValue::TppSource(source)) => source.clone(),
                                _ => bail!("missing tpp package source for {region_id}"),
                            };
                            let package = package_record_for_region(&source.package_outputs_path, region)?;
                            let zip_path = source.package_root.join(&package.zip);
                            let unpacked_root = published_unpacked_root(&config, &bundle_cycle)?;
                            let (cache_hit, unpack_dir) =
                                sync_unpacked_zip(&config, &zip_path, &unpacked_root)?;
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
                                Some(TaskValue::Data { main_db: _, zip }) => zip.clone(),
                                _ => bail!("missing data zip"),
                            };
                            let unpacked_root = published_unpacked_root(&config, &bundle_cycle)?;
                            let (cache_hit, unpack_dir) =
                                sync_unpacked_zip(&config, &zip, &unpacked_root)?;
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
                                Some(TaskValue::ZipArtifact { zip }) => zip.clone(),
                                _ => bail!("missing vectors zip"),
                            };
                            let unpacked_root = published_unpacked_root(&config, &bundle_cycle)?;
                            let (cache_hit, unpack_dir) =
                                sync_unpacked_zip(&config, &zip, &unpacked_root)?;
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
                    };
                    let _ = tx.send((task_id, task_weight, result));
                    Ok(())
                });
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

            let (task_id, task_weight, result) = rx
                .recv()
                .context("scheduler channel closed unexpectedly")?;
            running_jobs -= 1;
            running_units = running_units.saturating_sub(task_weight);
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
        let build_manifest_path = config
            .build_root
            .join(format!("build-manifest_{bundle_cycle}.json"));
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
        sync_unpacked_metadata(config, &build_manifest, &build_manifest_path, &bundle_manifest_path)?;
        Ok(bundle_manifest_path)
    })();

    match result {
        Ok(manifest_path) => {
            master_log.log(format!("complete PASS manifest={}", manifest_path.display()))?;
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

    let resource_index_path = resolve_artifact_path(config, output_path(resource_index_record, "resource_index")?);
    let catalog_path = resolve_artifact_path(config, output_path(resource_index_record, "catalog")?);
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
        .context("resource-index missing end-valid date")?;

    Ok(BundleManifest {
        schema_version: 1,
        cycle: build_manifest.cycle.clone(),
        generated_at_utc: build_manifest.generated_at_utc.clone(),
        start_valid,
        end_valid,
        catalog: bundle_artifact(config, output_path(resource_index_record, "catalog")?, &catalog_path)?,
        resource_index: bundle_artifact(
            config,
            output_path(resource_index_record, "resource_index")?,
            &resource_index_path,
        )?,
        data: bundle_artifact(config, output_path(data_record, "zip")?, &data_zip_path)?,
        vectors: bundle_artifact(config, output_path(vectors_record, "zip")?, &vectors_zip_path)?,
        packages: index
            .packages
            .iter()
            .map(|package| {
                let package_path = resolve_product_build_path(config, &package.artifact_path);
                Ok(BundlePackageArtifact {
                    id: package.id.clone(),
                    family_id: package.family_id.clone(),
                    region_id: package.region_id.clone(),
                    filename: Path::new(&package.artifact_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    relative_path: package.artifact_path.clone(),
                    checksum_sha256: package.checksum_sha256.clone(),
                    size_bytes: fs::metadata(&package_path)
                        .with_context(|| format!("failed to stat {}", package_path.display()))?
                        .len(),
                    effective_date: package.effective_date.clone(),
                    expiration_date: package.expiration_date.clone(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
    })
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

fn resolve_product_build_path(config: &ProductBuildConfig, relative_path: &str) -> PathBuf {
    config
        .build_root
        .parent()
        .expect("build root should live under <artifact-root>/product-builds/<profile>")
        .join(relative_path)
}

fn published_unpacked_root(
    config: &ProductBuildConfig,
    cycle: &str,
) -> anyhow::Result<PathBuf> {
    Ok(
        artifact_root_from_build_root(&config.build_root)
            .join("published-unpacked")
            .join(config.profile.as_str())
            .join(cycle),
    )
}

fn unpacked_target_dir(config: &ProductBuildConfig, unpacked_root: &Path, zip_path: &Path) -> anyhow::Result<PathBuf> {
    let artifact_root = normalize_absolute_path(artifact_root_from_build_root(&config.build_root));
    let normalized_zip_path = normalize_absolute_path(zip_path);
    let relative = normalized_zip_path
        .strip_prefix(&artifact_root)
        .with_context(|| format!("failed to relativize {}", zip_path.display()))?;
    let relative_dir = relative.with_extension("");
    Ok(unpacked_root.join(relative_dir))
}

fn sync_unpacked_zip(
    config: &ProductBuildConfig,
    zip_path: &Path,
    unpacked_root: &Path,
) -> anyhow::Result<(bool, PathBuf)> {
    let unpack_dir = unpacked_target_dir(config, unpacked_root, zip_path)?;
    let marker_path = unpack_dir.join(".source-zip-sha256");
    let zip_sha256 = hash_file(zip_path)?;
    if unpack_dir.is_dir() && fs::read_to_string(&marker_path).ok().as_deref() == Some(zip_sha256.as_str()) {
        return Ok((true, unpack_dir));
    }
    if unpack_dir.exists() {
        fs::remove_dir_all(&unpack_dir)
            .with_context(|| format!("failed to remove {}", unpack_dir.display()))?;
    }
    fs::create_dir_all(&unpack_dir)
        .with_context(|| format!("failed to create {}", unpack_dir.display()))?;
    extract_zip_to_dir(zip_path, &unpack_dir)?;
    fs::write(&marker_path, format!("{zip_sha256}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))?;
    Ok((false, unpack_dir))
}

fn extract_zip_to_dir(zip_path: &Path, output_dir: &Path) -> anyhow::Result<()> {
    let file = File::open(zip_path).with_context(|| format!("failed to open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", zip_path.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).with_context(|| {
            format!("failed to read zip member #{index} from {}", zip_path.display())
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
        let mut output =
            File::create(&outpath).with_context(|| format!("failed to create {}", outpath.display()))?;
        std::io::copy(&mut entry, &mut output)
            .with_context(|| format!("failed to extract {} from {}", member, zip_path.display()))?;
    }
    Ok(())
}

fn sync_unpacked_metadata(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    build_manifest_path: &Path,
    bundle_manifest_path: &Path,
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root(config, &build_manifest.cycle)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    copy_into_unpacked_root(config, build_manifest_path, &unpacked_root)?;
    copy_into_unpacked_root(config, bundle_manifest_path, &unpacked_root)?;
    for node in &build_manifest.nodes {
        for output in node.outputs.values() {
            let candidate = if Path::new(output).is_absolute() {
                PathBuf::from(output)
            } else {
                resolve_artifact_path(config, output)
            };
            if candidate.extension().and_then(|value| value.to_str()) == Some("zip") {
                continue;
            }
            if candidate.is_file() {
                copy_into_unpacked_root(config, &candidate, &unpacked_root)?;
            }
        }
    }
    Ok(())
}

fn copy_into_unpacked_root(
    config: &ProductBuildConfig,
    source_path: &Path,
    unpacked_root: &Path,
) -> anyhow::Result<()> {
    let artifact_root = normalize_absolute_path(artifact_root_from_build_root(&config.build_root));
    let normalized_source_path = normalize_absolute_path(source_path);
    let relative = normalized_source_path
        .strip_prefix(&artifact_root)
        .with_context(|| format!("failed to relativize {}", source_path.display()))?;
    let dest_path = unpacked_root.join(relative);
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source_path, &dest_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source_path.display(),
            dest_path.display()
        )
    })?;
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

fn bundle_artifact(
    _config: &ProductBuildConfig,
    relative_path: &str,
    absolute_path: &Path,
) -> anyhow::Result<BundleArtifact> {
    Ok(BundleArtifact {
        filename: absolute_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        relative_path: relative_path.to_string(),
        checksum_sha256: hash_file(absolute_path)?,
        size_bytes: fs::metadata(absolute_path)
            .with_context(|| format!("failed to stat {}", absolute_path.display()))?
            .len(),
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
        let mut build_root = artifact_root.join("product-builds").join(profile.as_str());
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
                    build_root = artifact_root.join("product-builds").join(profile.as_str());
                    index += 2;
                }
                "--chart-cutline-root" => {
                    chart_cutline_root = PathBuf::from(args.get(index + 1).context("missing value for --chart-cutline-root")?);
                    index += 2;
                }
                "--build-root" => {
                    build_root = PathBuf::from(args.get(index + 1).context("missing value for --build-root")?);
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

fn build_source_urls_node(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    if let Some(override_root) = env_path("AEROBAG_SOURCE_URLS_ROOT") {
        return build_overridden_source_urls_node(config, &override_root);
    }
    let resolved_cycle = match &config.target_cycle {
        Some(cycle) => cycle.clone(),
        None => discover_published_cycles()?
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
    env::set_var("FETCH_CACHE_ROOT", &config.fetch_cache_root);
    env::set_var("FETCH_CACHE_MODE", &config.fetch_cache_mode);
    emit_source_urls(&output_dir, Some(&resolved_cycle))?;
    let outputs = BTreeMap::from([("output_dir".to_string(), relative_artifact_path(&output_dir, &config.build_root))]);
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
    let outputs =
        BTreeMap::from([("output_dir".to_string(), relative_artifact_path(&output_dir, &config.build_root))]);
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
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
    prefetch_archives_with_provenance(&urls, &work_dir, fetch_jobs, &provenance_dir, family.capture_label())?;
    build_family_vrts(family, &work_dir, cpu_jobs)?;
    build_family_tiles(family, &work_dir, cpu_jobs)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(&work_dir, &config.build_root)),
        ("tiles_root".to_string(), relative_artifact_path(&tiles_root, &config.build_root)),
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
    let render_prepared =
        prepare_node_at(&build_shared_node_dir(config, &render_node_name)?, &render_node_name, &render_inputs)?;
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
        let node_name = format!("charts-{family_id}-package-{}", region.code().to_ascii_lowercase());
        let inputs = BTreeMap::from([
            ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
            ("region".to_string(), region.code().to_string()),
            ("version_label".to_string(), version_label.to_string()),
        ]);
        let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
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
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
            let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
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
                ("zip".to_string(), relative_artifact_path(&zip_path, &config.build_root)),
                ("manifest".to_string(), relative_artifact_path(&manifest_path, &config.build_root)),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
    let marker = work_dir.join(format!(".render-complete-{}", region.code().to_ascii_lowercase()));
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&marker))? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    render_csup_region(work_dir, region, render_jobs)?;
    fs::write(&marker, b"ok")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(work_dir, &config.build_root)),
        ("marker".to_string(), relative_artifact_path(&marker, &config.build_root)),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, "csup-stage")?, "csup-stage", &inputs)?;
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
    prefetch_archives_with_provenance(&urls, &work_dir, fetch_jobs, &provenance_dir, "csup")?;
    prepare_csup_inputs(&work_dir)?;
    fs::write(&marker, b"ok")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(&work_dir, &config.build_root)),
        (
            "provenance_dir".to_string(),
            relative_artifact_path(&provenance_dir, &config.build_root),
        ),
        ("marker".to_string(), relative_artifact_path(&marker, &config.build_root)),
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
    let stage_prepared = prepare_node_at(&build_shared_node_dir(config, "csup-stage")?, "csup-stage", &stage_inputs)?;
    let stage_record = load_existing_node_record(&stage_prepared.record_path, "csup-stage")?;
    let work_dir = resolve_artifact_path(config, output_path(&stage_record, "work_dir")?);
    let provenance_dir = resolve_artifact_path(config, output_path(&stage_record, "provenance_dir")?);
    let aggregate_path = provenance_dir.join("package_outputs.jsonl");
    let existing_package_records = read_package_outputs_by_region(&aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
        let render_inputs =
            csup_render_inputs(&stage_record.fingerprint, region, config.cpu_jobs.max(1), version_label)?;
        let render_prepared =
            prepare_node_at(&build_shared_node_dir(config, &render_node_name)?, &render_node_name, &render_inputs)?;
        let render_record =
            load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
        let node_name = format!("csup-package-{}", region.code().to_ascii_lowercase());
        let inputs = BTreeMap::from([
            ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
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
        let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
        let zip_path = work_dir.join(format!("{}_CSUP_{}.zip", region.code(), version_label));
        let manifest_path = work_dir.join(format!("{}_CSUP_{}.manifest", region.code(), version_label));
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
            let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
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
                ("zip".to_string(), relative_artifact_path(&zip_path, &config.build_root)),
                ("manifest".to_string(), relative_artifact_path(&manifest_path, &config.build_root)),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
    let plates_root = request.run_root.join(format!("work/tpp-{region_id}/plates"));
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&plates_root))? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = render_native_tpp(request)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), result.work_dir.display().to_string()),
        (
            "provenance_dir".to_string(),
            result.provenance_dir.display().to_string(),
        ),
        ("plates_root".to_string(), relative_artifact_path(&plates_root, &config.build_root)),
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
    run_root: &Path,
    source_urls_path: &Path,
    version_label: &str,
) -> anyhow::Result<(NodeRecord, AssetSource)> {
    let region_id = region.code().to_ascii_lowercase();
    let render_request = NativeTppRunRequest {
        region,
        source_repo: PathBuf::new(),
        run_root: run_root.to_path_buf(),
        prefetch_source_urls: Some(source_urls_path.to_path_buf()),
        fetch_jobs: config.fetch_jobs,
        render_jobs: config.cpu_jobs.max(1),
    };
    let render_node_name = format!("tpp-{region_id}-render");
    let render_inputs = tpp_render_inputs(&render_request, source_urls_path, &region_id)?;
    let render_prepared = prepare_node_at(
        &build_shared_node_dir(config, &render_node_name)?,
        &render_node_name,
        &render_inputs,
    )?;
    let render_record = load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
    let inputs = BTreeMap::from([
        ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
        ("region".to_string(), region.code().to_string()),
        ("version_label".to_string(), version_label.to_string()),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, &node_name)?, &node_name, &inputs)?;
    let package_outputs_path = run_root.join(format!("meta/provenance/tpp-{region_id}/package_outputs.jsonl"));
    let work_dir = run_root.join(format!("work/tpp-{region_id}"));
    let zip_path = work_dir.join(format!("{}_TPP_{}.zip", region.code(), version_label));
    let manifest_path = work_dir.join(format!("{}_TPP_{}.manifest", region.code(), version_label));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[package_outputs_path.clone(), zip_path.clone(), manifest_path.clone()],
    )? {
        NodeCacheState::CacheHit(record) => {
            return Ok((
                record,
                AssetSource {
                    package_outputs_path,
                    asset_root: work_dir.clone(),
                    package_root: work_dir.clone(),
                    source_urls_path: Some(source_urls_path.to_path_buf()),
                },
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let provenance_dir = run_root.join(format!("meta/provenance/tpp-{region_id}"));
    let result =
        package_native_tpp_versioned(&work_dir, &provenance_dir, region, version_label, version_label)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(&work_dir, &config.build_root)),
        ("package_outputs".to_string(), relative_artifact_path(&package_outputs_path, &config.build_root)),
        ("zip".to_string(), relative_artifact_path(&zip_path, &config.build_root)),
        ("manifest".to_string(), relative_artifact_path(&manifest_path, &config.build_root)),
        ("package_count".to_string(), result.package_count.to_string()),
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
            asset_root: work_dir.clone(),
            package_root: work_dir.clone(),
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

fn csup_stage_inputs(source_urls: &Path, fetch_jobs: usize) -> anyhow::Result<BTreeMap<String, String>> {
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
        ("stage_fingerprint".to_string(), stage_fingerprint.to_string()),
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
) -> anyhow::Result<Vec<NodeRecord>> {
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    let data_version = data_version_label(source_urls_dir)?;
    let data_manifest_version = data_manifest_cycle(source_urls_dir)?;
    let (staged_input_dir, staging_record) = build_data_input_node(config, &source_urls)?;

    let artifact_stem = data_version.clone();
    let inputs = BTreeMap::from([
        ("staged_input_dir".to_string(), relative_artifact_path(&staged_input_dir, &config.build_root)),
        (
            "staged_input_fingerprint".to_string(),
            staging_record.fingerprint.clone(),
        ),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        ("manifest_version".to_string(), data_manifest_version.clone()),
        ("artifact_stem".to_string(), artifact_stem.clone()),
    ]);
    let prepared = prepare_node_at(&build_shared_node_dir(config, "data")?, "data", &inputs)?;
    let provenance_dir = prepared.dir.join("meta/provenance/data");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;

    let request = DataBuildRequest {
        input_dir: staged_input_dir.clone(),
        output_dir: prepared.dir.join("output"),
        mode: DataBuildMode::Production,
        manifest_version: data_manifest_version.clone(),
        artifact_stem: Some(artifact_stem),
    };
    let manifest_path = request
        .output_dir
        .join(format!("{}.manifest", request.artifact_stem.as_deref().unwrap_or("databases")));
    let zip_path = request
        .output_dir
        .join(format!("{}.zip", request.artifact_stem.as_deref().unwrap_or("databases")));
    let _build_lock = match claim_or_wait_for_node(&prepared, &[manifest_path.clone(), zip_path.clone()])? {
        NodeCacheState::CacheHit(record) => return Ok(vec![staging_record, record]),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package(&request)?;
    let outputs = BTreeMap::from([
        ("main_db".to_string(), relative_artifact_path(&result.main_db, &config.build_root)),
        ("manifest".to_string(), relative_artifact_path(&result.manifest_path, &config.build_root)),
        ("zip".to_string(), relative_artifact_path(&result.zip_path, &config.build_root)),
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

fn build_vectors_node(
    config: &ProductBuildConfig,
    main_db: &Path,
    version_label: &str,
) -> anyhow::Result<NodeRecord> {
    let inputs = BTreeMap::from([
        ("main_db".to_string(), hash_file(main_db)?),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, "vectors")?, "vectors", &inputs)?;
    let output_dir = prepared.dir.join("output");
    let request = BuildVectorsRequest {
        main_db: main_db.to_path_buf(),
        output_dir: output_dir.clone(),
        version_label: version_label.to_string(),
    };
    let zip_path = output_dir.join(format!("vectors_{version_label}.zip"));
    let stats_path = output_dir.join("stats.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), stats_path.clone()])? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_vectors_dataset(&request)?;
    let outputs = BTreeMap::from([
        ("manifest".to_string(), relative_artifact_path(&result.manifest_path, &config.build_root)),
        ("stats".to_string(), relative_artifact_path(&result.stats_path, &config.build_root)),
        ("zip".to_string(), relative_artifact_path(&result.zip_path, &config.build_root)),
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
    let prepared = prepare_node_at(&build_shared_node_dir(config, "data-input-staging")?, "data-input-staging", &inputs)?;
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
        &provenance_dir,
        "data",
    )?;
    fs::write(&marker, b"ok")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        ("staged_input_dir".to_string(), relative_artifact_path(&staged_root, &config.build_root)),
        ("provenance_dir".to_string(), relative_artifact_path(&provenance_dir, &config.build_root)),
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

fn read_package_outputs_by_region(path: &Path) -> anyhow::Result<BTreeMap<String, PackageOutputRecord>> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
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
            chart: value.get("chart").and_then(|v| v.as_str()).map(ToOwned::to_owned),
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
    let record: NodeRecord = serde_json::from_slice(&bytes).context("failed to parse node record")?;
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
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock_if_needed(&prepared.lock_path)?;
                thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to acquire {}", prepared.lock_path.display()));
            }
        }
    }
}

fn reset_node_dir_for_rebuild(prepared: &PreparedNode) -> anyhow::Result<()> {
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
            fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
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
        Err(err) => Err(err).with_context(|| format!("failed to remove stale {}", lock_path.display())),
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
    let record = NodeRecord {
        name: prepared.name,
        fingerprint: prepared.fingerprint,
        started_at_utc,
        finished_at_utc,
        elapsed_ms,
        cache_hit,
        inputs,
        outputs,
    };
    fs::write(
        &prepared.record_path,
        serde_json::to_vec_pretty(&record).context("failed to encode node record")?,
    )
    .with_context(|| format!("failed to write {}", prepared.record_path.display()))?;
    Ok(record)
}

fn artifact_root_from_build_root(build_root: &Path) -> &Path {
    build_root
        .parent()
        .and_then(|value| value.parent())
        .unwrap_or(build_root)
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
    path.parent()
        .and_then(|value| value.parent())
        .and_then(|artifact_root| path.strip_prefix(artifact_root).ok())
        .map(|value| value.display().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn build_node_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = config.build_root.join("work").join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn build_shared_work_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = shared_build_root(config).join("work").join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn build_shared_node_dir(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = shared_build_root(config).join("nodes").join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn shared_build_root(config: &ProductBuildConfig) -> PathBuf {
    config
        .build_root
        .parent()
        .unwrap_or(&config.build_root)
        .join("shared")
}

fn load_existing_node_record(record_path: &Path, expected_name: &str) -> anyhow::Result<NodeRecord> {
    let bytes = fs::read(record_path)
        .with_context(|| format!("failed to read {}", record_path.display()))?;
    let record: NodeRecord = serde_json::from_slice(&bytes).context("failed to parse node record")?;
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
        hasher.update(fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) -> anyhow::Result<()> {
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
    Ok(hash_text(&serde_json::to_string(&value).context("fingerprint json")?))
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
        anyhow::bail!("failed to read RLIMIT_NOFILE: {}", std::io::Error::last_os_error());
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
        let file =
            File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self {
            start: Instant::now(),
            file,
        })
    }

    fn log(&mut self, message: impl AsRef<str>) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        writeln!(
            self.file,
            "{} {} {}",
            now,
            format_elapsed(self.start.elapsed().as_secs()),
            message.as_ref()
        )
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
    let source_urls = source_urls_dir.join(format!("charts-{}/source_urls.jsonl", family_slug(family)));
    let effective =
        find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
            .with_context(|| format!("missing chart effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn csup_version_label(source_urls_dir: &Path) -> anyhow::Result<String> {
    let source_urls = source_urls_dir.join("csup/source_urls.jsonl");
    let effective =
        find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
            .with_context(|| format!("missing csup effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn tpp_region_version_label(source_urls_dir: &Path, region: Region) -> anyhow::Result<String> {
    let region_id = region.code().to_ascii_lowercase();
    let source_urls = source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
    let effective =
        find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
            .with_context(|| format!("missing tpp effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn data_version_label(source_urls_dir: &Path) -> anyhow::Result<String> {
    Ok(format!("data_{}", data_manifest_cycle(source_urls_dir)?))
}

fn data_manifest_cycle(source_urls_dir: &Path) -> anyhow::Result<String> {
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    let effective =
        find_effective_date_from_urls(&read_source_urls_jsonl(&source_urls)?)
            .with_context(|| format!("missing data effective date in {}", source_urls.display()))?;
    cycle_code_from_effective_date(effective)
}

fn cycle_data_urls(urls: Vec<String>) -> Vec<String> {
    urls.into_iter()
        .filter(|url| !url.split('#').next().unwrap_or(url).ends_with("/DAILY_DOF_DAT.ZIP"))
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
                                &format!("20{}-{}-{}", &compact[0..2], &compact[2..4], &compact[4..6]),
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
    let first_date = first_cycle_day(year)
        .ok_or_else(|| anyhow::anyhow!("unsupported cycle year {year}"))?;
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
    use tempfile::tempdir;

    fn write_source_urls(root: &Path, relative: &str, lines: &[&str]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    #[test]
    fn derives_distinct_vintage_labels_from_source_urls() {
        let temp = tempdir().unwrap();
        write_source_urls(
            temp.path(),
            "charts-sec/source_urls.jsonl",
            &[r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/visual/03-19-2026/sectional-files/Seattle.zip"]}"#],
        );
        write_source_urls(
            temp.path(),
            "charts-enr-l/source_urls.jsonl",
            &[r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/enroute/03-19-2026/enr_l01.zip"]}"#],
        );
        write_source_urls(
            temp.path(),
            "csup/source_urls.jsonl",
            &[r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/Upload_313-d/supplements/DCS_20260319.zip"]}"#],
        );
        write_source_urls(
            temp.path(),
            "tpp-ne/source_urls.jsonl",
            &[r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/upload_313-d/terminal/DDTPPA_260416.zip"]}"#],
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
        assert_eq!(tpp_region_version_label(temp.path(), Region::Ne).unwrap(), "2604");
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
        assert_eq!(filtered, vec!["https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260416.zip"]);
    }

    #[test]
    fn versioned_work_roots_are_distinct_between_cycles() {
        let temp = tempdir().unwrap();
        let config = ProductBuildConfig {
            chart_cutline_root: temp.path().join("cutlines"),
            build_root: temp.path().join("product-builds/validation"),
            profile: ProductBuildProfile::Validation,
            target_cycle: None,
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: temp.path().join("cache/fetch"),
            fetch_cache_mode: "fill".to_string(),
        };

        let sec_2603 = build_shared_work_root(&config, "charts-sec-2603").unwrap();
        let sec_2605 = build_shared_work_root(&config, "charts-sec-2605").unwrap();
        let tpp_2604 = build_shared_work_root(&config, "tpp-ne-2604").unwrap();
        let tpp_2605 = build_shared_work_root(&config, "tpp-ne-2605").unwrap();
        let data_a = build_shared_work_root(&config, "data-data_2604").unwrap();
        let data_b = build_shared_work_root(&config, "data-data_2605").unwrap();

        assert_ne!(sec_2603, sec_2605);
        assert_ne!(tpp_2604, tpp_2605);
        assert_ne!(data_a, data_b);
        assert!(sec_2603.ends_with("shared/work/charts-sec-2603"));
        assert!(tpp_2604.ends_with("shared/work/tpp-ne-2604"));
        assert!(data_a.ends_with("shared/work/data-data_2604"));
    }
}
