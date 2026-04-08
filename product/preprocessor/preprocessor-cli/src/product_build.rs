use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use preprocessor_charts::{run_native_family, NativeChartRunRequest};
use preprocessor_core::{ChartFamily, Region};
use preprocessor_csup::{run_native_csup, NativeCsupRunRequest};
use preprocessor_data::{build_data_package, DataBuildRequest};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance, read_source_urls_jsonl,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource,
};
use preprocessor_tpp::{run_native_tpp, NativeTppRunRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pub repo_root: PathBuf,
    pub source_root: PathBuf,
    pub build_root: PathBuf,
    pub profile: ProductBuildProfile,
    pub fetch_jobs: usize,
    pub cpu_jobs: usize,
    pub fetch_cache_root: PathBuf,
    pub fetch_cache_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeRecord {
    name: String,
    fingerprint: String,
    started_at_utc: String,
    finished_at_utc: String,
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

pub fn explain_product_build(config: &ProductBuildConfig) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("profile {}", config.profile.as_str()));
    lines.push(format!("build_root {}", config.build_root.display()));
    lines.push(format!("source_root {}", config.source_root.display()));
    lines.push(format!("fetch_cache_root {}", config.fetch_cache_root.display()));
    lines.push(format!("fetch_cache_mode {}", config.fetch_cache_mode));
    lines.push("nodes".to_string());
    lines.push("  source-urls".to_string());
    for family in ["sec", "tac", "enr-l", "enr-h"] {
        lines.push(format!("  charts-{family}"));
    }
    lines.push("  csup".to_string());
    for region in config.profile.tpp_regions() {
        lines.push(format!("  tpp-{}", region.code().to_ascii_lowercase()));
    }
    lines.push("  data".to_string());
    lines.push("  resource-index".to_string());
    Ok(lines.join("\n") + "\n")
}

