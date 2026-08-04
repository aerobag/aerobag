// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::data_status::{
    procedure_geometry_warning_presentation, ProcedureGeometryWarningContext, UiDataStatusBox,
    UiDataStatusState, UiStatusSeverity,
};
use crate::planning::{FlightPlan, RouteComponent};

pub const FAA_CHART_USERS_GUIDE_LABEL: &str = "🔗 Chart User's Guide";
pub const FAA_CHART_USERS_GUIDE_URL: &str =
    "https://aeronav.faa.gov/user_guide/cug-complete_20260709.pdf";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartPage {
    pub airports: Vec<DerivedChartAirport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartPageState {
    pub airports: Vec<DerivedChartAirport>,
    #[serde(default)]
    pub reference_families: Vec<DerivedChartReferenceFamily>,
    pub airport_menu_entries: Vec<DerivedChartAirportMenuEntry>,
    pub recent_airport_ids: Vec<String>,
    pub selected_airport_id: String,
    #[serde(default)]
    pub selected_reference_family_id: Option<String>,
    pub selected_chart_id: String,
    #[serde(default)]
    pub suggested_chart_ids: Vec<String>,
    pub procedure_geometry_status: UiDataStatusState,
}

pub type DerivedChartCatalog = DerivedChartPage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartAirport {
    pub id: String,
    pub label: String,
    pub charts: Vec<DerivedChartAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedChartReferenceFamily {
    pub id: String,
    pub label: String,
    pub charts: Vec<DerivedChartAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DerivedChartAirportMenuEntry {
    Separator {
        label: String,
    },
    Airport {
        airport: DerivedChartAirport,
    },
    Reference {
        reference: DerivedChartReferenceFamily,
    },
    ExternalLink {
        label: String,
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartReferenceFamilySummary {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartReferenceFamilyRecord {
    pub id: String,
    pub label: String,
    pub chart_ids: Vec<String>,
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
    #[serde(default)]
    pub airport_id: Option<String>,
    #[serde(default)]
    pub collection_id: String,
    pub label: String,
    pub kind: String,
    pub folder_category: String,
    pub has_thumbnail: bool,
    #[serde(default)]
    pub procedure_geometry_warning_count: usize,
    #[serde(default, skip_serializing)]
    pub procedure_geometry_warnings: Vec<PlateProcedureGeometryWarning>,
    #[serde(default)]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartAssetRecord {
    pub id: String,
    #[serde(default)]
    pub airport_id: Option<String>,
    #[serde(default)]
    pub collection_id: String,
    pub package_id: String,
    #[serde(default)]
    pub package_ids: Vec<String>,
    pub label: String,
    pub kind: String,
    pub folder_category: String,
    pub asset_path: String,
    pub thumbnail_path: Option<String>,
    #[serde(default)]
    pub procedure_geometry_warning_count: usize,
    #[serde(default)]
    pub procedure_geometry_warnings: Vec<PlateProcedureGeometryWarning>,
    #[serde(default)]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateProcedureGeometryWarning {
    pub airport_id: String,
    pub procedure_id: String,
    #[serde(default)]
    pub runway_transition: Option<String>,
    #[serde(default)]
    pub enroute_transition: Option<String>,
    #[serde(default)]
    pub messages: Vec<String>,
}

impl From<ChartAssetRecord> for DerivedChartAsset {
    fn from(record: ChartAssetRecord) -> Self {
        Self {
            id: record.id,
            airport_id: record.airport_id,
            collection_id: record.collection_id,
            label: record.label,
            kind: record.kind,
            folder_category: record.folder_category,
            has_thumbnail: record.thumbnail_path.is_some(),
            procedure_geometry_warning_count: record.procedure_geometry_warning_count,
            procedure_geometry_warnings: record.procedure_geometry_warnings,
            georef: record.georef,
        }
    }
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
    plan: &FlightPlan,
    airports: Vec<DerivedChartAirport>,
    stored_recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
    selected_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> DerivedChartPageState {
    derive_chart_page_state_from_collections(
        plan,
        airports,
        Vec::new(),
        stored_recent_airport_ids,
        plate_target_airport_id,
        selected_airport_id,
        None,
        candidate_chart_id,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn derive_chart_page_state_from_collections(
    plan: &FlightPlan,
    airports: Vec<DerivedChartAirport>,
    reference_families: Vec<DerivedChartReferenceFamily>,
    stored_recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
    selected_airport_id: Option<&str>,
    selected_reference_family_id: Option<&str>,
    candidate_chart_id: Option<&str>,
    suggested_chart_ids: &[String],
) -> DerivedChartPageState {
    let recent_airport_ids = merge_recent_airport_ids(&airports, stored_recent_airport_ids);
    let selected_airport_id = resolve_airport_id(
        &airports,
        selected_airport_id.or(plate_target_airport_id),
        &recent_airport_ids,
    );
    let selected_reference_family_id = selected_reference_family_id
        .filter(|family_id| {
            reference_families
                .iter()
                .any(|family| family.id == *family_id)
        })
        .map(str::to_string);
    let selected_chart_id = selected_reference_family_id
        .as_deref()
        .and_then(|family_id| {
            reference_families
                .iter()
                .find(|family| family.id == family_id)
                .and_then(|family| resolve_chart_in_assets(&family.charts, candidate_chart_id))
        })
        .unwrap_or_else(|| resolve_chart_id(&airports, &selected_airport_id, candidate_chart_id));
    let selected_chart = selected_reference_family_id
        .as_deref()
        .and_then(|family_id| {
            reference_families
                .iter()
                .find(|family| family.id == family_id)
        })
        .and_then(|family| {
            family
                .charts
                .iter()
                .find(|chart| chart.id == selected_chart_id)
        })
        .or_else(|| {
            airports
                .iter()
                .find(|airport| airport.id == selected_airport_id)
                .and_then(|airport| {
                    airport
                        .charts
                        .iter()
                        .find(|chart| chart.id == selected_chart_id)
                })
        });
    let procedure_geometry_status = procedure_geometry_status(selected_chart);
    let mut airport_menu_entries = derive_airport_menu_entries(
        &airports,
        plan,
        stored_recent_airport_ids,
        plate_target_airport_id,
    );
    if !reference_families.is_empty() {
        airport_menu_entries.push(DerivedChartAirportMenuEntry::Separator {
            label: "Chart references".to_string(),
        });
        airport_menu_entries.extend(
            reference_families
                .iter()
                .cloned()
                .map(|reference| DerivedChartAirportMenuEntry::Reference { reference }),
        );
    }
    airport_menu_entries.push(DerivedChartAirportMenuEntry::ExternalLink {
        label: FAA_CHART_USERS_GUIDE_LABEL.to_string(),
        url: FAA_CHART_USERS_GUIDE_URL.to_string(),
    });
    DerivedChartPageState {
        airports,
        reference_families,
        airport_menu_entries,
        recent_airport_ids,
        selected_airport_id,
        selected_reference_family_id,
        selected_chart_id,
        suggested_chart_ids: suggested_chart_ids.to_vec(),
        procedure_geometry_status,
    }
}

fn procedure_geometry_status(chart: Option<&DerivedChartAsset>) -> UiDataStatusState {
    let Some(chart) = chart else {
        return empty_procedure_geometry_status();
    };
    if chart.procedure_geometry_warnings.is_empty() {
        if chart.procedure_geometry_warning_count == 0 {
            return empty_procedure_geometry_status();
        }
        return UiDataStatusState {
            boxes: vec![UiDataStatusBox {
                id: "plate:procedure_geometry:aggregate".to_string(),
                label: "PROCEDURE GEOMETRY".to_string(),
                value: None,
                severity: UiStatusSeverity::Caution,
                drives_caution: true,
                detail: format!(
                    "This publication reports {} procedure geometry warning{} for this plate but does not include the underlying details. Verify the geometry against the published plate.",
                    chart.procedure_geometry_warning_count,
                    if chart.procedure_geometry_warning_count == 1 { "" } else { "s" },
                ),
                actions: Vec::new(),
                hushed: false,
            }],
            launcher_count: Some(chart.procedure_geometry_warning_count.to_string()),
            launcher_severity: UiStatusSeverity::Caution,
        };
    }
    let boxes = chart
        .procedure_geometry_warnings
        .iter()
        .enumerate()
        .map(|(index, warning)| {
            let presentation = procedure_geometry_warning_presentation(
                &warning.airport_id,
                &chart.label,
                ProcedureGeometryWarningContext::PublishedPlate {
                    transition: warning
                        .enroute_transition
                        .as_deref()
                        .or(warning.runway_transition.as_deref()),
                },
                &warning.messages,
            );
            UiDataStatusBox {
                id: format!("plate:procedure_geometry:{index}"),
                label: "PROC".to_string(),
                value: Some(presentation.value),
                severity: UiStatusSeverity::Caution,
                drives_caution: true,
                detail: presentation.detail,
                actions: Vec::new(),
                hushed: false,
            }
        })
        .collect::<Vec<_>>();
    let warning_count = boxes.len();
    UiDataStatusState {
        boxes,
        launcher_count: Some(warning_count.to_string()),
        launcher_severity: UiStatusSeverity::Caution,
    }
}

fn empty_procedure_geometry_status() -> UiDataStatusState {
    UiDataStatusState {
        boxes: Vec::new(),
        launcher_count: None,
        launcher_severity: UiStatusSeverity::Ok,
    }
}

fn resolve_chart_in_assets(
    charts: &[DerivedChartAsset],
    candidate_chart_id: Option<&str>,
) -> Option<String> {
    candidate_chart_id
        .filter(|chart_id| charts.iter().any(|chart| chart.id == *chart_id))
        .map(str::to_string)
        .or_else(|| charts.first().map(|chart| chart.id.clone()))
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

pub fn route_airport_ids_from_plan(plan: &FlightPlan) -> Vec<String> {
    let mut airport_ids = Vec::new();
    if let Some(departure) = &plan.departure {
        airport_ids.push(departure.0.clone());
    }
    for component in &plan.route_components {
        append_route_component_airports(&mut airport_ids, component);
    }
    if let Some(destination) = &plan.destination {
        airport_ids.push(destination.0.clone());
    }
    airport_ids
}

pub fn chart_page_airport_ids_from_plan(plan: &FlightPlan) -> Vec<String> {
    let mut airport_ids = route_airport_ids_from_plan(plan);
    if let Some(alternate) = &plan.alternate {
        airport_ids.push(alternate.0.clone());
    }
    airport_ids
}

fn append_route_component_airports(airport_ids: &mut Vec<String>, component: &RouteComponent) {
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

fn derive_airport_menu_entries(
    airports: &[DerivedChartAirport],
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
) -> Vec<DerivedChartAirportMenuEntry> {
    let mut entries = Vec::new();
    let mut emitted_airport_ids = Vec::new();
    if let Some(airport_id) = normalize_airport_id(plate_target_airport_id) {
        append_airport_menu_section(
            &mut entries,
            &mut emitted_airport_ids,
            "▶ Selected",
            &[airport_id],
            airports,
        );
    }

    let route_airport_ids = unique_airport_ids(route_airport_ids_from_plan(plan));
    let departure_airport_ids = route_airport_ids
        .first()
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let arrival_airport_ids = route_airport_ids
        .last()
        .filter(|airport_id| departure_airport_ids.first() != Some(*airport_id))
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    append_airport_menu_section(
        &mut entries,
        &mut emitted_airport_ids,
        "🛫 Departure",
        &departure_airport_ids,
        airports,
    );
    append_airport_menu_section(
        &mut entries,
        &mut emitted_airport_ids,
        "🛬 Arrival",
        &arrival_airport_ids,
        airports,
    );

    let mut plan_airport_ids = if route_airport_ids.len() > 2 {
        route_airport_ids[1..route_airport_ids.len() - 1].to_vec()
    } else {
        Vec::new()
    };
    if let Some(alternate) = &plan.alternate {
        plan_airport_ids.push(alternate.0.clone());
    }
    append_airport_menu_section(
        &mut entries,
        &mut emitted_airport_ids,
        "☷ Plan",
        &unique_airport_ids(plan_airport_ids),
        airports,
    );

    append_airport_menu_section(
        &mut entries,
        &mut emitted_airport_ids,
        "◷ Recent",
        &stored_recent_airport_ids
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>(),
        airports,
    );
    entries
}

fn append_airport_menu_section(
    entries: &mut Vec<DerivedChartAirportMenuEntry>,
    emitted_airport_ids: &mut Vec<String>,
    label: &str,
    airport_ids: &[String],
    airports: &[DerivedChartAirport],
) {
    let mut section_airports = Vec::new();
    for airport_id in airport_ids {
        let Some(airport_id) = normalize_airport_id(Some(airport_id.as_str())) else {
            continue;
        };
        if emitted_airport_ids
            .iter()
            .any(|existing| existing == &airport_id)
        {
            continue;
        }
        let Some(airport) = airports.iter().find(|airport| airport.id == airport_id) else {
            continue;
        };
        emitted_airport_ids.push(airport_id);
        section_airports.push(airport.clone());
    }
    if section_airports.is_empty() {
        return;
    }
    entries.push(DerivedChartAirportMenuEntry::Separator {
        label: label.to_string(),
    });
    entries.extend(
        section_airports
            .into_iter()
            .map(|airport| DerivedChartAirportMenuEntry::Airport { airport }),
    );
}

fn normalize_airport_id(airport_id: Option<&str>) -> Option<String> {
    airport_id
        .map(str::trim)
        .filter(|airport_id| !airport_id.is_empty())
        .map(str::to_ascii_uppercase)
}

fn unique_airport_ids(airport_ids: Vec<String>) -> Vec<String> {
    let mut unique_airport_ids = Vec::new();
    for airport_id in airport_ids {
        let Some(airport_id) = normalize_airport_id(Some(airport_id.as_str())) else {
            continue;
        };
        if !unique_airport_ids
            .iter()
            .any(|existing| existing == &airport_id)
        {
            unique_airport_ids.push(airport_id);
        }
    }
    unique_airport_ids
}

fn resolve_airport_id(
    airports: &[DerivedChartAirport],
    candidate_airport_id: Option<&str>,
    recent_airport_ids: &[String],
) -> String {
    if let Some(candidate_airport_id) = normalize_airport_id(candidate_airport_id) {
        if airports
            .iter()
            .any(|airport| airport.id == candidate_airport_id)
        {
            return candidate_airport_id;
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
    use crate::ids::AirportId;

    use super::*;

    fn chart(airport_id: &str, id: &str, kind: &str, folder_category: &str) -> DerivedChartAsset {
        DerivedChartAsset {
            id: id.to_string(),
            airport_id: Some(airport_id.to_string()),
            collection_id: format!("airport:{airport_id}"),
            label: id.to_string(),
            kind: kind.to_string(),
            folder_category: folder_category.to_string(),
            has_thumbnail: false,
            procedure_geometry_warning_count: 0,
            procedure_geometry_warnings: Vec::new(),
            georef: None,
        }
    }

    #[test]
    fn chart_asset_preserves_procedure_geometry_warnings() {
        let record: ChartAssetRecord = serde_json::from_value(serde_json::json!({
            "id": "plate:KAAA:IAP-AA-VOR RWY 01.png",
            "airport_id": "KAAA",
            "collection_id": "airport:KAAA",
            "package_id": "tpp-aa",
            "label": "VOR 01",
            "kind": "plate",
            "folder_category": "approach",
            "asset_path": "plates/AAA/IAP-AA-VOR RWY 01.png",
            "thumbnail_path": "thumbnails/plates/AAA/IAP-AA-VOR RWY 01.png",
            "procedure_geometry_warning_count": 2,
            "procedure_geometry_warnings": [
                {
                    "airport_id": "KAAA",
                    "procedure_id": "V01",
                    "runway_transition": null,
                    "enroute_transition": "TRANS",
                    "messages": ["first warning", "second warning"]
                }
            ]
        }))
        .unwrap();

        let chart = DerivedChartAsset::from(record);

        assert_eq!(chart.procedure_geometry_warning_count, 2);
        assert_eq!(chart.procedure_geometry_warnings.len(), 1);
        assert_eq!(chart.procedure_geometry_warnings[0].procedure_id, "V01");
        assert_eq!(
            chart.procedure_geometry_warnings[0].messages,
            vec!["first warning".to_string(), "second warning".to_string()],
        );
    }

    #[test]
    fn procedure_geometry_warnings_become_contextual_status_boxes() {
        let mut selected = chart("KOMA", "plate:KOMA:approach.png", "plate", "approach");
        selected.label = "ILS or LOC 32R".to_string();
        selected.procedure_geometry_warning_count = 2;
        selected.procedure_geometry_warnings = vec![
            PlateProcedureGeometryWarning {
                airport_id: "KOMA".to_string(),
                procedure_id: "I32R".to_string(),
                runway_transition: None,
                enroute_transition: Some("FIRST".to_string()),
                messages: vec!["first validation gripe".to_string()],
            },
            PlateProcedureGeometryWarning {
                airport_id: "KOMA".to_string(),
                procedure_id: "I32R".to_string(),
                runway_transition: Some("SECOND".to_string()),
                enroute_transition: None,
                messages: vec!["second validation gripe".to_string()],
            },
        ];

        let status = procedure_geometry_status(Some(&selected));

        assert_eq!(status.launcher_count.as_deref(), Some("2"));
        assert_eq!(status.launcher_severity, UiStatusSeverity::Caution);
        assert_eq!(status.boxes.len(), 2);
        assert_eq!(status.boxes[0].label, "PROC");
        assert_eq!(status.boxes[0].value.as_deref(), Some("ILS or LOC 32R"));
        assert_eq!(
            status.boxes[0].detail,
            "This publication reports a procedure geometry warning for KOMA ILS or LOC 32R from FIRST:\nfirst validation gripe",
        );
        assert_eq!(status.boxes[1].label, "PROC");
        assert_eq!(status.boxes[1].value.as_deref(), Some("ILS or LOC 32R"));
        assert_eq!(
            status.boxes[1].detail,
            "This publication reports a procedure geometry warning for KOMA ILS or LOC 32R from SECOND:\nsecond validation gripe",
        );
    }

    #[test]
    fn count_only_publication_gets_one_unfractioned_explanation() {
        let mut selected = chart("KAAA", "plate:KAAA:approach.png", "plate", "approach");
        selected.procedure_geometry_warning_count = 2;

        let status = procedure_geometry_status(Some(&selected));

        assert_eq!(status.launcher_count.as_deref(), Some("2"));
        assert_eq!(status.boxes.len(), 1);
        assert_eq!(status.boxes[0].value, None);
        assert_eq!(
            status.boxes[0].detail,
            "This publication reports 2 procedure geometry warnings for this plate but does not include the underlying details. Verify the geometry against the published plate."
        );
    }

    fn airport() -> DerivedChartAirport {
        airport_with_id("KXYZ")
    }

    fn airport_with_id(id: &str) -> DerivedChartAirport {
        DerivedChartAirport {
            id: id.to_string(),
            label: id.to_string(),
            charts: vec![
                chart(id, &format!("plate:{id}:diagram.png"), "plate", "airport"),
                chart(id, &format!("csup:{id}:csup.pdf"), "csup", "csup"),
            ],
        }
    }

    #[test]
    fn plate_folder_target_selects_first_chart() {
        let state = derive_chart_page_state_from_airports(
            &FlightPlan::default(),
            vec![airport()],
            &[],
            Some("KXYZ"),
            Some("KXYZ"),
            Some("Plate:KXYZ:Folder"),
        );

        assert_eq!(state.selected_airport_id, "KXYZ");
        assert_eq!(state.selected_chart_id, "plate:KXYZ:diagram.png");
    }

    #[test]
    fn plate_csup_target_selects_chart_supplement() {
        let state = derive_chart_page_state_from_airports(
            &FlightPlan::default(),
            vec![airport()],
            &[],
            Some("KXYZ"),
            Some("KXYZ"),
            Some("Plate:KXYZ:CSup"),
        );

        assert_eq!(state.selected_airport_id, "KXYZ");
        assert_eq!(state.selected_chart_id, "csup:KXYZ:csup.pdf");
    }

    #[test]
    fn airport_menu_entries_are_sectioned_and_recent_is_capped() {
        let plan = FlightPlan {
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KBLI".to_string())),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: crate::NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: crate::NavRef::Airport("KORS".to_string()),
                },
            ],
            ..FlightPlan::default()
        };
        let state = derive_chart_page_state_from_airports(
            &plan,
            vec![
                airport_with_id("KPWT"),
                airport_with_id("KRNT"),
                airport_with_id("KBLI"),
                airport_with_id("KPAE"),
                airport_with_id("KORS"),
                airport_with_id("KPLU"),
                airport_with_id("KSEA"),
                airport_with_id("KBFI"),
                airport_with_id("KTCM"),
                airport_with_id("KOLM"),
                airport_with_id("KTIW"),
            ],
            &[
                "KPLU".to_string(),
                "KSEA".to_string(),
                "KBFI".to_string(),
                "KTCM".to_string(),
                "KOLM".to_string(),
                "KTIW".to_string(),
            ],
            Some("KPWT"),
            Some("KPAE"),
            None,
        );

        let labels = state
            .airport_menu_entries
            .iter()
            .map(|entry| match entry {
                DerivedChartAirportMenuEntry::Separator { label } => format!("--{label}"),
                DerivedChartAirportMenuEntry::Airport { airport } => airport.id.clone(),
                DerivedChartAirportMenuEntry::Reference { reference } => reference.id.clone(),
                DerivedChartAirportMenuEntry::ExternalLink { label, .. } => label.clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "--▶ Selected",
                "KPWT",
                "--🛫 Departure",
                "KRNT",
                "--🛬 Arrival",
                "KBLI",
                "--☷ Plan",
                "KPAE",
                "KORS",
                "--◷ Recent",
                "KPLU",
                "KSEA",
                "KBFI",
                "KTCM",
                "KOLM",
                FAA_CHART_USERS_GUIDE_LABEL,
            ]
        );
    }

    #[test]
    fn chart_reference_target_preserves_airport_recents_and_multiple_suggestions() {
        let reference = DerivedChartReferenceFamily {
            id: "tac".to_string(),
            label: "TAC".to_string(),
            charts: vec![
                DerivedChartAsset {
                    id: "legend".to_string(),
                    airport_id: None,
                    collection_id: "reference:tac".to_string(),
                    label: "TAC Legend".to_string(),
                    kind: "legend".to_string(),
                    folder_category: "legend".to_string(),
                    has_thumbnail: true,
                    procedure_geometry_warning_count: 0,
                    procedure_geometry_warnings: Vec::new(),
                    georef: None,
                },
                DerivedChartAsset {
                    id: "la-inset".to_string(),
                    airport_id: None,
                    collection_id: "reference:tac".to_string(),
                    label: "Los Angeles Insets".to_string(),
                    kind: "inset".to_string(),
                    folder_category: "inset".to_string(),
                    has_thumbnail: true,
                    procedure_geometry_warning_count: 0,
                    procedure_geometry_warnings: Vec::new(),
                    georef: None,
                },
            ],
        };
        let state = derive_chart_page_state_from_collections(
            &FlightPlan::default(),
            vec![airport_with_id("KSEA")],
            vec![reference],
            &["KSEA".to_string()],
            None,
            Some("KSEA"),
            Some("tac"),
            None,
            &["la-inset".to_string(), "other-inset".to_string()],
        );

        assert_eq!(state.selected_reference_family_id.as_deref(), Some("tac"));
        assert_eq!(state.selected_chart_id, "legend");
        assert_eq!(state.recent_airport_ids, vec!["KSEA"]);
        assert_eq!(state.suggested_chart_ids, vec!["la-inset", "other-inset"]);
        assert!(state.airport_menu_entries.iter().any(|entry| matches!(
            entry,
            DerivedChartAirportMenuEntry::Reference { reference } if reference.id == "tac"
        )));
        assert!(matches!(
            state.airport_menu_entries.last(),
            Some(DerivedChartAirportMenuEntry::ExternalLink { label, url })
                if label == FAA_CHART_USERS_GUIDE_LABEL && url == FAA_CHART_USERS_GUIDE_URL
        ));
    }
}
