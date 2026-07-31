// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::BTreeMap,
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
    ChartFamily, ChartPackageCollection, ChartReferenceAssetRecord, ChartReferenceCoverage,
    ChartReferenceManifest, ConcurrencyConfig, Parallelism, PhasePlan, Region, RegionBounds,
    RunPaths, WorkKind, CHART_REFERENCE_MANIFEST_DIR,
};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives, prefetch_archives_with_provenance,
    read_source_prefetch_requests_jsonl, write_package_outputs_jsonl, FetchCacheConfig,
    PackageOutputRecord,
};
use preprocessor_tools::{
    append_pngs_vertical, command_output_diagnostic_summary, flatten_png_onto_white,
    sanitize_label, write_thumbnail_from_png, ToolInvocation, ToolOutcome,
};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use product_contracts::{ChartPackageTier, CHART_PACKAGE_TIER_METADATA_KEY};
use serde::{Deserialize, Serialize};

pub const FULL_COVERAGE_ZOOM: u32 = 7;
pub const WIDE_ANGLE_REGION_ID: &str = "wide";
pub const CHART_REFERENCE_CATALOG_NAME: &str = "chart-reference-catalog.json";

#[derive(Debug, Clone)]
pub struct ChartRunRequest {
    pub family: ChartFamily,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
    pub fetch_cache: Option<FetchCacheConfig>,
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
pub struct ExtractBuildResult {
    pub family: ChartFamily,
    pub kind: ChartExtractKind,
    pub output_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartExtractKind {
    Legend,
    Inset,
}

impl ChartExtractKind {
    fn metadata_type(self) -> &'static str {
        match self {
            Self::Legend => "legend",
            Self::Inset => "inset",
        }
    }

    fn output_dir(self) -> &'static str {
        match self {
            Self::Legend => "legends",
            Self::Inset => "insets",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ChartExtractLayout {
    pub schema_version: u32,
    pub source: String,
    pub source_width: u32,
    pub source_height: u32,
    pub max_output_width: u32,
    #[serde(default)]
    pub coverage_source: Option<String>,
    pub regions: Vec<ChartExtractRegion>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct ChartExtractRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChartReferenceCatalog {
    schema_version: u32,
    family_id: String,
    assets: Vec<ChartReferenceAssetRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RasterInspection {
    dimensions: (u32, u32),
    has_palette: bool,
}

#[derive(Debug, Clone)]
pub struct NativeChartRunRequest {
    pub family: ChartFamily,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub cpu_jobs: usize,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
    pub fetch_cache: Option<FetchCacheConfig>,
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
    base_max_zoom: u32,
    vrt_kind: VrtKind,
}

#[derive(Debug, Clone, Copy)]
struct PackageChartSource<'a> {
    spec: ChartSpec,
    work_dir: &'a Path,
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
                base_max_zoom: 10,
                vrt_kind: VrtKind::Vfr,
            },
            ChartFamily::Tac => Self {
                family,
                chart_name: "TAC",
                script_name: "tac.py",
                chart_dir_name: "TAC",
                tile_index: "1",
                base_max_zoom: 11,
                vrt_kind: VrtKind::Vfr,
            },
            ChartFamily::Flyway => Self {
                family,
                chart_name: "FLY",
                script_name: "flyway.py",
                chart_dir_name: "FLY",
                tile_index: "2",
                base_max_zoom: 11,
                vrt_kind: VrtKind::Vfr,
            },
            ChartFamily::EnrL => Self {
                family,
                chart_name: "ENR_L",
                script_name: "enr_l.py",
                chart_dir_name: "ENR_L",
                tile_index: "3",
                base_max_zoom: 10,
                vrt_kind: VrtKind::Ifr,
            },
            ChartFamily::EnrH => Self {
                family,
                chart_name: "ENR_H",
                script_name: "enr_h.py",
                chart_dir_name: "ENR_H",
                tile_index: "4",
                base_max_zoom: 9,
                vrt_kind: VrtKind::Ifr,
            },
        }
    }

    fn detail_zoom(self) -> u32 {
        self.base_max_zoom + 1
    }
}

pub fn run_family(request: &ChartRunRequest) -> anyhow::Result<ChartRunResult> {
    if request.family == ChartFamily::Flyway {
        bail!("Flyway has no legacy Python chart pipeline; use the native chart pipeline");
    }
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
        let requests = read_source_prefetch_requests_jsonl(source_urls_path)?;
        prefetch_archives(
            &requests,
            &work_dir,
            request.fetch_jobs,
            request.fetch_cache.as_ref(),
        )?;
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
    invocation.ensure_success(
        &outcome,
        &format!("{} failed", request.family.capture_label()),
    )?;

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
        let requests = read_source_prefetch_requests_jsonl(source_urls_path)?;
        prefetch_archives_with_provenance(
            &requests,
            &work_dir,
            request.fetch_jobs,
            request.fetch_cache.as_ref(),
            &provenance_dir,
            request.family.capture_label(),
        )?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let vrt_result = build_family_vrts(request.family, &work_dir, request.cpu_jobs)?;
    build_family_legends(request.family, &work_dir)?;
    build_family_insets(request.family, &work_dir)?;
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
        ChartFamily::Flyway => "reuse FAA TAC archives for Flyway chart TIFFs",
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

pub fn package_family_region(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    region: Region,
) -> anyhow::Result<PackageOutputRecord> {
    package_family_region_versioned(
        family,
        work_dir,
        region,
        &calculate_manifest_cycle(),
        &calculate_manifest_cycle(),
    )
}

pub fn package_family_region_versioned(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    package_family_region_versioned_to(
        family,
        work_dir.as_ref(),
        work_dir.as_ref(),
        region,
        manifest_version,
        artifact_version,
    )
}

pub fn package_family_region_versioned_to(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    package_family_bundle_region_versioned_to(
        family,
        work_dir.as_ref(),
        &[],
        output_dir.as_ref(),
        region,
        manifest_version,
        artifact_version,
    )
}

pub fn package_family_bundle_region_versioned_to(
    family: ChartFamily,
    work_dir: &Path,
    bundled_families: &[(ChartFamily, &Path)],
    output_dir: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    let mut sources = vec![PackageChartSource {
        spec: ChartSpec::for_family(family),
        work_dir,
    }];
    sources.extend(
        bundled_families
            .iter()
            .map(|(family, work_dir)| PackageChartSource {
                spec: ChartSpec::for_family(*family),
                work_dir,
            }),
    );
    package_region_records_from_sources(
        output_dir,
        &sources,
        &[region],
        ChartPackageTier::Regional,
        true,
        manifest_version,
        artifact_version,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| anyhow::anyhow!("no package record generated for {}", region.code()))
}

pub fn package_family_bundle_detail_region_versioned_to(
    family: ChartFamily,
    work_dir: &Path,
    bundled_families: &[(ChartFamily, &Path)],
    output_dir: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    let mut sources = vec![PackageChartSource {
        spec: ChartSpec::for_family(family),
        work_dir,
    }];
    sources.extend(
        bundled_families
            .iter()
            .map(|(family, work_dir)| PackageChartSource {
                spec: ChartSpec::for_family(*family),
                work_dir,
            }),
    );
    package_region_records_from_sources(
        output_dir,
        &sources,
        &[region],
        ChartPackageTier::Detail,
        true,
        manifest_version,
        artifact_version,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| anyhow::anyhow!("no detail package record generated for {}", region.code()))
}

pub fn package_family_wide_angle_versioned(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    package_family_wide_angle_versioned_to(
        family,
        work_dir.as_ref(),
        work_dir.as_ref(),
        manifest_version,
        artifact_version,
    )
}

pub fn package_family_wide_angle_versioned_to(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    package_family_bundle_wide_angle_versioned_to(
        family,
        work_dir.as_ref(),
        &[],
        output_dir.as_ref(),
        manifest_version,
        artifact_version,
    )
}

pub fn package_family_bundle_wide_angle_versioned_to(
    family: ChartFamily,
    work_dir: &Path,
    bundled_families: &[(ChartFamily, &Path)],
    output_dir: &Path,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    let mut sources = vec![PackageChartSource {
        spec: ChartSpec::for_family(family),
        work_dir,
    }];
    sources.extend(
        bundled_families
            .iter()
            .map(|(family, work_dir)| PackageChartSource {
                spec: ChartSpec::for_family(*family),
                work_dir,
            }),
    );
    package_wide_angle_record_from_sources(
        output_dir,
        &sources,
        true,
        manifest_version,
        artifact_version,
    )
}

pub fn build_family_vrts(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    cpu_jobs: usize,
) -> anyhow::Result<VrtBuildResult> {
    let spec = ChartSpec::for_family(family);
    build_vrts_from_spec(work_dir.as_ref(), spec, cpu_jobs)
}

pub fn build_family_legends(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
) -> anyhow::Result<ExtractBuildResult> {
    build_family_extracts(family, work_dir, ChartExtractKind::Legend)
}

pub fn build_family_insets(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
) -> anyhow::Result<ExtractBuildResult> {
    build_family_extracts(family, work_dir, ChartExtractKind::Inset)
}

pub fn build_family_reference_catalog(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    let work_dir = work_dir.as_ref();
    let spec = ChartSpec::for_family(family);
    let layout_dir = work_dir.join(spec.chart_dir_name);
    let mut assets = Vec::new();
    for kind in [ChartExtractKind::Legend, ChartExtractKind::Inset] {
        let mut layouts = fs::read_dir(&layout_dir)
            .with_context(|| format!("failed to read {}", layout_dir.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(&format!(".{}.json", kind.metadata_type())))
            })
            .collect::<Vec<_>>();
        layouts.sort();
        for layout_path in layouts {
            let layout: ChartExtractLayout = serde_json::from_slice(&fs::read(&layout_path)?)
                .with_context(|| format!("failed to parse {}", layout_path.display()))?;
            if layout.regions.is_empty() {
                continue;
            }
            let source_chart_id = Path::new(&layout.source)
                .file_stem()
                .and_then(|value| value.to_str())
                .with_context(|| format!("extract source {} has no UTF-8 stem", layout.source))?
                .to_string();
            let file_name = format!("{source_chart_id}.png");
            let asset_path = format!("{}/{}", kind.output_dir(), file_name);
            let thumbnail_path = format!("thumbnails/{}/{}", kind.output_dir(), file_name);
            if !work_dir.join(&asset_path).is_file() || !work_dir.join(&thumbnail_path).is_file() {
                bail!(
                    "chart reference outputs missing for {source_chart_id} {}",
                    kind.metadata_type()
                );
            }
            let coverage_source = layout
                .coverage_source
                .as_deref()
                .unwrap_or(&source_chart_id);
            let source_coverage = source_chart_coverage(&layout_dir, coverage_source)?;
            let kind_label = match kind {
                ChartExtractKind::Legend => "Legend",
                ChartExtractKind::Inset => "Insets",
            };
            assets.push(ChartReferenceAssetRecord {
                id: format!(
                    "chart-reference:{}:{}:{}",
                    chart_family_id(family),
                    kind.metadata_type(),
                    stable_id_component(&source_chart_id)
                ),
                family_id: chart_family_id(family).to_string(),
                source_chart_id: source_chart_id.clone(),
                label: format!("{source_chart_id} {kind_label}"),
                kind: kind.metadata_type().to_string(),
                asset_path,
                thumbnail_path,
                source_coverage: Some(source_coverage),
            });
        }
    }
    assets.sort_by(|left, right| left.id.cmp(&right.id));
    let output_path = work_dir.join(CHART_REFERENCE_CATALOG_NAME);
    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&ChartReferenceCatalog {
            schema_version: 1,
            family_id: chart_family_id(family).to_string(),
            assets,
        })?,
    )
    .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(output_path)
}

