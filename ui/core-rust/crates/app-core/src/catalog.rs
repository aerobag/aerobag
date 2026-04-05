use serde::{Deserialize, Serialize};

use crate::geometry::GeoBounds;
use crate::ids::{AirportId, ChartFamilyId, ChartId, PackageId, PlateId, RegionId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogHandle {
    pub bundle: CatalogBundle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogBundle {
    pub schema_version: u32,
    pub cycle: String,
    pub catalog_revision: String,
    pub families: Vec<CatalogFamily>,
    pub regions: Vec<CatalogRegion>,
    pub packages: Vec<CatalogPackage>,
    pub charts: Vec<ChartRecord>,
    #[serde(default)]
    pub plates: Vec<PlateRecord>,
    #[serde(default)]
    pub supplements: Vec<SupplementRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogFamily {
    pub id: ChartFamilyId,
    pub display_name: String,
    pub kind: String,
    pub max_zoom: Option<u8>,
    pub tile_size: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogRegion {
    pub id: RegionId,
    pub display_name: String,
    pub sort_order: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogPackage {
    pub id: PackageId,
    pub package_name: String,
    pub family_id: ChartFamilyId,
    pub region_id: RegionId,
    pub cycle: String,
    pub artifact_kind: String,
    pub relative_url: String,
    pub manifest_name: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ChartCoverage {
    PolygonRef { polygon_id: String },
    BBox(GeoBounds),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartRecord {
    pub id: ChartId,
    pub family_id: ChartFamilyId,
    pub name: String,
    pub display_name: String,
    pub cycle: String,
    pub region_ids: Vec<RegionId>,
    pub max_zoom: u8,
    pub tile_path_template: String,
    pub coverage: ChartCoverage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateRecord {
    pub id: PlateId,
    pub airport_id: AirportId,
    pub region_id: RegionId,
    pub cycle: String,
    pub procedure_code: String,
    pub display_name: String,
    pub kind: String,
    pub georeferenced: bool,
    pub page_count: u16,
    pub asset_base_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplementRecord {
    pub airport_id: AirportId,
    pub region_id: RegionId,
    pub cycle: String,
    pub page_count: u16,
    pub asset_base_path: String,
}
