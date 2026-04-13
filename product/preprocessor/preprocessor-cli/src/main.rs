use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use chrono::{Duration, NaiveDate, Utc};
mod emit_source_urls;
mod product_build;
use crate::emit_source_urls::{cycle_effective_date, discover_published_cycles};
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, likely_current_bottleneck, package_family_regions,
    phase_plan, run_family, run_native_family, ChartRunRequest, NativeChartRunRequest,
};
use preprocessor_core::{
    CaptureManifest, ChartFamily, ConcurrencyConfig, ExpectedTileCounts, Parallelism, Region,
    WorkKind,
};
use preprocessor_csup::{run_native_csup, NativeCsupRunRequest};
use preprocessor_data::{build_data_package, compare_databases, DataBuildMode, DataBuildRequest};
use preprocessor_fetch::{
    hash_text, manifest_path_for_run, manifest_summary, prefetch_archives_with_provenance, read_download_records,
    read_extract_records, read_source_url_set, CacheLayout, FetchCacheConfig, FetchCacheMode,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource,
};
use preprocessor_tools::{comparison_targets, ToolInvocation};
use preprocessor_tpp::{run_native_tpp, NativeTppRunRequest};
use preprocessor_vectors::{
    build_obstacle_dataset, build_vectors_dataset, BuildObstacleDatasetRequest, BuildVectorsRequest,
};
use product_build::{
    build_cycle, default_artifact_write_path, explain_product_build, maybe_reexec_build_cycle_under_cgroup,
    ProductBuildConfig,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

fn load_manifest(run_root: &PathBuf) -> anyhow::Result<CaptureManifest> {
    let manifest_path = manifest_path_for_run(&run_root.display().to_string());
    let bytes = fs::read(&manifest_path)
        .with_context(|| format!("failed to read manifest at {manifest_path}"))?;
    let manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse manifest at {manifest_path}"))?;
    Ok(manifest)
}

fn print_partial_run_hint(run_root: &PathBuf) {
    println!(
        "run {} does not have meta/manifest.json yet",
        run_root.display()
    );
    println!("the legacy capture is probably still in flight");
}

fn usage() -> &'static str {
    "usage:
  preprocessor-cli print-baseline
  preprocessor-cli inspect-run --run-root <path>
  preprocessor-cli compare-tile-counts --run-root <path>
  preprocessor-cli compare-sec-packages --legacy-work-dir <path> --rust-work-dir <path>
  preprocessor-cli compare-chart-packages --family <sec|tac|enr-l|enr-h> --legacy-work-dir <path> --rust-work-dir <path>
  preprocessor-cli compare-chart-tile-paths --family <sec|tac|enr-l|enr-h> --legacy-work-dir <path> --rust-work-dir <path>
  preprocessor-cli compare-csup-packages --legacy-work-dir <path> --rust-work-dir <path>
  preprocessor-cli compare-tpp-packages --region <AK|PAC|NW|SW|NC|EC|SC|NE|SE> --legacy-work-dir <path> --rust-work-dir <path>
  preprocessor-cli compare-csup-images --legacy-work-dir <path> --rust-work-dir <path> [--sample-percent <0-100>] [--rmse-threshold <0-1>] [--limit <count>]
  preprocessor-cli compare-tpp-images --region <AK|PAC|NW|SW|NC|EC|SC|NE|SE> --legacy-work-dir <path> --rust-work-dir <path> [--sample-percent <0-100>] [--rmse-threshold <0-1>] [--limit <count>]
  preprocessor-cli compare-provenance --left-provenance-dir <path> --right-provenance-dir <path>
  preprocessor-cli compare-data-db --left-db <path> --right-db <path>
  preprocessor-cli compare-sampled-images --left-root <path> --right-root <path> [--sample-percent <0-100>] [--rmse-threshold <0-1>] [--limit <count>]
  preprocessor-cli print-cache-layout --cache-root <path> --url <url> --sha256 <sha256>
  preprocessor-cli print-tool-example --cwd <path>
  preprocessor-cli explain-chart --family <sec|tac|enr-l|enr-h> --cpus <count>
  preprocessor-cli build-vrts --family <sec|tac|enr-l|enr-h> --work-dir <path> --cpu-jobs <count>
  preprocessor-cli build-tiles --family <sec|tac|enr-l|enr-h> --work-dir <path> --cpu-jobs <count>
  preprocessor-cli package-regions --family <sec|tac|enr-l|enr-h> --work-dir <path>
  preprocessor-cli run-native-chart --family <sec|tac|enr-l|enr-h> --source-repo <path> --run-root <path> --cpu-jobs <count> [--prefetch-source-urls <path>] [--fetch-jobs <count>]
  preprocessor-cli run-native-csup --source-repo <path> --run-root <path> [--prefetch-source-urls <path>] [--fetch-jobs <count>]
  preprocessor-cli run-native-tpp --region <AK|PAC|NW|SW|NC|EC|SC|NE|SE> --source-repo <path> --run-root <path> [--prefetch-source-urls <path>] [--fetch-jobs <count>]
  preprocessor-cli build-data --input-dir <path> --output-dir <path> --manifest-version <cycle> [--resource-index-output <path>] [--chart-source <family-id>:<package_outputs_jsonl>:<package_root>]... [--tpp-source <package_outputs_jsonl>:<asset_root>:<package_root>]... [--csup-source <package_outputs_jsonl>:<asset_root>:<package_root>]...
  preprocessor-cli build-vectors --main-db <path> --output-dir <path> --version-label <label>
  preprocessor-cli build-obstacles [--build-root <path>] [--fetch-jobs <count>] [--snapshot-date <YYYY-MM-DD>]
  preprocessor-cli build-resource-index --nav-db-zip <path> --output <path> [--chart-source <family-id>:<package_outputs_jsonl>:<package_root>]... [--tpp-source <package_outputs_jsonl>:<asset_root>:<package_root>]... [--csup-source <package_outputs_jsonl>:<asset_root>:<package_root>]...
  preprocessor-cli build-cycle [--profile <validation|production>] [--cycle <YYCC>] [--source-root <path>] [--build-root <path>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli build-product [--profile <validation|production>] [--cycle <YYCC>] [--source-root <path>] [--build-root <path>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli explain-product-build [--profile <validation|production>] [--source-root <path>] [--build-root <path>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli run-chart --family <sec|tac|enr-l|enr-h> --source-repo <path> --run-root <path> [--prefetch-source-urls <path>] [--fetch-jobs <count>]"
}

fn count_lines(path: &PathBuf) -> anyhow::Result<u64> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text.lines().count() as u64)
}

fn hash_file(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn obstacle_snapshot_label(value: &str) -> anyhow::Result<String> {
    Ok(
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("failed to parse obstacle snapshot date {value}"))?
            .format("%Y.%m.%d")
            .to_string(),
    )
}

fn fetch_cache_config_from_root(root: PathBuf) -> anyhow::Result<FetchCacheConfig> {
    Ok(FetchCacheConfig {
        root,
        mode: FetchCacheMode::parse(&env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string()))?,
    })
}

