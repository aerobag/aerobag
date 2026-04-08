mod full_validation;
mod emit_source_urls;

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use emit_source_urls::{compare_source_url_emission, emit_source_urls};
use full_validation::{FullValidationConfig, maybe_reexec_under_cgroup, run_full_validation};
use preprocessor_charts::{
    ChartRunRequest, NativeChartRunRequest, build_family_tiles, build_family_vrts,
    likely_current_bottleneck, package_family_regions, phase_plan, run_family,
    run_native_family,
};
use preprocessor_csup::{NativeCsupRunRequest, run_native_csup};
use preprocessor_data::{DataBuildRequest, build_data_package, compare_databases};
use preprocessor_tpp::{NativeTppRunRequest, run_native_tpp};
use preprocessor_core::{
    CaptureManifest, ChartFamily, ConcurrencyConfig, ExpectedTileCounts, Parallelism, Region,
    WorkKind,
};
use preprocessor_fetch::{
    CacheLayout, hash_text, manifest_path_for_run, manifest_summary, read_download_records,
    read_extract_records, read_source_url_set,
};
use preprocessor_resource_index::{
    AssetSource, BuildResourceIndexRequest, ChartSource, write_resource_index,
};
use preprocessor_tools::{comparison_targets, ToolInvocation};
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
  preprocessor-cli emit-source-urls --output-dir <path>
  preprocessor-cli compare-source-url-emission --repo-root <path> --avare-source-root <path> --work-dir <path>
  preprocessor-cli run-full-validation [--run-id <id>] [--validation-root <path>] [--fetch-cache-mode <fill|offline>] [--image-sample-percent <0-100>] [--image-rmse-threshold <0-1>]
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
  preprocessor-cli build-data --input-dir <path> --output-dir <path> --manifest-version <cycle>
  preprocessor-cli build-resource-index --nav-db-zip <path> --output <path> [--chart-source <family-id>:<package_outputs_jsonl>:<package_root>]... [--tpp-source <package_outputs_jsonl>:<asset_root>]... [--csup-source <package_outputs_jsonl>:<asset_root>]...
  preprocessor-cli run-chart --family <sec|tac|enr-l|enr-h> --source-repo <path> --run-root <path> [--prefetch-source-urls <path>] [--fetch-jobs <count>]"
}

fn count_lines(path: &PathBuf) -> anyhow::Result<u64> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text.lines().count() as u64)
}

fn hash_file(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
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
    let mut parts = value.splitn(3, ':');
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
    Ok(ChartSource {
        family_id: family_id.to_string(),
        package_outputs_path: PathBuf::from(package_outputs_path),
        package_root: PathBuf::from(package_root),
    })
}

fn parse_asset_source_spec(value: &str) -> anyhow::Result<AssetSource> {
    let mut parts = value.splitn(2, ':');
    let package_outputs_path = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing package outputs path"))?;
    let asset_root = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("asset source is missing asset root"))?;
    Ok(AssetSource {
        package_outputs_path: PathBuf::from(package_outputs_path),
        asset_root: PathBuf::from(asset_root),
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
    let left_only = left_paths.difference(&right_paths).cloned().collect::<Vec<_>>();
    let right_only = right_paths.difference(&left_paths).cloned().collect::<Vec<_>>();
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
        read_image_paths(left_root)?.into_iter().collect::<BTreeSet<_>>(),
        read_image_paths(right_root)?.into_iter().collect::<BTreeSet<_>>(),
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

fn parse_image_compare_options(args: &[String], start_index: usize) -> anyhow::Result<(u8, f64, Option<usize>)> {
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
        let legacy_set = legacy_paths.iter().cloned().collect::<std::collections::BTreeSet<_>>();
        let rust_set = rust_paths.iter().cloned().collect::<std::collections::BTreeSet<_>>();
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

fn compare_provenance(left_provenance_dir: &Path, right_provenance_dir: &Path) -> anyhow::Result<()> {
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
                let path = run_root.join("meta").join(format!("{label}.tile-paths.txt"));
                let actual = count_lines(&path)?;
                if let Some(expected) = family.baseline_tile_count() {
                    let status = if actual == expected { "match" } else { "mismatch" };
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
            println!("object {}", layout.object_metadata_path("example-source").display());
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
        Some("emit-source-urls") => {
            if args.get(2).map(String::as_str) != Some("--output-dir") {
                anyhow::bail!("{}", usage());
            }
            let output_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            for result in emit_source_urls(&output_dir)? {
                println!("{} {}", result.label, result.path.display());
            }
        }
        Some("compare-source-url-emission") => {
            if args.get(2).map(String::as_str) != Some("--repo-root")
                || args.get(4).map(String::as_str) != Some("--avare-source-root")
                || args.get(6).map(String::as_str) != Some("--work-dir")
            {
                anyhow::bail!("{}", usage());
            }
            let repo_root = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let avare_source_root = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let work_dir = PathBuf::from(
                args.get(7)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            compare_source_url_emission(&repo_root, &avare_source_root, &work_dir)?;
        }
        Some("run-full-validation") => {
            if maybe_reexec_under_cgroup(&args[2..])? {
                return Ok(());
            }
            let config = FullValidationConfig::from_env_and_args(&args[2..])?;
            run_full_validation(&config)?;
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
            if args.get(2).map(String::as_str) != Some("--input-dir")
                || args.get(4).map(String::as_str) != Some("--output-dir")
                || args.get(6).map(String::as_str) != Some("--manifest-version")
            {
                anyhow::bail!("{}", usage());
            }
            let input_dir = PathBuf::from(
                args.get(3)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let output_dir = PathBuf::from(
                args.get(5)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{}", usage()))?,
            );
            let manifest_version = args
                .get(7)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{}", usage()))?;
            let result = build_data_package(&DataBuildRequest {
                input_dir,
                output_dir,
                manifest_version,
            })?;
            println!("main_db {}", result.main_db.display());
            println!("manifest {}", result.manifest_path.display());
            println!("zip {}", result.zip_path.display());
            for (table, count) in result.row_counts {
                println!("table {} rows {}", table, count);
            }
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
