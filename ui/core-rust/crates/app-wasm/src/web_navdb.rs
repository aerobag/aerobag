use std::collections::HashMap;

use app_core::{
    classify_procedure_identifier, describe_plate_procedure_load_options,
    describe_procedure_options_from_rows, describe_show_plate_for_procedure,
    interpret_path_termination, parse_airport_magnetic_variation, parse_cifp_altitude_ft,
    parse_cifp_tenths_value, point_vector_record_to_symbol_feature, prepare_airway_presentation,
    select_preferred_cifp_tpp_match, AirwayAutoSelection, AirwayBranch, AirwayEntryCandidate,
    AirwayExitCandidate, AirwayFixPoint, AirwaySegment, AirwaySuggestion, AppError, AppErrorKind,
    AppResult, CifpTppMatchRow, FlightPlan, FlightPlanRouteSegment, FlightPlanRouteSegmentStatus,
    LatLon, NavRef, NavSymbolFeature, PlateProcedureLoadCandidateInput, PointVectorRecord,
    ProcedureDistinctRow, ProcedureKind, ProcedureLegMaterializationRecord, ProcedureSummary,
    ProcedureVariantKey, ResolvedLeg, ResolvedLegSource, RouteComponent,
    WaypointIdentifierSuggestion,
};
use serde::Serialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = globalThis, js_name = __aerobagNavDbQueryObjects, catch)]
    fn navdb_query_objects(sql: &str, bind_json: &str) -> Result<String, JsValue>;
}

pub fn suggest_airways_near_json(anchor_json: &str, limit: usize) -> Result<String, String> {
    let anchor: NavRef = serde_json::from_str(anchor_json).map_err(|err| err.to_string())?;
    let suggestions = suggest_airways_near(&anchor, limit).map_err(|err| err.to_string())?;
    serde_json::to_string(&suggestions).map_err(|err| err.to_string())
}

pub fn prepare_airway_presentation_for_anchors_json(
    airway_name: &str,
    origin_anchor_json: &str,
    destination_anchor_json: &str,
) -> Result<String, String> {
    let origin_anchor: NavRef =
        serde_json::from_str(origin_anchor_json).map_err(|err| err.to_string())?;
    let destination_anchor: Option<NavRef> =
        serde_json::from_str(destination_anchor_json).map_err(|err| err.to_string())?;
    let branches = load_airway_branches(airway_name).map_err(|err| err.to_string())?;
    let origin_position =
        resolve_nav_ref_position(&origin_anchor, None).map_err(|err| err.to_string())?;
    let destination_position = destination_anchor
        .as_ref()
        .map(|nav_ref| resolve_nav_ref_position(nav_ref, None))
        .transpose()
        .map_err(|err| err.to_string())?;
    let presentation =
        prepare_airway_presentation(airway_name, branches, origin_position, destination_position)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&presentation).map_err(|err| err.to_string())
}

pub fn materialize_airway_selection_json(
    start_component_index: usize,
    entry_json: &str,
    exit_json: &str,
    origin_anchor_json: &str,
    destination_anchor_json: &str,
) -> Result<String, String> {
    let entry: AirwayEntryCandidate =
        serde_json::from_str(entry_json).map_err(|err| err.to_string())?;
    let exit: AirwayExitCandidate =
        serde_json::from_str(exit_json).map_err(|err| err.to_string())?;
    let origin_anchor: NavRef =
        serde_json::from_str(origin_anchor_json).map_err(|err| err.to_string())?;
    let destination_anchor: Option<NavRef> =
        serde_json::from_str(destination_anchor_json).map_err(|err| err.to_string())?;
    let (airway, resolved_legs) =
        materialize_airway_selection(start_component_index, &entry, &exit)
            .map_err(|err| err.to_string())?;
    let origin_pos =
        resolve_nav_ref_position(&origin_anchor, None).map_err(|err| err.to_string())?;
    let entry_pos =
        resolve_nav_ref_position(&entry.nav_ref, None).map_err(|err| err.to_string())?;
    let exit_pos = resolve_nav_ref_position(&exit.nav_ref, None).map_err(|err| err.to_string())?;
    let origin_distance_nm = distance_nm(origin_pos, entry_pos);
    let destination_distance_nm = destination_anchor
        .as_ref()
        .map(|nav_ref| {
            resolve_nav_ref_position(nav_ref, None).map(|position| distance_nm(position, exit_pos))
        })
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or(0.0);
    let response = MaterializedAirwayResponse {
        selection: AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry,
            exit,
            origin_distance_nm,
            destination_distance_nm,
            total_anchor_distance_nm: origin_distance_nm + destination_distance_nm,
        },
        airway,
        resolved_legs,
    };
    serde_json::to_string(&response).map_err(|err| err.to_string())
}

pub fn resolve_waypoint_identifier_json(identifier: &str) -> Result<String, String> {
    let nav_ref = classify_identifier(identifier).map_err(|err| err.to_string())?;
    serde_json::to_string(&nav_ref).map_err(|err| err.to_string())
}

pub fn suggest_waypoint_identifiers_json(
    plan_json: &str,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
) -> Result<String, String> {
    let plan: FlightPlan = serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let suggestions = suggest_waypoint_identifiers(&plan, component_index, before, prefix, limit)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&suggestions).map_err(|err| err.to_string())
}

pub fn resolve_nav_symbol_feature_json(nav_ref_json: &str) -> Result<String, String> {
    let nav_ref: NavRef = serde_json::from_str(nav_ref_json).map_err(|err| err.to_string())?;
    let feature = resolve_nav_symbol_feature(&nav_ref).map_err(|err| err.to_string())?;
    serde_json::to_string(&feature).map_err(|err| err.to_string())
}

pub fn project_flight_plan_route_json(plan_json: &str) -> Result<String, String> {
    let plan: FlightPlan = serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    let ui_state = app_core::project_ui_state(&plan);
    let route = plan
        .resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| {
            let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
                (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
            });
            let from = resolve_nav_ref_position(&leg.from, procedure_airport_id)?;
            let to = resolve_nav_ref_position(&leg.to, procedure_airport_id)?;
            Ok(FlightPlanRouteSegment {
                id: leg.id.clone(),
                from,
                to,
                distance_nm: app_core::flight_leg_distance_nm(from, to),
                course_deg: app_core::flight_leg_course_deg(from, to),
                status: route_status_for_leg(&ui_state, leg_index),
            })
        })
        .collect::<AppResult<Vec<_>>>()
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&route).map_err(|err| err.to_string())
}