fn run_build_obstacles_command(args: &[String]) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
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

    let mut build_root = None;
    let mut fetch_jobs = 4_usize;
    let mut snapshot_date = env::var("AEROBAG_OBSTACLE_SNAPSHOT_DATE").ok();
    let mut index = 0;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--build-root") => {
                build_root = Some(PathBuf::from(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                index += 2;
            }
            Some("--fetch-jobs") => {
                fetch_jobs = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                    .parse()
                    .context("failed to parse fetch jobs")?;
                index += 2;
            }
            Some("--snapshot-date") => {
                snapshot_date = Some(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                );
                index += 2;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }

    let snapshot_label = obstacle_snapshot_label(
        snapshot_date
            .as_deref()
            .unwrap_or(&Utc::now().format("%Y-%m-%d").to_string()),
    )?;
    let build_root = build_root.unwrap_or_else(|| {
        artifact_root
            .join("published-packaged")
            .join("obstacles")
            .join(&snapshot_label)
    });
    let output_dir = build_root.join("output");
    let manifest_path = output_dir.join(format!("obstacles_{snapshot_label}"));
    let stats_path = output_dir.join("stats.json");
    let zip_path = output_dir.join(format!("obstacles_{snapshot_label}.zip"));
    if manifest_path.is_file() && stats_path.is_file() && zip_path.is_file() {
        return Ok((manifest_path, stats_path, zip_path));
    }

    let fetch_cache_root = env::var("FETCH_CACHE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| artifact_root.join("cache").join("fetch"));
    let fetch_cache = fetch_cache_config_from_root(fetch_cache_root)?;

    let work_dir = artifact_root
        .join("private-work")
        .join("obstacles")
        .join(&snapshot_label)
        .join("work");
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    let provenance_dir = artifact_root
        .join("private-work")
        .join("obstacles")
        .join(&snapshot_label)
        .join("meta")
        .join("provenance")
        .join("obstacles");
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
        fetch_jobs,
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

fn product_cycles_to_build(config: &product_build::ProductBuildConfig) -> anyhow::Result<Vec<String>> {
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
            Ok(effective) => effective + Duration::days(28) >= as_of_date,
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

#[derive(Debug, Serialize)]
struct CurrentArtifactsManifest {
    schema_version: u32,
    as_of_date: String,
    bundles: Vec<CurrentBundleEntry>,
    obstacles: CurrentObstacleEntry,
}

#[derive(Debug, Serialize)]
struct CurrentBundleEntry {
    filename: String,
    cycle: String,
    start_valid: String,
    end_valid: String,
    checksum_sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Serialize)]
struct CurrentObstacleEntry {
    filename: String,
    published_date: String,
    checksum_sha256: String,
    size_bytes: u64,
}

fn publish_content_addressed_obstacle_zip(
    build_root: &Path,
    obstacle_zip_path: &Path,
) -> anyhow::Result<(PathBuf, String, u64)> {
    let sha256 = hash_file(obstacle_zip_path)?;
    let size_bytes = fs::metadata(obstacle_zip_path)
        .with_context(|| format!("failed to stat {}", obstacle_zip_path.display()))?
        .len();
    let published_path = build_root.join(format!("obstacles_{sha256}.zip"));
    if !published_path.is_file() {
        fs::copy(obstacle_zip_path, &published_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                obstacle_zip_path.display(),
                published_path.display()
            )
        })?;
    }
    Ok((published_path, sha256, size_bytes))
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
            checksum_sha256: bundle_manifest
                .get("checksum_sha256")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .unwrap_or(hash_file(&bundle_path)?),
            size_bytes: bundle_manifest
                .get("size_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(
                    fs::metadata(&bundle_path)
                        .with_context(|| format!("failed to stat {}", bundle_path.display()))?
                        .len(),
                ),
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
    };
    let manifest_path = build_root.join(format!(
        "current_artifacts_{}.json",
        as_of_date.format("%Y%m%d")
    ));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).context("failed to encode current artifacts manifest")?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(manifest_path)
}

