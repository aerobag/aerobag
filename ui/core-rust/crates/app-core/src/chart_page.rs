use serde::{Deserialize, Serialize};

use crate::planning::FlightPlan;

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
    pub asset_path: String,
    pub asset_url: String,
    pub thumbnail_path: Option<String>,
    pub thumbnail_url: Option<String>,
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
    let mut ordered_airport_ids: Vec<String> = Vec::new();
    for airport_id in airport_ids_from_plan(plan) {
        if !ordered_airport_ids.iter().any(|id| id == &airport_id) {
            ordered_airport_ids.push(airport_id);
        }
    }
    for entry in &resource_index.airport_resources {
        if !ordered_airport_ids.iter().any(|id| id == &entry.airport_id) {
            ordered_airport_ids.push(entry.airport_id.clone());
        }
    }

    let airports = ordered_airport_ids
        .into_iter()
        .filter_map(|airport_id| {
            let airport_resources = resource_index
                .airport_resources
                .iter()
                .find(|entry| entry.airport_id == airport_id)?;
            let mut charts: Vec<DerivedChartAsset> = Vec::new();
            for plate_id in &airport_resources.plate_ids {
                if let Some(plate) = resource_index.plates.iter().find(|record| &record.id == plate_id) {
                    charts.push(chart_asset_for_plate(&airport_id, plate));
                }
            }
            for csup_id in &airport_resources.csup_ids {
                if let Some(csup) = resource_index.csups.iter().find(|record| &record.id == csup_id) {
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

pub fn derive_chart_page_state(
    resource_index: &ResourceIndexChartPageInput,
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    let page = derive_chart_page(resource_index, plan);
    let recent_airport_ids = merge_recent_airport_ids(&page.airports, stored_recent_airport_ids);
    let selected_airport_id = resolve_airport_id(&page.airports, candidate_airport_id, &recent_airport_ids);
    let selected_chart_id = resolve_chart_id(&page.airports, &selected_airport_id, candidate_chart_id);
    let airports = order_airports_by_recency(&page.airports, &recent_airport_ids);
    DerivedChartPageState {
        airports,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    }
}

fn airport_ids_from_plan(plan: &FlightPlan) -> Vec<String> {
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
    for leg in &plan.legs {
        if let Some(code) = leg.from.airport_code() {
            airport_ids.push(code.to_string());
        }
        if let Some(code) = leg.to.airport_code() {
            airport_ids.push(code.to_string());
        }
    }
    airport_ids
}

fn merge_recent_airport_ids(airports: &[DerivedChartAirport], stored_ids: &[String]) -> Vec<String> {
    let valid_ids: Vec<String> = airports.iter().map(|airport| airport.id.clone()).collect();
    let mut ordered_ids: Vec<String> = Vec::new();
    for id in stored_ids {
        if valid_ids.iter().any(|valid_id| valid_id == id) && !ordered_ids.iter().any(|existing| existing == id) {
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

fn order_airports_by_recency(airports: &[DerivedChartAirport], recent_airport_ids: &[String]) -> Vec<DerivedChartAirport> {
    recent_airport_ids
        .iter()
        .filter_map(|airport_id| airports.iter().find(|airport| &airport.id == airport_id).cloned())
        .collect()
}

fn resolve_airport_id(
    airports: &[DerivedChartAirport],
    candidate_airport_id: Option<&str>,
    recent_airport_ids: &[String],
) -> String {
    if let Some(candidate_airport_id) = candidate_airport_id {
        if airports.iter().any(|airport| airport.id == candidate_airport_id) {
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
            .map(|airport| airport.charts.iter().any(|chart| chart.id == candidate_chart_id))
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
    let filename = plate.asset_path.rsplit('/').next().unwrap_or(&plate.asset_path);
    let thumbnail_filename = plate
        .thumbnail_path
        .as_ref()
        .and_then(|path| path.rsplit('/').next().map(|value| value.to_string()));
    DerivedChartAsset {
        id: format!("plate:{airport_id}:{filename}"),
        airport_id: airport_id.to_string(),
        package_id: plate.package_id.clone(),
        label: plate.label.clone(),
        kind: "plate".to_string(),
        folder_category: folder_category("plate", &plate.label),
        asset_path: format!("chart-assets/{airport_id}/{filename}"),
        asset_url: format!("/chart-assets/{airport_id}/{filename}"),
        thumbnail_path: thumbnail_filename
            .clone()
            .map(|name| format!("chart-thumbnails/{airport_id}/{name}")),
        thumbnail_url: thumbnail_filename
            .map(|name| format!("/chart-thumbnails/{airport_id}/{name}")),
    }
}

fn chart_asset_for_csup(airport_id: &str, csup: &ResourceCsup) -> DerivedChartAsset {
    let filename = csup.asset_path.rsplit('/').next().unwrap_or(&csup.asset_path);
    let thumbnail_filename = csup
        .thumbnail_path
        .as_ref()
        .and_then(|path| path.rsplit('/').next().map(|value| value.to_string()));
    DerivedChartAsset {
        id: format!("csup:{airport_id}:{filename}"),
        airport_id: airport_id.to_string(),
        package_id: csup.package_id.clone(),
        label: "CSup".to_string(),
        kind: "csup".to_string(),
        folder_category: "csup".to_string(),
        asset_path: format!("chart-assets/{airport_id}/{filename}"),
        asset_url: format!("/chart-assets/{airport_id}/{filename}"),
        thumbnail_path: thumbnail_filename
            .clone()
            .map(|name| format!("chart-thumbnails/{airport_id}/{name}")),
        thumbnail_url: thumbnail_filename
            .map(|name| format!("/chart-thumbnails/{airport_id}/{name}")),
    }
}

fn folder_category(kind: &str, label: &str) -> String {
    if kind == "csup" {
        return "csup".to_string();
    }
    let normalized = label.to_uppercase();
    if normalized.contains("AIRPORT DIAGRAM") {
        "airport-diagram".to_string()
    } else if normalized.starts_with("MIN-")
        || normalized.contains("TAKEOFF MINIMUMS")
        || normalized.contains("ALTERNATE MINIMUMS")
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

fn folder_category_rank(category: &str) -> usize {
    match category {
        "airport-diagram" => 0,
        "csup" => 1,
        "takeoff-mins" => 2,
        "approach" => 3,
        "departure" => 4,
        "star" => 5,
        _ => 6,
    }
}

#[cfg(test)]
mod tests {
    use crate::{AirportId, NavRef, PlanLeg};

    use super::*;

    #[test]
    fn derives_chart_page_in_plan_then_resource_order() {
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
                label: "APD-WA-AIRPORT DIAGRAM".to_string(),
                asset_kind: "plate".to_string(),
            }],
            csups: vec![ResourceCsup {
                id: "c1".to_string(),
                airport_id: "KPAE".to_string(),
                package_id: "NW_CSUP".to_string(),
                asset_path: "csup/KPAE/page1.png".to_string(),
                thumbnail_path: None,
                label: "CSup".to_string(),
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
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KSEA".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let page = derive_chart_page(&resource_index, &plan);

        assert_eq!(page.airports.iter().map(|airport| airport.id.as_str()).collect::<Vec<_>>(), vec!["KSEA", "KPAE"]);
        assert_eq!(page.airports[0].charts[0].folder_category, "airport-diagram");
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
                    label: "APD-WA-AIRPORT DIAGRAM".to_string(),
                    asset_kind: "plate".to_string(),
                },
                ResourcePlate {
                    id: "p2".to_string(),
                    airport_id: "KPAE".to_string(),
                    package_id: "NW_TPP".to_string(),
                    asset_path: "plates/PAE/DP-WA-RNDR TWO.png".to_string(),
                    thumbnail_path: None,
                    label: "DP-WA-RNDR TWO".to_string(),
                    asset_kind: "plate".to_string(),
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

        assert_eq!(state.recent_airport_ids, vec!["KPAE".to_string(), "KSEA".to_string()]);
        assert_eq!(state.selected_airport_id, "KPAE");
        assert_eq!(state.selected_chart_id, "plate:KPAE:DP-WA-RNDR TWO.png");
        assert_eq!(state.airports[0].id, "KPAE");
    }
}