pub fn list_procedures_json(airport_id: &str, kind_json: &str) -> Result<String, String> {
    let kind: ProcedureKind = serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let procedures = if kind == ProcedureKind::Approach {
        let rows = load_cifp_tpp_matches_for_airport(airport_id).map_err(|err| err.to_string())?;
        app_core::list_approach_procedures_from_match_rows(airport_id, rows)
            .map_err(|err| err.to_string())?
    } else {
        list_procedures(airport_id, kind).map_err(|err| err.to_string())?
    };
    serde_json::to_string(&procedures).map_err(|err| err.to_string())
}

pub fn describe_procedure_options_json(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
) -> Result<String, String> {
    let kind: ProcedureKind = serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let rows =
        load_procedure_distinct_rows(airport_id, procedure_id).map_err(|err| err.to_string())?;
    let options = describe_procedure_options_from_rows(airport_id, procedure_id, kind, rows)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&options).map_err(|err| err.to_string())
}

pub fn materialize_procedure_json(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
    component_index: usize,
) -> Result<String, String> {
    let kind: ProcedureKind = serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let runway_transition: Option<String> =
        serde_json::from_str(runway_transition_json).map_err(|err| err.to_string())?;
    let enroute_transition: Option<String> =
        serde_json::from_str(enroute_transition_json).map_err(|err| err.to_string())?;
    let rows =
        load_procedure_distinct_rows(airport_id, procedure_id).map_err(|err| err.to_string())?;
    let legs = load_procedure_materialization_records(airport_id, procedure_id)
        .map_err(|err| err.to_string())?;
    let built = app_core::materialize_procedure_from_records(
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        component_index,
        rows,
        legs,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&built).map_err(|err| err.to_string())
}

pub fn find_procedure_plate_match_json(airport_id: &str, cifp_id: &str) -> Result<String, String> {
    let rows =
        load_cifp_tpp_matches_for_procedure(airport_id, cifp_id).map_err(|err| err.to_string())?;
    let match_row = describe_show_plate_for_procedure(rows);
    serde_json::to_string(&match_row).map_err(|err| err.to_string())
}

pub fn describe_plate_procedure_loads_json(
    plan_json: &str,
    plate_id: &str,
) -> Result<String, String> {
    let plan: FlightPlan = serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let rows = load_cifp_tpp_matches_for_plate(plate_id).map_err(|err| err.to_string())?;
    let mut grouped = HashMap::<String, Vec<CifpTppMatchRow>>::new();
    for row in rows {
        grouped
            .entry(format!("{}:{}", row.airport_id, row.cifp_id))
            .or_default()
            .push(row);
    }

    let mut candidates = Vec::<PlateProcedureLoadCandidateInput>::new();
    for match_rows in grouped.into_values() {
        let Some(preferred) = select_preferred_cifp_tpp_match(match_rows.clone()) else {
            continue;
        };
        let distinct_rows = load_procedure_distinct_rows(&preferred.airport_id, &preferred.cifp_id)
            .map_err(|err| err.to_string())?;
        if distinct_rows.is_empty() {
            continue;
        }
        candidates.push(PlateProcedureLoadCandidateInput {
            airport_id: preferred.airport_id,
            cifp_id: preferred.cifp_id,
            match_rows,
            distinct_rows,
        });
    }

    let options =
        describe_plate_procedure_load_options(&plan, candidates).map_err(|err| err.to_string())?;
    serde_json::to_string(&options).map_err(|err| err.to_string())
}

#[derive(Serialize)]
struct MaterializedAirwayResponse {
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    #[serde(rename = "resolvedLegs")]
    resolved_legs: Vec<ResolvedLeg>,
}

fn query(sql: &str, bind: Value) -> AppResult<Vec<Value>> {
    let rows_json = navdb_query_objects(sql, &bind.to_string()).map_err(|err| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "web nav database query failed: {:?}",
            err.as_string()
                .unwrap_or_else(|| "unknown JS error".to_string())
        ),
    })?;
    serde_json::from_str(&rows_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("web nav database returned invalid JSON: {err}"),
    })
}

fn table_exists(table_name: &str) -> AppResult<bool> {
    Ok(!query(
        "SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        json!([table_name]),
    )?
    .is_empty())
}

