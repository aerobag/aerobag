use had_key::{component, upper_component};
pub use had_nav_kv::{NavKvLookup, NavKvRoot, NavKvStore};
use serde::{Deserialize, Serialize};

use crate::{NavRef, ProcedureKind};

pub const REQUIRED_NAV_DB_CONTRACT_VERSION: u32 = 1;
pub const NAV_DB_CONTRACT_KEY: &str = "contract/nav-db";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NavKvQuery {
    NavDbContract,
    ChartCatalog,
    OfflineRegionCatalog,
    MetarImportantStations,
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
    ObstacleTile {
        z: u32,
        x: u32,
        y: u32,
    },
    VectorAirspaceFeature {
        id: String,
    },
}

pub fn nav_kv_key_for_query(query: &NavKvQuery) -> Option<String> {
    match query {
        NavKvQuery::NavDbContract => Some(NAV_DB_CONTRACT_KEY.to_string()),
        NavKvQuery::ChartCatalog => Some("chart/catalog".to_string()),
        NavKvQuery::OfflineRegionCatalog => Some("offline-region/catalog".to_string()),
        NavKvQuery::MetarImportantStations => Some("weather/metar-important-stations".to_string()),
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
        NavKvQuery::ObstacleTile { z, x, y } => {
            Some(format!("obstacle/tile/z{z:02}/x{x:06}/y{y:06}"))
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

#[cfg(any(test, debug_assertions))]
pub(crate) fn nav_kv_store_for_test(entries: &[(&str, &[u8])], page_size: u32) -> NavKvStore {
    let (root, pages) = test_build_root(entries, page_size);
    let mut store = NavKvStore::new(root);
    for (index, page) in pages.into_iter().enumerate() {
        store.insert_page(index as u32, page);
    }
    store
}

#[cfg(debug_assertions)]
pub fn nav_kv_store_for_smoke_test(entries: &[(&str, &[u8])], page_size: u32) -> NavKvStore {
    nav_kv_store_for_test(entries, page_size)
}

#[cfg(test)]
pub(crate) fn nav_kv_store_without_pages_for_test(
    entries: &[(&str, &[u8])],
    page_size: u32,
) -> NavKvStore {
    let (root, _) = test_build_root(entries, page_size);
    NavKvStore::new(root)
}

#[cfg(test)]
pub(crate) fn nav_kv_store_without_pages_and_pages_for_test(
    entries: &[(&str, &[u8])],
    page_size: u32,
) -> (NavKvStore, Vec<Vec<u8>>) {
    let (root, pages) = test_build_root(entries, page_size);
    (NavKvStore::new(root), pages)
}

#[cfg(any(test, debug_assertions))]
fn test_build_root(entries: &[(&str, &[u8])], page_size: u32) -> (NavKvRoot, Vec<Vec<u8>>) {
    let pairs = if entries.is_empty() {
        vec![had_nav_kv::NavKvPair {
            key: "__test__/dummy".to_string(),
            value: b"{}".to_vec(),
        }]
    } else {
        entries
            .iter()
            .map(|(key, value)| had_nav_kv::NavKvPair {
                key: (*key).to_string(),
                value: (*value).to_vec(),
            })
            .collect()
    };
    let built = had_nav_kv::build_nav_kv_sorted(pairs, page_size).expect("build nav_kv fixture");
    (
        NavKvRoot::parse(&built.root_bytes).expect("parse nav_kv fixture root"),
        built.pages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_plate_and_procedure_keys_in_core() {
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::NavDbContract),
            Some("contract/nav-db".to_string())
        );
        assert_eq!(
            nav_kv_key_for_query(&NavKvQuery::MetarImportantStations),
            Some("weather/metar-important-stations".to_string())
        );
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

    #[test]
    fn test_store_helpers_use_shared_nav_kv_reader() {
        let mut store = nav_kv_store_without_pages_for_test(
            &[("a", b"abc".as_slice()), ("b", b"defghij".as_slice())],
            128,
        );
        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::MissingPages(vec![0])
        );

        let (_, pages) = test_build_root(
            &[("a", b"abc".as_slice()), ("b", b"defghij".as_slice())],
            128,
        );
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        assert_eq!(
            store.get_bytes("b").unwrap(),
            NavKvLookup::Hit(b"defghij".to_vec())
        );
    }
}
