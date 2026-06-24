use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use chrono::{DateTime, NaiveDate, Utc};
mod emit_source_urls;
mod product_build;
use preprocessor_charts::{
    build_family_tiles, build_family_vrts, likely_current_bottleneck, package_family_regions,
    phase_plan, run_family, run_native_family, ChartRunRequest, NativeChartRunRequest,
};
use preprocessor_core::{ChartFamily, ConcurrencyConfig, Parallelism, Region, WorkKind};
use preprocessor_csup::{run_native_csup, NativeCsupRunRequest};
use preprocessor_data::{
    audit_tpp_cifp_matching, build_data_package, choose_matching_bundle, load_matching_bundle,
    resolve_matching_db_path, tpp_zip_paths_from_bundle, DataBuildRequest,
};
use preprocessor_fetch::{
    hash_text, prefetch_archives_with_provenance, read_download_records, read_extract_records,
    read_source_url_set, CacheLayout, FetchCacheConfig, FetchCacheMode, PrefetchRequest,
};
use preprocessor_live_feeds::{
    build_notam_dataset, terrain_ellipsoid_height_feet_from_navd88_meters, BuildNotamRequest,
    GeoidGrid,
};
use preprocessor_resource_index::{
    write_resource_index, AssetSource, BuildResourceIndexRequest, ChartSource,
};
use preprocessor_tools::ToolInvocation;
use preprocessor_tpp::{run_native_tpp, NativeTppRunRequest};
use preprocessor_vectors::{
    analyze_obstacle_thresholds, audit_class_airspace_simplification, build_bravo_union_svg,
    build_obstacle_dataset, build_vectors_dataset, AnalyzeObstacleThresholdsRequest,
    AuditClassAirspaceSimplificationRequest, BuildBravoUnionSvgRequest,
    BuildObstacleDatasetRequest, BuildVectorsRequest,
};
use product_build::{
    audit_procedure_geometry_from_sqlite, build_cycle, build_product, default_artifact_write_path,
    explain_product_build, gc_build_cache, gc_fetch_cache, gc_publication,
    maybe_reexec_build_cycle_under_cgroup, merge_current_artifacts_manifests,
    publish_discovery_manifest, BuildCacheGcConfig, BuildCacheGcMode, BuildCacheGcReport,
    FetchCacheGcCandidateKind, FetchCacheGcConfig, FetchCacheGcReport,
    ProcedureGeometryAuditFilter, ProductBuildConfig, PublicationGcConfig, PublicationGcReport,
};
use sha2::{Digest, Sha256};

fn usage() -> &'static str {
    "usage:
  preprocessor-cli build-product [--cycle <YYCC>] [--source-root <path>] [--build-root <path>] [--publish-label <label>] [--publish-timestamp <YYYYMMDDTHHMMSSZ>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli merge-current-artifacts [--source-root <path>] [--build-root <path>] [--as-of-utc <RFC3339 UTC>] --manifest <path> [--manifest <path>]...
  preprocessor-cli publish-discovery-manifest [--source-root <path>] [--build-root <path>] --as-of-utc <RFC3339 UTC> --bundle <filename> [--bundle <filename>]...
  preprocessor-cli gc [--build-root <path>] [--dry-run|--execute] [--grace-hours <count>]
  preprocessor-cli analyze-obstacle-thresholds --input-dir <path> [--cap <count>] [--min-zoom <z>] [--max-zoom <z>] [--step-ft <count>]
  preprocessor-cli normalize-swim-notams --input-jsonl <path> --output-dir <path> --version-label <label>

Use --long-help to show internal/debug commands."
}

fn long_usage() -> &'static str {
    "usage:
  preprocessor-cli compare-provenance --left-provenance-dir <path> --right-provenance-dir <path>
  preprocessor-cli audit-cifp-tpp-matching [--artifact-root <path>] [--bundle <path>] [--limit <count>]
  preprocessor-cli audit-terrain-airports --nav-db <path> --dem-vrt <path> --geo-csv <path> --output-dir <path> [--bbox <west,south,east,north>] [--limit <count>]
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
  preprocessor-cli build-data --input-dir <path> --output-dir <path> --manifest-version <cycle> [--resource-index-output <path>] [--chart-source <family-id>:<package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]... [--tpp-source <package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]... [--csup-source <package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]...
  preprocessor-cli audit-procedure-geometry --main-db <path> [--airport <id>] [--procedure <id>] [--transition <id>]
  preprocessor-cli build-vectors --main-db <path> --output-dir <path> --version-label <label> [--data-input-dir <path>] [--include-class-e-airspace]
  preprocessor-cli audit-bravo-unions --class-airspace-shp <path> --output-svg <path> [--version-label <label>]
  preprocessor-cli audit-class-airspace-simplification --class-airspace-shp <path> [--tolerances-degrees <csv>] [--ident <id>]
  preprocessor-cli build-obstacles [--build-root <path>] [--fetch-jobs <count>] [--snapshot-date <YYYY-MM-DD>]
  preprocessor-cli build-obstacles-from-input --input-dir <path> --output-dir <path> --version-label <label> [--generated-at-utc <RFC3339 UTC>]
  preprocessor-cli analyze-obstacle-thresholds --input-dir <path> [--cap <count>] [--min-zoom <z>] [--max-zoom <z>] [--step-ft <count>]
  preprocessor-cli normalize-swim-notams --input-jsonl <path> --output-dir <path> --version-label <label>
  preprocessor-cli build-resource-index --nav-db-zip <path> --output <path> [--chart-source <family-id>:<package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]... [--tpp-source <package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]... [--csup-source <package_outputs_jsonl>:<asset_root>:<package_root>:<unpack_source_root>]...
  preprocessor-cli build-cycle [--cycle <YYCC>] [--source-root <path>] [--build-root <path>] [--publish-label <label>] [--publish-timestamp <YYYYMMDDTHHMMSSZ>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli build-product [--cycle <YYCC>] [--source-root <path>] [--build-root <path>] [--publish-label <label>] [--publish-timestamp <YYYYMMDDTHHMMSSZ>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli publish-discovery-manifest [--source-root <path>] [--build-root <path>] --as-of-utc <RFC3339 UTC> --bundle <filename> [--bundle <filename>]...
  preprocessor-cli gc [--build-root <path>] [--dry-run|--execute] [--grace-hours <count>]
  preprocessor-cli gc-build-cache [--build-root <path>] [--dry-run|--execute] [--grace-hours <count>] [--bootstrap-from-build-manifests]
  preprocessor-cli gc-publication [--build-root <path>] [--dry-run|--execute] [--grace-hours <count>]
  preprocessor-cli gc-fetch-cache [--build-root <path>] [--dry-run|--execute] [--grace-hours <count>]
  preprocessor-cli explain-product-build [--source-root <path>] [--build-root <path>] [--fetch-jobs <count>] [--cpu-jobs <count>] [--max-heavy-jobs <count>]
  preprocessor-cli run-chart --family <sec|tac|enr-l|enr-h> --source-repo <path> --run-root <path> [--prefetch-source-urls <path>] [--fetch-jobs <count>]"
}

fn collect_workspace_hash_inputs(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, path: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if child
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "target" || name == ".git")
                {
                    continue;
                }
                walk(root, &child, files);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let include = child
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "rs")
                || child
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "Cargo.toml" || name == "Cargo.lock");
            if include {
                files.push(
                    child
                        .strip_prefix(root)
                        .expect("hashed file should live under workspace root")
                        .to_path_buf(),
                );
            }
        }
    }

    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