fn load_airway_branches(airway_name: &str) -> AppResult<Vec<AirwayBranch>> {
    if table_exists("airways_branch")? {
        let rows = query(
            "
            SELECT trim(name) AS name,
                   trim(branch_key) AS branch_key,
                   CAST(sequence_number AS INTEGER) AS sequence_number,
                   Latitude AS Latitude,
                   Longitude AS Longitude
            FROM airways_branch
            WHERE trim(name) = trim(?1)
            ORDER BY trim(branch_key), CAST(sequence_number AS INTEGER)
            ",
            json!([airway_name]),
        )?;
        let mut by_branch = Vec::<(String, Vec<AirwayFixPoint>)>::new();
        for row in rows {
            let branch_key = field_string(&row, "branch_key")?;
            let point = AirwayFixPoint {
                airway_name: field_string(&row, "name")?,
                sequence: field_i32(&row, "sequence_number")?,
                position: LatLon {
                    lat: field_f64(&row, "Latitude")?,
                    lon: field_f64(&row, "Longitude")?,
                },
                nav_ref: resolve_named_nav_ref(LatLon {
                    lat: field_f64(&row, "Latitude")?,
                    lon: field_f64(&row, "Longitude")?,
                })?
                .unwrap_or(NavRef::LatLon(LatLon {
                    lat: field_f64(&row, "Latitude")?,
                    lon: field_f64(&row, "Longitude")?,
                })),
            };
            if let Some((_, points)) = by_branch.iter_mut().find(|(key, _)| key == &branch_key) {
                points.push(point);
            } else {
                by_branch.push((branch_key, vec![point]));
            }
        }
        return Ok(by_branch
            .into_iter()
            .map(|(branch_key, points)| AirwayBranch {
                display_name: airway_name.to_string(),
                branch_key,
                points,
            })
            .collect());
    }

    let rows = query(
        "
        SELECT trim(name) AS name,
               CAST(sequence AS INTEGER) AS sequence,
               Latitude AS Latitude,
               Longitude AS Longitude
        FROM airways
        WHERE trim(name) = trim(?1)
        ORDER BY CAST(sequence AS INTEGER)
        ",
        json!([airway_name]),
    )?;
    let points = rows
        .into_iter()
        .map(|row| {
            let position = LatLon {
                lat: field_f64(&row, "Latitude")?,
                lon: field_f64(&row, "Longitude")?,
            };
            Ok(AirwayFixPoint {
                airway_name: field_string(&row, "name")?,
                sequence: field_i32(&row, "sequence")?,
                position,
                nav_ref: resolve_named_nav_ref(position)?.unwrap_or(NavRef::LatLon(position)),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(split_airway_branches(points))
}

fn suggest_airways_near(anchor: &NavRef, limit: usize) -> AppResult<Vec<AirwaySuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let anchor_position = resolve_nav_ref_position(anchor, None)?;
    let mut suggestions = Vec::new();
    for radius_nm in [25.0, 50.0, 100.0, 200.0, 400.0] {
        suggestions = if table_exists("airways_branch")? {
            query_airway_suggestions_from_branch_table(anchor_position, radius_nm, limit)?
        } else {
            query_airway_suggestions_from_legacy_table(anchor_position, radius_nm, limit)?
        };
        if suggestions.len() >= limit {
            break;
        }
    }
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

fn suggest_waypoint_identifiers(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
) -> AppResult<Vec<WaypointIdentifierSuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let prefix = prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let anchor = component_insert_anchor(plan, component_index, before)?;
    let anchor_position = resolve_nav_ref_position(&anchor, None)?;
    let query_limit = (limit.max(16) * 16).min(512) as i64;
    let like_prefix = format!("{prefix}%");
    let rows = query(
        "
        SELECT identifier, kind, city, state, facility_name, lat, lon FROM (
            SELECT trim(LocationID) AS identifier, 'airport' AS kind,
                   trim(City) AS city,
                   trim(State) AS state,
                   trim(FacilityName) AS facility_name,
                   CAST(ARPLatitude AS REAL) AS lat, CAST(ARPLongitude AS REAL) AS lon
              FROM airports
             WHERE trim(LocationID) LIKE ?1
            UNION ALL
            SELECT trim(LocationID) AS identifier, 'navaid' AS kind,
                   '' AS city,
                   '' AS state,
                   trim(FacilityName) AS facility_name,
                   CAST(ARPLatitude AS REAL) AS lat, CAST(ARPLongitude AS REAL) AS lon
              FROM nav
             WHERE trim(LocationID) LIKE ?1
            UNION ALL
            SELECT trim(LocationID) AS identifier, 'fix' AS kind,
                   '' AS city,
                   '' AS state,
                   trim(FacilityName) AS facility_name,
                   CAST(ARPLatitude AS REAL) AS lat, CAST(ARPLongitude AS REAL) AS lon
              FROM fix
             WHERE trim(LocationID) LIKE ?1
        )
        WHERE identifier <> ''
        ORDER BY length(identifier), identifier, kind
        LIMIT ?2
        ",
        json!([like_prefix, query_limit]),
    )?;
    let mut suggestions = Vec::new();
    for row in rows {
        let identifier = field_string(&row, "identifier")?;
        let kind = field_string(&row, "kind")?;
        let city = field_string(&row, "city")?;
        let state = field_string(&row, "state")?;
        let facility_name = field_string(&row, "facility_name")?;
        let position = LatLon {
            lat: field_f64(&row, "lat")?,
            lon: field_f64(&row, "lon")?,
        };
        let nav_ref = match kind.as_str() {
            "airport" => NavRef::Airport(identifier.clone()),
            "navaid" => NavRef::Navaid(identifier.clone()),
            _ => NavRef::Fix(identifier.clone()),
        };
        let display_name = waypoint_identifier_display_name(&kind, &city, &state, &facility_name);
        suggestions.push(WaypointIdentifierSuggestion {
            identifier,
            nav_ref,
            kind,
            display_name,
            distance_from_anchor_nm: distance_nm(anchor_position, position),
        });
    }
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.identifier.cmp(&right.identifier))
            .then_with(|| {
                nav_ref_kind_order(&left.nav_ref).cmp(&nav_ref_kind_order(&right.nav_ref))
            })
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

fn waypoint_identifier_display_name(
    kind: &str,
    city: &str,
    state: &str,
    facility_name: &str,
) -> String {
    let city = city.trim();
    let state = state.trim();
    let facility_name = facility_name.trim();
    if kind == "airport" && !city.is_empty() {
        let city = titlecase_nav_label(city);
        return if state.is_empty() {
            city
        } else {
            format!("{city}, {}", state.to_ascii_uppercase())
        };
    }
    titlecase_nav_label(facility_name)
}

fn titlecase_nav_label(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut normalized = first.to_uppercase().collect::<String>();
                    normalized.push_str(&chars.as_str().to_ascii_lowercase());
                    normalized
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn component_insert_anchor(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
) -> AppResult<NavRef> {
    let plan = plan.clone().normalized();
    let component = plan
        .route_components
        .get(component_index)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        })?;
    let waypoint = match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => {
            if before {
                Some(airway.entry.clone())
            } else {
                Some(airway.exit.clone())
            }
        }
        RouteComponent::Procedure { .. } => {
            let mut legs = plan.resolved_legs.iter().filter(|leg| {
                matches!(
                    leg.source,
                    ResolvedLegSource::RouteComponent { component_index: index } if index == component_index
                )
            });
            if before {
                legs.next().map(|leg| leg.from.clone())
            } else {
                legs.last().map(|leg| leg.to.clone())
            }
        }
    };
    waypoint.ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "selected component has no waypoint anchor".to_string(),
    })
}

fn nav_ref_kind_order(nav_ref: &NavRef) -> usize {
    match nav_ref {
        NavRef::Navaid(_) => 0,
        NavRef::Airport(_) => 1,
        NavRef::Fix(_) => 2,
        NavRef::LatLon(_) => 3,
    }
}

