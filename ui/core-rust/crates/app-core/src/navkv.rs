use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{NavRef, ProcedureKind};

const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 48;
const ENTRY_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    key_offset: u32,
    value_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvRoot {
    entries: Vec<Entry>,
    key_bytes: Vec<u8>,
    page_size: u32,
    value_bytes_len: u32,
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
        let real_entry_count = read_u32(root_bytes, 20)? as usize;
        let page_size = read_u32(root_bytes, 24)?;
        let entry_table_offset = read_u32(root_bytes, 28)? as usize;
        let key_bytes_offset = read_u32(root_bytes, 32)? as usize;
        let key_bytes_len = read_u32(root_bytes, 36)? as usize;
        let value_bytes_len = read_u32(root_bytes, 40)?;
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        let entry_count = real_entry_count + 1;
        let entry_bytes_len = entry_count
            .checked_mul(ENTRY_LEN)
            .ok_or_else(|| "nav_kv entry table length overflow".to_string())?;
        if entry_table_offset != HEADER_LEN {
            return Err("nav_kv entry table offset must follow header in v1".to_string());
        }
        if key_bytes_offset != entry_table_offset + entry_bytes_len {
            return Err("nav_kv key bytes offset does not follow entry table".to_string());
        }
        if root_bytes.len() != key_bytes_offset + key_bytes_len {
            return Err("nav_kv root length does not match key bytes length".to_string());
        }

        let mut entries = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let offset = entry_table_offset + index * ENTRY_LEN;
            entries.push(Entry {
                key_offset: read_u32(root_bytes, offset)?,
                value_offset: read_u32(root_bytes, offset + 4)?,
            });
        }
        let key_bytes = root_bytes[key_bytes_offset..].to_vec();
        validate_parts(&entries, &key_bytes, value_bytes_len)?;
        Ok(Self {
            entries,
            key_bytes,
            page_size,
            value_bytes_len,
        })
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn value_bytes_len(&self) -> u32 {
        self.value_bytes_len
    }

    pub fn len(&self) -> usize {
        self.entries.len() - 1
    }

    pub fn value_range(&self, key: &str) -> Option<(u32, u32)> {
        let target = key.as_bytes();
        let mut left = 0usize;
        let mut right = self.len();
        while left < right {
            let mid = left + (right - left) / 2;
            match self.key_at(mid).cmp(target) {
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => right = mid,
                std::cmp::Ordering::Equal => return Some(self.value_range_at(mid)),
            }
        }
        None
    }

    pub fn value_pages(&self, key: &str) -> Option<Vec<u32>> {
        let (start, end) = self.value_range(key)?;
        if start == end {
            return Some(Vec::new());
        }
        let start_page = start / self.page_size;
        let end_page = (end - 1) / self.page_size;
        Some((start_page..=end_page).collect())
    }

    fn key_at(&self, index: usize) -> &[u8] {
        let start = self.entries[index].key_offset as usize;
        let end = self.entries[index + 1].key_offset as usize;
        &self.key_bytes[start..end]
    }

    fn value_range_at(&self, index: usize) -> (u32, u32) {
        (
            self.entries[index].value_offset,
            self.entries[index + 1].value_offset,
        )
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
        let Some((start, end)) = self.root.value_range(key) else {
            return Ok(NavKvLookup::MissingKey);
        };
        if start == end {
            return Ok(NavKvLookup::Hit(Vec::new()));
        }

        let pages = self.root.value_pages(key).unwrap_or_default();
        let missing = pages
            .iter()
            .copied()
            .filter(|page| !self.pages.contains_key(page))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Ok(NavKvLookup::MissingPages(missing));
        }

        let mut out = Vec::with_capacity((end - start) as usize);
        for page_index in pages {
            let page = self
                .pages
                .get(&page_index)
                .ok_or_else(|| format!("nav_kv page {page_index} missing"))?;
            let page_start = page_index
                .checked_mul(self.root.page_size)
                .ok_or_else(|| "nav_kv page start overflow".to_string())?;
            let slice_start = start.saturating_sub(page_start) as usize;
            let slice_end = end.min(page_start + self.root.page_size) - page_start;
            let slice_end = slice_end as usize;
            if slice_start > slice_end || slice_end > page.len() {
                return Err(format!("nav_kv value page {page_index} is too short"));
            }
            out.extend_from_slice(&page[slice_start..slice_end]);
        }
        Ok(NavKvLookup::Hit(out))
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
        (0..self.root.len())
            .filter_map(|index| {
                let key = std::str::from_utf8(self.root.key_at(index)).ok()?;
                key.starts_with(prefix).then(|| key.to_string())
            })
            .collect()
    }
}