pub fn build_product(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;

    let mut node_records = Vec::new();
    let source_urls_dir = build_source_urls_node(config, &mut node_records)?;

    let mut chart_sources = Vec::new();
    for family in [
        ChartFamily::Sec,
        ChartFamily::Tac,
        ChartFamily::EnrL,
        ChartFamily::EnrH,
    ] {
        let family_id = family_slug(family).to_string();
        let run_root = build_node_root(config, &format!("charts-{family_id}"))?;
        let request = NativeChartRunRequest {
            family,
            source_repo: config.source_root.join("charts"),
            run_root: run_root.clone(),
            cpu_jobs: config.cpu_jobs.min(8).max(1),
            prefetch_source_urls: Some(source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl"))),
            fetch_jobs: config.fetch_jobs,
        };
        let record = build_chart_node(config, &request)?;
        node_records.push(record);
        chart_sources.push(ChartSource {
            family_id,
            package_outputs_path: run_root
                .join("meta")
                .join("provenance")
                .join(format!("charts-{}", family_slug(family)))
                .join("package_outputs.jsonl"),
            package_root: run_root.join("work").join(format!("charts-{}", family_slug(family))),
        });
    }

    let csup_run_root = build_node_root(config, "csup")?;
    let csup_request = NativeCsupRunRequest {
        source_repo: config.source_root.join("csup"),
        run_root: csup_run_root.clone(),
        prefetch_source_urls: Some(source_urls_dir.join("csup/source_urls.jsonl")),
        fetch_jobs: config.fetch_jobs,
    };
    let csup_record = build_csup_node(config, &csup_request)?;
    node_records.push(csup_record);
    let csup_sources = vec![AssetSource {
        package_outputs_path: csup_run_root.join("meta/provenance/csup/package_outputs.jsonl"),
        asset_root: csup_run_root.join("work/csup"),
    }];

    let mut tpp_sources = Vec::new();
    for region in config.profile.tpp_regions() {
        let region_id = region.code().to_ascii_lowercase();
        let run_root = build_node_root(config, &format!("tpp-{region_id}"))?;
        let request = NativeTppRunRequest {
            region: *region,
            source_repo: config.source_root.join("tpp"),
            run_root: run_root.clone(),
            prefetch_source_urls: Some(source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"))),
            fetch_jobs: config.fetch_jobs,
        };
        let record = build_tpp_node(config, &request)?;
        node_records.push(record);
        tpp_sources.push(AssetSource {
            package_outputs_path: run_root.join(format!("meta/provenance/tpp-{region_id}/package_outputs.jsonl")),
            asset_root: run_root.join(format!("work/tpp-{region_id}")),
        });
    }

    let data_record = build_data_node(config, &source_urls_dir)?;
    let data_zip = PathBuf::from(
        data_record
            .outputs
            .get("zip")
            .cloned()
            .context("data node did not record zip output")?,
    );
    node_records.push(data_record);

    let resource_index_record =
        build_resource_index_node(config, &data_zip, chart_sources, tpp_sources, csup_sources)?;
    node_records.push(resource_index_record);

    let manifest = ProductBuildManifest {
        schema_version: 1,
        profile: config.profile.as_str().to_string(),
        build_root: config.build_root.display().to_string(),
        generated_at_utc: utc_now_string(),
        fetch_cache_root: config.fetch_cache_root.display().to_string(),
        fetch_cache_mode: config.fetch_cache_mode.clone(),
        nodes: node_records,
    };
    let manifest_path = config.build_root.join("product-build.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).context("failed to encode product build manifest")?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(manifest_path)
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

        let mut profile = ProductBuildProfile::Production;
        let mut source_root = repo_root.join("avare-source");
        let mut build_root = repo_root.join("product-builds").join(profile.as_str());
        let mut fetch_jobs = env_usize("FETCH_JOBS").unwrap_or(4);
        let mut cpu_jobs = env_usize("CPU_JOBS").unwrap_or_else(default_cpu_jobs);
        let fetch_cache_root =
            env_path("FETCH_CACHE_ROOT").unwrap_or_else(|| repo_root.join("cache").join("fetch"));
        let fetch_cache_mode = env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string());

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--profile" => {
                    let value = args.get(index + 1).context("missing value for --profile")?;
                    profile = ProductBuildProfile::parse(value)
                        .ok_or_else(|| anyhow::anyhow!("unsupported profile: {value}"))?;
                    build_root = repo_root.join("product-builds").join(profile.as_str());
                    index += 2;
                }
                "--source-root" => {
                    source_root = PathBuf::from(args.get(index + 1).context("missing value for --source-root")?);
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
                other => bail!("unknown build-product argument: {other}"),
            }
        }

        Ok(Self {
            repo_root,
            source_root,
            build_root,
            profile,
            fetch_jobs,
            cpu_jobs,
            fetch_cache_root,
            fetch_cache_mode,
        })
    }
}