fn query_airway_suggestions_from_branch_table(
    anchor_position: LatLon,
    radius_nm: f64,
    limit: usize,
) -> AppResult<Vec<AirwaySuggestion>> {
    let bounds = search_bounds(anchor_position, radius_nm);
    let sql_limit = (limit.saturating_mul(4).max(256)) as i64;
    let rows = query(
        "
        SELECT trim(name) AS name,
               trim(branch_key) AS branch_key,
               CAST(sequence_number AS INTEGER) AS sequence_number,
               Latitude AS Latitude,
               Longitude AS Longitude
        FROM airways_branch
        WHERE Latitude BETWEEN ?1 AND ?2
          AND Longitude BETWEEN ?3 AND ?4
        ORDER BY ((Latitude - ?5) * (Latitude - ?5)) + ((Longitude - ?6) * (Longitude - ?6))
        LIMIT ?7
        ",
        json!([
            bounds.min_lat,
            bounds.max_lat,
            bounds.min_lon,
            bounds.max_lon,
            anchor_position.lat,
            anchor_position.lon,
            sql_limit
        ]),
    )?;
    let mut seen = HashMap::<String, AirwaySuggestion>::new();
    for row in rows {
        let position = LatLon {
            lat: field_f64(&row, "Latitude")?,
            lon: field_f64(&row, "Longitude")?,
        };
        let nav_ref = resolve_named_nav_ref(position)?.unwrap_or(NavRef::LatLon(position));
        let distance_from_anchor_nm = distance_nm(anchor_position, position);
        let airway_name = field_string(&row, "name")?;
        let suggestion = AirwaySuggestion {
            airway_name: airway_name.clone(),
            nearest_branch_key: Some(field_string(&row, "branch_key")?),
            nearest_nav_ref: nav_ref,
            nearest_sequence: field_i32(&row, "sequence_number")?,
            distance_from_anchor_nm,
        };
        match seen.get(&airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(airway_name, suggestion);
            }
        }
    }
    Ok(seen.into_values().collect())
}

fn query_airway_suggestions_from_legacy_table(
    anchor_position: LatLon,
    radius_nm: f64,
    limit: usize,
) -> AppResult<Vec<AirwaySuggestion>> {
    let bounds = search_bounds(anchor_position, radius_nm);
    let sql_limit = (limit.saturating_mul(8).max(256)) as i64;
    let rows = query(
        "
        SELECT trim(name) AS name,
               CAST(sequence AS INTEGER) AS sequence,
               Latitude AS Latitude,
               Longitude AS Longitude
        FROM airways
        WHERE Latitude BETWEEN ?1 AND ?2
          AND Longitude BETWEEN ?3 AND ?4
        ORDER BY ((Latitude - ?5) * (Latitude - ?5)) + ((Longitude - ?6) * (Longitude - ?6))
        LIMIT ?7
        ",
        json!([
            bounds.min_lat,
            bounds.max_lat,
            bounds.min_lon,
            bounds.max_lon,
            anchor_position.lat,
            anchor_position.lon,
            sql_limit
        ]),
    )?;
    let mut seen = HashMap::<String, AirwaySuggestion>::new();
    for row in rows {
        let position = LatLon {
            lat: field_f64(&row, "Latitude")?,
            lon: field_f64(&row, "Longitude")?,
        };
        let distance_from_anchor_nm = distance_nm(anchor_position, position);
        let airway_name = field_string(&row, "name")?;
        let suggestion = AirwaySuggestion {
            airway_name: airway_name.clone(),
            nearest_branch_key: None,
            nearest_nav_ref: resolve_named_nav_ref(position)?.unwrap_or(NavRef::LatLon(position)),
            nearest_sequence: field_i32(&row, "sequence")?,
            distance_from_anchor_nm,
        };
        match seen.get(&airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(airway_name, suggestion);
            }
        }
    }
    Ok(seen.into_values().collect())
}

fn materialize_airway_selection(
    start_component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
) -> AppResult<(AirwaySegment, Vec<ResolvedLeg>)> {
    if entry.airway_name != exit.airway_name || entry.branch_key != exit.branch_key {
        return Err(invalid(format!(
            "entry airway {} branch {} does not match exit airway {} branch {}",
            entry.airway_name, entry.branch_key, exit.airway_name, exit.branch_key
        )));
    }
    let branch = load_airway_branches(&entry.airway_name)?
        .into_iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .ok_or_else(|| {
            invalid(format!(
                "unknown airway branch {} {}",
                entry.airway_name, entry.branch_key
            ))
        })?;
    let first_index = entry.branch_point_index;
    let last_index = exit.branch_point_index;
    if first_index == last_index {
        return Err(invalid("airway entry and exit must differ"));
    }
    let slice = if first_index < last_index {
        branch.points[first_index..=last_index].to_vec()
    } else {
        branch.points[last_index..=first_index]
            .iter()
            .cloned()
            .rev()
            .collect()
    };
    let mut resolved_legs = Vec::new();
    for (index, pair) in slice.windows(2).enumerate() {
        resolved_legs.push(ResolvedLeg {
            id: format!(
                "airway:{}:{}:{}",
                entry.airway_name, entry.branch_key, index
            ),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: ResolvedLegSource::RouteComponent {
                component_index: start_component_index + 1,
            },
            procedure_provenance: None,
        });
    }
    Ok((
        AirwaySegment {
            name: entry.airway_name.clone(),
            branch_key: Some(entry.branch_key.clone()),
            entry: entry.nav_ref.clone(),
            exit: exit.nav_ref.clone(),
        },
        resolved_legs,
    ))
}

fn list_procedures(airport_id: &str, kind: ProcedureKind) -> AppResult<Vec<ProcedureSummary>> {
    let rows = query(
        "
        SELECT DISTINCT
          trim(airport_identifier) AS airport_id,
          trim(sid_star_approach_identifier) AS procedure_id,
          trim(route_type) AS route_type
        FROM cifp_sid_star_app
        WHERE trim(airport_identifier) = trim(?1)
        ORDER BY trim(sid_star_approach_identifier), trim(route_type)
        ",
        json!([airport_id]),
    )?;
    let mut procedures = rows
        .into_iter()
        .map(|row| {
            Ok(ProcedureSummary {
                airport_id: field_string(&row, "airport_id")?,
                procedure_id: field_string(&row, "procedure_id")?,
                kind: infer_procedure_kind(&field_string(&row, "route_type")?),
            })
        })
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .filter(|procedure| procedure.kind == kind)
        .collect::<Vec<_>>();
    procedures.sort_by(|left, right| left.procedure_id.cmp(&right.procedure_id));
    procedures
        .dedup_by(|left, right| left.procedure_id == right.procedure_id && left.kind == right.kind);
    Ok(procedures)
}

