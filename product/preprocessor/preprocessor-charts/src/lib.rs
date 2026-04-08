use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use anyhow::{bail, Context};
use chrono::Datelike;
use preprocessor_core::{
    ChartFamily, ConcurrencyConfig, Parallelism, PhasePlan, Region, RegionBounds, RunPaths,
    WorkKind,
};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives, prefetch_archives_with_provenance,
    read_source_urls_jsonl, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_tools::{ToolInvocation, ToolOutcome};

#[derive(Debug, Clone)]
pub struct ChartRunRequest {
    pub family: ChartFamily,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
}

#[derive(Debug, Clone)]
pub struct ChartRunResult {
    pub family: ChartFamily,
    pub work_dir: PathBuf,
    pub outcome: ToolOutcome,
    pub tile_count: u64,
    pub prefetch_elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct VrtBuildResult {
    pub family: ChartFamily,
    pub work_dir: PathBuf,
    pub vrt_count: usize,
    pub elapsed_ms: u128,
    pub main_vrt: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TileBuildResult {
    pub family: ChartFamily,
    pub tile_count: u64,
    pub elapsed_ms: u128,
    pub tiles_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PackageBuildResult {
    pub family: ChartFamily,
    pub package_count: usize,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct NativeChartRunRequest {
    pub family: ChartFamily,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub cpu_jobs: usize,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
}

#[derive(Debug, Clone)]
pub struct NativeChartRunResult {
    pub family: ChartFamily,
    pub work_dir: PathBuf,
    pub prefetch_elapsed_ms: u128,
    pub vrt_count: usize,
    pub vrt_elapsed_ms: u128,
    pub tile_count: u64,
    pub tile_elapsed_ms: u128,
    pub package_count: usize,
    pub package_elapsed_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VrtKind {
    Vfr,
    Ifr,
}

#[derive(Debug, Clone, Copy)]
struct ChartSpec {
    family: ChartFamily,
    chart_name: &'static str,
    script_name: &'static str,
    chart_dir_name: &'static str,
    tile_index: &'static str,
    max_zoom: u32,
    vrt_kind: VrtKind,
}

impl ChartSpec {
    fn for_family(family: ChartFamily) -> Self {
        match family {
            ChartFamily::Sec => Self {
                family,
                chart_name: "SEC",
                script_name: "sec.py",
                chart_dir_name: "SEC",
                tile_index: "0",
                max_zoom: 10,
                vrt_kind: VrtKind::Vfr,
            },
            ChartFamily::Tac => Self {
                family,
                chart_name: "TAC",
                script_name: "tac.py",
                chart_dir_name: "TAC",
                tile_index: "1",
                max_zoom: 11,
                vrt_kind: VrtKind::Vfr,
            },
            ChartFamily::EnrL => Self {
                family,
                chart_name: "ENR_L",
                script_name: "enr_l.py",
                chart_dir_name: "ENR_L",
                tile_index: "3",
                max_zoom: 10,
                vrt_kind: VrtKind::Ifr,
            },
            ChartFamily::EnrH => Self {
                family,
                chart_name: "ENR_H",
                script_name: "enr_h.py",
                chart_dir_name: "ENR_H",
                tile_index: "4",
                max_zoom: 9,
                vrt_kind: VrtKind::Ifr,
            },
        }
    }
}

pub fn run_family(request: &ChartRunRequest) -> anyhow::Result<ChartRunResult> {
    let paths = RunPaths::new(&request.run_root);
    fs::create_dir_all(&paths.logs).context("failed to create logs dir")?;
    fs::create_dir_all(&paths.meta).context("failed to create meta dir")?;

    let work_dir = request
        .run_root
        .join("work")
        .join(request.family.capture_label());
    copy_dir_recursive(&request.source_repo, &work_dir, false)?;

    let mut prefetch_elapsed_ms = 0_u128;
    if let Some(source_urls_path) = &request.prefetch_source_urls {
        let start = Instant::now();
        let urls = read_source_urls_jsonl(source_urls_path)?;
        prefetch_archives(&urls, &work_dir, request.fetch_jobs)?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let provenance_dir = paths
        .meta
        .join("provenance")
        .join(request.family.capture_label());
    fs::create_dir_all(&provenance_dir).context("failed to create provenance dir")?;

    let invocation = ToolInvocation {
        program: "python3".to_string(),
        args: vec![ChartSpec::for_family(request.family)
            .script_name
            .to_string()],
        cwd: work_dir.clone(),
        label: request.family.capture_label().to_string(),
        env: vec![
            (
                "CAPTURE_LABEL".to_string(),
                request.family.capture_label().to_string(),
            ),
            (
                "CAPTURE_META_DIR".to_string(),
                provenance_dir.display().to_string(),
            ),
        ],
        stdin_text: None,
    };

    let outcome = invocation.run_logged(&paths.logs)?;
    if !outcome.success {
        bail!(
            "{} failed with exit code {:?}",
            request.family.capture_label(),
            outcome.exit_code
        );
    }

    let tile_count = count_tile_files(&work_dir.join("tiles"))?;

    Ok(ChartRunResult {
        family: request.family,
        work_dir,
        outcome,
        tile_count,
        prefetch_elapsed_ms,
    })
}

pub fn stage_work_dir(
    family: ChartFamily,
    source_repo: impl AsRef<Path>,
    run_root: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    let spec = ChartSpec::for_family(family);
    let source_repo = source_repo.as_ref();
    let work_dir = run_root
        .as_ref()
        .join("work")
        .join(spec.family.capture_label());
    copy_dir_recursive(
        source_repo,
        &work_dir,
        looks_like_populated_work_dir(source_repo),
    )?;
    Ok(work_dir)
}

pub fn run_native_family(request: &NativeChartRunRequest) -> anyhow::Result<NativeChartRunResult> {
    let paths = RunPaths::new(&request.run_root);
    fs::create_dir_all(&paths.logs).context("failed to create logs dir")?;
    fs::create_dir_all(&paths.meta).context("failed to create meta dir")?;
    let work_dir = stage_work_dir(request.family, &request.source_repo, &request.run_root)?;
    let provenance_dir = paths
        .meta
        .join("provenance")
        .join(request.family.capture_label());
    fs::create_dir_all(&provenance_dir).context("failed to create provenance dir")?;

    let mut prefetch_elapsed_ms = 0_u128;
    if let Some(source_urls_path) = &request.prefetch_source_urls {
        let start = Instant::now();
        copy_source_urls_provenance(source_urls_path, &provenance_dir)?;
        let urls = read_source_urls_jsonl(source_urls_path)?;
        prefetch_archives_with_provenance(
            &urls,
            &work_dir,
            request.fetch_jobs,
            &provenance_dir,
            request.family.capture_label(),
        )?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let vrt_result = build_family_vrts(request.family, &work_dir, request.cpu_jobs)?;
    let tile_result = build_family_tiles(request.family, &work_dir, request.cpu_jobs)?;
    let package_result = package_regions_from_spec(
        &work_dir,
        ChartSpec::for_family(request.family),
        Some(&provenance_dir),
    )?;

    Ok(NativeChartRunResult {
        family: request.family,
        work_dir,
        prefetch_elapsed_ms,
        vrt_count: vrt_result.vrt_count,
        vrt_elapsed_ms: vrt_result.elapsed_ms,
        tile_count: tile_result.tile_count,
        tile_elapsed_ms: tile_result.elapsed_ms,
        package_count: package_result.package_count,
        package_elapsed_ms: package_result.elapsed_ms,
    })
}

pub fn phase_plan(family: ChartFamily, concurrency: &ConcurrencyConfig) -> Vec<PhasePlan> {
    let crawl_note = match family {
        ChartFamily::Sec => "crawl VFR and Caribbean FAA pages for sectional ZIPs",
        ChartFamily::Tac => "crawl FAA VFR pages for TAC ZIPs",
        ChartFamily::EnrL => "crawl FAA IFR pages for low chart ZIPs",
        ChartFamily::EnrH => "crawl FAA IFR pages for high chart ZIPs",
    };

    vec![
        PhasePlan {
            name: "crawl-source-pages",
            work_kind: WorkKind::Network,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Bounded,
            recommended_jobs: 1,
            expected_bottleneck: "small network latency",
            note: crawl_note,
        },
        PhasePlan {
            name: "download-source-archives",
            work_kind: WorkKind::Network,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Wide,
            recommended_jobs: concurrency.fetch_jobs,
            expected_bottleneck: "network throughput and remote server pacing",
            note: "legacy loops one archive at a time; Rust should fetch multiple archives concurrently",
        },
        PhasePlan {
            name: "extract-source-archives",
            work_kind: WorkKind::Extract,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Bounded,
            recommended_jobs: concurrency.extract_jobs,
            expected_bottleneck: "local disk bandwidth",
            note: "extraction should be decoupled from fetch so both queues stay busy",
        },
        PhasePlan {
            name: "build-per-chart-vrts",
            work_kind: WorkKind::Cpu,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Wide,
            recommended_jobs: concurrency.cpu_jobs,
            expected_bottleneck: "GDAL warp CPU and memory bandwidth",
            note: "each chart warp is independent and should run on a bounded worker pool",
        },
        PhasePlan {
            name: "build-main-vrt",
            work_kind: WorkKind::Cpu,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Serial,
            recommended_jobs: 1,
            expected_bottleneck: "single VRT assembly step",
            note: "this step is cheap relative to warping and tiling",
        },
        PhasePlan {
            name: "generate-tiles",
            work_kind: WorkKind::Cpu,
            legacy_parallelism: Parallelism::Bounded,
            rust_parallelism: Parallelism::Bounded,
            recommended_jobs: concurrency.cpu_jobs,
            expected_bottleneck: "tiling CPU and output write rate",
            note: "keep one tiler busy with a process count close to available cores",
        },
        PhasePlan {
            name: "package-regions",
            work_kind: WorkKind::Io,
            legacy_parallelism: Parallelism::Serial,
            rust_parallelism: Parallelism::Bounded,
            recommended_jobs: concurrency.zip_jobs,
            expected_bottleneck: "ZIP write throughput",
            note: "region ZIP assembly can run in parallel once tile selection is known",
        },
    ]
}

pub fn likely_current_bottleneck() -> &'static str {
    "download-source-archives"
}

pub fn build_family_tiles(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    cpu_jobs: usize,
) -> anyhow::Result<TileBuildResult> {
    let spec = ChartSpec::for_family(family);
    build_tiles_from_spec(work_dir.as_ref(), spec, cpu_jobs)
}

pub fn package_family_regions(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
) -> anyhow::Result<PackageBuildResult> {
    let spec = ChartSpec::for_family(family);
    package_regions_from_spec(work_dir.as_ref(), spec, None)
}

pub fn build_family_vrts(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    cpu_jobs: usize,
) -> anyhow::Result<VrtBuildResult> {
    let spec = ChartSpec::for_family(family);
    build_vrts_from_spec(work_dir.as_ref(), spec, cpu_jobs)
}

fn build_vrts_from_spec(
    work_dir: &Path,
    spec: ChartSpec,
    cpu_jobs: usize,
) -> anyhow::Result<VrtBuildResult> {
    match spec.vrt_kind {
        VrtKind::Vfr => build_vfr_vrts(work_dir, spec, cpu_jobs),
        VrtKind::Ifr => build_ifr_vrts(work_dir, spec, cpu_jobs),
    }
}

fn build_vfr_vrts(
    work_dir: &Path,
    spec: ChartSpec,
    cpu_jobs: usize,
) -> anyhow::Result<VrtBuildResult> {
    let chart_dir_name = spec.chart_dir_name;
    let chart_dir = work_dir.join(chart_dir_name);
    // Compatibility note: overlap precedence in the main family VRT depends on input order.
    // We therefore reproduce the legacy Python script's file discovery and any family-
    // specific reordering quirks exactly, instead of choosing a tidier/deterministic order
    // of our own.
    let inputs = ordered_chart_input_names(spec.family, &chart_dir)?;
    let vrts = inputs
        .iter()
        .map(|base_name| work_dir.join(format!("{base_name}.vrt")))
        .collect::<Vec<_>>();

    let queue = Arc::new(Mutex::new(inputs));
    let job_count = cpu_jobs.max(1);
    let start = Instant::now();
    let mut handles = Vec::with_capacity(job_count);

    for worker_index in 0..job_count {
        let queue = Arc::clone(&queue);
        let work_dir = work_dir.to_path_buf();
        let chart_dir_name = chart_dir_name.to_string();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let next = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("queue poisoned"))?;
                    guard.pop()
                };
                let Some(base_name) = next else {
                    break;
                };
                build_one_vfr_vrt(
                    &work_dir,
                    &base_name,
                    &chart_dir_name,
                    spec.family,
                    worker_index,
                )?;
            }
            Ok(())
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("vrt worker panicked"))??;
    }

    build_main_vrt(work_dir, chart_dir_name, &vrts)?;
    let elapsed_ms = start.elapsed().as_millis();

    Ok(VrtBuildResult {
        family: spec.family,
        work_dir: work_dir.to_path_buf(),
        vrt_count: vrts.len(),
        elapsed_ms,
        main_vrt: work_dir.join(format!("{chart_dir_name}.vrt")),
    })
}

fn build_ifr_vrts(
    work_dir: &Path,
    spec: ChartSpec,
    cpu_jobs: usize,
) -> anyhow::Result<VrtBuildResult> {
    let chart_dir_name = spec.chart_dir_name;
    let chart_dir = work_dir.join(chart_dir_name);
    // Compatibility note: IFR families also depend on legacy VRT stacking order for overlap
    // precedence. Keep the legacy discovery/order contract instead of normalizing it.
    let inputs = ordered_chart_input_names(spec.family, &chart_dir)?;
    let vrts = inputs
        .iter()
        .map(|base_name| work_dir.join(format!("{base_name}.vrt")))
        .collect::<Vec<_>>();

    let queue = Arc::new(Mutex::new(inputs));
    let job_count = cpu_jobs.max(1);
    let start = Instant::now();
    let mut handles = Vec::with_capacity(job_count);

    for worker_index in 0..job_count {
        let queue = Arc::clone(&queue);
        let work_dir = work_dir.to_path_buf();
        let chart_dir_name = chart_dir_name.to_string();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let next = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("queue poisoned"))?;
                    guard.pop()
                };
                let Some(base_name) = next else {
                    break;
                };
                build_one_ifr_vrt(
                    &work_dir,
                    &base_name,
                    &chart_dir_name,
                    spec.family,
                    worker_index,
                )?;
            }
            Ok(())
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("vrt worker panicked"))??;
    }

    build_main_vrt(work_dir, chart_dir_name, &vrts)?;
    let elapsed_ms = start.elapsed().as_millis();

    Ok(VrtBuildResult {
        family: spec.family,
        work_dir: work_dir.to_path_buf(),
        vrt_count: vrts.len(),
        elapsed_ms,
        main_vrt: work_dir.join(format!("{chart_dir_name}.vrt")),
    })
}