fn build_source_urls_node(
    config: &ProductBuildConfig,
    node_records: &mut Vec<NodeRecord>,
) -> anyhow::Result<PathBuf> {
    let emit_script = config.repo_root.join("legacy-capture/emit_source_urls.py");
    let inputs = BTreeMap::from([
        ("emit_script".to_string(), hash_file(&emit_script)?),
        ("charts_cycle".to_string(), hash_file(&config.source_root.join("charts/cycle.py"))?),
        ("csup_cycle".to_string(), hash_file(&config.source_root.join("csup/cycle.py"))?),
        ("tpp_cycle".to_string(), hash_file(&config.source_root.join("tpp/cycle.py"))?),
        ("data_cycle".to_string(), hash_file(&config.source_root.join("data/cycle.py"))?),
    ]);
    let prepared = prepare_node(config, "source-urls", &inputs)?;
    let output_dir = prepared.dir.join("out");
    let expected = vec![
        output_dir.join("charts-sec/source_urls.jsonl"),
        output_dir.join("charts-tac/source_urls.jsonl"),
        output_dir.join("charts-enr-l/source_urls.jsonl"),
        output_dir.join("charts-enr-h/source_urls.jsonl"),
        output_dir.join("csup/source_urls.jsonl"),
        output_dir.join("tpp-ne/source_urls.jsonl"),
        output_dir.join("tpp-nw/source_urls.jsonl"),
        output_dir.join("data/source_urls.jsonl"),
    ];
    if let Some(record) = try_load_node_record(&prepared, &expected)? {
        node_records.push(record);
        return Ok(output_dir);
    }
    fs::create_dir_all(&output_dir)?;
    let output = Command::new("python3")
        .env("FETCH_CACHE_ROOT", &config.fetch_cache_root)
        .env("FETCH_CACHE_MODE", &config.fetch_cache_mode)
        .arg(&emit_script)
        .args(["--avare-source-root", &config.source_root.display().to_string()])
        .args(["--output-dir", &output_dir.display().to_string()])
        .output()
        .with_context(|| format!("failed to run {}", emit_script.display()))?;
    if !output.status.success() {
        bail!(
            "emit_source_urls.py failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let outputs = BTreeMap::from([("output_dir".to_string(), output_dir.display().to_string())]);
    let record = write_node_record(prepared, inputs, outputs, false)?;
    node_records.push(record);
    Ok(output_dir)
}

fn build_chart_node(
    _config: &ProductBuildConfig,
    request: &NativeChartRunRequest,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(request.family).to_string();
    let source_urls = request
        .prefetch_source_urls
        .as_ref()
        .context("chart build requires source urls")?;
    let inputs = BTreeMap::from([
        ("family".to_string(), family_id.clone()),
        ("source_repo".to_string(), hash_tree(&request.source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("cpu_jobs".to_string(), request.cpu_jobs.to_string()),
        ("fetch_jobs".to_string(), request.fetch_jobs.to_string()),
    ]);
    let prepared = prepare_existing_node_root(
        &format!("charts-{family_id}"),
        &request.run_root,
        &inputs,
    )?;
    let package_outputs = request
        .run_root
        .join("meta")
        .join("provenance")
        .join(format!("charts-{family_id}"))
        .join("package_outputs.jsonl");
    if let Some(record) = try_load_node_record(&prepared, &[package_outputs.clone()])? {
        return Ok(record);
    }
    let result = run_native_family(request)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), result.work_dir.display().to_string()),
        ("package_outputs".to_string(), package_outputs.display().to_string()),
    ]);
    write_node_record(prepared, inputs, outputs, false)
}

fn build_csup_node(
    _config: &ProductBuildConfig,
    request: &NativeCsupRunRequest,
) -> anyhow::Result<NodeRecord> {
    let source_urls = request
        .prefetch_source_urls
        .as_ref()
        .context("csup build requires source urls")?;
    let inputs = BTreeMap::from([
        ("source_repo".to_string(), hash_tree(&request.source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), request.fetch_jobs.to_string()),
    ]);
    let prepared = prepare_existing_node_root("csup", &request.run_root, &inputs)?;
    let package_outputs = request
        .run_root
        .join("meta/provenance/csup/package_outputs.jsonl");
    if let Some(record) = try_load_node_record(&prepared, &[package_outputs.clone()])? {
        return Ok(record);
    }
    let result = run_native_csup(request)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), result.work_dir.display().to_string()),
        ("package_outputs".to_string(), package_outputs.display().to_string()),
    ]);
    write_node_record(prepared, inputs, outputs, false)
}

fn build_tpp_node(
    _config: &ProductBuildConfig,
    request: &NativeTppRunRequest,
) -> anyhow::Result<NodeRecord> {
    let region_id = request.region.code().to_ascii_lowercase();
    let source_urls = request
        .prefetch_source_urls
        .as_ref()
        .context("tpp build requires source urls")?;
    let inputs = BTreeMap::from([
        ("region".to_string(), region_id.clone()),
        ("source_repo".to_string(), hash_tree(&request.source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), request.fetch_jobs.to_string()),
    ]);
    let prepared = prepare_existing_node_root(&format!("tpp-{region_id}"), &request.run_root, &inputs)?;
    let package_outputs = request
        .run_root
        .join(format!("meta/provenance/tpp-{region_id}/package_outputs.jsonl"));
    if let Some(record) = try_load_node_record(&prepared, &[package_outputs.clone()])? {
        return Ok(record);
    }
    let result = run_native_tpp(request)?;
    let outputs = BTreeMap::from([
        ("work_dir".to_string(), result.work_dir.display().to_string()),
        ("package_outputs".to_string(), package_outputs.display().to_string()),
    ]);
    write_node_record(prepared, inputs, outputs, false)
}

