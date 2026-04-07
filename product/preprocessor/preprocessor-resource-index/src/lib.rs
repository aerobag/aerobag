use anyhow::{Context, bail};
use chrono::Utc;
use preprocessor_core::Region;
use preprocessor_fetch::PackageOutputRecord;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildResourceIndexRequest {
    pub nav_db_zip: PathBuf,
    pub output_path: PathBuf,
    pub chart_sources: Vec<ChartSource>,
    pub tpp_sources: Vec<AssetSource>,
    pub csup_sources: Vec<AssetSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartSource {
    pub family_id: String,
    pub package_outputs_path: PathBuf,
    pub package_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSource {
    pub package_outputs_path: PathBuf,
    pub asset_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceIndex {
    pub schema_version: u32,
    pub cycle: Option<String>,
    pub generated_at_utc: String,
    pub nav_db: NavDbRef,
    pub families: Vec<ResourceFamily>,
    pub regions: Vec<ResourceRegion>,
    pub packages: Vec<ResourcePackage>,
    pub chart_collections: Vec<ChartCollectionRecord>,
    pub airports: Vec<AirportRecord>,
    pub plates: Vec<PlateRecord>,
    pub csups: Vec<CsupRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NavDbRef {
    pub artifact_path: String,
    pub sqlite_entry: String,
    pub cycle_code: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourcePackage {
    pub family_id: String,
    pub region_id: String,
    pub manifest_name: String,
    pub artifact_path: String,
    pub size_bytes: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartCollectionRecord {
    pub id: String,
    pub family_id: String,
    pub region_id: String,
    pub package_name: String,
    pub chart_index: u32,
    pub tile_path_template: String,
    pub levels: Vec<TileLevelRecord>,
    pub coverage_bounds: CoverageBounds,
    pub default_view: DefaultView,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileLevelRecord {
    pub zoom: u32,
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
pub struct PlateRecord {
    pub airport_id: String,
    pub region_id: String,
    pub package_name: String,
    pub asset_path: String,
    pub label: String,
    pub asset_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CsupRecord {
    pub airport_id: String,
    pub region_id: String,
    pub package_name: String,
    pub asset_path: String,
    pub label: String,
    pub asset_kind: String,
}

pub fn build_resource_index(request: &BuildResourceIndexRequest) -> anyhow::Result<ResourceIndex> {
    let nav_cycle_code = read_nav_cycle_code(&request.nav_db_zip)?;
    let cycle = infer_cycle(&request.chart_sources, &request.tpp_sources, &request.csup_sources)?
        .or_else(|| nav_cycle_code.clone());
    let packages = collect_packages(&request.chart_sources, &request.tpp_sources, &request.csup_sources)?;
    let chart_collections = collect_chart_collections(&request.chart_sources)?;
    let airports = load_airports_from_nav_db(&request.nav_db_zip)?;
    let plates = collect_plate_records(&request.tpp_sources)?;
    let csups = collect_csup_records(&request.csup_sources)?;
    let families = collect_families(&packages, !plates.is_empty(), !csups.is_empty());
    let regions = Region::ALL
        .iter()
        .enumerate()
        .map(|(index, region)| ResourceRegion {
            id: region.code().to_ascii_lowercase(),
            display_name: region_display_name(*region).to_string(),
            sort_order: index as u32,
        })
        .collect();

    Ok(ResourceIndex {
        schema_version: 1,
        cycle,
        generated_at_utc: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        nav_db: NavDbRef {
            artifact_path: request.nav_db_zip.display().to_string(),
            sqlite_entry: "main.db".to_string(),
            cycle_code: nav_cycle_code,
        },
        families,
        regions,
        packages,
        chart_collections,
        airports,
        plates,
        csups,
    })
}

pub fn write_resource_index(request: &BuildResourceIndexRequest) -> anyhow::Result<ResourceIndex> {
    let index = build_resource_index(request)?;
    let parent = request
        .output_path
        .parent()
        .context("output path must have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let json = serde_json::to_vec_pretty(&index).context("failed to serialize resource index")?;
    fs::write(&request.output_path, json)
        .with_context(|| format!("failed to write {}", request.output_path.display()))?;
    Ok(index)
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
) -> anyhow::Result<Vec<ResourcePackage>> {
    let mut packages = Vec::new();
    for source in chart_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            packages.push(package_from_record(
                &source.family_id,
                &source.package_root,
                &record,
            )?);
        }
    }
    for source in tpp_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            packages.push(package_from_record("tpp", &source.asset_root, &record)?);
        }
    }
    for source in csup_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            packages.push(package_from_record("csup", &source.asset_root, &record)?);
        }
    }
    packages.sort();
    Ok(packages)
}

fn collect_chart_collections(chart_sources: &[ChartSource]) -> anyhow::Result<Vec<ChartCollectionRecord>> {
    let mut collections = Vec::new();
    for source in chart_sources {
        for record in read_package_outputs(&source.package_outputs_path)? {
            let artifact_path = source.package_root.join(&record.zip);
            let metadata = read_chart_zip_metadata(&artifact_path)?;
            collections.push(ChartCollectionRecord {
                id: format!("{}:{}", source.family_id, record.region.to_ascii_lowercase()),
                family_id: source.family_id.clone(),
                region_id: record.region.to_ascii_lowercase(),
                package_name: record.manifest.clone(),
                chart_index: metadata.chart_index,
                tile_path_template: format!("tiles/{}/{}/{{x}}/{{y}}.webp", metadata.chart_index, "{z}"),
                levels: metadata.levels,
                coverage_bounds: metadata.coverage_bounds,
                default_view: metadata.default_view,
            });
        }
    }
    collections.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(collections)
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
) -> anyhow::Result<ResourcePackage> {
    let artifact_path = package_root.join(&record.zip);
    let size_bytes = fs::metadata(&artifact_path)
        .with_context(|| format!("failed to stat {}", artifact_path.display()))?
        .len();
    Ok(ResourcePackage {
        family_id: family_id.to_string(),
        region_id: record.region.to_ascii_lowercase(),
        manifest_name: record.manifest.clone(),
        artifact_path: artifact_path.display().to_string(),
        size_bytes,
        checksum_sha256: record.zip_sha256.clone(),
    })
}

fn load_airports_from_nav_db(nav_db_zip: &Path) -> anyhow::Result<Vec<AirportRecord>> {
    let sqlite_path = extract_sqlite_entry(nav_db_zip, "main.db")?;
    let connection = Connection::open(sqlite_path.path()).context("failed to open main.db")?;
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

fn collect_plate_records(sources: &[AssetSource]) -> anyhow::Result<Vec<PlateRecord>> {
    let mut records = Vec::new();
    for source in sources {
        let package_map = package_map_by_region(&source.package_outputs_path)?;
        let plates_root = source.asset_root.join("plates");
        if !plates_root.is_dir() {
            continue;
        }
        for airport_dir in read_child_dirs(&plates_root)? {
            let airport_id = airport_dir
                .file_name()
                .and_then(|value| value.to_str())
                .context("invalid airport directory name")?
                .to_string();
            let region_id = infer_region_from_airport_dir(&source.asset_root)
                .or_else(|| infer_region_from_package_map(&package_map))
                .context("failed to infer TPP region")?;
            let package_name = package_map
                .get(&region_id.to_ascii_uppercase())
                .cloned()
                .context("missing TPP package for region")?;
            for asset in read_files_recursive(&airport_dir)? {
                let asset_path = asset.strip_prefix(&source.asset_root).unwrap_or(&asset);
                records.push(PlateRecord {
                    airport_id: airport_id.clone(),
                    region_id: region_id.clone(),
                    package_name: package_name.clone(),
                    label: asset
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    asset_kind: asset
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                    asset_path: asset_path.display().to_string(),
                });
            }
        }
    }
    records.sort();
    Ok(records)
}

fn collect_csup_records(sources: &[AssetSource]) -> anyhow::Result<Vec<CsupRecord>> {
    let mut records = Vec::new();
    for source in sources {
        let package_map = package_map_by_region(&source.package_outputs_path)?;
        let afd_root = source.asset_root.join("afd");
        if !afd_root.is_dir() {
            continue;
        }
        for airport_dir in read_child_dirs(&afd_root)? {
            let airport_id = airport_dir
                .file_name()
                .and_then(|value| value.to_str())
                .context("invalid airport directory name")?
                .to_string();
            for asset in read_files_recursive(&airport_dir)? {
                let region_id = infer_region_from_csup_filename(&asset)
                    .context("failed to infer CSUP region from filename")?;
                let package_name = package_map
                    .get(&region_id.to_ascii_uppercase())
                    .cloned()
                    .context("missing CSUP package for region")?;
                let asset_path = asset.strip_prefix(&source.asset_root).unwrap_or(&asset);
                records.push(CsupRecord {
                    airport_id: airport_id.clone(),
                    region_id,
                    package_name,
                    label: asset
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    asset_kind: asset
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                    asset_path: asset_path.display().to_string(),
                });
            }
        }
    }
    records.sort();
    Ok(records)
}

fn package_map_by_region(path: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for record in read_package_outputs(path)? {
        map.insert(record.region.to_ascii_uppercase(), record.manifest);
    }
    Ok(map)
}

fn read_package_outputs(path: &Path) -> anyhow::Result<Vec<PackageOutputRecord>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).context("failed to parse package output json"))
        .map(|value| {
            let value = value?;
            if value.get("event").and_then(|v| v.as_str()) != Some("package_output") {
                bail!("unexpected package output event: {value}");
            }
            Ok(PackageOutputRecord {
                label: value.get("label").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                chart: value.get("chart").and_then(|v| v.as_str()).map(ToOwned::to_owned),
                region: value.get("region").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                manifest: value.get("manifest").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                manifest_sha256: value
                    .get("manifest_sha256")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                zip: value.get("zip").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                zip_sha256: value
                    .get("zip_sha256")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
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
    std::io::Read::read_to_string(&mut entry, &mut text).context("failed to read databases entry")?;
    Ok(text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "main.db")
        .map(ToOwned::to_owned))
}

fn read_child_dirs(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut dirs = fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    Ok(dirs)
}

fn read_files_recursive(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fn visit(path: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory {}", path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to iterate {}", path.display()))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let child = entry.path();
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
    let _ = manifest;
    None
}

#[derive(Debug, Clone, PartialEq)]
struct ChartZipMetadata {
    chart_index: u32,
    levels: Vec<TileLevelRecord>,
    coverage_bounds: CoverageBounds,
    default_view: DefaultView,
}

fn read_chart_zip_metadata(path: &Path) -> anyhow::Result<ChartZipMetadata> {
    let file = fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let archive = ZipArchive::new(file).context("failed to open chart zip")?;
    let mut levels: BTreeMap<u32, (u32, u32, u32, u32)> = BTreeMap::new();
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
        let entry = levels.entry(zoom).or_insert((x, x, y_tms, y_tms));
        entry.0 = entry.0.min(x);
        entry.1 = entry.1.max(x);
        entry.2 = entry.2.min(y_tms);
        entry.3 = entry.3.max(y_tms);
    }
    let chart_index = chart_index.context("no tile entries found in chart zip")?;
    let level_records = levels
        .into_iter()
        .map(|(zoom, (x_min, x_max, y_tms_min, y_tms_max))| TileLevelRecord {
            zoom,
            x_min,
            x_max,
            y_tms_min,
            y_tms_max,
        })
        .collect::<Vec<_>>();
    let coverage_bounds = coverage_bounds_from_levels(&level_records)
        .context("failed to derive coverage bounds from levels")?;
    let default_view = default_view_from_levels(&level_records)
        .context("failed to derive default view from levels")?;
    Ok(ChartZipMetadata {
        chart_index,
        levels: level_records,
        coverage_bounds,
        default_view,
    })
}

fn coverage_bounds_from_levels(levels: &[TileLevelRecord]) -> Option<CoverageBounds> {
    let level = levels.iter().max_by_key(|level| level.zoom)?;
    let scale = 2_u32.pow(level.zoom) as f64;
    let y_xyz_min = (scale as u32 - 1 - level.y_tms_max) as f64;
    let y_xyz_max = (scale as u32 - 1 - level.y_tms_min) as f64;
    let lon_min = tile_x_to_lon(level.x_min as f64, scale);
    let lon_max = tile_x_to_lon((level.x_max + 1) as f64, scale);
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
    let scale = 2_u32.pow(level.zoom) as f64;
    let center_x = ((level.x_min + level.x_max + 1) as f64) / 2.0;
    let center_y_tms = ((level.y_tms_min + level.y_tms_max + 1) as f64) / 2.0;
    let center_y_xyz = scale - center_y_tms;
    Some(DefaultView {
        lat: tile_y_to_lat(center_y_xyz, scale),
        lon: tile_x_to_lon(center_x, scale),
        zoom: f64::from(level.zoom) - 2.0,
    })
}

fn tile_x_to_lon(tile_x: f64, scale: f64) -> f64 {
    (tile_x / scale) * 360.0 - 180.0
}

fn tile_y_to_lat(tile_y_xyz: f64, scale: f64) -> f64 {
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * tile_y_xyz) / scale;
    n.sinh().atan().to_degrees()
}

fn infer_region_from_airport_dir(root: &Path) -> Option<String> {
    root.file_name()
        .and_then(|value| value.to_str())
        .and_then(|name| name.strip_prefix("tpp-"))
        .map(|value| value.to_ascii_lowercase())
}

fn infer_region_from_package_map(package_map: &BTreeMap<String, String>) -> Option<String> {
    if package_map.len() == 1 {
        package_map.keys().next().map(|value| value.to_ascii_lowercase())
    } else {
        None
    }
}

fn infer_region_from_csup_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let suffix = stem.strip_prefix("CSUP-")?;
    let region = suffix.split('_').next()?;
    Some(region.to_ascii_lowercase())
}

fn family_display_name(id: &str) -> &'static str {
    match id {
        "sectional" => "Sectional",
        "tac" => "TAC",
        "ifr_low" => "IFR Low",
        "ifr_high" => "IFR High",
        "tpp" => "TPP",
        "csup" => "CSUP",
        _ => "Unknown",
    }
}

