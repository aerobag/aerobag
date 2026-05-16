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
pub struct PlateAirportRecord {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub airport_type: Option<String>,
    #[serde(default)]
    pub package_ids: Vec<String>,
    pub chart_ids: Vec<String>,
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
    pub thumbnail_source_path: Option<String>,
    pub thumbnail_path: Option<String>,
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

pub fn derive_chart_page_state_from_airports(
    airports: Vec<DerivedChartAirport>,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    let recent_airport_ids = merge_recent_airport_ids(&airports, stored_recent_airport_ids);
    let selected_airport_id =
        resolve_airport_id(&airports, candidate_airport_id, &recent_airport_ids);
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
        if plate_target_kind(candidate_chart_id, airport_id) == Some("csup") {
            return airport
                .and_then(|airport| {
                    airport
                        .charts
                        .iter()
                        .find(|chart| chart.kind == "csup" || chart.folder_category == "csup")
                        .map(|chart| chart.id.clone())
                })
                .unwrap_or_default();
        }
        if plate_target_kind(candidate_chart_id, airport_id) == Some("folder") {
            return airport
                .and_then(|airport| airport.charts.first().map(|chart| chart.id.clone()))
                .unwrap_or_default();
        }
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

fn plate_target_kind(candidate_chart_id: &str, airport_id: &str) -> Option<&'static str> {
    let mut parts = candidate_chart_id.split(':');
    let page = parts.next()?;
    let candidate_airport_id = parts.next()?;
    let target = parts.next()?;
    if parts.next().is_some()
        || page != "Plate"
        || !candidate_airport_id.eq_ignore_ascii_case(airport_id)
    {
        return None;
    }
    if target.eq_ignore_ascii_case("CSup") {
        Some("csup")
    } else if target.eq_ignore_ascii_case("Folder") {
        Some("folder")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(id: &str, kind: &str, folder_category: &str) -> DerivedChartAsset {
        DerivedChartAsset {
            id: id.to_string(),
            airport_id: "KXYZ".to_string(),
            package_id: "pkg".to_string(),
            label: id.to_string(),
            kind: kind.to_string(),
            folder_category: folder_category.to_string(),
            source_asset_path: String::new(),
            asset_path: String::new(),
            thumbnail_source_path: None,
            thumbnail_path: None,
            georef: None,
        }
    }

    fn airport() -> DerivedChartAirport {
        DerivedChartAirport {
            id: "KXYZ".to_string(),
            label: "KXYZ".to_string(),
            charts: vec![
                chart("plate:KXYZ:diagram.png", "plate", "airport"),
                chart("csup:KXYZ:csup.pdf", "csup", "csup"),
            ],
        }
    }

    #[test]
    fn plate_folder_target_selects_first_chart() {
        let state = derive_chart_page_state_from_airports(
            vec![airport()],
            &[],
            Some("KXYZ"),
            Some("Plate:KXYZ:Folder"),
        );

        assert_eq!(state.selected_airport_id, "KXYZ");
        assert_eq!(state.selected_chart_id, "plate:KXYZ:diagram.png");
    }

    #[test]
    fn plate_csup_target_selects_chart_supplement() {
        let state = derive_chart_page_state_from_airports(
            vec![airport()],
            &[],
            Some("KXYZ"),
            Some("Plate:KXYZ:CSup"),
        );

        assert_eq!(state.selected_airport_id, "KXYZ");
        assert_eq!(state.selected_chart_id, "csup:KXYZ:csup.pdf");
    }
}
