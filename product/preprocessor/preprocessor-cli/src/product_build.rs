// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    fs::{File, OpenOptions},
    io::{BufRead, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock, RwLock},
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use crossbeam_channel::{self, RecvTimeoutError};
use geo::{BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use had_key::{component as had_key_component, upper_component as had_upper_key_component};
use preprocessor_charts::{
    build_family_insets, build_family_legends, build_family_reference_catalog, build_family_tiles,
    build_family_vrts, package_family_bundle_detail_region_versioned_to,
    package_family_bundle_region_versioned_to, package_family_bundle_wide_angle_versioned_to,
    stage_work_dir, CHART_REFERENCE_CATALOG_NAME, FULL_COVERAGE_ZOOM, WIDE_ANGLE_REGION_ID,
};
use preprocessor_core::nav_kv::{
    build_nav_kv_sorted_with_extra_prefetch_keys, NavKvPair, NavKvRoot,
    NAVKV_STORAGE_FORMAT as NAV_KV_STORAGE_FORMAT,
};
use preprocessor_core::{
    xz_compress_bytes_with_system_xz, xz_compress_file_with_system_xz, ChartFamily, Region,
    RegionBounds,
};
use preprocessor_csup::{
    package_csup_region_versioned_to, prepare_csup_inputs, render_csup_region,
    stage_work_dir_for_product,
};
use preprocessor_data::{
    build_data_package, build_data_package_with_tpp_matches, DataBuildRequest, DataTppMatchRequest,
};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    prefetch_requests_with_provenance, read_download_records, read_download_records_lossy,
    read_source_prefetch_requests_jsonl, read_source_urls_jsonl, write_package_outputs_jsonl,
    CacheLayout, FetchCacheConfig, FetchCacheMode, PackageOutputRecord, PrefetchRequest,
};
use preprocessor_procedure_geometry::{
    audit_procedure_geometry_records, build_procedure_geometry_records, procedure_kinds_from_lists,
    ProcedureGeometryCourseJoinBypassCandidate, ProcedureGeometryFinalRouteRejection,
    ProcedureGeometryMaterializationRejection,
};
use preprocessor_resource_index::{
    validate_resource_package_catalog, write_resource_index, AssetSource,
    BuildResourceIndexRequest, ChartSource, DefaultView, ResourceIndex, TileBoundsRecord,
    TileLevelRecord,
};
use preprocessor_tpp::{
    assemble_package_region_from_sources, plan_package_region_from_members, plan_tpp_region_render,
    render_tpp_unit, tpp_prefetch_requests, write_tpp_thumbnail_from_source, TppPackagePlan,
    TppRegionRenderPlan, TppRenderUnitPlan, TppThumbnailPlan,
};
use preprocessor_vectors::{
    build_vectors_dataset, expanded_union_polygon_from_closed_ring, simplify_closed_ring,
    BuildVectorsRequest,
};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use procedure_geometry_types as pgt;
use product_contracts::{
    publication::{
        bundle::v2::{
            BundleArtifact, BundleManifest, BundlePackageArtifact,
            SCHEMA_VERSION as BUNDLE_SCHEMA_VERSION,
        },
        current::v1::{
            CurrentArtifactRoots, CurrentArtifactsManifest, CurrentBundleEntry,
            CurrentDiagnosticsEntry, CurrentStartupPrefetchCycleResources,
            CurrentStartupPrefetchManifest, CurrentStartupPrefetchResource,
            SCHEMA_VERSION as CURRENT_SCHEMA_VERSION,
        },
    },
    ChartPackageTier, WaypointSearchMatchKind, WaypointSearchRecord,
    CHART_PACKAGE_TIER_METADATA_KEY, NAV_DB_CONTRACT_ID, SHADED_RELIEF_CONTRACT_ID,
    TERRAIN_CONTRACT_ID, TERRAIN_TER2_HEIGHT_QUANTIZATION_FT, TERRAIN_TER2_MAX_ZOOM,
    WAYPOINT_SEARCH_MAX_RESULTS, WORLD_BASEMAP_CONTRACT_ID,
};
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::emit_source_urls::{cycle_effective_date, discover_published_cycles, emit_source_urls};

mod paths;
use paths::*;
mod source_fingerprints;

const PACKAGE_CYCLE_VERSION: &str = "01";
const CYCLE_PUBLICATION_LEAD_DAYS: i64 = 20;
const NAV_DB_STARTUP_PREFETCH_MEMBERS_METADATA_KEY: &str = "startup_prefetch_members";
const NAV_DB_UNPACKED_PAGE_ENCODING_MARKER: &str = "nav-db-page-xz-v1";
// Offline chart region polygons are only visual guides in the package picker.
// Grow chart cutlines coarsely before unioning to collapse tiny source-boundary
// mismatches, then simplify hard. This does not affect runtime chart coverage.
const OFFLINE_CHART_REGION_SIMPLIFY_TOLERANCE_DEGREES: f64 = 0.01;
const OFFLINE_CHART_REGION_UNION_SNAP_GRID_DEGREES: f64 = 0.0001;
const OFFLINE_CHART_REGION_UNION_EXPAND_DEGREES: f64 = 0.01;
const WMM_COEFFICIENTS_URL: &str =
    "https://www.ncei.noaa.gov/sites/default/files/2024-12/WMM2025COF.zip";
const EGM2008_INTERPOLATION_GRID_URL: &str =
    "https://earth-info.nga.mil/php/download.php?file=egm-08interpolation";
const EGM2008_GRID_MEMBER: &str = "Und_min2.5x2.5_egm2008_isw=82_WGS84_TideFree_SE";

fn log_error_chain(error: &anyhow::Error) -> String {
    log_field(&format!("{error:#}"))
}