pub fn build_family_extracts(
    family: ChartFamily,
    work_dir: impl AsRef<Path>,
    kind: ChartExtractKind,
) -> anyhow::Result<ExtractBuildResult> {
    let work_dir = work_dir.as_ref();
    let spec = ChartSpec::for_family(family);
    let layout_dir = work_dir.join(spec.chart_dir_name);
    let output_root = work_dir.join(kind.output_dir());
    let thumbnail_output_root = work_dir.join("thumbnails").join(kind.output_dir());
    if output_root.exists() {
        fs::remove_dir_all(&output_root)
            .with_context(|| format!("failed to clear {}", output_root.display()))?;
    }
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    if thumbnail_output_root.exists() {
        fs::remove_dir_all(&thumbnail_output_root)
            .with_context(|| format!("failed to clear {}", thumbnail_output_root.display()))?;
    }

    let mut layouts = fs::read_dir(&layout_dir)
        .with_context(|| format!("failed to read {}", layout_dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&format!(".{}.json", kind.metadata_type())))
        })
        .collect::<Vec<_>>();
    layouts.sort();

    let mut output_paths = Vec::with_capacity(layouts.len());
    for layout_path in layouts {
        if let Some(output_path) = render_chart_extract(work_dir, &output_root, &layout_path, kind)?
        {
            output_paths.push(output_path);
        }
    }
    Ok(ExtractBuildResult {
        family,
        kind,
        output_paths,
    })
}