fn hash_preprocessor_workspace(root: &Path) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    for relative in collect_workspace_hash_inputs(root) {
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(
            fs::read(root.join(&relative))
                .with_context(|| format!("failed to read {}", root.join(&relative).display()))?,
        );
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn ensure_binary_matches_workspace() -> anyhow::Result<()> {
    let expected = env!("PREPROCESSOR_WORKSPACE_HASH");
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .to_path_buf();
    if !workspace_root.exists() {
        return Ok(());
    }
    let actual = hash_preprocessor_workspace(&workspace_root)?;
    if actual == expected {
        return Ok(());
    }
    let binary = env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    anyhow::bail!(
        "binary/source mismatch: {} was built from workspace hash {} but current source tree is {}; rebuild preprocessor-cli from {} before running mutating commands",
        binary,
        expected,
        actual,
        workspace_root.display()
    );
}

fn obstacle_snapshot_label(value: &str) -> anyhow::Result<String> {
    Ok(NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("failed to parse obstacle snapshot date {value}"))?
        .format("%Y.%m.%d")
        .to_string())
}

fn fetch_cache_config_from_root(root: PathBuf) -> anyhow::Result<FetchCacheConfig> {
    Ok(FetchCacheConfig {
        root,
        mode: FetchCacheMode::parse(
            &env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string()),
        )?,
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
            .join("cache")
            .join("obstacles")
            .join(&snapshot_label)
    });
    let output_dir = build_root.join("output");
    let had_dir = output_dir.join("had");
    let manifest_path = had_dir.join("manifest.json");
    let stats_path = had_dir.join("stats.json");
    let zip_path = had_dir.join(format!("obstacles_{snapshot_label}.zip"));
    if manifest_path.is_file() && stats_path.is_file() && zip_path.is_file() {
        return Ok((manifest_path, stats_path, zip_path));
    }

    let fetch_cache_root = env::var("FETCH_CACHE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| artifact_root.join("cache").join("fetch"));
    let fetch_cache = fetch_cache_config_from_root(fetch_cache_root)?;

    let work_dir = build_root.join("work");
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    let provenance_dir = build_root.join("meta").join("provenance").join("obstacles");
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    let obstacle_url = "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP";
    let logical_file_name = format!("obstacle_{snapshot_label}.zip");
    let request = PrefetchRequest::new(obstacle_url)
        .with_logical_file_name(&logical_file_name)
        .with_cache_key(format!("{obstacle_url}#logical_name={logical_file_name}"));
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"obstacles\",\"url\":\"{}\",\"logical_file_name\":\"{}\",\"cache_key\":\"{}\"}}\n",
            request.url, logical_file_name, request.cache_key
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    prefetch_archives_with_provenance(
        std::slice::from_ref(&request),
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
        generated_at_utc: None,
    })?;
    Ok((result.manifest_path, result.stats_path, result.zip_path))
}

fn run_build_obstacles_from_input_command(
    args: &[String],
) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let mut input_dir = None;
    let mut output_dir = None;
    let mut version_label = None;
    let mut generated_at_utc = None;
    let mut index = 0;
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
            Some("--version-label") => {
                version_label = Some(
                    args.get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                );
                index += 2;
            }
            Some("--generated-at-utc") => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
                generated_at_utc = Some(
                    DateTime::parse_from_rfc3339(value)
                        .with_context(|| format!("failed to parse generated-at UTC {value}"))?
                        .with_timezone(&Utc),
                );
                index += 2;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }
    let result = build_obstacle_dataset(&BuildObstacleDatasetRequest {
        input_dir: input_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        output_dir: output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        version_label: version_label.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        generated_at_utc,
    })?;
    Ok((result.manifest_path, result.stats_path, result.zip_path))
}

fn run_analyze_obstacle_thresholds_command(args: &[String]) -> anyhow::Result<()> {
    let mut input_dir = None;
    let mut cap_per_tile = 100_usize;
    let mut min_zoom = 0_u8;
    let mut max_zoom = 12_u8;
    let mut step_ft = 50_i32;
    let mut index = 0;
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
            Some("--cap") => {
                cap_per_tile = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                    .parse()
                    .context("failed to parse cap")?;
                index += 2;
            }
            Some("--min-zoom") => {
                min_zoom = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                    .parse()
                    .context("failed to parse min zoom")?;
                index += 2;
            }
            Some("--max-zoom") => {
                max_zoom = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                    .parse()
                    .context("failed to parse max zoom")?;
                index += 2;
            }
            Some("--step-ft") => {
                step_ft = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                    .parse()
                    .context("failed to parse step-ft")?;
                index += 2;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }

    let rows = analyze_obstacle_thresholds(&AnalyzeObstacleThresholdsRequest {
        input_dir: input_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        cap_per_tile,
        min_zoom,
        max_zoom,
        threshold_step_ft: step_ft,
    })?;

    println!("rust_table [");
    for row in &rows {
        println!("    ({}, {}),", row.zoom, row.min_agl_ft);
    }
    println!("]");
    println!();
    println!(
        "{:>4} {:>11} {:>20} {:>12} {:>14}",
        "zoom", "min_agl_ft", "max_points_per_tile", "kept_points", "nonempty_tiles"
    );
    for row in &rows {
        println!(
            "{:>4} {:>11} {:>20} {:>12} {:>14}",
            row.zoom, row.min_agl_ft, row.max_points_per_tile, row.kept_points, row.nonempty_tiles
        );
    }
    Ok(())
}