fn load_procedure_distinct_rows(
    airport_id: &str,
    procedure_id: &str,
) -> AppResult<Vec<ProcedureDistinctRow>> {
    query(
        "
        SELECT DISTINCT
          trim(route_type) AS route_type,
          trim(transition_identifier) AS transition_id
        FROM cifp_sid_star_app
        WHERE trim(airport_identifier) = trim(?1)
          AND trim(sid_star_approach_identifier) = trim(?2)
        ORDER BY trim(route_type), trim(transition_identifier)
        ",
        json!([airport_id, procedure_id]),
    )?
    .into_iter()
    .map(|row| {
        Ok(ProcedureDistinctRow {
            route_type: field_string(&row, "route_type")?,
            transition_id: field_string(&row, "transition_id")?,
        })
    })
    .collect()
}

fn load_procedure_materialization_records(
    airport_id: &str,
    procedure_id: &str,
) -> AppResult<Vec<ProcedureLegMaterializationRecord>> {
    query(
        "
        SELECT
          trim(airport_identifier) AS airport_id,
          trim(sid_star_approach_identifier) AS procedure_id,
          trim(route_type) AS route_type,
          trim(transition_identifier) AS transition_id,
          CAST(sequence_number AS INTEGER) AS sequence,
          trim(fix_identifier) AS fix_identifier,
          trim(recommended_navaid) AS recommended_navaid,
          trim((SELECT Variation FROM nav WHERE trim(LocationID) = trim(fix_identifier) LIMIT 1)) AS nav_magnetic_variation,
          trim((SELECT Variation FROM nav WHERE trim(LocationID) = trim(recommended_navaid) LIMIT 1)) AS defining_nav_magnetic_variation,
          trim((SELECT MagneticVariation FROM airports WHERE trim(LocationID) = trim(airport_identifier) LIMIT 1)) AS airport_magnetic_variation,
          trim(altitude_1) AS altitude_1,
          trim(altitude_2) AS altitude_2,
          trim(path_and_termination) AS path_termination,
          trim(turn_direction) AS turn_direction,
          trim(magnetic_course) AS magnetic_course,
          trim(route_distance_holding_distance_or_time) AS route_distance_or_time
        FROM cifp_sid_star_app
        WHERE trim(airport_identifier) = trim(?1)
          AND trim(sid_star_approach_identifier) = trim(?2)
          AND trim(path_and_termination) <> ''
        ORDER BY trim(route_type), trim(transition_identifier), CAST(sequence_number AS INTEGER)
        ",
        json!([airport_id, procedure_id]),
    )?
    .into_iter()
    .map(|row| {
        let airport_id = field_string(&row, "airport_id")?;
        let procedure_id = field_string(&row, "procedure_id")?;
        let fix_identifier = field_string(&row, "fix_identifier")?;
        let recommended_navaid = field_string(&row, "recommended_navaid")?;
        let nav_ref = classify_identifier(&fix_identifier)?;
        let defining_nav_ref = classify_identifier(&recommended_navaid)?;
        let nav_position = nav_ref
            .as_ref()
            .and_then(|nav_ref| resolve_nav_ref_position(nav_ref, Some(&airport_id)).ok());
        let defining_nav_position = defining_nav_ref
            .as_ref()
            .and_then(|nav_ref| resolve_nav_ref_position(nav_ref, Some(&airport_id)).ok());
        let path_termination = field_string(&row, "path_termination")?;
        Ok(ProcedureLegMaterializationRecord {
            key: ProcedureVariantKey {
                airport_id: airport_id.clone(),
                procedure_id,
                route_type: field_string(&row, "route_type")?,
                transition_id: field_string(&row, "transition_id")?,
            },
            sequence: field_i32(&row, "sequence")?,
            nav_ref,
            nav_position,
            nav_magnetic_variation_deg: field_optional_string(&row, "nav_magnetic_variation")
                .as_deref()
                .and_then(|value| value.trim().parse::<f64>().ok()),
            defining_nav_ref,
            defining_nav_position,
            defining_nav_magnetic_variation_deg: field_optional_string(&row, "defining_nav_magnetic_variation")
                .as_deref()
                .and_then(|value| value.trim().parse::<f64>().ok()),
            airport_magnetic_variation_deg: parse_airport_magnetic_variation(
                &field_string(&row, "airport_magnetic_variation")?,
            ),
            altitude_1_ft: parse_cifp_altitude_ft(&field_string(&row, "altitude_1")?),
            altitude_2_ft: parse_cifp_altitude_ft(&field_string(&row, "altitude_2")?),
            path_termination_kind: interpret_path_termination(&path_termination),
            path_termination,
            turn_direction: non_empty(field_string(&row, "turn_direction")?),
            magnetic_course_deg: parse_cifp_tenths_value(&field_string(&row, "magnetic_course")?),
            route_distance_or_time: non_empty(field_string(&row, "route_distance_or_time")?),
        })
    })
    .collect()
}

fn load_cifp_tpp_matches_for_airport(airport_id: &str) -> AppResult<Vec<CifpTppMatchRow>> {
    load_cifp_tpp_matches(
        "
        SELECT
          trim(airport_id) AS airport_id,
          trim(cifp_id) AS cifp_id,
          trim(plate_id) AS plate_id,
          trim(plate_label) AS plate_label,
          trim(package_id) AS package_id,
          CAST(public AS INTEGER) AS public,
          CAST(priority AS INTEGER) AS priority,
          trim(match_kind) AS match_kind,
          CAST(is_primary AS INTEGER) AS is_primary
        FROM cifp_tpp_matches
        WHERE trim(airport_id) = trim(?1)
        ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
        ",
        json!([airport_id]),
    )
}