pub fn nav_kv_key_for_query(query: &NavKvQuery) -> Option<String> {
    match query {
        NavKvQuery::ChartCatalog => Some("chart/catalog".to_string()),
        NavKvQuery::PackageById { package_id } => {
            Some(format!("package/by-id/{}", upper_component(package_id)))
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
        NavRef::LatLon(_) => None,
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
        NavRef::LatLon(_) => None,
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

fn validate_parts(entries: &[Entry], key_bytes: &[u8], value_bytes_len: u32) -> Result<(), String> {
    if entries.len() < 2 {
        return Err("nav_kv needs at least one real entry plus sentinel".to_string());
    }
    let sentinel = entries[entries.len() - 1];
    if sentinel.key_offset as usize != key_bytes.len() {
        return Err("nav_kv sentinel key offset must equal key_bytes_len".to_string());
    }
    if sentinel.value_offset != value_bytes_len {
        return Err("nav_kv sentinel value offset must equal value_bytes_len".to_string());
    }
    for index in 0..entries.len() - 1 {
        let current = entries[index];
        let next = entries[index + 1];
        if current.key_offset >= next.key_offset {
            return Err("nav_kv key offsets must be strictly increasing".to_string());
        }
        if current.value_offset >= next.value_offset {
            return Err("nav_kv values must be non-empty and increasing".to_string());
        }
        if next.key_offset as usize > key_bytes.len() {
            return Err("nav_kv key offset exceeds key byte length".to_string());
        }
        if next.value_offset > value_bytes_len {
            return Err("nav_kv value offset exceeds value byte length".to_string());
        }
        if index > 0 && entries[index - 1].value_offset >= current.value_offset {
            return Err("nav_kv value offsets must be increasing".to_string());
        }
        if index > 0 {
            let previous_key = key_slice(entries, key_bytes, index - 1);
            let current_key = key_slice(entries, key_bytes, index);
            if previous_key >= current_key {
                return Err("nav_kv keys must be strictly sorted".to_string());
            }
        }
    }
    Ok(())
}

fn key_slice<'a>(entries: &[Entry], key_bytes: &'a [u8], index: usize) -> &'a [u8] {
    &key_bytes[entries[index].key_offset as usize..entries[index + 1].key_offset as usize]
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
            NavKvLookup::MissingPages(vec![0, 1, 2])
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
    fn rejects_unsorted_keys() {
        let mut root = build_root(&[("b", b"1".as_slice()), ("a", b"2".as_slice())], 8);
        let err = NavKvRoot::parse(&root).unwrap_err();
        assert!(err.contains("strictly sorted"), "{err}");

        root[HEADER_LEN] = 0;
    }

    #[test]
    fn builds_plate_and_procedure_keys_in_core() {
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
        let root = build_root(entries, page_size);
        let values = entries
            .iter()
            .flat_map(|(_, value)| value.iter().copied())
            .collect::<Vec<_>>();
        let pages = values
            .chunks(page_size as usize)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        (NavKvRoot::parse(&root).unwrap(), pages)
    }

    fn build_root(entries: &[(&str, &[u8])], page_size: u32) -> Vec<u8> {
        let mut key_bytes = Vec::new();
        let mut value_offset = 0u32;
        let mut table = Vec::new();
        for (key, value) in entries {
            table.push(Entry {
                key_offset: key_bytes.len() as u32,
                value_offset,
            });
            key_bytes.extend_from_slice(key.as_bytes());
            value_offset += value.len() as u32;
        }
        table.push(Entry {
            key_offset: key_bytes.len() as u32,
            value_offset,
        });

        let mut root = vec![0; HEADER_LEN];
        root[..MAGIC.len()].copy_from_slice(MAGIC);
        write_u32(&mut root, 16, VERSION);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, HEADER_LEN as u32);
        write_u32(&mut root, 32, (HEADER_LEN + table.len() * ENTRY_LEN) as u32);
        write_u32(&mut root, 36, key_bytes.len() as u32);
        write_u32(&mut root, 40, value_offset);
        for entry in table {
            root.extend_from_slice(&entry.key_offset.to_le_bytes());
            root.extend_from_slice(&entry.value_offset.to_le_bytes());
        }
        root.extend_from_slice(&key_bytes);
        root
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