fn run_normalize_swim_notams_command(
    args: &[String],
) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let mut input_jsonl = None;
    let mut output_dir = None;
    let mut version_label = None;
    let mut index = 0;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--input-jsonl") => {
                input_jsonl = Some(PathBuf::from(
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

    let result = build_notam_dataset(&BuildNotamRequest {
        input_jsonl_path: input_jsonl.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        output_dir: output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        version_label: version_label.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
        generated_at_utc: Utc::now(),
    })?;
    Ok((
        result.manifest_path,
        result.structured_json_path,
        result.zip_path,
    ))
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
    let mut parts = value.splitn(6, ':');
    let family_id = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing family id"))?;
    let package_outputs_path = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing package outputs path"))?;
    let asset_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing asset root"))?;
    let package_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing package root"))?;
    let unpack_source_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chart source is missing unpack source root"))?;
    let source_urls_path = parts.next().filter(|part| !part.is_empty());
    Ok(ChartSource {
        family_id: family_id.to_string(),
        package_outputs_path: PathBuf::from(package_outputs_path),
        asset_root: PathBuf::from(asset_root),
        package_root: PathBuf::from(package_root),
        unpack_source_root: PathBuf::from(unpack_source_root),
        source_urls_path: source_urls_path.map(PathBuf::from),
    })
}

fn parse_asset_source_spec(value: &str) -> anyhow::Result<AssetSource> {
    let mut parts = value.splitn(5, ':');
    let package_outputs_path = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing package outputs path"))?;
    let asset_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing asset root"))?;
    let package_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing package root"))?;
    let unpack_source_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing unpack source root"))?;
    let source_urls_path = parts.next().filter(|part| !part.is_empty());
    Ok(AssetSource {
        package_outputs_path: PathBuf::from(package_outputs_path),
        asset_root: PathBuf::from(asset_root),
        package_root: PathBuf::from(package_root),
        unpack_source_root: PathBuf::from(unpack_source_root),
        source_urls_path: source_urls_path.map(PathBuf::from),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildDataCommand {
    input_dir: PathBuf,
    output_dir: PathBuf,
    manifest_version: String,
    resource_index_output: Option<PathBuf>,
    chart_sources: Vec<ChartSource>,
    tpp_sources: Vec<AssetSource>,
    csup_sources: Vec<AssetSource>,
}

fn parse_build_data_command(args: &[String]) -> anyhow::Result<BuildDataCommand> {
    let mut input_dir = None;
    let mut output_dir = None;
    let mut manifest_version = None;
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

fn audit_cifp_tpp_matching_command(
    artifact_root: &Path,
    explicit_bundle: Option<&Path>,
    limit: usize,
) -> anyhow::Result<()> {
    let bundle_path = choose_matching_bundle(artifact_root, explicit_bundle)?;
    let bundle = load_matching_bundle(&bundle_path)?;
    let db_path = resolve_matching_db_path(artifact_root, &bundle)?;
    let tpp_zips = tpp_zip_paths_from_bundle(artifact_root, &bundle)?;
    let report = audit_tpp_cifp_matching(&db_path, &tpp_zips)?;

    println!("bundle: {}", bundle_path.display());
    println!(
        "cycle: {}",
        bundle
            .get("cycle")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
    );
    println!("db: {}", db_path.display());
    println!("approach plates: {}", report.approach_plate_count);
    println!(
        "airports with approach plates: {}",
        report.airports_with_approach_plates
    );
    println!(
        "airports with CIFP approaches: {}",
        report.airports_with_cifp_approaches
    );
    println!();

    println!("count audit:");
    println!("  airports checked: {}", report.count_rows.len());
    println!("  exact count match: {}", report.exact_count_match);
    println!("  count mismatch: {}", report.count_mismatch);
    let mut mismatches = report
        .count_rows
        .iter()
        .filter(|(_, plates, cifp)| plates != cifp)
        .cloned()
        .collect::<Vec<_>>();
    mismatches.sort_by_key(|(airport, plate_count, cifp_count)| {
        (
            usize::MAX - plate_count.abs_diff(*cifp_count),
            airport.clone(),
        )
    });
    for (airport_id, plate_count, cifp_count) in mismatches.into_iter().take(limit) {
        println!(
            "  {}: plates={} cifp_iaps={}",
            airport_id, plate_count, cifp_count
        );
    }
    println!();

    println!("heuristic match audit:");
    println!("  matched_unique: {}", report.match_summary.matched_unique);
    println!(
        "  matched_partial: {}",
        report.match_summary.matched_partial
    );
    println!("  matched_none: {}", report.match_summary.matched_none);
    println!("  no_heuristic: {}", report.match_summary.no_heuristic);
    println!(
        "  airport_missing_from_cifp: {}",
        report.match_summary.airport_missing_from_cifp
    );
    println!();

    println!("relation audit:");
    println!(
        "  airports_considered: {}",
        report.relation_summary.airports_considered
    );
    println!(
        "  airports_with_no_unresolved_cids: {}",
        report.relation_summary.airports_with_no_unresolved_cids
    );
    println!(
        "  airports_with_unresolved_cids: {}",
        report.relation_summary.airports_with_unresolved_cids
    );
    println!(
        "  uniquely_bound_cids_total: {}",
        report.relation_summary.uniquely_bound_cids_total
    );
    println!(
        "  multiply_bound_cids_total: {}",
        report.relation_summary.multiply_bound_cids_total
    );
    println!(
        "  copter_only_cids_total: {}",
        report.relation_summary.copter_only_cids_total
    );
    println!(
        "  unresolved_cids_total: {}",
        report.relation_summary.unresolved_cids_total
    );
    println!(
        "  ignored_noheur_plates_total: {}",
        report.relation_summary.ignored_noheur_plates_total
    );
    println!(
        "  ignored_nomatch_plates_total: {}",
        report.relation_summary.ignored_nomatch_plates_total
    );
    println!();

    println!("sample difficult cases:");
    for example in report.match_examples.iter().take(limit) {
        println!(
            "  {} {} candidate_groups={:?} matched={:?}",
            example.airport_id, example.label, example.candidate_groups, example.matched
        );
    }
    println!();

    println!("sample relation exceptions:");
    for example in report.relation_examples.iter().take(limit) {
        println!(
            "  {} unresolved={} multiply_bound={} unresolved_cids={:?}",
            example.airport,
            example.unresolved_count,
            example.multiply_bound,
            example.unresolved_cids
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TerrainAuditPoint {
    source: String,
    airport_id: String,
    airport_type: String,
    point_id: String,
    latitude: f64,
    longitude: f64,
    charted_elevation_ft_msl: f64,
}

#[derive(Debug, Clone)]
struct TerrainAuditRow {
    point: TerrainAuditPoint,
    dem_height_ft_msl: f64,
    geoid_height_ft: f64,
    transformed_height_ft_wgs84_ellipsoid: f64,
    residual_ft: f64,
}

fn audit_terrain_airports_command(args: &[String]) -> anyhow::Result<()> {
    let mut nav_db = None;
    let mut dem_vrt = None;
    let mut geo_csv = None;
    let mut output_dir = None;
    let mut bbox = None;
    let mut include_heliports = false;
    let mut limit = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--nav-db" => {
                nav_db = Some(PathBuf::from(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                i += 2;
            }
            "--dem-vrt" => {
                dem_vrt = Some(PathBuf::from(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                i += 2;
            }
            "--geo-csv" => {
                geo_csv = Some(PathBuf::from(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                i += 2;
            }
            "--output-dir" => {
                output_dir = Some(PathBuf::from(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                ));
                i += 2;
            }
            "--bbox" => {
                bbox = Some(parse_bbox(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                )?);
                i += 2;
            }
            "--limit" => {
                limit = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                        .parse::<usize>()
                        .context("failed to parse --limit")?,
                );
                i += 2;
            }
            "--include-heliports" => {
                include_heliports = true;
                i += 1;
            }
            _ => anyhow::bail!("{}", usage()),
        }
    }

    let nav_db = nav_db.ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
    let dem_vrt = dem_vrt.ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
    let geo_csv = geo_csv.ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
    let output_dir = output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let geoid_grid = GeoidGrid::from_geo_csv(&geo_csv)?;
    let points = load_terrain_audit_points(&nav_db, bbox, include_heliports, limit)?;
    let mut rows = Vec::new();
    for point in points {
        let dem_height_msl_m = probe_dem_meters(&dem_vrt, point.longitude, point.latitude)
            .with_context(|| {
                format!(
                    "failed to probe DEM for {} {} at {},{}",
                    point.airport_id, point.point_id, point.latitude, point.longitude
                )
            })?;
        let dem_height_ft_msl = dem_height_msl_m * 3.280_839_895;
        let transformed_height_ft_wgs84_ellipsoid =
            terrain_ellipsoid_height_feet_from_navd88_meters(
                dem_height_msl_m,
                point.latitude,
                point.longitude,
                &geoid_grid,
            );
        let geoid_height_ft = transformed_height_ft_wgs84_ellipsoid - dem_height_ft_msl;
        let charted_elevation_ft_wgs84_ellipsoid = point.charted_elevation_ft_msl + geoid_height_ft;
        rows.push(TerrainAuditRow {
            point,
            dem_height_ft_msl,
            geoid_height_ft,
            transformed_height_ft_wgs84_ellipsoid,
            residual_ft: transformed_height_ft_wgs84_ellipsoid
                - charted_elevation_ft_wgs84_ellipsoid,
        });
    }

    let csv_path = output_dir.join("terrain_airport_audit.csv");
    write_terrain_audit_csv(&csv_path, &rows)?;
    let svg_path = output_dir.join("terrain_airport_scatter.svg");
    write_terrain_audit_svg(&svg_path, &rows)?;
    println!("points {}", rows.len());
    println!("csv {}", csv_path.display());
    println!("scatter {}", svg_path.display());
    if !rows.is_empty() {
        let mean_abs =
            rows.iter().map(|row| row.residual_ft.abs()).sum::<f64>() / rows.len() as f64;
        let max_abs = rows
            .iter()
            .map(|row| row.residual_ft.abs())
            .fold(0.0, f64::max);
        println!("mean_abs_residual_ft {:.1}", mean_abs);
        println!("max_abs_residual_ft {:.1}", max_abs);
    }
    if !include_heliports {
        println!(
            "heliports skipped; pass --include-heliports to audit rooftop/pad elevations separately"
        );
    }
    Ok(())
}

fn load_terrain_audit_points(
    nav_db: &Path,
    bbox: Option<(f64, f64, f64, f64)>,
    include_heliports: bool,
    limit: Option<usize>,
) -> anyhow::Result<Vec<TerrainAuditPoint>> {
    let conn = rusqlite::Connection::open(nav_db)
        .with_context(|| format!("failed to open {}", nav_db.display()))?;
    let mut points = Vec::new();

    let mut airports = conn.prepare(
        "select LocationID, Type, ARPLatitude, ARPLongitude, ARPElevation
         from airports
         where ARPLatitude is not null and ARPLongitude is not null and trim(ARPElevation) != ''
         order by LocationID",
    )?;
    let airport_rows = airports.query_map([], |row| {
        Ok(TerrainAuditPoint {
            source: "airport_arp".to_string(),
            airport_id: row.get::<_, String>(0)?,
            airport_type: row.get::<_, String>(1)?,
            point_id: "ARP".to_string(),
            latitude: row.get::<_, f64>(2)?,
            longitude: row.get::<_, f64>(3)?,
            charted_elevation_ft_msl: parse_sql_text_f64(row.get::<_, String>(4)?),
        })
    })?;
    for point in airport_rows {
        let point = point?;
        if !include_heliports && point.airport_type == "HELIPORT" {
            continue;
        }
        if point.charted_elevation_ft_msl.is_finite()
            && point_is_inside_bbox(point.longitude, point.latitude, bbox)
        {
            points.push(point);
        }
        if limit.is_some_and(|limit| points.len() >= limit) {
            return Ok(points);
        }
    }

    let mut runways = conn.prepare(
        "select r.LocationID, a.Type, r.LEIdent, r.LELatitude, r.LELongitude, r.LEElevation,
                r.HEIdent, r.HELatitude, r.HELongitude, r.HEElevation
         from airportrunways r
         join airports a on a.LocationID = r.LocationID
         order by r.LocationID, r.LEIdent, r.HEIdent",
    )?;
    let runway_rows = runways.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
        ))
    })?;
    for row in runway_rows {
        let (
            airport_id,
            airport_type,
            le_ident,
            le_lat,
            le_lon,
            le_elev,
            he_ident,
            he_lat,
            he_lon,
            he_elev,
        ) = row?;
        if !include_heliports && airport_type == "HELIPORT" {
            continue;
        }
        for (ident, lat, lon, elevation) in [
            (le_ident, le_lat, le_lon, le_elev),
            (he_ident, he_lat, he_lon, he_elev),
        ] {
            let Some(latitude) = parse_optional_f64(&lat) else {
                continue;
            };
            let Some(longitude) = parse_optional_f64(&lon) else {
                continue;
            };
            let Some(charted_elevation_ft_msl) = parse_optional_f64(&elevation) else {
                continue;
            };
            if !point_is_inside_bbox(longitude, latitude, bbox) {
                continue;
            }
            points.push(TerrainAuditPoint {
                source: "runway_endpoint".to_string(),
                airport_id: airport_id.clone(),
                airport_type: airport_type.clone(),
                point_id: format!("RWY-{ident}"),
                latitude,
                longitude,
                charted_elevation_ft_msl,
            });
            if limit.is_some_and(|limit| points.len() >= limit) {
                return Ok(points);
            }
        }
    }
    Ok(points)
}

fn parse_sql_text_f64(value: String) -> f64 {
    parse_optional_f64(&value).unwrap_or(f64::NAN)
}

fn parse_optional_f64(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

fn parse_bbox(value: &str) -> anyhow::Result<(f64, f64, f64, f64)> {
    let parts = value.split(',').collect::<Vec<_>>();
    if parts.len() != 4 {
        anyhow::bail!("--bbox must be west,south,east,north");
    }
    let west = parts[0]
        .parse::<f64>()
        .context("failed to parse bbox west")?;
    let south = parts[1]
        .parse::<f64>()
        .context("failed to parse bbox south")?;
    let east = parts[2]
        .parse::<f64>()
        .context("failed to parse bbox east")?;
    let north = parts[3]
        .parse::<f64>()
        .context("failed to parse bbox north")?;
    if west >= east || south >= north {
        anyhow::bail!("--bbox must satisfy west < east and south < north");
    }
    Ok((west, south, east, north))
}

fn point_is_inside_bbox(longitude: f64, latitude: f64, bbox: Option<(f64, f64, f64, f64)>) -> bool {
    let Some((west, south, east, north)) = bbox else {
        return true;
    };
    longitude >= west && longitude <= east && latitude >= south && latitude <= north
}

fn probe_dem_meters(dem_vrt: &Path, longitude: f64, latitude: f64) -> anyhow::Result<f64> {
    let output = Command::new("gdallocationinfo")
        .arg("-valonly")
        .arg("-wgs84")
        .arg(dem_vrt)
        .arg(longitude.to_string())
        .arg(latitude.to_string())
        .output()
        .with_context(|| format!("failed to run gdallocationinfo on {}", dem_vrt.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "gdallocationinfo failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8(output.stdout).context("gdallocationinfo stdout not utf-8")?;
    stdout
        .split_whitespace()
        .find_map(|part| part.parse::<f64>().ok())
        .ok_or_else(|| anyhow::anyhow!("gdallocationinfo produced no numeric value: {stdout:?}"))
}

fn write_terrain_audit_csv(path: &Path, rows: &[TerrainAuditRow]) -> anyhow::Result<()> {
    let mut out = String::from("source,airport_id,airport_type,point_id,latitude,longitude,charted_elevation_ft_msl,dem_height_ft_msl,geoid_height_ft,transformed_height_ft_wgs84_ellipsoid,residual_ft\n");
    for row in rows {
        out.push_str(&format!(
            "{},{},{},{},{:.8},{:.8},{:.2},{:.2},{:.2},{:.2},{:.2}\n",
            row.point.source,
            row.point.airport_id,
            row.point.airport_type,
            row.point.point_id,
            row.point.latitude,
            row.point.longitude,
            row.point.charted_elevation_ft_msl,
            row.dem_height_ft_msl,
            row.geoid_height_ft,
            row.transformed_height_ft_wgs84_ellipsoid,
            row.residual_ft
        ));
    }
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))
}

fn write_terrain_audit_svg(path: &Path, rows: &[TerrainAuditRow]) -> anyhow::Result<()> {
    let width = 900.0;
    let height = 700.0;
    let pad = 70.0;
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for row in rows {
        let x = row.point.charted_elevation_ft_msl + row.geoid_height_ft;
        let y = row.transformed_height_ft_wgs84_ellipsoid;
        min_v = min_v.min(x).min(y);
        max_v = max_v.max(x).max(y);
    }
    if !min_v.is_finite() || !max_v.is_finite() || (max_v - min_v).abs() < 1.0 {
        min_v = 0.0;
        max_v = 1.0;
    }
    let span = max_v - min_v;
    let sx = |value: f64| pad + ((value - min_v) / span) * (width - 2.0 * pad);
    let sy = |value: f64| height - pad - ((value - min_v) / span) * (height - 2.0 * pad);
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="white"/>
<text x="{pad}" y="35" font-family="sans-serif" font-size="22">Terrain DEM airport scatter</text>
<line x1="{pad}" y1="{}" x2="{}" y2="{}" stroke="#333" stroke-width="1"/>
<line x1="{pad}" y1="{}" x2="{pad}" y2="{pad}" stroke="#333" stroke-width="1"/>
<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#999" stroke-width="1" stroke-dasharray="6 4"/>
"##,
        height - pad,
        width - pad,
        height - pad,
        height - pad,
        sx(min_v),
        sy(min_v),
        sx(max_v),
        sy(max_v),
    );
    for row in rows {
        let x_value = row.point.charted_elevation_ft_msl + row.geoid_height_ft;
        let y_value = row.transformed_height_ft_wgs84_ellipsoid;
        let color = if row.point.source == "runway_endpoint" {
            "#0b7285"
        } else {
            "#c92a2a"
        };
        svg.push_str(&format!(
            r##"<circle cx="{:.2}" cy="{:.2}" r="2.2" fill="{color}"><title>{} {} residual {:.1} ft</title></circle>
"##,
            sx(x_value),
            sy(y_value),
            row.point.airport_id,
            row.point.point_id,
            row.residual_ft
        ));
    }
    svg.push_str(&format!(
        r##"<text x="{pad}" y="{}" font-family="sans-serif" font-size="13">x: charted elevation + geoid height (ft WGS84 ellipsoid)</text>
<text x="{pad}" y="{}" font-family="sans-serif" font-size="13">y: DEM probe + geoid height (ft WGS84 ellipsoid)</text>
<text x="{pad}" y="{}" font-family="sans-serif" font-size="13">red: ARP, blue: runway endpoint</text>
</svg>
"##,
        height - 34.0,
        height - 18.0,
        height - 2.0
    ));
    fs::write(path, svg).with_context(|| format!("failed to write {}", path.display()))
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

struct FullGcConfig {
    build_root: PathBuf,
    mode: BuildCacheGcMode,
    grace_hours: u64,
}

fn full_gc_config_from_args(args: &[String]) -> anyhow::Result<FullGcConfig> {
    let mut base = ProductBuildConfig::from_env_and_args(&[])?;
    let mut mode = BuildCacheGcMode::Execute;
    let mut grace_hours = 24_u64;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--build-root" => {
                base.build_root = PathBuf::from(
                    args.get(index + 1)
                        .context("missing value for --build-root")?,
                );
                index += 2;
            }
            "--dry-run" => {
                mode = BuildCacheGcMode::DryRun;
                index += 1;
            }
            "--execute" => {
                mode = BuildCacheGcMode::Execute;
                index += 1;
            }
            "--grace-hours" => {
                grace_hours = args
                    .get(index + 1)
                    .context("missing value for --grace-hours")?
                    .parse()
                    .context("failed to parse --grace-hours")?;
                index += 2;
            }
            other => anyhow::bail!("unknown gc argument: {other}"),
        }
    }
    Ok(FullGcConfig {
        build_root: base.build_root,
        mode,
        grace_hours,
    })
}

fn build_cache_gc_config_from_args(args: &[String]) -> anyhow::Result<BuildCacheGcConfig> {
    let mut base = ProductBuildConfig::from_env_and_args(&[])?;
    let mut mode = BuildCacheGcMode::DryRun;
    let mut grace_hours = 24_u64;
    let mut bootstrap_from_build_manifests = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--build-root" => {
                base.build_root = PathBuf::from(
                    args.get(index + 1)
                        .context("missing value for --build-root")?,
                );
                index += 2;
            }
            "--dry-run" => {
                mode = BuildCacheGcMode::DryRun;
                index += 1;
            }
            "--execute" => {
                mode = BuildCacheGcMode::Execute;
                index += 1;
            }
            "--grace-hours" => {
                grace_hours = args
                    .get(index + 1)
                    .context("missing value for --grace-hours")?
                    .parse()
                    .context("failed to parse --grace-hours")?;
                index += 2;
            }
            "--bootstrap-from-build-manifests" => {
                bootstrap_from_build_manifests = true;
                index += 1;
            }
            other => anyhow::bail!("unknown gc-build-cache argument: {other}"),
        }
    }
    Ok(BuildCacheGcConfig {
        build_root: base.build_root,
        mode,
        grace_hours,
        bootstrap_from_build_manifests,
    })
}

fn publication_gc_config_from_args(args: &[String]) -> anyhow::Result<PublicationGcConfig> {
    let (build_root, mode, grace_hours) = basic_gc_args(args, "gc-publication")?;
    Ok(PublicationGcConfig {
        build_root,
        mode,
        grace_hours,
    })
}

fn fetch_cache_gc_config_from_args(args: &[String]) -> anyhow::Result<FetchCacheGcConfig> {
    let (build_root, mode, grace_hours) = basic_gc_args(args, "gc-fetch-cache")?;
    Ok(FetchCacheGcConfig {
        build_root,
        mode,
        grace_hours,
    })
}

fn basic_gc_args(
    args: &[String],
    command: &str,
) -> anyhow::Result<(PathBuf, BuildCacheGcMode, u64)> {
    let base = ProductBuildConfig::from_env_and_args(&[])?;
    let mut build_root = base.build_root;
    let mut mode = BuildCacheGcMode::DryRun;
    let mut grace_hours = 24_u64;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--build-root" => {
                build_root = PathBuf::from(
                    args.get(index + 1)
                        .context("missing value for --build-root")?,
                );
                index += 2;
            }
            "--dry-run" => {
                mode = BuildCacheGcMode::DryRun;
                index += 1;
            }
            "--execute" => {
                mode = BuildCacheGcMode::Execute;
                index += 1;
            }
            "--grace-hours" => {
                grace_hours = args
                    .get(index + 1)
                    .context("missing value for --grace-hours")?
                    .parse()
                    .context("failed to parse --grace-hours")?;
                index += 2;
            }
            other => anyhow::bail!("unknown {command} argument: {other}"),
        }
    }
    Ok((build_root, mode, grace_hours))
}