fn ordered_chart_input_names(family: ChartFamily, chart_dir: &Path) -> anyhow::Result<Vec<String>> {
    // Legacy charts/common.py uses Python glob.glob("*.geojson", root_dir=...), so we shell
    // out to Python here rather than depending on Rust fs iteration order. This is deliberate:
    // several chart-family visual mismatches were caused by source-order drift that changed
    // gdalbuildvrt overlap precedence.
    let script = r#"import glob, sys
from pathlib import Path
chart_dir = Path(sys.argv[1])
for path in glob.glob("*.geojson", root_dir=chart_dir):
    print(Path(path).stem)
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(chart_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| {
            format!(
                "failed to enumerate chart inputs under {}",
                chart_dir.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("python glob enumeration failed: {stderr}");
    }

    let mut names: Vec<String> = String::from_utf8(output.stdout)
        .context("chart input enumeration was not utf-8")?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    match family {
        // Legacy enr_l.py explicitly sorts ascending before the main VRT is assembled.
        ChartFamily::EnrL => names.sort(),
        // Legacy enr_h.py explicitly sorts descending so "P" charts are overwritten in the
        // same way as the historical pipeline. Keep that surprising behavior for parity.
        ChartFamily::EnrH => names.sort_by(|a, b| b.cmp(a)),
        _ => {}
    }

    Ok(names)
}

fn build_one_vfr_vrt(
    work_dir: &Path,
    base_name: &str,
    chart_dir_name: &str,
    family: ChartFamily,
    worker_index: usize,
) -> anyhow::Result<()> {
    let tif_name = format!("{base_name}.tif");
    let rgb_vrt_name = format!("{base_name}rgb.vrt");
    let vrt_name = format!("{base_name}.vrt");
    let cutline = format!("{chart_dir_name}/{base_name}.geojson");
    let logs_dir = work_dir.join(".rust-logs");
    let family_label = family.capture_label();

    remove_if_exists(work_dir.join(&rgb_vrt_name))?;
    remove_if_exists(work_dir.join(&vrt_name))?;

    let translate = ToolInvocation {
        program: "gdal_translate".to_string(),
        args: vec![
            "-of".to_string(),
            "vrt".to_string(),
            "-r".to_string(),
            "cubicspline".to_string(),
            "-expand".to_string(),
            "rgb".to_string(),
            tif_name.clone(),
            rgb_vrt_name.clone(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "{family_label}-translate-{worker_index}-{}",
            sanitize_label(base_name)
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let translate_outcome = translate.run_logged(&logs_dir)?;
    if !translate_outcome.success {
        bail!("gdal_translate failed for {base_name}");
    }

    let warp = ToolInvocation {
        program: "gdalwarp".to_string(),
        args: vec![
            "-of".to_string(),
            "vrt".to_string(),
            "-r".to_string(),
            "cubicspline".to_string(),
            "-dstnodata".to_string(),
            "51".to_string(),
            "-t_srs".to_string(),
            "EPSG:3857".to_string(),
            "-cutline".to_string(),
            cutline,
            "-crop_to_cutline".to_string(),
            rgb_vrt_name,
            vrt_name,
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "{family_label}-warp-{worker_index}-{}",
            sanitize_label(base_name)
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let warp_outcome = warp.run_logged(&logs_dir)?;
    if !warp_outcome.success {
        bail!("gdalwarp failed for {base_name}");
    }

    Ok(())
}

fn build_one_ifr_vrt(
    work_dir: &Path,
    base_name: &str,
    chart_dir_name: &str,
    family: ChartFamily,
    worker_index: usize,
) -> anyhow::Result<()> {
    let tif_name = format!("{base_name}.tif");
    let vrt_name = format!("{base_name}.vrt");
    let cutline = format!("{chart_dir_name}/{base_name}.geojson");
    let logs_dir = work_dir.join(".rust-logs");
    let family_label = family.capture_label();

    remove_if_exists(work_dir.join(&vrt_name))?;

    let warp = ToolInvocation {
        program: "gdalwarp".to_string(),
        args: vec![
            "-of".to_string(),
            "vrt".to_string(),
            "-r".to_string(),
            "cubic".to_string(),
            "-dstnodata".to_string(),
            "51".to_string(),
            "-t_srs".to_string(),
            "EPSG:3857".to_string(),
            "-cutline".to_string(),
            cutline,
            "-crop_to_cutline".to_string(),
            tif_name,
            vrt_name,
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "{family_label}-warp-{worker_index}-{}",
            sanitize_label(base_name)
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let warp_outcome = warp.run_logged(&logs_dir)?;
    if !warp_outcome.success {
        bail!("gdalwarp failed for {base_name}");
    }

    Ok(())
}

fn remove_if_exists(path: PathBuf) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove stale {}", path.display()))?;
    }
    Ok(())
}

fn build_main_vrt(work_dir: &Path, chart_name: &str, vrts: &[PathBuf]) -> anyhow::Result<()> {
    let mut args = vec![
        "-r".to_string(),
        "cubicspline".to_string(),
        "-srcnodata".to_string(),
        "51".to_string(),
        "-vrtnodata".to_string(),
        "51".to_string(),
        "-resolution".to_string(),
        "highest".to_string(),
        "-overwrite".to_string(),
        format!("{chart_name}.vrt"),
    ];
    for vrt in vrts {
        let file_name = vrt
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow::anyhow!("failed to derive vrt filename"))?;
        args.push(file_name.to_string());
    }

    let invocation = ToolInvocation {
        program: "gdalbuildvrt".to_string(),
        args,
        cwd: work_dir.to_path_buf(),
        label: format!("{}-main-vrt", chart_name.to_ascii_lowercase()),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("gdalbuildvrt failed for {chart_name}");
    }
    Ok(())
}

fn build_tiles_from_spec(
    work_dir: &Path,
    spec: ChartSpec,
    cpu_jobs: usize,
) -> anyhow::Result<TileBuildResult> {
    let tiles_root = work_dir.join("tiles").join(spec.tile_index);
    if tiles_root.exists() {
        // gdal2tiles.py overview tiles are generated from already-written child image files.
        // For parity/debugging we must never inherit stale outputs from an earlier run, so the
        // Rust path always starts from a clean tile tree instead of relying on --resume.
        fs::remove_dir_all(&tiles_root)
            .with_context(|| format!("failed to remove stale {}", tiles_root.display()))?;
    }

    let invocation = ToolInvocation {
        program: "gdal2tiles.py".to_string(),
        args: vec![
            "-t".to_string(),
            spec.chart_name.to_string(),
            "--tilesize=512".to_string(),
            "--tiledriver=WEBP".to_string(),
            "--webp-quality=60".to_string(),
            "--exclude".to_string(),
            "--webviewer=all".to_string(),
            "-c".to_string(),
            "MUAVLLC".to_string(),
            "--no-kml".to_string(),
            // Chart parity depends on matching the legacy gdal2tiles process count. We pass
            // through the caller's cpu_jobs here, and the validation harness pins charts to 8
            // processes because the legacy Python chart scripts hard-code --processes 8.
            "--processes".to_string(),
            cpu_jobs.to_string(),
            "-z".to_string(),
            format!("0-{}", spec.max_zoom),
            "-r".to_string(),
            "near".to_string(),
            format!("{}.vrt", spec.chart_name),
            format!("tiles/{}", spec.tile_index),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!("{}-gdal2tiles", spec.family.capture_label()),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("gdal2tiles failed for {}", spec.family.capture_label());
    }

    Ok(TileBuildResult {
        family: spec.family,
        tile_count: count_tile_files(&tiles_root)?,
        elapsed_ms: outcome.elapsed_ms,
        tiles_root,
    })
}

fn package_regions_from_spec(
    work_dir: &Path,
    spec: ChartSpec,
    provenance_dir: Option<&Path>,
) -> anyhow::Result<PackageBuildResult> {
    let start = Instant::now();
    let regions = Region::ALL;
    let manifest_cycle = calculate_manifest_cycle();
    let tile_paths = collect_tile_paths_glob(work_dir, spec.tile_index)?;
    let mut package_records = Vec::with_capacity(regions.len());

    for region in &regions {
        let manifest_name = format!("{}_{}", region.code(), spec.chart_name);
        let zip_name = format!("{}_{}.zip", region.code(), spec.chart_name);
        let manifest_path = work_dir.join(&manifest_name);
        let zip_path = work_dir.join(&zip_name);

        if zip_path.exists() {
            fs::remove_file(&zip_path)
                .with_context(|| format!("failed to remove {}", zip_path.display()))?;
        }

        let mut selected = Vec::new();
        for tile_path in &tile_paths {
            if tile_belongs_to_region(tile_path, region) {
                selected.push(tile_path.clone());
            }
        }

        let mut manifest_text = String::new();
        manifest_text.push_str(&manifest_cycle);
        manifest_text.push('\n');
        for path in &selected {
            manifest_text.push_str(path);
            manifest_text.push('\n');
        }
        fs::write(&manifest_path, manifest_text)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;

        let mut stdin_text = String::new();
        for path in &selected {
            stdin_text.push_str(path);
            stdin_text.push('\n');
        }
        stdin_text.push_str(&manifest_name);
        stdin_text.push('\n');

        let invocation = ToolInvocation {
            program: "zip".to_string(),
            args: vec!["-q".to_string(), zip_name.clone(), "-@".to_string()],
            cwd: work_dir.to_path_buf(),
            label: format!("{}-package-{}", spec.family.capture_label(), region.code()),
            env: Vec::new(),
            stdin_text: Some(stdin_text),
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        if !outcome.success {
            bail!("zip failed for region {}", region.code());
        }

        if provenance_dir.is_some() {
            package_records.push(PackageOutputRecord {
                label: spec.family.capture_label().to_string(),
                chart: Some(spec.chart_name.to_string()),
                region: region.code().to_string(),
                manifest: manifest_name,
                manifest_sha256: hash_file(&manifest_path)?,
                zip: zip_name,
                zip_sha256: hash_file(&zip_path)?,
            });
        }
    }

    if let Some(provenance_dir) = provenance_dir {
        write_package_outputs_jsonl(provenance_dir, &package_records)?;
    }

    Ok(PackageBuildResult {
        family: spec.family,
        package_count: regions.len(),
        elapsed_ms: start.elapsed().as_millis(),
    })
}

fn sanitize_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn collect_tile_paths_glob(work_dir: &Path, tile_index: &str) -> anyhow::Result<Vec<String>> {
    let script = r#"import glob, sys
from pathlib import Path
root = Path(sys.argv[1])
tile_index = sys.argv[2]
for path in glob.glob(str(root / f"tiles/{tile_index}/**/*.webp"), recursive=True):
    print(Path(path).relative_to(root).as_posix())
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(work_dir)
        .arg(tile_index)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to enumerate tiles under {}", work_dir.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("python glob enumeration failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("tile enumeration was not utf-8")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn tile_belongs_to_region(tile_path: &str, region: &Region) -> bool {
    let tokens: Vec<&str> = tile_path.split('/').collect();
    let (z_index, x_index, y_index) = match tokens.len() {
        4 => (1, 2, 3),
        5 if tokens[0] == "tiles" => (2, 3, 4),
        _ => return false,
    };
    let z = match tokens[z_index].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let x = match tokens[x_index].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let y = match tokens[y_index].trim_end_matches(".webp").parse::<u32>() {
        Ok(value) => value,
        Err(_) => return false,
    };

    if z <= 7 {
        return true;
    }

    let RegionBounds {
        lon_min: region_lon_min,
        lat_max: region_lat_max,
        lon_max: region_lon_max,
        lat_min: region_lat_min,
    } = region.bounds();
    let (tile_lon_min, tile_lat_min, tile_lon_max, tile_lat_max) = find_bounds(x, y, z);
    let lon_overlap = tile_lon_max >= region_lon_min && tile_lon_min <= region_lon_max;
    let lat_overlap = tile_lat_max >= region_lat_min && tile_lat_min <= region_lat_max;
    lon_overlap && lat_overlap
}

fn calculate_manifest_cycle() -> String {
    calculate_cycle(1).0.to_string()
}

fn calculate_cycle(future: i32) -> (u32, u32) {
    let start_utc = chrono::DateTime::parse_from_rfc3339("2020-01-02T09:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut start_utc = start_utc;
    let mut cycle = 1_u32;
    let mut last_year = 2019_i32;
    let mut combined = 2001_u32;
    let mut is56 = true;

    let now_utc = chrono::Utc::now() + chrono::Duration::days((future as i64) * 28);

    while start_utc < now_utc {
        if last_year != start_utc.year() {
            cycle = 1;
            last_year = start_utc.year();
        } else {
            cycle += 1;
        }

        combined = ((start_utc.year() % 2000) as u32) * 100 + cycle;
        is56 = !is56;
        start_utc += chrono::Duration::days(28);
    }

    if is56 {
        (combined, combined)
    } else {
        let (_, prev) = calculate_cycle(future - 1);
        (combined, prev)
    }
}

fn find_bounds(x: u32, y: u32, zoom: u32) -> (f64, f64, f64, f64) {
    let size = 512.0_f64;
    let origin_shift = std::f64::consts::PI * 6378137.0;
    let initial_resolution = (2.0 * std::f64::consts::PI * 6378137.0) / size;
    let resolution = initial_resolution / 2_f64.powi(zoom as i32);

    let lon_u = meters_to_lon(
        x_pixels_to_meters(zoom, x as f64 * size, resolution, origin_shift),
        origin_shift,
    );
    let lon_l = meters_to_lon(
        x_pixels_to_meters(zoom, (x as f64 + 1.0) * size, resolution, origin_shift),
        origin_shift,
    );
    let lat_l = meters_to_lat(
        y_pixels_to_meters(zoom, y as f64 * size, resolution, origin_shift),
        origin_shift,
    );
    let lat_u = meters_to_lat(
        y_pixels_to_meters(zoom, (y as f64 + 1.0) * size, resolution, origin_shift),
        origin_shift,
    );
    (lon_u, lat_l, lon_l, lat_u)
}

fn x_pixels_to_meters(_zoom: u32, px: f64, resolution: f64, origin_shift: f64) -> f64 {
    px * resolution - origin_shift
}

fn y_pixels_to_meters(_zoom: u32, py: f64, resolution: f64, origin_shift: f64) -> f64 {
    py * resolution - origin_shift
}

fn meters_to_lon(mx: f64, origin_shift: f64) -> f64 {
    mx / (origin_shift / 180.0)
}

fn meters_to_lat(my: f64, origin_shift: f64) -> f64 {
    let lat = my / (origin_shift / 180.0);
    (180.0 / std::f64::consts::PI)
        * (2.0 * (lat.to_radians()).exp().atan() - std::f64::consts::FRAC_PI_2)
}

fn copy_dir_recursive(src: &Path, dst: &Path, preserve_generated: bool) -> anyhow::Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create destination {}", dst.display()))?;

    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        if should_skip_copy(&src_path, file_type.is_dir(), preserve_generated) {
            continue;
        }
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, preserve_generated)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn looks_like_populated_work_dir(path: &Path) -> bool {
    path.join("tiles").is_dir()
        || path
            .read_dir()
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .any(|entry| {
                let entry_path = entry.path();
                entry_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext,
                            "zip" | "tif" | "tfw" | "vrt" | "webp" | "png" | "htm" | "html"
                        )
                    })
            })
}

fn should_skip_copy(path: &Path, is_dir: bool, preserve_generated: bool) -> bool {
    let name = match path.file_name().and_then(|value| value.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if is_dir {
        if matches!(name, "logs" | "meta" | "work" | "rust-runs") {
            return true;
        }
        return if preserve_generated {
            matches!(name, ".git" | "__pycache__" | ".rust-logs")
        } else {
            matches!(name, ".git" | "__pycache__" | "tiles" | ".rust-logs")
        };
    }

    if preserve_generated {
        return false;
    }

    if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("zip" | "tif" | "tfw" | "vrt" | "webp" | "db" | "htm" | "html" | "png")
    ) {
        return true;
    }

    false
}

fn count_tile_files(tiles_root: &Path) -> anyhow::Result<u64> {
    if !tiles_root.is_dir() {
        return Ok(0);
    }

    let mut count = 0_u64;
    count_files_recursive(tiles_root, &mut count)?;
    Ok(count)
}

fn count_files_recursive(path: &Path, count: &mut u64) -> anyhow::Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            count_files_recursive(&entry.path(), count)?;
        } else if file_type.is_file() {
            *count += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_dir_recursive;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("aerobag-{label}-{unique}"));
            fs::create_dir_all(&path).expect("failed to create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn populated_staging_skips_prior_run_scaffolding() {
        let temp = TempDir::new("charts-stage-copy");
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(src.join("rust-runs/old-run/meta/provenance"))
            .expect("failed to create nested rust-runs path");
        fs::create_dir_all(src.join("meta/provenance/charts-sec"))
            .expect("failed to create meta path");
        fs::create_dir_all(src.join("logs")).expect("failed to create logs path");
        fs::create_dir_all(src.join("work/charts-sec")).expect("failed to create work path");
        fs::write(src.join("Anchorage SEC.tif"), b"chart").expect("failed to write chart artifact");
        fs::write(src.join("Anchorage SEC.zip"), b"zip").expect("failed to write zip artifact");
        fs::write(
            src.join("meta/provenance/charts-sec/source_urls.jsonl"),
            b"[]",
        )
        .expect("failed to write provenance artifact");
        fs::write(src.join("logs/charts-sec.stderr.log"), b"log").expect("failed to write log");
        fs::write(src.join("work/charts-sec/copied.txt"), b"work").expect("failed to write work");
        fs::write(
            src.join("rust-runs/old-run/meta/provenance/copied.txt"),
            b"nested",
        )
        .expect("failed to write nested artifact");

        copy_dir_recursive(&src, &dst, true).expect("copy should succeed");

        assert!(dst.join("Anchorage SEC.tif").is_file());
        assert!(dst.join("Anchorage SEC.zip").is_file());
        assert!(!dst.join("meta").exists());
        assert!(!dst.join("logs").exists());
        assert!(!dst.join("work").exists());
        assert!(!dst.join("rust-runs").exists());
    }
}