fn load_cifp_tpp_matches_for_procedure(
    airport_id: &str,
    cifp_id: &str,
) -> AppResult<Vec<CifpTppMatchRow>> {
    load_cifp_tpp_matches(
        "
        SELECT
          trim(airport_id) AS airport_id,
          trim(cifp_id) AS cifp_id,
          trim(plate_id) AS plate_id,
          trim(plate_label) AS plate_label,
          trim(package_id) AS package_id,
          CAST(public AS INTEGER) AS public,
          CAST(priority AS INTEGER) AS priority,
          trim(match_kind) AS match_kind,
          CAST(is_primary AS INTEGER) AS is_primary
        FROM cifp_tpp_matches
        WHERE trim(airport_id) = trim(?1)
          AND trim(cifp_id) = trim(?2)
        ORDER BY CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
        ",
        json!([airport_id, cifp_id]),
    )
}

fn load_cifp_tpp_matches_for_plate(plate_id: &str) -> AppResult<Vec<CifpTppMatchRow>> {
    load_cifp_tpp_matches(
        "
        SELECT
          trim(airport_id) AS airport_id,
          trim(cifp_id) AS cifp_id,
          trim(plate_id) AS plate_id,
          trim(plate_label) AS plate_label,
          trim(package_id) AS package_id,
          CAST(public AS INTEGER) AS public,
          CAST(priority AS INTEGER) AS priority,
          trim(match_kind) AS match_kind,
          CAST(is_primary AS INTEGER) AS is_primary
        FROM cifp_tpp_matches
        WHERE trim(plate_id) = trim(?1)
        ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
        ",
        json!([plate_id]),
    )
}

fn load_cifp_tpp_matches(sql: &str, bind: Value) -> AppResult<Vec<CifpTppMatchRow>> {
    query(sql, bind)?
        .into_iter()
        .map(|row| {
            Ok(CifpTppMatchRow {
                airport_id: field_string(&row, "airport_id")?,
                cifp_id: field_string(&row, "cifp_id")?,
                plate_id: field_string(&row, "plate_id")?,
                plate_label: field_string(&row, "plate_label")?,
                package_id: field_string(&row, "package_id")?,
                public: field_i32(&row, "public")?,
                priority: field_i32(&row, "priority")?,
                match_kind: field_string(&row, "match_kind")?,
                is_primary: field_i32(&row, "is_primary")?,
            })
        })
        .collect()
}

fn classify_identifier(identifier: &str) -> AppResult<Option<NavRef>> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let exists_as_airport = identifier_exists("airports", trimmed)?;
    let exists_as_navaid = identifier_exists("nav", trimmed)?;
    let exists_as_fix = identifier_exists("fix", trimmed)?;
    Ok(classify_procedure_identifier(
        trimmed,
        exists_as_airport,
        exists_as_navaid,
        exists_as_fix,
    ))
}

fn identifier_exists(table: &str, identifier: &str) -> AppResult<bool> {
    Ok(!query(
        &format!("SELECT LocationID FROM {table} WHERE trim(LocationID) = trim(?1) LIMIT 1"),
        json!([identifier]),
    )?
    .is_empty())
}

fn resolve_nav_ref_position(nav_ref: &NavRef, airport_id: Option<&str>) -> AppResult<LatLon> {
    match nav_ref {
        NavRef::LatLon(position) => Ok(*position),
        NavRef::Airport(code) => lookup_nav_ref_position("airports", code),
        NavRef::Navaid(code) => lookup_nav_ref_position("nav", code),
        NavRef::Fix(code) if code.trim().starts_with("RW") => {
            if let Some(airport_id) = airport_id {
                if let Some(position) = lookup_runway_threshold_position(airport_id, code)? {
                    return Ok(position);
                }
            }
            lookup_nav_ref_position("fix", code)
        }
        NavRef::Fix(code) => lookup_nav_ref_position("fix", code),
    }
}

fn resolve_nav_symbol_feature(nav_ref: &NavRef) -> AppResult<Option<NavSymbolFeature>> {
    let record = match nav_ref {
        NavRef::Airport(code) => airport_symbol_record(code)?,
        NavRef::Navaid(code) => navaid_symbol_record(code)?,
        NavRef::Fix(code) => fix_symbol_record(code)?,
        NavRef::LatLon(_) => None,
    };
    Ok(record.and_then(|record| point_vector_record_to_symbol_feature(&record)))
}

fn airport_symbol_record(code: &str) -> AppResult<Option<PointVectorRecord>> {
    let rows = query(
        "
        SELECT trim(LocationID) AS id,
               CAST(ARPLatitude AS REAL) AS lat,
               CAST(ARPLongitude AS REAL) AS lon,
               trim(FacilityName) AS label,
               trim(Type) AS kind,
               trim(ATCT) AS atct,
               trim(FuelTypes) AS fuel_types,
               trim(Use) AS use_code
        FROM airports
        WHERE trim(LocationID) = trim(?1)
        LIMIT 1
        ",
        json!([code]),
    )?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let id = field_string(row, "id")?;
    let kind = field_string(row, "kind")?;
    let runway_info = airport_runway_info(&id)?;
    let kind_upper = kind.trim().to_ascii_uppercase();
    Ok(Some(PointVectorRecord {
        id: format!("airports:{id}"),
        kind: kind.to_lowercase(),
        lat: field_f64(row, "lat")?,
        lon: field_f64(row, "lon")?,
        label: field_string(row, "label")?,
        style_class: "airport".to_string(),
        towered: Some(field_string(row, "atct")?.trim().eq_ignore_ascii_case("Y")),
        fuel_available: Some(!field_string(row, "fuel_types")?.trim().is_empty()),
        public_use: Some(
            field_string(row, "use_code")?
                .trim()
                .eq_ignore_ascii_case("PU"),
        ),
        private_use: Some(
            field_string(row, "use_code")?
                .trim()
                .eq_ignore_ascii_case("PR"),
        ),
        has_paved_runway: runway_info.as_ref().map(|runway| runway.has_paved_runway),
        heliport: Some(kind_upper.contains("HELIPORT")),
        has_water_runway: Some(
            runway_info
                .as_ref()
                .map(|runway| runway.has_water_runway)
                .unwrap_or(false)
                || kind.trim().eq_ignore_ascii_case("SEAPLANE BAS"),
        ),
        longest_runway_length_ft: runway_info.as_ref().map(|runway| runway.length_ft),
        longest_runway_heading_true_deg: runway_info.as_ref().map(|runway| runway.heading_true_deg),
    }))
}

