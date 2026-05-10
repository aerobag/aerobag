use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{NavRef, ProcedureKind};

const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
const VERSION: u32 = 2;
const HEADER_LEN: usize = 64;
const ENTRY_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    key_offset: u32,
    value_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvRoot {
    entry_count: u32,
    page_size: u32,
    offset_table_offset: u32,
    key_table_offset: u32,
    value_table_offset: u32,
    value_bytes_len: u32,
    logical_bytes_len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavKvLookup {
    Hit(Vec<u8>),
    MissingKey,
    MissingPages(Vec<u32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvStore {
    root: NavKvRoot,
    pages: HashMap<u32, Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NavKvQuery {
    ChartCatalog,
    OfflineRegionCatalog,
    PackageById {
        package_id: String,
    },
    PolygonSet {
        polygon_set_id: String,
    },
    PlateAirportIndex,
    PlateAirport {
        airport_id: String,
    },
    PlateById {
        plate_id: String,
    },
    PlateCifpMatch {
        airport_id: String,
        cifp_id: String,
    },
    PlateProcedureCandidates {
        plate_id: String,
    },
    ProcedureGeometry {
        airport_id: String,
        procedure_kind: ProcedureKind,
        procedure_id: String,
        runway_transition: Option<String>,
        enroute_transition: Option<String>,
    },
    NavRefPosition {
        nav_ref: NavRef,
        procedure_airport_id: Option<String>,
    },
    NavRefSymbol {
        nav_ref: NavRef,
    },
    AirwayBranches {
        airway_name: String,
    },
    AirwaySpatial {
        lat_tile: i32,
        lon_tile: i32,
    },
    WaypointIdentifier {
        identifier: String,
    },
    WaypointPrefix {
        prefix: String,
    },
    VectorManifest,
    VectorPointTile {
        layer: String,
        z: u32,
        x: u32,
        y: u32,
    },
    VectorAirspaceRefTile {
        z: u32,
        x: u32,
        y: u32,
    },
    VectorAirspaceLabelTile {
        z: u32,
        x: u32,
        y: u32,
    },
    VectorAirspaceFeature {
        id: String,
    },
}

impl NavKvRoot {
    pub fn parse(root_bytes: &[u8]) -> Result<Self, String> {
        if root_bytes.len() < HEADER_LEN {
            return Err("nav_kv root is shorter than header".to_string());
        }
        if &root_bytes[..MAGIC.len()] != MAGIC {
            return Err("nav_kv root has invalid magic".to_string());
        }
        let actual_version = read_u32(root_bytes, 16)?;
        if actual_version != VERSION {
            return Err(format!("unsupported nav_kv version {actual_version}"));
        }
        let entry_count = read_u32(root_bytes, 20)?;
        let page_size = read_u32(root_bytes, 24)?;
        let offset_table_offset = read_u32(root_bytes, 28)?;
        let offset_table_len = read_u32(root_bytes, 32)?;
        let key_table_offset = read_u32(root_bytes, 36)?;
        let key_table_len = read_u32(root_bytes, 40)?;
        let value_table_offset = read_u32(root_bytes, 44)?;
        let value_bytes_len = read_u32(root_bytes, 48)?;
        let logical_bytes_len = read_u32(root_bytes, 52)?;
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        if root_bytes.len() != HEADER_LEN {
            return Err("nav_kv v2 root must be exactly the fixed header length".to_string());
        }
        let sentinel_count = entry_count
            .checked_add(1)
            .ok_or_else(|| "nav_kv entry count overflow".to_string())?;
        let expected_offset_table_len = sentinel_count
            .checked_mul(ENTRY_LEN as u32)
            .ok_or_else(|| "nav_kv entry table length overflow".to_string())?;
        if offset_table_offset != 0 {
            return Err("nav_kv offset table must start at logical offset 0".to_string());
        }
        if offset_table_len != expected_offset_table_len {
            return Err("nav_kv offset table length does not match entry count".to_string());
        }
        if key_table_offset != offset_table_offset + offset_table_len {
            return Err("nav_kv key table offset does not follow offset table".to_string());
        }
        if value_table_offset != key_table_offset + key_table_len {
            return Err("nav_kv value table offset does not follow key table".to_string());
        }
        if logical_bytes_len != value_table_offset + value_bytes_len {
            return Err("nav_kv logical length does not match table lengths".to_string());
        }
        Ok(Self {
            entry_count,
            page_size,
            offset_table_offset,
            key_table_offset,
            value_table_offset,
            value_bytes_len,
            logical_bytes_len,
        })
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn value_bytes_len(&self) -> u32 {
        self.value_bytes_len
    }

    pub fn logical_bytes_len(&self) -> u32 {
        self.logical_bytes_len
    }

    pub fn page_count(&self) -> u32 {
        self.logical_bytes_len.div_ceil(self.page_size)
    }

    pub fn len(&self) -> usize {
        self.entry_count as usize
    }
}

impl NavKvStore {
    pub fn new(root: NavKvRoot) -> Self {
        Self {
            root,
            pages: HashMap::new(),
        }
    }

    pub fn root(&self) -> &NavKvRoot {
        &self.root
    }

    pub fn insert_page(&mut self, page_index: u32, bytes: Vec<u8>) {
        self.pages.insert(page_index, bytes);
    }

    pub fn get_bytes(&self, key: &str) -> Result<NavKvLookup, String> {
        let lookup = match self.value_range(key)? {
            RangeRead::Hit(Some(range)) => range,
            RangeRead::Hit(None) => {
                log_nav_kv_lookup("exact", key, "missing_key", &[], 0);
                return Ok(NavKvLookup::MissingKey);
            }
            RangeRead::MissingPages(pages) => {
                log_nav_kv_lookup("exact", key, "missing_pages", &pages, 0);
                return Ok(NavKvLookup::MissingPages(pages));
            }
        };
        let (start, end) = lookup;
        if start == end {
            log_nav_kv_lookup("exact", key, "hit", &[], 0);
            return Ok(NavKvLookup::Hit(Vec::new()));
        }
        match self.read_logical_range(self.root.value_table_offset + start, end - start)? {
            RangeRead::Hit(bytes) => {
                log_nav_kv_lookup("exact", key, "hit", &[], bytes.len());
                Ok(NavKvLookup::Hit(bytes))
            }
            RangeRead::MissingPages(pages) => {
                log_nav_kv_lookup("exact", key, "missing_pages", &pages, 0);
                Ok(NavKvLookup::MissingPages(pages))
            }
        }
    }

    pub fn missing_pages_for_keys(&self, keys: &[String]) -> Result<Vec<u32>, String> {
        let mut pages = BTreeSet::new();
        for key in keys {
            if let NavKvLookup::MissingPages(missing) = self.get_bytes(key)? {
                pages.extend(missing);
            }
        }
        Ok(pages.into_iter().collect())
    }

    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        match self.keys_with_prefix_checked(prefix) {
            Ok(RangeRead::Hit(keys)) => keys,
            Ok(RangeRead::MissingPages(_)) | Err(_) => Vec::new(),
        }
    }

    pub fn keys_with_prefix_lookup(&self, prefix: &str) -> Result<NavKvLookup, String> {
        match self.keys_with_prefix_checked(prefix)? {
            RangeRead::Hit(keys) => {
                log_nav_kv_lookup("prefix", prefix, "hit", &[], keys.len());
                Ok(NavKvLookup::Hit(keys.join("\n").into_bytes()))
            }
            RangeRead::MissingPages(pages) => {
                log_nav_kv_lookup("prefix", prefix, "missing_pages", &pages, 0);
                Ok(NavKvLookup::MissingPages(pages))
            }
        }
    }

    fn keys_with_prefix_checked(&self, prefix: &str) -> Result<RangeRead<Vec<String>>, String> {
        let prefix = prefix.as_bytes();
        let mut left = 0usize;
        let mut right = self.root.len();
        let mut missing_pages = BTreeSet::new();
        while left < right {
            let mid = left + (right - left) / 2;
            let key = match self.key_at(mid)? {
                RangeRead::Hit(key) => key,
                RangeRead::MissingPages(pages) => {
                    missing_pages.extend(pages);
                    break;
                }
            };
            if key.as_slice() < prefix {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        if !missing_pages.is_empty() {
            return Ok(RangeRead::MissingPages(missing_pages.into_iter().collect()));
        }
        let mut out = Vec::new();
        let mut index = left;
        while index < self.root.len() {
            let key = match self.key_at(index)? {
                RangeRead::Hit(key) => key,
                RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
            };
            if !key.starts_with(prefix) {
                break;
            }
            out.push(String::from_utf8_lossy(&key).into_owned());
            index += 1;
        }
        Ok(RangeRead::Hit(out))
    }

    fn value_range(&self, key: &str) -> Result<RangeRead<Option<(u32, u32)>>, String> {
        let target = key.as_bytes();
        let mut left = 0usize;
        let mut right = self.root.len();
        let mut missing_pages = BTreeSet::new();
        while left < right {
            let mid = left + (right - left) / 2;
            let key_at_mid = match self.key_at(mid)? {
                RangeRead::Hit(key) => key,
                RangeRead::MissingPages(pages) => {
                    missing_pages.extend(pages);
                    break;
                }
            };
            match key_at_mid.as_slice().cmp(target) {
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => right = mid,
                std::cmp::Ordering::Equal => {
                    return match self.value_range_at(mid)? {
                        RangeRead::Hit(range) => Ok(RangeRead::Hit(Some(range))),
                        RangeRead::MissingPages(pages) => Ok(RangeRead::MissingPages(pages)),
                    };
                }
            }
        }
        if missing_pages.is_empty() {
            Ok(RangeRead::Hit(None))
        } else {
            Ok(RangeRead::MissingPages(missing_pages.into_iter().collect()))
        }
    }

    fn key_at(&self, index: usize) -> Result<RangeRead<Vec<u8>>, String> {
        let (start, end) = match self.key_range_at(index)? {
            RangeRead::Hit(Some(range)) => range,
            RangeRead::Hit(None) => return Ok(RangeRead::Hit(Vec::new())),
            RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
        };
        self.read_logical_range(self.root.key_table_offset + start, end - start)
    }

    fn key_range_at(&self, index: usize) -> Result<RangeRead<Option<(u32, u32)>>, String> {
        match self.entry_pair_at(index)? {
            RangeRead::Hit(Some((current, next))) => {
                Ok(RangeRead::Hit(Some((current.key_offset, next.key_offset))))
            }
            RangeRead::Hit(None) => Ok(RangeRead::Hit(None)),
            RangeRead::MissingPages(pages) => Ok(RangeRead::MissingPages(pages)),
        }
    }

    fn value_range_at(&self, index: usize) -> Result<RangeRead<(u32, u32)>, String> {
        match self.entry_pair_at(index)? {
            RangeRead::Hit(Some((current, next))) => {
                Ok(RangeRead::Hit((current.value_offset, next.value_offset)))
            }
            RangeRead::Hit(None) => Ok(RangeRead::Hit((0, 0))),
            RangeRead::MissingPages(pages) => Ok(RangeRead::MissingPages(pages)),
        }
    }

    fn entry_pair_at(&self, index: usize) -> Result<RangeRead<Option<(Entry, Entry)>>, String> {
        if index >= self.root.len() {
            return Ok(RangeRead::Hit(None));
        }
        let offset = self.root.offset_table_offset + (index as u32) * ENTRY_LEN as u32;
        let bytes = match self.read_logical_range(offset, (ENTRY_LEN * 2) as u32)? {
            RangeRead::Hit(bytes) => bytes,
            RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
        };
        Ok(RangeRead::Hit(Some((
            Entry {
                key_offset: read_u32(&bytes, 0)?,
                value_offset: read_u32(&bytes, 4)?,
            },
            Entry {
                key_offset: read_u32(&bytes, 8)?,
                value_offset: read_u32(&bytes, 12)?,
            },
        ))))
    }

    fn read_logical_range(&self, start: u32, len: u32) -> Result<RangeRead<Vec<u8>>, String> {
        if len == 0 {
            return Ok(RangeRead::Hit(Vec::new()));
        }
        let end = start
            .checked_add(len)
            .ok_or_else(|| "nav_kv logical range overflow".to_string())?;
        if end > self.root.logical_bytes_len {
            return Err("nav_kv logical range exceeds logical bytes length".to_string());
        }
        let start_page = start / self.root.page_size;
        let end_page = (end - 1) / self.root.page_size;
        let missing = (start_page..=end_page)
            .filter(|page| !self.pages.contains_key(page))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Ok(RangeRead::MissingPages(missing));
        }
        let mut out = Vec::with_capacity(len as usize);
        for page_index in start_page..=end_page {
            let page = self
                .pages
                .get(&page_index)
                .ok_or_else(|| format!("nav_kv page {page_index} missing"))?;
            let page_start = page_index
                .checked_mul(self.root.page_size)
                .ok_or_else(|| "nav_kv page start overflow".to_string())?;
            let slice_start = start.saturating_sub(page_start) as usize;
            let slice_end = (end.min(page_start + self.root.page_size) - page_start) as usize;
            if slice_start > slice_end || slice_end > page.len() {
                return Err(format!("nav_kv page {page_index} is too short"));
            }
            out.extend_from_slice(&page[slice_start..slice_end]);
        }
        Ok(RangeRead::Hit(out))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RangeRead<T> {
    Hit(T),
    MissingPages(Vec<u32>),
}

fn log_nav_kv_lookup(kind: &str, key: &str, result: &str, pages: &[u32], size: usize) {
    eprintln!(
        "NAV_KV_LOOKUP kind={kind} result={result} size={size} pages={} key={key}",
        format_page_list(pages)
    );
}

fn format_page_list(pages: &[u32]) -> String {
    if pages.is_empty() {
        return "-".to_string();
    }
    pages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn nav_kv_key_for_query(query: &NavKvQuery) -> Option<String> {
    match query {
        NavKvQuery::ChartCatalog => Some("chart/catalog".to_string()),
        NavKvQuery::OfflineRegionCatalog => Some("offline-region/catalog".to_string()),
        NavKvQuery::PackageById { package_id } => {
            Some(format!("package/by-id/{}", component(package_id)))
        }
        NavKvQuery::PolygonSet { polygon_set_id } => Some(format!(
            "geometry/polygon-set/{}",
            component(polygon_set_id)
        )),
        NavKvQuery::PlateAirportIndex => Some("plate/airport-index".to_string()),
        NavKvQuery::PlateAirport { airport_id } => {
            Some(format!("plate/airport/{}", upper_component(airport_id)))
        }
        NavKvQuery::PlateById { plate_id } => Some(format!("plate/by-id/{}", component(plate_id))),
        NavKvQuery::PlateCifpMatch {
            airport_id,
            cifp_id,
        } => Some(format!(
            "plate/cifp/{}/{}",
            upper_component(airport_id),
            upper_component(cifp_id)
        )),
        NavKvQuery::PlateProcedureCandidates { plate_id } => Some(format!(
            "plate/procedure-candidates/{}",
            component(plate_id)
        )),
        NavKvQuery::ProcedureGeometry {
            airport_id,
            procedure_kind,
            procedure_id,
            runway_transition,
            enroute_transition,
        } => Some(procedure_geometry_key(
            airport_id,
            procedure_kind,
            procedure_id,
            runway_transition.as_deref(),
            enroute_transition.as_deref(),
        )),
        NavKvQuery::NavRefPosition {
            nav_ref,
            procedure_airport_id,
        } => nav_ref_position_key(nav_ref, procedure_airport_id.as_deref()),
        NavKvQuery::NavRefSymbol { nav_ref } => nav_ref_symbol_key(nav_ref),
        NavKvQuery::AirwayBranches { airway_name } => {
            Some(format!("airway/{}", upper_component(airway_name)))
        }
        NavKvQuery::AirwaySpatial { lat_tile, lon_tile } => {
            Some(format!("airway/spatial/{lat_tile}/{lon_tile}"))
        }
        NavKvQuery::WaypointIdentifier { identifier } => Some(format!(
            "waypoint/identifier/{}",
            upper_component(identifier)
        )),
        NavKvQuery::WaypointPrefix { prefix } => {
            let normalized = prefix.trim().to_uppercase();
            let shard = if normalized.len() <= 2 {
                normalized
            } else {
                normalized.chars().take(2).collect()
            };
            Some(format!("waypoint/prefix/{}", component(&shard)))
        }
        NavKvQuery::VectorManifest => Some("vector/manifest".to_string()),
        NavKvQuery::VectorPointTile { layer, z, x, y } => Some(format!(
            "vector/point-tile/{}/{z}/{x}/{y}",
            component(layer)
        )),
        NavKvQuery::VectorAirspaceRefTile { z, x, y } => {
            Some(format!("vector/airspace/ref-tile/{z}/{x}/{y}"))
        }
        NavKvQuery::VectorAirspaceLabelTile { z, x, y } => {
            Some(format!("vector/airspace/label-tile/{z}/{x}/{y}"))
        }
        NavKvQuery::VectorAirspaceFeature { id } => {
            Some(format!("vector/airspace/feature/{}", component(id)))
        }
    }
}

fn nav_ref_position_key(nav_ref: &NavRef, procedure_airport_id: Option<&str>) -> Option<String> {
    match nav_ref {
        NavRef::Airport(id) => Some(format!("navref/position/airport/{}", upper_component(id))),
        NavRef::Navaid(id) => Some(format!("navref/position/navaid/{}", upper_component(id))),
        NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => Some(format!(
            "navref/position/arinc-navaid/{}/{}/{}/{}",
            upper_component(section_code),
            upper_component(subsection_code),
            upper_component(icao_code),
            upper_component(identifier)
        )),
        NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => Some(format!(
            "navref/position/terminal-navaid/{}/{}/{}/{}/{}",
            upper_component(airport_id),
            upper_component(section_code),
            upper_component(subsection_code),
            upper_component(icao_code),
            upper_component(identifier)
        )),
        NavRef::Fix(id)
            if procedure_airport_id.is_some() && id.trim().to_uppercase().starts_with("RW") =>
        {
            Some(format!(
                "navref/position/runway/{}/{}",
                upper_component(procedure_airport_id.unwrap_or_default()),
                upper_component(id)
            ))
        }
        NavRef::Fix(id) => Some(format!("navref/position/fix/{}", upper_component(id))),
        NavRef::LatLon(_) | NavRef::Spot(_) => None,
    }
}

fn nav_ref_symbol_key(nav_ref: &NavRef) -> Option<String> {
    match nav_ref {
        NavRef::Airport(id) => Some(format!("navref/symbol/airport/{}", upper_component(id))),
        NavRef::Navaid(id) => Some(format!("navref/symbol/navaid/{}", upper_component(id))),
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => Some(
            format!("navref/symbol/navaid/{}", upper_component(identifier)),
        ),
        NavRef::Fix(id) => Some(format!("navref/symbol/fix/{}", upper_component(id))),
        NavRef::LatLon(_) | NavRef::Spot(_) => None,
    }
}

fn procedure_kind_component(kind: &ProcedureKind) -> &'static str {
    match kind {
        ProcedureKind::Sid => "SID",
        ProcedureKind::Star => "STAR",
        ProcedureKind::Approach => "APPROACH",
    }
}

pub fn procedure_geometry_prefix(
    airport_id: &str,
    procedure_kind: &ProcedureKind,
    procedure_id: &str,
) -> String {
    format!(
        "procedure/geometry/{}/{}/{}/",
        upper_component(airport_id),
        procedure_kind_component(procedure_kind),
        upper_component(procedure_id)
    )
}

pub fn procedure_geometry_kind_prefix(airport_id: &str, procedure_kind: &ProcedureKind) -> String {
    format!(
        "procedure/geometry/{}/{}/",
        upper_component(airport_id),
        procedure_kind_component(procedure_kind)
    )
}

pub fn procedure_geometry_key(
    airport_id: &str,
    procedure_kind: &ProcedureKind,
    procedure_id: &str,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> String {
    format!(
        "{}{}/{}",
        procedure_geometry_prefix(airport_id, procedure_kind, procedure_id),
        optional_transition_component(runway_transition),
        optional_transition_component(enroute_transition)
    )
}

fn optional_transition_component(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(upper_component)
        .unwrap_or_else(|| "_".to_string())
}

fn upper_component(value: &str) -> String {
    component(&value.to_uppercase())
}

fn component(value: &str) -> String {
    percent_encode_component(value.trim().as_bytes())
}

fn percent_encode_component(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        {
            out.push(ch);
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => unreachable!("hex nybble out of range"),
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset + 4;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| "nav_kv root truncated while reading u32".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_value_spanning_pages_after_pages_are_supplied() {
        let (root, pages) = fixture(
            &[
                ("a", b"abc".as_slice()),
                ("b", b"defghij".as_slice()),
                ("z", b"k".as_slice()),
            ],
            4,
        );
        let mut store = NavKvStore::new(root);

        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::MissingPages(vec![2, 3, 4, 5])
        );
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::Hit(b"defghij".to_vec())
        );
        assert_eq!(store.get_bytes("missing").unwrap(), NavKvLookup::MissingKey);
    }

    #[test]
    fn rejects_unsupported_version() {
        let (mut root, _) = build_root(&[("a", b"1".as_slice())], 8);
        write_u32(&mut root, 16, 1);
        let err = NavKvRoot::parse(&root).unwrap_err();
        assert!(err.contains("unsupported nav_kv version"), "{err}");
    }

    #[test]
    fn builds_plate_and_procedure_keys_in_core() {
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::PackageById {
                package_id: " world-basemap ".to_string()
            }),
            Some("package/by-id/world-basemap".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::PackageById {
                package_id: "NW_SEC_2604".to_string()
            }),
            Some("package/by-id/NW_SEC_2604".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::PlateAirport {
                airport_id: " krdd ".to_string()
            }),
            Some("plate/airport/KRDD".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::PlateById {
                plate_id: "plate:KRDD:IAP-CA-ILS OR LOC RWY 34.png".to_string()
            }),
            Some("plate/by-id/plate%3AKRDD%3AIAP-CA-ILS%20OR%20LOC%20RWY%2034.png".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::ProcedureGeometry {
                airport_id: "kgrk".to_string(),
                procedure_kind: ProcedureKind::Approach,
                procedure_id: "vor-a".to_string(),
                runway_transition: None,
                enroute_transition: Some("darte".to_string()),
            }),
            Some("procedure/geometry/KGRK/APPROACH/VOR-A/_/DARTE".to_string())
        );
    }

    #[test]
    fn builds_nav_ref_keys_in_core() {
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::NavRefPosition {
                nav_ref: NavRef::Fix("RW34".to_string()),
                procedure_airport_id: Some("krdd".to_string()),
            }),
            Some("navref/position/runway/KRDD/RW34".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::NavRefSymbol {
                nav_ref: NavRef::LatLon(crate::LatLon { lat: 1.0, lon: 2.0 }),
            }),
            None
        );
    }

    fn fixture(entries: &[(&str, &[u8])], page_size: u32) -> (NavKvRoot, Vec<Vec<u8>>) {
        let (root, logical) = build_root(entries, page_size);
        let pages = logical
            .chunks(page_size as usize)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        (NavKvRoot::parse(&root).unwrap(), pages)
    }

    fn build_root(entries: &[(&str, &[u8])], page_size: u32) -> (Vec<u8>, Vec<u8>) {
        let mut key_bytes = Vec::new();
        let mut value_bytes = Vec::new();
        let mut value_offset = 0u32;
        let mut table = Vec::new();
        for (key, value) in entries {
            table.push(Entry {
                key_offset: key_bytes.len() as u32,
                value_offset,
            });
            key_bytes.extend_from_slice(key.as_bytes());
            value_bytes.extend_from_slice(value);
            value_offset += value.len() as u32;
        }
        table.push(Entry {
            key_offset: key_bytes.len() as u32,
            value_offset,
        });

        let mut offset_table = Vec::new();
        for entry in table {
            offset_table.extend_from_slice(&entry.key_offset.to_le_bytes());
            offset_table.extend_from_slice(&entry.value_offset.to_le_bytes());
        }
        let offset_table_len = offset_table.len() as u32;
        let key_table_offset = offset_table_len;
        let key_table_len = key_bytes.len() as u32;
        let value_table_offset = key_table_offset + key_table_len;
        let value_bytes_len = value_bytes.len() as u32;
        let logical_bytes_len = value_table_offset + value_bytes_len;
        let mut logical = Vec::new();
        logical.extend_from_slice(&offset_table);
        logical.extend_from_slice(&key_bytes);
        logical.extend_from_slice(&value_bytes);

        let mut root = vec![0; HEADER_LEN];
        root[..MAGIC.len()].copy_from_slice(MAGIC);
        write_u32(&mut root, 16, VERSION);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, 0);
        write_u32(&mut root, 32, offset_table_len);
        write_u32(&mut root, 36, key_table_offset);
        write_u32(&mut root, 40, key_table_len);
        write_u32(&mut root, 44, value_table_offset);
        write_u32(&mut root, 48, value_bytes_len);
        write_u32(&mut root, 52, logical_bytes_len);
        (root, logical)
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