fn log_field(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other if other.is_control() => "?".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct ProductBuildConfig {
    pub chart_metadata_root: PathBuf,
    pub build_root: PathBuf,
    pub publish_dir: PathBuf,
    pub packaged_dir: PathBuf,
    pub publish_label: String,
    pub publish_timestamp: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fetch_cache_refs: Vec<FetchCacheRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeOutputDetail {
    path: String,
    sha256: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct FetchCacheRef {
    cache_key: String,
    url: String,
    file: String,
    sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildManifest {
    schema_version: u32,
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
    pub by_node_name: BTreeMap<String, BuildCacheGcBucket>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildCacheGcBucket {
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct PublicationGcConfig {
    pub build_root: PathBuf,
    pub mode: BuildCacheGcMode,
    pub grace_hours: u64,
}

#[derive(Debug, Clone)]
pub struct PublicationGcReport {
    pub current_artifacts_path: PathBuf,
    pub current_publish_roots: usize,
    pub scanned_publish_roots: usize,
    pub grace_roots: usize,
    pub evictable_roots: usize,
    pub reclaimed_bytes: u64,
    pub candidates: Vec<PublicationGcCandidate>,
}

#[derive(Debug, Clone)]
pub struct PublicationGcCandidate {
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct FetchCacheGcConfig {
    pub build_root: PathBuf,
    pub mode: BuildCacheGcMode,
    pub grace_hours: u64,
}

#[derive(Debug, Clone)]
pub struct FetchCacheGcReport {
    pub current_artifacts_path: PathBuf,
    pub build_manifests: usize,
    pub rooted_fetch_refs: usize,
    pub rooted_blobs: usize,
    pub scanned_metadata: usize,
    pub scanned_blobs: usize,
    pub grace_metadata: usize,
    pub grace_blobs: usize,
    pub evictable_metadata: usize,
    pub evictable_blobs: usize,
    pub reclaimed_bytes: u64,
    pub candidates: Vec<FetchCacheGcCandidate>,
    pub missing_fetch_refs: bool,
}

#[derive(Debug, Clone)]
pub struct FetchCacheGcCandidate {
    pub kind: FetchCacheGcCandidateKind,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchCacheGcCandidateKind {
    Metadata,
    Blob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildDiagnosticsManifest {
    schema_version: u32,
    generated_at_utc: String,
    error_count: usize,
    errors: Vec<BuildDiagnosticEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildDiagnosticEntry {
    product: String,
    cycle: Option<String>,
    severity: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct BuildStatusDocument {
    schema_version: u32,
    generated_at_utc: String,
    build_root: String,
    current_artifacts: String,
    disk: BuildStatusDisk,
    warnings: Vec<BuildStatusWarning>,
    products: Vec<BuildStatusProduct>,
}

#[derive(Debug, Clone, Serialize)]
struct BuildStatusWarning {
    severity: String,
    code: String,
    path: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct BuildStatusDisk {
    path: String,
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
    available_bytes: u64,
    percent_free: f64,
}

#[derive(Debug, Clone, Serialize)]
struct BuildStatusProduct {
    bundle_type: String,
    bundle_id: String,
    cycle: Option<String>,
    id: String,
    family_id: String,
    region_id: Option<String>,
    filename: String,
    size_bytes: u64,
    declared_time: Option<String>,
    fetch_time: Option<String>,
    effective_date: Option<String>,
    expiration_date: Option<String>,
    source_generated_at_utc: Option<String>,
    source_fetched_at_utc: Option<String>,
    published_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProductFactsDocument {
    schema_version: u32,
    generated_at_utc: String,
    build: ProductFactsBuild,
    products: Vec<ProductFactsProduct>,
}

#[derive(Debug, Clone, Serialize)]
struct ProductFactsBuild {
    status: String,
    completed_at_utc: String,
    current_artifacts: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProductFactsProduct {
    product_id: String,
    family: String,
    contract: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    region_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cycle_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_generated_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_fetched_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at_utc: Option<String>,
    error_count: usize,
    warning_count: usize,
    diagnostics: ProductFactsDiagnostics,
}

#[derive(Debug, Clone, Default, Serialize)]
struct ProductFactsDiagnostics {
    procedure_geometry_warning_count: usize,
    procedure_geometry_error_count: usize,
    vector_validator_warning_count: usize,
    vector_validator_error_count: usize,
    other_warning_count: usize,
    other_error_count: usize,
}

#[derive(Debug, Clone)]
struct BuiltNavDbArtifacts {
    node_record: NodeRecord,
    package: BundlePackageArtifact,
}

#[derive(Debug, Deserialize)]
struct VectorHadPairLine {
    key: String,
    value_json: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ChartCutlinePolygonRecord {
    id: String,
    points: Vec<[f64; 2]>,
}

impl Serialize for ChartCutlinePolygonRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("ChartCutlinePolygonRecord", 2)?;
        state.serialize_field("id", &self.id)?;
        let points = self
            .points
            .iter()
            .map(|point| {
                [
                    nav_db::round_nav_coordinate(point[0]),
                    nav_db::round_nav_coordinate(point[1]),
                ]
            })
            .collect::<Vec<_>>();
        state.serialize_field("points", &points)?;
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChartCutlinePolygonSetRecord {
    schema_version: u32,
    id: String,
    polygons: Vec<ChartCutlinePolygonRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OfflineRegionCatalogRecord {
    schema_version: u32,
    regions: Vec<OfflineRegionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OfflineRegionRecord {
    id: String,
    kind: String,
    region_id: String,
    label: String,
    color_key: String,
    summary: Vec<OfflineRegionSummaryEntry>,
    polygons: Vec<Vec<OfflineRegionLatLon>>,
    label_position: OfflineRegionLatLon,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OfflineRegionSummaryEntry {
    action: String,
    cycle: String,
    count: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
struct OfflineRegionLatLon {
    lat: f64,
    lon: f64,
}

impl Serialize for OfflineRegionLatLon {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("OfflineRegionLatLon", 2)?;
        state.serialize_field("lat", &nav_db::round_nav_coordinate(self.lat))?;
        state.serialize_field("lon", &nav_db::round_nav_coordinate(self.lon))?;
        state.end()
    }
}

#[derive(Debug, Clone)]
struct RawChartCutlinePolygon {
    points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductBuildResult {
    pub cycle_manifest_paths: Vec<PathBuf>,
    pub product_artifacts_path: PathBuf,
}

fn static_product_task_ids(_config: &ProductBuildConfig) -> Vec<String> {
    let mut task_ids = vec!["publish-world-basemap".to_string()];
    if include_static_terrain_products() {
        task_ids.extend(
            Region::ALL
                .iter()
                .map(|region| format!("publish-terrain-{}", region.code().to_ascii_lowercase())),
        );
        task_ids.push(format!("publish-terrain-{WIDE_ANGLE_REGION_ID}"));
        task_ids.extend(Region::ALL.iter().map(|region| {
            format!(
                "publish-shaded-relief-{}",
                region.code().to_ascii_lowercase()
            )
        }));
        task_ids.push(format!("publish-shaded-relief-{WIDE_ANGLE_REGION_ID}"));
    }
    task_ids
}

fn stable_product_family_region(id: &str) -> anyhow::Result<(String, Option<String>)> {
    let id = stable_product_base_id(id);
    if id == "world-basemap" {
        return Ok(("world-basemap".to_string(), None));
    }
    if let Some(region_id) = id.strip_prefix("terrain-") {
        return Ok(("terrain".to_string(), Some(region_id.to_string())));
    }
    if let Some(region_id) = id.strip_prefix("shaded-relief-") {
        return Ok(("shaded-relief".to_string(), Some(region_id.to_string())));
    }
    bail!("unrecognized stable product id: {id}")
}

fn product_contract_id_for_family(family_id: &str) -> anyhow::Result<&'static str> {
    product_contracts::contract_id_for_family(family_id)
        .with_context(|| format!("unrecognized product family for contract id: {family_id}"))
}

fn stable_product_contract_id(id: &str) -> anyhow::Result<&'static str> {
    let (family_id, _) = stable_product_family_region(id)?;
    product_contract_id_for_family(&family_id)
}

fn contract_artifact_version(contract_id: &str, version_label: &str) -> String {
    format!("{contract_id}_{version_label}")
}

fn stable_product_id_with_contract(id: &str) -> anyhow::Result<String> {
    Ok(format!(
        "{}_{}",
        stable_product_base_id(id),
        stable_product_contract_id(id)?
    ))
}

fn stable_product_base_id(id: &str) -> &str {
    for contract_id in [
        TERRAIN_CONTRACT_ID,
        SHADED_RELIEF_CONTRACT_ID,
        WORLD_BASEMAP_CONTRACT_ID,
    ] {
        if let Some(base) = id.strip_suffix(&format!("_{contract_id}")) {
            return base;
        }
    }
    id
}

fn package_metadata_with_contract_id(
    mut metadata: BTreeMap<String, serde_json::Value>,
    contract_id: &str,
) -> BTreeMap<String, serde_json::Value> {
    metadata.insert(
        "contract_id".to_string(),
        serde_json::Value::String(contract_id.to_string()),
    );
    metadata
}

fn stable_effective_date_from_published_file(path: &Path) -> anyhow::Result<(String, String)> {
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let published_at: DateTime<Utc> = modified.into();
    Ok((
        published_at.format("%Y-%m-%d").to_string(),
        published_at.to_rfc3339_opts(SecondsFormat::Secs, true),
    ))
}

fn build_stable_bundle_package_artifact(
    id: &str,
    published_zip: &Path,
    sha256: &str,
    size_bytes: u64,
    source_version: &str,
    source_fetched_at_utc: Option<String>,
) -> anyhow::Result<BundlePackageArtifact> {
    let (family_id, region_id) = stable_product_family_region(id)?;
    let contract_id = product_contract_id_for_family(&family_id)?;
    let filename = filename_string(published_zip)?;
    let (effective_date, published_at_utc) =
        stable_effective_date_from_published_file(published_zip)?;
    Ok(BundlePackageArtifact {
        id: id.to_string(),
        family_id,
        contract_id: contract_id.to_string(),
        region_id,
        filename: filename.clone(),
        relative_path: filename,
        cycle: None,
        cycle_version: None,
        checksum_sha256: sha256.to_string(),
        size_bytes,
        published_at_utc: Some(published_at_utc),
        source_generated_at_utc: None,
        source_version: Some(source_version.to_string()),
        source_fetched_at_utc,
        effective_date: Some(effective_date),
        expiration_date: None,
        warning_text: None,
        metadata: package_metadata_with_contract_id(
            stable_product_package_metadata(id),
            contract_id,
        ),
    })
}

fn stable_product_package_metadata(id: &str) -> BTreeMap<String, serde_json::Value> {
    let id = stable_product_base_id(id);
    if id == "world-basemap" {
        return BTreeMap::from([
            ("tile_format".to_string(), serde_json::json!("png")),
            (
                "tile_path_template".to_string(),
                serde_json::json!("tiles/0/{z}/{x}/{y}.png"),
            ),
            ("tile_size".to_string(), serde_json::json!(WORLD_BASEMAP_TILE_SIZE)),
            ("min_zoom".to_string(), serde_json::json!(WORLD_BASEMAP_MIN_ZOOM)),
            (
                "max_source_zoom".to_string(),
                serde_json::json!(WORLD_BASEMAP_MAX_SOURCE_ZOOM),
            ),
            (
                "max_display_zoom".to_string(),
                serde_json::json!(WORLD_BASEMAP_MAX_DISPLAY_ZOOM),
            ),
            (
                "source".to_string(),
                serde_json::json!("Natural Earth 110m land and admin-0 boundary lines"),
            ),
            ("license".to_string(), serde_json::json!("public-domain")),
            (
                "attribution".to_string(),
                serde_json::json!(
                    "Made with Natural Earth. Free vector and raster map data @ naturalearthdata.com."
                ),
            ),
        ]);
    }
    if let Some(region_id) = id.strip_prefix("terrain-") {
        let is_wide_angle = region_id == WIDE_ANGLE_REGION_ID;
        return BTreeMap::from([
            (
                "wide_angle_region_id".to_string(),
                serde_json::json!(WIDE_ANGLE_REGION_ID),
            ),
            (
                "wide_angle_max_zoom".to_string(),
                serde_json::json!(FULL_COVERAGE_ZOOM),
            ),
            ("wide_angle".to_string(), serde_json::json!(is_wide_angle)),
            (
                "max_source_zoom".to_string(),
                serde_json::json!(if is_wide_angle {
                    FULL_COVERAGE_ZOOM
                } else {
                    TERRAIN_ZOOM
                }),
            ),
        ]);
    }
    if let Some(region_id) = id.strip_prefix("shaded-relief-") {
        let is_wide_angle = region_id == WIDE_ANGLE_REGION_ID;
        let mut metadata = BTreeMap::from([
            ("tile_format".to_string(), serde_json::json!("webp")),
            (
                "tile_path_template".to_string(),
                serde_json::json!("tiles/0/{z}/{x}/{y}.webp"),
            ),
            (
                "tile_size".to_string(),
                serde_json::json!(TERRAIN_TILE_SIZE),
            ),
            (
                "wide_angle_region_id".to_string(),
                serde_json::json!(WIDE_ANGLE_REGION_ID),
            ),
            (
                "wide_angle_max_zoom".to_string(),
                serde_json::json!(FULL_COVERAGE_ZOOM),
            ),
            ("wide_angle".to_string(), serde_json::json!(is_wide_angle)),
        ]);
        if is_wide_angle {
            metadata.insert("min_zoom".to_string(), serde_json::json!(TERRAIN_MIN_ZOOM));
            metadata.insert(
                "max_source_zoom".to_string(),
                serde_json::json!(FULL_COVERAGE_ZOOM),
            );
        } else {
            metadata.insert(
                "min_source_zoom".to_string(),
                serde_json::json!(FULL_COVERAGE_ZOOM + 1),
            );
            metadata.insert(
                "max_source_zoom".to_string(),
                serde_json::json!(TERRAIN_ZOOM),
            );
        }
        return metadata;
    }
    BTreeMap::new()
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

#[derive(Debug)]
struct PublicationLockGuard {
    path: PathBuf,
}

impl Drop for BuildLockGuard {
    fn drop(&mut self) {
        let _ = set_tree_readonly(&self.node_dir, false);
        let _ = fs::remove_file(&self.path);
        if self.node_dir.join("build-record.json").is_file() {
            let _ = set_tree_readonly(&self.node_dir, true);
        }
    }
}

impl Drop for PublicationLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn node_output_file_detail(record: &NodeRecord, key: &str) -> (Option<String>, Option<u64>) {
    record
        .output_details
        .get(key)
        .map(|detail| (detail.sha256.clone(), detail.size_bytes))
        .unwrap_or((None, None))
}

fn output_sha_or_hash(record: &NodeRecord, key: &str, path: &Path) -> anyhow::Result<String> {
    if let Some(sha256) = node_output_file_detail(record, key).0 {
        return Ok(sha256);
    }
    hash_file(path)
}

fn sqlite_output_path(record: &NodeRecord) -> anyhow::Result<&str> {
    record
        .outputs
        .get("intermediate_sqlite_db")
        .or_else(|| record.outputs.get("main_db"))
        .map(String::as_str)
        .with_context(|| format!("node {} missing sqlite output", record.name))
}

enum NodeCacheState {
    CacheHit(NodeRecord),
    Build(BuildLockGuard),
}

#[derive(Debug, Clone)]
enum ScheduledTaskKind {
    ChartFetch {
        family: ChartFamily,
    },
    ChartProcess {
        family: ChartFamily,
    },
    CsupFetch,
    CsupProcess,
    CsupRender {
        region: Region,
    },
    TppFetch,
    TppPlan {
        region: Region,
    },
    TppRenderUnit {
        region: Region,
        unit: Box<TppRenderUnitPlan>,
    },
    TppRenderAssemble {
        region: Region,
    },
    TppPackagePlan {
        region: Region,
    },
    TppThumbnail {
        region: Region,
        thumbnail: TppThumbnailPlan,
    },
    DataBase,
    DataMatch,
    ChartPackage {
        family: ChartFamily,
    },
    CsupPackage,
    TppPackage {
        region: Region,
    },
    Vectors,
    ResourceIndex,
    CsupUnpack {
        region: Region,
    },
    TppUnpack {
        region: Region,
    },
    DataUnpack,
}

#[derive(Debug, Clone)]
enum TaskValue {
    None,
    ChartFetch {
        record: NodeRecord,
    },
    CsupFetch {
        record: NodeRecord,
    },
    CsupProcess {
        record: NodeRecord,
        work_dir: PathBuf,
    },
    TppFetch {
        record: NodeRecord,
    },
    TppPlan {
        record: NodeRecord,
        source_root: PathBuf,
        plan: TppRegionRenderPlan,
        source_content_fingerprint: String,
    },
    TppRender {
        record: NodeRecord,
    },
    TppPackagePlan {
        record: NodeRecord,
        metadata_root: PathBuf,
        plate_sources: TppPlateSourceMap,
        plan: TppPackagePlan,
    },
    ChartSource(ChartSource),
    CsupSource(AssetSource),
    FingerprintedData {
        intermediate_sqlite_db: PathBuf,
        source_input_dir: PathBuf,
        zip: PathBuf,
        fingerprint: String,
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
    ChartFetch {
        record: NodeRecord,
    },
    CsupFetch {
        record: NodeRecord,
    },
    CsupProcess {
        record: NodeRecord,
        work_dir: PathBuf,
    },
    TppFetch {
        record: NodeRecord,
    },
    TppPlan {
        record: NodeRecord,
        source_root: PathBuf,
        plan: TppRegionRenderPlan,
        source_content_fingerprint: String,
    },
    TppRender {
        record: NodeRecord,
    },
    TppPackagePlan {
        record: NodeRecord,
        metadata_root: PathBuf,
        plate_sources: TppPlateSourceMap,
        plan: TppPackagePlan,
    },
    ChartSource(ChartSource),
    CsupSource(AssetSource),
    FingerprintedData {
        intermediate_sqlite_db: PathBuf,
        source_input_dir: PathBuf,
        zip: PathBuf,
        fingerprint: String,
    },
    VectorHad {
        pairs: PathBuf,
        errors: PathBuf,
    },
    WmmSource {
        cof_path: PathBuf,
        metadata_path: PathBuf,
    },
    GeoidSource {
        csv_path: PathBuf,
        metadata_path: PathBuf,
        source_fetched_at_utc: Option<String>,
    },
    FingerprintedTppSource {
        source: AssetSource,
        fingerprint: String,
    },
    CycleManifest {
        path: PathBuf,
    },
    PublishedNavDb {
        package: BundlePackageArtifact,
        unpack_source_root: PathBuf,
    },
    BuiltStandaloneProduct {
        zip_path: PathBuf,
        unpack_source_root: PathBuf,
        zip_sha256: Option<String>,
        zip_size_bytes: Option<u64>,
        source_version: String,
        source_fetched_at_utc: Option<String>,
    },
    BuiltStaticTileProduct {
        zip_path: PathBuf,
        unpack_source_root: PathBuf,
        zip_sha256: Option<String>,
        zip_size_bytes: Option<u64>,
        source_version: String,
        source_fetched_at_utc: Option<String>,
        tile_levels: Vec<TileLevelRecord>,
    },
    BuiltWaterMask {
        mask_tiles_dir: PathBuf,
        source_version: String,
    },
    TerrainDiscovery {
        index_path: PathBuf,
        source_fetched_at_utc: Option<String>,
    },
    PublishedStandaloneProduct {
        id: String,
        unpack_source_root: PathBuf,
        published_zip: PathBuf,
        sha256: String,
        size_bytes: u64,
        source_version: String,
        source_fetched_at_utc: Option<String>,
    },
}

type ProductTaskCompletion = GraphTaskCompletion<ProductTaskValue>;
type TppPlateSourceMap = BTreeMap<String, PathBuf>;

mod graph;
use graph::*;

#[derive(Debug, Clone)]
struct PublishedZipArtifact {
    unpack_strategy: PublishedZipUnpackStrategy,
    published_zip_path: PathBuf,
    checksum_sha256: String,
}

#[derive(Debug, Clone)]
enum PublishedZipUnpackStrategy {
    HardlinkZipMembers { source_root: PathBuf },
}

type TaskCompletion = GraphTaskCompletion<TaskValue>;

const PRODUCT_BUILD_CGROUP_ACTIVE_ENV: &str = "PRODUCT_BUILD_CGROUP_ACTIVE";
const DEFAULT_PRODUCT_BUILD_MEMORY_MAX: &str = "80G";
const SCHEDULER_WEIGHT_SCALE: usize = 8;
const LIGHT_TASK_WEIGHT: usize = SCHEDULER_WEIGHT_SCALE;
const CHART_PROCESS_WEIGHT: usize = 4 * SCHEDULER_WEIGHT_SCALE;
const DATA_BASE_WEIGHT: usize = 4 * SCHEDULER_WEIGHT_SCALE;
const CSUP_RENDER_WEIGHT: usize = 2 * SCHEDULER_WEIGHT_SCALE;
const RESOURCE_INDEX_WEIGHT: usize = 2 * SCHEDULER_WEIGHT_SCALE;
const TERRAIN_BUILD_WEIGHT: usize = 6 * SCHEDULER_WEIGHT_SCALE;
const WATER_MASK_BUILD_WEIGHT: usize = 4 * SCHEDULER_WEIGHT_SCALE;
const TPP_RENDER_UNIT_WEIGHT: usize = 2;
const TPP_THUMBNAIL_WEIGHT: usize = 2;
const TPP_CACHE_LAYOUT_VERSION: &str = "v2-cache-nodes";
const TPP_FETCH_NODE_VERSION: &str = "v2-source-content-fingerprint";
// TPP plan/render/package work is expensive enough that broad source-file hashes
// are too costly. These versions cover the Rust TPP implementation and shared
// output-affecting helpers it calls; bump them when those output semantics
// change. Do not add broad helper-crate hashes to TPP node inputs for logging or
// subprocess-runner changes.
const TPP_PLAN_NODE_VERSION: &str = "v4-cardinal-text-orientation-faa-deletion-encodings";
const TPP_RENDER_NODE_VERSION: &str = "v3-per-output-render-nodes";
const TPP_RENDER_ASSEMBLE_NODE_VERSION: &str = "v2-source-map-assemble";
const TPP_PACKAGE_NODE_VERSION: &str = "v4-per-thumbnail-nodes-faa-deletion-encodings";
const TPP_THUMBNAIL_NODE_VERSION: &str = "v1-source-png-thumbnail";
const STATIC_SOURCE_FETCH_NODE_VERSION: &str = "v2-source-content-fingerprint";
const TERRAIN_PIPELINE_VERSION: &str = "v6-ter2-z9-max-none-ceil64-gradient";
const SHADED_RELIEF_PIPELINE_VERSION: &str = "v8-wide-angle-split-tile-boxes";
const SHADED_RELIEF_OVERLAY_STYLE_VERSION: &str = "v1-gray-borders-bluegray-primary-roads";
const SHADED_RELIEF_STATE_BORDERS_URL: &str =
    "https://naturalearth.s3.amazonaws.com/50m_cultural/ne_50m_admin_1_states_provinces_lines.zip";
const SHADED_RELIEF_PRIMARY_ROADS_URL: &str =
    "https://www2.census.gov/geo/tiger/TIGER2025/PRIMARYROADS/tl_2025_us_primaryroads.zip";
const WATER_MASK_PIPELINE_VERSION: &str = "v3";
const TERRAIN_TILE_WORKERS: u32 = 16;
const SHADED_RELIEF_TILE_WORKERS: u32 = 16;
const WATER_MASK_FETCH_WORKERS: u32 = 2;
const WATER_MASK_TILE_WORKERS: u32 = 4;
const WATER_MASK_PAGE_SIZE: usize = 10;
const WATER_MASK_MAX_SPLIT_SOURCE_PAGES: usize = 64;
const WATER_MASK_MAX_OMITTED_OBJECTS: usize = 16;
const WATER_MASK_NHD_SERVICE: &str =
    "https://hydro.nationalmap.gov/arcgis/rest/services/nhd/MapServer";
const WATER_MASK_NHD_LAYERS: &[(u32, &str, &str)] = &[
    (
        9,
        "Area - Large Scale",
        "AREASQKM >= 1 AND FTYPE IN (312,445,460)",
    ),
    (
        12,
        "Waterbody - Large Scale",
        "AREASQKM >= 1 AND FTYPE IN (378,390,436,493)",
    ),
];
const WORLD_BASEMAP_PIPELINE_VERSION: &str = "v2-tile-boxes";
const WORLD_BASEMAP_MIN_ZOOM: u32 = 0;
const WORLD_BASEMAP_MAX_SOURCE_ZOOM: u32 = 4;
const WORLD_BASEMAP_MAX_DISPLAY_ZOOM: f64 = 8.0;
const WORLD_BASEMAP_TILE_SIZE: u32 = 512;
const WORLD_BASEMAP_LAND_URL: &str =
    "https://naturalearth.s3.amazonaws.com/110m_physical/ne_110m_land.zip";
const WORLD_BASEMAP_BOUNDARIES_URL: &str =
    "https://naturalearth.s3.amazonaws.com/110m_cultural/ne_110m_admin_0_boundary_lines_land.zip";
const TERRAIN_MIN_ZOOM: u32 = 0;
const TERRAIN_ZOOM: u32 = TERRAIN_TER2_MAX_ZOOM;
const TERRAIN_TILE_SIZE: u32 = 512;
const RASTER_BASEMAP_MAX_DISPLAY_ZOOM: f64 = 14.0;

pub fn explain_product_build(config: &ProductBuildConfig) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("build_root {}", config.build_root.display()));
    lines.push(format!("publish_dir {}", config.publish_dir.display()));
    lines.push(format!("packaged_dir {}", config.packaged_dir.display()));
    lines.push(format!(
        "chart_metadata_root {}",
        config.chart_metadata_root.display()
    ));
    lines.push(format!(
        "fetch_cache_root {}",
        config.fetch_cache_root.display()
    ));
    lines.push(format!("fetch_cache_mode {}", config.fetch_cache_mode));
    lines.push(format!("max_heavy_jobs {}", config.max_heavy_jobs));
    lines.push("nodes".to_string());
    lines.push("  source-urls".to_string());
    for family in ["sec", "tac", "flyway", "enr-l", "enr-h"] {
        lines.push(format!("  charts-{family}"));
    }
    lines.push("  csup".to_string());
    for region in Region::ALL.iter() {
        lines.push(format!("  tpp-{}", region.code().to_ascii_lowercase()));
    }
    lines.push("  data-input-staging".to_string());
    lines.push("  data-base".to_string());
    lines.push("  data".to_string());
    lines.push("  vectors".to_string());
    lines.push("  resource-index".to_string());
    lines.push("  nav-db".to_string());
    lines.push("  bundle-manifest".to_string());
    Ok(lines.join("\n") + "\n")
}

mod product;
pub use product::build_product;

mod cycle;
pub use cycle::build_cycle;

mod nav_db;
use nav_db::*;
pub use nav_db::{audit_procedure_geometry_from_sqlite, ProcedureGeometryAuditFilter};

fn resolve_resource_package_artifact_path(
    config: &ProductBuildConfig,
    package: &preprocessor_resource_index::ResourcePackage,
) -> anyhow::Result<PathBuf> {
    let relative_path = Path::new(&package.artifact_path);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!(
            "resource package {} has invalid artifact_path {}",
            package.id,
            package.artifact_path
        );
    }
    let path = config.build_root.join(relative_path);
    verify_artifact_file(
        &path,
        &package.checksum_sha256,
        package.size_bytes,
        &format!("resource package {}", package.id),
    )?;
    Ok(path)
}

fn output_path<'a>(record: &'a NodeRecord, key: &str) -> anyhow::Result<&'a str> {
    record
        .outputs
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("node {} missing outputs.{key}", record.name))
}

fn resolve_artifact_path(config: &ProductBuildConfig, relative_path: &str) -> PathBuf {
    config.build_root.join(relative_path)
}

fn published_unpacked_root(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    published_unpacked_root_from_packaged_dir(&config.packaged_dir)
}

fn internal_build_manifest_path(
    config: &ProductBuildConfig,
    bundle_cycle: &str,
) -> anyhow::Result<PathBuf> {
    let publish_key = publish_path_key(&config.publish_dir, &config.build_root);
    let dir = build_manifests_root(&config.build_root).join(publish_key);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir.join(format!("build-manifest_{bundle_cycle}.json")))
}

pub fn published_unpacked_root_from_packaged_dir(packaged_dir: &Path) -> anyhow::Result<PathBuf> {
    let publish_dir = packaged_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "packaged publication dir has no publish_dir parent: {}",
            packaged_dir.display()
        )
    })?;
    Ok(publish_dir.join("unpacked"))
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
    let publish_dir = unpacked_root.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "unpacked publication dir has no publish_dir parent: {}",
            unpacked_root.display()
        )
    })?;
    let build_root = artifact_root_from_publish_dir(publish_dir)?;
    let publish_key = publish_path_key(publish_dir, &build_root);
    let marker_dir = published_unpacked_state_root(&build_root).join(publish_key);
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
    sync_unpacked_zip_from_source_with_marker(
        zip_path,
        source_root,
        unpacked_root,
        published_filename,
        known_sha256,
        None,
        hardlink_zip_members_from_source_root,
    )
}

fn sync_nav_db_unpacked_zip_from_source(
    zip_path: &Path,
    source_root: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: Option<&str>,
    xz_jobs: usize,
) -> anyhow::Result<(bool, PathBuf)> {
    sync_unpacked_zip_from_source_with_marker(
        zip_path,
        source_root,
        unpacked_root,
        published_filename,
        known_sha256,
        Some(NAV_DB_UNPACKED_PAGE_ENCODING_MARKER),
        |zip_path, source_root, output_dir| {
            hardlink_nav_db_zip_members_from_source_root(zip_path, source_root, output_dir, xz_jobs)
        },
    )
}

fn sync_unpacked_zip_from_source_with_marker(
    zip_path: &Path,
    source_root: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: Option<&str>,
    marker_suffix: Option<&str>,
    sync_members: impl FnOnce(&Path, &Path, &Path) -> anyhow::Result<()>,
) -> anyhow::Result<(bool, PathBuf)> {
    let unpack_dir = unpacked_target_dir(unpacked_root, published_filename)?;
    let marker_path = unpacked_marker_path(unpacked_root, published_filename)?;
    let zip_sha256 = match known_sha256 {
        Some(value) => value.to_string(),
        None => hash_file(zip_path)?,
    };
    let marker_value = unpacked_marker_value(&zip_sha256, marker_suffix);
    if unpack_dir.is_dir()
        && unpacked_dir_has_files(&unpack_dir)?
        && fs::read_to_string(&marker_path)
            .ok()
            .as_deref()
            .map(str::trim)
            == Some(marker_value.as_str())
    {
        return Ok((true, unpack_dir));
    }
    if unpack_dir.exists() {
        fs::remove_dir_all(&unpack_dir)
            .with_context(|| format!("failed to remove {}", unpack_dir.display()))?;
    }
    fs::create_dir_all(&unpack_dir)
        .with_context(|| format!("failed to create {}", unpack_dir.display()))?;
    sync_members(zip_path, source_root, &unpack_dir)?;
    fs::write(&marker_path, format!("{marker_value}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))?;
    Ok((false, unpack_dir))
}

fn unpacked_marker_value(zip_sha256: &str, marker_suffix: Option<&str>) -> String {
    match marker_suffix {
        Some(suffix) => format!("{zip_sha256} {suffix}"),
        None => zip_sha256.to_string(),
    }
}

fn unpacked_dir_has_files(path: &Path) -> anyhow::Result<bool> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", entry.path().display()))?;
        if file_type.is_file() {
            return Ok(true);
        }
        if file_type.is_dir() && unpacked_dir_has_files(&entry.path())? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn hardlink_zip_members_from_source_root(
    zip_path: &Path,
    source_root: &Path,
    output_dir: &Path,
) -> anyhow::Result<()> {
    sync_zip_members_from_source_root(zip_path, source_root, output_dir, |source, target| {
        fs::hard_link(source, target).with_context(|| {
            format!(
                "failed to hardlink {} to {}",
                source.display(),
                target.display()
            )
        })
    })
}

fn hardlink_nav_db_zip_members_from_source_root(
    zip_path: &Path,
    source_root: &Path,
    output_dir: &Path,
    xz_jobs: usize,
) -> anyhow::Result<()> {
    let mut page_jobs = Vec::new();
    sync_zip_members_from_source_root(zip_path, source_root, output_dir, |source, target| {
        if source
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("page_"))
        {
            page_jobs.push(XzFileJob {
                source: source.to_path_buf(),
                target: target.to_path_buf(),
            });
            Ok(())
        } else {
            fs::hard_link(source, target).with_context(|| {
                format!(
                    "failed to hardlink {} to {}",
                    source.display(),
                    target.display()
                )
            })
        }
    })?;
    write_xz_files_parallel(page_jobs, xz_jobs)
}

fn sync_zip_members_from_source_root(
    zip_path: &Path,
    source_root: &Path,
    output_dir: &Path,
    mut sync_file: impl FnMut(&Path, &Path) -> anyhow::Result<()>,
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
        let source = source_root.join(&member);
        if !source.is_file() {
            bail!(
                "missing source member {} for {} under {}",
                member,
                zip_path.display(),
                source_root.display()
            );
        }
        sync_file(&source, &outpath)?;
    }
    Ok(())
}

#[derive(Debug)]
struct XzFileJob {
    source: PathBuf,
    target: PathBuf,
}

fn write_xz_files_parallel(jobs: Vec<XzFileJob>, xz_jobs: usize) -> anyhow::Result<()> {
    let worker_count = xz_jobs.max(1).min(jobs.len().max(1));
    if worker_count == 1 {
        for job in jobs {
            write_xz_file_deterministic(&job.source, &job.target)?;
        }
        return Ok(());
    }
    let (sender, receiver) = crossbeam_channel::unbounded::<XzFileJob>();
    for job in jobs {
        sender
            .send(job)
            .context("failed to enqueue nav-db xz job")?;
    }
    drop(sender);
    let worker_results = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..worker_count {
            let receiver = receiver.clone();
            handles.push(scope.spawn(move || -> anyhow::Result<()> {
                for job in receiver {
                    write_xz_file_deterministic(&job.source, &job.target)?;
                }
                Ok(())
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join())
            .collect::<Vec<_>>()
    });
    for result in worker_results {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => bail!("nav-db xz worker thread panicked"),
        }
    }
    Ok(())
}

fn write_xz_file_deterministic(source: &Path, target: &Path) -> anyhow::Result<()> {
    let encoded = xz_compress_file_with_system_xz(source)?;
    fs::write(target, encoded).with_context(|| format!("failed to write {}", target.display()))
}

pub(super) fn producer_xz_compress_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    xz_compress_bytes_with_system_xz(bytes).map_err(|err| err.to_string())
}

fn prepare_package_unpack_source_root(
    zip_paths: &[PathBuf],
    asset_root: &Path,
    package_root: &Path,
    unpack_source_root: &Path,
    generated_member_prefixes: &[&str],
) -> anyhow::Result<()> {
    prepare_package_unpack_source_root_with_member_sources(
        zip_paths,
        asset_root,
        package_root,
        unpack_source_root,
        generated_member_prefixes,
        None,
    )
}

fn prepare_package_unpack_source_root_with_member_sources(
    zip_paths: &[PathBuf],
    asset_root: &Path,
    package_root: &Path,
    unpack_source_root: &Path,
    generated_member_prefixes: &[&str],
    member_sources: Option<&BTreeMap<String, PathBuf>>,
) -> anyhow::Result<()> {
    if unpack_source_root.exists() {
        fs::remove_dir_all(unpack_source_root)
            .with_context(|| format!("failed to remove {}", unpack_source_root.display()))?;
    }
    fs::create_dir_all(unpack_source_root)
        .with_context(|| format!("failed to create {}", unpack_source_root.display()))?;
    for zip_path in zip_paths {
        hardlink_package_zip_members_to_unpack_source_root(
            zip_path,
            asset_root,
            package_root,
            unpack_source_root,
            generated_member_prefixes,
            member_sources,
        )?;
    }
    Ok(())
}

fn hardlink_package_zip_members_to_unpack_source_root(
    zip_path: &Path,
    asset_root: &Path,
    package_root: &Path,
    unpack_source_root: &Path,
    generated_member_prefixes: &[&str],
    member_sources: Option<&BTreeMap<String, PathBuf>>,
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
        let outpath = unpack_source_root.join(&member);
        if member.ends_with('/') || entry.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("failed to create {}", outpath.display()))?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let source = package_zip_member_source_path(
            asset_root,
            package_root,
            &member,
            generated_member_prefixes,
            member_sources,
        );
        if !source.is_file() {
            bail!(
                "package zip member {} from {} is not present at disciplined unpack source {}",
                member,
                zip_path.display(),
                source.display()
            );
        }
        if outpath.exists() {
            fs::remove_file(&outpath)
                .with_context(|| format!("failed to remove {}", outpath.display()))?;
        }
        fs::hard_link(&source, &outpath).with_context(|| {
            format!(
                "failed to hardlink {} to {}",
                source.display(),
                outpath.display()
            )
        })?;
    }
    Ok(())
}

fn package_zip_member_source_path(
    asset_root: &Path,
    package_root: &Path,
    member: &str,
    generated_member_prefixes: &[&str],
    member_sources: Option<&BTreeMap<String, PathBuf>>,
) -> PathBuf {
    let member_path = Path::new(member);
    if member_path.components().count() == 1
        || generated_member_prefixes
            .iter()
            .any(|prefix| member.starts_with(prefix))
    {
        package_root.join(member)
    } else if let Some(source) = member_sources.and_then(|sources| sources.get(member)) {
        source.clone()
    } else {
        asset_root.join(member)
    }
}

fn sync_unpacked_metadata(
    config: &ProductBuildConfig,
    bundle_manifest: &BundleManifest,
    bundle_manifest_path: &Path,
    task_values: Option<&BTreeMap<String, ProductTaskValue>>,
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_file(bundle_manifest_path, &unpacked_root)?;
    for artifact in &bundle_manifest.ancillary {
        if artifact.filename.ends_with(".zip") {
            continue;
        }
        sync_unpacked_file(
            &config.packaged_dir.join(&artifact.filename),
            &unpacked_root,
        )?;
    }
    sync_cycle_bundle_unpacked_zips(config, bundle_manifest, &unpacked_root, task_values)?;
    Ok(())
}

fn sync_cycle_bundle_unpacked_zips(
    config: &ProductBuildConfig,
    bundle_manifest: &BundleManifest,
    unpacked_root: &Path,
    task_values: Option<&BTreeMap<String, ProductTaskValue>>,
) -> anyhow::Result<()> {
    let build_manifest = if task_values.is_none() {
        let path = internal_build_manifest_path(config, &bundle_manifest.cycle)?;
        Some(
            serde_json::from_slice::<BuildManifest>(
                &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", path.display()))?,
        )
    } else {
        None
    };
    for package in &bundle_manifest.packages {
        if package.family_id == "nav-db" {
            let source_dir = if let Some(task_values) = task_values {
                resolve_cycle_bundle_package_unpack_source_root(
                    task_values,
                    &bundle_manifest.cycle,
                    package,
                )?
            } else {
                resolve_cycle_bundle_package_unpack_source_root_from_build_manifest(
                    config,
                    build_manifest
                        .as_ref()
                        .expect("build manifest should exist for standalone cycle unpack"),
                    package,
                )?
            }
            .with_context(|| {
                format!(
                    "failed to resolve unpack source root for package {}",
                    package.id
                )
            })?;
            sync_nav_db_unpacked_zip_from_source(
                &config.packaged_dir.join(&package.filename),
                &source_dir,
                unpacked_root,
                &package.filename,
                Some(&package.checksum_sha256),
                config.cpu_jobs,
            )
            .with_context(|| format!("failed to unpack package {}", package.id))?;
            continue;
        }
        if package.cycle.is_none() {
            continue;
        }
        let source_root = if let Some(task_values) = task_values {
            resolve_cycle_bundle_package_unpack_source_root(
                task_values,
                &bundle_manifest.cycle,
                package,
            )?
        } else {
            resolve_cycle_bundle_package_unpack_source_root_from_build_manifest(
                config,
                build_manifest
                    .as_ref()
                    .expect("build manifest should exist for standalone cycle unpack"),
                package,
            )?
        }
        .with_context(|| {
            format!(
                "failed to resolve unpack source root for package {}",
                package.id
            )
        })?;
        sync_unpacked_zip_from_source(
            &config.packaged_dir.join(&package.filename),
            &source_root,
            unpacked_root,
            &package.filename,
            Some(&package.checksum_sha256),
        )
        .with_context(|| format!("failed to unpack package {}", package.id))?;
    }
    Ok(())
}

fn resolve_cycle_bundle_package_unpack_source_root_from_build_manifest(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    package: &BundlePackageArtifact,
) -> anyhow::Result<Option<PathBuf>> {
    if package.cycle.is_none() {
        return Ok(None);
    }
    let region_id = package
        .region_id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let node_name = match package.family_id.as_str() {
        "nav-db" => "nav-db".to_string(),
        "csup" => "csup-package".to_string(),
        "tpp" => format!("tpp-{region_id}-package"),
        "sec" | "tac" | "enr-l" | "enr-h" => {
            format!("charts-{}-package", package.family_id)
        }
        "vectors" => "vectors".to_string(),
        _ => return Ok(None),
    };
    let record = match build_manifest
        .nodes
        .iter()
        .find(|node| node.name == node_name)
    {
        Some(record) => record,
        None => return Ok(None),
    };
    let root = if package.family_id == "nav-db" {
        resolve_nav_db_unpack_source_root_from_record(config, record)
    } else {
        record
            .outputs
            .get("unpack_source_root")
            .map(|path| resolve_artifact_path(config, path))
    };
    Ok(root)
}

fn resolve_nav_db_unpack_source_root_from_record(
    config: &ProductBuildConfig,
    record: &NodeRecord,
) -> Option<PathBuf> {
    if let Some(path) = record.outputs.get("unpack_source_root") {
        return Some(resolve_artifact_path(config, path));
    }
    let zip_path = record
        .outputs
        .get("nav_db_zip")
        .map(|path| resolve_artifact_path(config, path))?;
    zip_path.parent().map(|parent| parent.join("nav_db"))
}

fn resolve_cycle_bundle_package_unpack_source_root(
    task_values: &BTreeMap<String, ProductTaskValue>,
    bundle_cycle: &str,
    package: &BundlePackageArtifact,
) -> anyhow::Result<Option<PathBuf>> {
    fn task_id(cycle: &str, name: &str) -> String {
        format!("{cycle}:{name}")
    }

    if package.cycle.is_none() {
        return Ok(None);
    }
    let region_id = package
        .region_id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let task_id = match package.family_id.as_str() {
        "nav-db" => task_id(bundle_cycle, "nav-db"),
        "csup" => task_id(bundle_cycle, "csup-package"),
        "tpp" => task_id(bundle_cycle, &format!("tpp-{region_id}-package")),
        "sec" | "tac" | "enr-l" | "enr-h" => task_id(
            bundle_cycle,
            &format!("charts-{}-package", package.family_id),
        ),
        "vectors" => task_id(bundle_cycle, "vectors"),
        _ => return Ok(None),
    };

    let root = match task_values.get(&task_id) {
        Some(ProductTaskValue::ChartSource(source)) => source.unpack_source_root.clone(),
        Some(ProductTaskValue::CsupSource(source)) => source.unpack_source_root.clone(),
        Some(ProductTaskValue::FingerprintedTppSource { source, .. }) => {
            source.unpack_source_root.clone()
        }
        Some(ProductTaskValue::PublishedNavDb {
            unpack_source_root, ..
        }) => unpack_source_root.clone(),
        _ => return Ok(None),
    };
    Ok(Some(root))
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

fn sync_unpacked_discovery_manifests(
    _packaged_root: &Path,
    _current_artifacts_path: &Path,
    _unpacked_root: &Path,
) -> anyhow::Result<()> {
    Ok(())
}

fn static_product_unpacked_strategy(
    id: &str,
    unpack_source_root: &Path,
) -> anyhow::Result<PublishedZipUnpackStrategy> {
    if !unpack_source_root.is_dir() {
        bail!(
            "static product {} must unpack from declared source root {}, but it does not exist",
            id,
            unpack_source_root.display()
        );
    }
    Ok(PublishedZipUnpackStrategy::HardlinkZipMembers {
        source_root: unpack_source_root.to_path_buf(),
    })
}

fn sync_product_level_unpacked(
    packaged_dir: &Path,
    product_artifacts_path: &Path,
    zip_artifacts: &[PublishedZipArtifact],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_packaged_dir(packaged_dir)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(packaged_dir, product_artifacts_path, &unpacked_root)?;
    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(product_artifacts_path)
            .with_context(|| format!("failed to read {}", product_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", product_artifacts_path.display()))?;
    if let Some(diagnostics) = &current.diagnostics {
        sync_unpacked_file(&packaged_dir.join(&diagnostics.filename), &unpacked_root)?;
    }
    for artifact in zip_artifacts {
        let published_filename = artifact
            .published_zip_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("failed to determine published filename"))?;
        match &artifact.unpack_strategy {
            PublishedZipUnpackStrategy::HardlinkZipMembers { source_root } => {
                sync_unpacked_zip_from_source(
                    &artifact.published_zip_path,
                    source_root,
                    &unpacked_root,
                    published_filename,
                    Some(&artifact.checksum_sha256),
                )?;
            }
        }
    }
    cleanup_published_unpacked_root(&unpacked_root, product_artifacts_path)?;
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
    .filter(|cycle| product_cycle_is_currently_publishable(cycle, as_of_date))
    .collect::<Vec<_>>();
    cycles.sort();
    cycles.dedup();
    if cycles.is_empty() {
        anyhow::bail!("no published FAA cycles are currently buildable");
    }
    Ok(cycles)
}

fn product_cycle_is_currently_publishable(cycle: &str, as_of_date: NaiveDate) -> bool {
    match cycle_effective_date(cycle) {
        Ok(effective) => {
            let publication_opens = effective - chrono::Duration::days(CYCLE_PUBLICATION_LEAD_DAYS);
            publication_opens <= as_of_date && effective + chrono::Duration::days(28) >= as_of_date
        }
        Err(_) => false,
    }
}

mod static_products;
use static_products::*;

mod artifact_verification;
use artifact_verification::*;

mod publication;
use publication::*;
pub use publication::{merge_current_artifacts_manifests, publish_discovery_manifest};

mod gc;
use gc::*;
pub use gc::{gc_build_cache, gc_fetch_cache, gc_publication};

mod artifact_retention;
pub use artifact_retention::{gc_artifact_retention, ArtifactRetentionGcReport};

mod rust_build_gc;
pub use rust_build_gc::{gc_rust_build_cache, RustBuildCacheGcReport};

mod config;
pub(crate) use config::default_artifact_write_path;
use config::*;

mod cycle_nodes;
use cycle_nodes::*;

mod node_cache;
use node_cache::*;

fn manifest_chart_name(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "SEC",
        ChartFamily::Tac => "TAC",
        ChartFamily::Flyway => "FLY",
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

fn hard_link_or_copy_file(from: &Path, to: &Path) -> anyhow::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        Err(link_error) => {
            fs::copy(from, to).with_context(|| {
                format!(
                    "failed to hardlink {} to {} ({link_error}); copy fallback also failed",
                    from.display(),
                    to.display()
                )
            })?;
            Ok(())
        }
    }
}

fn hard_link_or_copy_dir_recursive(from: &Path, to: &Path) -> anyhow::Result<()> {
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
            if dest.is_file() {
                fs::remove_file(&dest)
                    .with_context(|| format!("failed to remove {}", dest.display()))?;
            }
            hard_link_or_copy_dir_recursive(&source, &dest)?;
        } else if file_type.is_file() {
            if dest.is_dir() {
                fs::remove_dir_all(&dest)
                    .with_context(|| format!("failed to remove {}", dest.display()))?;
            } else {
                let _ = fs::remove_file(&dest);
            }
            hard_link_or_copy_file(&source, &dest)?;
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

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn utc_now_string() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

mod process;
pub use process::{ensure_nofile_limit, maybe_reexec_build_under_cgroup};

mod logging;
use logging::*;

fn family_slug(family: ChartFamily) -> &'static str {
    match family {
        ChartFamily::Sec => "sec",
        ChartFamily::Tac => "tac",
        ChartFamily::Flyway => "flyway",
        ChartFamily::EnrL => "enr-l",
        ChartFamily::EnrH => "enr-h",
    }
}

fn chart_source_family(family: ChartFamily) -> ChartFamily {
    match family {
        ChartFamily::Flyway => ChartFamily::Tac,
        family => family,
    }
}

fn chart_source_urls_path(source_urls_dir: &Path, family: ChartFamily) -> PathBuf {
    source_urls_dir.join(format!(
        "charts-{}/source_urls.jsonl",
        family_slug(chart_source_family(family))
    ))
}

fn chart_family_version_label(
    source_urls_dir: &Path,
    family: ChartFamily,
) -> anyhow::Result<String> {
    let source_urls = chart_source_urls_path(source_urls_dir, family);
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

fn cycle_data_requests(requests: Vec<PrefetchRequest>) -> Vec<PrefetchRequest> {
    requests
        .into_iter()
        .filter(|request| !request.url.ends_with("/DAILY_DOF_DAT.ZIP"))
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

    fn test_contracts(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(family_id, contract_id)| ((*family_id).to_string(), (*contract_id).to_string()))
            .collect()
    }
    use chrono::TimeZone;
    use preprocessor_resource_index::{
        AirportRecord, AirportResourcesRecord, ChartCollectionRecord, CoverageBounds, CsupRecord,
        DefaultView, NavDbRef, PlateRecord, ResourceFamily, ResourcePackage, ResourceRegion,
        TemporalSummary, TileLevelRecord,
    };
    use product_contracts::{CSUP_CONTRACT_ID, ENR_L_CONTRACT_ID, SEC_CONTRACT_ID};
    use tempfile::tempdir;

    #[test]
    fn terrain_wide_metadata_declares_automatic_wide_coverage() {
        let metadata = stable_product_package_metadata(&format!(
            "terrain-{WIDE_ANGLE_REGION_ID}_{TERRAIN_CONTRACT_ID}"
        ));

        assert_eq!(metadata.get("wide_angle"), Some(&serde_json::json!(true)));
        assert_eq!(
            metadata.get("wide_angle_region_id"),
            Some(&serde_json::json!(WIDE_ANGLE_REGION_ID))
        );
        assert_eq!(
            metadata.get("max_source_zoom"),
            Some(&serde_json::json!(FULL_COVERAGE_ZOOM))
        );
    }

    fn write_source_urls(root: &Path, relative: &str, lines: &[&str]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    fn bundle_package(family_id: &str, region_id: Option<&str>) -> BundlePackageArtifact {
        BundlePackageArtifact {
            id: format!("{}_2605_01", family_id.to_ascii_uppercase()),
            family_id: family_id.to_string(),
            contract_id: product_contract_id_for_family(family_id)
                .unwrap()
                .to_string(),
            region_id: region_id.map(str::to_string),
            filename: format!("{family_id}_2605_01_deadbeef.zip"),
            relative_path: format!("{family_id}_2605_01_deadbeef.zip"),
            cycle: Some("2605".to_string()),
            cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
            checksum_sha256: "deadbeef".to_string(),
            size_bytes: 123,
            published_at_utc: None,
            source_generated_at_utc: None,
            source_version: None,
            source_fetched_at_utc: None,
            effective_date: Some("2026-05-14".to_string()),
            expiration_date: Some("2026-06-11".to_string()),
            warning_text: None,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn product_warning_texts_are_disabled_by_default() {
        assert_eq!(nav_db::nav_db_warning_text(), None);
        assert_eq!(nav_db::nav_kv_family_warning_text("enr-h"), None);
        assert_eq!(nav_db::nav_kv_family_warning_text("sec"), None);
    }

    #[test]
    fn canonical_chart_detail_filename_preserves_tier_identity() {
        assert_eq!(
            canonical_package_filename_hashed(
                "sec",
                "nw",
                "NW_SEC_DETAIL_SEC1_2607.zip",
                "abc123",
                Some(ChartPackageTier::Detail),
            )
            .expect("detail filename"),
            "sec_nw_detail_SEC1_2607_01_abc123.zip"
        );
    }

    #[test]
    fn automatic_product_build_includes_cycles_inside_publication_lead_window() {
        let before_lead_window = NaiveDate::from_ymd_opt(2026, 6, 18).unwrap();
        let inside_lead_window = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let on_2607 = NaiveDate::from_ymd_opt(2026, 7, 9).unwrap();
        let after_2607_expires = NaiveDate::from_ymd_opt(2026, 8, 7).unwrap();

        assert!(product_cycle_is_currently_publishable(
            "2606",
            inside_lead_window
        ));
        assert!(!product_cycle_is_currently_publishable(
            "2607",
            before_lead_window
        ));
        assert!(product_cycle_is_currently_publishable(
            "2607",
            inside_lead_window
        ));
        assert!(product_cycle_is_currently_publishable("2607", on_2607));
        assert!(!product_cycle_is_currently_publishable(
            "2607",
            after_2607_expires
        ));
    }

    #[test]
    fn package_unpack_source_root_mirrors_package_member_namespace() {
        use zip::{write::SimpleFileOptions, ZipWriter};

        let temp = tempdir().expect("tempdir");
        let asset_root = temp.path().join("assets");
        let package_root = temp.path().join("packages");
        let unpack_source_root = temp.path().join("unpack-source");
        fs::create_dir_all(asset_root.join("afd/01A")).expect("create asset dir");
        fs::create_dir_all(&package_root).expect("create package dir");
        fs::write(
            package_root.join("AK_CSUP_2605.manifest"),
            b"generated manifest",
        )
        .expect("write manifest");
        fs::write(
            package_root.join("package-assets.json"),
            b"generated package metadata",
        )
        .expect("write package metadata");
        fs::create_dir_all(package_root.join("thumbnails/plates/BKH"))
            .expect("create generated thumbnail dir");
        fs::write(
            package_root.join("thumbnails/plates/BKH/APD-HI-AIRPORT DIAGRAM.png"),
            b"generated thumbnail",
        )
        .expect("write generated thumbnail");
        fs::write(asset_root.join("afd/01A/CSUP-AK_0.png"), b"source image").expect("write image");

        let zip_path = package_root.join("AK_CSUP_2605.zip");
        let file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(file);
        zip.start_file("AK_CSUP_2605.manifest", SimpleFileOptions::default())
            .expect("start manifest");
        zip.write_all(b"zip manifest").expect("write manifest");
        zip.start_file("package-assets.json", SimpleFileOptions::default())
            .expect("start package metadata");
        zip.write_all(b"zip package metadata")
            .expect("write package metadata");
        zip.start_file(
            "thumbnails/plates/BKH/APD-HI-AIRPORT DIAGRAM.png",
            SimpleFileOptions::default(),
        )
        .expect("start generated thumbnail");
        zip.write_all(b"zip generated thumbnail")
            .expect("write generated thumbnail");
        zip.start_file("afd/01A/CSUP-AK_0.png", SimpleFileOptions::default())
            .expect("start image");
        zip.write_all(b"zip image").expect("write image");
        zip.finish().expect("finish zip");

        prepare_package_unpack_source_root(
            &[zip_path],
            &asset_root,
            &package_root,
            &unpack_source_root,
            &["thumbnails/"],
        )
        .expect("prepare unpack source root");

        assert_eq!(
            fs::read(unpack_source_root.join("AK_CSUP_2605.manifest")).expect("read manifest"),
            b"generated manifest"
        );
        assert_eq!(
            fs::read(unpack_source_root.join("package-assets.json"))
                .expect("read package metadata"),
            b"generated package metadata"
        );
        assert_eq!(
            fs::read(unpack_source_root.join("thumbnails/plates/BKH/APD-HI-AIRPORT DIAGRAM.png"))
                .expect("read generated thumbnail"),
            b"generated thumbnail"
        );
        assert_eq!(
            fs::read(unpack_source_root.join("afd/01A/CSUP-AK_0.png")).expect("read image"),
            b"source image"
        );
    }

    #[test]
    fn cycle_bundle_package_resolution_uses_declared_unpack_source_root() {
        let temp = tempdir().expect("tempdir");
        let asset_root = temp.path().join("assets");
        let package_root = temp.path().join("packages");
        let unpack_source_root = temp.path().join("unpack-source");
        let package_outputs_path = package_root.join("package_outputs.jsonl");

        let mut task_values = BTreeMap::new();
        task_values.insert(
            "2605:charts-sec-package".to_string(),
            ProductTaskValue::ChartSource(ChartSource {
                family_id: "sec".to_string(),
                package_outputs_path: package_outputs_path.clone(),
                asset_root: asset_root.clone(),
                package_root: package_root.clone(),
                unpack_source_root: unpack_source_root.clone(),
                source_urls_path: None,
            }),
        );
        task_values.insert(
            "2605:csup-package".to_string(),
            ProductTaskValue::CsupSource(AssetSource {
                package_outputs_path: package_outputs_path.clone(),
                asset_root: asset_root.clone(),
                package_root: package_root.clone(),
                unpack_source_root: unpack_source_root.clone(),
                source_urls_path: None,
            }),
        );
        task_values.insert(
            "2605:tpp-ak-package".to_string(),
            ProductTaskValue::FingerprintedTppSource {
                source: AssetSource {
                    package_outputs_path,
                    asset_root: asset_root.clone(),
                    package_root: package_root.clone(),
                    unpack_source_root: unpack_source_root.clone(),
                    source_urls_path: None,
                },
                fingerprint: "tpp-fingerprint".to_string(),
            },
        );
        task_values.insert(
            "2605:nav-db".to_string(),
            ProductTaskValue::PublishedNavDb {
                package: bundle_package("nav-db", None),
                unpack_source_root: unpack_source_root.clone(),
            },
        );

        for package in [
            bundle_package("sec", Some("nw")),
            bundle_package("csup", Some("ak")),
            bundle_package("tpp", Some("ak")),
            bundle_package("nav-db", None),
        ] {
            let resolved =
                resolve_cycle_bundle_package_unpack_source_root(&task_values, "2605", &package)
                    .unwrap()
                    .expect("package root");
            assert_eq!(resolved, unpack_source_root);
        }
    }

    #[test]
    fn nav_db_build_manifest_resolution_uses_node_output_source_root() {
        let temp = tempdir().expect("tempdir");
        let build_root = temp.path().join("artifacts");
        let config = ProductBuildConfig {
            chart_metadata_root: temp.path().join("chart-metadata"),
            build_root: build_root.clone(),
            publish_dir: build_root
                .join("published")
                .join("master")
                .join("20260514T000000Z"),
            packaged_dir: build_root
                .join("published")
                .join("master")
                .join("20260514T000000Z")
                .join("packaged"),
            publish_label: "master".to_string(),
            publish_timestamp: "20260514T000000Z".to_string(),
            target_cycle: Some("2605".to_string()),
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: build_root.join("cache").join("fetch"),
            fetch_cache_mode: "shared".to_string(),
        };
        let build_manifest = BuildManifest {
            schema_version: 1,
            cycle: "2605".to_string(),
            build_root: build_root.display().to_string(),
            generated_at_utc: "2026-05-14T00:00:00Z".to_string(),
            fetch_cache_root: "cache/fetch".to_string(),
            fetch_cache_mode: "shared".to_string(),
            nodes: vec![NodeRecord {
                name: "nav-db".to_string(),
                fingerprint: "fingerprint".to_string(),
                started_at_utc: "2026-05-14T00:00:00Z".to_string(),
                finished_at_utc: "2026-05-14T00:00:00Z".to_string(),
                elapsed_ms: 0,
                cache_hit: true,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([(
                    "nav_db_zip".to_string(),
                    "cache/nodes/nav-db/abc/output/nav_db_2605.zip".to_string(),
                )]),
                output_details: BTreeMap::new(),
                fetch_cache_refs: Vec::new(),
            }],
        };

        let resolved = resolve_cycle_bundle_package_unpack_source_root_from_build_manifest(
            &config,
            &build_manifest,
            &bundle_package("nav-db", None),
        )
        .unwrap()
        .expect("nav-db source root");

        assert_eq!(
            resolved,
            build_root.join("cache/nodes/nav-db/abc/output/nav_db")
        );
    }

    #[test]
    fn static_product_unpack_strategy_uses_declared_source_root() {
        let temp = tempdir().expect("tempdir");
        let terrain_root = temp.path().join("terrain-output");
        fs::create_dir_all(&terrain_root).expect("create terrain root");

        let strategy =
            static_product_unpacked_strategy("terrain-ak", &terrain_root).expect("strategy");
        match strategy {
            PublishedZipUnpackStrategy::HardlinkZipMembers { source_root } => {
                assert_eq!(source_root, terrain_root);
            }
        }
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
                ResourceFamily {
                    id: "enr-h".to_string(),
                    display_name: "IFR High".to_string(),
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
                artifact_path: String::new(),
                size_bytes: 0,
                checksum_sha256: String::new(),
                cycle_code: None,
                version_label: None,
                effective_date: None,
                expiration_date: None,
                metadata: chart_wide_angle_package_metadata(false, Some(1)),
            }],
            chart_collections: vec![ChartCollectionRecord {
                id: "sec:nw".to_string(),
                family_id: "sec".to_string(),
                region_id: "nw".to_string(),
                package_id: "NW_SEC".to_string(),
                detail_package_id: None,
                chart_index: 0,
                tile_path_template: "tiles/0/{z}/{x}/{y}.webp".to_string(),
                levels: vec![TileLevelRecord {
                    zoom: 10,
                    boxes: vec![TileBoundsRecord {
                        x_min: 1,
                        x_max: 2,
                        y_tms_min: 3,
                        y_tms_max: 4,
                    }],
                }],
                detail_levels: Vec::new(),
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
            chart_references: vec![],
            airports: Vec::<AirportRecord>::new(),
            airport_resources: Vec::<AirportResourcesRecord>::new(),
            plates: Vec::<PlateRecord>::new(),
            csups: Vec::<CsupRecord>::new(),
        }
    }

    fn package_publication_test_config(root: &Path) -> ProductBuildConfig {
        let build_root = root.join("artifacts");
        let publish_dir = build_root
            .join("published")
            .join("test")
            .join("20260731T000000Z");
        ProductBuildConfig {
            chart_metadata_root: root.join("chart-metadata"),
            build_root: build_root.clone(),
            publish_dir: publish_dir.clone(),
            packaged_dir: publish_dir.join("packaged"),
            publish_label: "test".to_string(),
            publish_timestamp: "20260731T000000Z".to_string(),
            target_cycle: Some("2607".to_string()),
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: build_root.join("cache/fetch"),
            fetch_cache_mode: "cache-first".to_string(),
        }
    }

    fn chart_resource_package(
        config: &ProductBuildConfig,
        id: &str,
        relative_path: &str,
        payload: &[u8],
        tier: ChartPackageTier,
    ) -> ResourcePackage {
        let path = config.build_root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, payload).unwrap();
        ResourcePackage {
            id: id.to_string(),
            family_id: "tac".to_string(),
            region_id: "nw".to_string(),
            artifact_path: relative_path.to_string(),
            size_bytes: payload.len() as u64,
            checksum_sha256: hash_file(&path).unwrap(),
            cycle_code: Some("2607".to_string()),
            version_label: Some(PACKAGE_CYCLE_VERSION.to_string()),
            effective_date: Some("2026-07-09".to_string()),
            expiration_date: Some("2026-08-06".to_string()),
            metadata: BTreeMap::from([(
                CHART_PACKAGE_TIER_METADATA_KEY.to_string(),
                serde_json::to_value(tier).unwrap(),
            )]),
        }
    }

    fn build_manifest_for_resource_index(
        config: &ProductBuildConfig,
        resource_index: &ResourceIndex,
    ) -> BuildManifest {
        let relative_path = "intermediate/resource-index.json";
        let path = config.build_root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(resource_index).unwrap()).unwrap();
        BuildManifest {
            schema_version: 1,
            cycle: "2607".to_string(),
            build_root: config.build_root.display().to_string(),
            generated_at_utc: "2026-07-31T00:00:00Z".to_string(),
            fetch_cache_root: "cache/fetch".to_string(),
            fetch_cache_mode: "cache-first".to_string(),
            nodes: vec![NodeRecord {
                name: "resource-index".to_string(),
                fingerprint: "resource-index-test".to_string(),
                started_at_utc: "2026-07-31T00:00:00Z".to_string(),
                finished_at_utc: "2026-07-31T00:00:00Z".to_string(),
                elapsed_ms: 0,
                cache_hit: false,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([(
                    "resource_index".to_string(),
                    relative_path.to_string(),
                )]),
                output_details: BTreeMap::new(),
                fetch_cache_refs: Vec::new(),
            }],
        }
    }

    #[test]
    fn bundle_publication_preserves_each_exact_same_region_package_payload() {
        let temp = tempdir().unwrap();
        let config = package_publication_test_config(temp.path());
        fs::create_dir_all(&config.packaged_dir).unwrap();
        let regional_payload = b"regional zoom payload";
        let detail_payload = b"detail zoom payload";
        let regional = chart_resource_package(
            &config,
            "NW_TAC_TAC1_2607",
            "cache/packages/NW_TAC_TAC1_2607.zip",
            regional_payload,
            ChartPackageTier::Regional,
        );
        let detail = chart_resource_package(
            &config,
            "NW_TAC_DETAIL_TAC1_2607",
            "cache/packages/NW_TAC_DETAIL_TAC1_2607.zip",
            detail_payload,
            ChartPackageTier::Detail,
        );
        let mut index = minimal_resource_index();
        index.cycle = Some("2607".to_string());
        index.temporal_summary.uniform_effective_date = Some("2026-07-09".to_string());
        index.temporal_summary.uniform_expiration_date = Some("2026-08-06".to_string());
        index.packages = vec![regional.clone(), detail.clone()];
        let build_manifest = build_manifest_for_resource_index(&config, &index);
        let mut nav_db = bundle_package("nav-db", None);
        nav_db.cycle = Some("2607".to_string());

        let bundle = build_bundle_manifest(&config, &build_manifest, &[], &nav_db).unwrap();

        for (source, expected_payload) in [
            (&regional, regional_payload.as_slice()),
            (&detail, detail_payload.as_slice()),
        ] {
            let published = bundle
                .packages
                .iter()
                .find(|package| package.id == source.id)
                .unwrap();
            let published_path = config.packaged_dir.join(&published.filename);
            assert_eq!(fs::read(&published_path).unwrap(), expected_payload);
            verify_artifact_file(
                &published_path,
                &source.checksum_sha256,
                source.size_bytes,
                &source.id,
            )
            .unwrap();
        }

        let bundle_path = write_hashed_bundle_manifest(&config.packaged_dir, &bundle).unwrap();
        let regional_published = bundle
            .packages
            .iter()
            .find(|package| package.id == regional.id)
            .unwrap();
        fs::write(
            config.packaged_dir.join(&regional_published.filename),
            b"tampered regional payload",
        )
        .unwrap();
        let error = validate_bundle_manifest(&config.packaged_dir, &bundle_path).unwrap_err();
        assert!(
            error.to_string().contains("mismatch"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn bundle_publication_rejects_catalog_path_checksum_mismatch() {
        let temp = tempdir().unwrap();
        let config = package_publication_test_config(temp.path());
        fs::create_dir_all(&config.packaged_dir).unwrap();
        let mut package = chart_resource_package(
            &config,
            "NW_TAC_TAC1_2607",
            "cache/packages/NW_TAC_TAC1_2607.zip",
            b"regional zoom payload",
            ChartPackageTier::Regional,
        );
        package.checksum_sha256 = "0".repeat(64);
        let mut index = minimal_resource_index();
        index.cycle = Some("2607".to_string());
        index.temporal_summary.uniform_effective_date = Some("2026-07-09".to_string());
        index.temporal_summary.uniform_expiration_date = Some("2026-08-06".to_string());
        index.packages = vec![package];
        let build_manifest = build_manifest_for_resource_index(&config, &index);
        let nav_db = bundle_package("nav-db", None);

        let error = build_bundle_manifest(&config, &build_manifest, &[], &nav_db).unwrap_err();
        assert!(
            error.to_string().contains("checksum mismatch"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn same_region_chart_packages_unpack_from_their_exact_published_zip() {
        let temp = tempdir().unwrap();
        let config = package_publication_test_config(temp.path());
        fs::create_dir_all(&config.packaged_dir).unwrap();
        let source_root = config.build_root.join("cache/chart-unpack-source");
        let regional_member = "tiles/1/9/10/11.webp";
        let detail_member = "tiles/1/12/80/88.webp";
        for (member, payload) in [
            (regional_member, b"regional tile".as_slice()),
            (detail_member, b"detail tile".as_slice()),
        ] {
            let path = source_root.join(member);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, payload).unwrap();
        }

        let make_package = |id: &str, member: &str, tier: ChartPackageTier| {
            let source_path = source_root.join(member);
            let temporary_zip = config.build_root.join(format!("cache/{id}.zip"));
            write_deterministic_zip(
                &temporary_zip,
                &[ZipSource::new(member, &source_path).stored()],
            )
            .unwrap();
            let checksum_sha256 = hash_file(&temporary_zip).unwrap();
            let filename = canonical_package_filename_hashed(
                "tac",
                "nw",
                &format!("{id}.zip"),
                &checksum_sha256,
                Some(tier),
            )
            .unwrap();
            let published_path = config.packaged_dir.join(&filename);
            publish_flat_artifact(&temporary_zip, &published_path).unwrap();
            BundlePackageArtifact {
                id: id.to_string(),
                family_id: "tac".to_string(),
                contract_id: product_contract_id_for_family("tac").unwrap().to_string(),
                region_id: Some("nw".to_string()),
                filename: filename.clone(),
                relative_path: filename,
                cycle: Some("2607".to_string()),
                cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
                checksum_sha256,
                size_bytes: fs::metadata(published_path).unwrap().len(),
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-07-09".to_string()),
                expiration_date: Some("2026-08-06".to_string()),
                warning_text: None,
                metadata: BTreeMap::from([(
                    CHART_PACKAGE_TIER_METADATA_KEY.to_string(),
                    serde_json::to_value(tier).unwrap(),
                )]),
            }
        };
        let regional = make_package(
            "NW_TAC_TAC1_2607",
            regional_member,
            ChartPackageTier::Regional,
        );
        let detail = make_package(
            "NW_TAC_DETAIL_TAC1_2607",
            detail_member,
            ChartPackageTier::Detail,
        );
        let bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2607_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2607".to_string(),
            cycle_version: PACKAGE_CYCLE_VERSION.to_string(),
            generated_at_utc: "2026-07-31T00:00:00Z".to_string(),
            effective_date: "2026-07-09".to_string(),
            expiration_date: "2026-08-06".to_string(),
            start_valid: "2026-07-09".to_string(),
            end_valid: "2026-08-06".to_string(),
            packages: vec![regional.clone(), detail.clone()],
            ancillary: vec![],
        };
        let task_values = BTreeMap::from([(
            "2607:charts-tac-package".to_string(),
            ProductTaskValue::ChartSource(ChartSource {
                family_id: "tac".to_string(),
                package_outputs_path: config.build_root.join("unused-package-outputs.jsonl"),
                asset_root: source_root.clone(),
                package_root: config.build_root.join("unused-package-root"),
                unpack_source_root: source_root,
                source_urls_path: None,
            }),
        )]);
        let unpacked_root = published_unpacked_root(&config).unwrap();
        fs::create_dir_all(&unpacked_root).unwrap();

        sync_cycle_bundle_unpacked_zips(&config, &bundle, &unpacked_root, Some(&task_values))
            .unwrap();

        assert_eq!(
            fs::read(
                unpacked_root
                    .join(zip_stem(&regional.filename).unwrap())
                    .join(regional_member)
            )
            .unwrap(),
            b"regional tile"
        );
        assert_eq!(
            fs::read(
                unpacked_root
                    .join(zip_stem(&detail.filename).unwrap())
                    .join(detail_member)
            )
            .unwrap(),
            b"detail tile"
        );
    }

    #[test]
    fn offline_region_catalog_emits_chart_bounds_and_plate_convex_hulls() {
        let mut index = minimal_resource_index();
        index.airports = vec![
            AirportRecord {
                id: "KAAA".to_string(),
                facility_name: "A".to_string(),
                lat: 47.0,
                lon: -124.0,
                airport_type: "AIRPORT".to_string(),
            },
            AirportRecord {
                id: "KBBB".to_string(),
                facility_name: "B".to_string(),
                lat: 45.0,
                lon: -123.0,
                airport_type: "AIRPORT".to_string(),
            },
            AirportRecord {
                id: "KCCC".to_string(),
                facility_name: "C".to_string(),
                lat: 45.0,
                lon: -121.0,
                airport_type: "AIRPORT".to_string(),
            },
            AirportRecord {
                id: "KDDD".to_string(),
                facility_name: "D".to_string(),
                lat: 47.0,
                lon: -121.0,
                airport_type: "AIRPORT".to_string(),
            },
            AirportRecord {
                id: "KINR".to_string(),
                facility_name: "Interior".to_string(),
                lat: 46.0,
                lon: -122.0,
                airport_type: "AIRPORT".to_string(),
            },
        ];
        index.plates = vec![
            test_plate_record("plate:a", "KAAA"),
            test_plate_record("plate:b", "KBBB"),
            test_plate_record("plate:inside", "KINR"),
        ];
        index.csups = vec![
            test_csup_record("csup:c", "KCCC"),
            test_csup_record("csup:d", "KDDD"),
        ];

        let catalog = build_offline_region_catalog(&index, &BTreeMap::new());
        let chart = catalog
            .regions
            .iter()
            .find(|region| region.id == "chart:nw")
            .expect("chart region");
        assert_eq!(chart.kind, "chart");
        assert_eq!(
            chart.polygons[0][0],
            OfflineRegionLatLon {
                lat: 50.0,
                lon: -125.0
            }
        );
        assert_eq!(
            chart.polygons[0][2],
            OfflineRegionLatLon {
                lat: 40.0,
                lon: -103.0
            }
        );

        let plate = catalog
            .regions
            .iter()
            .find(|region| region.id == "plate:nw")
            .expect("plate region");
        assert_eq!(plate.kind, "plate");
        assert_eq!(plate.color_key, "class_c_magenta");
        assert_eq!(plate.polygons.len(), 1);
        assert_eq!(plate.polygons[0].len(), 4);
        assert!(plate.polygons[0].contains(&OfflineRegionLatLon {
            lat: 47.0,
            lon: -124.0
        }));
        assert!(!plate.polygons[0].contains(&OfflineRegionLatLon {
            lat: 46.0,
            lon: -122.0
        }));
    }

    #[test]
    fn pac_chart_offline_region_falls_back_to_multiple_bounds_polygons() {
        let index = minimal_resource_index();
        let catalog = build_offline_region_catalog(&index, &BTreeMap::new());
        let pac = catalog
            .regions
            .iter()
            .find(|region| region.id == "chart:pac")
            .expect("PAC chart region");

        assert_eq!(pac.polygons.len(), 3);
        assert!(pac
            .polygons
            .iter()
            .any(|polygon| polygon.contains(&OfflineRegionLatLon {
                lat: -16.0,
                lon: -174.0
            })));
        assert!(pac
            .polygons
            .iter()
            .any(|polygon| polygon.contains(&OfflineRegionLatLon {
                lat: 10.0,
                lon: 147.0
            })));
    }

    #[test]
    fn chart_offline_region_uses_simplified_cutline_union_when_available() {
        let index = minimal_resource_index();
        let mut polygon_sets = BTreeMap::new();
        polygon_sets.insert(
            "sec:nw".to_string(),
            ChartCutlinePolygonSetRecord {
                schema_version: 1,
                id: "chart-coverage:sec:nw".to_string(),
                polygons: vec![
                    ChartCutlinePolygonRecord {
                        id: "chart-coverage:sec:nw:0".to_string(),
                        points: vec![
                            [-124.0, 49.0],
                            [-120.0, 49.0],
                            [-120.0, 45.0],
                            [-124.0, 45.0],
                            [-124.0, 49.0],
                        ],
                    },
                    ChartCutlinePolygonRecord {
                        id: "chart-coverage:sec:nw:1".to_string(),
                        points: vec![
                            [-120.001, 49.0],
                            [-116.0, 49.0],
                            [-116.0, 45.0],
                            [-120.001, 45.0],
                            [-120.001, 49.0],
                        ],
                    },
                    ChartCutlinePolygonRecord {
                        id: "chart-coverage:sec:nw:2".to_string(),
                        points: vec![
                            [-150.0, 60.0],
                            [-140.0, 60.0],
                            [-140.0, 55.0],
                            [-150.0, 55.0],
                            [-150.0, 60.0],
                        ],
                    },
                ],
            },
        );

        let catalog = build_offline_region_catalog(&index, &polygon_sets);
        let chart = catalog
            .regions
            .iter()
            .find(|region| region.id == "chart:nw")
            .expect("chart region");

        assert_eq!(chart.polygons.len(), 1);
        assert!(
            chart.polygons[0].len() < 10,
            "offline chart polygon should be simplified: {:?}",
            chart.polygons[0]
        );
        assert!(
            chart.polygons[0]
                .iter()
                .any(|point| point.lon > -116.02 && point.lon < -115.98),
            "cutline-derived polygon should reach the eastern cutline, not the NW bbox: {:?}",
            chart.polygons[0]
        );
        assert!(
            !chart.polygons[0].iter().any(|point| point.lon < -124.5),
            "cutline-derived polygon should replace the coarse NW bbox: {:?}",
            chart.polygons[0]
        );
        for polygon in &chart.polygons {
            for point in polygon {
                assert!(
                    (-125.0..=-103.0).contains(&point.lon) && (40.0..=50.0).contains(&point.lat),
                    "cutline union should be clipped to NW bounds, got {point:?}"
                );
            }
        }
    }

    #[test]
    fn offline_region_plate_hull_wraps_antimeridian() {
        let hull = convex_hull_lat_lon(vec![
            OfflineRegionLatLon {
                lat: 52.0,
                lon: 179.0,
            },
            OfflineRegionLatLon {
                lat: 53.0,
                lon: -179.0,
            },
            OfflineRegionLatLon {
                lat: 54.0,
                lon: -172.0,
            },
            OfflineRegionLatLon {
                lat: 51.0,
                lon: 176.0,
            },
        ]);
        assert!(hull.iter().any(|point| point.lon > 170.0));
        assert!(hull.iter().any(|point| point.lon < -170.0));
        let mut unwrapped = hull
            .iter()
            .map(|point| point.lon.rem_euclid(360.0))
            .collect::<Vec<_>>();
        unwrapped.sort_by(f64::total_cmp);
        let largest_gap = unwrapped
            .iter()
            .enumerate()
            .map(|(index, lon)| {
                let next = if index + 1 < unwrapped.len() {
                    unwrapped[index + 1]
                } else {
                    unwrapped[0] + 360.0
                };
                next - lon
            })
            .fold(0.0, f64::max);
        assert!(
            360.0 - largest_gap < 20.0,
            "hull should occupy the small antimeridian span, got {hull:?}"
        );
    }

    #[test]
    fn offline_region_plate_label_wraps_antimeridian() {
        let polygon = convex_hull_lat_lon(vec![
            OfflineRegionLatLon {
                lat: 52.0,
                lon: 179.0,
            },
            OfflineRegionLatLon {
                lat: 53.0,
                lon: -179.0,
            },
            OfflineRegionLatLon {
                lat: 54.0,
                lon: -172.0,
            },
            OfflineRegionLatLon {
                lat: 51.0,
                lon: 176.0,
            },
        ]);
        let label = polygon_label_position(&polygon);

        assert!(
            label.lon.abs() > 170.0,
            "label should stay near the antimeridian instead of averaging to zero: {label:?}"
        );
    }

    #[test]
    fn pacific_plate_label_uses_short_dateline_span() {
        let polygon = convex_hull_lat_lon(vec![
            OfflineRegionLatLon {
                lat: 21.3,
                lon: -157.9,
            },
            OfflineRegionLatLon {
                lat: 19.7,
                lon: -155.1,
            },
            OfflineRegionLatLon {
                lat: 13.5,
                lon: 144.8,
            },
            OfflineRegionLatLon {
                lat: 7.3,
                lon: 134.5,
            },
        ]);
        let label = polygon_label_position(&polygon);

        assert!(
            label.lon.abs() > 150.0,
            "PAC Plates label should use the short Pacific dateline span: {label:?}"
        );
    }

    #[test]
    fn offline_region_labels_are_deconflicted_at_z4() {
        let mut regions = vec![
            OfflineRegionRecord {
                id: "chart:test".to_string(),
                kind: "chart".to_string(),
                region_id: "test".to_string(),
                label: "TEST Charts".to_string(),
                color_key: "class_b_d_blue".to_string(),
                summary: Vec::new(),
                polygons: Vec::new(),
                label_position: OfflineRegionLatLon {
                    lat: 47.0,
                    lon: -122.0,
                },
            },
            OfflineRegionRecord {
                id: "plate:test".to_string(),
                kind: "plate".to_string(),
                region_id: "test".to_string(),
                label: "TEST Plates".to_string(),
                color_key: "class_c_magenta".to_string(),
                summary: Vec::new(),
                polygons: Vec::new(),
                label_position: OfflineRegionLatLon {
                    lat: 47.0,
                    lon: -122.0,
                },
            },
        ];

        deconflict_offline_region_labels(&mut regions);
        let labels = regions
            .iter()
            .map(offline_region_label_layout)
            .collect::<Vec<_>>();
        let dx = shortest_world_delta(labels[1].x - labels[0].x, offline_region_label_world_size());
        let dy = labels[1].y - labels[0].y;
        let overlap_x = (labels[0].width + labels[1].width) / 2.0 - dx.abs();
        let overlap_y = (labels[0].height + labels[1].height) / 2.0 - dy.abs();

        assert!(
            overlap_x <= 0.0 || overlap_y <= 0.0,
            "labels should not overlap after deconfliction: {regions:?}"
        );
    }

    fn test_plate_record(id: &str, airport_id: &str) -> PlateRecord {
        PlateRecord {
            id: id.to_string(),
            airport_id: airport_id.to_string(),
            icao_airport_id: None,
            region_id: "nw".to_string(),
            package_id: "NW_TPP".to_string(),
            asset_path: format!("plates/{airport_id}/A.png"),
            thumbnail_path: format!("thumbnails/plates/{airport_id}/A.png"),
            label: "A".to_string(),
            asset_kind: "plate".to_string(),
            document_type: "approach".to_string(),
            procedure_uid: None,
            cifp_procedure_id: None,
            georef: None,
        }
    }

    fn test_csup_record(id: &str, airport_id: &str) -> CsupRecord {
        CsupRecord {
            id: id.to_string(),
            airport_id: airport_id.to_string(),
            region_id: "nw".to_string(),
            package_id: "NW_CSUP".to_string(),
            asset_path: format!("afd/{airport_id}/CSUP-NW_0.png"),
            thumbnail_path: format!("thumbnails/afd/{airport_id}/CSUP-NW_0.png"),
            label: "Chart Supplement".to_string(),
            asset_kind: "csup".to_string(),
            document_type: "csup".to_string(),
        }
    }

    #[test]
    fn nav_kv_airport_carries_charted_procedures_without_cifp_geometry() {
        let mut index = minimal_resource_index();
        index.airports.push(AirportRecord {
            id: "KSEA".to_string(),
            facility_name: "Seattle-Tacoma International".to_string(),
            lat: 47.45,
            lon: -122.31,
            airport_type: "AIRPORT".to_string(),
        });
        let mut plate = test_plate_record("plate:KSEA:STR-WA-GLASR THREE.png", "KSEA");
        plate.label = "GLASR THREE".to_string();
        plate.document_type = "star".to_string();
        plate.cifp_procedure_id = Some("GLASR3".to_string());
        index.plates.push(plate);
        index.airport_resources.push(AirportResourcesRecord {
            airport_id: "KSEA".to_string(),
            plate_ids: vec!["plate:KSEA:STR-WA-GLASR THREE.png".to_string()],
            csup_ids: Vec::new(),
            package_ids: vec!["NW_TPP".to_string()],
        });

        let pairs = build_nav_kv_plate_pairs(&index).expect("plate pairs");
        let airport = pairs
            .iter()
            .find(|pair| pair.key == "plate/airport/KSEA")
            .map(|pair| {
                serde_json::from_slice::<serde_json::Value>(&pair.value)
                    .expect("decode KSEA plate airport")
            })
            .expect("KSEA plate airport pair");
        assert_eq!(
            airport["charted_procedures"],
            serde_json::json!([{
                "procedure_id": "GLASR3",
                "display_label": "GLASR THREE",
                "kind": "star",
                "plate_id": "plate:KSEA:STR-WA-GLASR THREE.png"
            }])
        );
    }

    #[test]
    fn nav_kv_chart_catalog_includes_shaded_relief_static_products() {
        let static_raster_entries = vec![
            StaticRasterCatalogEntry {
                product_id: "world-basemap_WBM1".to_string(),
                label: "World Basemap".to_string(),
                chart_family: "world-basemap".to_string(),
                tile_url_root: "tiles".to_string(),
                tile_path_template: "0/{z}/{x}/{y}.png".to_string(),
                tile_size: WORLD_BASEMAP_TILE_SIZE,
                min_zoom: WORLD_BASEMAP_MIN_ZOOM,
                max_source_zoom: WORLD_BASEMAP_MAX_SOURCE_ZOOM,
                max_display_zoom: WORLD_BASEMAP_MAX_DISPLAY_ZOOM,
                initial_viewport: DefaultView {
                    lat: 20.0,
                    lon: 0.0,
                    zoom: 1.5,
                },
                levels: vec![TileLevelRecord {
                    zoom: 4,
                    boxes: vec![TileBoundsRecord {
                        x_min: 0,
                        x_max: 15,
                        y_tms_min: 0,
                        y_tms_max: 15,
                    }],
                }],
            },
            StaticRasterCatalogEntry {
                product_id: "shaded-relief-nw_SHD1".to_string(),
                label: String::new(),
                chart_family: "shaded-relief".to_string(),
                tile_url_root: String::new(),
                tile_path_template: "0/{z}/{x}/{y}.webp".to_string(),
                tile_size: TERRAIN_TILE_SIZE,
                min_zoom: TERRAIN_MIN_ZOOM,
                max_source_zoom: TERRAIN_ZOOM,
                max_display_zoom: RASTER_BASEMAP_MAX_DISPLAY_ZOOM,
                initial_viewport: DefaultView {
                    lat: 0.0,
                    lon: 0.0,
                    zoom: 0.0,
                },
                levels: vec![TileLevelRecord {
                    zoom: 10,
                    boxes: vec![TileBoundsRecord {
                        x_min: 156,
                        x_max: 219,
                        y_tms_min: 636,
                        y_tms_max: 676,
                    }],
                }],
            },
        ];
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &static_raster_entries);
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let world = entries
            .iter()
            .find(|entry| entry["id"] == "world-basemap_WBM1")
            .expect("world basemap entry");
        assert_eq!(world["region_id"], "world");
        assert_eq!(world["map_view"]["chart_family"], "world-basemap");
        assert_eq!(world["map_view"]["max_source_zoom"], 4);
        assert_eq!(world["map_view"]["max_display_zoom"], 8.0);
        assert_eq!(world["map_view"]["tile_path_template"], "0/{z}/{x}/{y}.png");

        let shaded = entries
            .iter()
            .find(|entry| entry["id"] == "shaded-relief-nw_SHD1")
            .expect("shaded relief entry");

        assert_eq!(shaded["label"], "Northwest Shaded Relief");
        assert_eq!(shaded["map_view"]["chart_family"], "shaded-relief");
        assert_eq!(shaded["map_view"]["tile_url_root"], "tiles");
        assert_eq!(
            shaded["map_view"]["tile_path_template"],
            "0/{z}/{x}/{y}.webp"
        );
        assert_eq!(shaded["map_view"]["storage_kind"], "static_product");
        assert_eq!(
            shaded["map_view"]["max_zoom"],
            RASTER_BASEMAP_MAX_DISPLAY_ZOOM
        );
        assert_eq!(shaded["map_view"]["initial_viewport"]["lat"], 45.0);
        let levels = shaded["map_view"]["levels"]
            .as_array()
            .expect("levels should be an array");
        assert_eq!(levels.len(), 1);
        let z10 = levels
            .iter()
            .find(|level| level["zoom"] == 10)
            .expect("z10 level");
        assert_eq!(z10["boxes"][0]["x_min"], 156);
        assert_eq!(z10["boxes"][0]["x_max"], 219);
        assert_eq!(z10["boxes"][0]["y_tms_min"], 636);
        assert_eq!(z10["boxes"][0]["y_tms_max"], 676);
    }

    #[test]
    fn nav_kv_magvar_pairs_publish_source_metadata_and_grid() {
        let temp = tempdir().expect("tempdir");
        let cof_path = temp.path().join("WMM.COF");
        fs::write(
            &cof_path,
            "\
2025.0 WMM-TEST 01/01/2025
1 0 -29351.8 0.0 12.0 0.0
1 1 -1410.8 4545.4 9.7 -21.5
999999999999999999999999999999999999999999999999
",
        )
        .unwrap();
        let metadata_path = temp.path().join("wmm-source.json");
        fs::write(
            &metadata_path,
            serde_json::to_vec(&WmmFetchedSourceMetadata {
                source_url: "https://example.test/WMM2025COF.zip".to_string(),
                source_zip_sha256: "abc123".to_string(),
                source_fetched_at_utc: Some("2026-01-02T03:04:05Z".to_string()),
                model: "WMM-TEST".to_string(),
                model_epoch: 2025.0,
                model_effective_date: "2025-01-01".to_string(),
                coefficient_release_date: "01/01/2025".to_string(),
                valid_decimal_year_start: 2025.0,
                valid_decimal_year_end: 2030.0,
            })
            .unwrap(),
        )
        .unwrap();
        let pairs = build_nav_kv_magvar_pairs(&cof_path, &metadata_path, 2026.0).unwrap();
        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };
        assert_eq!(64_801, pairs.len());
        let source = pair_value("magvar/source");
        assert_eq!(source["source_url"], "https://example.test/WMM2025COF.zip");
        assert_eq!(source["source_zip_sha256"], "abc123");
        assert_eq!(source["source_fetched_at_utc"], "2026-01-02T03:04:05Z");
        assert_eq!(source["model"], "WMM-TEST");
        assert_eq!(source["model_effective_date"], "2025-01-01");
        assert_eq!(source["coefficient_release_date"], "01/01/2025");
        assert_eq!(source["computed_decimal_year"], 2026.0);
        let rnt = pair_value("magvar/47/-123");
        assert!(rnt.as_f64().unwrap().abs() > 1.0);
    }

    #[test]
    fn nav_kv_chart_catalog_emits_tile_path_templates_for_chart_packages() {
        let mut resource_index = minimal_resource_index();
        resource_index.chart_collections[0].detail_package_id =
            Some("NW_SEC_DETAIL_SEC1_2607".to_string());
        resource_index.chart_collections[0].detail_levels = vec![TileLevelRecord {
            zoom: 11,
            boxes: vec![TileBoundsRecord {
                x_min: 2,
                x_max: 4,
                y_tms_min: 6,
                y_tms_max: 8,
            }],
        }];
        let mut flyway = resource_index.chart_collections[0].clone();
        flyway.id = "flyway:nw".to_string();
        flyway.family_id = "flyway".to_string();
        flyway.package_id = "NW_TAC_TAC1_2607".to_string();
        flyway.detail_package_id = Some("NW_TAC_DETAIL_TAC1_2607".to_string());
        flyway.chart_index = 2;
        flyway.tile_path_template = "tiles/2/{z}/{x}/{y}.webp".to_string();
        resource_index.chart_collections.push(flyway);
        resource_index.families.push(ResourceFamily {
            id: "flyway".to_string(),
            display_name: "Flyway".to_string(),
            kind: "tiled_raster".to_string(),
        });
        let catalog = build_nav_kv_chart_catalog(&resource_index, &[]);
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let sectional = entries
            .iter()
            .find(|entry| entry["id"] == "sec:nw")
            .expect("sectional entry");

        assert_eq!(sectional["map_view"]["tile_url_root"], "tiles");
        assert_eq!(
            sectional["map_view"]["tile_path_template"],
            "0/{z}/{x}/{y}.webp"
        );
        assert_eq!(
            sectional["map_view"]["detail"]["package_name"],
            "NW_SEC_DETAIL_SEC1_2607"
        );
        assert_eq!(
            sectional["map_view"]["detail"]["tile_path_template"],
            "0/{z}/{x}/{y}.webp"
        );
        assert_eq!(sectional["map_view"]["detail"]["levels"][0]["zoom"], 11);
        let flyway = entries
            .iter()
            .find(|entry| entry["id"] == "flyway:nw")
            .expect("flyway entry");
        assert_eq!(flyway["map_view"]["package_name"], "NW_TAC_TAC1_2607");
        assert_eq!(
            flyway["map_view"]["tile_path_template"],
            "2/{z}/{x}/{y}.webp"
        );
    }

    #[test]
    fn nav_kv_chart_catalog_does_not_emit_polygon_set_coverage_for_chart_packages() {
        let cutline_root = tempdir().expect("tempdir");
        let sec_dir = cutline_root.path().join("SEC");
        fs::create_dir_all(&sec_dir).expect("create cutline dir");
        fs::write(
            sec_dir.join("Northwest SEC.geojson"),
            serde_json::json!({
                "type": "FeatureCollection",
                "features": [{
                    "type": "Feature",
                    "properties": {"location": "Northwest SEC.tif"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[
                            [-124.0, 41.0],
                            [-124.0, 49.0],
                            [-104.0, 49.0],
                            [-104.0, 41.0],
                            [-124.0, 41.0]
                        ]]
                    }
                }]
            })
            .to_string(),
        )
        .expect("write cutline");
        let polygon_sets =
            build_chart_cutline_polygon_sets(cutline_root.path(), &minimal_resource_index())
                .expect("polygon sets");
        let catalog = build_nav_kv_chart_catalog(&minimal_resource_index(), &[]);
        let entries = catalog
            .as_array()
            .expect("chart catalog should be an array");
        let sectional = entries
            .iter()
            .find(|entry| entry["id"] == "sec:nw")
            .expect("sectional entry");

        assert!(sectional.get("coverage").is_none());
        let polygon_set = polygon_sets
            .get("sec:nw")
            .expect("internal cutline polygon set remains available to offline regions");
        assert_eq!(polygon_set.id, "chart-coverage:sec:nw");
        assert_eq!(polygon_set.polygons.len(), 1);
        assert_eq!(polygon_set.polygons[0].points[0], [-124.0, 41.0]);
    }

    #[test]
    fn nav_kv_package_pairs_publish_bundle_package_rows() {
        let pairs = build_nav_kv_package_pairs(&[
            BundlePackageArtifact {
                id: "NW_SEC_SEC1_2604_01".to_string(),
                family_id: "sec".to_string(),
                contract_id: SEC_CONTRACT_ID.to_string(),
                region_id: Some("nw".to_string()),
                filename: "sec_nw_SEC1_2604_01_deadbeef.zip".to_string(),
                relative_path: "sec_nw_SEC1_2604_01_deadbeef.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "deadbeef".to_string(),
                size_bytes: 123,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                warning_text: None,
                metadata: package_metadata_with_contract_id(
                    chart_wide_angle_package_metadata(false, Some(1)),
                    SEC_CONTRACT_ID,
                ),
            },
            BundlePackageArtifact {
                id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2604_01"),
                family_id: "nav-db".to_string(),
                contract_id: NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: format!("nav_db_{NAV_DB_CONTRACT_ID}_2604_01_cafebabe.zip"),
                relative_path: format!("nav_db_{NAV_DB_CONTRACT_ID}_2604_01_cafebabe.zip"),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "cafebabe".to_string(),
                size_bytes: 456,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                warning_text: None,
                metadata: BTreeMap::from([(
                    "contract_id".to_string(),
                    serde_json::json!(NAV_DB_CONTRACT_ID),
                )]),
            },
        ])
        .unwrap();

        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let index = pair_value("package/index");
        assert_eq!(index.as_array().unwrap().len(), 2);
        assert_eq!(index[0]["id"], "NW_SEC_SEC1_2604_01");
        assert_eq!(index[0]["contract_id"], SEC_CONTRACT_ID);
        assert_eq!(index[0]["metadata"]["wide_angle_max_zoom"], 7);
        assert_eq!(index[0]["metadata"]["wide_angle_region_id"], "wide");
        assert_eq!(index[0]["metadata"]["min_source_zoom"], 8);
        assert_eq!(
            index[1]["id"],
            format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2604_01")
        );
        assert_eq!(index[1]["contract_id"], NAV_DB_CONTRACT_ID);

        let sectional = pair_value("package/by-id/NW_SEC_SEC1_2604_01");
        assert_eq!(sectional["contract_id"], SEC_CONTRACT_ID);
        assert_eq!(sectional["metadata"]["wide_angle_max_zoom"], 7);

        let nav_db = pair_value(&format!(
            "package/by-id/NAV_DB_{NAV_DB_CONTRACT_ID}_2604_01"
        ));
        assert_eq!(nav_db["family_id"], "nav-db");
        assert_eq!(nav_db["contract_id"], NAV_DB_CONTRACT_ID);
        assert_eq!(nav_db["region_id"], serde_json::Value::Null);
        assert_eq!(
            nav_db["relative_path"],
            format!("nav_db_{NAV_DB_CONTRACT_ID}_2604_01_cafebabe.zip")
        );
        assert_eq!(nav_db["size_bytes"], 456);
        assert_eq!(nav_db["checksum_sha256"], "cafebabe");
        assert_eq!(nav_db["cycle"], "2604");
        assert_eq!(nav_db["cycle_version"], "01");
        assert_eq!(nav_db["metadata"]["contract_id"], NAV_DB_CONTRACT_ID);
        let sec = pair_value("package/by-id/NW_SEC_SEC1_2604_01");
        assert_eq!(sec["metadata"]["wide_angle_region_id"], "wide");
        assert_eq!(sec["metadata"]["contract_id"], SEC_CONTRACT_ID);
    }

    #[test]
    fn nav_kv_procedure_geometry_pairs_split_reused_role_segments_preserving_runtime_fields() {
        let common = vec![test_procedure_geometry_bundle(
            "common-10",
            pgt::ProcedureSegmentRole::Common,
            "ENTRY",
            "FINAL",
            10,
        )];
        let first = test_procedure_geometry_record(
            "KAAA",
            "RNAV-A",
            Some("TRANS1"),
            vec![
                test_procedure_geometry_bundle(
                    "feeder-1",
                    pgt::ProcedureSegmentRole::EnrouteTransition,
                    "IF1",
                    "ENTRY",
                    1,
                ),
                common[0].clone(),
            ],
        );
        let second = test_procedure_geometry_record(
            "KAAA",
            "RNAV-A",
            Some("TRANS2"),
            vec![
                test_procedure_geometry_bundle(
                    "feeder-2",
                    pgt::ProcedureSegmentRole::EnrouteTransition,
                    "IF2",
                    "ENTRY",
                    1,
                ),
                common[0].clone(),
            ],
        );
        let originals = vec![first.clone(), second.clone()];

        let pairs = build_nav_kv_procedure_geometry_pairs(originals.clone()).unwrap();
        let segment_pairs = pairs
            .iter()
            .filter(|pair| pair.key.starts_with("procedure/geometry-segment/"))
            .collect::<Vec<_>>();
        assert_eq!(
            segment_pairs.len(),
            1,
            "the reused common segment should be emitted exactly once"
        );
        let mut segment_record: pgt::ProcedureGeometrySegmentRecord =
            serde_json::from_slice(&segment_pairs[0].value).unwrap();
        populate_test_segment_waypoints(&mut segment_record);
        let mut expected_common = common.clone();
        clear_omitted_procedure_geometry_fields(&mut expected_common);
        assert_eq!(segment_record.leg_bundles, expected_common);

        let segments = segment_pairs
            .iter()
            .map(|pair| {
                let segment_ref = pair
                    .key
                    .strip_prefix("procedure/geometry-segment/")
                    .unwrap()
                    .to_ascii_lowercase()
                    .to_string();
                let record: pgt::ProcedureGeometrySegmentRecord =
                    serde_json::from_slice(&pair.value).unwrap();
                (segment_ref, record)
            })
            .collect::<BTreeMap<_, _>>();

        for mut original in originals {
            clear_omitted_procedure_geometry_fields(&mut original.leg_bundles);
            let pair = pairs
                .iter()
                .find(|pair| pair.key == pgt::procedure_geometry_navdb_key(&original.key))
                .expect("split geometry pair");
            let mut split: pgt::ProcedureGeometryRecord =
                serde_json::from_slice(&pair.value).unwrap();
            split.key = original.key.clone();
            split.leg_bundles = reassemble_test_geometry_components(&split.components, &segments);
            split.components.clear();
            pgt::populate_derived_procedure_geometry_fields(&mut split);
            assert_eq!(split, original);
        }
    }

    #[test]
    fn nav_kv_plate_assets_receive_aggregate_procedure_geometry_warnings() {
        let plate_id = "plate:KAAA:IAP-AA-VOR RWY 01.png";
        let mut pairs = vec![
            nav_db::json_pair(
                "plate/cifp/KAAA/V01".to_string(),
                &serde_json::json!([{
                    "airport_id": "KAAA",
                    "cifp_id": "V01",
                    "plate_id": plate_id,
                    "is_primary": 1
                }]),
                "test plate match",
            )
            .unwrap(),
            nav_db::json_pair(
                format!("plate/by-id/{}", had_key_component(plate_id)),
                &serde_json::json!({"id": plate_id, "label": "VOR 01"}),
                "test plate",
            )
            .unwrap(),
            nav_db::json_pair(
                "procedure/geometry/KAAA/APPROACH/V01/_/TRANS".to_string(),
                &serde_json::json!({
                    "data_quality": [
                        {"message": "first warning"},
                        {"message": "second warning"}
                    ]
                }),
                "test geometry",
            )
            .unwrap(),
            nav_db::json_pair(
                "procedure/geometry/KAAA/APPROACH/V01/RWY/_".to_string(),
                &serde_json::json!({
                    "data_quality": [
                        {"message": "runway warning"}
                    ]
                }),
                "test runway geometry",
            )
            .unwrap(),
        ];

        nav_db::attach_procedure_geometry_warnings_to_plate_pairs(&mut pairs).unwrap();

        let plate: serde_json::Value = serde_json::from_slice(
            &pairs
                .iter()
                .find(|pair| pair.key.starts_with("plate/by-id/"))
                .unwrap()
                .value,
        )
        .unwrap();
        assert_eq!(plate["procedure_geometry_warning_count"], 2);
        assert_eq!(
            plate["procedure_geometry_warnings"],
            serde_json::json!([
                {
                    "airport_id": "KAAA",
                    "procedure_id": "V01",
                    "runway_transition": null,
                    "enroute_transition": "TRANS",
                    "messages": ["first warning", "second warning"]
                },
                {
                    "airport_id": "KAAA",
                    "procedure_id": "V01",
                    "runway_transition": "RWY",
                    "enroute_transition": null,
                    "messages": ["runway warning"]
                }
            ]),
        );
    }

    fn clear_omitted_procedure_geometry_fields(
        leg_bundles: &mut [pgt::ProcedureGeometryLegBundle],
    ) {
        for bundle in leg_bundles {
            bundle.id.clear();
            bundle.leg_sequence = 0;
            bundle.source_row_sequences.clear();
        }
    }

    fn test_procedure_geometry_record(
        airport_id: &str,
        procedure_id: &str,
        enroute_transition: Option<&str>,
        leg_bundles: Vec<pgt::ProcedureGeometryLegBundle>,
    ) -> pgt::ProcedureGeometryRecord {
        pgt::ProcedureGeometryRecord {
            key: pgt::ProcedureGeometryKey {
                airport_id: airport_id.to_string(),
                procedure_id: procedure_id.to_string(),
                kind: pgt::ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition: enroute_transition.map(str::to_string),
            },
            terminal_discontinuity: None,
            components: Vec::new(),
            leg_bundles,
            data_quality: Vec::new(),
        }
    }

    fn test_procedure_geometry_bundle(
        id: &str,
        role: pgt::ProcedureSegmentRole,
        from: &str,
        to: &str,
        leg_sequence: i32,
    ) -> pgt::ProcedureGeometryLegBundle {
        pgt::ProcedureGeometryLegBundle {
            id: id.to_string(),
            role,
            from: pgt::ProcedureNavRef::Fix(from.to_string()),
            to: pgt::ProcedureNavRef::Fix(to.to_string()),
            path_termination: pgt::ProcedurePathTermination::TrackToFix,
            leg_sequence,
            path: pgt::ProcedureGeometryPath {
                style: pgt::ProcedureGeometryPathStyle::Solid,
                elements: Vec::new(),
                effective_terminal_course_deg: None,
            },
            waypoints: vec![pgt::ProcedureGeometryWaypoint {
                nav_ref: pgt::ProcedureNavRef::Fix(to.to_string()),
                name: None,
            }],
            sequencing_after: pgt::ProcedureSequencingRule::Continue,
            discontinuity_after: None,
            source_row_sequences: vec![leg_sequence],
        }
    }

    fn reassemble_test_geometry_components(
        components: &[pgt::ProcedureGeometryComponent],
        segments: &BTreeMap<String, pgt::ProcedureGeometrySegmentRecord>,
    ) -> Vec<pgt::ProcedureGeometryLegBundle> {
        let mut leg_bundles = Vec::new();
        for component in components {
            match component {
                pgt::ProcedureGeometryComponent::LegBundles {
                    leg_bundles: inline,
                } => leg_bundles.extend(inline.clone()),
                pgt::ProcedureGeometryComponent::SegmentRef { segment_ref } => {
                    leg_bundles.extend(
                        segments
                            .get(segment_ref)
                            .unwrap_or_else(|| panic!("missing segment {segment_ref}"))
                            .leg_bundles
                            .clone(),
                    );
                }
            }
        }
        leg_bundles
    }

    fn populate_test_segment_waypoints(segment: &mut pgt::ProcedureGeometrySegmentRecord) {
        let mut record = pgt::ProcedureGeometryRecord {
            key: pgt::ProcedureGeometryKey::default(),
            terminal_discontinuity: None,
            components: Vec::new(),
            leg_bundles: std::mem::take(&mut segment.leg_bundles),
            data_quality: Vec::new(),
        };
        pgt::populate_derived_procedure_geometry_fields(&mut record);
        segment.leg_bundles = record.leg_bundles;
    }

    #[test]
    fn nav_kv_vector_pairs_load_jsonl_into_vector_keyspace() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("vectors.had-pairs.jsonl");
        fs::write(
            &path,
            r#"{"key":"vector/manifest","value_json":"{\"schema_version\":1}"}"#,
        )
        .unwrap();
        let pairs = build_nav_kv_vector_pairs(&path).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].key, "vector/manifest");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&pairs[0].value).unwrap()["schema_version"],
            1
        );
    }

    #[test]
    fn nav_kv_package_inputs_include_resource_index_chart_metadata() {
        let mut resource_index = minimal_resource_index();
        resource_index.packages[0].id = "NW_SEC_SEC1_2604_01".to_string();
        resource_index.packages[0].artifact_path = "products/sec_nw_2604.zip".to_string();
        resource_index.packages[0].size_bytes = 123;
        resource_index.packages[0].checksum_sha256 = "deadbeef".to_string();
        resource_index.packages[0].cycle_code = Some("2604".to_string());
        resource_index.packages[0].version_label = Some("01".to_string());
        resource_index.packages[0].metadata = chart_wide_angle_package_metadata(false, Some(1));

        let artifacts = bundle_package_artifacts_from_resource_index(&resource_index)
            .expect("resource index packages should convert");
        let pairs = build_nav_kv_package_pairs(&artifacts).expect("package pairs");
        let pair = pairs
            .iter()
            .find(|pair| pair.key == "package/by-id/NW_SEC_SEC1_2604_01")
            .expect("sectional package by-id row");
        let value: serde_json::Value = serde_json::from_slice(&pair.value).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(value["metadata"]["wide_angle_max_zoom"], 7);
        assert_eq!(value["metadata"]["min_source_zoom"], 8);
        assert_eq!(value["contract_id"], SEC_CONTRACT_ID);
        assert_eq!(value["relative_path"], "sec_nw_SEC1_2604_01_deadbeef.zip");
        assert_eq!(value["size_bytes"], 123);
    }

    #[test]
    fn nav_kv_resource_summary_pairs_publish_family_region_and_temporal_tables() {
        let pairs = build_nav_kv_resource_summary_pairs(&minimal_resource_index()).unwrap();

        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let families = pair_value("resource/families");
        assert!(families
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value["id"] == "sec"));
        let ifr_high = families
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["id"] == "enr-h")
            .expect("ifr high family");
        assert!(ifr_high.get("warning_text").is_none());

        let regions = pair_value("resource/regions");
        assert_eq!(regions.as_array().unwrap()[0]["id"], "nw");

        let temporal = pair_value("resource/temporal-summary");
        assert_eq!(temporal["uniform_cycle_code"], serde_json::Value::Null);
    }

    #[test]
    fn nav_kv_waypoint_search_pairs_omit_overlarge_suggestion_lists() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE airports (
                    LocationID TEXT,
                    City TEXT,
                    State TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE enroute_navaids (
                    identifier TEXT PRIMARY KEY,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                ",
            )
            .unwrap();
        for index in 0..101 {
            connection
                .execute(
                    "INSERT INTO airports VALUES (?1, 'CITY', 'ST', 'FIELD', 47.0, -122.0)",
                    [format!("K{index:03}")],
                )
                .unwrap();
        }
        for ident in ["KRNT", "KRDD"] {
            connection
                .execute(
                    "INSERT INTO airports VALUES (?1, 'CITY', 'ST', 'FIELD', 47.0, -122.0)",
                    [ident],
                )
                .unwrap();
        }

        let pairs = build_nav_kv_waypoint_lookup_pairs(&connection).unwrap();
        assert!(pairs
            .iter()
            .all(|pair| !pair.key.starts_with("waypoint/prefix/")));
        assert!(pairs
            .iter()
            .all(|pair| pair.key != "waypoint/search-prefix/K"));
        let kr = pairs
            .iter()
            .find(|pair| pair.key == "waypoint/search-prefix/KR")
            .expect("KR prefix should remain below threshold");
        let suggestions = serde_json::from_slice::<Vec<WaypointSearchRecord>>(&kr.value).unwrap();
        assert_eq!(suggestions.len(), 2);
        assert!(suggestions
            .iter()
            .all(|suggestion| suggestion.match_kind == WaypointSearchMatchKind::Identifier));
        assert!(
            pairs
                .iter()
                .all(|pair| pair.key != "waypoint/search-prefix/KRNT"),
            "longer prefixes are redundant when a shorter emitted bucket can be filtered"
        );
    }

    #[test]
    fn nav_kv_waypoint_search_values_are_slim_and_use_the_enroute_domain() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE airports (
                    LocationID TEXT,
                    City TEXT,
                    State TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE enroute_navaids (
                    identifier TEXT PRIMARY KEY,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                INSERT INTO enroute_navaids VALUES ('SEA', 47.43538888888889, -122.30961111111111, 'VORTAC', 'Seattle 116.80', 15.0);
                ",
            )
            .unwrap();

        let pairs = build_nav_kv_waypoint_lookup_pairs(&connection).unwrap();
        let identifier_sea = pairs
            .iter()
            .find(|pair| pair.key == "waypoint/identifier/SEA")
            .expect("SEA identifier");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&identifier_sea.value).unwrap(),
            serde_json::json!({ "Navaid": "SEA" })
        );
        assert!(pairs
            .iter()
            .all(|pair| pair.key != "waypoint/identifier/RNT"));

        let prefix_s = pairs
            .iter()
            .find(|pair| pair.key == "waypoint/search-prefix/S")
            .expect("S prefix");
        let suggestions = serde_json::from_slice::<Vec<WaypointSearchRecord>>(&prefix_s.value)
            .expect("search records");
        assert_eq!(
            suggestions,
            vec![WaypointSearchRecord {
                identifier: "SEA".to_string(),
                kind: "navaid".to_string(),
                display_name: "Seattle 116.80".to_string(),
                lat: 47.4353889,
                lon: -122.3096111,
                matched_term: "SEA".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            }]
        );
    }

    #[test]
    fn nav_kv_airway_pairs_preserve_empty_branch_keys() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE airports (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    MagneticVariation TEXT,
                    ARPElevation TEXT
                );
                CREATE TABLE enroute_navaids (
                    identifier TEXT PRIMARY KEY,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL
                );
                CREATE TABLE procedure_navaids (
                    identifier TEXT,
                    icao_code TEXT,
                    section_code TEXT,
                    subsection_code TEXT,
                    airport_id TEXT,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL,
                    elevation TEXT,
                    PRIMARY KEY(identifier, icao_code, section_code, subsection_code, airport_id)
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE fix_usage (
                    LocationID TEXT,
                    Usage TEXT
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    LEIdent TEXT,
                    LELatitude REAL,
                    LELongitude REAL,
                    HEIdent TEXT,
                    HELatitude REAL,
                    HELongitude REAL
                );
                CREATE TABLE cifp_sid_star_app (
                    airport_identifier TEXT,
                    fix_identifier TEXT,
                    icao_code_2 TEXT,
                    section_code_2 TEXT,
                    subsection_code_2 TEXT,
                    recommended_navaid TEXT,
                    icao_code_3 TEXT,
                    recd_nav_section TEXT,
                    recd_nav_subsection TEXT
                );
                CREATE TABLE airways_branch (
                    name TEXT,
                    branch_key TEXT,
                    sequence_number INTEGER,
                    sequence_token TEXT,
                    point_name TEXT,
                    Latitude REAL,
                    Longitude REAL
                );
                INSERT INTO fix VALUES ('RAWER', 45.235644444444446, -122.79431666666666);
                INSERT INTO fix VALUES ('CANBY', 45.31056944444444, -122.76489166666667);
                INSERT INTO fix VALUES ('HARPR', 42.480555555555554, -122.88376111111111);
                INSERT INTO fix VALUES ('PDT', 45.0, -118.0);
                INSERT INTO enroute_navaids VALUES ('PDT', 45.0, -118.0, 'VORTAC', 'PENDLETON', 14.0);
                INSERT INTO enroute_navaids VALUES ('ILA', 39.0711736111111, -122.027269722222, 'VORTAC', 'WILLIAMS', 14.0);
                INSERT INTO enroute_navaids VALUES ('OAK', 37.7259255555556, -122.223591944444, 'VORTAC', 'OAKLAND', 14.0);
                -- BYI's longitude exposed inconsistent keys when only stored positions were rounded.
                INSERT INTO enroute_navaids VALUES ('BYI', 42.58023944444444453, -113.86585749999998995, 'VOR/DME', 'BURLEY', 14.0);
                INSERT INTO airways_branch VALUES ('V23', '', 690, '690', 'RAWER', 45.235644444444446, -122.79431666666666);
                INSERT INTO airways_branch VALUES ('V23', '', 700, '700', 'CANBY', 45.31056944444444, -122.76489166666667);
                INSERT INTO airways_branch VALUES ('V23', '', 710, '710', 'NAMEDBUTMISSING', 45.4, -122.7);
                INSERT INTO airways_branch VALUES ('V23', '', 720, '720', '', 45.5, -122.6);
                INSERT INTO airways_branch VALUES ('V4', '', 10, '10', 'PENDLETON', 45.0, -118.0);
                INSERT INTO airways_branch VALUES ('V4', '', 20, '20', 'CORDO', 45.1, -117.9);
                INSERT INTO airways_branch VALUES ('V4', '', 30, '30', 'BURLEY', 42.58023944444444453, -113.86585749999998995);
                INSERT INTO airways_branch VALUES ('Q801', 'A', 10, '10', 'HARPR', 42.480555555555554, -122.88376111111111);
                INSERT INTO airways_branch VALUES ('V195', 'RAGGS-JINGO', 220, '220', 'OAKLAND', 37.7259255555556, -122.223591944444);
                INSERT INTO airways_branch VALUES ('V195', 'RAGGS-JINGO', 300, '300', 'WILLIAMS', 39.0711736111111, -122.027269722222);
                ",
            )
            .unwrap();

        let pairs = build_nav_kv_airway_pairs(&connection).unwrap();
        let mut expected_colocated_navaids = BTreeMap::new();
        {
            let mut stmt = connection
                .prepare(
                    "
                    SELECT trim(a.name), trim(a.branch_key),
                           CAST(a.sequence_number AS INTEGER), trim(n.identifier)
                    FROM airways_branch a
                    JOIN enroute_navaids n
                      ON CAST(a.Latitude AS REAL) = CAST(n.latitude AS REAL)
                     AND CAST(a.Longitude AS REAL) = CAST(n.longitude AS REAL)
                    ",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i32>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .unwrap();
            for row in rows {
                let (airway, branch, sequence, navaid) = row.unwrap();
                expected_colocated_navaids.insert((airway, branch, sequence), navaid);
            }
        }

        let mut misclassified_colocated_navaids = Vec::new();
        for pair in &pairs {
            let Some(airway_name) = pair.key.strip_prefix("airway/") else {
                continue;
            };
            if airway_name.contains('/') {
                continue;
            }
            let branches: serde_json::Value = serde_json::from_slice(&pair.value).unwrap();
            for branch in branches.as_array().unwrap() {
                let branch_key = branch["branch_key"].as_str().unwrap();
                for point in branch["points"].as_array().unwrap() {
                    let sequence = point["sequence"].as_i64().unwrap() as i32;
                    let Some(expected_navaid) = expected_colocated_navaids.get(&(
                        airway_name.to_string(),
                        branch_key.to_string(),
                        sequence,
                    )) else {
                        continue;
                    };
                    if point["nav_ref"] != serde_json::json!({ "Navaid": expected_navaid }) {
                        misclassified_colocated_navaids.push(format!(
                            "{airway_name}/{branch_key}/{sequence}: expected Navaid({expected_navaid}), got {}",
                            point["nav_ref"]
                        ));
                    }
                }
            }
        }
        assert_eq!(
            misclassified_colocated_navaids,
            Vec::<String>::new(),
            "airway points colocated with known navaids must retain navaid identity"
        );

        let pair_value = |key: &str| -> serde_json::Value {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing nav_kv pair {key}"));
            serde_json::from_slice(&pair.value).unwrap()
        };

        let v23 = pair_value("airway/V23");
        assert_eq!(v23[0]["branch_key"], "");
        assert_eq!(
            v23[0]["points"][0]["nav_ref"],
            serde_json::json!({ "Fix": "RAWER" })
        );
        assert_eq!(
            v23[0]["points"][1]["nav_ref"],
            serde_json::json!({ "Fix": "CANBY" })
        );
        assert_eq!(
            v23[0]["points"][2]["nav_ref"],
            serde_json::json!({ "LatLon": { "lat": 45.4, "lon": -122.7 } })
        );
        assert_eq!(
            v23[0]["points"][3]["nav_ref"],
            serde_json::json!({ "LatLon": { "lat": 45.5, "lon": -122.6 } })
        );

        let q801 = pair_value("airway/Q801");
        assert_eq!(q801[0]["branch_key"], "A");

        let v4 = pair_value("airway/V4");
        assert_eq!(
            v4[0]["points"][0]["nav_ref"],
            serde_json::json!({ "Navaid": "PDT" })
        );

        let v195 = pair_value("airway/V195");
        assert_eq!(
            v195[0]["points"][0]["nav_ref"],
            serde_json::json!({ "Navaid": "OAK" })
        );
        assert_eq!(
            v195[0]["points"][1]["nav_ref"],
            serde_json::json!({ "Navaid": "ILA" })
        );

        let rawer_tile = pair_value("airway/spatial/45/-123");
        assert!(rawer_tile
            .as_array()
            .unwrap()
            .iter()
            .any(|point| point["airway_name"] == "V23" && point["branch_key"] == ""));
    }

    #[test]
    fn nav_kv_airway_navref_validation_rejects_missing_position_keys() {
        let mut pairs = BTreeMap::new();
        pairs.insert(
            "airway/V195".to_string(),
            serde_json::to_vec(&serde_json::json!([{
                "branch_key": "",
                "points": [{
                    "nav_ref": { "Navaid": "ILA" }
                }]
            }]))
            .unwrap(),
        );

        let error = validate_airway_navrefs_resolve(&pairs).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing navref/position/navaid/ILA"));

        pairs.insert(
            "navref/position/navaid/ILA".to_string(),
            serde_json::to_vec(&serde_json::json!({ "lat": 39.0, "lon": -122.0 })).unwrap(),
        );
        validate_airway_navrefs_resolve(&pairs).unwrap();
    }

    #[test]
    fn nav_kv_procedure_rows_resolve_arinc_navaids_with_blank_subsection() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE airports (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    MagneticVariation TEXT,
                    ARPElevation TEXT
                );
                CREATE TABLE enroute_navaids (
                    identifier TEXT PRIMARY KEY,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL
                );
                CREATE TABLE procedure_navaids (
                    identifier TEXT,
                    icao_code TEXT,
                    section_code TEXT,
                    subsection_code TEXT,
                    airport_id TEXT,
                    latitude REAL,
                    longitude REAL,
                    kind TEXT,
                    facility_name TEXT,
                    variation REAL,
                    elevation TEXT,
                    PRIMARY KEY(identifier, icao_code, section_code, subsection_code, airport_id)
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    LEIdent TEXT,
                    LELatitude REAL,
                    LELongitude REAL,
                    HEIdent TEXT,
                    HELatitude REAL,
                    HELongitude REAL
                );
                CREATE TABLE cifp_sid_star_app (
                    airport_identifier TEXT,
                    sid_star_approach_identifier TEXT,
                    subsection_code TEXT,
                    route_type TEXT,
                    transition_identifier TEXT,
                    sequence_number TEXT,
                    fix_identifier TEXT,
                    icao_code_2 TEXT,
                    section_code_2 TEXT,
                    subsection_code_2 TEXT,
                    recommended_navaid TEXT,
                    icao_code_3 TEXT,
                    recd_nav_section TEXT,
                    recd_nav_subsection TEXT,
                    altitude_1 TEXT,
                    altitude_2 TEXT,
                    path_and_termination TEXT,
                    turn_direction TEXT,
                    theta TEXT,
                    magnetic_course TEXT,
                    route_distance_holding_distance_or_time TEXT
                );
                INSERT INTO airports VALUES ('44C', 42.4978, -88.9676, 'W0030', '823');
                INSERT INTO airports VALUES ('KPAE', 47.9063, -122.2816, 'E0150', '606');
                INSERT INTO airportrunways VALUES ('KPAE', '16L', 47.9218, -122.2855, '34R', 47.8908, -122.2768);
                INSERT INTO procedure_navaids VALUES ('JVL', 'K5', 'D', '', '', 42.5580083333333, -89.1052583333333, 'NAVAID', 'JANESVILLE', 3.0, '');
                INSERT INTO fix VALUES ('MADMY', 42.5000, -89.0000);
                INSERT INTO cifp_sid_star_app VALUES ('44C', 'VOR-A', 'F', 'A', 'JVL', '010', 'JVL', 'K5', 'D', '', '', '', '', '', '', '', 'IF', '', '', '', '');
                INSERT INTO cifp_sid_star_app VALUES ('44C', 'VOR-A', 'F', 'A', 'JVL', '020', 'JVL', 'K5', 'D', '', 'JVL', 'K5', 'D', '', '', '', 'PI', '', '', '', '');
                INSERT INTO cifp_sid_star_app VALUES ('KPAE', 'PAINE6', 'D', 'T', 'RW16L', '010', '', '', '', '', '', '', '', '', '', '', 'VM', '', '', '1636', '');
                ",
            )
            .unwrap();

        let mut sid_lists = BTreeMap::new();
        let mut star_lists = BTreeMap::new();
        let mut distinct_by_procedure = BTreeMap::new();
        let mut materialization_by_procedure = BTreeMap::new();
        load_nav_kv_procedure_rows(
            &connection,
            &mut sid_lists,
            &mut star_lists,
            &mut distinct_by_procedure,
            &mut materialization_by_procedure,
        )
        .unwrap();

        let rows = materialization_by_procedure
            .get(&("44C".to_string(), "VOR-A".to_string()))
            .expect("44C VOR-A materialization rows");
        assert_eq!(
            rows[0]["nav_ref"],
            serde_json::json!({
                "ArincNavaid": {
                    "identifier": "JVL",
                    "icao_code": "K5",
                    "section_code": "D",
                    "subsection_code": ""
                }
            })
        );
        assert_eq!(
            rows[0]["nav_position"],
            serde_json::json!({ "lat": 42.5580083, "lon": -89.1052583 })
        );
        assert_eq!(
            rows[1]["defining_nav_ref"],
            serde_json::json!({
                "ArincNavaid": {
                    "identifier": "JVL",
                    "icao_code": "K5",
                    "section_code": "D",
                    "subsection_code": ""
                }
            })
        );

        let paine = materialization_by_procedure
            .get(&("KPAE".to_string(), "PAINE6".to_string()))
            .expect("KPAE PAINE6 materialization rows");
        assert_eq!(
            paine[0]["departure_anchor_ref"],
            serde_json::json!({ "Fix": "RW16L" })
        );
        assert_eq!(
            paine[0]["departure_anchor_position"],
            serde_json::json!({ "lat": 47.9218, "lon": -122.2855 })
        );
    }

    #[test]
    fn build_status_html_includes_cycle_products() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let cycle_bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2604_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2604".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-04-16T00:00:00Z".to_string(),
            effective_date: "2026-04-16".to_string(),
            expiration_date: "2026-05-14".to_string(),
            start_valid: "2026-04-16".to_string(),
            end_valid: "2026-05-14".to_string(),
            packages: vec![BundlePackageArtifact {
                id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2604_01"),
                family_id: "nav-db".to_string(),
                contract_id: NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: format!("nav_db_{NAV_DB_CONTRACT_ID}_2604_01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.zip"),
                relative_path: format!("nav_db_{NAV_DB_CONTRACT_ID}_2604_01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.zip"),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "a".repeat(64),
                size_bytes: 1234,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: Some("2026-04-15T23:00:00Z".to_string()),
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                warning_text: None,
                metadata: BTreeMap::from([(
                    "procedure_geometry_warning_count".to_string(),
                    serde_json::json!(2),
                )]),
            }],
            ancillary: vec![],
        };
        fs::write(
            root.join("bundle_cycle_2604_01_test.json"),
            serde_json::to_vec_pretty(&cycle_bundle).unwrap(),
        )
        .unwrap();
        let current = CurrentArtifactsManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            contracts: test_contracts(&[("nav-db", NAV_DB_CONTRACT_ID)]),
            artifact_roots: CurrentArtifactRoots {
                packaged: "packaged/".to_string(),
                unpacked: "unpacked/".to_string(),
            },
            as_of_date: "2026-05-03".to_string(),
            as_of_utc: "2026-05-03T18:01:00Z".to_string(),
            bundles: vec![CurrentBundleEntry {
                filename: "bundle_cycle_2604_01_test.json".to_string(),
                relative_path: "bundle_cycle_2604_01_test.json".to_string(),
                id: "cycle_2604_01".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: "2604".to_string(),
                cycle_version: "01".to_string(),
                start_valid: "2026-04-16".to_string(),
                end_valid: "2026-05-14".to_string(),
                checksum_sha256: "c".repeat(64),
                size_bytes: 1,
            }],
            startup_prefetch: None,
            diagnostics: Some(CurrentDiagnosticsEntry {
                filename: "build_errors_20260503.json".to_string(),
                error_count: 1,
            }),
        };
        let current_path = root.join("product_artifacts.json");
        fs::write(&current_path, serde_json::to_vec_pretty(&current).unwrap()).unwrap();
        fs::write(
            root.join("build_errors_20260503.json"),
            serde_json::to_vec_pretty(&BuildDiagnosticsManifest {
                schema_version: 1,
                generated_at_utc: "2026-05-03T18:02:00Z".to_string(),
                error_count: 1,
                errors: vec![
                    BuildDiagnosticEntry {
                        product: "vectors".to_string(),
                        cycle: Some("2604".to_string()),
                        severity: "ERROR".to_string(),
                        code: "saa_feature_count_mismatch".to_string(),
                        message: "SAA count mismatch".to_string(),
                        expected: Some(2),
                        actual: Some(1),
                    },
                    BuildDiagnosticEntry {
                        product: "vectors".to_string(),
                        cycle: Some("2604".to_string()),
                        severity: "WARNING".to_string(),
                        code: "vector_warning".to_string(),
                        message: "vector warning".to_string(),
                        expected: None,
                        actual: None,
                    },
                ],
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(root.join("bundle_cycle_stale_empty.json"), []).unwrap();

        let status = build_status_document(root, &current_path).unwrap();
        assert_eq!(status.products.len(), 1);
        assert_eq!(status.warnings.len(), 1);
        assert_eq!(status.warnings[0].code, "invalid_public_bundle_manifest");
        assert!(status
            .products
            .iter()
            .any(|product| product.bundle_type == "cycle" && product.family_id == "nav-db"));
        let html = render_build_status_html(&status).unwrap();
        assert!(html.contains("Aerobag Build Status"));
        assert!(html.contains("bundle_cycle_stale_empty.json"));
        assert!(html.contains("nav_db_"));

        let facts = product_facts_document(root, &current_path, "2026-05-03T18:03:00Z").unwrap();
        assert_eq!(facts.schema_version, 1);
        assert_eq!(facts.build.status, "pass");
        assert_eq!(facts.products.len(), 1);
        assert_eq!(
            facts.products[0].product_id,
            format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2604_01")
        );
        assert_eq!(facts.products[0].family, "nav-db");
        assert_eq!(
            facts.products[0].source_fetched_at_utc.as_deref(),
            Some("2026-04-15T23:00:00Z")
        );
        assert_eq!(facts.products[0].error_count, 1);
        assert_eq!(facts.products[0].warning_count, 3);
        assert_eq!(
            facts.products[0]
                .diagnostics
                .procedure_geometry_warning_count,
            2
        );
        assert_eq!(
            facts.products[0].diagnostics.vector_validator_error_count,
            1
        );
        assert_eq!(
            facts.products[0].diagnostics.vector_validator_warning_count,
            1
        );
    }

    #[test]
    fn product_artifacts_manifest_lives_in_publish_dir_and_names_roots() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("master")
            .join("20260504T010203Z");
        let packaged_root = publish_dir.join("packaged");
        fs::create_dir_all(&packaged_root).unwrap();

        let current_path = write_current_artifacts_manifest(
            &packaged_root,
            Utc.with_ymd_and_hms(2026, 5, 4, 1, 2, 3).unwrap(),
            None,
        )
        .unwrap();

        assert_eq!(current_path, publish_dir.join("product_artifacts.json"));
        assert!(publish_dir.join("product_artifacts.json").is_file());
        assert!(!packaged_root.join("current_artifacts.json").exists());
        fs::write(packaged_root.join("current_artifacts.json"), "{}").unwrap();
        cleanup_published_packaged_root(&packaged_root, &current_path).unwrap();
        assert!(!packaged_root.join("current_artifacts.json").exists());

        let current = load_current_artifacts_manifest(&current_path).unwrap();
        assert_eq!(
            current.artifact_roots.packaged,
            "master/20260504T010203Z/packaged/"
        );
        assert_eq!(
            current.artifact_roots.unpacked,
            "master/20260504T010203Z/unpacked/"
        );
        assert!(current.bundles.is_empty());
    }

    #[test]
    fn current_artifacts_manifest_embeds_nav_db_startup_prefetch_urls() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("master")
            .join("20260531T010203Z");
        let root = publish_dir.join("packaged");
        fs::create_dir_all(&root).unwrap();
        let nav_db_sha = sha256_hex(&[]);
        let nav_db_filename = format!("nav_db_{NAV_DB_CONTRACT_ID}_2605_01_{nav_db_sha}.zip");
        let bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2605_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2605".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-14T00:00:00Z".to_string(),
            effective_date: "2026-05-14".to_string(),
            expiration_date: "2026-06-11".to_string(),
            start_valid: "2026-05-14".to_string(),
            end_valid: "2026-06-11".to_string(),
            packages: vec![BundlePackageArtifact {
                id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2605_01"),
                family_id: "nav-db".to_string(),
                contract_id: NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: nav_db_filename.clone(),
                relative_path: nav_db_filename.clone(),
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: nav_db_sha,
                size_bytes: 123,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-05-14".to_string()),
                expiration_date: Some("2026-06-11".to_string()),
                warning_text: None,
                metadata: BTreeMap::from([(
                    NAV_DB_STARTUP_PREFETCH_MEMBERS_METADATA_KEY.to_string(),
                    serde_json::json!(["root", "page_0046", "page_0570"]),
                )]),
            }],
            ancillary: vec![],
        };
        write_hashed_bundle_manifest(&root, &bundle).unwrap();

        let current_path = write_current_artifacts_manifest(
            &root,
            Utc.with_ymd_and_hms(2026, 5, 31, 1, 2, 3).unwrap(),
            None,
        )
        .unwrap();
        let current = load_current_artifacts_manifest(&current_path).unwrap();
        let cycle_resources = current.startup_prefetch.unwrap().cycle_resources;
        assert_eq!(cycle_resources.len(), 1);
        assert_eq!(cycle_resources[0].bundle_id, "cycle_2605_01");
        assert_eq!(cycle_resources[0].cycle, "2605");
        assert_eq!(cycle_resources[0].cycle_version, "01");
        assert_eq!(cycle_resources[0].start_valid, "2026-05-14");
        assert_eq!(cycle_resources[0].end_valid, "2026-06-11");
        assert_eq!(
            cycle_resources[0]
                .resources
                .iter()
                .map(|resource| resource.url.clone())
                .collect::<Vec<_>>(),
            vec![
                format!(
                    "master/20260531T010203Z/unpacked/{}/root",
                    zip_stem(&nav_db_filename).unwrap()
                ),
                format!(
                    "master/20260531T010203Z/unpacked/{}/page_0046",
                    zip_stem(&nav_db_filename).unwrap()
                ),
                format!(
                    "master/20260531T010203Z/unpacked/{}/page_0570",
                    zip_stem(&nav_db_filename).unwrap()
                ),
            ]
        );
    }

    #[test]
    fn merge_current_artifacts_writes_list_alias() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("master")
            .join("20260514T000000Z");
        let packaged_root = publish_dir.join("packaged");
        fs::create_dir_all(&packaged_root).unwrap();
        let nav_db_sha = sha256_hex(&[]);
        let nav_db_filename = format!("nav_db_{NAV_DB_CONTRACT_ID}_2605_01_{nav_db_sha}.zip");
        fs::write(packaged_root.join(&nav_db_filename), []).unwrap();
        let bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2605_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2605".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-14T00:00:00Z".to_string(),
            effective_date: "2026-05-14".to_string(),
            expiration_date: "2026-06-11".to_string(),
            start_valid: "2026-05-14".to_string(),
            end_valid: "2026-06-11".to_string(),
            packages: vec![BundlePackageArtifact {
                id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2605_01"),
                family_id: "nav-db".to_string(),
                contract_id: NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: nav_db_filename.clone(),
                relative_path: nav_db_filename,
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: nav_db_sha,
                size_bytes: 0,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-05-14".to_string()),
                expiration_date: Some("2026-06-11".to_string()),
                warning_text: None,
                metadata: BTreeMap::new(),
            }],
            ancillary: vec![],
        };
        let bundle_path = write_hashed_bundle_manifest(&packaged_root, &bundle).unwrap();
        let product_manifest = CurrentArtifactsManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            contracts: test_contracts(&[("nav-db", NAV_DB_CONTRACT_ID)]),
            artifact_roots: current_artifact_roots_for_packaged_root(&packaged_root).unwrap(),
            as_of_date: "2026-05-14".to_string(),
            as_of_utc: "2026-05-14T00:00:00Z".to_string(),
            bundles: vec![current_bundle_entry_from_path(&bundle_path).unwrap()],
            startup_prefetch: None,
            diagnostics: None,
        };
        let product_path = publish_dir.join("product_artifacts.json");
        fs::write(
            &product_path,
            serde_json::to_vec_pretty(&product_manifest).unwrap(),
        )
        .unwrap();

        let current_path = merge_current_artifacts_manifests(
            temp.path(),
            Utc.with_ymd_and_hms(2026, 5, 14, 0, 1, 2).unwrap(),
            &[product_path],
        )
        .unwrap();

        assert_eq!(
            current_path,
            temp.path().join("published").join("current_artifacts.json")
        );
        assert!(!temp
            .path()
            .join("published")
            .join("current_artifacts_20260514T000102Z.json")
            .is_file());
        let manifests: Vec<CurrentArtifactsManifest> =
            serde_json::from_slice(&fs::read(current_path).unwrap()).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(
            manifests[0].contracts.get("nav-db").map(String::as_str),
            Some(NAV_DB_CONTRACT_ID)
        );
    }

    #[test]
    fn merge_does_not_repeat_producer_package_hashing() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("main")
            .join("20260514T000000Z");
        let packaged_root = publish_dir.join("packaged");
        fs::create_dir_all(&packaged_root).unwrap();

        let payload = b"valid package payload";
        let nav_db_sha = sha256_hex(payload);
        let nav_db_filename = format!("nav_db_{NAV_DB_CONTRACT_ID}_2605_01_{nav_db_sha}.zip");
        let package_path = packaged_root.join(&nav_db_filename);
        fs::write(&package_path, payload).unwrap();
        let bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2605_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2605".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-14T00:00:00Z".to_string(),
            effective_date: "2026-05-14".to_string(),
            expiration_date: "2026-06-11".to_string(),
            start_valid: "2026-05-14".to_string(),
            end_valid: "2026-06-11".to_string(),
            packages: vec![BundlePackageArtifact {
                id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_2605_01"),
                family_id: "nav-db".to_string(),
                contract_id: NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: nav_db_filename.clone(),
                relative_path: nav_db_filename,
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: nav_db_sha,
                size_bytes: payload.len() as u64,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-05-14".to_string()),
                expiration_date: Some("2026-06-11".to_string()),
                warning_text: None,
                metadata: BTreeMap::new(),
            }],
            ancillary: vec![],
        };
        let bundle_path = write_hashed_bundle_manifest(&packaged_root, &bundle).unwrap();
        let product_manifest = CurrentArtifactsManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            contracts: test_contracts(&[("nav-db", NAV_DB_CONTRACT_ID)]),
            artifact_roots: current_artifact_roots_for_packaged_root(&packaged_root).unwrap(),
            as_of_date: "2026-05-14".to_string(),
            as_of_utc: "2026-05-14T00:00:00Z".to_string(),
            bundles: vec![current_bundle_entry_from_path(&bundle_path).unwrap()],
            startup_prefetch: None,
            diagnostics: None,
        };
        let product_path = publish_dir.join("product_artifacts.json");
        fs::write(
            &product_path,
            serde_json::to_vec_pretty(&product_manifest).unwrap(),
        )
        .unwrap();

        assert!(!artifact_verification_is_cached(&package_path).unwrap());

        merge_current_artifacts_manifests(
            temp.path(),
            Utc.with_ymd_and_hms(2026, 5, 14, 0, 1, 2).unwrap(),
            &[product_path],
        )
        .unwrap();
        assert!(!artifact_verification_is_cached(&package_path).unwrap());
    }

    #[test]
    fn merge_current_artifacts_validates_version_subroots() {
        let temp = tempdir().unwrap();
        let mut product_paths = Vec::new();

        for (label, contract_id, timestamp) in [
            ("nav6-sunset", "NAV6", "20260514T000000Z"),
            ("master", NAV_DB_CONTRACT_ID, "20260514T000100Z"),
        ] {
            let publish_dir = temp.path().join("published").join(label).join(timestamp);
            let packaged_root = publish_dir.join("packaged");
            fs::create_dir_all(&packaged_root).unwrap();
            let nav_db_sha = sha256_hex(&[]);
            let nav_db_filename = format!("nav_db_{contract_id}_2605_01_{nav_db_sha}.zip");
            fs::write(packaged_root.join(&nav_db_filename), []).unwrap();
            let bundle = BundleManifest {
                schema_version: BUNDLE_SCHEMA_VERSION,
                bundle_id: format!("cycle_2605_01_{label}"),
                bundle_type: "cycle".to_string(),
                cycle: "2605".to_string(),
                cycle_version: "01".to_string(),
                generated_at_utc: "2026-05-14T00:00:00Z".to_string(),
                effective_date: "2026-05-14".to_string(),
                expiration_date: "2026-06-11".to_string(),
                start_valid: "2026-05-14".to_string(),
                end_valid: "2026-06-11".to_string(),
                packages: vec![BundlePackageArtifact {
                    id: format!("NAV_DB_{contract_id}_2605_01"),
                    family_id: "nav-db".to_string(),
                    contract_id: contract_id.to_string(),
                    region_id: None,
                    filename: nav_db_filename.clone(),
                    relative_path: nav_db_filename,
                    cycle: Some("2605".to_string()),
                    cycle_version: Some("01".to_string()),
                    checksum_sha256: nav_db_sha,
                    size_bytes: 0,
                    published_at_utc: None,
                    source_generated_at_utc: None,
                    source_version: None,
                    source_fetched_at_utc: None,
                    effective_date: Some("2026-05-14".to_string()),
                    expiration_date: Some("2026-06-11".to_string()),
                    warning_text: None,
                    metadata: BTreeMap::new(),
                }],
                ancillary: vec![],
            };
            let bundle_path = write_hashed_bundle_manifest(&packaged_root, &bundle).unwrap();
            let product_manifest = CurrentArtifactsManifest {
                schema_version: CURRENT_SCHEMA_VERSION,
                contracts: test_contracts(&[("nav-db", contract_id)]),
                artifact_roots: current_artifact_roots_for_packaged_root(&packaged_root).unwrap(),
                as_of_date: "2026-05-14".to_string(),
                as_of_utc: "2026-05-14T00:00:00Z".to_string(),
                bundles: vec![current_bundle_entry_from_path(&bundle_path).unwrap()],
                startup_prefetch: None,
                diagnostics: None,
            };
            let product_path = publish_dir.join("product_artifacts.json");
            fs::write(
                &product_path,
                serde_json::to_vec_pretty(&product_manifest).unwrap(),
            )
            .unwrap();
            product_paths.push(product_path);
        }

        let current_path = merge_current_artifacts_manifests(
            temp.path(),
            Utc.with_ymd_and_hms(2026, 5, 14, 0, 2, 0).unwrap(),
            &product_paths,
        )
        .unwrap();

        let manifests: Vec<CurrentArtifactsManifest> =
            serde_json::from_slice(&fs::read(current_path).unwrap()).unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(
            manifests[0].artifact_roots.packaged,
            "nav6-sunset/20260514T000000Z/packaged/"
        );
        assert_eq!(
            manifests[1].artifact_roots.packaged,
            "master/20260514T000100Z/packaged/"
        );
    }

    #[test]
    fn nav_db_unpacked_sync_xzs_pages_but_leaves_root_raw() {
        let temp = tempdir().unwrap();
        let source_root = temp.path().join("source");
        let unpacked_root = temp
            .path()
            .join("published")
            .join("master")
            .join("20260504T000000Z")
            .join("unpacked");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&unpacked_root).unwrap();
        fs::write(source_root.join("root"), b"raw-root").unwrap();
        fs::write(source_root.join("page_0001"), b"raw-page-one").unwrap();
        fs::write(source_root.join("page_0002"), b"raw-page-two").unwrap();
        let zip_path = temp.path().join("nav_db_test.zip");
        zip_directory_deterministic(&zip_path, &source_root, &["root", "page_0001", "page_0002"])
            .unwrap();

        let (_, unpack_dir) = sync_nav_db_unpacked_zip_from_source(
            &zip_path,
            &source_root,
            &unpacked_root,
            "nav_db_test.zip",
            None,
            2,
        )
        .unwrap();

        assert_eq!(fs::read(unpack_dir.join("root")).unwrap(), b"raw-root");
        for (page_name, expected) in [
            ("page_0001", b"raw-page-one".as_slice()),
            ("page_0002", b"raw-page-two".as_slice()),
        ] {
            let page_path = unpack_dir.join(page_name);
            let page = fs::read(&page_path).unwrap();
            assert_eq!(&page[..6], &[0xfd, b'7', b'z', b'X', b'Z', 0x00]);
            let output = Command::new("xz")
                .arg("--decompress")
                .arg("--stdout")
                .arg(&page_path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected);
        }
    }

    #[test]
    fn packaged_cleanup_prunes_entries_not_reachable_from_product_artifacts() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("master")
            .join("20260504T000000Z");
        let root = publish_dir.join("packaged");
        fs::create_dir_all(&root).unwrap();
        let current_cycle_bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2604_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2604".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-04T00:00:00Z".to_string(),
            effective_date: "2026-04-16".to_string(),
            expiration_date: "2026-05-14".to_string(),
            start_valid: "2026-04-16".to_string(),
            end_valid: "2026-05-14".to_string(),
            packages: vec![],
            ancillary: vec![],
        };
        let current_cycle_bundle_path =
            write_hashed_bundle_manifest(&root, &current_cycle_bundle).unwrap();
        let current = CurrentArtifactsManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            contracts: test_contracts(&[]),
            artifact_roots: current_artifact_roots_for_packaged_root(&root).unwrap(),
            as_of_date: "2026-05-04".to_string(),
            as_of_utc: "2026-05-04T00:00:00Z".to_string(),
            bundles: vec![current_bundle_entry_from_path(&current_cycle_bundle_path).unwrap()],
            startup_prefetch: None,
            diagnostics: None,
        };
        let current_path = publish_dir.join("product_artifacts.json");
        fs::write(&current_path, serde_json::to_vec_pretty(&current).unwrap()).unwrap();

        let stale_cycle_bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2604_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2604".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-03T00:00:00Z".to_string(),
            effective_date: "2026-04-16".to_string(),
            expiration_date: "2026-05-14".to_string(),
            start_valid: "2026-04-16".to_string(),
            end_valid: "2026-05-14".to_string(),
            packages: vec![BundlePackageArtifact {
                id: "EC_ENR_L_ENL1_2603_01".to_string(),
                family_id: "enr-l".to_string(),
                contract_id: ENR_L_CONTRACT_ID.to_string(),
                region_id: Some("ec".to_string()),
                filename: "enr_l_ec_ENL1_2603_01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.zip".to_string(),
                relative_path: "enr_l_ec_ENL1_2603_01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.zip".to_string(),
                cycle: Some("2603".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "a".repeat(64),
                size_bytes: 123,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-03-19".to_string()),
                expiration_date: Some("2026-04-16".to_string()),
                warning_text: None,
                metadata: BTreeMap::new(),
            }],
            ancillary: vec![],
        };
        let stale_cycle_bundle_path =
            write_hashed_bundle_manifest(&root, &stale_cycle_bundle).unwrap();

        cleanup_published_packaged_root(&root, &current_path).unwrap();

        assert!(current_path.is_file());
        assert!(current_cycle_bundle_path.is_file());
        assert!(!stale_cycle_bundle_path.exists());
    }

    #[test]
    fn unpacked_cleanup_uses_unpacked_root_for_package_dirs() {
        let temp = tempdir().unwrap();
        let publish_dir = temp
            .path()
            .join("published")
            .join("master")
            .join("20260504T000000Z");
        let packaged_root = publish_dir.join("packaged");
        let unpacked_root = publish_dir.join("unpacked");
        fs::create_dir_all(&packaged_root).unwrap();
        fs::create_dir_all(&unpacked_root).unwrap();
        let package_filename =
            "csup_ak_CSUP1_2604_01_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.zip";
        let package_stem = zip_stem(package_filename).unwrap();
        let package_dir = unpacked_root.join(&package_stem);
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(package_dir.join("manifest.json"), "{}\n").unwrap();
        let bundle = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: "cycle_2604_01".to_string(),
            bundle_type: "cycle".to_string(),
            cycle: "2604".to_string(),
            cycle_version: "01".to_string(),
            generated_at_utc: "2026-05-04T00:00:00Z".to_string(),
            effective_date: "2026-04-16".to_string(),
            expiration_date: "2026-05-14".to_string(),
            start_valid: "2026-04-16".to_string(),
            end_valid: "2026-05-14".to_string(),
            packages: vec![BundlePackageArtifact {
                id: "AK_CSUP_CSUP1_2604_01".to_string(),
                family_id: "csup".to_string(),
                contract_id: CSUP_CONTRACT_ID.to_string(),
                region_id: Some("ak".to_string()),
                filename: package_filename.to_string(),
                relative_path: package_filename.to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: "b".repeat(64),
                size_bytes: 123,
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                warning_text: None,
                metadata: BTreeMap::new(),
            }],
            ancillary: vec![],
        };
        let bundle_path = write_hashed_bundle_manifest(&packaged_root, &bundle).unwrap();
        sync_unpacked_file(&bundle_path, &unpacked_root).unwrap();
        let current = CurrentArtifactsManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            contracts: test_contracts(&[("csup", CSUP_CONTRACT_ID)]),
            artifact_roots: current_artifact_roots_for_packaged_root(&packaged_root).unwrap(),
            as_of_date: "2026-05-04".to_string(),
            as_of_utc: "2026-05-04T00:00:00Z".to_string(),
            bundles: vec![current_bundle_entry_from_path(&bundle_path).unwrap()],
            startup_prefetch: None,
            diagnostics: None,
        };
        let current_path = publish_dir.join("product_artifacts.json");
        fs::write(&current_path, serde_json::to_vec_pretty(&current).unwrap()).unwrap();

        cleanup_published_unpacked_root(&unpacked_root, &current_path).unwrap();

        assert!(unpacked_root.join(&package_stem).is_dir());
        assert!(!packaged_root.join(&package_stem).exists());
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
            "charts-tac/source_urls.jsonl",
            &[
                r#"{"event":"list_crawl","results":["https://aeronav.faa.gov/visual/03-19-2026/tac-files/Seattle_TAC.zip"]}"#,
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
            chart_family_version_label(temp.path(), ChartFamily::Flyway).unwrap(),
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
        let requests = vec![
            PrefetchRequest::new("https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP"),
            PrefetchRequest::new("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260416.zip"),
        ];
        let filtered = cycle_data_requests(requests);
        assert_eq!(
            filtered
                .iter()
                .map(|request| request.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260416.zip"]
        );
    }

    #[test]
    fn interprets_usgs_dem_cell_names_as_northwest_corners() {
        assert_eq!(terrain_cell_origin("n36w102"), Some((35.0, -102.0)));
        assert_eq!(terrain_cell_origin("s01e123"), Some((-2.0, 123.0)));

        assert!(terrain_cell_intersects_region("n37w102", Region::Sc));
        assert!(terrain_cell_intersects_region("n40w107", Region::Sw));
    }

    #[test]
    fn folder_categories_keep_other_out_of_approach_and_sort_hotspots_last() {
        assert_eq!(folder_category_for_document_type("approach"), "approach");
        assert_eq!(folder_category_for_document_type("other"), "other");
        assert_eq!(folder_category_for_document_type("hotspot"), "hotspot");
        assert!(folder_category_rank("takeoff-mins") < folder_category_rank("other"));
        assert!(folder_category_rank("other") < folder_category_rank("hotspot"));
    }

    #[test]
    fn cifp_declared_navaid_section_overrides_fix_like_procedure_identifier() {
        let context = NavLookupContext {
            airport_positions: BTreeMap::new(),
            navaid_positions: BTreeMap::from([(
                "RWF".to_string(),
                serde_json::json!({ "lat": 44.46727361111111, "lon": -95.12823 }),
            )]),
            arinc_navaid_positions: BTreeMap::new(),
            terminal_navaid_positions: BTreeMap::new(),
            fix_positions: BTreeMap::new(),
            airport_positions_by_coord: BTreeMap::new(),
            navaid_positions_by_coord: BTreeMap::new(),
            fix_positions_by_coord: BTreeMap::new(),
            runway_positions: BTreeMap::new(),
            navaid_variation: BTreeMap::new(),
            arinc_navaid_variation: BTreeMap::new(),
            terminal_navaid_variation: BTreeMap::new(),
            airport_variation: BTreeMap::new(),
            airport_elevation_ft: BTreeMap::new(),
        };

        assert_eq!(
            context.classify_cifp_reference_json("RWF", "", "D", "", "KRWF"),
            serde_json::json!({ "Navaid": "RWF" })
        );
        assert_eq!(
            context.classify_json("RWF"),
            serde_json::json!({ "Navaid": "RWF" })
        );
        assert_eq!(
            context.classify_json("RW17"),
            serde_json::json!({ "Fix": "RW17" })
        );
    }

    #[test]
    fn cifp_qualified_navaid_reference_uses_arinc_scope() {
        let key = ArincNavaidKey::new("JN", "K7", "D", "B");
        let procedure_key = TerminalNavaidKey::new("KABI", "AB", "K4", "P", "N");
        let context = NavLookupContext {
            airport_positions: BTreeMap::new(),
            // Ambiguous identifiers have no bare NavRef; qualified records remain usable.
            navaid_positions: BTreeMap::new(),
            arinc_navaid_positions: BTreeMap::from([(
                key.clone(),
                serde_json::json!({ "lat": 35.4749992, "lon": -78.4252856 }),
            )]),
            terminal_navaid_positions: BTreeMap::from([(
                procedure_key.clone(),
                serde_json::json!({ "lat": 32.2988633333333, "lon": -99.6742280555556 }),
            )]),
            fix_positions: BTreeMap::new(),
            airport_positions_by_coord: BTreeMap::new(),
            navaid_positions_by_coord: BTreeMap::new(),
            fix_positions_by_coord: BTreeMap::new(),
            runway_positions: BTreeMap::new(),
            navaid_variation: BTreeMap::new(),
            arinc_navaid_variation: BTreeMap::from([(key, Some(-9.0))]),
            terminal_navaid_variation: BTreeMap::from([(procedure_key, Some(5.0))]),
            airport_variation: BTreeMap::new(),
            airport_elevation_ft: BTreeMap::new(),
        };

        let nav_ref = context.classify_cifp_reference_json("JN", "K7", "D", "B", "KJNX");
        assert_eq!(
            nav_ref,
            serde_json::json!({
                "ArincNavaid": {
                    "identifier": "JN",
                    "icao_code": "K7",
                    "section_code": "D",
                    "subsection_code": "B",
                }
            })
        );
        assert_eq!(
            context.resolve_position_json(&nav_ref, Some("KJNX")),
            serde_json::json!({ "lat": 35.4749992, "lon": -78.4252856 })
        );
        assert_eq!(
            context.variation_for_nav_ref(&nav_ref),
            serde_json::json!(-9.0)
        );

        let procedure_nav_ref = context.classify_cifp_reference_json("AB", "K4", "P", "N", "KABI");
        assert_eq!(
            procedure_nav_ref,
            serde_json::json!({
                "TerminalNavaid": {
                    "airport_id": "KABI",
                    "identifier": "AB",
                    "icao_code": "K4",
                    "section_code": "P",
                    "subsection_code": "N",
                }
            })
        );
        assert_eq!(
            context.resolve_position_json(&procedure_nav_ref, Some("KABI")),
            serde_json::json!({ "lat": 32.2988633333333, "lon": -99.6742280555556 })
        );
        assert_eq!(
            context.classify_cifp_reference_json("AB", "", "D", "B", "KABI"),
            serde_json::Value::Null
        );
    }
}