fn navaid_symbol_record(code: &str) -> AppResult<Option<PointVectorRecord>> {
    let rows = query(
        "
        SELECT trim(LocationID) AS id,
               CAST(ARPLatitude AS REAL) AS lat,
               CAST(ARPLongitude AS REAL) AS lon,
               trim(FacilityName) AS label,
               trim(Type) AS kind
        FROM nav
        WHERE trim(LocationID) = trim(?1)
          AND UPPER(trim(Type)) IN ('VOR', 'VOR/DME', 'VORTAC')
        LIMIT 1
        ",
        json!([code]),
    )?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    Ok(Some(PointVectorRecord {
        id: format!("nav:{}", field_string(row, "id")?),
        kind: field_string(row, "kind")?.to_lowercase(),
        lat: field_f64(row, "lat")?,
        lon: field_f64(row, "lon")?,
        label: field_string(row, "label")?,
        style_class: "nav".to_string(),
        towered: None,
        fuel_available: None,
        public_use: None,
        private_use: None,
        has_paved_runway: None,
        heliport: None,
        has_water_runway: None,
        longest_runway_length_ft: None,
        longest_runway_heading_true_deg: None,
    }))
}

fn fix_symbol_record(code: &str) -> AppResult<Option<PointVectorRecord>> {
    let rows = query(
        "
        SELECT trim(LocationID) AS id,
               CAST(ARPLatitude AS REAL) AS lat,
               CAST(ARPLongitude AS REAL) AS lon,
               trim(FacilityName) AS label,
               trim(Type) AS kind
        FROM fix
        WHERE trim(LocationID) = trim(?1)
        LIMIT 1
        ",
        json!([code]),
    )?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    Ok(Some(PointVectorRecord {
        id: format!("fix:{}", field_string(row, "id")?),
        kind: field_string(row, "kind")?.to_lowercase(),
        lat: field_f64(row, "lat")?,
        lon: field_f64(row, "lon")?,
        label: field_string(row, "label")?,
        style_class: "fix".to_string(),
        towered: None,
        fuel_available: None,
        public_use: None,
        private_use: None,
        has_paved_runway: None,
        heliport: None,
        has_water_runway: None,
        longest_runway_length_ft: None,
        longest_runway_heading_true_deg: None,
    }))
}

#[derive(Debug, Clone, Copy)]
struct AirportRunwaySymbolInfo {
    length_ft: f64,
    heading_true_deg: f64,
    has_paved_runway: bool,
    has_water_runway: bool,
}

fn airport_runway_info(airport_id: &str) -> AppResult<Option<AirportRunwaySymbolInfo>> {
    let rows = query(
        "
        SELECT trim(Length) AS length,
               trim(Surface) AS surface,
               trim(LEHeadingT) AS le_heading,
               trim(LELatitude) AS le_lat,
               trim(LELongitude) AS le_lon,
               trim(HELatitude) AS he_lat,
               trim(HELongitude) AS he_lon
        FROM airportrunways
        WHERE trim(LocationID) = trim(?1)
        ",
        json!([airport_id]),
    )?;
    let mut best: Option<AirportRunwaySymbolInfo> = None;
    for row in rows {
        let length = parse_float(&field_string(&row, "length")?);
        if length <= 0.0 {
            continue;
        }
        let surface = field_string(&row, "surface")?.trim().to_ascii_uppercase();
        let has_paved_runway = surface_is_paved(&surface);
        let has_water_runway = surface.contains("WATER");
        let heading = parse_float(&field_string(&row, "le_heading")?);
        let heading = if heading > 0.0 {
            normalize_heading(heading)
        } else {
            let le_lat = parse_float(&field_string(&row, "le_lat")?);
            let le_lon = parse_float(&field_string(&row, "le_lon")?);
            let he_lat = parse_float(&field_string(&row, "he_lat")?);
            let he_lon = parse_float(&field_string(&row, "he_lon")?);
            if !valid_lat_lon(le_lat, le_lon) || !valid_lat_lon(he_lat, he_lon) {
                continue;
            }
            bearing_true_deg(le_lat, le_lon, he_lat, he_lon)
        };
        match best.as_mut() {
            Some(existing) if existing.length_ft >= length => {
                existing.has_paved_runway |= has_paved_runway;
                existing.has_water_runway |= has_water_runway;
            }
            _ => {
                best = Some(AirportRunwaySymbolInfo {
                    length_ft: length,
                    heading_true_deg: heading,
                    has_paved_runway,
                    has_water_runway,
                });
            }
        }
    }
    Ok(best)
}

fn surface_is_paved(surface: &str) -> bool {
    surface
        .split('-')
        .any(|part| matches!(part.trim(), "ASPH" | "CONC" | "BIT" | "PEM"))
}

fn parse_float(value: &str) -> f64 {
    value.trim().parse::<f64>().unwrap_or(0.0)
}

fn valid_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
}

fn bearing_true_deg(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64) -> f64 {
    let start_lat_rad = start_lat.to_radians();
    let end_lat_rad = end_lat.to_radians();
    let delta_lon_rad = (end_lon - start_lon).to_radians();
    let y = delta_lon_rad.sin() * end_lat_rad.cos();
    let x = start_lat_rad.cos() * end_lat_rad.sin()
        - start_lat_rad.sin() * end_lat_rad.cos() * delta_lon_rad.cos();
    normalize_heading(y.atan2(x).to_degrees())
}

fn normalize_heading(heading: f64) -> f64 {
    let normalized = heading.rem_euclid(360.0);
    if normalized == 0.0 {
        360.0
    } else {
        normalized
    }
}

fn lookup_nav_ref_position(table: &str, code: &str) -> AppResult<LatLon> {
    let rows = query(
        &format!(
            "SELECT ARPLatitude AS lat, ARPLongitude AS lon FROM {table}
             WHERE trim(LocationID) = trim(?1)
             LIMIT 1"
        ),
        json!([code]),
    )?;
    let row = rows
        .first()
        .ok_or_else(|| invalid(format!("unknown nav ref {code} in {table}")))?;
    Ok(LatLon {
        lat: field_f64(row, "lat")?,
        lon: field_f64(row, "lon")?,
    })
}

