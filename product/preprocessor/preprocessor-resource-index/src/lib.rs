use anyhow::{bail, Context};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use preprocessor_core::{
    PackageAssetManifest, PackageAssetRecord, PlateGeoref, Region, PACKAGE_ASSET_MANIFEST_NAME,
};
use preprocessor_data::INTERMEDIATE_SQLITE_BASENAME;
use preprocessor_fetch::PackageOutputRecord;
use rayon::prelude::*;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildResourceIndexRequest {
    pub nav_db_zip: PathBuf,
    pub output_path: PathBuf,
    pub catalog_output_path: Option<PathBuf>,
    pub chart_sources: Vec<ChartSource>,
    pub tpp_sources: Vec<AssetSource>,
    pub csup_sources: Vec<AssetSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartSource {
    pub family_id: String,
    pub package_outputs_path: PathBuf,
    pub asset_root: PathBuf,
    pub package_root: PathBuf,
    pub unpack_source_root: PathBuf,
    pub source_urls_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSource {
    pub package_outputs_path: PathBuf,
    pub asset_root: PathBuf,
    pub package_root: PathBuf,
    pub unpack_source_root: PathBuf,
    pub source_urls_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceIndex {
    pub schema_version: u32,
    pub cycle: Option<String>,
    pub generated_at_utc: String,
    pub temporal_summary: TemporalSummary,
    pub nav_db: NavDbRef,
    pub families: Vec<ResourceFamily>,
    pub regions: Vec<ResourceRegion>,
    pub packages: Vec<ResourcePackage>,
    pub chart_collections: Vec<ChartCollectionRecord>,
    pub airports: Vec<AirportRecord>,
    pub airport_resources: Vec<AirportResourcesRecord>,
    pub plates: Vec<PlateRecord>,
    pub csups: Vec<CsupRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub schema_version: u32,
    pub cycle: String,
    pub catalog_revision: String,
    pub families: Vec<CatalogFamily>,
    pub regions: Vec<ResourceRegion>,
    pub packages: Vec<CatalogPackage>,
    pub charts: Vec<CatalogChartRecord>,
    pub plates: Vec<CatalogPlateRecord>,
    pub supplements: Vec<CatalogSupplementRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogFamily {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub max_zoom: Option<u32>,
    pub tile_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogPackage {
    pub id: CatalogPackageId,
    pub package_name: String,
    pub family_id: String,
    pub region_id: String,
    pub cycle: String,
    pub artifact_kind: String,
    pub relative_url: String,
    pub manifest_name: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogPackageId {
    pub region: String,
    pub family: String,
    pub cycle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogChartRecord {
    pub id: CatalogChartId,
    pub family_id: String,
    pub name: String,
    pub display_name: String,
    pub cycle: String,
    pub region_ids: Vec<String>,
    pub max_zoom: u32,
    pub tile_path_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogChartId {
    pub family: String,
    pub name: String,
    pub cycle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogPlateRecord {
    pub id: CatalogPlateId,
    pub airport_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao_airport_id: Option<String>,
    pub region_id: String,
    pub cycle: String,
    pub procedure_code: String,
    pub display_name: String,
    pub kind: String,
    pub georeferenced: bool,
    pub page_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogPlateId {
    pub airport_id: String,
    pub procedure_code: String,
    pub page: u32,
    pub cycle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogSupplementRecord {
    pub airport_id: String,
    pub region_id: String,
    pub cycle: String,
    pub page_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NavDbRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    pub sqlite_entry: String,
    pub cycle_code: Option<String>,
    pub version_label: Option<String>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemporalSummary {
    pub cycle_codes: Vec<String>,
    pub effective_dates: Vec<String>,
    pub expiration_dates: Vec<String>,
    pub uniform_cycle_code: Option<String>,
    pub uniform_effective_date: Option<String>,
    pub uniform_expiration_date: Option<String>,
    pub uniform_good_beyond_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceFamily {
    pub id: String,
    pub display_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceRegion {
    pub id: String,
    pub display_name: String,
    pub sort_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcePackage {
    pub id: String,
    pub family_id: String,
    pub region_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    pub size_bytes: u64,
    pub checksum_sha256: String,
    pub cycle_code: Option<String>,
    pub version_label: Option<String>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FaaTemporalMetadata {
    cycle_code: Option<String>,
    effective_date: Option<String>,
    expiration_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartCollectionRecord {
    pub id: String,
    pub family_id: String,
    pub region_id: String,
    pub package_id: String,
    pub chart_index: u32,
    pub tile_path_template: String,
    pub levels: Vec<TileLevelRecord>,
    pub coverage_bounds: CoverageBounds,
    pub default_view: DefaultView,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileLevelRecord {
    pub zoom: u32,
    pub boxes: Vec<TileBoundsRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileBoundsRecord {
    pub x_min: u32,
    pub x_max: u32,
    pub y_tms_min: u32,
    pub y_tms_max: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageBounds {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultView {
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AirportRecord {
    pub id: String,
    pub facility_name: String,
    pub lat: f64,
    pub lon: f64,
    pub airport_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct AirportResourcesRecord {
    pub airport_id: String,
    pub plate_ids: Vec<String>,
    pub csup_ids: Vec<String>,
    pub package_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlateRecord {
    pub id: String,
    pub airport_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao_airport_id: Option<String>,
    pub region_id: String,
    pub package_id: String,
    pub asset_path: String,
    pub thumbnail_path: String,
    pub label: String,
    pub asset_kind: String,
    pub document_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsupRecord {
    pub id: String,
    pub airport_id: String,
    pub region_id: String,
    pub package_id: String,
    pub asset_path: String,
    pub thumbnail_path: String,
    pub label: String,
    pub asset_kind: String,
    pub document_type: String,
}

pub fn build_resource_index(request: &BuildResourceIndexRequest) -> anyhow::Result<ResourceIndex> {
    log_progress(request, "begin build_resource_index")?;
    let artifact_root = artifact_root(&request.output_path)?;
    log_progress(request, "located artifact_root")?;
    let nav_cycle_code = read_nav_cycle_code(&request.nav_db_zip)?;
    log_progress(request, "read nav cycle code")?;
    let nav_temporal = nav_cycle_code
        .as_deref()
        .and_then(temporal_from_cycle_code)
        .unwrap_or(FaaTemporalMetadata {
            cycle_code: None,
            effective_date: None,
            expiration_date: None,
        });
    let cycle = infer_cycle(
        &request.chart_sources,
        &request.tpp_sources,
        &request.csup_sources,
    )?
    .or_else(|| nav_cycle_code.clone());
    let packages = collect_packages(
        &request.chart_sources,
        &request.tpp_sources,
        &request.csup_sources,
        &artifact_root,
    )?;
    log_progress(request, "collected packages")?;
    let temporal_summary = build_temporal_summary(&packages, &nav_temporal);
    let chart_collections = collect_chart_collections(&request.chart_sources)?;
    log_progress(request, "collected chart collections")?;
    let sqlite_path = extract_sqlite_entry(&request.nav_db_zip, INTERMEDIATE_SQLITE_BASENAME)?;
    log_progress(request, "extracted nav sqlite")?;
    let connection = Connection::open(sqlite_path.path())
        .context("failed to open extracted intermediate sqlite")?;
    log_progress(request, "opened nav sqlite")?;
    let airports = load_airports_from_nav_db(&connection)?;
    log_progress(request, "loaded airports")?;
    let airport_aliases = load_airport_aliases_from_nav_db(&connection)?;
    log_progress(request, "loaded airport aliases")?;
    let thumbnail_root = request
        .output_path
        .parent()
        .context("resource-index output path must have a parent directory")?
        .join("thumbnails");
    let plates = collect_plate_records(&request.tpp_sources, &airport_aliases, &thumbnail_root)?;
    log_progress(request, "collected plate records")?;
    let csups = collect_csup_records(&request.csup_sources, &airport_aliases, &thumbnail_root)?;
    log_progress(request, "collected csup records")?;
    let airport_resources = collect_airport_resources(&plates, &csups);
    log_progress(request, "collected airport resources")?;
    let families = collect_families(&packages, !plates.is_empty(), !csups.is_empty());
    log_progress(request, "collected families")?;
    let regions = Region::ALL
        .iter()
        .enumerate()
        .map(|(index, region)| ResourceRegion {
            id: region.code().to_ascii_lowercase(),
            display_name: region_display_name(*region).to_string(),
            sort_order: index as u32,
        })
        .collect();
    log_progress(request, "collected regions")?;

    let index = ResourceIndex {
        schema_version: 5,
        cycle,
        generated_at_utc: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        temporal_summary,
        nav_db: NavDbRef {
            artifact_path: Some(relativize_to_artifact_root(
                &request.nav_db_zip,
                &artifact_root,
            )),
            sqlite_entry: INTERMEDIATE_SQLITE_BASENAME.to_string(),
            cycle_code: nav_cycle_code,
            version_label: nav_db_version_label(&request.nav_db_zip, &nav_temporal),
            effective_date: nav_temporal.effective_date,
            expiration_date: nav_temporal.expiration_date,
        },
        families,
        regions,
        packages,
        chart_collections,
        airports,
        airport_resources,
        plates,
        csups,
    };
    log_progress(request, "built index structure")?;
    validate_thumbnail_paths(&index, &thumbnail_root)?;
    log_progress(request, "validated thumbnails")?;
    validate_packaged_assets(&index, &index.packages, &artifact_root)?;
    log_progress(request, "validated packaged assets")?;
    Ok(index)
}

pub fn write_resource_index(request: &BuildResourceIndexRequest) -> anyhow::Result<ResourceIndex> {
    let parent = request
        .output_path
        .parent()
        .context("output path must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    log_progress(request, "write_resource_index parent ready")?;
    let thumbnail_root = parent.join("thumbnails");
    if thumbnail_root.exists() {
        fs::remove_dir_all(&thumbnail_root)
            .with_context(|| format!("failed to clear {}", thumbnail_root.display()))?;
    }
    log_progress(request, "thumbnail root cleared")?;
    let index = build_resource_index(request)?;
    log_progress(request, "build_resource_index returned")?;
    let json = serde_json::to_vec_pretty(&index).context("failed to serialize resource index")?;
    fs::write(&request.output_path, json)
        .with_context(|| format!("failed to write {}", request.output_path.display()))?;
    log_progress(request, "wrote resource-index.json")?;
    let catalog_output_path = request
        .catalog_output_path
        .clone()
        .unwrap_or_else(|| parent.join("catalog.json"));
    let catalog = build_catalog(&index);
    let catalog_json =
        serde_json::to_vec_pretty(&catalog).context("failed to serialize catalog")?;
    fs::write(&catalog_output_path, catalog_json)
        .with_context(|| format!("failed to write {}", catalog_output_path.display()))?;
    log_progress(request, "wrote catalog.json")?;
    Ok(index)
}

fn log_progress(request: &BuildResourceIndexRequest, message: &str) -> anyhow::Result<()> {
    eprintln!(
        "resource-index-progress {} {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        message
    );
    let parent = request
        .output_path
        .parent()
        .context("resource-index output path must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let path = parent.join("resource-index.progress.log");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(
        file,
        "{} {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        message
    )
    .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn build_catalog(index: &ResourceIndex) -> Catalog {
    let cycle = index.cycle.clone().unwrap_or_else(|| "unknown".to_string());
    let supported_families = BTreeSet::from([
        "sec".to_string(),
        "tac".to_string(),
        "enr-l".to_string(),
        "enr-h".to_string(),
    ]);
    let family_by_id = index
        .families
        .iter()
        .map(|family| (family.id.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let package_by_id = index
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect::<BTreeMap<_, _>>();

    let families = index
        .families
        .iter()
        .filter(|family| supported_families.contains(&family.id))
        .map(|family| CatalogFamily {
            id: family.id.clone(),
            display_name: family.display_name.clone(),
            kind: family.kind.clone(),
            max_zoom: index
                .chart_collections
                .iter()
                .filter(|collection| collection.family_id == family.id)
                .flat_map(|collection| collection.levels.iter().map(|level| level.zoom))
                .max(),
            tile_size: (family.kind == "tiled_raster").then_some(512),
        })
        .collect::<Vec<_>>();

    let packages = index
        .packages
        .iter()
        .filter(|package| supported_families.contains(&package.family_id))
        .map(|package| CatalogPackage {
            id: CatalogPackageId {
                region: package.region_id.clone(),
                family: package.family_id.clone(),
                cycle: cycle.clone(),
            },
            package_name: package.id.clone(),
            family_id: package.family_id.clone(),
            region_id: package.region_id.clone(),
            cycle: cycle.clone(),
            artifact_kind: "zip".to_string(),
            relative_url: package.id.clone(),
            manifest_name: package.id.clone(),
            size_bytes: Some(package.size_bytes),
            checksum_sha256: Some(package.checksum_sha256.clone()),
        })
        .collect::<Vec<_>>();

    let charts = index
        .chart_collections
        .iter()
        .filter(|collection| supported_families.contains(&collection.family_id))
        .map(|collection| CatalogChartRecord {
            id: CatalogChartId {
                family: collection.family_id.clone(),
                name: collection.id.clone(),
                cycle: cycle.clone(),
            },
            family_id: collection.family_id.clone(),
            name: collection.id.clone(),
            display_name: format!(
                "{} {}",
                region_display_name_from_id(&index.regions, &collection.region_id),
                family_display_name_from_id(&family_by_id, &collection.family_id)
            ),
            cycle: cycle.clone(),
            region_ids: vec![collection.region_id.clone()],
            max_zoom: collection
                .levels
                .iter()
                .map(|level| level.zoom)
                .max()
                .unwrap_or(0),
            tile_path_template: collection.tile_path_template.clone(),
        })
        .collect::<Vec<_>>();

    let plates = index
        .plates
        .iter()
        .filter_map(|plate| {
            package_by_id.get(&plate.package_id)?;
            let procedure_code = procedure_code_from_asset_path(&plate.asset_path);
            Some(CatalogPlateRecord {
                id: CatalogPlateId {
                    airport_id: plate.airport_id.clone(),
                    procedure_code: procedure_code.clone(),
                    page: 1,
                    cycle: cycle.clone(),
                },
                airport_id: plate.airport_id.clone(),
                icao_airport_id: plate.icao_airport_id.clone(),
                region_id: plate.region_id.clone(),
                cycle: cycle.clone(),
                procedure_code,
                display_name: plate.label.clone(),
                kind: plate.asset_kind.clone(),
                georeferenced: plate.georef.is_some(),
                page_count: 1,
                procedure_uid: plate.procedure_uid.clone(),
                georef: plate.georef.clone(),
            })
        })
        .collect::<Vec<_>>();

    let supplements = index
        .csups
        .iter()
        .filter_map(|csup| {
            package_by_id.get(&csup.package_id)?;
            Some(CatalogSupplementRecord {
                airport_id: csup.airport_id.clone(),
                region_id: csup.region_id.clone(),
                cycle: cycle.clone(),
                page_count: 1,
            })
        })
        .collect::<Vec<_>>();

    Catalog {
        schema_version: index.schema_version,
        cycle,
        catalog_revision: index.generated_at_utc.clone(),
        families,
        regions: index.regions.clone(),
        packages,
        charts,
        plates,
        supplements,
    }
}

fn region_display_name_from_id(regions: &[ResourceRegion], region_id: &str) -> String {
    regions
        .iter()
        .find(|region| region.id == region_id)
        .map(|region| region.display_name.clone())
        .unwrap_or_else(|| region_id.to_ascii_uppercase())
}

fn family_display_name_from_id(
    families: &BTreeMap<String, &ResourceFamily>,
    family_id: &str,
) -> String {
    families
        .get(family_id)
        .map(|family| family.display_name.clone())
        .unwrap_or_else(|| family_id.to_string())
}

fn validate_thumbnail_paths(index: &ResourceIndex, thumbnail_root: &Path) -> anyhow::Result<()> {
    let indexed_plate = index
        .plates
        .iter()
        .map(|record| record.thumbnail_path.clone())
        .collect::<BTreeSet<_>>();
    let indexed_csup = index
        .csups
        .iter()
        .map(|record| record.thumbnail_path.clone())
        .collect::<BTreeSet<_>>();
    let actual_plate = collect_actual_thumbnail_paths(thumbnail_root, "plates")?;
    let actual_csup = collect_actual_thumbnail_paths(thumbnail_root, "afd")?;
    compare_indexed_vs_actual("plate thumbnails", &indexed_plate, &actual_plate)?;
    compare_indexed_vs_actual("csup thumbnails", &indexed_csup, &actual_csup)?;
    Ok(())
}

fn validate_packaged_assets(
    index: &ResourceIndex,
    packages: &[ResourcePackage],
    artifact_root: &Path,
) -> anyhow::Result<()> {
    let package_map = packages
        .iter()
        .map(|package| {
            let artifact_path = package.artifact_path.as_ref().with_context(|| {
                format!("package {} missing internal artifact_path", package.id)
            })?;
            Ok::<_, anyhow::Error>((package.id.clone(), artifact_root.join(artifact_path)))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let package_members = package_map
        .iter()
        .map(|(package_id, package_path)| {
            Ok::<_, anyhow::Error>((package_id.clone(), read_package_members(package_path)?))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    for plate in &index.plates {
        validate_packaged_member(
            &package_members,
            &package_map,
            &plate.package_id,
            &plate.asset_path,
            &plate.id,
        )?;
        validate_packaged_member(
            &package_members,
            &package_map,
            &plate.package_id,
            &plate.thumbnail_path,
            &plate.id,
        )?;
    }
    for csup in &index.csups {
        validate_packaged_member(
            &package_members,
            &package_map,
            &csup.package_id,
            &csup.asset_path,
            &csup.id,
        )?;
        validate_packaged_member(
            &package_members,
            &package_map,
            &csup.package_id,
            &csup.thumbnail_path,
            &csup.id,
        )?;
    }
    Ok(())
}

fn validate_packaged_member(
    package_members: &BTreeMap<String, BTreeSet<String>>,
    package_map: &BTreeMap<String, PathBuf>,
    package_id: &str,
    member_path: &str,
    record_id: &str,
) -> anyhow::Result<()> {
    let members = package_members
        .get(package_id)
        .with_context(|| format!("missing package member index for {package_id}"))?;
    if members.contains(member_path) {
        return Ok(());
    }
    let package_path = package_map
        .get(package_id)
        .with_context(|| format!("missing package artifact for {package_id}"))?;
    bail!(
        "missing packaged member {member_path} in {} for {record_id}",
        package_path.display()
    );
}

fn read_package_members(package_path: &Path) -> anyhow::Result<BTreeSet<String>> {
    let file = fs::File::open(package_path)
        .with_context(|| format!("failed to open {}", package_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", package_path.display()))?;
    let mut members = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).with_context(|| {
            format!(
                "failed to read zip member #{index} from {}",
                package_path.display()
            )
        })?;
        members.insert(entry.name().to_string());
    }
    Ok(members)
}

fn collect_actual_thumbnail_paths(
    thumbnail_root: &Path,
    root_dir_name: &str,
) -> anyhow::Result<BTreeSet<String>> {
    let root = thumbnail_root.join(root_dir_name);
    let mut paths = BTreeSet::new();
    if !root.is_dir() {
        return Ok(paths);
    }
    for asset in read_files_recursive(&root)? {
        let extension = asset
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != "png" {
            continue;
        }
        let relative = asset
            .strip_prefix(thumbnail_root.parent().unwrap_or(thumbnail_root))
            .with_context(|| format!("failed to relativize {}", asset.display()))?;
        paths.insert(relative.display().to_string());
    }
    Ok(paths)
}

fn compare_indexed_vs_actual(
    label: &str,
    indexed: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> anyhow::Result<()> {
    if indexed == actual {
        return Ok(());
    }
    let missing_from_index = actual
        .difference(indexed)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    let missing_from_filesystem = indexed
        .difference(actual)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    bail!(
        "{label} index/filesystem mismatch: indexed={} actual={} missing_from_index={missing_from_index:?} missing_from_filesystem={missing_from_filesystem:?}",
        indexed.len(),
        actual.len()
    );
}

fn infer_cycle(
    chart_sources: &[ChartSource],
    tpp_sources: &[AssetSource],
    csup_sources: &[AssetSource],
) -> anyhow::Result<Option<String>> {
    let mut values = BTreeSet::new();
    for source in chart_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            values.insert(infer_cycle_from_manifest(&record.manifest));
        }
    }
    for source in tpp_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            values.insert(infer_cycle_from_manifest(&record.manifest));
        }
    }
    for source in csup_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            values.insert(infer_cycle_from_manifest(&record.manifest));
        }
    }
    Ok(values.into_iter().flatten().next())
}

fn collect_packages(
    chart_sources: &[ChartSource],
    tpp_sources: &[AssetSource],
    csup_sources: &[AssetSource],
    artifact_root: &Path,
) -> anyhow::Result<Vec<ResourcePackage>> {
    let chart_packages = chart_sources
        .par_iter()
        .map(|source| {
            collect_packages_for_source(
                &source.family_id,
                &source.package_root,
                source.source_urls_path.as_deref(),
                &source.package_outputs_path,
                artifact_root,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    let tpp_packages = tpp_sources
        .par_iter()
        .map(|source| {
            collect_packages_for_source(
                "tpp",
                &source.package_root,
                source.source_urls_path.as_deref(),
                &source.package_outputs_path,
                artifact_root,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    let csup_packages = csup_sources
        .par_iter()
        .map(|source| {
            collect_packages_for_source(
                "csup",
                &source.package_root,
                source.source_urls_path.as_deref(),
                &source.package_outputs_path,
                artifact_root,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut packages = chart_packages
        .into_iter()
        .chain(tpp_packages)
        .chain(csup_packages)
        .flatten()
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        (
            &left.family_id,
            &left.region_id,
            &left.id,
            &left.artifact_path,
            left.size_bytes,
            &left.checksum_sha256,
            &left.cycle_code,
            &left.version_label,
            &left.effective_date,
            &left.expiration_date,
        )
            .cmp(&(
                &right.family_id,
                &right.region_id,
                &right.id,
                &right.artifact_path,
                right.size_bytes,
                &right.checksum_sha256,
                &right.cycle_code,
                &right.version_label,
                &right.effective_date,
                &right.expiration_date,
            ))
    });
    Ok(packages)
}

fn collect_chart_collections(
    chart_sources: &[ChartSource],
) -> anyhow::Result<Vec<ChartCollectionRecord>> {
    let mut collections_with_tiles = chart_sources
        .par_iter()
        .map(|source| {
            read_package_outputs(&source.package_outputs_path)?
                .into_par_iter()
                .filter_map(|record| {
                    let artifact_path = source.package_root.join(&record.zip);
                    match package_record_has_tiles(&record, &source.package_root) {
                        Ok(true) => Some(Ok((record, artifact_path))),
                        Ok(false) => None,
                        Err(error) => Some(Err(error)),
                    }
                })
                .map(|entry| {
                    let (record, artifact_path) = entry?;
                    let metadata = read_chart_zip_metadata(&artifact_path).with_context(|| {
                        format!(
                            "failed to read chart tile metadata for {} region {} from {}",
                            source.family_id,
                            record.region,
                            artifact_path.display()
                        )
                    })?;
                    let collection = ChartCollectionRecord {
                        id: format!(
                            "{}:{}",
                            source.family_id,
                            record.region.to_ascii_lowercase()
                        ),
                        family_id: source.family_id.clone(),
                        region_id: record.region.to_ascii_lowercase(),
                        package_id: package_id_from_manifest_name(&record.manifest),
                        chart_index: metadata.chart_index,
                        tile_path_template: format!(
                            "tiles/{}/{}/{{x}}/{{y}}.webp",
                            metadata.chart_index, "{z}"
                        ),
                        levels: metadata.levels,
                        coverage_bounds: metadata.coverage_bounds,
                        default_view: metadata.default_view,
                    };
                    Ok::<_, anyhow::Error>(ChartCollectionWithTiles {
                        collection,
                        tiles: metadata.tiles,
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    validate_chart_tile_bbox_invariant(&collections_with_tiles)?;
    let mut collections = collections_with_tiles
        .drain(..)
        .map(|entry| entry.collection)
        .collect::<Vec<_>>();
    collections.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(collections)
}

#[derive(Debug)]
struct ChartCollectionWithTiles {
    collection: ChartCollectionRecord,
    tiles: BTreeSet<(u32, u32, u32)>,
}

fn validate_chart_tile_bbox_invariant(
    collections: &[ChartCollectionWithTiles],
) -> anyhow::Result<()> {
    // Runtime tile planning intentionally does not probe every region package for
    // a chart family. It picks any regional package whose tile bbox contains the
    // requested tile and treats a 404 from that package as "this tile is empty
    // everywhere." That is only safe if overlapping regional bboxes have
    // identical tile availability: for any family/z/x/y, every non-wide region
    // whose bbox contains the address must either all contain the concrete tile
    // file or all omit it. If this fires, do not weaken it casually; otherwise
    // the client can choose a region with no tile and fail to draw a tile that
    // exists in another region package.
    let mut by_family: BTreeMap<&str, Vec<&ChartCollectionWithTiles>> = BTreeMap::new();
    for collection in collections
        .iter()
        .filter(|entry| entry.collection.region_id != "wide")
    {
        by_family
            .entry(collection.collection.family_id.as_str())
            .or_default()
            .push(collection);
    }
    for (family_id, entries) in by_family {
        let mut all_tiles = BTreeSet::new();
        for entry in &entries {
            all_tiles.extend(entry.tiles.iter().copied());
        }
        for (zoom, x, y_tms) in all_tiles {
            for entry in &entries {
                if collection_contains_tile(&entry.collection, zoom, x, y_tms)
                    && !entry.tiles.contains(&(zoom, x, y_tms))
                {
                    bail!(
                        "chart tile bbox invariant failed: family {family_id} region {} bbox contains z{zoom}/{x}/{y_tms} but package {} does not contain that tile",
                        entry.collection.region_id,
                        entry.collection.package_id
                    );
                }
            }
        }
    }
    Ok(())
}

fn collection_contains_tile(
    collection: &ChartCollectionRecord,
    zoom: u32,
    x: u32,
    y_tms: u32,
) -> bool {
    collection.levels.iter().any(|level| {
        level.zoom == zoom
            && level.boxes.iter().any(|bbox| {
                x >= bbox.x_min
                    && x <= bbox.x_max
                    && y_tms >= bbox.y_tms_min
                    && y_tms <= bbox.y_tms_max
            })
    })
}

fn collect_packages_for_source(
    family_id: &str,
    package_root: &Path,
    source_urls_path: Option<&Path>,
    package_outputs_path: &Path,
    artifact_root: &Path,
) -> anyhow::Result<Vec<ResourcePackage>> {
    let temporal = infer_temporal_from_source_urls(source_urls_path)?;
    read_package_outputs(package_outputs_path)?
        .into_iter()
        .filter_map(
            |record| match package_record_has_tiles(&record, package_root) {
                Ok(true) => Some(Ok(record)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .map(|record| {
            let record = record?;
            package_from_record(
                family_id,
                package_root,
                &record,
                temporal.as_ref(),
                artifact_root,
            )
        })
        .collect()
}

fn package_record_has_tiles(
    record: &PackageOutputRecord,
    package_root: &Path,
) -> anyhow::Result<bool> {
    let Some(tile_count) = record
        .metadata
        .get("tile_count")
        .and_then(|value| value.as_u64())
    else {
        return Ok(true);
    };
    if tile_count == 0 {
        return Ok(false);
    }
    Ok(count_chart_zip_tile_entries(&package_root.join(&record.zip))? > 0)
}

fn count_chart_zip_tile_entries(path: &Path) -> anyhow::Result<u64> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open chart zip {}", path.display()))?;
    let archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open chart zip {}", path.display()))?;
    let count = archive
        .file_names()
        .filter(|name| {
            if !name.ends_with(".webp") {
                return false;
            }
            let parts = name.split('/').collect::<Vec<_>>();
            parts.len() == 5 && parts[0] == "tiles"
        })
        .count();
    Ok(count as u64)
}

fn collect_families(
    packages: &[ResourcePackage],
    has_tpp: bool,
    has_csup: bool,
) -> Vec<ResourceFamily> {
    let mut ids = BTreeSet::new();
    for package in packages {
        ids.insert(package.family_id.clone());
    }
    if has_tpp {
        ids.insert("tpp".to_string());
    }
    if has_csup {
        ids.insert("csup".to_string());
    }
    ids.into_iter()
        .map(|id| ResourceFamily {
            display_name: family_display_name(&id).to_string(),
            kind: family_kind(&id).to_string(),
            id,
        })
        .collect()
}

fn package_from_record(
    family_id: &str,
    package_root: &Path,
    record: &PackageOutputRecord,
    temporal: Option<&FaaTemporalMetadata>,
    artifact_root: &Path,
) -> anyhow::Result<ResourcePackage> {
    let artifact_path = package_root.join(&record.zip);
    let size_bytes = fs::metadata(&artifact_path)
        .with_context(|| format!("failed to stat {}", artifact_path.display()))?
        .len();
    Ok(ResourcePackage {
        id: package_id_from_manifest_name(&record.manifest),
        family_id: family_id.to_string(),
        region_id: record.region.to_ascii_lowercase(),
        artifact_path: Some(relativize_to_artifact_root(&artifact_path, artifact_root)),
        size_bytes,
        checksum_sha256: record.zip_sha256.clone(),
        cycle_code: temporal.and_then(|value| value.cycle_code.clone()),
        version_label: package_version_label(temporal),
        effective_date: temporal.and_then(|value| value.effective_date.clone()),
        expiration_date: temporal.and_then(|value| value.expiration_date.clone()),
        metadata: record.metadata.clone(),
    })
}

fn artifact_root(output_path: &Path) -> anyhow::Result<PathBuf> {
    output_path
        .ancestors()
        .find_map(|path| {
            let name = path.file_name().and_then(|v| v.to_str())?;
            match name {
                "published_packaged" | "product-builds" | "private-work" | "cache"
                | "published_unpacked" => path.parent().map(Path::to_path_buf),
                _ => None,
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "failed to locate artifact root from {}",
                output_path.display()
            )
        })
}

fn relativize_to_artifact_root(path: &Path, artifact_root: &Path) -> String {
    path.strip_prefix(artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn load_airports_from_nav_db(connection: &Connection) -> anyhow::Result<Vec<AirportRecord>> {
    let mut statement = connection.prepare(
        "select LocationID, FacilityName, ARPLatitude, ARPLongitude, Type
         from airports
         where LocationID is not null
           and ARPLatitude is not null
           and ARPLongitude is not null
         order by LocationID",
    )?;
    let airports = statement
        .query_map([], |row| {
            Ok(AirportRecord {
                id: row.get::<_, String>(0)?,
                facility_name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                lat: row.get::<_, f64>(2)?,
                lon: row.get::<_, f64>(3)?,
                airport_type: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to read airport rows")?;
    Ok(airports)
}

fn load_airport_aliases_from_nav_db(
    connection: &Connection,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut statement = connection.prepare(
        "select alias_id, airport_id
         from airport_aliases
         where alias_id is not null
           and airport_id is not null
         order by alias_id",
    )?;
    let aliases = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<BTreeMap<_, _>, _>>()
        .context("failed to read airport_aliases rows")?;
    Ok(aliases)
}

fn canonicalize_airport_id(raw_id: &str, airport_aliases: &BTreeMap<String, String>) -> String {
    airport_aliases
        .get(raw_id)
        .cloned()
        .unwrap_or_else(|| raw_id.to_string())
}

fn pretty_tpp_label(raw_label: &str, document_type: &str) -> String {
    let label = raw_label.trim();
    let Some((prefix, remainder)) = split_tpp_prefix(label) else {
        return label.to_string();
    };
    match (prefix, document_type) {
        ("APD", _) => "Airport Diagram".to_string(),
        ("MIN", "alternate_minimums") => {
            pretty_minimums(remainder, "ALTERNATE MINIMUMS", "Alt Minimums")
        }
        ("MIN", "takeoff_minimums") => {
            pretty_minimums(remainder, "TAKEOFF MINIMUMS", "Takeoff Minimums")
        }
        ("MIN", _) => pretty_minimums(remainder, "MINIMUMS", "Minimums"),
        ("IAP", _) => pretty_approach_label(remainder),
        ("HOT", "hotspot") => pretty_hotspot_label(remainder),
        ("DP", _) | ("ODP", _) | ("STAR", _) => remainder.to_string(),
        _ => label.to_string(),
    }
}

fn split_tpp_prefix(label: &str) -> Option<(&str, &str)> {
    let mut parts = label.splitn(3, '-');
    let prefix = parts.next()?;
    let state = parts.next()?;
    let remainder = parts.next()?;
    if prefix.is_empty() || state.len() != 2 || remainder.is_empty() {
        return None;
    }
    Some((prefix, remainder))
}

fn pretty_minimums(remainder: &str, raw_prefix: &str, pretty_prefix: &str) -> String {
    if let Some(suffix) = remainder.strip_prefix(raw_prefix) {
        if let Some(page) = suffix.strip_prefix('-') {
            return format!("{pretty_prefix} {page}");
        }
        return pretty_prefix.to_string();
    }
    remainder.replace('-', " ")
}

fn pretty_approach_label(remainder: &str) -> String {
    remainder
        .replace("RNAV (GPS)", "RNAV")
        .replace(" RWY ", " ")
        .replace(" OR ", " or ")
        .replace(" AND ", " and ")
}

fn pretty_hotspot_label(remainder: &str) -> String {
    if remainder == "HOT SPOT" {
        return "Hot Spot".to_string();
    }
    remainder
        .strip_prefix("HOT SPOT-")
        .map(|suffix| format!("Hot Spot {suffix}"))
        .unwrap_or_else(|| remainder.to_string())
}

fn procedure_code_from_asset_path(asset_path: &str) -> String {
    asset_path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".png"))
        .unwrap_or(asset_path)
        .to_string()
}

fn collect_plate_records(
    sources: &[AssetSource],
    airport_aliases: &BTreeMap<String, String>,
    thumbnail_root: &Path,
) -> anyhow::Result<Vec<PlateRecord>> {
    let mut records = collect_asset_records_parallel(sources, "tpp", thumbnail_root, |packaged| {
        let airport_id = canonicalize_airport_id(&packaged.asset.airport_id, airport_aliases);
        PlateRecord {
            id: packaged.asset.id.clone(),
            airport_id,
            icao_airport_id: packaged.asset.icao_airport_id.clone(),
            region_id: packaged.region_id.clone(),
            package_id: packaged.package_id.clone(),
            label: pretty_tpp_label(&packaged.asset.label, &packaged.asset.document_type),
            asset_kind: packaged.asset.asset_kind.clone(),
            document_type: packaged.asset.document_type.clone(),
            procedure_uid: packaged.asset.procedure_uid.clone(),
            georef: packaged.asset.georef.clone(),
            asset_path: packaged.asset.asset_path.clone(),
            thumbnail_path: packaged.asset.thumbnail_path.clone(),
        }
    })?;
    records.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(records)
}

fn collect_csup_records(
    sources: &[AssetSource],
    airport_aliases: &BTreeMap<String, String>,
    thumbnail_root: &Path,
) -> anyhow::Result<Vec<CsupRecord>> {
    let mut records =
        collect_asset_records_parallel(sources, "csup", thumbnail_root, |packaged| {
            let airport_id = canonicalize_airport_id(&packaged.asset.airport_id, airport_aliases);
            CsupRecord {
                id: packaged.asset.id.clone(),
                airport_id,
                region_id: packaged.region_id.clone(),
                package_id: packaged.package_id.clone(),
                label: "Chart Supplement".to_string(),
                thumbnail_path: packaged.asset.thumbnail_path.clone(),
                asset_kind: packaged.asset.asset_kind.clone(),
                document_type: packaged.asset.document_type.clone(),
                asset_path: packaged.asset.asset_path.clone(),
            }
        })?;
    records.sort();
    Ok(records)
}

fn collect_asset_records_parallel<T, F>(
    sources: &[AssetSource],
    expected_family_id: &str,
    thumbnail_root: &Path,
    build_record: F,
) -> anyhow::Result<Vec<T>>
where
    T: Send,
    F: Fn(&PackagedAssetEntry) -> T + Sync,
{
    let packaged_entries = sources
        .par_iter()
        .map(|source| packaged_asset_entries(source, expected_family_id))
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    packaged_entries
        .par_iter()
        .map(|packaged| {
            mirror_thumbnail_from_package(
                &packaged.package_zip_path,
                thumbnail_root,
                &packaged.asset.thumbnail_path,
            )?;
            Ok::<T, anyhow::Error>(build_record(packaged))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Clone)]
struct PackagedAssetEntry {
    package_id: String,
    region_id: String,
    package_zip_path: PathBuf,
    asset: PackageAssetRecord,
}

fn packaged_asset_entries(
    source: &AssetSource,
    expected_family_id: &str,
) -> anyhow::Result<Vec<PackagedAssetEntry>> {
    let mut entries = Vec::new();
    for record in read_package_outputs(&source.package_outputs_path)? {
        let package_zip_path = source.package_root.join(&record.zip);
        let manifest = read_package_asset_manifest(&package_zip_path)?;
        if manifest.family_id != expected_family_id {
            bail!(
                "unexpected package asset manifest family {} in {}",
                manifest.family_id,
                package_zip_path.display()
            );
        }
        let package_id = package_id_from_manifest_name(&record.manifest);
        if manifest.package_id != package_id {
            bail!(
                "package asset manifest id {} != package output manifest {} in {}",
                manifest.package_id,
                package_id,
                package_zip_path.display()
            );
        }
        for asset in manifest.assets {
            entries.push(PackagedAssetEntry {
                package_id: package_id.clone(),
                region_id: record.region.to_ascii_lowercase(),
                package_zip_path: package_zip_path.clone(),
                asset,
            });
        }
    }
    Ok(entries)
}

fn read_package_asset_manifest(package_zip_path: &Path) -> anyhow::Result<PackageAssetManifest> {
    let file = fs::File::open(package_zip_path)
        .with_context(|| format!("failed to open {}", package_zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", package_zip_path.display()))?;
    let mut entry = archive
        .by_name(PACKAGE_ASSET_MANIFEST_NAME)
        .with_context(|| {
            format!(
                "missing {} in {}",
                PACKAGE_ASSET_MANIFEST_NAME,
                package_zip_path.display()
            )
        })?;
    serde_json::from_reader(&mut entry).with_context(|| {
        format!(
            "failed to parse {} in {}",
            PACKAGE_ASSET_MANIFEST_NAME,
            package_zip_path.display()
        )
    })
}

fn mirror_thumbnail_from_package(
    package_zip_path: &Path,
    thumbnail_root: &Path,
    thumbnail_member_path: &str,
) -> anyhow::Result<()> {
    let target_path = thumbnail_root
        .parent()
        .unwrap_or(thumbnail_root)
        .join(thumbnail_member_path);
    if target_path.is_file() {
        return Ok(());
    }
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file = fs::File::open(package_zip_path)
        .with_context(|| format!("failed to open {}", package_zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", package_zip_path.display()))?;
    let mut entry = archive.by_name(thumbnail_member_path).with_context(|| {
        format!(
            "missing {thumbnail_member_path} in {}",
            package_zip_path.display()
        )
    })?;
    let mut output = fs::File::create(&target_path)
        .with_context(|| format!("failed to create {}", target_path.display()))?;
    std::io::copy(&mut entry, &mut output).with_context(|| {
        format!(
            "failed to extract {} from {}",
            thumbnail_member_path,
            package_zip_path.display()
        )
    })?;
    Ok(())
}

fn collect_airport_resources(
    plates: &[PlateRecord],
    csups: &[CsupRecord],
) -> Vec<AirportResourcesRecord> {
    let mut by_airport: BTreeMap<String, AirportResourcesRecord> = BTreeMap::new();
    for plate in plates {
        let entry = by_airport
            .entry(plate.airport_id.clone())
            .or_insert_with(|| AirportResourcesRecord {
                airport_id: plate.airport_id.clone(),
                plate_ids: Vec::new(),
                csup_ids: Vec::new(),
                package_ids: Vec::new(),
            });
        entry.plate_ids.push(plate.id.clone());
        entry.package_ids.push(plate.package_id.clone());
    }
    for csup in csups {
        let entry = by_airport
            .entry(csup.airport_id.clone())
            .or_insert_with(|| AirportResourcesRecord {
                airport_id: csup.airport_id.clone(),
                plate_ids: Vec::new(),
                csup_ids: Vec::new(),
                package_ids: Vec::new(),
            });
        entry.csup_ids.push(csup.id.clone());
        entry.package_ids.push(csup.package_id.clone());
    }
    let mut values = by_airport
        .into_values()
        .map(|mut entry| {
            entry.plate_ids.sort();
            entry.plate_ids.dedup();
            entry.csup_ids.sort();
            entry.csup_ids.dedup();
            entry.package_ids.sort();
            entry.package_ids.dedup();
            entry
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn read_package_outputs(path: &Path) -> anyhow::Result<Vec<PackageOutputRecord>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .context("failed to parse package output json")
        })
        .map(|value| {
            let value = value?;
            if value.get("event").and_then(|v| v.as_str()) != Some("package_output") {
                bail!("unexpected package output event: {value}");
            }
            Ok(PackageOutputRecord {
                label: value
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                chart: value
                    .get("chart")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned),
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
                metadata: value
                    .get("metadata")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn extract_sqlite_entry(nav_db_zip: &Path, entry_name: &str) -> anyhow::Result<NamedTempFile> {
    let file = fs::File::open(nav_db_zip)
        .with_context(|| format!("failed to open {}", nav_db_zip.display()))?;
    let mut archive = ZipArchive::new(file).context("failed to open nav db zip archive")?;
    let mut entry = archive
        .by_name(entry_name)
        .with_context(|| format!("missing {entry_name} in {}", nav_db_zip.display()))?;
    let mut temp = NamedTempFile::new().context("failed to create temp sqlite file")?;
    std::io::copy(&mut entry, &mut temp).context("failed to extract sqlite entry")?;
    Ok(temp)
}

fn read_nav_cycle_code(nav_db_zip: &Path) -> anyhow::Result<Option<String>> {
    let file = fs::File::open(nav_db_zip)
        .with_context(|| format!("failed to open {}", nav_db_zip.display()))?;
    let mut archive = ZipArchive::new(file).context("failed to open nav db zip archive")?;
    let mut entry = archive
        .by_name("databases")
        .with_context(|| format!("missing databases entry in {}", nav_db_zip.display()))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text)
        .context("failed to read databases entry")?;
    Ok(text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != INTERMEDIATE_SQLITE_BASENAME)
        .map(ToOwned::to_owned))
}

fn nav_db_version_label(nav_db_zip: &Path, temporal: &FaaTemporalMetadata) -> Option<String> {
    nav_db_zip
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| value.starts_with("data_"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            temporal
                .expiration_date
                .as_deref()
                .map(version_label_from_date)
        })
}

fn read_files_recursive(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fn is_hidden_name(path: &Path) -> bool {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.starts_with('.'))
            .unwrap_or(false)
    }

    fn visit(path: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory {}", path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to iterate {}", path.display()))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let child = entry.path();
            if is_hidden_name(&child) {
                continue;
            }
            if child.is_dir() {
                visit(&child, out)?;
            } else if child.is_file() {
                out.push(child);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, &mut files)?;
    Ok(files)
}

fn infer_cycle_from_manifest(manifest: &str) -> Option<String> {
    let cycle = manifest
        .split('_')
        .find(|part| part.len() == 4 && part.chars().all(|ch| ch.is_ascii_digit()))?;
    Some(cycle.to_string())
}

fn package_id_from_manifest_name(manifest: &str) -> String {
    manifest
        .strip_suffix(".manifest")
        .unwrap_or(manifest)
        .to_string()
}

fn build_temporal_summary(
    packages: &[ResourcePackage],
    nav_temporal: &FaaTemporalMetadata,
) -> TemporalSummary {
    let mut cycle_codes = BTreeSet::new();
    let mut effective_dates = BTreeSet::new();
    let mut expiration_dates = BTreeSet::new();

    if let Some(value) = &nav_temporal.cycle_code {
        cycle_codes.insert(value.clone());
    }
    if let Some(value) = &nav_temporal.effective_date {
        effective_dates.insert(value.clone());
    }
    if let Some(value) = &nav_temporal.expiration_date {
        expiration_dates.insert(value.clone());
    }
    for package in packages {
        if let Some(value) = &package.cycle_code {
            cycle_codes.insert(value.clone());
        }
        if let Some(value) = &package.effective_date {
            effective_dates.insert(value.clone());
        }
        if let Some(value) = &package.expiration_date {
            expiration_dates.insert(value.clone());
        }
    }

    let cycle_codes = cycle_codes.into_iter().collect::<Vec<_>>();
    let effective_dates = effective_dates.into_iter().collect::<Vec<_>>();
    let expiration_dates = expiration_dates.into_iter().collect::<Vec<_>>();
    TemporalSummary {
        uniform_cycle_code: singleton_value(&cycle_codes),
        uniform_effective_date: singleton_value(&effective_dates),
        uniform_expiration_date: singleton_value(&expiration_dates),
        uniform_good_beyond_date: effective_dates.last().cloned(),
        cycle_codes,
        effective_dates,
        expiration_dates,
    }
}

fn package_version_label(temporal: Option<&FaaTemporalMetadata>) -> Option<String> {
    let temporal = temporal?;
    if let Some(cycle_code) = &temporal.cycle_code {
        return Some(cycle_code.clone());
    }
    if temporal_cadence_days(temporal) == Some(56) {
        if let Some(effective_date) = &temporal.effective_date {
            let effective = NaiveDate::parse_from_str(effective_date, "%Y-%m-%d").ok()?;
            return cycle_code_from_effective_date(effective).ok();
        }
    }
    temporal
        .expiration_date
        .as_deref()
        .map(version_label_from_date)
}

fn temporal_cadence_days(temporal: &FaaTemporalMetadata) -> Option<i64> {
    let effective =
        NaiveDate::parse_from_str(temporal.effective_date.as_deref()?, "%Y-%m-%d").ok()?;
    let expiration =
        NaiveDate::parse_from_str(temporal.expiration_date.as_deref()?, "%Y-%m-%d").ok()?;
    Some((expiration - effective).num_days())
}

fn version_label_from_date(date: &str) -> String {
    date.replace('-', ".")
}

fn singleton_value(values: &[String]) -> Option<String> {
    if values.len() == 1 {
        Some(values[0].clone())
    } else {
        None
    }
}

fn infer_temporal_from_source_urls(
    source_urls_path: Option<&Path>,
) -> anyhow::Result<Option<FaaTemporalMetadata>> {
    let Some(path) = source_urls_path else {
        return Ok(None);
    };
    let urls = preprocessor_fetch::read_source_urls_jsonl(path)?;
    let mut cycle_codes = BTreeSet::new();
    let mut effective_dates = BTreeSet::new();
    let mut expiration_dates = BTreeSet::new();
    for url in urls {
        let Some(temporal) = temporal_from_url(&url)? else {
            continue;
        };
        if let Some(value) = temporal.cycle_code {
            cycle_codes.insert(value);
        }
        if let Some(value) = temporal.effective_date {
            effective_dates.insert(value);
        }
        if let Some(value) = temporal.expiration_date {
            expiration_dates.insert(value);
        }
    }
    if cycle_codes.is_empty() && effective_dates.is_empty() && expiration_dates.is_empty() {
        return Ok(None);
    }
    if cycle_codes.len() > 1 {
        bail!(
            "mixed FAA cycle codes in {}: {:?}",
            path.display(),
            cycle_codes
        );
    }
    if effective_dates.len() > 1 {
        bail!(
            "mixed effective dates in {}: {:?}",
            path.display(),
            effective_dates
        );
    }
    if expiration_dates.len() > 1 {
        bail!(
            "mixed expiration dates in {}: {:?}",
            path.display(),
            expiration_dates
        );
    }
    Ok(Some(FaaTemporalMetadata {
        cycle_code: cycle_codes.into_iter().next(),
        effective_date: effective_dates.into_iter().next(),
        expiration_date: expiration_dates.into_iter().next(),
    }))
}

fn temporal_from_url(url: &str) -> anyhow::Result<Option<FaaTemporalMetadata>> {
    let url = url.split('#').next().unwrap_or(url);
    if let Some(date) = extract_between(url, "/visual/", "/") {
        let effective = parse_date(&date, "%m-%d-%Y")?;
        return Ok(Some(temporal_from_effective_date(effective, 56, None)));
    }
    if let Some(date) = extract_between(url, "/enroute/", "/") {
        let effective = parse_date(&date, "%m-%d-%Y")?;
        return Ok(Some(temporal_from_effective_date(effective, 56, None)));
    }
    if let Some(compact) = extract_suffix_between(url, "DCS_", ".zip") {
        let effective = parse_date(&compact, "%Y%m%d")?;
        return Ok(Some(temporal_from_effective_date(effective, 56, None)));
    }
    if let Some(date) = extract_suffix_between(url, "28DaySubscription_Effective_", ".zip") {
        let effective = parse_date(&date, "%Y-%m-%d")?;
        return Ok(Some(temporal_from_effective_date(
            effective,
            28,
            Some(cycle_code_from_effective_date(effective)?),
        )));
    }
    if let Some(date) = extract_between(url, "/28DaySub/", "/aixm5.0.zip") {
        let effective = parse_date(&date, "%Y-%m-%d")?;
        return Ok(Some(temporal_from_effective_date(
            effective,
            28,
            Some(cycle_code_from_effective_date(effective)?),
        )));
    }
    if let Some(compact) = extract_suffix_between(url, "CIFP_", ".zip") {
        if compact.len() == 6 && compact.chars().all(|ch| ch.is_ascii_digit()) {
            let effective = parse_date(
                &format!("20{}-{}-{}", &compact[0..2], &compact[2..4], &compact[4..6]),
                "%Y-%m-%d",
            )?;
            return Ok(Some(temporal_from_effective_date(
                effective,
                28,
                Some(compact[0..4].to_string()),
            )));
        }
    }
    if let Some(compact) = url
        .split('/')
        .next_back()
        .and_then(|name| name.strip_suffix(".zip"))
        .and_then(|name| name.rsplit('_').next())
    {
        if compact.len() == 6
            && compact.chars().all(|ch| ch.is_ascii_digit())
            && url.contains("DDTPP")
        {
            let effective = parse_date(
                &format!("20{}-{}-{}", &compact[0..2], &compact[2..4], &compact[4..6]),
                "%Y-%m-%d",
            )?;
            return Ok(Some(temporal_from_effective_date(
                effective,
                28,
                Some(compact[0..4].to_string()),
            )));
        }
    }
    Ok(None)
}

fn temporal_from_cycle_code(cycle_code: &str) -> Option<FaaTemporalMetadata> {
    let effective = effective_date_from_cycle_code(cycle_code).ok()?;
    Some(temporal_from_effective_date(
        effective,
        28,
        Some(cycle_code.to_string()),
    ))
}

fn temporal_from_effective_date(
    effective: NaiveDate,
    cadence_days: i64,
    cycle_code: Option<String>,
) -> FaaTemporalMetadata {
    FaaTemporalMetadata {
        cycle_code,
        effective_date: Some(effective.format("%Y-%m-%d").to_string()),
        expiration_date: Some(
            (effective + Duration::days(cadence_days))
                .format("%Y-%m-%d")
                .to_string(),
        ),
    }
}

fn parse_date(value: &str, format: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(value, format)
        .with_context(|| format!("failed to parse FAA date {value} with {format}"))
}

fn extract_between(value: &str, prefix: &str, suffix: &str) -> Option<String> {
    let tail = value.split_once(prefix)?.1;
    Some(tail.split_once(suffix)?.0.to_string())
}

fn extract_suffix_between(value: &str, prefix: &str, suffix: &str) -> Option<String> {
    let tail = value.rsplit_once(prefix)?.1;
    Some(tail.split_once(suffix)?.0.to_string())
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

fn effective_date_from_cycle_code(cycle_code: &str) -> anyhow::Result<NaiveDate> {
    if cycle_code.len() != 4 || !cycle_code.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("invalid FAA cycle code {cycle_code}");
    }
    let year = 2000
        + cycle_code[0..2]
            .parse::<i32>()
            .context("invalid FAA cycle year")?;
    let cycle = cycle_code[2..4]
        .parse::<u32>()
        .context("invalid FAA cycle number")?;
    let first_date =
        first_cycle_day(year).ok_or_else(|| anyhow::anyhow!("unsupported cycle year {year}"))?;
    let first = NaiveDate::from_ymd_opt(year, 1, first_date)
        .ok_or_else(|| anyhow::anyhow!("invalid first cycle day for {year}"))?;
    Ok(first + Duration::days(28 * i64::from(cycle.saturating_sub(1))))
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

#[derive(Debug, Clone, PartialEq)]
struct ChartZipMetadata {
    chart_index: u32,
    levels: Vec<TileLevelRecord>,
    tiles: BTreeSet<(u32, u32, u32)>,
    coverage_bounds: CoverageBounds,
    default_view: DefaultView,
}

fn read_chart_zip_metadata(path: &Path) -> anyhow::Result<ChartZipMetadata> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let archive = ZipArchive::new(file).context("failed to open chart zip")?;
    let mut tiles_by_zoom: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut tiles = BTreeSet::new();
    let mut chart_index = None;
    for name in archive.file_names() {
        if !name.ends_with(".webp") {
            continue;
        }
        let parts = name.split('/').collect::<Vec<_>>();
        if parts.len() != 5 || parts[0] != "tiles" {
            continue;
        }
        let parsed_chart_index: u32 = parts[1].parse().context("invalid chart index")?;
        let zoom: u32 = parts[2].parse().context("invalid zoom")?;
        let x: u32 = parts[3].parse().context("invalid x tile")?;
        let y_tms: u32 = parts[4]
            .strip_suffix(".webp")
            .context("missing webp suffix")?
            .parse()
            .context("invalid tms y tile")?;
        chart_index.get_or_insert(parsed_chart_index);
        tiles.insert((zoom, x, y_tms));
        tiles_by_zoom.entry(zoom).or_default().insert((x, y_tms));
    }
    let chart_index = chart_index.context("no tile entries found in chart zip")?;
    let level_records = tiles_by_zoom
        .into_iter()
        .map(|(zoom, tiles)| TileLevelRecord {
            zoom,
            boxes: tile_run_boxes(&tiles),
        })
        .collect::<Vec<_>>();
    let coverage_bounds = coverage_bounds_from_levels(&level_records)
        .context("failed to derive coverage bounds from levels")?;
    let default_view = default_view_from_levels(&level_records)
        .context("failed to derive default view from levels")?;
    Ok(ChartZipMetadata {
        chart_index,
        levels: level_records,
        tiles,
        coverage_bounds,
        default_view,
    })
}

fn coverage_bounds_from_levels(levels: &[TileLevelRecord]) -> Option<CoverageBounds> {
    let level = levels.iter().max_by_key(|level| level.zoom)?;
    let bounds = level_bounds(level)?;
    let scale = 2_u32.pow(level.zoom) as f64;
    let y_xyz_min = (scale as u32 - 1 - bounds.y_tms_max) as f64;
    let y_xyz_max = (scale as u32 - 1 - bounds.y_tms_min) as f64;
    let lon_min = tile_x_to_lon(bounds.x_min as f64, scale);
    let lon_max = tile_x_to_lon((bounds.x_max + 1) as f64, scale);
    let lat_max = tile_y_to_lat(y_xyz_min, scale);
    let lat_min = tile_y_to_lat(y_xyz_max + 1.0, scale);
    Some(CoverageBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    })
}

fn default_view_from_levels(levels: &[TileLevelRecord]) -> Option<DefaultView> {
    let level = levels.iter().max_by_key(|level| level.zoom)?;
    let bounds = level_bounds(level)?;
    let scale = 2_u32.pow(level.zoom) as f64;
    let center_x = ((bounds.x_min + bounds.x_max + 1) as f64) / 2.0;
    let center_y_tms = ((bounds.y_tms_min + bounds.y_tms_max + 1) as f64) / 2.0;
    let center_y_xyz = scale - center_y_tms;
    Some(DefaultView {
        lat: tile_y_to_lat(center_y_xyz, scale),
        lon: tile_x_to_lon(center_x, scale),
        zoom: f64::from(level.zoom) - 2.0,
    })
}

fn level_bounds(level: &TileLevelRecord) -> Option<TileBoundsRecord> {
    let first = level.boxes.first()?;
    let mut bounds = first.clone();
    for bbox in &level.boxes[1..] {
        bounds.x_min = bounds.x_min.min(bbox.x_min);
        bounds.x_max = bounds.x_max.max(bbox.x_max);
        bounds.y_tms_min = bounds.y_tms_min.min(bbox.y_tms_min);
        bounds.y_tms_max = bounds.y_tms_max.max(bbox.y_tms_max);
    }
    Some(bounds)
}

fn tile_run_boxes(tiles: &BTreeSet<(u32, u32)>) -> Vec<TileBoundsRecord> {
    let mut row_runs: BTreeMap<(u32, u32), Vec<TileBoundsRecord>> = BTreeMap::new();
    let mut row_tiles: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (x, y_tms) in tiles {
        row_tiles.entry(*y_tms).or_default().push(*x);
    }

    for (y_tms, mut xs) in row_tiles {
        xs.sort_unstable();
        xs.dedup();
        let Some(mut x_min) = xs.first().copied() else {
            continue;
        };
        let mut previous = x_min;
        for x in xs.into_iter().skip(1) {
            if x == previous + 1 {
                previous = x;
                continue;
            }
            row_runs
                .entry((x_min, previous))
                .or_default()
                .push(TileBoundsRecord {
                    x_min,
                    x_max: previous,
                    y_tms_min: y_tms,
                    y_tms_max: y_tms,
                });
            x_min = x;
            previous = x;
        }
        row_runs
            .entry((x_min, previous))
            .or_default()
            .push(TileBoundsRecord {
                x_min,
                x_max: previous,
                y_tms_min: y_tms,
                y_tms_max: y_tms,
            });
    }

    let mut boxes: Vec<TileBoundsRecord> = Vec::new();
    for mut runs in row_runs.into_values() {
        runs.sort_by_key(|run| run.y_tms_min);
        for run in runs {
            if let Some(last) = boxes.last_mut() {
                if last.x_min == run.x_min
                    && last.x_max == run.x_max
                    && last.y_tms_max + 1 == run.y_tms_min
                {
                    last.y_tms_max = run.y_tms_max;
                    continue;
                }
            }
            boxes.push(run);
        }
    }
    boxes.sort();
    boxes
}

fn tile_x_to_lon(tile_x: f64, scale: f64) -> f64 {
    (tile_x / scale) * 360.0 - 180.0
}

fn tile_y_to_lat(tile_y_xyz: f64, scale: f64) -> f64 {
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * tile_y_xyz) / scale;
    n.sinh().atan().to_degrees()
}

fn family_display_name(id: &str) -> &'static str {
    match id {
        "sec" => "Sectional",
        "tac" => "TAC",
        "enr-l" => "IFR Low",
        "enr-h" => "IFR High",
        "tpp" => "TPP",
        "csup" => "CSUP",
        _ => "Unknown",
    }
}

fn family_kind(id: &str) -> &'static str {
    match id {
        "sec" | "tac" | "enr-l" | "enr-h" => "tiled_raster",
        "tpp" | "csup" => "flat_image",
        _ => "unknown",
    }
}

fn region_display_name(region: Region) -> &'static str {
    match region {
        Region::Ak => "Alaska",
        Region::Pac => "Pacific",
        Region::Nw => "Northwest",
        Region::Sw => "Southwest",
        Region::Nc => "North Central",
        Region::Ec => "East Coast",
        Region::Sc => "South Central",
        Region::Ne => "Northeast",
        Region::Se => "Southeast",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba, RgbaImage};
    use rusqlite::Connection;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_test_png(path: impl AsRef<Path>, width: u32, height: u32) {
        let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        image.save(path).expect("write test png");
    }

    fn test_png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encode test png");
        bytes
    }

    fn write_chart_zip(path: &Path, tiles: &[(u32, u32, u32)]) {
        let file = fs::File::create(path).expect("create chart zip");
        let mut zip = ZipWriter::new(file);
        for (z, x, y_tms) in tiles {
            zip.start_file(
                format!("tiles/0/{z}/{x}/{y_tms}.webp"),
                SimpleFileOptions::default(),
            )
            .expect("start tile");
            zip.write_all(b"tile").expect("write tile");
        }
        zip.finish().expect("finish chart zip");
    }

    // TODO: if you touch this test, you need to refactor away the synthetic
    // legacy sidecar/work-layout fixture. It still models a loose plate `.tif`
    // beside the PNG and hand-built package/work roots instead of relying
    // purely on package-assets manifests and the current published contract.
    #[test]
    fn builds_index_from_realistic_inputs() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join(INTERMEDIATE_SQLITE_BASENAME);
        let conn = Connection::open(&db_path).expect("open sqlite");
        conn.execute_batch(
            "create table airports (
                LocationID text,
                ARPLatitude float,
                ARPLongitude float,
                Type text,
                FacilityName text
            );
            create table airport_aliases (
                alias_id text,
                airport_id text
            );
            insert into airports values ('KBOS', 42.3656, -71.0096, 'AIRPORT', 'Boston Logan');
            insert into airports values ('KRNT', 47.4931, -122.2160, 'AIRPORT', 'Renton');",
        )
        .expect("seed airports");
        conn.execute("insert into airport_aliases values ('BOS', 'KBOS')", [])
            .expect("seed airport alias");
        conn.execute("insert into airport_aliases values ('KBOS', 'KBOS')", [])
            .expect("seed airport alias");

        let nav_zip = temp.path().join("databases.zip");
        {
            let file = fs::File::create(&nav_zip).expect("create zip");
            let mut zip = ZipWriter::new(file);
            zip.start_file("databases", SimpleFileOptions::default())
                .expect("start databases entry");
            zip.write_all(format!("2604\n{}\n", INTERMEDIATE_SQLITE_BASENAME).as_bytes())
                .expect("write databases entry");
            zip.start_file(INTERMEDIATE_SQLITE_BASENAME, SimpleFileOptions::default())
                .expect("start sqlite entry");
            zip.write_all(&fs::read(&db_path).expect("read db"))
                .expect("write sqlite bytes");
            zip.finish().expect("finish zip");
        }

        let chart_root = temp.path().join("charts-sec");
        fs::create_dir_all(&chart_root).expect("chart root");
        {
            let file = fs::File::create(chart_root.join("NW_SEC.zip")).expect("chart zip");
            let mut zip = ZipWriter::new(file);
            zip.start_file("tiles/0/7/20/50.webp", SimpleFileOptions::default())
                .expect("start chart tile");
            zip.write_all(b"webp").expect("write chart tile");
            zip.start_file("tiles/0/7/21/50.webp", SimpleFileOptions::default())
                .expect("start chart tile");
            zip.write_all(b"webp").expect("write chart tile");
            zip.start_file("tiles/0/7/20/49.webp", SimpleFileOptions::default())
                .expect("start chart tile");
            zip.write_all(b"webp").expect("write chart tile");
            zip.start_file("tiles/0/7/21/49.webp", SimpleFileOptions::default())
                .expect("start chart tile");
            zip.write_all(b"webp").expect("write chart tile");
            zip.finish().expect("finish chart zip");
        }
        let chart_outputs = temp.path().join("chart-package-outputs.jsonl");
        fs::write(
            &chart_outputs,
            "{\"event\":\"package_output\",\"label\":\"charts-sec\",\"chart\":\"SEC\",\"manifest\":\"NW_SEC\",\"manifest_sha256\":\"m\",\"region\":\"NW\",\"zip\":\"NW_SEC.zip\",\"zip_sha256\":\"abc\"}\n",
        )
        .expect("chart outputs");
        let chart_source_urls = temp.path().join("chart-source-urls.jsonl");
        fs::write(
            &chart_source_urls,
            "{\"event\":\"list_crawl\",\"label\":\"charts-sec\",\"match\":\"x\",\"results\":[\"https://aeronav.faa.gov/visual/03-19-2026/sectional-files/Seattle.zip\"],\"url\":\"https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/\"}\n",
        )
        .expect("chart source urls");

        let tpp_root = temp.path().join("tpp-ne");
        fs::create_dir_all(tpp_root.join("plates/BOS")).expect("tpp root");
        {
            let file = fs::File::create(tpp_root.join("NE_TPP.zip")).expect("tpp zip");
            let mut zip = ZipWriter::new(file);
            let tpp_manifest = PackageAssetManifest {
                schema_version: 2,
                family_id: "tpp".to_string(),
                package_id: "NE_TPP".to_string(),
                assets: vec![PackageAssetRecord {
                    id: "plate:KBOS:IAP-MA-ILS OR LOC RWY 04R.png".to_string(),
                    airport_id: "BOS".to_string(),
                    icao_airport_id: None,
                    label: "IAP-MA-ILS OR LOC RWY 04R".to_string(),
                    asset_kind: "plate".to_string(),
                    document_type: "approach".to_string(),
                    asset_path: "plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png".to_string(),
                    thumbnail_path: "thumbnails/plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png"
                        .to_string(),
                    procedure_uid: None,
                    georef: None,
                }],
            };
            zip.start_file(PACKAGE_ASSET_MANIFEST_NAME, SimpleFileOptions::default())
                .expect("start tpp asset manifest");
            zip.write_all(
                serde_json::to_vec_pretty(&tpp_manifest)
                    .expect("serialize tpp asset manifest")
                    .as_slice(),
            )
            .expect("write tpp asset manifest");
            zip.start_file(
                "plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png",
                SimpleFileOptions::default(),
            )
            .expect("start tpp png");
            zip.write_all(b"png").expect("write tpp png");
            zip.start_file(
                "thumbnails/plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png",
                SimpleFileOptions::default(),
            )
            .expect("start tpp thumb");
            zip.write_all(&test_png_bytes(100, 150))
                .expect("write tpp thumb");
            zip.finish().expect("finish tpp zip");
        }
        write_test_png(
            tpp_root.join("plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png"),
            200,
            300,
        );
        fs::write(
            tpp_root.join("plates/BOS/IAP-MA-ILS OR LOC RWY 04R.tif"),
            b"tif",
        )
        .expect("plate tif sidecar");
        let tpp_outputs = temp.path().join("tpp-package-outputs.jsonl");
        fs::write(
            &tpp_outputs,
            "{\"event\":\"package_output\",\"label\":\"tpp-ne\",\"manifest\":\"NE_TPP\",\"manifest_sha256\":\"m\",\"region\":\"NE\",\"zip\":\"NE_TPP.zip\",\"zip_sha256\":\"def\"}\n",
        )
        .expect("tpp outputs");
        let tpp_source_urls = temp.path().join("tpp-source-urls.jsonl");
        fs::write(
            &tpp_source_urls,
            "{\"event\":\"list_crawl\",\"label\":\"tpp-ne\",\"match\":\"x\",\"results\":[\"https://aeronav.faa.gov/upload_313-d/terminal/DDTPPA_260416.zip\"],\"url\":\"https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/\"}\n",
        )
        .expect("tpp source urls");

        let csup_root = temp.path().join("csup");
        fs::create_dir_all(csup_root.join("afd/BOS")).expect("csup root");
        {
            let file = fs::File::create(csup_root.join("NE_CSUP.zip")).expect("csup zip");
            let mut zip = ZipWriter::new(file);
            let csup_manifest = PackageAssetManifest {
                schema_version: 2,
                family_id: "csup".to_string(),
                package_id: "NE_CSUP".to_string(),
                assets: vec![PackageAssetRecord {
                    id: "csup:KBOS:CSUP-NE_0-0.png".to_string(),
                    airport_id: "BOS".to_string(),
                    icao_airport_id: None,
                    label: "CSUP-NE_0-0".to_string(),
                    asset_kind: "csup_page".to_string(),
                    document_type: "csup".to_string(),
                    asset_path: "afd/BOS/CSUP-NE_0-0.png".to_string(),
                    thumbnail_path: "thumbnails/afd/BOS/CSUP-NE_0-0.png".to_string(),
                    procedure_uid: None,
                    georef: None,
                }],
            };
            zip.start_file(PACKAGE_ASSET_MANIFEST_NAME, SimpleFileOptions::default())
                .expect("start csup asset manifest");
            zip.write_all(
                serde_json::to_vec_pretty(&csup_manifest)
                    .expect("serialize csup asset manifest")
                    .as_slice(),
            )
            .expect("write csup asset manifest");
            zip.start_file("afd/BOS/CSUP-NE_0-0.png", SimpleFileOptions::default())
                .expect("start csup png");
            zip.write_all(b"png").expect("write csup png");
            zip.start_file(
                "thumbnails/afd/BOS/CSUP-NE_0-0.png",
                SimpleFileOptions::default(),
            )
            .expect("start csup thumb");
            zip.write_all(&test_png_bytes(100, 150))
                .expect("write csup thumb");
            zip.finish().expect("finish csup zip");
        }
        write_test_png(csup_root.join("afd/BOS/CSUP-NE_0-0.png"), 300, 200);
        let csup_outputs = temp.path().join("csup-package-outputs.jsonl");
        fs::write(
            &csup_outputs,
            "{\"event\":\"package_output\",\"label\":\"csup\",\"manifest\":\"NE_CSUP\",\"manifest_sha256\":\"m\",\"region\":\"NE\",\"zip\":\"NE_CSUP.zip\",\"zip_sha256\":\"ghi\"}\n",
        )
        .expect("csup outputs");
        let csup_source_urls = temp.path().join("csup-source-urls.jsonl");
        fs::write(
            &csup_source_urls,
            "{\"event\":\"list_crawl\",\"label\":\"csup\",\"match\":\"x\",\"results\":[\"https://aeronav.faa.gov/Upload_313-d/supplements/DCS_20260319.zip\"],\"url\":\"https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/\"}\n",
        )
        .expect("csup source urls");

        let request = BuildResourceIndexRequest {
            nav_db_zip: nav_zip.clone(),
            output_path: temp
                .path()
                .join("product-builds/test/work/resource-index/resource-index.json"),
            catalog_output_path: None,
            chart_sources: vec![ChartSource {
                family_id: "sec".to_string(),
                package_outputs_path: chart_outputs,
                asset_root: chart_root.clone(),
                package_root: chart_root.clone(),
                unpack_source_root: chart_root,
                source_urls_path: Some(chart_source_urls),
            }],
            tpp_sources: vec![AssetSource {
                package_outputs_path: tpp_outputs,
                asset_root: tpp_root.clone(),
                package_root: temp.path().join("tpp-ne"),
                unpack_source_root: tpp_root,
                source_urls_path: Some(tpp_source_urls),
            }],
            csup_sources: vec![AssetSource {
                package_outputs_path: csup_outputs,
                asset_root: csup_root.clone(),
                package_root: temp.path().join("csup"),
                unpack_source_root: csup_root,
                source_urls_path: Some(csup_source_urls),
            }],
        };

        let index = write_resource_index(&request).expect("build index");
        assert_eq!(index.cycle.as_deref(), Some("2604"));
        assert_eq!(
            index.temporal_summary.uniform_cycle_code.as_deref(),
            Some("2604")
        );
        assert_eq!(
            index.temporal_summary.effective_dates,
            vec!["2026-03-19".to_string(), "2026-04-16".to_string()]
        );
        assert_eq!(
            index.temporal_summary.expiration_dates,
            vec!["2026-05-14".to_string()]
        );
        assert_eq!(
            index.temporal_summary.uniform_good_beyond_date.as_deref(),
            Some("2026-04-16")
        );
        assert_eq!(index.nav_db.sqlite_entry, INTERMEDIATE_SQLITE_BASENAME);
        assert_eq!(index.nav_db.cycle_code.as_deref(), Some("2604"));
        assert_eq!(index.nav_db.version_label.as_deref(), Some("2026.05.14"));
        assert_eq!(index.nav_db.effective_date.as_deref(), Some("2026-04-16"));
        assert_eq!(index.nav_db.expiration_date.as_deref(), Some("2026-05-14"));
        assert_eq!(index.airports.len(), 2);
        assert_eq!(index.packages[0].id, "NE_CSUP");
        assert!(index
            .packages
            .iter()
            .any(|package| package.family_id == "sec"));
        assert!(index.packages.iter().any(|package| {
            package.family_id == "sec"
                && package.effective_date.as_deref() == Some("2026-03-19")
                && package.expiration_date.as_deref() == Some("2026-05-14")
                && package.cycle_code.is_none()
                && package.version_label.as_deref() == Some("2603")
        }));
        assert!(index
            .packages
            .iter()
            .any(|package| package.family_id == "tpp"));
        assert!(index.packages.iter().any(|package| {
            package.family_id == "tpp"
                && package.effective_date.as_deref() == Some("2026-04-16")
                && package.expiration_date.as_deref() == Some("2026-05-14")
                && package.cycle_code.as_deref() == Some("2604")
                && package.version_label.as_deref() == Some("2604")
        }));
        assert!(index
            .packages
            .iter()
            .any(|package| package.family_id == "csup"));
        assert!(index.packages.iter().any(|package| {
            package.family_id == "csup"
                && package.effective_date.as_deref() == Some("2026-03-19")
                && package.expiration_date.as_deref() == Some("2026-05-14")
                && package.cycle_code.is_none()
                && package.version_label.as_deref() == Some("2603")
        }));
        assert_eq!(index.chart_collections.len(), 1);
        assert_eq!(index.chart_collections[0].family_id, "sec");
        assert_eq!(index.chart_collections[0].region_id, "nw");
        assert_eq!(index.chart_collections[0].package_id, "NW_SEC");
        assert_eq!(index.chart_collections[0].chart_index, 0);
        assert_eq!(
            index.chart_collections[0].tile_path_template,
            "tiles/0/{z}/{x}/{y}.webp"
        );
        assert_eq!(
            index.chart_collections[0].levels,
            vec![TileLevelRecord {
                zoom: 7,
                boxes: vec![TileBoundsRecord {
                    x_min: 20,
                    x_max: 21,
                    y_tms_min: 49,
                    y_tms_max: 50,
                }],
            }]
        );
        assert!(
            index.chart_collections[0].coverage_bounds.lon_min
                < index.chart_collections[0].coverage_bounds.lon_max
        );
        assert!(
            index.chart_collections[0].coverage_bounds.lat_min
                < index.chart_collections[0].coverage_bounds.lat_max
        );
        assert_eq!(
            index.plates[0].id,
            "plate:KBOS:IAP-MA-ILS OR LOC RWY 04R.png"
        );
        assert_eq!(index.plates[0].airport_id, "KBOS");
        assert_eq!(index.plates[0].package_id, "NE_TPP");
        assert_eq!(index.plates[0].label, "ILS or LOC 04R");
        assert_eq!(index.plates[0].document_type, "approach");
        assert_eq!(
            index.plates[0].thumbnail_path,
            "thumbnails/plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png"
        );
        assert_eq!(index.plates.len(), 1);
        assert_eq!(index.csups[0].id, "csup:KBOS:CSUP-NE_0-0.png");
        assert_eq!(index.csups[0].airport_id, "KBOS");
        assert_eq!(index.csups[0].region_id, "ne");
        assert_eq!(index.csups[0].package_id, "NE_CSUP");
        assert_eq!(index.csups[0].label, "Chart Supplement");
        assert_eq!(index.csups[0].document_type, "csup");
        assert_eq!(
            index.csups[0].thumbnail_path,
            "thumbnails/afd/BOS/CSUP-NE_0-0.png"
        );
        assert_eq!(index.airport_resources.len(), 1);
        assert_eq!(index.airport_resources[0].airport_id, "KBOS");
        assert_eq!(
            index.airport_resources[0].plate_ids,
            vec!["plate:KBOS:IAP-MA-ILS OR LOC RWY 04R.png"]
        );
        assert_eq!(
            index.airport_resources[0].csup_ids,
            vec!["csup:KBOS:CSUP-NE_0-0.png"]
        );
        assert_eq!(
            index.airport_resources[0].package_ids,
            vec!["NE_CSUP", "NE_TPP"]
        );
        assert!(request.output_path.exists());
        let thumbnail_root = request
            .output_path
            .parent()
            .expect("resource-index parent")
            .join("thumbnails");
        let plate_thumb =
            image::open(thumbnail_root.join("plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png"))
                .expect("open plate thumbnail");
        assert_eq!(plate_thumb.dimensions(), (100, 150));
        let csup_thumb = image::open(thumbnail_root.join("afd/BOS/CSUP-NE_0-0.png"))
            .expect("open csup thumbnail");
        assert_eq!(csup_thumb.dimensions(), (100, 150));
    }

    #[test]
    fn chart_zip_metadata_keeps_disconnected_pac_boxes() {
        let temp = tempdir().expect("temp dir");
        let zip_path = temp.path().join("pac.zip");
        write_chart_zip(
            &zip_path,
            &[(8, 12, 141), (8, 13, 141), (8, 7, 117), (8, 231, 137)],
        );

        let metadata = read_chart_zip_metadata(&zip_path).expect("metadata");
        let z8 = metadata
            .levels
            .iter()
            .find(|level| level.zoom == 8)
            .expect("z8");

        assert_eq!(
            z8.boxes,
            vec![
                TileBoundsRecord {
                    x_min: 7,
                    x_max: 7,
                    y_tms_min: 117,
                    y_tms_max: 117,
                },
                TileBoundsRecord {
                    x_min: 12,
                    x_max: 13,
                    y_tms_min: 141,
                    y_tms_max: 141,
                },
                TileBoundsRecord {
                    x_min: 231,
                    x_max: 231,
                    y_tms_min: 137,
                    y_tms_max: 137,
                },
            ]
        );
    }

    #[test]
    fn chart_zip_metadata_keeps_antimeridian_split_boxes() {
        let temp = tempdir().expect("temp dir");
        let zip_path = temp.path().join("ak.zip");
        write_chart_zip(
            &zip_path,
            &[(7, 0, 85), (7, 1, 85), (7, 126, 85), (7, 127, 85)],
        );

        let metadata = read_chart_zip_metadata(&zip_path).expect("metadata");
        let z7 = metadata
            .levels
            .iter()
            .find(|level| level.zoom == 7)
            .expect("z7");

        assert_eq!(
            z7.boxes,
            vec![
                TileBoundsRecord {
                    x_min: 0,
                    x_max: 1,
                    y_tms_min: 85,
                    y_tms_max: 85,
                },
                TileBoundsRecord {
                    x_min: 126,
                    x_max: 127,
                    y_tms_min: 85,
                    y_tms_max: 85,
                },
            ]
        );
    }

    #[test]
    fn chart_zip_metadata_does_not_fill_component_holes() {
        let temp = tempdir().expect("temp dir");
        let zip_path = temp.path().join("l_shape.zip");
        write_chart_zip(
            &zip_path,
            &[(8, 10, 20), (8, 11, 20), (8, 12, 20), (8, 10, 21)],
        );

        let metadata = read_chart_zip_metadata(&zip_path).expect("metadata");
        let z8 = metadata
            .levels
            .iter()
            .find(|level| level.zoom == 8)
            .expect("z8");

        assert_eq!(
            z8.boxes,
            vec![
                TileBoundsRecord {
                    x_min: 10,
                    x_max: 10,
                    y_tms_min: 21,
                    y_tms_max: 21,
                },
                TileBoundsRecord {
                    x_min: 10,
                    x_max: 12,
                    y_tms_min: 20,
                    y_tms_max: 20,
                },
            ]
        );
    }

    #[test]
    fn chart_tile_bbox_invariant_catches_overlapping_region_drift() {
        let left = ChartCollectionWithTiles {
            collection: ChartCollectionRecord {
                id: "sec:left".to_string(),
                family_id: "sec".to_string(),
                region_id: "left".to_string(),
                package_id: "SEC_LEFT".to_string(),
                chart_index: 0,
                tile_path_template: "tiles/0/{z}/{x}/{y}.webp".to_string(),
                levels: vec![TileLevelRecord {
                    zoom: 8,
                    boxes: vec![TileBoundsRecord {
                        x_min: 10,
                        x_max: 12,
                        y_tms_min: 20,
                        y_tms_max: 20,
                    }],
                }],
                coverage_bounds: CoverageBounds {
                    lat_min: 0.0,
                    lat_max: 1.0,
                    lon_min: 0.0,
                    lon_max: 1.0,
                },
                default_view: DefaultView {
                    lat: 0.0,
                    lon: 0.0,
                    zoom: 8.0,
                },
            },
            tiles: BTreeSet::from([(8, 10, 20), (8, 12, 20)]),
        };
        let right = ChartCollectionWithTiles {
            collection: ChartCollectionRecord {
                id: "sec:right".to_string(),
                family_id: "sec".to_string(),
                region_id: "right".to_string(),
                package_id: "SEC_RIGHT".to_string(),
                chart_index: 0,
                tile_path_template: "tiles/0/{z}/{x}/{y}.webp".to_string(),
                levels: vec![TileLevelRecord {
                    zoom: 8,
                    boxes: vec![TileBoundsRecord {
                        x_min: 11,
                        x_max: 11,
                        y_tms_min: 20,
                        y_tms_max: 20,
                    }],
                }],
                coverage_bounds: CoverageBounds {
                    lat_min: 0.0,
                    lat_max: 1.0,
                    lon_min: 0.0,
                    lon_max: 1.0,
                },
                default_view: DefaultView {
                    lat: 0.0,
                    lon: 0.0,
                    zoom: 8.0,
                },
            },
            tiles: BTreeSet::from([(8, 11, 20)]),
        };

        let err = validate_chart_tile_bbox_invariant(&[left, right])
            .expect_err("invariant should catch left bbox containing right-only tile");
        assert!(err.to_string().contains("chart tile bbox invariant failed"));
    }

    #[test]
    fn pretty_tpp_label_trims_hotspot_prefix() {
        assert_eq!(
            pretty_tpp_label("HOT-WA-HOT SPOT-0", "hotspot"),
            "Hot Spot 0"
        );
        assert_eq!(
            pretty_tpp_label("HOT-WA-HOT SPOT-1", "hotspot"),
            "Hot Spot 1"
        );
        assert_eq!(pretty_tpp_label("HOT-WA-HOT SPOT", "hotspot"), "Hot Spot");
    }
}