fn build_data_node(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
) -> anyhow::Result<NodeRecord> {
    let node_root = build_node_root(config, "data")?;
    let input_dir = node_root.join("input");
    if !input_dir.exists() {
        copy_dir_recursive(&config.source_root.join("data"), &input_dir)?;
    }
    let provenance_dir = node_root.join("meta/provenance/data");
    fs::create_dir_all(&provenance_dir)?;
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;
    let urls = read_source_urls_jsonl(&source_urls)?;
    prefetch_archives_with_provenance(
        &urls,
        &input_dir,
        config.fetch_jobs,
        &provenance_dir,
        "data",
    )?;

    let request = DataBuildRequest {
        input_dir: input_dir.clone(),
        output_dir: node_root.join("output"),
        manifest_version: current_data_manifest_cycle(),
    };
    let inputs = BTreeMap::from([
        ("source_repo".to_string(), hash_tree(&config.source_root.join("data"))?),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        ("input_dir".to_string(), hash_tree(&input_dir)?),
        ("manifest_version".to_string(), request.manifest_version.clone()),
    ]);
    let prepared = prepare_existing_node_root("data", &node_root, &inputs)?;
    let zip_path = request.output_dir.join("databases.zip");
    if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone()])? {
        return Ok(record);
    }
    let result = build_data_package(&request)?;
    let outputs = BTreeMap::from([
        ("main_db".to_string(), result.main_db.display().to_string()),
        ("manifest".to_string(), result.manifest_path.display().to_string()),
        ("zip".to_string(), result.zip_path.display().to_string()),
    ]);
    write_node_record(prepared, inputs, outputs, false)
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
                "{}:{}:{}",
                source.family_id,
                source.package_outputs_path.display(),
                source.package_root.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tpp_json = tpp_sources
        .iter()
        .map(|source| {
            format!(
                "{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let csup_json = csup_sources
        .iter()
        .map(|source| {
            format!(
                "{}:{}",
                source.package_outputs_path.display(),
                source.asset_root.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let inputs = BTreeMap::from([
        ("nav_db_zip".to_string(), hash_file(nav_db_zip)?),
        ("chart_sources".to_string(), hash_text(&chart_json)),
        ("tpp_sources".to_string(), hash_text(&tpp_json)),
        ("csup_sources".to_string(), hash_text(&csup_json)),
    ]);
    let prepared = prepare_existing_node_root("resource-index", &node_root, &inputs)?;
    let output_path = node_root.join("resource-index.json");
    if let Some(record) = try_load_node_record(&prepared, &[output_path.clone()])? {
        return Ok(record);
    }
    let request = BuildResourceIndexRequest {
        nav_db_zip: nav_db_zip.to_path_buf(),
        output_path: output_path.clone(),
        chart_sources,
        tpp_sources,
        csup_sources,
    };
    write_resource_index(&request)?;
    let outputs = BTreeMap::from([("resource_index".to_string(), output_path.display().to_string())]);
    write_node_record(prepared, inputs, outputs, false)
}

fn prepare_node(
    config: &ProductBuildConfig,
    name: &str,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let fingerprint = fingerprint_for_node(name, inputs)?;
    let dir = config.build_root.join("nodes").join(name).join(&fingerprint);
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

fn write_node_record(
    prepared: PreparedNode,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    cache_hit: bool,
) -> anyhow::Result<NodeRecord> {
    let record = NodeRecord {
        name: prepared.name,
        fingerprint: prepared.fingerprint,
        started_at_utc: utc_now_string(),
        finished_at_utc: utc_now_string(),
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

fn build_node_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = config.build_root.join("work").join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
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

fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
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
