use std::collections::HashMap;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::planning::FlightPlanRowActionId;
use crate::{
    describe_plate_procedure_load_options, describe_procedure_options_from_rows,
    describe_show_plate_for_procedure, flight_leg_course_deg, flight_leg_distance_nm,
    great_circle_display_path,
    materialize_procedure_from_records, prepare_airway_presentation, AirwayAutoSelection,
    AirwayBranch, AirwayEntryCandidate, AirwayExitCandidate, AirwayPresentationPlan, AirwaySegment,
    AirwaySpatialPoint, AirwaySuggestion, AppError, AppErrorKind, AppResult, CifpTppMatchRow,
    FlightPlan, FlightPlanRouteSegment, FlightPlanRouteSegmentStatus, FlightPlanUiMutation,
    FlightPlanUiState, LatLon, MaterializedProcedure, NavKvLookup, NavKvQuery, NavKvStore, NavRef,
    NavSymbolFeature, PlateProcedureLoadCandidateInput, ProcedureDistinctRow, ProcedureKind,
    ProcedureLegMaterializationRecord, ProcedureLoadOption, ProcedureOptions, ProcedureSummary,
    ResolvedLeg, ResolvedLegSource, RouteComponent, WaypointIdentifierRecord,
    WaypointIdentifierSuggestion,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HadOperationOutcome {
    Complete { result: Value },
    NeedPages { pages: Vec<u32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HadOperation {
    ChartCatalog,
    PlateAirport {
        airport_id: String,
    },
    PlateById {
        plate_id: String,
    },
    FlightPlanUiState {
        plan: FlightPlan,
    },
    FlightPlanUiMutation {
        mutation: FlightPlanUiMutation,
    },
    ProjectFlightPlanRoute {
        plan: FlightPlan,
    },
    ResolveWaypointIdentifier {
        identifier: String,
    },
    ResolveNavRefPosition {
        nav_ref: NavRef,
    },
    ResolveNavSymbolFeature {
        nav_ref: NavRef,
    },
    SuggestWaypointIdentifiers {
        plan: FlightPlan,
        component_index: usize,
        before: bool,
        prefix: String,
        limit: usize,
    },
    SuggestAirwaysNearAnchor {
        anchor: NavRef,
        limit: usize,
    },
    AirwayBranches {
        airway_name: String,
    },
    PrepareAirwayPresentationForAnchors {
        airway_name: String,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    MaterializeAirwaySelection {
        start_component_index: usize,
        entry: AirwayEntryCandidate,
        exit: AirwayExitCandidate,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    MaterializeAirwayPresentationSelection {
        start_component_index: usize,
        presentation: AirwayPresentationPlan,
        entry_index: usize,
        exit_index: usize,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    ListProcedures {
        airport_id: String,
        procedure_kind: ProcedureKind,
    },
    DescribeProcedureOptions {
        airport_id: String,
        procedure_id: String,
        procedure_kind: ProcedureKind,
    },
    MaterializeProcedure {
        airport_id: String,
        procedure_id: String,
        procedure_kind: ProcedureKind,
        runway_transition: Option<String>,
        enroute_transition: Option<String>,
        component_index: usize,
    },
    FindProcedurePlateMatch {
        airport_id: String,
        cifp_id: String,
    },
    DescribePlateProcedureLoads {
        plan: FlightPlan,
        plate_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum HadReadError {
    NeedPages(Vec<u32>),
    Fatal(String),
}

impl From<AppError> for HadReadError {
    fn from(err: AppError) -> Self {
        Self::Fatal(err.to_string())
    }
}

pub fn run_had_operation(store: &NavKvStore, op: HadOperation) -> AppResult<HadOperationOutcome> {
    match run_had_operation_value(store, op) {
        Ok(result) => Ok(HadOperationOutcome::Complete { result }),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedPages { pages }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn run_had_operation_value(store: &NavKvStore, op: HadOperation) -> Result<Value, HadReadError> {
    let value = match op {
        HadOperation::ChartCatalog => {
            serde_json::to_value(read_optional::<Value>(store, NavKvQuery::ChartCatalog)?)?
        }
        HadOperation::PlateAirport { airport_id } => serde_json::to_value(read_optional::<Value>(
            store,
            NavKvQuery::PlateAirport { airport_id },
        )?)?,
        HadOperation::PlateById { plate_id } => serde_json::to_value(read_optional::<Value>(
            store,
            NavKvQuery::PlateById { plate_id },
        )?)?,
        HadOperation::FlightPlanUiState { plan } => {
            serde_json::to_value(flight_plan_ui_state(store, plan)?)?
        }
        HadOperation::FlightPlanUiMutation { mutation } => {
            serde_json::to_value(FlightPlanUiMutation {
                ui_state: flight_plan_ui_state(store, mutation.plan.clone())?,
                ..mutation
            })?
        }
        HadOperation::ProjectFlightPlanRoute { plan } => {
            serde_json::to_value(project_flight_plan_route(store, &plan)?)?
        }
        HadOperation::ResolveWaypointIdentifier { identifier } => serde_json::to_value(
            read_optional::<NavRef>(store, NavKvQuery::WaypointIdentifier { identifier })?,
        )?,
        HadOperation::ResolveNavRefPosition { nav_ref } => {
            serde_json::to_value(nav_ref_position(store, &nav_ref, None)?)?
        }
        HadOperation::ResolveNavSymbolFeature { nav_ref } => {
            serde_json::to_value(nav_symbol_feature(store, &nav_ref)?)?
        }
        HadOperation::SuggestWaypointIdentifiers {
            plan,
            component_index,
            before,
            prefix,
            limit,
        } => serde_json::to_value(suggest_waypoint_identifiers(
            store,
            &plan,
            component_index,
            before,
            &prefix,
            limit,
        )?)?,
        HadOperation::SuggestAirwaysNearAnchor { anchor, limit } => {
            serde_json::to_value(suggest_airways_near_anchor(store, &anchor, limit)?)?
        }
        HadOperation::AirwayBranches { airway_name } => {
            serde_json::to_value(read_required::<Vec<AirwayBranch>>(
                store,
                NavKvQuery::AirwayBranches { airway_name },
                "airway branches",
            )?)?
        }
        HadOperation::PrepareAirwayPresentationForAnchors {
            airway_name,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(prepare_airway_presentation_for_anchors(
            store,
            &airway_name,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::MaterializeAirwaySelection {
            start_component_index,
            entry,
            exit,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(materialize_airway_selection(
            store,
            start_component_index,
            entry,
            exit,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::MaterializeAirwayPresentationSelection {
            start_component_index,
            presentation,
            entry_index,
            exit_index,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(materialize_airway_presentation_selection(
            store,
            start_component_index,
            presentation,
            entry_index,
            exit_index,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::ListProcedures {
            airport_id,
            procedure_kind,
        } => serde_json::to_value(read_required::<Vec<ProcedureSummary>>(
            store,
            NavKvQuery::ProcedureList {
                airport_id,
                procedure_kind,
            },
            "procedure list",
        )?)?,
        HadOperation::DescribeProcedureOptions {
            airport_id,
            procedure_id,
            procedure_kind,
        } => serde_json::to_value(describe_procedure_options(
            store,
            &airport_id,
            &procedure_id,
            procedure_kind,
        )?)?,
        HadOperation::MaterializeProcedure {
            airport_id,
            procedure_id,
            procedure_kind,
            runway_transition,
            enroute_transition,
            component_index,
        } => serde_json::to_value(materialize_procedure(
            store,
            &airport_id,
            &procedure_id,
            procedure_kind,
            runway_transition.as_deref(),
            enroute_transition.as_deref(),
            component_index,
        )?)?,
        HadOperation::FindProcedurePlateMatch {
            airport_id,
            cifp_id,
        } => {
            let rows = read_optional::<Vec<CifpTppMatchRow>>(
                store,
                NavKvQuery::PlateCifpMatch {
                    airport_id,
                    cifp_id,
                },
            )?;
            serde_json::to_value(rows.and_then(describe_show_plate_for_procedure))?
        }
        HadOperation::DescribePlateProcedureLoads { plan, plate_id } => {
            serde_json::to_value(describe_plate_loads(store, &plan, &plate_id)?)?
        }
    };
    Ok(value)
}

fn read_required<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
    family: &str,
) -> Result<T, HadReadError> {
    read_optional(store, query.clone())?.ok_or_else(|| {
        let key = crate::nav_kv_key_for_query(&query).unwrap_or_else(|| "<no-key>".to_string());
        HadReadError::Fatal(format!("HAD missing required {family} key: {key}"))
    })
}

fn read_optional<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
) -> Result<Option<T>, HadReadError> {
    let Some(key) = crate::nav_kv_key_for_query(&query) else {
        return Ok(None);
    };
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Ok(None),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
}

fn nav_ref_position(
    store: &NavKvStore,
    nav_ref: &NavRef,
    procedure_airport_id: Option<&str>,
) -> Result<LatLon, HadReadError> {
    if let NavRef::LatLon(position) = nav_ref {
        return Ok(*position);
    }
    read_required(
        store,
        NavKvQuery::NavRefPosition {
            nav_ref: nav_ref.clone(),
            procedure_airport_id: procedure_airport_id.map(str::to_string),
        },
        "navref position",
    )
}

fn nav_symbol_feature(
    store: &NavKvStore,
    nav_ref: &NavRef,
) -> Result<Option<NavSymbolFeature>, HadReadError> {
    read_optional(
        store,
        NavKvQuery::NavRefSymbol {
            nav_ref: nav_ref.clone(),
        },
    )
}

fn flight_plan_ui_state(
    store: &NavKvStore,
    plan: FlightPlan,
) -> Result<FlightPlanUiState, HadReadError> {
    let plan = crate::build_flight_plan(plan)?;
    let mut ui_state = crate::project_ui_state(&plan);
    let route = project_flight_plan_route(store, &plan)?;
    for row in &mut ui_state.display_rows {
        row.symbol_feature = match &row.nav_ref {
            Some(nav_ref) => nav_symbol_feature(store, nav_ref)?,
            None => None,
        };
        if let Some(leg_index) = row.leg_index {
            if let Some(segment) = route.get(leg_index) {
                row.distance_nm = Some(segment.distance_nm);
                row.course_deg = Some(segment.course_deg);
            }
        }
        if row
            .actions
            .iter()
            .any(|action| action.id == FlightPlanRowActionId::ShowPlate)
        {
            let match_rows = match (&row.chart_airport_id, &row.procedure_id) {
                (Some(airport_id), Some(procedure_id)) => read_optional::<Vec<CifpTppMatchRow>>(
                    store,
                    NavKvQuery::PlateCifpMatch {
                        airport_id: airport_id.clone(),
                        cifp_id: procedure_id.clone(),
                    },
                )?,
                _ => None,
            };
            let plate_match = match_rows.and_then(describe_show_plate_for_procedure);
            row.show_plate_target_id = plate_match.as_ref().map(|matched| matched.plate_id.clone());
            for action in &mut row.actions {
                if action.id == FlightPlanRowActionId::ShowPlate {
                    action.enabled = plate_match.is_some();
                }
            }
        }
    }
    Ok(ui_state)
}

fn project_flight_plan_route(
    store: &NavKvStore,
    plan: &FlightPlan,
) -> Result<Vec<FlightPlanRouteSegment>, HadReadError> {
    let plan = crate::build_flight_plan(plan.clone())?;
    let ui_state = crate::project_ui_state(&plan);
    plan.resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| {
            let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
                (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
            });
            let from = nav_ref_position(store, &leg.from, procedure_airport_id)?;
            let to = nav_ref_position(store, &leg.to, procedure_airport_id)?;
            Ok(FlightPlanRouteSegment {
                id: leg.id.clone(),
                from,
                to,
                path: great_circle_display_path(from, to),
                distance_nm: flight_leg_distance_nm(from, to),
                course_deg: flight_leg_course_deg(from, to),
                status: route_status_for_leg(&ui_state, leg_index),
            })
        })
        .collect()
}

fn route_status_for_leg(
    ui_state: &FlightPlanUiState,
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
    FlightPlanRouteSegmentStatus::Remaining
}

fn component_insert_anchor(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
) -> Result<NavRef, HadReadError> {
    let plan = plan.clone().normalized();
    let component = plan.route_components.get(component_index).ok_or_else(|| {
        HadReadError::Fatal(format!("component index out of bounds: {component_index}"))
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
    waypoint
        .ok_or_else(|| HadReadError::Fatal("selected component has no waypoint anchor".to_string()))
}

fn suggest_waypoint_identifiers(
    store: &NavKvStore,
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let prefix = prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let candidates = read_required::<Vec<WaypointIdentifierRecord>>(
        store,
        NavKvQuery::WaypointPrefix {
            prefix: prefix.clone(),
        },
        "waypoint prefix",
    )?;
    let anchor = component_insert_anchor(plan, component_index, before)?;
    let anchor_position = nav_ref_position(store, &anchor, None)?;
    let mut suggestions = candidates
        .into_iter()
        .filter(|candidate| {
            candidate
                .identifier
                .trim()
                .to_ascii_uppercase()
                .starts_with(&prefix)
        })
        .map(|candidate| WaypointIdentifierSuggestion {
            identifier: candidate.identifier,
            nav_ref: candidate.nav_ref,
            kind: candidate.kind.clone(),
            display_name: waypoint_identifier_display_name(
                &candidate.kind,
                &candidate.city,
                &candidate.state,
                &candidate.facility_name,
            ),
            distance_from_anchor_nm: flight_leg_distance_nm(anchor_position, candidate.position),
        })
        .collect::<Vec<_>>();
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
    let facility_name = facility_name.trim();
    let location = [city.trim(), state.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !facility_name.is_empty() && !location.is_empty() {
        format!("{facility_name}\n{location}")
    } else if !facility_name.is_empty() {
        facility_name.to_string()
    } else if !location.is_empty() {
        location
    } else {
        kind.to_string()
    }
}

fn nav_ref_kind_order(nav_ref: &NavRef) -> usize {
    match nav_ref {
        NavRef::Airport(_) => 0,
        NavRef::Navaid(_) => 1,
        NavRef::Fix(_) => 2,
        NavRef::LatLon(_) => 3,
    }
}

fn suggest_airways_near_anchor(
    store: &NavKvStore,
    anchor: &NavRef,
    limit: usize,
) -> Result<Vec<AirwaySuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let anchor_position = nav_ref_position(store, anchor, None)?;
    let mut points = Vec::new();
    for radius_nm in [25.0, 50.0, 100.0, 200.0, 400.0] {
        for (lat_tile, lon_tile) in airway_spatial_tiles(anchor_position, radius_nm) {
            if let Some(tile_points) = read_optional::<Vec<AirwaySpatialPoint>>(
                store,
                NavKvQuery::AirwaySpatial { lat_tile, lon_tile },
            )? {
                points.extend(tile_points);
            }
        }
        let mut suggestions = suggestions_from_airway_points(anchor_position, &points, limit);
        if suggestions.len() >= limit || radius_nm == 400.0 {
            suggestions.truncate(limit);
            return Ok(suggestions);
        }
    }
    Ok(Vec::new())
}

fn airway_spatial_tiles(anchor: LatLon, radius_nm: f64) -> Vec<(i32, i32)> {
    let lat_delta = radius_nm / 60.0;
    let lon_delta = radius_nm / (60.0 * anchor.lat.to_radians().cos().abs().max(0.1));
    let min_lat = (anchor.lat - lat_delta).floor() as i32;
    let max_lat = (anchor.lat + lat_delta).floor() as i32;
    let min_lon = (anchor.lon - lon_delta).floor() as i32;
    let max_lon = (anchor.lon + lon_delta).floor() as i32;
    let mut tiles = Vec::new();
    for lat_tile in min_lat..=max_lat {
        for lon_tile in min_lon..=max_lon {
            tiles.push((lat_tile, lon_tile));
        }
    }
    tiles
}

fn suggestions_from_airway_points(
    anchor_position: LatLon,
    points: &[AirwaySpatialPoint],
    limit: usize,
) -> Vec<AirwaySuggestion> {
    let mut seen = HashMap::<String, AirwaySuggestion>::new();
    for point in points {
        let distance_from_anchor_nm = flight_leg_distance_nm(anchor_position, point.position);
        let suggestion = AirwaySuggestion {
            airway_name: point.airway_name.clone(),
            nearest_branch_key: Some(point.branch_key.clone()),
            nearest_nav_ref: point.nav_ref.clone(),
            nearest_sequence: point.sequence,
            distance_from_anchor_nm,
        };
        match seen.get(&point.airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(point.airway_name.clone(), suggestion);
            }
        }
    }
    let mut suggestions = seen.into_values().collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
    });
    suggestions.truncate(limit);
    suggestions
}

fn prepare_airway_presentation_for_anchors(
    store: &NavKvStore,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<AirwayPresentationPlan, HadReadError> {
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: airway_name.to_string(),
        },
        "airway branches",
    )?;
    let origin_position = nav_ref_position(store, origin_anchor, None)?;
    let destination_position = destination_anchor
        .map(|anchor| nav_ref_position(store, anchor, None))
        .transpose()?;
    prepare_airway_presentation(airway_name, branches, origin_position, destination_position)
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct MaterializedAirwayResponse {
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    #[serde(rename = "resolvedLegs")]
    resolved_legs: Vec<ResolvedLeg>,
}

fn materialize_airway_selection(
    store: &NavKvStore,
    start_component_index: usize,
    entry: AirwayEntryCandidate,
    exit: AirwayExitCandidate,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<MaterializedAirwayResponse, HadReadError> {
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: entry.airway_name.clone(),
        },
        "airway branches",
    )?;
    let origin_position = nav_ref_position(store, origin_anchor, None)?;
    let destination_position = destination_anchor
        .map(|anchor| nav_ref_position(store, anchor, None))
        .transpose()?;
    let (airway, resolved_legs) =
        materialize_airway_from_branches(start_component_index, &entry, &exit, &branches)?;
    let entry_position = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .and_then(|branch| branch.points.get(entry.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| {
            HadReadError::Fatal("selected airway entry point is not on branch".to_string())
        })?;
    let exit_position = branches
        .iter()
        .find(|branch| branch.branch_key == exit.branch_key)
        .and_then(|branch| branch.points.get(exit.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| {
            HadReadError::Fatal("selected airway exit point is not on branch".to_string())
        })?;
    let origin_distance_nm = flight_leg_distance_nm(origin_position, entry_position);
    let destination_distance_nm = destination_position
        .map(|position| flight_leg_distance_nm(position, exit_position))
        .unwrap_or(0.0);
    Ok(MaterializedAirwayResponse {
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
    })
}

fn materialize_airway_presentation_selection(
    store: &NavKvStore,
    start_component_index: usize,
    presentation: AirwayPresentationPlan,
    entry_index: usize,
    exit_index: usize,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<MaterializedAirwayResponse, HadReadError> {
    if entry_index >= presentation.points.len() {
        return Err(HadReadError::Fatal(format!(
            "airway presentation entry index {entry_index} is out of bounds"
        )));
    }
    if exit_index >= presentation.points.len() {
        return Err(HadReadError::Fatal(format!(
            "airway presentation exit index {exit_index} is out of bounds"
        )));
    }
    if entry_index == exit_index {
        return Err(HadReadError::Fatal(
            "airway presentation exit cannot be the entry point".to_string(),
        ));
    }
    let entry = airway_entry_candidate_from_presentation(&presentation, entry_index);
    let exit = airway_exit_candidate_from_presentation(&presentation, entry_index, exit_index);
    materialize_airway_selection(
        store,
        start_component_index,
        entry,
        exit,
        origin_anchor,
        destination_anchor,
    )
}

fn airway_entry_candidate_from_presentation(
    presentation: &AirwayPresentationPlan,
    point_index: usize,
) -> AirwayEntryCandidate {
    let point = &presentation.points[point_index];
    AirwayEntryCandidate {
        airway_name: presentation.airway_name.clone(),
        branch_key: presentation.branch_key.clone(),
        branch_point_index: point.branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        distance_from_anchor_nm: 0.0,
        previous_nav_ref: point_index
            .checked_sub(1)
            .and_then(|index| presentation.points.get(index))
            .map(|point| point.nav_ref.clone()),
        next_nav_ref: presentation
            .points
            .get(point_index + 1)
            .map(|point| point.nav_ref.clone()),
    }
}

fn airway_exit_candidate_from_presentation(
    presentation: &AirwayPresentationPlan,
    entry_index: usize,
    point_index: usize,
) -> AirwayExitCandidate {
    let point = &presentation.points[point_index];
    AirwayExitCandidate {
        airway_name: presentation.airway_name.clone(),
        branch_key: presentation.branch_key.clone(),
        branch_point_index: point.branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        leg_offset_from_entry: point_index as isize - entry_index as isize,
        is_entry: point_index == entry_index,
        distance_from_target_nm: None,
    }
}

fn materialize_airway_from_branches(
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    branches: &[AirwayBranch],
) -> Result<(AirwaySegment, Vec<ResolvedLeg>), HadReadError> {
    if entry.airway_name != exit.airway_name || entry.branch_key != exit.branch_key {
        return Err(HadReadError::Fatal(format!(
            "entry airway {} branch {} does not match exit airway {} branch {}",
            entry.airway_name, entry.branch_key, exit.airway_name, exit.branch_key
        )));
    }
    let branch = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .ok_or_else(|| {
            HadReadError::Fatal(format!(
                "unknown airway branch {} {}",
                entry.airway_name, entry.branch_key
            ))
        })?;
    let entry_point = branch.points.get(entry.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "entry index {} is out of bounds for airway {} branch {}",
            entry.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    let exit_point = branch.points.get(exit.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "exit index {} is out of bounds for airway {} branch {}",
            exit.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    if entry.branch_point_index == exit.branch_point_index {
        return Err(HadReadError::Fatal(
            "airway entry and exit cannot be the same point".to_string(),
        ));
    }
    let slice = if entry.branch_point_index < exit.branch_point_index {
        &branch.points[entry.branch_point_index..=exit.branch_point_index]
    } else {
        &branch.points[exit.branch_point_index..=entry.branch_point_index]
    };
    let traversed = if entry.branch_point_index < exit.branch_point_index {
        slice.to_vec()
    } else {
        slice.iter().rev().cloned().collect::<Vec<_>>()
    };
    let resolved_legs = traversed
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!("airway-{}-{index}", branch.branch_key),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: ResolvedLegSource::RouteComponent { component_index },
            procedure_provenance: None,
        })
        .collect::<Vec<_>>();
    Ok((
        AirwaySegment {
            name: branch.display_name.clone(),
            branch_key: Some(branch.branch_key.clone()),
            entry: entry_point.nav_ref.clone(),
            exit: exit_point.nav_ref.clone(),
        },
        resolved_legs,
    ))
}

fn describe_procedure_options(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> Result<ProcedureOptions, HadReadError> {
    let rows = read_required::<Vec<ProcedureDistinctRow>>(
        store,
        NavKvQuery::ProcedureDistinctRows {
            airport_id: airport_id.to_string(),
            procedure_id: procedure_id.to_string(),
        },
        "procedure distinct rows",
    )?;
    describe_procedure_options_from_rows(airport_id, procedure_id, kind, rows).map_err(Into::into)
}

fn materialize_procedure(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
    component_index: usize,
) -> Result<MaterializedProcedure, HadReadError> {
    let rows = read_required::<Vec<ProcedureDistinctRow>>(
        store,
        NavKvQuery::ProcedureDistinctRows {
            airport_id: airport_id.to_string(),
            procedure_id: procedure_id.to_string(),
        },
        "procedure distinct rows",
    )?;
    let legs = read_required::<Vec<ProcedureLegMaterializationRecord>>(
        store,
        NavKvQuery::ProcedureMaterializationRows {
            airport_id: airport_id.to_string(),
            procedure_id: procedure_id.to_string(),
        },
        "procedure materialization rows",
    )?;
    materialize_procedure_from_records(
        airport_id,
        procedure_id,
        kind,
        runway_transition.map(str::to_string),
        enroute_transition.map(str::to_string),
        component_index,
        rows,
        legs,
    )
    .map_err(Into::into)
}

fn describe_plate_loads(
    store: &NavKvStore,
    plan: &FlightPlan,
    plate_id: &str,
) -> Result<Vec<ProcedureLoadOption>, HadReadError> {
    let Some(rows) = read_optional::<Vec<CifpTppMatchRow>>(
        store,
        NavKvQuery::PlateProcedureCandidates {
            plate_id: plate_id.to_string(),
        },
    )?
    else {
        return Ok(Vec::new());
    };
    let mut grouped = HashMap::<String, Vec<CifpTppMatchRow>>::new();
    for row in rows {
        grouped
            .entry(format!("{}:{}", row.airport_id, row.cifp_id))
            .or_default()
            .push(row);
    }
    let mut candidates = Vec::new();
    for match_rows in grouped.into_values() {
        let Some(preferred) = crate::select_preferred_cifp_tpp_match(match_rows.clone()) else {
            continue;
        };
        let distinct_rows = read_required::<Vec<ProcedureDistinctRow>>(
            store,
            NavKvQuery::ProcedureDistinctRows {
                airport_id: preferred.airport_id.clone(),
                procedure_id: preferred.cifp_id.clone(),
            },
            "procedure distinct rows",
        )?;
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
    describe_plate_procedure_load_options(plan, candidates).map_err(Into::into)
}

impl From<serde_json::Error> for HadReadError {
    fn from(err: serde_json::Error) -> Self {
        Self::Fatal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NavKvRoot;

    #[test]
    fn operation_reports_page_faults_instead_of_exposing_query_keys() {
        let (root, _pages) = fixture(&[("chart/catalog", br#"{"charts":[]}"#.as_slice())], 4);
        let store = NavKvStore::new(root);

        assert_eq!(
            run_had_operation(&store, HadOperation::ChartCatalog).unwrap(),
            HadOperationOutcome::NeedPages {
                pages: vec![0, 1, 2, 3]
            }
        );
    }

    #[test]
    fn operation_decodes_values_after_platform_supplies_pages() {
        let (root, pages) = fixture(&[("chart/catalog", br#"{"charts":[]}"#.as_slice())], 4);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        assert_eq!(
            run_had_operation(&store, HadOperation::ChartCatalog).unwrap(),
            HadOperationOutcome::Complete {
                result: serde_json::json!({"charts":[]})
            }
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
        let mut table = Vec::<(u32, u32)>::new();
        for (key, value) in entries {
            table.push((key_bytes.len() as u32, value_offset));
            key_bytes.extend_from_slice(key.as_bytes());
            value_offset += value.len() as u32;
        }
        table.push((key_bytes.len() as u32, value_offset));

        let header_len = 48usize;
        let entry_len = 8usize;
        let mut root = vec![0; header_len];
        root[..16].copy_from_slice(b"AEROBAGNAVKV0001");
        write_u32(&mut root, 16, 1);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, header_len as u32);
        write_u32(&mut root, 32, (header_len + table.len() * entry_len) as u32);
        write_u32(&mut root, 36, key_bytes.len() as u32);
        write_u32(&mut root, 40, value_offset);
        for (key_offset, value_offset) in table {
            root.extend_from_slice(&key_offset.to_le_bytes());
            root.extend_from_slice(&value_offset.to_le_bytes());
        }
        root.extend_from_slice(&key_bytes);
        root
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
