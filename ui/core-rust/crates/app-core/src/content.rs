use serde::{Deserialize, Serialize};

use crate::ids::{ChartId, PackageId, PlateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentAvailability {
    LocalOnly,
    RemoteOnly,
    LocalAndRemote,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentPolicy {
    OfflineRequired,
    PreferLocal,
    StreamAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailabilityDetail {
    pub availability: ContentAvailability,
    pub cycle_current: bool,
    pub integrity_ok: bool,
    pub cached: bool,
    pub offline_usable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentInventory {
    pub installed_packages: Vec<InstalledPackage>,
    pub cached_tilesets: Vec<CachedTileset>,
    pub cached_plates: Vec<CachedPlate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub package_id: PackageId,
    pub integrity_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedTileset {
    pub chart_id: ChartId,
    pub fully_cached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedPlate {
    pub plate_id: PlateId,
    pub cached_pages: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentRequirement {
    pub package_ids: Vec<PackageId>,
    pub chart_ids: Vec<ChartId>,
    pub plate_ids: Vec<PlateId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentReportItem {
    pub label: String,
    pub availability: AvailabilityDetail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentReport {
    pub fully_satisfied: bool,
    pub items: Vec<ContentReportItem>,
}
