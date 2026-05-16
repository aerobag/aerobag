use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{NavRef, ProcedureKind};

const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
const VERSION: u32 = 4;
const HEADER_LEN: usize = 64;
const NODE_KIND_LEAF: u32 = 1;
const NODE_KIND_INTERNAL: u32 = 2;
const LEAF_HEADER_LEN: usize = 12;
const INTERNAL_HEADER_LEN: usize = 12;
const NO_PAGE: u32 = u32::MAX;
const VALUE_KIND_EXTERNAL: u32 = 0;
const VALUE_KIND_INLINE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvRoot {
    entry_count: u32,
    page_size: u32,
    root_page: u32,
    page_count: u32,
    value_page_start: u32,
    value_bytes_len: u32,
    prefetch_pages: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LeafEntryValue {
    Inline(Vec<u8>),
    External { offset: u32, len: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LeafLookup {
    key: Vec<u8>,
    value: LeafEntryValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LeafNode {
    next_leaf: Option<u32>,
    entries: Vec<LeafLookup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalNode {
    children: Vec<u32>,
    pivots: Vec<Vec<u8>>,
}

enum Node {
    Leaf(LeafNode),
    Internal(InternalNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavKvLookup {
    Hit(Vec<u8>),
    MissingKey,
    MissingPages(Vec<u32>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NavKvLookupDiagnostic<'a> {
    pub kind: &'a str,
    pub result: &'a str,
    pub size: usize,
    pub pages: Vec<u32>,
    pub key: &'a str,
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
    MagneticVariation {
        lat: i32,
        lon: i32,
    },
    WaypointIdentifier {
        identifier: String,
    },
    WaypointPrefix {
        prefix: String,
    },
    VectorManifest,
    VectorTile {
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
        let root_page = read_u32(root_bytes, 28)?;
        let page_count = read_u32(root_bytes, 32)?;
        let value_page_start = read_u32(root_bytes, 36)?;
        let value_bytes_len = read_u32(root_bytes, 40)?;
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        if page_count == 0 {
            return Err("nav_kv page_count must be non-zero".to_string());
        }
        if root_page >= page_count {
            return Err("nav_kv root_page exceeds page count".to_string());
        }
        if value_page_start > page_count {
            return Err("nav_kv value_page_start exceeds page count".to_string());
        }
        let value_page_count = value_bytes_len.div_ceil(page_size);
        if value_page_start + value_page_count > page_count {
            return Err("nav_kv value bytes exceed value pages".to_string());
        }
        let prefetch_page_count = read_u32(root_bytes, 56)? as usize;
        let reserved = read_u32(root_bytes, 60)?;
        if reserved != 0 {
            return Err("nav_kv v4 reserved root field must be zero".to_string());
        }
        let expected_root_len = HEADER_LEN
            .checked_add(
                prefetch_page_count
                    .checked_mul(4)
                    .ok_or_else(|| "nav_kv prefetch page count overflow".to_string())?,
            )
            .ok_or_else(|| "nav_kv v4 root length overflow".to_string())?;
        if root_bytes.len() != expected_root_len {
            return Err("nav_kv v4 root length does not match prefetch page count".to_string());
        }
        let mut prefetch_pages = Vec::with_capacity(prefetch_page_count);
        let mut previous_prefetch_page = None;
        for index in 0..prefetch_page_count {
            let page = read_u32(root_bytes, HEADER_LEN + index * 4)?;
            if page >= page_count {
                return Err("nav_kv prefetch page exceeds page count".to_string());
            }
            if let Some(previous) = previous_prefetch_page {
                if page <= previous {
                    return Err("nav_kv prefetch pages must be sorted and unique".to_string());
                }
            }
            previous_prefetch_page = Some(page);
            prefetch_pages.push(page);
        }
        Ok(Self {
            entry_count,
            page_size,
            root_page,
            page_count,
            value_page_start,
            value_bytes_len,
            prefetch_pages,
        })
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn value_bytes_len(&self) -> u32 {
        self.value_bytes_len
    }

    pub fn logical_bytes_len(&self) -> u32 {
        self.page_count.saturating_mul(self.page_size)
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn prefetch_pages(&self) -> &[u32] {
        &self.prefetch_pages
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
        let entry = match self.find_leaf_entry(key.as_bytes())? {
            RangeRead::Hit(Some(entry)) => entry,
            RangeRead::Hit(None) => {
                log_nav_kv_lookup("exact", key, "missing_key", &[], 0);
                return Ok(NavKvLookup::MissingKey);
            }
            RangeRead::MissingPages(pages) => {
                log_nav_kv_lookup("exact", key, "missing_pages", &pages, 0);
                return Ok(NavKvLookup::MissingPages(pages));
            }
        };
        match entry.value {
            LeafEntryValue::Inline(bytes) => {
                log_nav_kv_lookup("exact", key, "hit", &[], bytes.len());
                Ok(NavKvLookup::Hit(bytes))
            }
            LeafEntryValue::External { offset, len } => {
                match self.read_external_value(offset, len)? {
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
        let mut leaf_page = match self.find_leaf_page_for_key(prefix.as_bytes())? {
            RangeRead::Hit(page) => page,
            RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
        };
        let mut out = Vec::new();
        loop {
            let leaf = match self.read_leaf(leaf_page)? {
                RangeRead::Hit(leaf) => leaf,
                RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
            };
            for entry in &leaf.entries {
                if entry.key.as_slice() < prefix.as_bytes() {
                    continue;
                }
                if !entry.key.starts_with(prefix.as_bytes()) {
                    return Ok(RangeRead::Hit(out));
                }
                out.push(String::from_utf8_lossy(&entry.key).into_owned());
            }
            match leaf.next_leaf {
                Some(next) => leaf_page = next,
                None => return Ok(RangeRead::Hit(out)),
            }
        }
    }

    fn find_leaf_entry(&self, key: &[u8]) -> Result<RangeRead<Option<LeafLookup>>, String> {
        let leaf_page = match self.find_leaf_page_for_key(key)? {
            RangeRead::Hit(page) => page,
            RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
        };
        let leaf = match self.read_leaf(leaf_page)? {
            RangeRead::Hit(leaf) => leaf,
            RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
        };
        Ok(RangeRead::Hit(
            leaf.entries
                .binary_search_by(|entry| entry.key.as_slice().cmp(key))
                .ok()
                .and_then(|index| leaf.entries.get(index).cloned()),
        ))
    }

    fn find_leaf_page_for_key(&self, key: &[u8]) -> Result<RangeRead<u32>, String> {
        let mut page_index = self.root.root_page;
        loop {
            match self.read_node(page_index)? {
                RangeRead::Hit(Node::Leaf(_)) => return Ok(RangeRead::Hit(page_index)),
                RangeRead::Hit(Node::Internal(node)) => {
                    let child_index = node.pivots.partition_point(|pivot| pivot.as_slice() <= key);
                    page_index = *node
                        .children
                        .get(child_index)
                        .ok_or_else(|| "nav_kv internal child index out of range".to_string())?;
                }
                RangeRead::MissingPages(pages) => return Ok(RangeRead::MissingPages(pages)),
            }
        }
    }

    fn read_node(&self, page: u32) -> Result<RangeRead<Node>, String> {
        if page >= self.root.value_page_start {
            return Err("nav_kv attempted to read value page as node".to_string());
        }
        let bytes = match self.page_bytes(page) {
            Some(bytes) => bytes,
            None => return Ok(RangeRead::MissingPages(vec![page])),
        };
        match read_u32(bytes, 0)? {
            NODE_KIND_LEAF => Ok(RangeRead::Hit(Node::Leaf(parse_leaf_node(bytes)?))),
            NODE_KIND_INTERNAL => Ok(RangeRead::Hit(Node::Internal(parse_internal_node(bytes)?))),
            _ => Err("nav_kv node has invalid kind".to_string()),
        }
    }

    fn read_leaf(&self, page: u32) -> Result<RangeRead<LeafNode>, String> {
        match self.read_node(page)? {
            RangeRead::Hit(Node::Leaf(leaf)) => Ok(RangeRead::Hit(leaf)),
            RangeRead::Hit(Node::Internal(_)) => Err("nav_kv expected leaf node".to_string()),
            RangeRead::MissingPages(pages) => Ok(RangeRead::MissingPages(pages)),
        }
    }

    fn read_external_value(&self, start: u32, len: u32) -> Result<RangeRead<Vec<u8>>, String> {
        if len == 0 {
            return Ok(RangeRead::Hit(Vec::new()));
        }
        let end = start
            .checked_add(len)
            .ok_or_else(|| "nav_kv value range overflow".to_string())?;
        if end > self.root.value_bytes_len {
            return Err("nav_kv value range exceeds value bytes length".to_string());
        }
        let start_page = start / self.root.page_size;
        let end_page = (end - 1) / self.root.page_size;
        let missing = (start_page..=end_page)
            .map(|value_page| self.root.value_page_start + value_page)
            .filter(|page| !self.pages.contains_key(page))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Ok(RangeRead::MissingPages(missing));
        }
        let mut out = Vec::with_capacity(len as usize);
        for value_page in start_page..=end_page {
            let page_index = self.root.value_page_start + value_page;
            let page = self
                .pages
                .get(&page_index)
                .ok_or_else(|| format!("nav_kv page {page_index} missing"))?;
            let page_start = value_page
                .checked_mul(self.root.page_size)
                .ok_or_else(|| "nav_kv value page start overflow".to_string())?;
            let slice_start = start.saturating_sub(page_start) as usize;
            let slice_end = (end.min(page_start + self.root.page_size) - page_start) as usize;
            if slice_start > slice_end || slice_end > page.len() {
                return Err(format!("nav_kv page {page_index} is too short"));
            }
            out.extend_from_slice(&page[slice_start..slice_end]);
        }
        Ok(RangeRead::Hit(out))
    }

    fn page_bytes(&self, page: u32) -> Option<&[u8]> {
        self.pages.get(&page).map(Vec::as_slice)
    }
}

fn parse_leaf_node(bytes: &[u8]) -> Result<LeafNode, String> {
    let count = read_u32(bytes, 4)? as usize;
    let next_raw = read_u32(bytes, 8)?;
    let mut offset = LEAF_HEADER_LEN;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let value_kind = read_u32(bytes, offset)?;
        offset += 4;
        let value_a = read_u32(bytes, offset)?;
        offset += 4;
        let value_b = read_u32(bytes, offset)?;
        offset += 4;
        let key = bytes
            .get(offset..offset + key_len)
            .ok_or_else(|| "nav_kv leaf key extends past page".to_string())?
            .to_vec();
        offset += key_len;
        let value = match value_kind {
            VALUE_KIND_INLINE => {
                let value_len = value_a as usize;
                let value = bytes
                    .get(offset..offset + value_len)
                    .ok_or_else(|| "nav_kv inline value extends past page".to_string())?
                    .to_vec();
                offset += value_len;
                LeafEntryValue::Inline(value)
            }
            VALUE_KIND_EXTERNAL => LeafEntryValue::External {
                offset: value_a,
                len: value_b,
            },
            _ => return Err("nav_kv leaf entry has invalid value kind".to_string()),
        };
        entries.push(LeafLookup { key, value });
    }
    Ok(LeafNode {
        next_leaf: (next_raw != NO_PAGE).then_some(next_raw),
        entries,
    })
}

fn parse_internal_node(bytes: &[u8]) -> Result<InternalNode, String> {
    let pivot_count = read_u32(bytes, 4)? as usize;
    let child_count = read_u32(bytes, 8)? as usize;
    if child_count != pivot_count + 1 {
        return Err("nav_kv internal child/pivot count mismatch".to_string());
    }
    let mut offset = INTERNAL_HEADER_LEN;
    let mut children = Vec::with_capacity(child_count);
    for _ in 0..child_count {
        children.push(read_u32(bytes, offset)?);
        offset += 4;
    }
    let mut pivots = Vec::with_capacity(pivot_count);
    for _ in 0..pivot_count {
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let key = bytes
            .get(offset..offset + key_len)
            .ok_or_else(|| "nav_kv internal pivot extends past page".to_string())?
            .to_vec();
        offset += key_len;
        pivots.push(key);
    }
    Ok(InternalNode { children, pivots })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RangeRead<T> {
    Hit(T),
    MissingPages(Vec<u32>),
}

fn log_nav_kv_lookup(kind: &str, key: &str, result: &str, pages: &[u32], size: usize) {
    let diagnostic = NavKvLookupDiagnostic {
        kind,
        result,
        size,
        pages: pages.to_vec(),
        key,
    };
    crate::core_debug_log("NAV_KV_LOOKUP", &diagnostic);
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
        NavKvQuery::MagneticVariation { lat, lon } => Some(format!("magvar/{lat}/{lon}")),
        NavKvQuery::WaypointIdentifier { identifier } => Some(format!(
            "waypoint/identifier/{}",
            upper_component(identifier)
        )),
        NavKvQuery::WaypointPrefix { prefix } => {
            let normalized = prefix.trim().to_uppercase();
            Some(format!("waypoint/prefix/{}", component(&normalized)))
        }
        NavKvQuery::VectorManifest => Some("vector/manifest".to_string()),
        NavKvQuery::VectorTile { z, x, y } => Some(format!("vector/tile/z{z:02}/x{x:06}/y{y:06}")),
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
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
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
pub(crate) fn nav_kv_store_for_test(entries: &[(&str, &[u8])], page_size: u32) -> NavKvStore {
    let (root, pages) = test_build_root(entries, page_size);
    let mut store = NavKvStore::new(NavKvRoot::parse(&root).unwrap());
    for (index, page) in pages.into_iter().enumerate() {
        store.insert_page(index as u32, page);
    }
    store
}

#[cfg(test)]
pub(crate) fn nav_kv_store_without_pages_for_test(
    entries: &[(&str, &[u8])],
    page_size: u32,
) -> NavKvStore {
    let (root, _) = test_build_root(entries, page_size);
    NavKvStore::new(NavKvRoot::parse(&root).unwrap())
}

#[cfg(test)]
fn test_build_root(entries: &[(&str, &[u8])], page_size: u32) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut entries = entries.to_vec();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    let mut value_bytes = Vec::new();
    let mut leaf = Vec::new();
    leaf.extend_from_slice(&NODE_KIND_LEAF.to_le_bytes());
    leaf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    leaf.extend_from_slice(&NO_PAGE.to_le_bytes());
    for (key, value) in &entries {
        leaf.extend_from_slice(&(key.len() as u32).to_le_bytes());
        if value.len() > 4 {
            let offset = value_bytes.len() as u32;
            leaf.extend_from_slice(&VALUE_KIND_EXTERNAL.to_le_bytes());
            leaf.extend_from_slice(&offset.to_le_bytes());
            leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
            value_bytes.extend_from_slice(value);
        } else {
            leaf.extend_from_slice(&VALUE_KIND_INLINE.to_le_bytes());
            leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
            leaf.extend_from_slice(&0u32.to_le_bytes());
        }
        leaf.extend_from_slice(key.as_bytes());
        if value.len() <= 4 {
            leaf.extend_from_slice(value);
        }
    }
    assert!(leaf.len() <= page_size as usize);
    let mut pages = vec![leaf];
    let value_page_start = pages.len() as u32;
    pages.extend(
        value_bytes
            .chunks(page_size as usize)
            .map(|chunk| chunk.to_vec()),
    );
    let page_count = pages.len() as u32;

    let mut root = vec![0; HEADER_LEN];
    root[..MAGIC.len()].copy_from_slice(MAGIC);
    test_write_u32(&mut root, 16, VERSION);
    test_write_u32(&mut root, 20, entries.len() as u32);
    test_write_u32(&mut root, 24, page_size);
    test_write_u32(&mut root, 28, 0);
    test_write_u32(&mut root, 32, page_count);
    test_write_u32(&mut root, 36, value_page_start);
    test_write_u32(&mut root, 40, value_bytes.len() as u32);
    (root, pages)
}

#[cfg(test)]
fn test_write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
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
            128,
        );
        let mut store = NavKvStore::new(root);

        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::MissingPages(vec![0])
        );
        store.insert_page(0, pages[0].clone());
        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::MissingPages(vec![1])
        );
        for (index, page) in pages.into_iter().enumerate().skip(1) {
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
        let (mut root, _) = build_root(&[("a", b"1".as_slice())], 64);
        write_u32(&mut root, 16, 1);
        let err = NavKvRoot::parse(&root).unwrap_err();
        assert!(err.contains("unsupported nav_kv version"), "{err}");
    }

    #[test]
    fn parses_v4_prefetch_pages() {
        let (mut root, _) = build_root(&[("a", b"1".as_slice()), ("b", b"2".as_slice())], 64);
        write_u32(&mut root, 56, 1);
        root.extend_from_slice(&0u32.to_le_bytes());

        let root = NavKvRoot::parse(&root).unwrap();
        assert_eq!(root.prefetch_pages(), &[0]);
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
            nav_kv_key_for_query(&NavKvQuery::PlateProcedureCandidates {
                plate_id: "plate:KORS:IAP-WA-RNAV (GPS)-A.png".to_string()
            }),
            Some(
                "plate/procedure-candidates/plate%3AKORS%3AIAP-WA-RNAV%20%28GPS%29-A.png"
                    .to_string()
            )
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::MagneticVariation { lat: 48, lon: -110 }),
            Some("magvar/48/-110".to_string())
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
        let (root, pages) = build_root(entries, page_size);
        (NavKvRoot::parse(&root).unwrap(), pages)
    }

    fn build_root(entries: &[(&str, &[u8])], page_size: u32) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut value_bytes = Vec::new();
        let mut leaf = Vec::new();
        leaf.extend_from_slice(&NODE_KIND_LEAF.to_le_bytes());
        leaf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        leaf.extend_from_slice(&NO_PAGE.to_le_bytes());
        for (key, value) in entries {
            leaf.extend_from_slice(&(key.len() as u32).to_le_bytes());
            if value.len() > 4 {
                let offset = value_bytes.len() as u32;
                leaf.extend_from_slice(&VALUE_KIND_EXTERNAL.to_le_bytes());
                leaf.extend_from_slice(&offset.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                value_bytes.extend_from_slice(value);
            } else {
                leaf.extend_from_slice(&VALUE_KIND_INLINE.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                leaf.extend_from_slice(&0u32.to_le_bytes());
            }
            leaf.extend_from_slice(key.as_bytes());
            if value.len() <= 4 {
                leaf.extend_from_slice(value);
            }
        }
        assert!(leaf.len() <= page_size as usize);
        let mut pages = vec![leaf];
        let value_page_start = pages.len() as u32;
        pages.extend(
            value_bytes
                .chunks(page_size as usize)
                .map(|chunk| chunk.to_vec()),
        );
        let page_count = pages.len() as u32;

        let mut root = vec![0; HEADER_LEN];
        root[..MAGIC.len()].copy_from_slice(MAGIC);
        write_u32(&mut root, 16, VERSION);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, 0);
        write_u32(&mut root, 32, page_count);
        write_u32(&mut root, 36, value_page_start);
        write_u32(&mut root, 40, value_bytes.len() as u32);
        (root, pages)
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
