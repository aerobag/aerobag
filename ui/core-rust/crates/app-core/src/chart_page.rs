use serde::{Deserialize, Serialize};

use crate::planning::{FlightPlan, RouteComponent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartPage {
    pub airports: Vec<DerivedChartAirport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartPageState {
    pub airports: Vec<DerivedChartAirport>,
    pub recent_airport_ids: Vec<String>,
    pub selected_airport_id: String,
    pub selected_chart_id: String,
}

pub type DerivedChartCatalog = DerivedChartPage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartAirport {
    pub id: String,
    pub label: String,
    pub charts: Vec<DerivedChartAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartAsset {
    pub id: String,
    pub airport_id: String,
    pub package_id: String,
    pub label: String,
    pub kind: String,
    pub folder_category: String,
    pub source_asset_path: String,
    pub asset_path: String,
    pub asset_url: String,
    pub thumbnail_source_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceIndexChartPageInput {
    pub airport_resources: Vec<ResourceAirportResources>,
    pub plates: Vec<ResourcePlate>,
    pub csups: Vec<ResourceCsup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceAirportResources {
    pub airport_id: String,
    #[serde(default)]
    pub plate_ids: Vec<String>,
    #[serde(default)]
    pub csup_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourcePlate {
    pub id: String,
    pub airport_id: String,
    pub package_id: String,
    pub asset_path: String,
    pub thumbnail_path: Option<String>,
    pub label: String,
    pub asset_kind: String,
    #[serde(default)]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlateGeoref {
    PlateTransformV1 {
        pixels_per_longitude: f64,
        pixels_per_latitude: f64,
        top_left_lon: f64,
        top_left_lat: f64,
    },
    AirportDiagramTransformV1 {
        pixel_x_from_lon: f64,
        pixel_x_from_lat: f64,
        pixel_x_offset: f64,
        pixel_y_from_lon: f64,
        pixel_y_from_lat: f64,
        pixel_y_offset: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceCsup {
    pub id: String,
    pub airport_id: String,
    pub package_id: String,
    pub asset_path: String,
    pub thumbnail_path: Option<String>,
    pub label: String,
    pub asset_kind: String,
}

pub fn derive_chart_page(
    resource_index: &ResourceIndexChartPageInput,
    plan: &FlightPlan,
) -> DerivedChartPage {
    let catalog = build_chart_catalog(resource_index);
    derive_chart_page_from_catalog(&catalog, plan)
}

pub fn derive_chart_page_state(
    resource_index: &ResourceIndexChartPageInput,
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    let catalog = build_chart_catalog(resource_index);
    derive_chart_page_state_from_catalog(
        &catalog,
        plan,
        stored_recent_airport_ids,
        candidate_airport_id,
        candidate_chart_id,
    )
}

pub fn build_chart_catalog(resource_index: &ResourceIndexChartPageInput) -> DerivedChartCatalog {
    let airports = resource_index
        .airport_resources
        .iter()
        .filter_map(|airport_resources| {
            let airport_id = airport_resources.airport_id.clone();
            let mut charts: Vec<DerivedChartAsset> = Vec::new();
            for plate_id in &airport_resources.plate_ids {
                if let Some(plate) = resource_index
                    .plates
                    .iter()
                    .find(|record| &record.id == plate_id)
                {
                    charts.push(chart_asset_for_plate(&airport_id, plate));
                }
            }
            for csup_id in &airport_resources.csup_ids {
                if let Some(csup) = resource_index
                    .csups
                    .iter()
                    .find(|record| &record.id == csup_id)
                {
                    charts.push(chart_asset_for_csup(&airport_id, csup));
                }
            }
            charts.sort_by(|left, right| {
                folder_category_rank(&left.folder_category)
                    .cmp(&folder_category_rank(&right.folder_category))
                    .then_with(|| left.label.cmp(&right.label))
            });
            if charts.is_empty() {
                return None;
            }
            Some(DerivedChartAirport {
                id: airport_id.clone(),
                label: airport_id,
                charts,
            })
        })
        .collect();

    DerivedChartPage { airports }
}

pub fn derive_chart_page_from_catalog(
    catalog: &DerivedChartCatalog,
    plan: &FlightPlan,
) -> DerivedChartPage {
    let mut ordered_airport_ids: Vec<String> = Vec::new();
    for airport_id in airport_ids_from_plan(plan) {
        if !ordered_airport_ids.iter().any(|id| id == &airport_id) {
            ordered_airport_ids.push(airport_id);
        }
    }
    let airports = ordered_airport_ids
        .into_iter()
        .filter_map(|airport_id| {
            catalog
                .airports
                .iter()
                .find(|airport| airport.id == airport_id)
                .cloned()
        })
        .collect();
    DerivedChartPage { airports }
}

pub fn derive_chart_page_state_from_catalog(
    catalog: &DerivedChartCatalog,
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    let page = derive_chart_page_from_catalog(catalog, plan);
    derive_chart_page_state_from_airports(
        page.airports,
        stored_recent_airport_ids,
        candidate_airport_id,
        candidate_chart_id,
    )
}

pub fn derive_chart_page_state_from_airports(
    airports: Vec<DerivedChartAirport>,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    let recent_airport_ids = merge_recent_airport_ids(&airports, stored_recent_airport_ids);
    let selected_airport_id = resolve_airport_id(&airports, candidate_airport_id, &recent_airport_ids);
    let selected_chart_id = resolve_chart_id(&airports, &selected_airport_id, candidate_chart_id);
    DerivedChartPageState {
        airports,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    }
}

pub fn airport_ids_from_plan(plan: &FlightPlan) -> Vec<String> {
    let mut airport_ids = Vec::new();
    if let Some(departure) = &plan.departure {
        airport_ids.push(departure.0.clone());
    }
    if let Some(destination) = &plan.destination {
        airport_ids.push(destination.0.clone());
    }
    if let Some(alternate) = &plan.alternate {
        airport_ids.push(alternate.0.clone());
    }
    for component in &plan.route_components {
        match component {
            RouteComponent::Waypoint { waypoint } => {
                if let Some(code) = waypoint.airport_code() {
                    airport_ids.push(code.to_string());
                }
            }
            RouteComponent::Procedure { procedure } => {
                airport_ids.push(procedure.airport_id.0.clone());
            }
            RouteComponent::Airway { airway } => {
                if let Some(code) = airway.entry.airport_code() {
                    airport_ids.push(code.to_string());
                }
                if let Some(code) = airway.exit.airport_code() {
                    airport_ids.push(code.to_string());
                }
            }
        }
    }
    airport_ids
}

fn merge_recent_airport_ids(
    airports: &[DerivedChartAirport],
    stored_ids: &[String],
) -> Vec<String> {
    let valid_ids: Vec<String> = airports.iter().map(|airport| airport.id.clone()).collect();
    let mut ordered_ids: Vec<String> = Vec::new();
    for id in stored_ids {
        if valid_ids.iter().any(|valid_id| valid_id == id)
            && !ordered_ids.iter().any(|existing| existing == id)
        {
            ordered_ids.push(id.clone());
        }
    }
    for airport in airports {
        if !ordered_ids.iter().any(|id| id == &airport.id) {
            ordered_ids.push(airport.id.clone());
        }
    }
    ordered_ids
}

fn resolve_airport_id(
    airports: &[DerivedChartAirport],
    candidate_airport_id: Option<&str>,
    recent_airport_ids: &[String],
) -> String {
    if let Some(candidate_airport_id) = candidate_airport_id {
        if airports
            .iter()
            .any(|airport| airport.id == candidate_airport_id)
        {
            return candidate_airport_id.to_string();
        }
    }
    recent_airport_ids
        .first()
        .cloned()
        .or_else(|| airports.first().map(|airport| airport.id.clone()))
        .unwrap_or_default()
}

fn resolve_chart_id(
    airports: &[DerivedChartAirport],
    airport_id: &str,
    candidate_chart_id: Option<&str>,
) -> String {
    let airport = airports.iter().find(|airport| airport.id == airport_id);
    if let Some(candidate_chart_id) = candidate_chart_id {
        if airport
            .map(|airport| {
                airport
                    .charts
                    .iter()
                    .any(|chart| chart.id == candidate_chart_id)
            })
            .unwrap_or(false)
        {
            return candidate_chart_id.to_string();
        }
    }
    airport
        .and_then(|airport| airport.charts.first().map(|chart| chart.id.clone()))
        .unwrap_or_default()
}

fn chart_asset_for_plate(airport_id: &str, plate: &ResourcePlate) -> DerivedChartAsset {
    let filename = plate
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&plate.asset_path);
    DerivedChartAsset {
        id: format!("plate:{airport_id}:{filename}"),
        airport_id: airport_id.to_string(),
        package_id: plate.package_id.clone(),
        label: plate.label.clone(),
        kind: "plate".to_string(),
        folder_category: folder_category("plate", raw_label_from_asset_path(&plate.asset_path)),
        source_asset_path: plate.asset_path.clone(),
        asset_path: plate.asset_path.clone(),
        asset_url: format!("/{}", plate.asset_path),
        thumbnail_source_path: plate.thumbnail_path.clone(),
        thumbnail_path: plate.thumbnail_path.clone(),
        thumbnail_url: plate.thumbnail_path.as_ref().map(|path| format!("/{path}")),
        georef: plate.georef.clone(),
    }
}

fn chart_asset_for_csup(airport_id: &str, csup: &ResourceCsup) -> DerivedChartAsset {
    let filename = csup
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&csup.asset_path);
    DerivedChartAsset {
        id: format!("csup:{airport_id}:{filename}"),
        airport_id: airport_id.to_string(),
        package_id: csup.package_id.clone(),
        label: csup.label.clone(),
        kind: "csup".to_string(),
        folder_category: "csup".to_string(),
        source_asset_path: csup.asset_path.clone(),
        asset_path: csup.asset_path.clone(),
        asset_url: format!("/{}", csup.asset_path),
        thumbnail_source_path: csup.thumbnail_path.clone(),
        thumbnail_path: csup.thumbnail_path.clone(),
        thumbnail_url: csup.thumbnail_path.as_ref().map(|path| format!("/{path}")),
        georef: None,
    }
}

fn folder_category(kind: &str, label: &str) -> String {
    if kind == "csup" {
        return "csup".to_string();
    }
    let normalized = label.to_uppercase();
    if normalized.contains("HOT SPOT")
        || normalized.contains("HOT_OR_HOT")
        || normalized.contains("HOT-SPOT")
    {
        "hotspot".to_string()
    } else if normalized.contains("AIRPORT DIAGRAM") {
        "airport-diagram".to_string()
    } else if normalized.starts_with("MIN-")
        || normalized.contains("TAKEOFF MINIMUMS")
        || normalized.contains("ALTERNATE MINIMUMS")
        || normalized.contains("MINIMUMS")
    {
        "takeoff-mins".to_string()
    } else if normalized.starts_with("DP-")
        || normalized.starts_with("ODP-")
        || normalized.contains("DEPARTURE")
    {
        "departure".to_string()
    } else if normalized.starts_with("STAR-") || normalized.contains(" ARRIVAL") {
        "star".to_string()
    } else {
        "approach".to_string()
    }
}

fn raw_label_from_asset_path(asset_path: &str) -> &str {
    asset_path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".png"))
        .unwrap_or(asset_path)
}

fn folder_category_rank(category: &str) -> usize {
    match category {
        "approach" => 0,
        "departure" => 1,
        "star" => 2,
        "airport-diagram" => 3,
        "csup" => 4,
        "takeoff-mins" => 5,
        "hotspot" => 6,
        _ => 7,
    }
}

#[cfg(test)]
mod tests {
    use crate::{AirportId, NavRef, PlanLeg};

    use super::*;

    #[test]
    fn derives_chart_page_for_plan_airports_only() {
        let resource_index = ResourceIndexChartPageInput {
            airport_resources: vec![
                ResourceAirportResources {
                    airport_id: "KSEA".to_string(),
                    plate_ids: vec!["p1".to_string()],
                    csup_ids: vec![],
                },
                ResourceAirportResources {
                    airport_id: "KPAE".to_string(),
                    plate_ids: vec![],
                    csup_ids: vec!["c1".to_string()],
                },
            ],
            plates: vec![ResourcePlate {
                id: "p1".to_string(),
                airport_id: "KSEA".to_string(),
                package_id: "NW_TPP".to_string(),
                asset_path: "plates/SEA/APD-WA-AIRPORT DIAGRAM.png".to_string(),
                thumbnail_path: Some("thumbs/SEA/APD-WA-AIRPORT DIAGRAM.png".to_string()),
                label: "Airport Diagram".to_string(),
                asset_kind: "plate".to_string(),
                georef: None,
            }],
            csups: vec![ResourceCsup {
                id: "c1".to_string(),
                airport_id: "KPAE".to_string(),
                package_id: "NW_CSUP".to_string(),
                asset_path: "csup/KPAE/page1.png".to_string(),
                thumbnail_path: None,
                label: "Chart Supplement".to_string(),
                asset_kind: "csup".to_string(),
            }],
        };
        let plan = FlightPlan {
            id: "1".to_string(),
            name: "plan".to_string(),
            legs: vec![PlanLeg {
                from: NavRef::Airport("KRNT".to_string()),
                to: NavRef::Airport("KSEA".to_string()),
                airway: None,
            }],
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KSEA".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let page = derive_chart_page(&resource_index, &plan);

        assert_eq!(
            page.airports
                .iter()
                .map(|airport| airport.id.as_str())
                .collect::<Vec<_>>(),
            vec!["KSEA"]
        );
        assert_eq!(
            page.airports[0].charts[0].folder_category,
            "airport-diagram"
        );
    }

    #[test]
    fn derives_chart_page_state_with_recent_and_selection_normalization() {
        let resource_index = ResourceIndexChartPageInput {
            airport_resources: vec![
                ResourceAirportResources {
                    airport_id: "KSEA".to_string(),
                    plate_ids: vec!["p1".to_string()],
                    csup_ids: vec![],
                },
                ResourceAirportResources {
                    airport_id: "KPAE".to_string(),
                    plate_ids: vec!["p2".to_string()],
                    csup_ids: vec![],
                },
            ],
            plates: vec![
                ResourcePlate {
                    id: "p1".to_string(),
                    airport_id: "KSEA".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/SEA/APD-WA-AIRPORT DIAGRAM.png".to_string(),
                    thumbnail_path: None,
                    label: "Airport Diagram".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "p2".to_string(),
                    airport_id: "KPAE".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/PAE/DP-WA-RNDR TWO.png".to_string(),
                    thumbnail_path: None,
                    label: "RNDR TWO".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
            ],
            csups: vec![],
        };
        let plan = FlightPlan {
            id: "1".to_string(),
            name: "plan".to_string(),
            legs: vec![PlanLeg {
                from: NavRef::Airport("KRNT".to_string()),
                to: NavRef::Airport("KSEA".to_string()),
                airway: None,
            }],
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KSEA".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let state = derive_chart_page_state(
            &resource_index,
            &plan,
            &["KPAE".to_string()],
            Some("MISSING"),
            Some("missing-chart"),
        );

        assert_eq!(state.recent_airport_ids, vec!["KSEA".to_string()]);
        assert_eq!(state.selected_airport_id, "KSEA");
        assert_eq!(
            state.selected_chart_id,
            "plate:KSEA:APD-WA-AIRPORT DIAGRAM.png"
        );
        assert_eq!(state.airports[0].id, "KSEA");
    }

    #[test]
    fn orders_airport_options_by_terminal_fields_then_route_components() {
        let resource_index = ResourceIndexChartPageInput {
            airport_resources: vec![
                ResourceAirportResources {
                    airport_id: "KPAE".to_string(),
                    plate_ids: vec!["p1".to_string()],
                    csup_ids: vec![],
                },
                ResourceAirportResources {
                    airport_id: "KSFO".to_string(),
                    plate_ids: vec!["p2".to_string()],
                    csup_ids: vec![],
                },
                ResourceAirportResources {
                    airport_id: "KUAO".to_string(),
                    plate_ids: vec!["p3".to_string()],
                    csup_ids: vec![],
                },
            ],
            plates: vec![
                ResourcePlate {
                    id: "p1".to_string(),
                    airport_id: "KPAE".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/PAE/IAP-WA-RNAV RWY 16R.png".to_string(),
                    thumbnail_path: None,
                    label: "RNAV RWY 16R".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "p2".to_string(),
                    airport_id: "KSFO".to_string(),
                    package_id: "SW_TPP".to_string(),
                    asset_path: "plates/SFO/IAP-CA-RNAV RWY 28R.png".to_string(),
                    thumbnail_path: None,
                    label: "RNAV RWY 28R".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "p3".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/IAP-OR-RNAV RWY 35.png".to_string(),
                    thumbnail_path: None,
                    label: "RNAV RWY 35".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
            ],
            csups: vec![],
        };
        let plan = FlightPlan {
            id: "1".to_string(),
            name: "plan".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSFO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KUAO".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let state = derive_chart_page_state(
            &resource_index,
            &plan,
            &["KUAO".to_string(), "KSFO".to_string(), "KPAE".to_string()],
            Some("KUAO"),
            None,
        );

        assert_eq!(
            state
                .airports
                .iter()
                .map(|airport| airport.id.as_str())
                .collect::<Vec<_>>(),
            vec!["KUAO", "KPAE", "KSFO"]
        );
        assert_eq!(state.selected_airport_id, "KUAO");
    }

    #[test]
    fn orders_plate_folder_by_core_policy() {
        let resource_index = ResourceIndexChartPageInput {
            airport_resources: vec![ResourceAirportResources {
                airport_id: "KUAO".to_string(),
                plate_ids: vec![
                    "hot".to_string(),
                    "min".to_string(),
                    "apd".to_string(),
                    "star".to_string(),
                    "dp".to_string(),
                    "iap".to_string(),
                ],
                csup_ids: vec!["csup".to_string()],
            }],
            plates: vec![
                ResourcePlate {
                    id: "hot".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/HOT_OR_HOT SPOT-0.png".to_string(),
                    thumbnail_path: None,
                    label: "Hot Spot".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "min".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/MIN-TAKEOFF MINIMUMS.png".to_string(),
                    thumbnail_path: None,
                    label: "Takeoff Minimums".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "apd".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/APD-OR-AIRPORT DIAGRAM.png".to_string(),
                    thumbnail_path: None,
                    label: "Airport Diagram".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "star".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/STAR-OR-BUXOM ARRIVAL.png".to_string(),
                    thumbnail_path: None,
                    label: "BUXOM Arrival".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "dp".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/DP-OR-WESLA TWO.png".to_string(),
                    thumbnail_path: None,
                    label: "WESLA TWO".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
                ResourcePlate {
                    id: "iap".to_string(),
                    airport_id: "KUAO".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/UAO/IAP-OR-RNAV (GPS) RWY 35.png".to_string(),
                    thumbnail_path: None,
                    label: "RNAV (GPS) RWY 35".to_string(),
                    asset_kind: "plate".to_string(),
                    georef: None,
                },
            ],
            csups: vec![ResourceCsup {
                id: "csup".to_string(),
                airport_id: "KUAO".to_string(),
                package_id: "NW_CSUP".to_string(),
                asset_path: "csup/UAO/page1.png".to_string(),
                thumbnail_path: None,
                label: "Chart Supplement".to_string(),
                asset_kind: "csup".to_string(),
            }],
        };
        let plan = FlightPlan {
            id: "1".to_string(),
            name: "plan".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KUAO".to_string())),
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let page = derive_chart_page(&resource_index, &plan);

        assert_eq!(
            page.airports[0]
                .charts
                .iter()
                .map(|chart| chart.folder_category.as_str())
                .collect::<Vec<_>>(),
            vec![
                "approach",
                "departure",
                "star",
                "airport-diagram",
                "csup",
                "takeoff-mins",
                "hotspot"
            ]
        );
    }
}
