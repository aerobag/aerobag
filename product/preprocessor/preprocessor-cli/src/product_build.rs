use std::{
    collections::{BTreeMap, VecDeque},
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
use preprocessor_resource_index::{write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource};
use preprocessor_tpp::{
    package_native_tpp_versioned, render_native_tpp, NativeTppRunRequest,
};
use preprocessor_vectors::{build_vectors_dataset, BuildVectorsRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::emit_source_urls::emit_source_urls;

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
struct ProductBuildManifest {
    schema_version: u32,
    profile: String,
    build_root: String,
    generated_at_utc: String,
    fetch_cache_root: String,
    fetch_cache_mode: String,
    nodes: Vec<NodeRecord>,
}

#[derive(Debug)]
struct PreparedNode {
    name: String,
    fingerprint: String,
    dir: PathBuf,
    record_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct PackageSummary {
    total: usize,
    cache_hits: usize,
    rebuilt: usize,
}

#[derive(Debug, Clone)]
enum HeavyJobSpec {
    ChartRender {
        family: ChartFamily,
        source_repo: PathBuf,
        run_root: PathBuf,
        prefetch_source_urls: PathBuf,
        fetch_jobs: usize,
        cpu_jobs: usize,
    },
    CsupRender {
        region: Region,
        source_repo: PathBuf,
        run_root: PathBuf,
        prefetch_source_urls: PathBuf,
        fetch_jobs: usize,
        render_jobs: usize,
    },
    Tpp {
        region: Region,
        source_repo: PathBuf,
        run_root: PathBuf,
        prefetch_source_urls: PathBuf,
        fetch_jobs: usize,
        render_jobs: usize,
    },
    Data {
        source_urls_dir: PathBuf,
    },
}

impl HeavyJobSpec {
    fn name(&self) -> String {
        match self {
            Self::ChartRender { family, .. } => format!("charts-{}-render", family_slug(*family)),
            Self::CsupRender { region, .. } => {
                format!("csup-render-{}", region.code().to_ascii_lowercase())
            }
            Self::Tpp { region, .. } => format!("tpp-{}", region.code().to_ascii_lowercase()),
            Self::Data { .. } => "data".to_string(),
        }
    }

    fn run(self, config: &ProductBuildConfig) -> anyhow::Result<Vec<NodeRecord>> {
        match self {
            Self::ChartRender {
                family,
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                cpu_jobs,
            } => Ok(vec![build_chart_render_node(
                config,
                family,
                &source_repo,
                &run_root,
                &prefetch_source_urls,
                fetch_jobs,
                cpu_jobs,
            )?]),
            Self::CsupRender {
                region,
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                render_jobs,
            } => Ok(vec![build_csup_render_node(
                config,
                region,
                &source_repo,
                &run_root,
                &prefetch_source_urls,
                fetch_jobs,
                render_jobs,
            )?]),
            Self::Tpp {
                region,
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                render_jobs,
            } => {
                let request = NativeTppRunRequest {
                    region,
                    source_repo,
                    run_root,
                    prefetch_source_urls: Some(prefetch_source_urls),
                    fetch_jobs,
                    render_jobs,
                };
                Ok(vec![build_tpp_render_node(config, &request)?])
            }
            Self::Data { source_urls_dir } => build_data_nodes(config, &source_urls_dir),
        }
    }
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
        "begin profile={} build_root={} max_heavy_jobs={} cpu_jobs={} fetch_jobs={} fetch_cache_mode={}",
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

        let mut pending_jobs = VecDeque::new();
        for family in [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ] {
            let family_id = family_slug(family).to_string();
            let version = chart_versions
                .get(&family_id)
                .expect("chart family version should exist");
            let run_root =
                build_shared_work_root(config, &format!("charts-{family_id}-{version}"))?;
            pending_jobs.push_back(HeavyJobSpec::ChartRender {
                family,
                source_repo: config.chart_cutline_root.clone(),
                run_root: run_root.clone(),
                prefetch_source_urls: source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
                fetch_jobs: config.fetch_jobs,
                cpu_jobs: config.cpu_jobs.min(8).max(1),
            });
        }

        let csup_run_root = build_shared_work_root(config, &format!("csup-{csup_version}"))?;
        let csup_stage_record = build_csup_stage_node(
            config,
            Path::new(""),
            &csup_run_root,
            &source_urls_dir.join("csup/source_urls.jsonl"),
            config.fetch_jobs,
        )?;
        node_records.push(normalize_node_record_paths(
            csup_stage_record.clone(),
            &config.build_root,
        ));
        for region in Region::ALL {
            pending_jobs.push_back(HeavyJobSpec::CsupRender {
                region,
                source_repo: PathBuf::new(),
                run_root: csup_run_root.clone(),
                prefetch_source_urls: source_urls_dir.join("csup/source_urls.jsonl"),
                fetch_jobs: config.fetch_jobs,
                render_jobs: config.cpu_jobs.max(1),
            });
        }

        let mut tpp_package_requests = Vec::new();
        for region in config.profile.tpp_regions() {
            let region_id = region.code().to_ascii_lowercase();
            let version = tpp_versions
                .get(&region_id)
                .expect("tpp region version should exist");
            let run_root = build_shared_work_root(config, &format!("tpp-{region_id}-{version}"))?;
            pending_jobs.push_back(HeavyJobSpec::Tpp {
                region: *region,
                source_repo: PathBuf::new(),
                run_root: run_root.clone(),
                prefetch_source_urls: source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                fetch_jobs: config.fetch_jobs,
                render_jobs: config.cpu_jobs.max(1),
            });
            tpp_package_requests.push((*region, run_root));
        }

        pending_jobs.push_back(HeavyJobSpec::Data {
            source_urls_dir: source_urls_dir.clone(),
        });

        let total_heavy_jobs = pending_jobs.len();
        let (tx, rx) = mpsc::channel();
        let mut running_jobs = 0_usize;
        let mut launched_jobs = 0_usize;
        while running_jobs > 0 || !pending_jobs.is_empty() {
            while running_jobs < config.max_heavy_jobs && !pending_jobs.is_empty() {
                let spec = pending_jobs.pop_front().expect("queue should be non-empty");
                let name = spec.name();
                launched_jobs += 1;
                master_log.log(format!(
                    "launch {name} progress={}/{}",
                    launched_jobs, total_heavy_jobs
                ))?;
                let tx = tx.clone();
                let config = config.clone();
                thread::spawn(move || {
                    let result = spec.run(&config);
                    let _ = tx.send((name, result));
                });
                running_jobs += 1;
            }

            if running_jobs == 0 {
                break;
            }

            let (name, result) = rx
                .recv()
                .context("heavy-job scheduler channel closed unexpectedly")?;
            running_jobs -= 1;
            match result {
                Ok(records) => {
                    for record in records {
                        node_records.push(normalize_node_record_paths(record, &config.build_root));
                    }
                    master_log.log(format!(
                        "complete {name} progress={}/{}",
                        launched_jobs, total_heavy_jobs
                    ))?;
                }
                Err(err) => {
                    master_log.log(format!("complete {name} FAIL error={}", err))?;
                    return Err(err);
                }
            }
        }

        let mut chart_sources = Vec::new();
        for family in [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ] {
            let started = Instant::now();
            master_log.log(format!(
                "launch charts-{}-package",
                family_slug(family)
            ))?;
            let (records, source) = build_chart_package_nodes(
                config,
                family,
                &source_urls_dir,
                chart_versions
                    .get(family_slug(family))
                    .expect("chart family version should exist"),
            )?;
            let summary = summarize_package_records(&records);
            for record in records {
                node_records.push(normalize_node_record_paths(record, &config.build_root));
            }
            master_log.log(format!(
                "complete charts-{}-package elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                family_slug(family),
                started.elapsed().as_millis(),
                summary.total,
                summary.cache_hits,
                summary.rebuilt,
            ))?;
            chart_sources.push(source);
        }

        let started = Instant::now();
        master_log.log("launch csup-package")?;
        let (csup_records, csup_source) =
            build_csup_package_nodes(config, &source_urls_dir, &csup_version)?;
        let summary = summarize_package_records(&csup_records);
        for record in csup_records {
            node_records.push(normalize_node_record_paths(record, &config.build_root));
        }
        master_log.log(format!(
            "complete csup-package elapsed_ms={} regions={} cache_hits={} rebuilt={}",
            started.elapsed().as_millis(),
            summary.total,
            summary.cache_hits,
            summary.rebuilt,
        ))?;
        let csup_sources = vec![csup_source];

        let mut tpp_sources = Vec::new();
        for (region, run_root) in tpp_package_requests {
            let region_id = region.code().to_ascii_lowercase();
            let started = Instant::now();
            master_log.log(format!("launch tpp-{}-package", region_id))?;
            let (record, source) = build_tpp_package_node(
                config,
                region,
                &run_root,
                &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                tpp_versions
                    .get(&region_id)
                    .expect("tpp region version should exist"),
            )?;
            let cache_hit = record.cache_hit;
            node_records.push(normalize_node_record_paths(record, &config.build_root));
            master_log.log(format!(
                "complete tpp-{}-package elapsed_ms={} cache_hit={}",
                region_id,
                started.elapsed().as_millis(),
                cache_hit,
            ))?;
            tpp_sources.push(source);
        }

        let data_root = build_shared_work_root(config, &format!("data-{data_version}"))?;
        let data_zip = data_root
            .join("output")
            .join(format!("{data_version}.zip"));
        if !data_zip.is_file() {
            bail!("missing data zip at {}", data_zip.display());
        }
        let data_main_db = data_root.join("output").join("main.db");
        if !data_main_db.is_file() {
            bail!("missing data main.db at {}", data_main_db.display());
        }

        master_log.log("launch vectors")?;
        let vectors_record = build_vectors_node(config, &data_main_db, &data_version)?;
        master_log.log(format!(
            "complete vectors cache_hit={}",
            vectors_record.cache_hit
        ))?;
        node_records.push(normalize_node_record_paths(vectors_record, &config.build_root));

        master_log.log("launch resource-index")?;
        let resource_index_record =
            build_resource_index_node(config, &data_zip, chart_sources, tpp_sources, csup_sources)?;
        master_log.log(format!(
            "complete resource-index cache_hit={}",
            resource_index_record.cache_hit
        ))?;
        node_records.push(normalize_node_record_paths(resource_index_record, &config.build_root));

        let manifest = ProductBuildManifest {
            schema_version: 1,
            profile: config.profile.as_str().to_string(),
            build_root: relative_product_build_path(&config.build_root),
            generated_at_utc: utc_now_string(),
            fetch_cache_root: relative_artifact_path(&config.fetch_cache_root, &config.build_root),
            fetch_cache_mode: config.fetch_cache_mode.clone(),
            nodes: node_records,
        };
        let manifest_path = config
            .build_root
            .join(format!("bundle_{bundle_cycle}.json"));
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest)
                .context("failed to encode product build manifest")?,
        )
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
        Ok(manifest_path)
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
        let artifact_root = default_artifact_root(&repo_root);

        let mut profile = ProductBuildProfile::Production;
        let mut chart_cutline_root = repo_root.join("avare-assets").join("chart-cutlines");
        let mut build_root = artifact_root.join("product-builds").join(profile.as_str());
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
    let emit_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/emit_source_urls.rs");
    let inputs = BTreeMap::from([("emit_source".to_string(), hash_file(&emit_source)?)]);
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
    if let Some(record) = try_load_node_record(&prepared, &expected)? {
        return Ok((output_dir, record));
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&output_dir)?;
    env::set_var("FETCH_CACHE_ROOT", &config.fetch_cache_root);
    env::set_var("FETCH_CACHE_MODE", &config.fetch_cache_mode);
    emit_source_urls(&output_dir)?;
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
    if let Some(record) = try_load_node_record(&prepared, &expected)? {
        return Ok((output_dir, record));
    }
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
    run_root: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
    cpu_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(family).to_string();
    let inputs = BTreeMap::from([
        ("family".to_string(), family_id.clone()),
        ("source_repo".to_string(), hash_tree(source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("cpu_jobs".to_string(), cpu_jobs.to_string()),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
    ]);
    let prepared = prepare_existing_node_root(&format!("charts-{family_id}-render"), run_root, &inputs)?;
    let tiles_root = run_root
        .join("work")
        .join(format!("charts-{family_id}"))
        .join("tiles");
    if let Some(record) = try_load_node_record(&prepared, &[tiles_root.clone()])? {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let work_dir = stage_work_dir(family, source_repo, run_root)?;
    let provenance_dir = run_root.join("meta").join("provenance").join(format!("charts-{family_id}"));
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
    _source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, ChartSource)> {
    let family_id = family_slug(family).to_string();
    let run_root = build_shared_work_root(config, &format!("charts-{family_id}-{version_label}"))?;
    let work_dir = run_root.join("work").join(format!("charts-{family_id}"));
    let aggregate_path = run_root
        .join("meta")
        .join("provenance")
        .join(format!("charts-{family_id}"))
        .join("package_outputs.jsonl");
    let source_urls_path = run_root
        .join("meta")
        .join("provenance")
        .join(format!("charts-{family_id}"))
        .join("source_urls.jsonl");
    let render_record = load_existing_node_record(
        &run_root.join("build-record.json"),
        &format!("charts-{family_id}-render"),
    )?;
    let existing_package_records = read_package_outputs_by_region(&aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let node_name = format!("charts-{family_id}-package-{}", region.code().to_ascii_lowercase());
        let package_root = build_shared_work_root(config, &format!("{node_name}-{version_label}"))?;
        let inputs = BTreeMap::from([
            ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
            ("region".to_string(), region.code().to_string()),
            ("version_label".to_string(), version_label.to_string()),
        ]);
        let prepared = prepare_existing_node_root(&node_name, &package_root, &inputs)?;
        let zip_path = work_dir.join(format!(
            "{}_{}_{}.zip",
            region.code(),
            manifest_chart_name(family),
            version_label
        ));
        let manifest_path = work_dir.join(format!(
            "{}_{}_{}",
            region.code(),
            manifest_chart_name(family),
            version_label
        ));
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
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
                    "{}_{}_{}",
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
    source_repo: &Path,
    run_root: &Path,
    _source_urls: &Path,
    _fetch_jobs: usize,
    render_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let stage_record = load_existing_node_record(&run_root.join("build-record.json"), "csup-stage")?;
    let version_label = csup_version_label_from_run_root(run_root)?;
    let node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
    let inputs = BTreeMap::from([
        ("stage_fingerprint".to_string(), stage_record.fingerprint),
        ("region".to_string(), region.code().to_string()),
        ("render_jobs".to_string(), render_jobs.to_string()),
        ("version_label".to_string(), version_label.clone()),
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
    ]);
    let node_root = build_shared_work_root(config, &format!("{node_name}-{version_label}"))?;
    let prepared = prepare_existing_node_root(&node_name, &node_root, &inputs)?;
    let marker = node_root.join(".render-complete");
    if let Some(record) = try_load_node_record(&prepared, std::slice::from_ref(&marker))? {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let work_dir = stage_work_dir_for_product(source_repo, run_root)?;
    render_csup_region(&work_dir, region, render_jobs)?;
    fs::write(&marker, b"ok")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(&work_dir, &config.build_root)),
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
    run_root: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let inputs = BTreeMap::from([
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
    ]);
    let prepared = prepare_existing_node_root("csup-stage", run_root, &inputs)?;
    let marker = run_root.join(".stage-complete");
    if let Some(record) = try_load_node_record(&prepared, std::slice::from_ref(&marker))? {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let work_dir = stage_work_dir_for_product(source_repo, run_root)?;
    let provenance_dir = run_root.join("meta").join("provenance").join("csup");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(source_urls, &provenance_dir)?;
    let urls = read_source_urls_jsonl(source_urls)?;
    prefetch_archives_with_provenance(&urls, &work_dir, fetch_jobs, &provenance_dir, "csup")?;
    prepare_csup_inputs(&work_dir)?;
    fs::write(&marker, b"ok")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), relative_artifact_path(&work_dir, &config.build_root)),
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
    _source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, AssetSource)> {
    let run_root = build_shared_work_root(config, &format!("csup-{version_label}"))?;
    let work_dir = run_root.join("work").join("csup");
    let aggregate_path = run_root.join("meta/provenance/csup/package_outputs.jsonl");
    let source_urls_path = run_root.join("meta/provenance/csup/source_urls.jsonl");
    let existing_package_records = read_package_outputs_by_region(&aggregate_path)?;
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
        let render_record = load_existing_node_record(
            &build_shared_work_root(config, &format!("{render_node_name}-{version_label}"))?
                .join("build-record.json"),
            &render_node_name,
        )?;
        let node_name = format!("csup-package-{}", region.code().to_ascii_lowercase());
        let package_root = build_shared_work_root(config, &format!("{node_name}-{version_label}"))?;
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
        let prepared = prepare_existing_node_root(&node_name, &package_root, &inputs)?;
        let zip_path = work_dir.join(format!("{}_CSUP_{}.zip", region.code(), version_label));
        let manifest_path = work_dir.join(format!("{}_CSUP_{}", region.code(), version_label));
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
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
                manifest: format!("{}_CSUP_{}", region.code(), version_label),
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
            asset_root: work_dir,
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
    if let Some(record) = try_load_node_record(&prepared, std::slice::from_ref(&plates_root))? {
        return Ok(record);
    }
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
    let manifest_path = work_dir.join(format!("{}_TPP_{}", region.code(), version_label));
    if let Some(record) = try_load_node_record(
        &prepared,
        &[package_outputs_path.clone(), zip_path.clone(), manifest_path.clone()],
    )? {
        return Ok((
            record,
            AssetSource {
                package_outputs_path,
                asset_root: work_dir,
                source_urls_path: Some(source_urls_path.to_path_buf()),
            },
        ));
    }
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
            asset_root: work_dir,
            source_urls_path: Some(source_urls_path.to_path_buf()),
        },
    ))
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

    let node_root = build_shared_work_root(config, &format!("data-{data_version}"))?;
    let provenance_dir = node_root.join("meta/provenance/data");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;

    let request = DataBuildRequest {
        input_dir: staged_input_dir.clone(),
        output_dir: node_root.join("output"),
        mode: DataBuildMode::Production,
        manifest_version: data_manifest_version.clone(),
        artifact_stem: Some(data_version.clone()),
    };
    let inputs = BTreeMap::from([
        ("staged_input_dir".to_string(), relative_artifact_path(&staged_input_dir, &config.build_root)),
        (
            "staged_input_fingerprint".to_string(),
            staging_record.fingerprint.clone(),
        ),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        ("manifest_version".to_string(), request.manifest_version.clone()),
        ("artifact_stem".to_string(), request.artifact_stem.clone().unwrap_or_default()),
    ]);
    let prepared = prepare_existing_node_root("data", &node_root, &inputs)?;
    let zip_path = request
        .output_dir
        .join(format!("{}.zip", request.artifact_stem.as_deref().unwrap_or("databases")));
    if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone()])? {
        return Ok(vec![staging_record, record]);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package(&request)?;
    let outputs = BTreeMap::from([
        ("main_db".to_string(), relative_artifact_path(&result.main_db, &config.build_root)),
        ("manifest".to_string(), result.manifest_path.display().to_string()),
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
    let node_root = build_shared_work_root(config, &format!("vectors-{version_label}"))?;
    let output_dir = node_root.join("output");
    let request = BuildVectorsRequest {
        main_db: main_db.to_path_buf(),
        output_dir: output_dir.clone(),
        version_label: version_label.to_string(),
    };
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
    let prepared = prepare_existing_node_root("vectors", &node_root, &inputs)?;
    let zip_path = output_dir.join(format!("vectors_{version_label}.zip"));
    let stats_path = output_dir.join("stats.json");
    if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), stats_path.clone()])? {
        return Ok(record);
    }
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
    if let Some(record) = try_load_node_record(&prepared, std::slice::from_ref(&marker))? {
        return Ok((staged_root, record));
    }

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
                "{}:{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display(),
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
                "{}:{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display(),
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