fn family_kind(id: &str) -> &'static str {
    match id {
        "sectional" | "tac" | "ifr_low" | "ifr_high" => "tiled_raster",
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
    use rusqlite::Connection;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn builds_index_from_realistic_inputs() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("main.db");
        let conn = Connection::open(&db_path).expect("open sqlite");
        conn.execute_batch(
            "create table airports (
                LocationID text,
                ARPLatitude float,
                ARPLongitude float,
                Type text,
                FacilityName text
            );
            insert into airports values ('KBOS', 42.3656, -71.0096, 'AIRPORT', 'Boston Logan');
            insert into airports values ('KRNT', 47.4931, -122.2160, 'AIRPORT', 'Renton');",
        )
        .expect("seed airports");

        let nav_zip = temp.path().join("databases.zip");
        {
            let file = fs::File::create(&nav_zip).expect("create zip");
            let mut zip = ZipWriter::new(file);
            zip.start_file("databases", SimpleFileOptions::default())
                .expect("start databases entry");
            zip.write_all(b"2604\nmain.db\n")
                .expect("write databases entry");
            zip.start_file("main.db", SimpleFileOptions::default())
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

        let tpp_root = temp.path().join("tpp-ne");
        fs::create_dir_all(tpp_root.join("plates/KBOS")).expect("tpp root");
        fs::write(tpp_root.join("NE_TPP.zip"), b"zip-bytes").expect("tpp zip");
        fs::write(
            tpp_root.join("plates/KBOS/IAP-MA-ILS OR LOC RWY 04R.png"),
            b"png",
        )
        .expect("plate file");
        let tpp_outputs = temp.path().join("tpp-package-outputs.jsonl");
        fs::write(
            &tpp_outputs,
            "{\"event\":\"package_output\",\"label\":\"tpp-ne\",\"manifest\":\"NE_TPP\",\"manifest_sha256\":\"m\",\"region\":\"NE\",\"zip\":\"NE_TPP.zip\",\"zip_sha256\":\"def\"}\n",
        )
        .expect("tpp outputs");

        let csup_root = temp.path().join("csup");
        fs::create_dir_all(csup_root.join("afd/KBOS")).expect("csup root");
        fs::write(csup_root.join("NE_CSUP.zip"), b"zip-bytes").expect("csup zip");
        fs::write(csup_root.join("afd/KBOS/CSUP-NE_0-0.png"), b"png").expect("csup file");
        let csup_outputs = temp.path().join("csup-package-outputs.jsonl");
        fs::write(
            &csup_outputs,
            "{\"event\":\"package_output\",\"label\":\"csup\",\"manifest\":\"NE_CSUP\",\"manifest_sha256\":\"m\",\"region\":\"NE\",\"zip\":\"NE_CSUP.zip\",\"zip_sha256\":\"ghi\"}\n",
        )
        .expect("csup outputs");

        let request = BuildResourceIndexRequest {
            nav_db_zip: nav_zip.clone(),
            output_path: temp.path().join("resource-index.json"),
            chart_sources: vec![ChartSource {
                family_id: "sectional".to_string(),
                package_outputs_path: chart_outputs,
                package_root: chart_root,
            }],
            tpp_sources: vec![AssetSource {
                package_outputs_path: tpp_outputs,
                asset_root: tpp_root,
            }],
            csup_sources: vec![AssetSource {
                package_outputs_path: csup_outputs,
                asset_root: csup_root,
            }],
        };

        let index = write_resource_index(&request).expect("build index");
        assert_eq!(index.cycle.as_deref(), Some("2604"));
        assert_eq!(index.nav_db.sqlite_entry, "main.db");
        assert_eq!(index.nav_db.cycle_code.as_deref(), Some("2604"));
        assert_eq!(index.airports.len(), 2);
        assert!(index.packages.iter().any(|package| package.family_id == "sectional"));
        assert!(index.packages.iter().any(|package| package.family_id == "tpp"));
        assert!(index.packages.iter().any(|package| package.family_id == "csup"));
        assert_eq!(index.chart_collections.len(), 1);
        assert_eq!(index.chart_collections[0].family_id, "sectional");
        assert_eq!(index.chart_collections[0].region_id, "nw");
        assert_eq!(index.chart_collections[0].chart_index, 0);
        assert_eq!(index.chart_collections[0].tile_path_template, "tiles/0/{z}/{x}/{y}.webp");
        assert_eq!(
            index.chart_collections[0].levels,
            vec![TileLevelRecord {
                zoom: 7,
                x_min: 20,
                x_max: 21,
                y_tms_min: 49,
                y_tms_max: 50,
            }]
        );
        assert!(index.chart_collections[0].coverage_bounds.lon_min < index.chart_collections[0].coverage_bounds.lon_max);
        assert!(index.chart_collections[0].coverage_bounds.lat_min < index.chart_collections[0].coverage_bounds.lat_max);
        assert_eq!(index.plates[0].airport_id, "KBOS");
        assert_eq!(index.plates[0].package_name, "NE_TPP");
        assert_eq!(index.csups[0].region_id, "ne");
        assert_eq!(index.csups[0].package_name, "NE_CSUP");
        assert!(request.output_path.exists());
    }
}
