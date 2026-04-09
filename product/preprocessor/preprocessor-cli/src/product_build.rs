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
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, package_family_region, stage_work_dir,
};
use preprocessor_core::{ChartFamily, Region};
use preprocessor_csup::{
    package_csup_region, prepare_csup_inputs, render_csup_region, stage_work_dir_for_product,
};
use preprocessor_data::{build_data_package, DataBuildRequest};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    read_source_urls_jsonl, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_resource_index::{write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource};
use preprocessor_tpp::{
    package_native_tpp, render_native_tpp, NativeTppRunRequest,
};
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
    lines.push("  resource-index".to_string());
    Ok(lines.join("\n") + "\n")
}

pub fn build_product(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
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

        let mut pending_jobs = VecDeque::new();
        for family in [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ] {
            let family_id = family_slug(family).to_string();
            let run_root = build_shared_work_root(config, &format!("charts-{family_id}"))?;
            pending_jobs.push_back(HeavyJobSpec::ChartRender {
                family,
                source_repo: config.chart_cutline_root.clone(),
                run_root: run_root.clone(),
                prefetch_source_urls: source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
                fetch_jobs: config.fetch_jobs,
                cpu_jobs: config.cpu_jobs.min(8).max(1),
            });
        }

        let csup_run_root = build_shared_work_root(config, "csup")?;
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
            let run_root = build_shared_work_root(config, &format!("tpp-{region_id}"))?;
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
            master_log.log(format!(
                "launch charts-{}-package",
                family_slug(family)
            ))?;
            let (records, source) = build_chart_package_nodes(config, family, &source_urls_dir)?;
            for record in records {
                node_records.push(normalize_node_record_paths(record, &config.build_root));
            }
            master_log.log(format!(
                "complete charts-{}-package",
                family_slug(family)
            ))?;
            chart_sources.push(source);
        }

        master_log.log("launch csup-package")?;
        let (csup_records, csup_source) = build_csup_package_nodes(config, &source_urls_dir)?;
        for record in csup_records {
            node_records.push(normalize_node_record_paths(record, &config.build_root));
        }
        master_log.log("complete csup-package")?;
        let csup_sources = vec![csup_source];

        let mut tpp_sources = Vec::new();
        for (region, run_root) in tpp_package_requests {
            let region_id = region.code().to_ascii_lowercase();
            master_log.log(format!("launch tpp-{}-package", region_id))?;
            let (record, source) = build_tpp_package_node(
                config,
                region,
                &run_root,
                &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
            )?;
            node_records.push(normalize_node_record_paths(record, &config.build_root));
            master_log.log(format!("complete tpp-{}-package", region_id))?;
            tpp_sources.push(source);
        }

        let data_zip = build_shared_work_root(config, "data")?
            .join("output")
            .join("databases.zip");
        if !data_zip.is_file() {
            bail!("missing data zip at {}", data_zip.display());
        }

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
        let manifest_path = config.build_root.join("product-build.json");
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
                other => bail!("unknown build-product argument: {other}"),
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
) -> anyhow::Result<(Vec<NodeRecord>, ChartSource)> {
    let family_id = family_slug(family).to_string();
    let run_root = build_shared_work_root(config, &format!("charts-{family_id}"))?;
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
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let node_name = format!("charts-{family_id}-package-{}", region.code().to_ascii_lowercase());
        let package_root = build_shared_work_root(config, &node_name)?;
        let inputs = BTreeMap::from([
            ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
            ("region".to_string(), region.code().to_string()),
        ]);
        let prepared = prepare_existing_node_root(&node_name, &package_root, &inputs)?;
        let zip_path = work_dir.join(format!("{}_{}.zip", region.code(), manifest_chart_name(family)));
        let manifest_path = work_dir.join(format!("{}_{}", region.code(), manifest_chart_name(family)));
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            let package_record = package_family_region(family, &work_dir, region)?;
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
        package_records.push(PackageOutputRecord {
            label: family.capture_label().to_string(),
            chart: Some(manifest_chart_name(family).to_string()),
            region: region.code().to_string(),
            manifest: format!("{}_{}", region.code(), manifest_chart_name(family)),
            manifest_sha256: hash_file(&manifest_path)?,
            zip: format!("{}_{}.zip", region.code(), manifest_chart_name(family)),
            zip_sha256: hash_file(&zip_path)?,
        });
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
    let node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
    let inputs = BTreeMap::from([
        ("stage_fingerprint".to_string(), stage_record.fingerprint),
        ("region".to_string(), region.code().to_string()),
        ("render_jobs".to_string(), render_jobs.to_string()),
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
    let node_root = build_shared_work_root(config, &node_name)?;
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
) -> anyhow::Result<(Vec<NodeRecord>, AssetSource)> {
    let run_root = build_shared_work_root(config, "csup")?;
    let work_dir = run_root.join("work").join("csup");
    let aggregate_path = run_root.join("meta/provenance/csup/package_outputs.jsonl");
    let source_urls_path = run_root.join("meta/provenance/csup/source_urls.jsonl");
    let mut node_records = Vec::new();
    let mut package_records = Vec::new();
    for region in Region::ALL {
        let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
        let render_record = load_existing_node_record(
            &build_shared_work_root(config, &render_node_name)?.join("build-record.json"),
            &render_node_name,
        )?;
        let node_name = format!("csup-package-{}", region.code().to_ascii_lowercase());
        let package_root = build_shared_work_root(config, &node_name)?;
        let inputs = BTreeMap::from([
            ("render_fingerprint".to_string(), render_record.fingerprint.clone()),
            ("region".to_string(), region.code().to_string()),
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
        let zip_path = work_dir.join(format!("{}_CSUP.zip", region.code()));
        let manifest_path = work_dir.join(format!("{}_CSUP", region.code()));
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            node_records.push(record);
        } else {
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            let package_record = package_csup_region(&work_dir, region)?;
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
        package_records.push(PackageOutputRecord {
            label: "csup".to_string(),
            chart: None,
            region: region.code().to_string(),
            manifest: format!("{}_CSUP", region.code()),
            manifest_sha256: hash_file(&manifest_path)?,
            zip: format!("{}_CSUP.zip", region.code()),
            zip_sha256: hash_file(&zip_path)?,
        });
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
    let zip_path = work_dir.join(format!("{}_TPP.zip", region.code()));
    let manifest_path = work_dir.join(format!("{}_TPP", region.code()));
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
    let result = package_native_tpp(&work_dir, &provenance_dir, region)?;
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
    let (staged_input_dir, staging_record) = build_data_input_node(config, &source_urls)?;

    let node_root = build_shared_work_root(config, "data")?;
    let provenance_dir = node_root.join("meta/provenance/data");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;

    let request = DataBuildRequest {
        input_dir: staged_input_dir.clone(),
        output_dir: node_root.join("output"),
        manifest_version: current_data_manifest_cycle(),
    };
    let inputs = BTreeMap::from([
        ("staged_input_dir".to_string(), relative_artifact_path(&staged_input_dir, &config.build_root)),
        (
            "staged_input_fingerprint".to_string(),
            staging_record.fingerprint.clone(),
        ),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        ("manifest_version".to_string(), request.manifest_version.clone()),
    ]);
    let prepared = prepare_existing_node_root("data", &node_root, &inputs)?;
    let zip_path = request.output_dir.join("databases.zip");
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

fn build_data_input_node(
    config: &ProductBuildConfig,
    source_urls: &Path,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    let inputs = BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
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
    let urls = read_source_urls_jsonl(source_urls)?;
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
    let prepared = prepare_existing_node_root("resource-index", &node_root, &inputs)?;
    let output_path = node_root.join("resource-index.json");
    if let Some(record) = try_load_node_record(&prepared, &[output_path.clone()])? {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let request = BuildResourceIndexRequest {
        nav_db_zip: nav_db_zip.to_path_buf(),
        output_path: output_path.clone(),
        chart_sources,
        tpp_sources,
        csup_sources,
    };
    write_resource_index(&request)?;
    let outputs = BTreeMap::from([("resource_index".to_string(), relative_artifact_path(&output_path, &config.build_root))]);
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

fn prepare_existing_node_root(
    name: &str,
    root: &Path,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let fingerprint = fingerprint_for_node(name, inputs)?;
    Ok(PreparedNode {
        name: name.to_string(),
        fingerprint,
        record_path: root.join("build-record.json"),
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)
                .with_context(|| format!("failed to copy {} to {}", src_path.display(), dst_path.display()))?;
        }
    }
    Ok(())
}

fn utc_now_string() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn maybe_reexec_build_product_under_cgroup(args: &[String]) -> anyhow::Result<bool> {
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
        .arg("build-product")
        .args(args)
        .status()
        .context("failed to re-exec product build under systemd-run")?;
    let exit_code = status.code().unwrap_or(1);
    if exit_code == 0 {
        return Ok(true);
    }
    bail!("product build cgroup wrapper exited with code {exit_code}");
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

fn default_artifact_root(repo_root: &Path) -> PathBuf {
    env_path("AEROBAG_ARTIFACT_ROOT").unwrap_or_else(|| {
        repo_root
            .parent()
            .map(|parent| parent.join("aerobag-artifacts"))
            .unwrap_or_else(|| repo_root.join("artifacts"))
    })
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn default_cpu_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(8)
}

fn calculate_cycle(future: i64, now: DateTime<Utc>) -> (u32, u32) {
    let mut start_utc = Utc.with_ymd_and_hms(2020, 1, 2, 9, 0, 0).unwrap();
    let mut cycle = 1_u32;
    let mut last_year = 2019_i32;
    let mut combined = 2001_u32;
    let mut is56 = true;
    let now_utc = now + Duration::days(28 * future);

    while start_utc < now_utc {
        if last_year != start_utc.year() {
            cycle = 1;
            last_year = start_utc.year();
        } else {
            cycle += 1;
        }
        combined = ((start_utc.year() % 2000) as u32) * 100 + cycle;
        is56 = !is56;
        start_utc += Duration::days(28);
    }

    if is56 {
        (combined, combined)
    } else {
        let (_, previous_56) = calculate_cycle(future - 1, now);
        (combined, previous_56)
    }
}

fn current_data_manifest_cycle() -> String {
    let (cycle, _) = calculate_cycle(1, Utc::now());
    cycle.to_string()
}

fn family_slug(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "sec",
        ChartFamily::Tac => "tac",
        ChartFamily::EnrL => "enr-l",
        ChartFamily::EnrH => "enr-h",
    }
}