fn read_zip_members(path: &Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("unzip")
        .arg("-Z1")
        .arg(path)
        .output()
        .with_context(|| format!("failed to list zip members for {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!("unzip -Z1 failed for {}", path.display());
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

fn read_tile_paths(tiles_root: &Path) -> anyhow::Result<Vec<String>> {
    fn visit(root: &Path, current: &Path, acc: &mut Vec<String>) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(current)
            .with_context(|| format!("failed to read directory {}", current.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to iterate directory {}", current.display()))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to read file type for {}", path.display()))?;
            if file_type.is_dir() {
                visit(root, &path, acc)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .with_context(|| format!("failed to relativize {}", path.display()))?;
                acc.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    if tiles_root.is_dir() {
        visit(tiles_root, tiles_root, &mut paths)?;
    }
    Ok(paths)
}

fn visit_files(
    root: &Path,
    current: &Path,
    acc: &mut Vec<String>,
    predicate: &dyn Fn(&Path) -> bool,
) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate directory {}", current.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to read file type for {}", path.display()))?;
        if file_type.is_dir() {
            visit_files(root, &path, acc, predicate)?;
        } else if file_type.is_file() && predicate(&path) {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to relativize {}", path.display()))?;
            acc.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn is_image_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "webp"
    )
}

fn read_image_paths(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut paths = Vec::new();
    if root.is_dir() {
        visit_files(root, root, &mut paths, &is_image_path)?;
    }
    Ok(paths)
}

fn sample_hash_bucket(path: &str) -> u8 {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = hasher.finalize();
    digest[0]
}

fn select_sample_paths(
    shared_paths: &[String],
    sample_percent: u8,
    limit: Option<usize>,
) -> Vec<String> {
    if shared_paths.is_empty() {
        return Vec::new();
    }
    let threshold = usize::from(sample_percent).min(100);
    let mut selected = shared_paths
        .iter()
        .filter(|path| usize::from(sample_hash_bucket(path)) * 100 / 256 < threshold)
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() && !shared_paths.is_empty() {
        selected.push(shared_paths[0].clone());
    }
    if let Some(limit) = limit {
        selected.truncate(limit);
    }
    selected
}

fn compare_image_rmse(left: &Path, right: &Path) -> anyhow::Result<f64> {
    let output = Command::new("compare")
        .args(["-metric", "RMSE"])
        .arg(left)
        .arg(right)
        .arg("null:")
        .output()
        .with_context(|| {
            format!(
                "failed to run compare for {} and {}",
                left.display(),
                right.display()
            )
        })?;
    let stderr = String::from_utf8(output.stderr).context("compare stderr was not utf-8")?;
    let metric_text = stderr
        .split('(')
        .nth(1)
        .and_then(|tail| tail.split(')').next())
        .context("compare output did not contain normalized RMSE metric")?;
    let rmse = metric_text
        .trim()
        .parse::<f64>()
        .with_context(|| format!("failed to parse RMSE from {stderr:?}"))?;
    if !output.status.success() && rmse == 0.0 {
        anyhow::bail!(
            "compare failed for {} and {}: {}",
            left.display(),
            right.display(),
            stderr.trim()
        );
    }
    Ok(rmse)
}

fn parse_chart_source_spec(value: &str) -> anyhow::Result<ChartSource> {
    let mut parts = value.splitn(4, ':');
    let family_id = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing family id"))?;
    let package_outputs_path = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing package outputs path"))?;
    let package_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing package root"))?;
    let source_urls_path = parts.next().filter(|part| !part.is_empty());
    Ok(ChartSource {
        family_id: family_id.to_string(),
        package_outputs_path: PathBuf::from(package_outputs_path),
        package_root: PathBuf::from(package_root),
        source_urls_path: source_urls_path.map(PathBuf::from),
    })
}

fn parse_asset_source_spec(value: &str) -> anyhow::Result<AssetSource> {
    let mut parts = value.splitn(4, ':');
    let package_outputs_path = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing package outputs path"))?;
    let asset_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing asset root"))?;
    let third = parts.next().filter(|part| !part.is_empty());
    let fourth = parts.next().filter(|part| !part.is_empty());
    let (package_root, source_urls_path) = match (third, fourth) {
        (Some(package_root), Some(source_urls_path)) => (package_root, Some(source_urls_path)),
        (Some(source_urls_path), None) if source_urls_path.ends_with(".jsonl") => {
            (asset_root, Some(source_urls_path))
        }
        (Some(package_root), None) => (package_root, None),
        (None, None) => (asset_root, None),
        (None, Some(_)) => unreachable!("splitn(4) cannot yield fourth without third"),
    };
    Ok(AssetSource {
        package_outputs_path: PathBuf::from(package_outputs_path),
        asset_root: PathBuf::from(asset_root),
        package_root: PathBuf::from(package_root),
        source_urls_path: source_urls_path.map(PathBuf::from),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildDataCommand {
    input_dir: PathBuf,
    output_dir: PathBuf,
    manifest_version: String,
    mode: DataBuildMode,
    resource_index_output: Option<PathBuf>,
    chart_sources: Vec<ChartSource>,
    tpp_sources: Vec<AssetSource>,
    csup_sources: Vec<AssetSource>,
}

fn parse_build_data_command(args: &[String]) -> anyhow::Result<BuildDataCommand> {
    let mut input_dir = None;
    let mut output_dir = None;
    let mut manifest_version = None;
    let mut mode = DataBuildMode::Production;
    let mut resource_index_output = None;
    let mut chart_sources = Vec::new();
    let mut tpp_sources = Vec::new();
    let mut csup_sources = Vec::new();
    let mut index = 2;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--input-dir") => {
                input_dir = Some(PathBuf::from(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                index += 2;
            }
            Some("--output-dir") => {
                output_dir = Some(PathBuf::from(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                index += 2;
            }
            Some("--manifest-version") => {
                manifest_version = Some(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                );
                index += 2;
            }
            Some("--data-mode") => {
                mode = DataBuildMode::parse(
                    args.get(index + 1)
                        .map(String::as_str)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                )?;
                index += 2;
            }
            Some("--resource-index-output") => {
                resource_index_output = Some(PathBuf::from(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                index += 2;
            }
            Some("--chart-source") => {
                chart_sources.push(parse_chart_source_spec(
                    args.get(index + 1)
                        .map(String::as_str)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                )?);
                index += 2;
            }
            Some("--tpp-source") => {
                tpp_sources.push(parse_asset_source_spec(
                    args.get(index + 1)
                        .map(String::as_str)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                )?);
                index += 2;
            }
            Some("--csup-source") => {
                csup_sources.push(parse_asset_source_spec(
                    args.get(index + 1)
                        .map(String::as_str)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                )?);
                index += 2;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }

    Ok(BuildDataCommand {
        input_dir: input_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        output_dir: output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        manifest_version: manifest_version.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        mode,
        resource_index_output,
        chart_sources,
        tpp_sources,
        csup_sources,
    })
}

fn compare_relative_image_paths(
    left_root: &Path,
    right_root: &Path,
    left_paths: BTreeSet<String>,
    right_paths: BTreeSet<String>,
    sample_percent: u8,
    rmse_threshold: f64,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    let shared_paths = left_paths
        .intersection(&right_paths)
        .cloned()
        .collect::<Vec<_>>();
    let left_only = left_paths
        .difference(&right_paths)
        .cloned()
        .collect::<Vec<_>>();
    let right_only = right_paths
        .difference(&left_paths)
        .cloned()
        .collect::<Vec<_>>();
    let sampled_paths = select_sample_paths(&shared_paths, sample_percent, limit);

    println!(
        "images left={} right={} shared={} left_only={} right_only={} sampled={} sample_percent={} rmse_threshold={}",
        left_paths.len(),
        right_paths.len(),
        shared_paths.len(),
        left_only.len(),
        right_only.len(),
        sampled_paths.len(),
        sample_percent,
        rmse_threshold
    );
    for path in left_only.iter().take(10) {
        println!("left_only {}", path);
    }
    for path in right_only.iter().take(10) {
        println!("right_only {}", path);
    }

    let mut mismatches = Vec::new();
    for relative in &sampled_paths {
        let left_path = left_root.join(relative);
        let right_path = right_root.join(relative);
        let rmse = compare_image_rmse(&left_path, &right_path)?;
        if rmse > rmse_threshold {
            mismatches.push((relative.clone(), rmse));
        }
    }

    mismatches.sort_by(|(left_path, left_rmse), (right_path, right_rmse)| {
        right_rmse
            .partial_cmp(left_rmse)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_path.cmp(right_path))
    });

    for (path, rmse) in mismatches.iter().take(20) {
        println!("image_mismatch path={} rmse={:.8}", path, rmse);
    }
    let status = if left_only.is_empty() && right_only.is_empty() && mismatches.is_empty() {
        "match"
    } else {
        "mismatch"
    };
    println!(
        "visual status={} sampled={} mismatches={} left_only={} right_only={}",
        status,
        sampled_paths.len(),
        mismatches.len(),
        left_only.len(),
        right_only.len()
    );
    Ok(())
}

fn compare_sampled_images(
    left_root: &Path,
    right_root: &Path,
    sample_percent: u8,
    rmse_threshold: f64,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    compare_relative_image_paths(
        left_root,
        right_root,
        read_image_paths(left_root)?
            .into_iter()
            .collect::<BTreeSet<_>>(),
        read_image_paths(right_root)?
            .into_iter()
            .collect::<BTreeSet<_>>(),
        sample_percent,
        rmse_threshold,
        limit,
    )
}

fn read_manifest_entries(path: &Path) -> anyhow::Result<Vec<String>> {
    Ok(fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn compare_csup_images(
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
    sample_percent: u8,
    rmse_threshold: f64,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    let mut legacy_paths = BTreeSet::new();
    let mut rust_paths = BTreeSet::new();
    for region in Region::ALL {
        let manifest_name = format!("{}_CSUP", region.code());
        for entry in read_manifest_entries(&legacy_work_dir.join(&manifest_name))? {
            if is_image_path(Path::new(&entry)) {
                legacy_paths.insert(entry);
            }
        }
        for entry in read_manifest_entries(&rust_work_dir.join(&manifest_name))? {
            if is_image_path(Path::new(&entry)) {
                rust_paths.insert(entry);
            }
        }
    }
    compare_relative_image_paths(
        legacy_work_dir,
        rust_work_dir,
        legacy_paths,
        rust_paths,
        sample_percent,
        rmse_threshold,
        limit,
    )
}

fn compare_tpp_images(
    region: &str,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
    sample_percent: u8,
    rmse_threshold: f64,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    let manifest_name = format!("{region}_TPP");
    let legacy_paths = read_manifest_entries(&legacy_work_dir.join(&manifest_name))?
        .into_iter()
        .filter(|entry| is_image_path(Path::new(entry)))
        .collect::<BTreeSet<_>>();
    let rust_paths = read_manifest_entries(&rust_work_dir.join(&manifest_name))?
        .into_iter()
        .filter(|entry| is_image_path(Path::new(entry)))
        .collect::<BTreeSet<_>>();
    compare_relative_image_paths(
        legacy_work_dir,
        rust_work_dir,
        legacy_paths,
        rust_paths,
        sample_percent,
        rmse_threshold,
        limit,
    )
}

fn parse_image_compare_options(
    args: &[String],
    start_index: usize,
) -> anyhow::Result<(u8, f64, Option<usize>)> {
    let mut sample_percent = 1_u8;
    let mut rmse_threshold = 0.0_f64;
    let mut limit = None;
    let mut index = start_index;
    while index < args.len() {
        match args[index].as_str() {
            "--sample-percent" => {
                sample_percent = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --sample-percent"))?
                    .parse()
                    .context("invalid sample percent")?;
                index += 2;
            }
            "--rmse-threshold" => {
                rmse_threshold = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --rmse-threshold"))?
                    .parse()
                    .context("invalid rmse threshold")?;
                index += 2;
            }
            "--limit" => {
                limit = Some(
                    args.get(index + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --limit"))?
                        .parse()
                        .context("invalid limit")?,
                );
                index += 2;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }
    Ok((sample_percent, rmse_threshold, limit))
}

fn compare_sec_packages(legacy_work_dir: &Path, rust_work_dir: &Path) -> anyhow::Result<()> {
    compare_chart_packages("SEC", legacy_work_dir, rust_work_dir)
}

fn compare_csup_packages(legacy_work_dir: &Path, rust_work_dir: &Path) -> anyhow::Result<()> {
    compare_named_packages("CSUP", legacy_work_dir, rust_work_dir)
}

fn compare_tpp_packages(
    region: Region,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
) -> anyhow::Result<()> {
    compare_single_named_package(region, "TPP", legacy_work_dir, rust_work_dir)
}

fn compare_chart_packages(
    chart_name: &str,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
) -> anyhow::Result<()> {
    compare_named_packages(chart_name, legacy_work_dir, rust_work_dir)
}

fn compare_named_packages(
    suffix: &str,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
) -> anyhow::Result<()> {
    for region in Region::ALL {
        let region = region.code();
        let manifest_name = format!("{region}_{suffix}");
        let zip_name = format!("{region}_{suffix}.zip");
        let legacy_manifest = legacy_work_dir.join(&manifest_name);
        let rust_manifest = rust_work_dir.join(&manifest_name);
        let legacy_zip = legacy_work_dir.join(&zip_name);
        let rust_zip = rust_work_dir.join(&zip_name);

        let legacy_manifest_hash = hash_file(&legacy_manifest)?;
        let rust_manifest_hash = hash_file(&rust_manifest)?;
        let manifest_bytes_status = if legacy_manifest_hash == rust_manifest_hash {
            "match"
        } else {
            "mismatch"
        };
        let legacy_manifest_lines = fs::read_to_string(&legacy_manifest)
            .with_context(|| format!("failed to read {}", legacy_manifest.display()))?
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let rust_manifest_lines = fs::read_to_string(&rust_manifest)
            .with_context(|| format!("failed to read {}", rust_manifest.display()))?
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut legacy_manifest_set = legacy_manifest_lines.clone();
        let mut rust_manifest_set = rust_manifest_lines.clone();
        legacy_manifest_set.sort();
        rust_manifest_set.sort();
        let manifest_entries_status = if legacy_manifest_set == rust_manifest_set {
            "match"
        } else {
            "mismatch"
        };

        let legacy_members = read_zip_members(&legacy_zip)?;
        let rust_members = read_zip_members(&rust_zip)?;
        let member_status = if legacy_members == rust_members {
            "match"
        } else {
            "mismatch"
        };

        println!(
            "{region} manifest_bytes={} manifest_entries={} legacy_members={} rust_members={} members={}",
            manifest_bytes_status,
            manifest_entries_status,
            legacy_members.len(),
            rust_members.len(),
            member_status
        );
    }

    Ok(())
}

fn compare_single_named_package(
    region: Region,
    suffix: &str,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
) -> anyhow::Result<()> {
    let region = region.code();
    let manifest_name = format!("{region}_{suffix}");
    let zip_name = format!("{region}_{suffix}.zip");
    let legacy_manifest = legacy_work_dir.join(&manifest_name);
    let rust_manifest = rust_work_dir.join(&manifest_name);
    let legacy_zip = legacy_work_dir.join(&zip_name);
    let rust_zip = rust_work_dir.join(&zip_name);

    let legacy_manifest_hash = hash_file(&legacy_manifest)?;
    let rust_manifest_hash = hash_file(&rust_manifest)?;
    let manifest_bytes_status = if legacy_manifest_hash == rust_manifest_hash {
        "match"
    } else {
        "mismatch"
    };
    let legacy_manifest_lines = fs::read_to_string(&legacy_manifest)
        .with_context(|| format!("failed to read {}", legacy_manifest.display()))?
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let rust_manifest_lines = fs::read_to_string(&rust_manifest)
        .with_context(|| format!("failed to read {}", rust_manifest.display()))?
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut legacy_manifest_set = legacy_manifest_lines.clone();
    let mut rust_manifest_set = rust_manifest_lines.clone();
    legacy_manifest_set.sort();
    rust_manifest_set.sort();
    let manifest_entries_status = if legacy_manifest_set == rust_manifest_set {
        "match"
    } else {
        "mismatch"
    };

    let legacy_members = read_zip_members(&legacy_zip)?;
    let rust_members = read_zip_members(&rust_zip)?;
    let member_status = if legacy_members == rust_members {
        "match"
    } else {
        "mismatch"
    };

    println!(
        "{region} manifest_bytes={} manifest_entries={} legacy_members={} rust_members={} members={}",
        manifest_bytes_status,
        manifest_entries_status,
        legacy_members.len(),
        rust_members.len(),
        member_status
    );

    Ok(())
}

fn compare_chart_tile_paths(
    family: ChartFamily,
    legacy_work_dir: &Path,
    rust_work_dir: &Path,
) -> anyhow::Result<()> {
    let spec = match family {
        ChartFamily::Sec => ("SEC", "0"),
        ChartFamily::Tac => ("TAC", "1"),
        ChartFamily::EnrL => ("ENR_L", "3"),
        ChartFamily::EnrH => ("ENR_H", "4"),
    };

    let legacy_tiles_root = legacy_work_dir.join("tiles").join(spec.1);
    let rust_tiles_root = rust_work_dir.join("tiles").join(spec.1);
    let legacy_paths = read_tile_paths(&legacy_tiles_root)?;
    let rust_paths = read_tile_paths(&rust_tiles_root)?;

    let status = if legacy_paths == rust_paths {
        "match"
    } else {
        "mismatch"
    };
    println!(
        "{} legacy_tile_paths={} rust_tile_paths={} status={}",
        spec.0,
        legacy_paths.len(),
        rust_paths.len(),
        status
    );

    if legacy_paths != rust_paths {
        let legacy_set = legacy_paths
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let rust_set = rust_paths
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        for missing in legacy_set.difference(&rust_set).take(10) {
            println!("missing_in_rust {}", missing);
        }
        for extra in rust_set.difference(&legacy_set).take(10) {
            println!("extra_in_rust {}", extra);
        }
    }

    Ok(())
}

fn print_set_diff(
    label: &str,
    left: &std::collections::BTreeSet<String>,
    right: &std::collections::BTreeSet<String>,
) {
    for missing in left.difference(right).take(10) {
        println!("{label} missing_in_right {}", missing);
    }
    for extra in right.difference(left).take(10) {
        println!("{label} extra_in_right {}", extra);
    }
}

fn compare_provenance(
    left_provenance_dir: &Path,
    right_provenance_dir: &Path,
) -> anyhow::Result<()> {
    let left_source_urls = read_source_url_set(left_provenance_dir.join("source_urls.jsonl"))?;
    let right_source_urls = read_source_url_set(right_provenance_dir.join("source_urls.jsonl"))?;
    let source_url_status = if left_source_urls == right_source_urls {
        "match"
    } else {
        "mismatch"
    };
    println!(
        "source_urls left={} right={} status={}",
        left_source_urls.len(),
        right_source_urls.len(),
        source_url_status
    );
    if left_source_urls != right_source_urls {
        print_set_diff("source_urls", &left_source_urls, &right_source_urls);
    }

    let left_downloads = read_download_records(left_provenance_dir.join("downloads.jsonl"))?;
    let right_downloads = read_download_records(right_provenance_dir.join("downloads.jsonl"))?;
    let download_status = if left_downloads == right_downloads {
        "match"
    } else {
        "mismatch"
    };
    println!(
        "downloads left={} right={} status={}",
        left_downloads.len(),
        right_downloads.len(),
        download_status
    );
    if left_downloads != right_downloads {
        for missing in left_downloads.difference(&right_downloads).take(10) {
            println!(
                "downloads missing_in_right url={} file={} sha256={}",
                missing.url, missing.file, missing.sha256
            );
        }
        for extra in right_downloads.difference(&left_downloads).take(10) {
            println!(
                "downloads extra_in_right url={} file={} sha256={}",
                extra.url, extra.file, extra.sha256
            );
        }
    }

    let left_extracts = read_extract_records(left_provenance_dir.join("downloads.jsonl"))?;
    let right_extracts = read_extract_records(right_provenance_dir.join("downloads.jsonl"))?;
    let extract_status = if left_extracts == right_extracts {
        "match"
    } else {
        "mismatch"
    };
    println!(
        "extracts left={} right={} status={}",
        left_extracts.len(),
        right_extracts.len(),
        extract_status
    );
    if left_extracts != right_extracts {
        for missing in left_extracts.difference(&right_extracts).take(10) {
            println!(
                "extracts missing_in_right archive={} members={}",
                missing.archive,
                missing.members.join(",")
            );
        }
        for extra in right_extracts.difference(&left_extracts).take(10) {
            println!(
                "extracts extra_in_right archive={} members={}",
                extra.archive,
                extra.members.join(",")
            );
        }
    }

    Ok(())
}

fn parse_family(value: &str) -> anyhow::Result<ChartFamily> {
    match value {
        "sec" => Ok(ChartFamily::Sec),
        "tac" => Ok(ChartFamily::Tac),
        "enr-l" => Ok(ChartFamily::EnrL),
        "enr-h" => Ok(ChartFamily::EnrH),
        _ => anyhow::bail!("unknown family: {value}"),
    }
}

fn parse_region(value: &str) -> anyhow::Result<Region> {
    Region::from_code(value).ok_or_else(|| anyhow::anyhow!("unknown region: {value}"))
}

fn parallelism_name(value: Parallelism) -> &'static str {
    match value {
        Parallelism::Serial => "serial",
        Parallelism::Bounded => "bounded",
        Parallelism::Wide => "wide",
    }
}

fn work_kind_name(value: WorkKind) -> &'static str {
    match value {
        WorkKind::Network => "network",
        WorkKind::Extract => "extract",
        WorkKind::Cpu => "cpu",
        WorkKind::Io => "io",
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("inspect-run") => {
            if args.get(2).map(String::as_str) != Some("--run-root") {
                anyhow::bail!("{}", usage());
            }
            let run_root = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let manifest = match load_manifest(&run_root) {
                Ok(manifest) => manifest,
                Err(err) if err.to_string().contains("failed to read manifest") => {
                    print_partial_run_hint(&run_root);
                    return Ok(());
                }
                Err(err) => return Err(err),
            };
            println!("{}", manifest_summary(&manifest));
            for capture in &manifest.captures {
                let targets = comparison_targets(capture).join(", ");
                println!("{}: {}", capture.label, targets);
            }
        }
        Some("print-baseline") => {
            let baseline = ExpectedTileCounts::CURRENT_BASELINE;
            println!("SEC {}", baseline.sec);
            println!("TAC {}", baseline.tac);
            println!("ENR_L {}", baseline.enr_l);
            println!("ENR_H unknown");
        }
        Some("compare-tile-counts") => {
            if args.get(2).map(String::as_str) != Some("--run-root") {
                anyhow::bail!("{}", usage());
            }
            let run_root = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            for family in [
                ChartFamily::Sec,
                ChartFamily::Tac,
                ChartFamily::EnrL,
                ChartFamily::EnrH,
            ] {
                let label = family.capture_label();
                let path = run_root
                    .join("meta")
                    .join(format!("{label}.tile-paths.txt"));
                let actual = count_lines(&path)?;
                if let Some(expected) = family.baseline_tile_count() {
                    let status = if actual == expected {
                        "match"
                    } else {
                        "mismatch"
                    };
                    println!("{label} expected={expected} actual={actual} status={status}");
                } else {
                    println!("{label} expected=unknown actual={actual} status=unknown");
                }
            }
        }
        Some("compare-sec-packages") => {
            if args.get(2).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(4).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let legacy_work_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_sec_packages(&legacy_work_dir, &rust_work_dir)?;
        }
        Some("compare-chart-packages") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(6).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let chart_name = match family {
                ChartFamily::Sec => "SEC",
                ChartFamily::Tac => "TAC",
                ChartFamily::EnrL => "ENR_L",
                ChartFamily::EnrH => "ENR_H",
            };
            let legacy_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_chart_packages(chart_name, &legacy_work_dir, &rust_work_dir)?;
        }
        Some("compare-chart-tile-paths") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(6).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let legacy_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_chart_tile_paths(family, &legacy_work_dir, &rust_work_dir)?;
        }
        Some("compare-csup-packages") => {
            if args.get(2).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(4).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let legacy_work_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_csup_packages(&legacy_work_dir, &rust_work_dir)?;
        }
        Some("compare-tpp-packages") => {
            if args.get(2).map(String::as_str) != Some("--region")
                || args.get(4).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(6).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let region = parse_region(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let legacy_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_tpp_packages(region, &legacy_work_dir, &rust_work_dir)?;
        }
        Some("compare-csup-images") => {
            if args.get(2).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(4).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let legacy_work_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let (sample_percent, rmse_threshold, limit) = parse_image_compare_options(&args, 6)?;
            compare_csup_images(
                &legacy_work_dir,
                &rust_work_dir,
                sample_percent,
                rmse_threshold,
                limit,
            )?;
        }
        Some("compare-tpp-images") => {
            if args.get(2).map(String::as_str) != Some("--region")
                || args.get(4).map(String::as_str) != Some("--legacy-work-dir")
                || args.get(6).map(String::as_str) != Some("--rust-work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let region = parse_region(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let legacy_work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let rust_work_dir = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let (sample_percent, rmse_threshold, limit) = parse_image_compare_options(&args, 8)?;
            compare_tpp_images(
                region.code(),
                &legacy_work_dir,
                &rust_work_dir,
                sample_percent,
                rmse_threshold,
                limit,
            )?;
        }
        Some("print-cache-layout") => {
            if args.get(2).map(String::as_str) != Some("--cache-root")
                || args.get(4).map(String::as_str) != Some("--url")
                || args.get(6).map(String::as_str) != Some("--sha256")
            {
                anyhow::bail!("{}", usage());
            }
            let cache_root = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let url = args
                .get(5)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
            let sha256 = args
                .get(7)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
            let layout = CacheLayout::new(cache_root);
            println!("url_hash {}", hash_text(&url));
            println!("blob {}", layout.blob_path(&sha256).display());
            println!("http {}", layout.http_metadata_path(&url).display());
            println!(
                "object {}",
                layout.object_metadata_path("example-source").display()
            );
        }
        Some("compare-provenance") => {
            if args.get(2).map(String::as_str) != Some("--left-provenance-dir")
                || args.get(4).map(String::as_str) != Some("--right-provenance-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let left_provenance_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let right_provenance_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_provenance(&left_provenance_dir, &right_provenance_dir)?;
        }
        Some("compare-data-db") => {
            if args.get(2).map(String::as_str) != Some("--left-db")
                || args.get(4).map(String::as_str) != Some("--right-db")
            {
                anyhow::bail!("{}", usage());
            }
            let left_db = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let right_db = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_databases(&left_db, &right_db)?;
        }
        Some("compare-sampled-images") => {
            if args.get(2).map(String::as_str) != Some("--left-root")
                || args.get(4).map(String::as_str) != Some("--right-root")
            {
                anyhow::bail!("{}", usage());
            }
            let left_root = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let right_root = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let (sample_percent, rmse_threshold, limit) = parse_image_compare_options(&args, 6)?;
            compare_sampled_images(
                &left_root,
                &right_root,
                sample_percent,
                rmse_threshold,
                limit,
            )?;
        }
        Some("print-tool-example") => {
            if args.get(2).map(String::as_str) != Some("--cwd") {
                anyhow::bail!("{}", usage());
            }
            let cwd = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let invocation = ToolInvocation {
                program: "gdalwarp".to_string(),
                args: vec![
                    "-of".to_string(),
                    "vrt".to_string(),
                    "input.tif".to_string(),
                    "output.vrt".to_string(),
                ],
                cwd,
                label: "example-gdalwarp".to_string(),
                env: Vec::new(),
                stdin_text: None,
            };
            println!("{}", invocation.render_command_line());
            let logs = invocation.log_paths("logs");
            println!("stdout {}", logs.stdout.display());
            println!("stderr {}", logs.stderr.display());
        }
        Some("explain-chart") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--cpus")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let cpus: usize = args
                .get(5)
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                .parse()
                .context("failed to parse cpu count")?;
            let concurrency = ConcurrencyConfig::recommended_for_machine(cpus);
            println!("family {}", family.capture_label());
            println!("likely_bottleneck {}", likely_current_bottleneck());
            println!(
                "recommended_jobs fetch={} extract={} cpu={} zip={}",
                concurrency.fetch_jobs,
                concurrency.extract_jobs,
                concurrency.cpu_jobs,
                concurrency.zip_jobs
            );
            for phase in phase_plan(family, &concurrency) {
                println!(
                    "phase {} kind={} legacy={} rust={} jobs={} bottleneck={} note={}",
                    phase.name,
                    work_kind_name(phase.work_kind),
                    parallelism_name(phase.legacy_parallelism),
                    parallelism_name(phase.rust_parallelism),
                    phase.recommended_jobs,
                    phase.expected_bottleneck,
                    phase.note
                );
            }
        }
        Some("build-vrts") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--work-dir")
                || args.get(6).map(String::as_str) != Some("--cpu-jobs")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let cpu_jobs: usize = args
                .get(7)
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                .parse()
                .context("failed to parse cpu jobs")?;
            let result = build_family_vrts(family, &work_dir, cpu_jobs)?;
            println!("family {}", result.family.capture_label());
            println!("vrt_count {}", result.vrt_count);
            println!("elapsed_ms {}", result.elapsed_ms);
            println!("main_vrt {}", result.main_vrt.display());
        }
        Some("build-tiles") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--work-dir")
                || args.get(6).map(String::as_str) != Some("--cpu-jobs")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let cpu_jobs: usize = args
                .get(7)
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                .parse()
                .context("failed to parse cpu jobs")?;
            let result = build_family_tiles(family, &work_dir, cpu_jobs)?;
            println!("family {}", result.family.capture_label());
            println!("tile_count {}", result.tile_count);
            println!("elapsed_ms {}", result.elapsed_ms);
            println!("tiles_root {}", result.tiles_root.display());
        }
        Some("package-regions") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let work_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let result = package_family_regions(family, &work_dir)?;
            println!("family {}", result.family.capture_label());
            println!("package_count {}", result.package_count);
            println!("elapsed_ms {}", result.elapsed_ms);
        }
        Some("run-native-chart") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--source-repo")
                || args.get(6).map(String::as_str) != Some("--run-root")
                || args.get(8).map(String::as_str) != Some("--cpu-jobs")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let source_repo = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let run_root = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let cpu_jobs: usize = args
                .get(9)
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                .parse()
                .context("failed to parse cpu jobs")?;
            let mut prefetch_source_urls = None;
            let mut fetch_jobs = 4_usize;
            let mut index = 10;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--prefetch-source-urls") => {
                        prefetch_source_urls = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--fetch-jobs") => {
                        fetch_jobs = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .parse()
                            .context("failed to parse fetch jobs")?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let result = run_native_family(&NativeChartRunRequest {
                family,
                source_repo,
                run_root,
                cpu_jobs,
                prefetch_source_urls,
                fetch_jobs,
                fetch_cache: env::var("FETCH_CACHE_ROOT")
                    .ok()
                    .map(PathBuf::from)
                    .map(fetch_cache_config_from_root)
                    .transpose()?,
            })?;
            println!("family {}", result.family.capture_label());
            println!("prefetch_elapsed_ms {}", result.prefetch_elapsed_ms);
            println!("vrt_count {}", result.vrt_count);
            println!("vrt_elapsed_ms {}", result.vrt_elapsed_ms);
            println!("tile_count {}", result.tile_count);
            println!("tile_elapsed_ms {}", result.tile_elapsed_ms);
            println!("package_count {}", result.package_count);
            println!("package_elapsed_ms {}", result.package_elapsed_ms);
            println!("work_dir {}", result.work_dir.display());
        }
        Some("run-native-csup") => {
            if args.get(2).map(String::as_str) != Some("--source-repo")
                || args.get(4).map(String::as_str) != Some("--run-root")
            {
                anyhow::bail!("{}", usage());
            }
            let source_repo = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let run_root = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let mut prefetch_source_urls = None;
            let mut fetch_jobs = 4_usize;
            let mut index = 6;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--prefetch-source-urls") => {
                        prefetch_source_urls = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--fetch-jobs") => {
                        fetch_jobs = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .parse()
                            .context("failed to parse fetch jobs")?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let result = run_native_csup(&NativeCsupRunRequest {
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                render_jobs: std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(8),
                fetch_cache: env::var("FETCH_CACHE_ROOT")
                    .ok()
                    .map(PathBuf::from)
                    .map(fetch_cache_config_from_root)
                    .transpose()?,
            })?;
            println!("prefetch_elapsed_ms {}", result.prefetch_elapsed_ms);
            println!("render_elapsed_ms {}", result.render_elapsed_ms);
            println!("package_elapsed_ms {}", result.package_elapsed_ms);
            println!("package_count {}", result.package_count);
            println!("work_dir {}", result.work_dir.display());
        }
        Some("run-native-tpp") => {
            if args.get(2).map(String::as_str) != Some("--region")
                || args.get(4).map(String::as_str) != Some("--source-repo")
                || args.get(6).map(String::as_str) != Some("--run-root")
            {
                anyhow::bail!("{}", usage());
            }
            let region = parse_region(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let source_repo = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let run_root = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let mut prefetch_source_urls = None;
            let mut fetch_jobs = 4_usize;
            let mut index = 8;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--prefetch-source-urls") => {
                        prefetch_source_urls = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--fetch-jobs") => {
                        fetch_jobs = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .parse()
                            .context("failed to parse fetch jobs")?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let result = run_native_tpp(&NativeTppRunRequest {
                region,
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                render_jobs: std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(8),
                fetch_cache: env::var("FETCH_CACHE_ROOT")
                    .ok()
                    .map(PathBuf::from)
                    .map(fetch_cache_config_from_root)
                    .transpose()?,
            })?;
            println!("prefetch_elapsed_ms {}", result.prefetch_elapsed_ms);
            println!("render_elapsed_ms {}", result.render_elapsed_ms);
            println!("package_elapsed_ms {}", result.package_elapsed_ms);
            println!("package_count {}", result.package_count);
            println!("work_dir {}", result.work_dir.display());
        }
        Some("build-resource-index") => {
            let mut nav_db_zip = None;
            let mut output_path = None;
            let mut chart_sources = Vec::new();
            let mut tpp_sources = Vec::new();
            let mut csup_sources = Vec::new();
            let mut index = 2;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--nav-db-zip") => {
                        nav_db_zip = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--output") => {
                        output_path = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--chart-source") => {
                        chart_sources.push(parse_chart_source_spec(
                            args.get(index + 1)
                                .map(String::as_str)
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        )?);
                        index += 2;
                    }
                    Some("--tpp-source") => {
                        tpp_sources.push(parse_asset_source_spec(
                            args.get(index + 1)
                                .map(String::as_str)
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        )?);
                        index += 2;
                    }
                    Some("--csup-source") => {
                        csup_sources.push(parse_asset_source_spec(
                            args.get(index + 1)
                                .map(String::as_str)
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        )?);
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let request = BuildResourceIndexRequest {
                nav_db_zip: nav_db_zip.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                output_path: output_path.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                catalog_output_path: None,
                chart_sources,
                tpp_sources,
                csup_sources,
            };
            let index = write_resource_index(&request)?;
            println!("output {}", request.output_path.display());
            println!("airport_count {}", index.airports.len());
            println!("package_count {}", index.packages.len());
            println!("plate_count {}", index.plates.len());
            println!("csup_count {}", index.csups.len());
        }
        Some("build-data") => {
            let command = parse_build_data_command(&args)?;
            let result = build_data_package(&DataBuildRequest {
                input_dir: command.input_dir,
                output_dir: command.output_dir,
                manifest_version: command.manifest_version,
                mode: command.mode,
                artifact_stem: None,
            })?;
            println!("main_db {}", result.main_db.display());
            println!("manifest {}", result.manifest_path.display());
            println!("zip {}", result.zip_path.display());
            for (table, count) in result.row_counts {
                println!("table {} rows {}", table, count);
            }
            if let Some(output_path) = command.resource_index_output {
                let request = BuildResourceIndexRequest {
                    nav_db_zip: result.zip_path.clone(),
                    output_path,
                    catalog_output_path: None,
                    chart_sources: command.chart_sources,
                    tpp_sources: command.tpp_sources,
                    csup_sources: command.csup_sources,
                };
                let index = write_resource_index(&request)?;
                println!("resource_index {}", request.output_path.display());
                println!("airport_count {}", index.airports.len());
                println!("package_count {}", index.packages.len());
                println!("plate_count {}", index.plates.len());
                println!("csup_count {}", index.csups.len());
            }
        }
        Some("build-vectors") => {
            let mut main_db = None;
            let mut output_dir = None;
            let mut version_label = None;
            let mut index = 2;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--main-db") => {
                        main_db = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--output-dir") => {
                        output_dir = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--version-label") => {
                        version_label = Some(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        );
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let request = BuildVectorsRequest {
                main_db: main_db.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                output_dir: output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                version_label: version_label.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            };
            let result = build_vectors_dataset(&request)?;
            println!("manifest {}", result.manifest_path.display());
            println!("stats {}", result.stats_path.display());
            println!("zip {}", result.zip_path.display());
        }
        Some("build-obstacles") => {
            let (manifest_path, stats_path, zip_path) = run_build_obstacles_command(&args[2..])?;
            println!("manifest {}", manifest_path.display());
            println!("stats {}", stats_path.display());
            println!("zip {}", zip_path.display());
        }
        Some("build-cycle") => {
            if maybe_reexec_build_cycle_under_cgroup(&args[2..])? {
                return Ok(());
            }
            let config = ProductBuildConfig::from_env_and_args(&args[2..])?;
            let manifest_path = build_cycle(&config)?;
            println!("{}", manifest_path.display());
        }
        Some("build-product") => {
            let config = ProductBuildConfig::from_env_and_args(&args[2..])?;
            let mut cycle_manifest_paths = Vec::new();
            for cycle in product_cycles_to_build(&config)? {
                let mut cycle_config = config.clone();
                cycle_config.target_cycle = Some(cycle);
                cycle_manifest_paths.push(build_cycle(&cycle_config)?);
            }
            let (obstacle_manifest_path, obstacle_stats_path, obstacle_zip_path) =
                run_build_obstacles_command(&[])?;
            let as_of_date = Utc::now().date_naive();
            let (published_obstacle_zip, obstacle_sha256, obstacle_size_bytes) =
                publish_content_addressed_obstacle_zip(
                    &config.build_root,
                    &obstacle_zip_path,
                )?;
            let current_artifacts_path = write_current_artifacts_manifest(
                &config.build_root,
                as_of_date,
                &published_obstacle_zip,
                &obstacle_sha256,
                obstacle_size_bytes,
            )?;
            for cycle_manifest_path in cycle_manifest_paths {
                println!("cycle_manifest {}", cycle_manifest_path.display());
            }
            println!("current_artifacts {}", current_artifacts_path.display());
            println!("obstacle_manifest {}", obstacle_manifest_path.display());
            println!("obstacle_stats {}", obstacle_stats_path.display());
            println!("obstacle_zip {}", obstacle_zip_path.display());
            println!("published_obstacle_zip {}", published_obstacle_zip.display());
        }
        Some("explain-product-build") => {
            let config = ProductBuildConfig::from_env_and_args(&args[2..])?;
            print!("{}", explain_product_build(&config)?);
        }
        Some("run-chart") => {
            if args.get(2).map(String::as_str) != Some("--family")
                || args.get(4).map(String::as_str) != Some("--source-repo")
                || args.get(6).map(String::as_str) != Some("--run-root")
            {
                anyhow::bail!("{}", usage());
            }
            let family = parse_family(
                args.get(3)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            )?;
            let source_repo = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let run_root = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let mut prefetch_source_urls = None;
            let mut fetch_jobs = 4_usize;
            let mut index = 8;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--prefetch-source-urls") => {
                        prefetch_source_urls = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--fetch-jobs") => {
                        fetch_jobs = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .parse()
                            .context("failed to parse fetch jobs")?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let result = run_family(&ChartRunRequest {
                family,
                source_repo,
                run_root,
                prefetch_source_urls,
                fetch_jobs,
                fetch_cache: env::var("FETCH_CACHE_ROOT")
                    .ok()
                    .map(PathBuf::from)
                    .map(fetch_cache_config_from_root)
                    .transpose()?,
            })?;
            println!("family {}", result.family.capture_label());
            println!("prefetch_elapsed_ms {}", result.prefetch_elapsed_ms);
            println!("legacy_elapsed_ms {}", result.outcome.elapsed_ms);
            println!("elapsed_ms {}", result.outcome.elapsed_ms);
            println!("tile_count {}", result.tile_count);
            println!("stdout {}", result.outcome.logs.stdout.display());
            println!("stderr {}", result.outcome.logs.stderr.display());
            println!("work_dir {}", result.work_dir.display());
        }
        _ => anyhow::bail!("{}", usage()),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_build_data_command_accepts_minimal_form() {
        let args = vec![
            "preprocessor-cli".to_string(),
            "build-data".to_string(),
            "--input-dir".to_string(),
            "/tmp/input".to_string(),
            "--output-dir".to_string(),
            "/tmp/output".to_string(),
            "--manifest-version".to_string(),
            "2604".to_string(),
        ];
        let command = parse_build_data_command(&args).expect("parse build-data");
        assert_eq!(command.input_dir, PathBuf::from("/tmp/input"));
        assert_eq!(command.output_dir, PathBuf::from("/tmp/output"));
        assert_eq!(command.manifest_version, "2604");
        assert_eq!(command.mode, DataBuildMode::Production);
        assert_eq!(command.resource_index_output, None);
        assert!(command.chart_sources.is_empty());
        assert!(command.tpp_sources.is_empty());
        assert!(command.csup_sources.is_empty());
    }

    #[test]
    fn parse_build_data_command_accepts_resource_index_options() {
        let args = vec![
            "preprocessor-cli".to_string(),
            "build-data".to_string(),
            "--input-dir".to_string(),
            "/tmp/input".to_string(),
            "--output-dir".to_string(),
            "/tmp/output".to_string(),
            "--manifest-version".to_string(),
            "2604".to_string(),
            "--resource-index-output".to_string(),
            "/tmp/output/resource-index.json".to_string(),
            "--chart-source".to_string(),
            "sectional:/tmp/sec.jsonl:/tmp/sec-root".to_string(),
            "--tpp-source".to_string(),
            "/tmp/tpp.jsonl:/tmp/tpp-root".to_string(),
            "--csup-source".to_string(),
            "/tmp/csup.jsonl:/tmp/csup-root".to_string(),
        ];
        let command = parse_build_data_command(&args).expect("parse build-data");
        assert_eq!(
            command.resource_index_output,
            Some(PathBuf::from("/tmp/output/resource-index.json"))
        );
        assert_eq!(command.chart_sources.len(), 1);
        assert_eq!(command.chart_sources[0].family_id, "sectional");
        assert_eq!(
            command.chart_sources[0].package_outputs_path,
            PathBuf::from("/tmp/sec.jsonl")
        );
        assert_eq!(
            command.chart_sources[0].package_root,
            PathBuf::from("/tmp/sec-root")
        );
        assert_eq!(command.tpp_sources.len(), 1);
        assert_eq!(
            command.tpp_sources[0].package_outputs_path,
            PathBuf::from("/tmp/tpp.jsonl")
        );
        assert_eq!(
            command.tpp_sources[0].asset_root,
            PathBuf::from("/tmp/tpp-root")
        );
        assert_eq!(command.csup_sources.len(), 1);
        assert_eq!(
            command.csup_sources[0].package_outputs_path,
            PathBuf::from("/tmp/csup.jsonl")
        );
        assert_eq!(
            command.csup_sources[0].asset_root,
            PathBuf::from("/tmp/csup-root")
        );
    }

    #[test]
    fn parse_build_data_command_accepts_data_mode() {
        let args = vec![
            "preprocessor-cli".to_string(),
            "build-data".to_string(),
            "--input-dir".to_string(),
            "/tmp/input".to_string(),
            "--output-dir".to_string(),
            "/tmp/output".to_string(),
            "--manifest-version".to_string(),
            "2604".to_string(),
            "--data-mode".to_string(),
            "legacy_avare".to_string(),
        ];
        let command = parse_build_data_command(&args).expect("parse build-data");
        assert_eq!(command.mode, DataBuildMode::LegacyAvare);
    }
}