fn prepare_existing_node_root(
    name: &str,
    root: &Path,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let fingerprint = fingerprint_for_node(name, inputs)?;
    let record_path = root.join("build-record.json");
    if record_path.is_file() {
        let existing = load_existing_node_record(&record_path, name)?;
        if existing.fingerprint != fingerprint {
            bail!(
                "immutable output collision for node {name} at {}: existing fingerprint {} != new fingerprint {}; rename/version this output root instead of reusing it",
                root.display(),
                existing.fingerprint,
                fingerprint
            );
        }
    }
    Ok(PreparedNode {
        name: name.to_string(),
        fingerprint,
        record_path,
        dir: root.to_path_buf(),
    })
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

fn relative_artifact_path(path: &Path, build_root: &Path) -> String {
    path.strip_prefix(artifact_root_from_build_root(build_root))
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

pub fn maybe_reexec_build_cycle_under_cgroup(args: &[String]) -> anyhow::Result<bool> {
    if env::var_os(PRODUCT_BUILD_CGROUP_ACTIVE_ENV).is_some() {
        return Ok(false);
    }
    if !command_exists("systemd-run") {
        return Ok(false);
    }
    let memory_max = env::var("PRODUCT_BUILD_MEMORY_MAX")
        .unwrap_or_else(|_| DEFAULT_PRODUCT_BUILD_MEMORY_MAX.to_string());
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let status = Command::new("systemd-run")
        .args(["--quiet", "--wait", "--collect"])
        .args(["-p", &format!("MemoryMax={memory_max}")])
        .args(["-p", "MemorySwapMax=0"])
        .args(["-p", "OOMPolicy=kill"])
        .arg("env")
        .arg(format!("{PRODUCT_BUILD_CGROUP_ACTIVE_ENV}=1"))
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
        writeln!(
            self.file,
            "{} {}",
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

pub(crate) fn default_artifact_root(repo_root: &Path) -> PathBuf {
    if let Some(path) = env_path("AEROBAG_ARTIFACT_ROOT") {
        return if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        };
    }
    {
        let config_path = repo_root.join(".aerobag-artifact-root");
        let raw = fs::read_to_string(&config_path).unwrap_or_else(|error| {
            panic!(
                "artifact root config missing at {} and AEROBAG_ARTIFACT_ROOT is unset: {error}",
                config_path.display()
            )
        });
        let configured = raw.trim();
        assert!(
            !configured.is_empty(),
            "artifact root config at {} is empty",
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

fn csup_version_label_from_run_root(run_root: &Path) -> anyhow::Result<String> {
    let name = run_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid csup run root {}", run_root.display()))?;
    name.strip_prefix("csup-")
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("csup run root {} missing version suffix", run_root.display()))
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