fn gc_mode_name(mode: BuildCacheGcMode) -> &'static str {
    if mode == BuildCacheGcMode::Execute {
        "execute"
    } else {
        "dry-run"
    }
}

fn print_build_cache_gc_report(mode: BuildCacheGcMode, report: BuildCacheGcReport) {
    println!("roots {}", report.roots_path.display());
    println!("mode {}", gc_mode_name(mode));
    println!("rooted_nodes {}", report.rooted_nodes);
    println!("scanned_nodes {}", report.scanned_nodes);
    println!("active_nodes {}", report.active_nodes);
    println!("stale_lock_nodes {}", report.stale_lock_nodes);
    println!("grace_nodes {}", report.grace_nodes);
    println!("evictable_nodes {}", report.evictable_nodes);
    println!("reclaimable_bytes {}", report.reclaimed_bytes);
    println!(
        "reclaimable_gib {:.2}",
        report.reclaimed_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    println!("scratch_files {}", report.scratch_files);
    println!("scratch_reclaimable_bytes {}", report.scratch_bytes);
    println!(
        "scratch_reclaimable_gib {:.2}",
        report.scratch_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    println!("scratch_active_nodes {}", report.scratch_active_nodes);
    for (node_name, bucket) in report.by_node_name {
        println!(
            "candidate {} count={} bytes={} gib={:.2}",
            node_name,
            bucket.count,
            bucket.bytes,
            bucket.bytes as f64 / 1024.0 / 1024.0 / 1024.0
        );
    }
}

fn print_publication_gc_report(mode: BuildCacheGcMode, report: PublicationGcReport) {
    println!(
        "current_artifacts {}",
        report.current_artifacts_path.display()
    );
    println!("mode {}", gc_mode_name(mode));
    println!("current_publish_roots {}", report.current_publish_roots);
    println!("scanned_publish_roots {}", report.scanned_publish_roots);
    println!("grace_roots {}", report.grace_roots);
    println!("evictable_roots {}", report.evictable_roots);
    println!("reclaimable_bytes {}", report.reclaimed_bytes);
    println!(
        "reclaimable_gib {:.2}",
        report.reclaimed_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    for candidate in report.candidates {
        println!(
            "candidate {} bytes={} gib={:.2}",
            candidate.path.display(),
            candidate.bytes,
            candidate.bytes as f64 / 1024.0 / 1024.0 / 1024.0
        );
    }
}

fn print_fetch_cache_gc_report(mode: BuildCacheGcMode, report: FetchCacheGcReport) {
    println!(
        "current_artifacts {}",
        report.current_artifacts_path.display()
    );
    println!("mode {}", gc_mode_name(mode));
    println!("build_manifests {}", report.build_manifests);
    println!("rooted_fetch_refs {}", report.rooted_fetch_refs);
    println!("rooted_blobs {}", report.rooted_blobs);
    println!("missing_fetch_refs {}", report.missing_fetch_refs);
    println!("scanned_metadata {}", report.scanned_metadata);
    println!("scanned_blobs {}", report.scanned_blobs);
    println!("grace_metadata {}", report.grace_metadata);
    println!("grace_blobs {}", report.grace_blobs);
    println!("evictable_metadata {}", report.evictable_metadata);
    println!("evictable_blobs {}", report.evictable_blobs);
    println!("reclaimable_bytes {}", report.reclaimed_bytes);
    println!(
        "reclaimable_gib {:.2}",
        report.reclaimed_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    for candidate in report.candidates.iter().take(50) {
        let kind = match candidate.kind {
            FetchCacheGcCandidateKind::Metadata => "metadata",
            FetchCacheGcCandidateKind::Blob => "blob",
        };
        println!(
            "candidate {} {} bytes={} gib={:.2}",
            kind,
            candidate.path.display(),
            candidate.bytes,
            candidate.bytes as f64 / 1024.0 / 1024.0 / 1024.0
        );
    }
    if report.candidates.len() > 50 {
        println!("candidate_truncated {}", report.candidates.len() - 50);
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if matches!(args.get(1).map(String::as_str), Some("--help" | "-h")) {
        println!("{}", usage());
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--long-help") {
        println!("{}", long_usage());
        return Ok(());
    }
    if matches!(
        args.get(1).map(String::as_str),
        Some(
            "build-vrts"
                | "build-tiles"
                | "package-regions"
                | "run-native-chart"
                | "run-native-csup"
                | "run-native-tpp"
                | "build-resource-index"
                | "build-data"
                | "build-vectors"
                | "audit-bravo-unions"
                | "audit-class-airspace-simplification"
                | "build-obstacles"
                | "build-obstacles-from-input"
                | "build-cycle"
                | "build-product"
                | "gc"
                | "audit-terrain-airports"
                | "run-chart"
        )
    ) {
        ensure_binary_matches_workspace()?;
    }

    match args.get(1).map(String::as_str) {
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
        Some("audit-cifp-tpp-matching") => {
            let mut artifact_root = default_artifact_write_path(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli crate should live under workspace")
                    .parent()
                    .expect("workspace should live under product")
                    .parent()
                    .expect("product should live under repo root"),
            );
            let mut bundle = None;
            let mut limit = 20_usize;
            let mut index = 2;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--artifact-root") => {
                        artifact_root = PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        );
                        index += 2;
                    }
                    Some("--bundle") => {
                        bundle = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--limit") => {
                        limit = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .parse()
                            .context("failed to parse limit")?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            audit_cifp_tpp_matching_command(&artifact_root, bundle.as_deref(), limit)?;
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
            let mut data_input_dir = None;
            let mut output_dir = None;
            let mut version_label = None;
            let mut include_class_e_airspace = false;
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
                    Some("--data-input-dir") => {
                        data_input_dir = Some(PathBuf::from(
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
                    Some("--include-class-e-airspace") => {
                        include_class_e_airspace = true;
                        index += 1;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let request = BuildVectorsRequest {
                main_db: main_db.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                data_input_dir,
                output_dir: output_dir.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                version_label: version_label.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                include_class_e_airspace,
            };
            let result = build_vectors_dataset(&request)?;
            println!("manifest {}", result.manifest_path.display());
            println!("stats {}", result.stats_path.display());
            println!("errors {}", result.errors_path.display());
            println!("had_pairs {}", result.had_pairs_path.display());
        }
        Some("audit-bravo-unions") => {
            let mut class_airspace_shp = None;
            let mut output_svg = None;
            let mut version_label = "debug".to_string();
            let mut index = 2;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--class-airspace-shp") => {
                        class_airspace_shp = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--output-svg") => {
                        output_svg = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--version-label") => {
                        version_label = args
                            .get(index + 1)
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let request = BuildBravoUnionSvgRequest {
                class_airspace_shp: class_airspace_shp
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                output_svg: output_svg.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                version_label,
            };
            let result = build_bravo_union_svg(&request)?;
            println!("svg {}", result.output_svg.display());
            println!("bravos {}", result.bravo_count);
            println!("source_shelves {}", result.source_shelf_count);
            println!("union_polygons {}", result.union_polygon_count);
        }
        Some("audit-class-airspace-simplification") => {
            let mut class_airspace_shp = None;
            let mut ident = None;
            let mut tolerances_degrees =
                vec![0.0, 0.00005, 0.0001, 0.0002, 0.0005, 0.001, 0.002, 0.005];
            let mut index = 2;
            while index < args.len() {
                match args.get(index).map(String::as_str) {
                    Some("--class-airspace-shp") => {
                        class_airspace_shp = Some(PathBuf::from(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    Some("--tolerances-degrees") => {
                        tolerances_degrees = args
                            .get(index + 1)
                            .ok_or_else(|| anyhow::anyhow!("{}", usage()))?
                            .split(',')
                            .map(|value| value.parse::<f64>())
                            .collect::<Result<Vec<_>, _>>()
                            .context("failed to parse --tolerances-degrees")?;
                        index += 2;
                    }
                    Some("--ident") => {
                        ident = Some(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        );
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let rows =
                audit_class_airspace_simplification(&AuditClassAirspaceSimplificationRequest {
                    class_airspace_shp: class_airspace_shp
                        .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                    tolerances_degrees,
                    ident,
                })?;
            println!(
                "class tolerance_deg features source_points simplified_points source_mib rdp_mib rdp_reduction rdp_max_dev_ft rdp_max_dev_nm arc_primitives arc_lines arc_arcs arc_est_mib arc_reduction arc_max_dev_ft arc_max_dev_nm"
            );
            for row in rows {
                let source_mib = row.source_path_json_bytes as f64 / 1024.0 / 1024.0;
                let simplified_mib = row.simplified_path_json_bytes as f64 / 1024.0 / 1024.0;
                let rdp_reduction = if row.source_path_json_bytes == 0 {
                    0.0
                } else {
                    1.0 - row.simplified_path_json_bytes as f64 / row.source_path_json_bytes as f64
                };
                let arc_mib = row.arc_estimated_json_bytes as f64 / 1024.0 / 1024.0;
                let arc_reduction = if row.source_path_json_bytes == 0 {
                    0.0
                } else {
                    1.0 - row.arc_estimated_json_bytes as f64 / row.source_path_json_bytes as f64
                };
                println!(
                    "{} {:.6} {} {} {} {:.2} {:.2} {:.1}% {:.1} {:.4} {} {} {} {:.2} {:.1}% {:.1} {:.4}",
                    row.airspace_class,
                    row.tolerance_degrees,
                    row.feature_count,
                    row.source_points,
                    row.simplified_points,
                    source_mib,
                    simplified_mib,
                    rdp_reduction * 100.0,
                    row.max_deviation_ft,
                    row.max_deviation_ft / 6076.12,
                    row.arc_primitive_count,
                    row.arc_line_count,
                    row.arc_count,
                    arc_mib,
                    arc_reduction * 100.0,
                    row.arc_max_deviation_ft,
                    row.arc_max_deviation_ft / 6076.12
                );
            }
        }
        Some("build-obstacles") => {
            let (manifest_path, stats_path, zip_path) = run_build_obstacles_command(&args[2..])?;
            println!("manifest {}", manifest_path.display());
            println!("stats {}", stats_path.display());
            println!("zip {}", zip_path.display());
        }
        Some("build-obstacles-from-input") => {
            let (manifest_path, stats_path, zip_path) =
                run_build_obstacles_from_input_command(&args[2..])?;
            println!("manifest {}", manifest_path.display());
            println!("stats {}", stats_path.display());
            println!("zip {}", zip_path.display());
        }
        Some("analyze-obstacle-thresholds") => {
            run_analyze_obstacle_thresholds_command(&args[2..])?;
        }
        Some("normalize-swim-notams") => {
            let (manifest_path, structured_json_path, zip_path) =
                run_normalize_swim_notams_command(&args[2..])?;
            println!("manifest {}", manifest_path.display());
            println!("structured_json {}", structured_json_path.display());
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
            let result = build_product(&config)?;
            for cycle_manifest_path in result.cycle_manifest_paths {
                println!("cycle_manifest {}", cycle_manifest_path.display());
            }
            println!(
                "product_artifacts {}",
                result.product_artifacts_path.display()
            );
        }
        Some("merge-current-artifacts") => {
            let mut passthrough = Vec::new();
            let mut as_of_utc = None;
            let mut manifests = Vec::new();
            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--as-of-utc" => {
                        as_of_utc = Some(
                            DateTime::parse_from_rfc3339(
                                args.get(index + 1)
                                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                            )
                            .context("invalid --as-of-utc")?
                            .with_timezone(&Utc),
                        );
                        index += 2;
                    }
                    "--manifest" => {
                        manifests.push(PathBuf::from(
                            args.get(index + 1)
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        ));
                        index += 2;
                    }
                    _ => {
                        passthrough.push(args[index].clone());
                        index += 1;
                    }
                }
            }
            let config = ProductBuildConfig::from_env_and_args(&passthrough)?;
            let path = merge_current_artifacts_manifests(
                &config.build_root,
                as_of_utc.unwrap_or_else(Utc::now),
                &manifests,
            )?;
            println!("{}", path.display());
        }
        Some("audit-procedure-geometry") => {
            let mut main_db = None;
            let mut filter = ProcedureGeometryAuditFilter::default();
            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--main-db" => {
                        main_db = Some(PathBuf::from(
                            args.get(index + 1)
                                .ok_or_else(|| anyhow::anyhow!("{}", long_usage()))?,
                        ));
                        index += 2;
                    }
                    "--airport" => {
                        filter.airport_id = Some(
                            args.get(index + 1)
                                .ok_or_else(|| anyhow::anyhow!("{}", long_usage()))?
                                .clone(),
                        );
                        index += 2;
                    }
                    "--procedure" => {
                        filter.procedure_id = Some(
                            args.get(index + 1)
                                .ok_or_else(|| anyhow::anyhow!("{}", long_usage()))?
                                .clone(),
                        );
                        index += 2;
                    }
                    "--transition" => {
                        filter.enroute_transition = Some(
                            args.get(index + 1)
                                .ok_or_else(|| anyhow::anyhow!("{}", long_usage()))?
                                .clone(),
                        );
                        index += 2;
                    }
                    _ => return Err(anyhow::anyhow!("{}", long_usage())),
                }
            }
            let main_db = main_db.ok_or_else(|| anyhow::anyhow!("{}", long_usage()))?;
            let summary = audit_procedure_geometry_from_sqlite(&main_db, filter)?;
            println!("procedure_geometry_records {}", summary.record_count);
            println!(
                "procedure_geometry_records_with_data_quality {}",
                summary.records_with_data_quality
            );
            for (message, count) in summary.data_quality_messages {
                println!("procedure_geometry_data_quality {count} {message}");
            }
        }
        Some("publish-discovery-manifest") => {
            let config = ProductBuildConfig::from_env_and_args(&args[2..])?;
            let mut as_of_utc = None;
            let mut bundles = Vec::new();
            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--source-root" | "--build-root" | "--fetch-jobs" | "--cpu-jobs"
                    | "--max-heavy-jobs" | "--cycle" => {
                        index += 2;
                    }
                    "--as-of-utc" => {
                        as_of_utc = Some(
                            DateTime::parse_from_rfc3339(
                                args.get(index + 1)
                                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                            )
                            .context("invalid --as-of-utc")?
                            .with_timezone(&Utc),
                        );
                        index += 2;
                    }
                    "--bundle" => {
                        bundles.push(
                            args.get(index + 1)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                        );
                        index += 2;
                    }
                    _ => anyhow::bail!("{}", usage()),
                }
            }
            let path = publish_discovery_manifest(
                &config,
                as_of_utc.ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
                &bundles,
            )?;
            println!("{}", path.display());
        }
        Some("gc") => {
            let config = full_gc_config_from_args(&args[2..])?;
            println!("gc mode {}", gc_mode_name(config.mode));
            println!("gc build_root {}", config.build_root.display());
            println!("gc grace_hours {}", config.grace_hours);

            println!("gc step fetch-cache");
            let fetch_report = gc_fetch_cache(&FetchCacheGcConfig {
                build_root: config.build_root.clone(),
                mode: config.mode,
                grace_hours: config.grace_hours,
            })?;
            print_fetch_cache_gc_report(config.mode, fetch_report);

            println!("gc step publication");
            let publication_report = gc_publication(&PublicationGcConfig {
                build_root: config.build_root.clone(),
                mode: config.mode,
                grace_hours: config.grace_hours,
            })?;
            print_publication_gc_report(config.mode, publication_report);

            println!("gc step build-cache");
            let build_cache_report = gc_build_cache(&BuildCacheGcConfig {
                build_root: config.build_root,
                mode: config.mode,
                grace_hours: config.grace_hours,
                bootstrap_from_build_manifests: true,
            })?;
            print_build_cache_gc_report(config.mode, build_cache_report);
        }
        Some("gc-build-cache") => {
            let config = build_cache_gc_config_from_args(&args[2..])?;
            let mode = config.mode;
            let report = gc_build_cache(&config)?;
            print_build_cache_gc_report(mode, report);
        }
        Some("gc-publication") => {
            let config = publication_gc_config_from_args(&args[2..])?;
            let mode = config.mode;
            let report = gc_publication(&config)?;
            print_publication_gc_report(mode, report);
        }
        Some("gc-fetch-cache") => {
            let config = fetch_cache_gc_config_from_args(&args[2..])?;
            let mode = config.mode;
            let report = gc_fetch_cache(&config)?;
            print_fetch_cache_gc_report(mode, report);
        }
        Some("audit-terrain-airports") => {
            audit_terrain_airports_command(&args[2..])?;
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
            "sectional:/tmp/sec.jsonl:/tmp/sec-assets:/tmp/sec-packages:/tmp/sec-unpack"
                .to_string(),
            "--tpp-source".to_string(),
            "/tmp/tpp.jsonl:/tmp/tpp-assets:/tmp/tpp-packages:/tmp/tpp-unpack".to_string(),
            "--csup-source".to_string(),
            "/tmp/csup.jsonl:/tmp/csup-assets:/tmp/csup-packages:/tmp/csup-unpack".to_string(),
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
            PathBuf::from("/tmp/sec-packages")
        );
        assert_eq!(
            command.chart_sources[0].unpack_source_root,
            PathBuf::from("/tmp/sec-unpack")
        );
        assert_eq!(command.tpp_sources.len(), 1);
        assert_eq!(
            command.tpp_sources[0].package_outputs_path,
            PathBuf::from("/tmp/tpp.jsonl")
        );
        assert_eq!(
            command.tpp_sources[0].asset_root,
            PathBuf::from("/tmp/tpp-assets")
        );
        assert_eq!(
            command.tpp_sources[0].package_root,
            PathBuf::from("/tmp/tpp-packages")
        );
        assert_eq!(
            command.tpp_sources[0].unpack_source_root,
            PathBuf::from("/tmp/tpp-unpack")
        );
        assert_eq!(command.csup_sources.len(), 1);
        assert_eq!(
            command.csup_sources[0].package_outputs_path,
            PathBuf::from("/tmp/csup.jsonl")
        );
        assert_eq!(
            command.csup_sources[0].asset_root,
            PathBuf::from("/tmp/csup-assets")
        );
        assert_eq!(
            command.csup_sources[0].package_root,
            PathBuf::from("/tmp/csup-packages")
        );
        assert_eq!(
            command.csup_sources[0].unpack_source_root,
            PathBuf::from("/tmp/csup-unpack")
        );
    }

    #[test]
    fn parse_full_gc_defaults_to_execute() {
        let args = vec!["--build-root".to_string(), "/tmp/artifacts".to_string()];
        let config = full_gc_config_from_args(&args).expect("parse gc");
        assert_eq!(config.build_root, PathBuf::from("/tmp/artifacts"));
        assert_eq!(config.mode, BuildCacheGcMode::Execute);
        assert_eq!(config.grace_hours, 24);
    }

    #[test]
    fn parse_full_gc_accepts_dry_run_and_grace_hours() {
        let args = vec![
            "--build-root".to_string(),
            "/tmp/artifacts".to_string(),
            "--dry-run".to_string(),
            "--grace-hours".to_string(),
            "0".to_string(),
        ];
        let config = full_gc_config_from_args(&args).expect("parse gc");
        assert_eq!(config.mode, BuildCacheGcMode::DryRun);
        assert_eq!(config.grace_hours, 0);
    }
}