fn lookup_runway_threshold_position(
    airport_id: &str,
    runway_code: &str,
) -> AppResult<Option<LatLon>> {
    let runway_ident = runway_code
        .trim()
        .trim_start_matches("RW")
        .trim_start_matches("rw");
    let rows = query(
        "
        SELECT
          CASE WHEN trim(LEIdent) = trim(?2) THEN CAST(LELatitude AS REAL) ELSE CAST(HELatitude AS REAL) END AS lat,
          CASE WHEN trim(LEIdent) = trim(?2) THEN CAST(LELongitude AS REAL) ELSE CAST(HELongitude AS REAL) END AS lon
        FROM airportrunways
        WHERE trim(LocationID) = trim(?1)
          AND (trim(LEIdent) = trim(?2) OR trim(HEIdent) = trim(?2))
        LIMIT 1
        ",
        json!([airport_id, runway_ident]),
    )?;
    rows.first()
        .map(|row| {
            Ok(LatLon {
                lat: field_f64(row, "lat")?,
                lon: field_f64(row, "lon")?,
            })
        })
        .transpose()
}

fn resolve_named_nav_ref(position: LatLon) -> AppResult<Option<NavRef>> {
    for (table, variant) in [("fix", 0usize), ("nav", 1usize), ("airports", 2usize)] {
        let rows = query(
            &format!(
                "SELECT trim(LocationID) AS LocationID FROM {table}
                 WHERE abs(ARPLatitude - ?1) < 1e-6
                   AND abs(ARPLongitude - ?2) < 1e-6
                 LIMIT 1"
            ),
            json!([position.lat, position.lon]),
        )?;
        if let Some(row) = rows.first() {
            let id = field_string(row, "LocationID")?;
            return Ok(Some(match variant {
                0 => NavRef::Fix(id),
                1 => NavRef::Navaid(id),
                _ => NavRef::Airport(id),
            }));
        }
    }
    Ok(None)
}

fn route_status_for_leg(
    ui_state: &app_core::FlightPlanUiState,
    leg_index: usize,
) -> FlightPlanRouteSegmentStatus {
    let Some(guidance) = ui_state.guidance.as_ref() else {
        return FlightPlanRouteSegmentStatus::Remaining;
    };
    if let Some(active_leg_index) = guidance.active_leg_index {
        return if leg_index < active_leg_index {
            FlightPlanRouteSegmentStatus::Completed
        } else if leg_index == active_leg_index {
            FlightPlanRouteSegmentStatus::Active
        } else {
            FlightPlanRouteSegmentStatus::Remaining
        };
    }
    let split_index = guidance.display_split_leg_index.unwrap_or(0);
    if leg_index < split_index {
        FlightPlanRouteSegmentStatus::Completed
    } else {
        FlightPlanRouteSegmentStatus::Remaining
    }
}

fn infer_procedure_kind(route_type: &str) -> ProcedureKind {
    match route_type.trim() {
        "1" | "2" | "3" => ProcedureKind::Star,
        "4" | "5" | "6" => ProcedureKind::Sid,
        _ => ProcedureKind::Approach,
    }
}

fn split_airway_branches(points: Vec<AirwayFixPoint>) -> Vec<AirwayBranch> {
    let mut branches: Vec<Vec<AirwayFixPoint>> = Vec::new();
    for point in points {
        let mut assigned_branch = None;
        let mut best_distance = f64::MAX;
        for (branch_index, branch) in branches.iter().enumerate() {
            let Some(last) = branch.last() else {
                continue;
            };
            if point.sequence < last.sequence {
                continue;
            }
            let distance = distance_nm(last.position, point.position);
            if distance <= 500.0 && distance < best_distance {
                assigned_branch = Some(branch_index);
                best_distance = distance;
            }
        }
        if let Some(branch_index) = assigned_branch {
            branches[branch_index].push(point);
        } else {
            branches.push(vec![point]);
        }
    }
    branches
        .into_iter()
        .enumerate()
        .map(|(index, points)| AirwayBranch {
            display_name: points
                .first()
                .map(|point| point.airway_name.clone())
                .unwrap_or_default(),
            branch_key: format!(
                "{}-{}",
                points
                    .first()
                    .map(|point| point.airway_name.as_str())
                    .unwrap_or("AWY"),
                (b'A' + index as u8) as char
            ),
            points,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct SearchBounds {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

fn search_bounds(anchor: LatLon, radius_nm: f64) -> SearchBounds {
    let lat_delta = radius_nm / 60.0;
    let lon_delta = radius_nm / (60.0 * anchor.lat.to_radians().cos().abs().max(0.1));
    SearchBounds {
        min_lat: anchor.lat - lat_delta,
        max_lat: anchor.lat + lat_delta,
        min_lon: anchor.lon - lon_delta,
        max_lon: anchor.lon + lon_delta,
    }
}

fn distance_nm(a: LatLon, b: LatLon) -> f64 {
    let earth_radius_nm = 3440.065_f64;
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * earth_radius_nm * h.sqrt().asin()
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn field_string(row: &Value, name: &str) -> AppResult<String> {
    let value = row
        .get(name)
        .ok_or_else(|| invalid(format!("missing column {name}")))?;
    Ok(match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        _ => return Err(invalid(format!("column {name} is not scalar"))),
    })
}

fn field_optional_string(row: &Value, name: &str) -> Option<String> {
    row.get(name).and_then(|value| match value {
        Value::Null => None,
        Value::String(value) if value.trim().is_empty() => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn field_i32(row: &Value, name: &str) -> AppResult<i32> {
    let value = row
        .get(name)
        .ok_or_else(|| invalid(format!("missing column {name}")))?;
    if let Some(value) = value.as_i64() {
        return i32::try_from(value)
            .map_err(|_| invalid(format!("column {name} is out of i32 range")));
    }
    field_string(row, name)?
        .trim()
        .parse::<i32>()
        .map_err(|err| invalid(format!("column {name} is not i32: {err}")))
}

fn field_f64(row: &Value, name: &str) -> AppResult<f64> {
    let value = row
        .get(name)
        .ok_or_else(|| invalid(format!("missing column {name}")))?;
    if let Some(value) = value.as_f64() {
        return Ok(value);
    }
    field_string(row, name)?
        .trim()
        .parse::<f64>()
        .map_err(|err| invalid(format!("column {name} is not f64: {err}")))
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: message.into(),
    }
}