fn render_chart_extract(
    work_dir: &Path,
    output_root: &Path,
    layout_path: &Path,
    kind: ChartExtractKind,
) -> anyhow::Result<Option<PathBuf>> {
    let layout: ChartExtractLayout = serde_json::from_slice(
        &fs::read(layout_path)
            .with_context(|| format!("failed to read {}", layout_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", layout_path.display()))?;
    let source_path = work_dir.join(&layout.source);
    let source_raster = inspect_raster(&source_path)?;
    validate_chart_extract_layout(&layout, layout_path, source_raster.dimensions, kind)?;
    if layout.regions.is_empty() {
        return Ok(None);
    }

    let widest_region = layout
        .regions
        .iter()
        .map(|region| region.width)
        .max()
        .with_context(|| {
            format!(
                "{} layout unexpectedly had no regions",
                kind.metadata_type()
            )
        })?;
    let scale = (f64::from(layout.max_output_width) / f64::from(widest_region)).min(1.0);
    let source_stem = Path::new(&layout.source)
        .file_stem()
        .and_then(|value| value.to_str())
        .with_context(|| format!("{} source has no UTF-8 file stem", kind.metadata_type()))?;
    let temp_dir = work_dir
        .join(".chart-extract-parts")
        .join(kind.metadata_type())
        .join(sanitize_label(source_stem));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to clear {}", temp_dir.display()))?;
    }
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let logs_dir = work_dir.join(".rust-logs");
    let mut part_paths = Vec::with_capacity(layout.regions.len());
    for (index, region) in layout.regions.iter().enumerate() {
        let output_width = (f64::from(region.width) * scale).round().max(1.0) as u32;
        let output_height = (f64::from(region.height) * scale).round().max(1.0) as u32;
        let rgb_part_path = temp_dir.join(format!("part-{index:02}-rgb.tif"));
        let mut expand_args = vec!["-of".to_string(), "GTiff".to_string()];
        if source_raster.has_palette {
            expand_args.extend(["-expand".to_string(), "rgb".to_string()]);
        }
        expand_args.extend([
            "-srcwin".to_string(),
            region.x.to_string(),
            region.y.to_string(),
            region.width.to_string(),
            region.height.to_string(),
            layout.source.clone(),
            rgb_part_path.to_string_lossy().to_string(),
        ]);
        let expand = ToolInvocation {
            program: "gdal_translate".to_string(),
            args: expand_args,
            cwd: work_dir.to_path_buf(),
            label: format!(
                "{}-expand-{}-{index:02}",
                kind.metadata_type(),
                sanitize_label(source_stem)
            ),
            env: Vec::new(),
            stdin_text: None,
        };
        let expand_outcome = expand.run_logged(&logs_dir)?;
        expand.ensure_success(
            &expand_outcome,
            &format!(
                "failed to expand {} region {} from {} to RGB",
                kind.metadata_type(),
                index + 1,
                layout.source
            ),
        )?;

        let part_path = temp_dir.join(format!("part-{index:02}.png"));
        let invocation = ToolInvocation {
            program: "gdal_translate".to_string(),
            args: vec![
                "-of".to_string(),
                "PNG".to_string(),
                "-r".to_string(),
                "lanczos".to_string(),
                "-outsize".to_string(),
                output_width.to_string(),
                output_height.to_string(),
                rgb_part_path.to_string_lossy().to_string(),
                part_path.to_string_lossy().to_string(),
            ],
            cwd: work_dir.to_path_buf(),
            label: format!(
                "{}-crop-{}-{index:02}",
                kind.metadata_type(),
                sanitize_label(source_stem)
            ),
            env: Vec::new(),
            stdin_text: None,
        };
        let outcome = invocation.run_logged(&logs_dir)?;
        invocation.ensure_success(
            &outcome,
            &format!(
                "failed to crop {} region {} from {}",
                kind.metadata_type(),
                index + 1,
                layout.source
            ),
        )?;
        part_paths.push(part_path);
    }

    let output_path = output_root.join(format!("{source_stem}.png"));
    append_pngs_vertical(
        work_dir,
        &logs_dir,
        &part_paths,
        &output_path,
        &format!(
            "{}-append-{}",
            kind.metadata_type(),
            sanitize_label(source_stem)
        ),
    )?;
    flatten_png_onto_white(&output_path)?;
    write_thumbnail_from_png(
        &output_path,
        &work_dir.join("thumbnails"),
        &Path::new(kind.output_dir()).join(format!("{source_stem}.png")),
    )?;
    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(Some(output_path))
}

fn chart_family_id(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "sec",
        ChartFamily::Tac => "tac",
        ChartFamily::Flyway => "flyway",
        ChartFamily::EnrL => "enr-l",
        ChartFamily::EnrH => "enr-h",
    }
}

fn stable_id_component(value: &str) -> String {
    sanitize_label(value).trim_matches('-').to_ascii_lowercase()
}

fn source_chart_coverage(
    layout_dir: &Path,
    source_chart_id: &str,
) -> anyhow::Result<ChartReferenceCoverage> {
    let path = layout_dir.join(format!("{source_chart_id}.geojson"));
    let document: serde_json::Value = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let crs = document
        .pointer("/crs/properties/name")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let coordinates_are_lon_lat = match crs {
        value if value.ends_with("3857") => false,
        "urn:ogc:def:crs:OGC:1.3:CRS84" => true,
        _ => bail!(
            "chart reference coverage requires EPSG:3857 or CRS84 cutline in {}; got {crs:?}",
            path.display()
        ),
    };
    let coordinates = document
        .pointer("/features/0/geometry/coordinates")
        .with_context(|| format!("{} has no polygon coordinates", path.display()))?;
    let mut mercator_points = Vec::new();
    collect_coordinate_pairs(coordinates, &mut mercator_points);
    if mercator_points.is_empty() {
        bail!("{} has no coordinate pairs", path.display());
    }
    let mut coverage = ChartReferenceCoverage {
        lat_min: f64::INFINITY,
        lat_max: f64::NEG_INFINITY,
        lon_min: f64::INFINITY,
        lon_max: f64::NEG_INFINITY,
    };
    for (x, y) in mercator_points {
        let (lat, lon) = if coordinates_are_lon_lat {
            (y, x)
        } else {
            web_mercator_to_lat_lon(x, y)
        };
        coverage.lat_min = coverage.lat_min.min(lat);
        coverage.lat_max = coverage.lat_max.max(lat);
        coverage.lon_min = coverage.lon_min.min(lon);
        coverage.lon_max = coverage.lon_max.max(lon);
    }
    Ok(coverage)
}

fn collect_coordinate_pairs(value: &serde_json::Value, output: &mut Vec<(f64, f64)>) {
    let Some(values) = value.as_array() else {
        return;
    };
    if values.len() >= 2 {
        if let (Some(x), Some(y)) = (values[0].as_f64(), values[1].as_f64()) {
            output.push((x, y));
            return;
        }
    }
    for child in values {
        collect_coordinate_pairs(child, output);
    }
}

fn web_mercator_to_lat_lon(x: f64, y: f64) -> (f64, f64) {
    const EARTH_RADIUS_M: f64 = 6_378_137.0;
    let lon = (x / EARTH_RADIUS_M).to_degrees();
    let lat = (2.0 * (y / EARTH_RADIUS_M).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    (lat, lon)
}

fn inspect_raster(path: &Path) -> anyhow::Result<RasterInspection> {
    let output = Command::new("gdalinfo")
        .args(["-json", "-nomd", "-noct"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "gdalinfo failed for {}: {}",
            path.display(),
            command_output_diagnostic_summary(&output)
        );
    }
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("gdalinfo output for {} was not JSON", path.display()))?;
    let size = document
        .get("size")
        .and_then(|value| value.as_array())
        .filter(|value| value.len() == 2)
        .with_context(|| format!("gdalinfo omitted raster size for {}", path.display()))?;
    let width = size[0]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .with_context(|| format!("gdalinfo returned invalid width for {}", path.display()))?;
    let height = size[1]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .with_context(|| format!("gdalinfo returned invalid height for {}", path.display()))?;
    let has_palette = document
        .get("bands")
        .and_then(|value| value.as_array())
        .is_some_and(|bands| {
            bands.iter().any(|band| {
                band.get("colorInterpretation")
                    .and_then(|value| value.as_str())
                    == Some("Palette")
            })
        });
    Ok(RasterInspection {
        dimensions: (width, height),
        has_palette,
    })
}

fn validate_chart_extract_layout(
    layout: &ChartExtractLayout,
    layout_path: &Path,
    actual_dimensions: (u32, u32),
    kind: ChartExtractKind,
) -> anyhow::Result<()> {
    let metadata_type = kind.metadata_type();
    if layout.schema_version != 1 {
        bail!(
            "unsupported {metadata_type} schema_version {} in {}",
            layout.schema_version,
            layout_path.display()
        );
    }
    let source = Path::new(&layout.source);
    if source.components().count() != 1 || source.file_name().is_none() {
        bail!(
            "{metadata_type} source must be a file name in {}",
            layout_path.display()
        );
    }
    let expected_layout_name = format!(
        "{}.{metadata_type}.json",
        source
            .file_stem()
            .and_then(|value| value.to_str())
            .with_context(|| format!("{metadata_type} source has no UTF-8 file stem"))?
    );
    if layout_path.file_name().and_then(|value| value.to_str()) != Some(&expected_layout_name) {
        bail!(
            "{metadata_type} layout {} must be named {expected_layout_name}",
            layout_path.display()
        );
    }
    if (layout.source_width, layout.source_height) != actual_dimensions {
        bail!(
            "{metadata_type} layout {} expects {}x{} source but {} is {}x{}",
            layout_path.display(),
            layout.source_width,
            layout.source_height,
            layout.source,
            actual_dimensions.0,
            actual_dimensions.1
        );
    }
    if let Some(coverage_source) = &layout.coverage_source {
        let path = Path::new(coverage_source);
        if coverage_source.is_empty()
            || path.components().count() != 1
            || path.file_name().and_then(|value| value.to_str()) != Some(coverage_source)
        {
            bail!(
                "{metadata_type} coverage_source must be a chart stem in {}",
                layout_path.display()
            );
        }
    }
    if !(320..=4096).contains(&layout.max_output_width) {
        bail!("{metadata_type} max_output_width must be between 320 and 4096");
    }
    for (index, region) in layout.regions.iter().enumerate() {
        if region.width == 0 || region.height == 0 {
            bail!("{metadata_type} region {} has zero area", index + 1);
        }
        let right = region
            .x
            .checked_add(region.width)
            .with_context(|| format!("{metadata_type} region x overflow"))?;
        let bottom = region
            .y
            .checked_add(region.height)
            .with_context(|| format!("{metadata_type} region y overflow"))?;
        if right > layout.source_width || bottom > layout.source_height {
            bail!(
                "{metadata_type} region {} exceeds {}x{} source bounds",
                index + 1,
                layout.source_width,
                layout.source_height
            );
        }
    }
    Ok(())
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
    let vrts = vfr_vrt_paths(work_dir, &inputs)?;

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
        bail!(
            "python glob enumeration failed under {}; {}",
            chart_dir.display(),
            command_output_diagnostic_summary(&output)
        );
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
    let tif_name = resolve_chart_input_filename(work_dir, base_name, "tif")?;
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
    translate.ensure_success(
        &translate_outcome,
        &format!("gdal_translate failed for {base_name}"),
    )?;

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
            rgb_vrt_name.clone(),
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
    warp.ensure_success(&warp_outcome, &format!("gdalwarp failed for {base_name}"))?;

    if let Some(supplement) = antimeridian_supplement_from_chart_metadata(work_dir, base_name)? {
        build_vfr_antimeridian_supplement_vrt(
            work_dir,
            base_name,
            &rgb_vrt_name,
            supplement,
            family,
            worker_index,
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AntimeridianSupplement {
    west_lon: f64,
    south_lat: f64,
    east_lon: f64,
    north_lat: f64,
}

fn vfr_vrt_paths(work_dir: &Path, inputs: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    let mut vrts = Vec::new();
    for base_name in inputs {
        vrts.push(work_dir.join(format!("{base_name}.vrt")));
        if antimeridian_supplement_from_chart_metadata(work_dir, base_name)?.is_some() {
            vrts.push(antimeridian_supplement_vrt_path(work_dir, base_name));
        }
    }
    Ok(vrts)
}

fn antimeridian_supplement_from_chart_metadata(
    work_dir: &Path,
    base_name: &str,
) -> anyhow::Result<Option<AntimeridianSupplement>> {
    let html_name = resolve_chart_input_filename(work_dir, base_name, "htm")?;
    let html = fs::read_to_string(work_dir.join(html_name))
        .with_context(|| format!("failed to read metadata for {base_name}"))?;
    Ok(antimeridian_supplement_from_html(&html))
}

fn antimeridian_supplement_from_html(html: &str) -> Option<AntimeridianSupplement> {
    let west_lon = html_meta_content_f64(html, "dc.coverage.x.min")?;
    let wrapped_east_lon = html_meta_content_f64(html, "dc.coverage.x.max")?;
    if west_lon <= wrapped_east_lon {
        return None;
    }
    // FAA VFR metadata encodes dateline-crossing charts as x.min > x.max.
    // The normal cutline warp only covers the wrapped negative-longitude side;
    // add a tightly bounded positive-longitude warp so tiles just west of
    // +180 are not silently all-nodata.
    Some(AntimeridianSupplement {
        west_lon,
        south_lat: html_meta_content_f64(html, "dc.coverage.y.min")?,
        east_lon: 180.0,
        north_lat: html_meta_content_f64(html, "dc.coverage.y.max")?,
    })
}

fn html_meta_content_f64(html: &str, name: &str) -> Option<f64> {
    for line in html.lines() {
        if !line.contains(name) {
            continue;
        }
        let content_start = line.find("content=\"")? + "content=\"".len();
        let content_end = line[content_start..].find('"')? + content_start;
        return line[content_start..content_end].parse().ok();
    }
    None
}

fn antimeridian_supplement_vrt_path(work_dir: &Path, base_name: &str) -> PathBuf {
    work_dir.join(format!("{base_name} antimeridian-east.vrt"))
}

fn build_vfr_antimeridian_supplement_vrt(
    work_dir: &Path,
    base_name: &str,
    rgb_vrt_name: &str,
    supplement: AntimeridianSupplement,
    family: ChartFamily,
    worker_index: usize,
) -> anyhow::Result<()> {
    let vrt_path = antimeridian_supplement_vrt_path(work_dir, base_name);
    remove_if_exists(vrt_path.clone())?;
    let vrt_name = vrt_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("failed to derive antimeridian VRT filename"))?
        .to_string();
    let logs_dir = work_dir.join(".rust-logs");
    let family_label = family.capture_label();

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
            "-te".to_string(),
            supplement.west_lon.to_string(),
            supplement.south_lat.to_string(),
            supplement.east_lon.to_string(),
            supplement.north_lat.to_string(),
            "-te_srs".to_string(),
            "EPSG:4326".to_string(),
            rgb_vrt_name.to_string(),
            vrt_name,
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "{family_label}-warp-{worker_index}-{}-antimeridian-east",
            sanitize_label(base_name)
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let warp_outcome = warp.run_logged(&logs_dir)?;
    warp.ensure_success(
        &warp_outcome,
        &format!("gdalwarp failed for {base_name} antimeridian supplement"),
    )?;
    Ok(())
}

fn build_one_ifr_vrt(
    work_dir: &Path,
    base_name: &str,
    chart_dir_name: &str,
    family: ChartFamily,
    worker_index: usize,
) -> anyhow::Result<()> {
    let tif_name = resolve_chart_input_filename(work_dir, base_name, "tif")?;
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
    warp.ensure_success(&warp_outcome, &format!("gdalwarp failed for {base_name}"))?;

    Ok(())
}

fn remove_if_exists(path: PathBuf) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove stale {}", path.display()))?;
    }
    Ok(())
}

fn resolve_chart_input_filename(
    work_dir: &Path,
    base_name: &str,
    extension: &str,
) -> anyhow::Result<String> {
    let expected = format!("{base_name}.{extension}");
    if work_dir.join(&expected).is_file() {
        return Ok(expected);
    }
    let expected_lower = expected.to_ascii_lowercase();
    for entry in fs::read_dir(work_dir)
        .with_context(|| format!("failed to read chart work dir {}", work_dir.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.to_ascii_lowercase() == expected_lower && entry.path().is_file() {
            return Ok(name.to_string());
        }
    }
    bail!(
        "missing chart input {expected} under {}",
        work_dir.display()
    )
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
    invocation.ensure_success(&outcome, &format!("gdalbuildvrt failed for {chart_name}"))?;
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
            format!("0-{}", spec.detail_zoom()),
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
    invocation.ensure_success(
        &outcome,
        &format!("gdal2tiles failed for {}", spec.family.capture_label()),
    )?;

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
    let sources = [PackageChartSource { spec, work_dir }];
    let mut package_records = package_region_records_from_sources(
        work_dir,
        &sources,
        &regions,
        ChartPackageTier::Regional,
        true,
        &manifest_cycle,
        &manifest_cycle,
    )?;
    package_records.extend(package_region_records_from_sources(
        work_dir,
        &sources,
        &regions,
        ChartPackageTier::Detail,
        true,
        &manifest_cycle,
        &manifest_cycle,
    )?);
    package_records.push(package_wide_angle_record_from_sources(
        work_dir,
        &sources,
        true,
        &manifest_cycle,
        &manifest_cycle,
    )?);

    if let Some(provenance_dir) = provenance_dir {
        write_package_outputs_jsonl(provenance_dir, &package_records)?;
    }

    Ok(PackageBuildResult {
        family: spec.family,
        package_count: package_records.len(),
        elapsed_ms: start.elapsed().as_millis(),
    })
}

fn chart_package_tier_filename_token(tier: ChartPackageTier) -> &'static str {
    match tier {
        ChartPackageTier::Regional => "",
        ChartPackageTier::Detail => "_DETAIL",
        ChartPackageTier::Wide => unreachable!("wide chart packages use a separate package path"),
    }
}

fn package_region_records_from_sources(
    output_dir: &Path,
    sources: &[PackageChartSource<'_>],
    regions: &[Region],
    tier: ChartPackageTier,
    produce_records: bool,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<Vec<PackageOutputRecord>> {
    let primary = sources.first().context("chart package has no sources")?;
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let tile_paths = sources
        .iter()
        .map(|source| {
            Ok((
                *source,
                collect_tile_paths_glob(source.work_dir, source.spec.tile_index)?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut package_records = Vec::with_capacity(regions.len());

    for region in regions {
        let manifest_name = format!(
            "{}_{}{}_{}.manifest",
            region.code(),
            primary.spec.chart_name,
            chart_package_tier_filename_token(tier),
            artifact_version
        );
        let zip_name = format!(
            "{}_{}{}_{}.zip",
            region.code(),
            primary.spec.chart_name,
            chart_package_tier_filename_token(tier),
            artifact_version
        );
        let manifest_path = output_dir.join(&manifest_name);
        let zip_path = output_dir.join(&zip_name);

        if zip_path.exists() {
            fs::remove_file(&zip_path)
                .with_context(|| format!("failed to remove {}", zip_path.display()))?;
        }

        let mut selected = Vec::<(String, PathBuf)>::new();
        for (source, paths) in &tile_paths {
            for tile_path in paths {
                if tile_belongs_to_region_tier(tile_path, region, source.spec, tier) {
                    selected.push((tile_path.clone(), source.work_dir.join(tile_path)));
                }
            }
        }
        let included_sources = sources
            .iter()
            .copied()
            .filter(|source| {
                let prefix = format!("tiles/{}/", source.spec.tile_index);
                selected.iter().any(|(path, _)| path.starts_with(&prefix))
            })
            .collect::<Vec<_>>();

        let mut manifest_text = String::new();
        manifest_text.push_str(manifest_version);
        manifest_text.push('\n');
        for (path, _) in &selected {
            manifest_text.push_str(path);
            manifest_text.push('\n');
        }
        fs::write(&manifest_path, manifest_text)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;

        let package_id = manifest_name.trim_end_matches(".manifest");
        let reference_groups = if tier == ChartPackageTier::Regional {
            sources
                .iter()
                .map(|source| {
                    let assets = chart_reference_assets_for_package(source.work_dir, Some(region))?;
                    let manifest = write_chart_reference_manifest(
                        output_dir,
                        source.spec.family,
                        package_id,
                        &assets,
                        source.spec.family == primary.spec.family,
                    )?;
                    Ok((*source, assets, manifest))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };

        write_chart_package_zip(
            &zip_path,
            &selected,
            &manifest_name,
            &manifest_path,
            &reference_groups,
        )?;

        if produce_records {
            let metadata = chart_package_metadata(tier, selected.len() as u64, &included_sources);
            package_records.push(PackageOutputRecord {
                label: primary.spec.family.capture_label().to_string(),
                chart: Some(primary.spec.chart_name.to_string()),
                region: region.code().to_ascii_lowercase(),
                manifest: manifest_name,
                manifest_sha256: hash_file(&manifest_path)?,
                zip: zip_name,
                zip_sha256: hash_file(&zip_path)?,
                metadata,
            });
        }
    }

    Ok(package_records)
}

fn package_wide_angle_record_from_sources(
    output_dir: &Path,
    sources: &[PackageChartSource<'_>],
    produce_record: bool,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    let primary = sources.first().context("chart package has no sources")?;
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let manifest_name = format!(
        "WIDE_{}_{}.manifest",
        primary.spec.chart_name, artifact_version
    );
    let zip_name = format!("WIDE_{}_{}.zip", primary.spec.chart_name, artifact_version);
    let manifest_path = output_dir.join(&manifest_name);
    let zip_path = output_dir.join(&zip_name);

    if zip_path.exists() {
        fs::remove_file(&zip_path)
            .with_context(|| format!("failed to remove {}", zip_path.display()))?;
    }

    let mut selected = Vec::<(String, PathBuf)>::new();
    for source in sources {
        for tile_path in collect_tile_paths_glob(source.work_dir, source.spec.tile_index)? {
            if tile_belongs_to_wide_angle(&tile_path) {
                selected.push((tile_path.clone(), source.work_dir.join(tile_path)));
            }
        }
    }
    let included_sources = sources
        .iter()
        .copied()
        .filter(|source| {
            let prefix = format!("tiles/{}/", source.spec.tile_index);
            selected.iter().any(|(path, _)| path.starts_with(&prefix))
        })
        .collect::<Vec<_>>();

    let mut manifest_text = String::new();
    manifest_text.push_str(manifest_version);
    manifest_text.push('\n');
    for (path, _) in &selected {
        manifest_text.push_str(path);
        manifest_text.push('\n');
    }
    fs::write(&manifest_path, manifest_text)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let package_id = manifest_name.trim_end_matches(".manifest");
    let reference_groups = sources
        .iter()
        .map(|source| {
            let assets = chart_reference_assets_for_package(source.work_dir, None)?;
            let manifest = write_chart_reference_manifest(
                output_dir,
                source.spec.family,
                package_id,
                &assets,
                source.spec.family == primary.spec.family,
            )?;
            Ok((*source, assets, manifest))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    write_chart_package_zip(
        &zip_path,
        &selected,
        &manifest_name,
        &manifest_path,
        &reference_groups,
    )?;

    if produce_record {
        let metadata = chart_package_metadata(
            ChartPackageTier::Wide,
            selected.len() as u64,
            &included_sources,
        );
        Ok(PackageOutputRecord {
            label: primary.spec.family.capture_label().to_string(),
            chart: Some(primary.spec.chart_name.to_string()),
            region: WIDE_ANGLE_REGION_ID.to_string(),
            manifest: manifest_name,
            manifest_sha256: hash_file(&manifest_path)?,
            zip: zip_name,
            zip_sha256: hash_file(&zip_path)?,
            metadata,
        })
    } else {
        bail!("wide-angle package record requested without record production")
    }
}

fn write_chart_package_zip(
    zip_path: &Path,
    selected_tiles: &[(String, PathBuf)],
    manifest_name: &str,
    manifest_path: &Path,
    reference_groups: &[(
        PackageChartSource<'_>,
        Vec<ChartReferenceAssetRecord>,
        (String, PathBuf),
    )],
) -> anyhow::Result<()> {
    let mut members = selected_tiles
        .iter()
        .map(|(member, source)| ZipSource::new(member, source).stored())
        .collect::<Vec<_>>();
    for (source, assets, (manifest_member, manifest_path)) in reference_groups {
        for asset in assets {
            members.push(
                ZipSource::new(&asset.asset_path, source.work_dir.join(&asset.asset_path)).stored(),
            );
            members.push(
                ZipSource::new(
                    &asset.thumbnail_path,
                    source.work_dir.join(&asset.thumbnail_path),
                )
                .stored(),
            );
        }
        members.push(ZipSource::new(manifest_member, manifest_path).stored());
    }
    members.push(ZipSource::new(manifest_name, manifest_path).stored());
    write_deterministic_zip(zip_path, &members)
}

fn chart_reference_assets_for_package(
    work_dir: &Path,
    region: Option<&Region>,
) -> anyhow::Result<Vec<ChartReferenceAssetRecord>> {
    let catalog_path = work_dir.join(CHART_REFERENCE_CATALOG_NAME);
    let catalog: ChartReferenceCatalog = serde_json::from_slice(
        &fs::read(&catalog_path)
            .with_context(|| format!("failed to read {}", catalog_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", catalog_path.display()))?;
    Ok(catalog
        .assets
        .into_iter()
        .filter(|asset| match region {
            None => asset.kind == "legend",
            Some(region) => {
                asset.kind == "inset"
                    && asset.source_coverage.is_some_and(|coverage| {
                        region
                            .bounds_list()
                            .iter()
                            .any(|bounds| reference_coverage_intersects_region(coverage, *bounds))
                    })
            }
        })
        .collect())
}

fn reference_coverage_intersects_region(
    coverage: ChartReferenceCoverage,
    region: RegionBounds,
) -> bool {
    coverage.lon_min < region.lon_max
        && coverage.lon_max > region.lon_min
        && coverage.lat_min < region.lat_max
        && coverage.lat_max > region.lat_min
}

fn write_chart_reference_manifest(
    output_dir: &Path,
    family: ChartFamily,
    package_id: &str,
    assets: &[ChartReferenceAssetRecord],
    primary: bool,
) -> anyhow::Result<(String, PathBuf)> {
    let member = if primary {
        format!("{CHART_REFERENCE_MANIFEST_DIR}/{package_id}.json")
    } else {
        format!(
            "{CHART_REFERENCE_MANIFEST_DIR}/{package_id}-{}.json",
            chart_family_id(family)
        )
    };
    let path = output_dir.join(&member);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        &path,
        serde_json::to_vec_pretty(&ChartReferenceManifest {
            schema_version: 1,
            family_id: chart_family_id(family).to_string(),
            package_id: package_id.to_string(),
            assets: assets.to_vec(),
        })?,
    )
    .with_context(|| format!("failed to write {}", path.display()))?;
    Ok((member, path))
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
        bail!(
            "python tile enumeration failed under {} tile_index={}; {}",
            work_dir.display(),
            tile_index,
            command_output_diagnostic_summary(&output)
        );
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
    let Some((z, x, y)) = tile_path_xyz(tile_path) else {
        return false;
    };

    if z <= FULL_COVERAGE_ZOOM {
        return false;
    }

    let (tile_lon_min, tile_lat_min, tile_lon_max, tile_lat_max) = find_bounds(x, y, z);
    region.bounds_list().iter().any(|bounds| {
        tile_overlaps_region_bounds(
            tile_lon_min,
            tile_lat_min,
            tile_lon_max,
            tile_lat_max,
            *bounds,
        )
    })
}

fn tile_belongs_to_region_tier(
    tile_path: &str,
    region: &Region,
    spec: ChartSpec,
    tier: ChartPackageTier,
) -> bool {
    let Some((zoom, _, _)) = tile_path_xyz(tile_path) else {
        return false;
    };
    let zoom_matches = match tier {
        ChartPackageTier::Regional => zoom > FULL_COVERAGE_ZOOM && zoom <= spec.base_max_zoom,
        ChartPackageTier::Detail => zoom == spec.detail_zoom(),
        ChartPackageTier::Wide => unreachable!("wide packages use the wide-angle tile selector"),
    };
    zoom_matches && tile_belongs_to_region(tile_path, region)
}

fn tile_overlaps_region_bounds(
    tile_lon_min: f64,
    tile_lat_min: f64,
    tile_lon_max: f64,
    tile_lat_max: f64,
    bounds: RegionBounds,
) -> bool {
    let RegionBounds {
        lon_min: region_lon_min,
        lat_max: region_lat_max,
        lon_max: region_lon_max,
        lat_min: region_lat_min,
    } = bounds;
    let lon_overlap = tile_lon_max >= region_lon_min && tile_lon_min <= region_lon_max;
    let lat_overlap = tile_lat_max >= region_lat_min && tile_lat_min <= region_lat_max;
    lon_overlap && lat_overlap
}

fn tile_belongs_to_wide_angle(tile_path: &str) -> bool {
    tile_path_xyz(tile_path)
        .map(|(z, _x, _y)| z <= FULL_COVERAGE_ZOOM)
        .unwrap_or(false)
}

fn tile_path_xyz(tile_path: &str) -> Option<(u32, u32, u32)> {
    let tokens: Vec<&str> = tile_path.split('/').collect();
    let (z_index, x_index, y_index) = match tokens.len() {
        4 => (1, 2, 3),
        5 if tokens[0] == "tiles" => (2, 3, 4),
        _ => return None,
    };
    let z = match tokens[z_index].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let x = match tokens[x_index].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let y = match tokens[y_index].trim_end_matches(".webp").parse::<u32>() {
        Ok(value) => value,
        Err(_) => return None,
    };
    Some((z, x, y))
}

fn chart_package_metadata(
    tier: ChartPackageTier,
    tile_count: u64,
    sources: &[PackageChartSource<'_>],
) -> BTreeMap<String, serde_json::Value> {
    let is_wide_angle = tier == ChartPackageTier::Wide;
    let mut metadata = BTreeMap::from([
        (
            CHART_PACKAGE_TIER_METADATA_KEY.to_string(),
            serde_json::Value::from(tier.as_str()),
        ),
        (
            "wide_angle_region_id".to_string(),
            serde_json::Value::from(WIDE_ANGLE_REGION_ID),
        ),
        (
            "wide_angle_max_zoom".to_string(),
            serde_json::Value::from(FULL_COVERAGE_ZOOM),
        ),
        (
            "wide_angle".to_string(),
            serde_json::Value::from(is_wide_angle),
        ),
        (
            if is_wide_angle {
                "max_source_zoom".to_string()
            } else {
                "min_source_zoom".to_string()
            },
            serde_json::Value::from(if is_wide_angle {
                FULL_COVERAGE_ZOOM
            } else {
                FULL_COVERAGE_ZOOM + 1
            }),
        ),
    ]);
    metadata.insert(
        "tile_count".to_string(),
        serde_json::Value::from(tile_count),
    );
    if !is_wide_angle {
        let min_zoom = if tier == ChartPackageTier::Detail {
            sources
                .iter()
                .map(|source| source.spec.detail_zoom())
                .min()
                .unwrap_or(FULL_COVERAGE_ZOOM + 1)
        } else {
            FULL_COVERAGE_ZOOM + 1
        };
        let max_zoom = sources
            .iter()
            .map(|source| {
                if tier == ChartPackageTier::Detail {
                    source.spec.detail_zoom()
                } else {
                    source.spec.base_max_zoom
                }
            })
            .max()
            .unwrap_or(min_zoom);
        metadata.insert(
            "min_source_zoom".to_string(),
            serde_json::Value::from(min_zoom),
        );
        metadata.insert(
            "max_source_zoom".to_string(),
            serde_json::Value::from(max_zoom),
        );
    }
    metadata.insert(
        "chart_collections".to_string(),
        serde_json::to_value(
            sources
                .iter()
                .map(|source| ChartPackageCollection {
                    family_id: chart_family_id(source.spec.family).to_string(),
                    chart_index: source
                        .spec
                        .tile_index
                        .parse::<u32>()
                        .expect("numeric chart tile index"),
                })
                .collect::<Vec<_>>(),
        )
        .expect("chart package collection metadata is serializable"),
    );
    metadata
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
    use super::{
        antimeridian_supplement_from_html, build_family_insets, build_family_legends,
        build_family_reference_catalog, copy_dir_recursive, inspect_raster,
        package_family_bundle_detail_region_versioned_to,
        package_family_bundle_region_versioned_to, package_family_region_versioned_to,
        package_family_wide_angle_versioned_to, resolve_chart_input_filename,
        source_chart_coverage, tile_belongs_to_region, validate_chart_extract_layout,
        vfr_vrt_paths, AntimeridianSupplement, ChartExtractKind, ChartExtractLayout,
        ChartExtractRegion, ChartReferenceCatalog, CHART_REFERENCE_CATALOG_NAME,
    };
    use preprocessor_core::{
        ChartFamily, ChartReferenceAssetRecord, ChartReferenceCoverage, Region,
    };
    use product_contracts::{ChartPackageTier, CHART_PACKAGE_TIER_METADATA_KEY};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
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
    fn chart_legend_layout_rejects_stale_dimensions_and_out_of_bounds_regions() {
        let path = Path::new("Seattle TAC.legend.json");
        let mut layout = ChartExtractLayout {
            schema_version: 1,
            source: "Seattle TAC.tif".to_string(),
            source_width: 1000,
            source_height: 800,
            max_output_width: 1210,
            coverage_source: None,
            regions: vec![ChartExtractRegion {
                x: 100,
                y: 200,
                width: 300,
                height: 400,
            }],
        };
        validate_chart_extract_layout(&layout, path, (1000, 800), ChartExtractKind::Legend)
            .unwrap();
        assert!(validate_chart_extract_layout(
            &layout,
            path,
            (1001, 800),
            ChartExtractKind::Legend
        )
        .is_err());
        assert!(validate_chart_extract_layout(
            &layout,
            Path::new("Seattle TAC.inset.json"),
            (1000, 800),
            ChartExtractKind::Inset
        )
        .is_ok());
        layout.regions[0].width = 901;
        assert!(validate_chart_extract_layout(
            &layout,
            path,
            (1000, 800),
            ChartExtractKind::Legend
        )
        .is_err());
    }

    #[test]
    fn chart_legend_renderer_crops_and_stacks_regions() {
        let temp = TempDir::new("chart-legend-render");
        let work_dir = temp.path().join("charts-tac");
        let layout_dir = work_dir.join("TAC");
        fs::create_dir_all(&layout_dir).unwrap();
        assert!(Command::new("convert")
            .args(["-size", "8x6", "gradient:white-black", "-colors", "16"])
            .arg(format!("PNG8:{}", work_dir.join("Test TAC.tif").display()))
            .status()
            .unwrap()
            .success());
        fs::write(
            layout_dir.join("Test TAC.legend.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "source": "Test TAC.tif",
                "source_width": 8,
                "source_height": 6,
                "max_output_width": 320,
                "regions": [
                    {"x": 0, "y": 0, "width": 4, "height": 2},
                    {"x": 4, "y": 2, "width": 4, "height": 3}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::copy(
            layout_dir.join("Test TAC.legend.json"),
            layout_dir.join("Test TAC.inset.json"),
        )
        .unwrap();

        let result = build_family_legends(ChartFamily::Tac, &work_dir).unwrap();
        let inset_result = build_family_insets(ChartFamily::Tac, &work_dir).unwrap();
        assert_eq!(result.kind, ChartExtractKind::Legend);
        assert_eq!(inset_result.kind, ChartExtractKind::Inset);
        assert_eq!(result.output_paths.len(), 1);
        assert_eq!(inset_result.output_paths.len(), 1);
        assert_eq!(
            inspect_raster(&result.output_paths[0]).unwrap().dimensions,
            (4, 5)
        );
        assert_eq!(
            inspect_raster(&inset_result.output_paths[0])
                .unwrap()
                .dimensions,
            (4, 5)
        );
        assert_ne!(result.output_paths[0], inset_result.output_paths[0]);
    }

    #[test]
    fn chart_legend_renderer_expands_palette_before_resampling() {
        let temp = TempDir::new("chart-legend-palette-resample");
        let work_dir = temp.path().join("charts-tac");
        let layout_dir = work_dir.join("TAC");
        fs::create_dir_all(&layout_dir).unwrap();
        let palette_path = work_dir.join("palette.png");
        assert!(Command::new("convert")
            .args(["-size", "641x201", "pattern:checkerboard", "-colors", "2"])
            .arg(format!("PNG8:{}", palette_path.display()))
            .status()
            .unwrap()
            .success());
        assert!(Command::new("gdal_translate")
            .args(["-q", "-of", "GTiff"])
            .arg(&palette_path)
            .arg(work_dir.join("Test TAC.tif"))
            .status()
            .unwrap()
            .success());
        fs::write(
            layout_dir.join("Test TAC.legend.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "source": "Test TAC.tif",
                "source_width": 641,
                "source_height": 201,
                "max_output_width": 320,
                "regions": [
                    {"x": 0, "y": 0, "width": 641, "height": 201}
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let result = build_family_legends(ChartFamily::Tac, &work_dir).unwrap();
        let output = Command::new("identify")
            .args(["-format", "%k"])
            .arg(&result.output_paths[0])
            .output()
            .unwrap();
        assert!(output.status.success());
        let color_count = String::from_utf8(output.stdout)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        assert!(
            color_count > 2,
            "RGB resampling should introduce anti-aliased intermediate colors, got {color_count}"
        );
    }

    #[test]
    fn chart_legend_renderer_accepts_rgb_source_without_palette_expansion() {
        let temp = TempDir::new("chart-legend-rgb-source");
        let work_dir = temp.path().join("charts-enr-l");
        let layout_dir = work_dir.join("ENR_L");
        fs::create_dir_all(&layout_dir).unwrap();
        let ppm_path = work_dir.join("source.ppm");
        let mut ppm = b"P6\n8 6\n255\n".to_vec();
        for index in 0..48_u8 {
            ppm.extend([index, 255 - index, index.saturating_mul(3)]);
        }
        fs::write(&ppm_path, ppm).unwrap();
        assert!(Command::new("gdal_translate")
            .args(["-q", "-of", "GTiff"])
            .arg(&ppm_path)
            .arg(work_dir.join("Test IFR.tif"))
            .status()
            .unwrap()
            .success());
        fs::write(
            layout_dir.join("Test IFR.legend.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "source": "Test IFR.tif",
                "source_width": 8,
                "source_height": 6,
                "max_output_width": 320,
                "regions": [
                    {"x": 1, "y": 1, "width": 6, "height": 4}
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            !inspect_raster(&work_dir.join("Test IFR.tif"))
                .unwrap()
                .has_palette
        );
        let result = build_family_legends(ChartFamily::EnrL, &work_dir).unwrap();
        assert_eq!(result.output_paths.len(), 1);
        assert_eq!(
            inspect_raster(&result.output_paths[0]).unwrap().dimensions,
            (6, 4)
        );
    }

    #[test]
    fn chart_reference_coverage_accepts_crs84_lon_lat_coordinates() {
        let temp = TempDir::new("chart-reference-crs84");
        fs::write(
            temp.path().join("Las Vegas TAC.geojson"),
            serde_json::to_vec(&serde_json::json!({
                "type": "FeatureCollection",
                "crs": {
                    "type": "name",
                    "properties": {"name": "urn:ogc:def:crs:OGC:1.3:CRS84"}
                },
                "features": [{
                    "type": "Feature",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[
                            [-115.6, 35.7],
                            [-113.8, 35.7],
                            [-113.8, 36.8],
                            [-115.6, 36.8],
                            [-115.6, 35.7]
                        ]]
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            source_chart_coverage(temp.path(), "Las Vegas TAC").unwrap(),
            ChartReferenceCoverage {
                lat_min: 35.7,
                lat_max: 36.8,
                lon_min: -115.6,
                lon_max: -113.8,
            }
        );
    }

    #[test]
    fn unreferenced_extract_uses_parent_chart_coverage() {
        let temp = TempDir::new("chart-reference-parent-coverage");
        let work_dir = temp.path().join("charts-tac");
        let layout_dir = work_dir.join("TAC");
        fs::create_dir_all(work_dir.join("insets")).unwrap();
        fs::create_dir_all(work_dir.join("thumbnails/insets")).unwrap();
        fs::create_dir_all(&layout_dir).unwrap();
        fs::write(work_dir.join("insets/Reference Sheet.png"), b"image").unwrap();
        fs::write(
            work_dir.join("thumbnails/insets/Reference Sheet.png"),
            b"thumbnail",
        )
        .unwrap();
        fs::write(
            layout_dir.join("Reference Sheet.inset.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "source": "Reference Sheet.tif",
                "source_width": 100,
                "source_height": 100,
                "max_output_width": 1210,
                "coverage_source": "Parent TAC",
                "regions": [{"x": 0, "y": 0, "width": 100, "height": 100}]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            layout_dir.join("Parent TAC.geojson"),
            serde_json::to_vec(&serde_json::json!({
                "type": "FeatureCollection",
                "crs": {
                    "type": "name",
                    "properties": {"name": "urn:ogc:def:crs:OGC:1.3:CRS84"}
                },
                "features": [{
                    "type": "Feature",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[
                            [-150.0, 60.0],
                            [-149.0, 60.0],
                            [-149.0, 61.0],
                            [-150.0, 61.0],
                            [-150.0, 60.0]
                        ]]
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let catalog_path = build_family_reference_catalog(ChartFamily::Tac, &work_dir).unwrap();
        let catalog: ChartReferenceCatalog =
            serde_json::from_slice(&fs::read(catalog_path).unwrap()).unwrap();

        assert_eq!(catalog.assets.len(), 1);
        assert_eq!(catalog.assets[0].source_chart_id, "Reference Sheet");
        assert_eq!(
            catalog.assets[0].source_coverage,
            Some(ChartReferenceCoverage {
                lat_min: 60.0,
                lat_max: 61.0,
                lon_min: -150.0,
                lon_max: -149.0,
            })
        );
    }

    #[test]
    fn chart_packages_put_legends_in_wide_and_insets_with_source_region() {
        let temp = TempDir::new("chart-reference-packages");
        let work_dir = temp.path.join("work");
        let output_dir = temp.path.join("packages");
        for member in [
            "legends/Seattle TAC.png",
            "thumbnails/legends/Seattle TAC.png",
            "insets/Los Angeles TAC.png",
            "thumbnails/insets/Los Angeles TAC.png",
            "insets/Miami TAC.png",
            "thumbnails/insets/Miami TAC.png",
        ] {
            let path = work_dir.join(member);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"image").unwrap();
        }
        let asset =
            |id: &str, source: &str, kind: &str, path: &str, coverage| ChartReferenceAssetRecord {
                id: id.to_string(),
                family_id: "tac".to_string(),
                source_chart_id: source.to_string(),
                label: source.to_string(),
                kind: kind.to_string(),
                asset_path: path.to_string(),
                thumbnail_path: format!("thumbnails/{path}"),
                source_coverage: coverage,
            };
        let catalog = ChartReferenceCatalog {
            schema_version: 1,
            family_id: "tac".to_string(),
            assets: vec![
                asset(
                    "legend",
                    "Seattle TAC",
                    "legend",
                    "legends/Seattle TAC.png",
                    None,
                ),
                asset(
                    "la",
                    "Los Angeles TAC",
                    "inset",
                    "insets/Los Angeles TAC.png",
                    Some(ChartReferenceCoverage {
                        lat_min: 32.0,
                        lat_max: 35.0,
                        lon_min: -120.0,
                        lon_max: -116.0,
                    }),
                ),
                asset(
                    "miami",
                    "Miami TAC",
                    "inset",
                    "insets/Miami TAC.png",
                    Some(ChartReferenceCoverage {
                        lat_min: 24.0,
                        lat_max: 27.0,
                        lon_min: -82.0,
                        lon_max: -79.0,
                    }),
                ),
            ],
        };
        fs::write(
            work_dir.join(CHART_REFERENCE_CATALOG_NAME),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();

        let wide = package_family_wide_angle_versioned_to(
            ChartFamily::Tac,
            &work_dir,
            &output_dir,
            "2607",
            "2607-test",
        )
        .unwrap();
        let sw = package_family_region_versioned_to(
            ChartFamily::Tac,
            &work_dir,
            &output_dir,
            Region::Sw,
            "2607",
            "2607-test",
        )
        .unwrap();
        let wide_entries = zip_entries(&output_dir.join(wide.zip));
        let sw_entries = zip_entries(&output_dir.join(sw.zip));
        assert!(wide_entries
            .iter()
            .any(|entry| entry == "legends/Seattle TAC.png"));
        assert!(!wide_entries.iter().any(|entry| entry.contains("insets/")));
        assert!(sw_entries
            .iter()
            .any(|entry| entry == "insets/Los Angeles TAC.png"));
        assert!(!sw_entries
            .iter()
            .any(|entry| entry == "insets/Miami TAC.png"));
    }

    #[test]
    fn tac_package_can_carry_independent_flyway_tiles_and_references() {
        let temp = TempDir::new("tac-flyway-package");
        let tac_work = temp.path.join("tac");
        let flyway_work = temp.path.join("flyway");
        let output_dir = temp.path.join("packages");
        for (work_dir, family_id, tile_path) in [
            (&tac_work, "tac", "tiles/1/11/125/1146.webp"),
            (&flyway_work, "flyway", "tiles/2/11/125/1146.webp"),
        ] {
            let path = work_dir.join(tile_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, family_id.as_bytes()).unwrap();
            fs::write(
                work_dir.join(CHART_REFERENCE_CATALOG_NAME),
                serde_json::to_vec(&ChartReferenceCatalog {
                    schema_version: 1,
                    family_id: family_id.to_string(),
                    assets: vec![],
                })
                .unwrap(),
            )
            .unwrap();
        }

        let record = package_family_bundle_region_versioned_to(
            ChartFamily::Tac,
            &tac_work,
            &[(ChartFamily::Flyway, flyway_work.as_path())],
            &output_dir,
            Region::Pac,
            "2607",
            "TAC1_2607",
        )
        .unwrap();

        let entries = zip_entries(&output_dir.join(record.zip));
        assert!(entries
            .iter()
            .any(|entry| entry == "tiles/1/11/125/1146.webp"));
        assert!(entries
            .iter()
            .any(|entry| entry == "tiles/2/11/125/1146.webp"));
        assert!(entries
            .iter()
            .any(|entry| entry == "chart-references/PAC_TAC_TAC1_2607.json"));
        assert!(entries
            .iter()
            .any(|entry| { entry == "chart-references/PAC_TAC_TAC1_2607-flyway.json" }));
        assert_eq!(
            record.metadata["chart_collections"],
            serde_json::json!([
                {"family_id": "tac", "chart_index": 1},
                {"family_id": "flyway", "chart_index": 2}
            ])
        );
    }

    #[test]
    fn regional_base_and_detail_packages_partition_zoom_levels() {
        let temp = TempDir::new("chart-detail-package");
        let work_dir = temp.path.join("sec");
        let output_dir = temp.path.join("packages");
        let tile_paths = [
            "tiles/0/7/6/70.webp",
            "tiles/0/8/13/141.webp",
            "tiles/0/10/52/564.webp",
            "tiles/0/11/104/1128.webp",
        ];
        for tile_path in tile_paths {
            let path = work_dir.join(tile_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, tile_path.as_bytes()).unwrap();
        }
        fs::write(
            work_dir.join(CHART_REFERENCE_CATALOG_NAME),
            serde_json::to_vec(&ChartReferenceCatalog {
                schema_version: 1,
                family_id: "sec".to_string(),
                assets: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();

        let base = package_family_bundle_region_versioned_to(
            ChartFamily::Sec,
            &work_dir,
            &[],
            &output_dir,
            Region::Pac,
            "2607",
            "SEC1_2607",
        )
        .unwrap();
        let detail = package_family_bundle_detail_region_versioned_to(
            ChartFamily::Sec,
            &work_dir,
            &[],
            &output_dir,
            Region::Pac,
            "2607",
            "SEC1_2607",
        )
        .unwrap();

        let base_entries = zip_entries(&output_dir.join(&base.zip));
        let detail_entries = zip_entries(&output_dir.join(&detail.zip));
        assert!(base_entries.iter().any(|entry| entry == tile_paths[1]));
        assert!(base_entries.iter().any(|entry| entry == tile_paths[2]));
        assert!(!base_entries.iter().any(|entry| entry == tile_paths[0]));
        assert!(!base_entries.iter().any(|entry| entry == tile_paths[3]));
        assert_eq!(
            detail_entries
                .iter()
                .filter(|entry| entry.ends_with(".webp"))
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec![tile_paths[3]]
        );
        assert!(!detail_entries
            .iter()
            .any(|entry| entry.starts_with("chart-references/")));
        assert_eq!(
            base.metadata[CHART_PACKAGE_TIER_METADATA_KEY],
            ChartPackageTier::Regional.as_str()
        );
        assert_eq!(
            detail.metadata[CHART_PACKAGE_TIER_METADATA_KEY],
            ChartPackageTier::Detail.as_str()
        );
        assert_eq!(base.metadata["max_source_zoom"], 10);
        assert_eq!(detail.metadata["min_source_zoom"], 11);
        assert_eq!(detail.metadata["max_source_zoom"], 11);
        assert!(detail.zip.contains("_DETAIL_"));
        assert!(detail.manifest.contains("_DETAIL_"));
    }

    fn zip_entries(path: &Path) -> Vec<String> {
        let output = Command::new("unzip")
            .args(["-Z1"])
            .arg(path)
            .output()
            .expect("run unzip");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn chart_input_lookup_tolerates_source_filename_case_drift() {
        let temp = TempDir::new("charts-case-lookup");
        fs::write(temp.path().join("Washington Sec.tif"), b"chart")
            .expect("failed to write chart artifact");

        let resolved = resolve_chart_input_filename(temp.path(), "Washington SEC", "tif")
            .expect("case-insensitive source lookup should succeed");

        assert_eq!(resolved, "Washington Sec.tif");
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

    #[test]
    fn pac_region_admits_detailed_hawaii_samoa_and_guam_tiles() {
        assert!(tile_belongs_to_region(
            "tiles/0/8/13/141.webp",
            &Region::Pac
        ));
        assert!(tile_belongs_to_region(
            "tiles/1/11/125/1146.webp",
            &Region::Pac
        ));
        assert!(tile_belongs_to_region("tiles/0/8/7/117.webp", &Region::Pac));
        assert!(tile_belongs_to_region(
            "tiles/0/9/14/235.webp",
            &Region::Pac
        ));
        assert!(tile_belongs_to_region(
            "tiles/0/8/231/137.webp",
            &Region::Pac
        ));
        assert!(!tile_belongs_to_region("tiles/0/7/3/58.webp", &Region::Pac));
    }

    #[test]
    fn alaska_region_admits_detailed_tiles_on_both_sides_of_antimeridian() {
        assert!(tile_belongs_to_region("tiles/0/8/0/171.webp", &Region::Ak));
        assert!(tile_belongs_to_region(
            "tiles/0/8/255/171.webp",
            &Region::Ak
        ));
        assert!(tile_belongs_to_region(
            "tiles/0/8/253/171.webp",
            &Region::Ak
        ));
        assert!(tile_belongs_to_region(
            "tiles/0/9/510/342.webp",
            &Region::Ak
        ));
    }

    #[test]
    fn antimeridian_chart_metadata_requests_positive_hemisphere_supplement() {
        let html = r#"
            <meta name="dc.coverage.x.min" scheme="DD" content="177.159202"/>
            <meta name="dc.coverage.x.max" scheme="DD" content="-172.350442"/>
            <meta name="dc.coverage.y.min" scheme="DD" content="50.754556"/>
            <meta name="dc.coverage.y.max" scheme="DD" content="53.287320"/>
        "#;

        assert_eq!(
            antimeridian_supplement_from_html(html),
            Some(AntimeridianSupplement {
                west_lon: 177.159202,
                south_lat: 50.754556,
                east_lon: 180.0,
                north_lat: 53.287320,
            })
        );
    }

    #[test]
    fn antimeridian_chart_vrt_inputs_include_supplemental_positive_side() {
        let temp = TempDir::new("charts-antimeridian-vrts");
        fs::write(
            temp.path().join("Western Aleutian Islands East SEC.htm"),
            r#"
                <meta name="dc.coverage.x.min" scheme="DD" content="177.159202"/>
                <meta name="dc.coverage.x.max" scheme="DD" content="-172.350442"/>
                <meta name="dc.coverage.y.min" scheme="DD" content="50.754556"/>
                <meta name="dc.coverage.y.max" scheme="DD" content="53.287320"/>
            "#,
        )
        .expect("write metadata");

        let vrts = vfr_vrt_paths(
            temp.path(),
            &[String::from("Western Aleutian Islands East SEC")],
        )
        .expect("vrt paths");

        assert_eq!(
            vrts.iter()
                .map(|path| path.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "Western Aleutian Islands East SEC.vrt",
                "Western Aleutian Islands East SEC antimeridian-east.vrt",
            ]
        );
    }
}
