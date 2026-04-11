use serde::{Deserialize, Serialize};

pub mod catalog;
pub mod chart_page;
pub mod content;
pub mod errors;
pub mod geometry;
pub mod ids;
#[cfg(not(target_arch = "wasm32"))]
pub mod navdb;
pub mod navdb_types;
pub mod planning;
pub mod situation;
pub mod session;
pub mod state;

pub use catalog::{
    CatalogBundle, CatalogFamily, CatalogHandle, CatalogPackage, CatalogRegion, ChartCoverage,
    ChartRecord, PlateRecord, SupplementRecord,
};
pub use chart_page::{
    build_chart_catalog, derive_chart_page, derive_chart_page_from_catalog,
    derive_chart_page_state, derive_chart_page_state_from_catalog, DerivedChartAirport,
    DerivedChartAsset, DerivedChartCatalog, DerivedChartPage, DerivedChartPageState,
    ResourceAirportResources, ResourceCsup, ResourceIndexChartPageInput, ResourcePlate,
};
pub use content::{
    AvailabilityDetail, CachedPlate, CachedTileset, ContentAvailability, ContentInventory,
    ContentPolicy, ContentReport, ContentReportItem, ContentRequirement, InstalledPackage,
};
pub use errors::{AppError, AppErrorKind, AppResult};
pub use geometry::{GeoBounds, GeometryBundle, LatLon, MapViewport, PolygonRecord};
pub use ids::{AirportId, ChartFamilyId, ChartId, PackageId, PlateId, RegionId};
pub use navdb_types::{
    AirwayAutoSelection, AirwayBranch, AirwayEntryCandidate, AirwayExitCandidate,
    AirwayExitSelection, AirwayFixPoint, AirwayPoint, AirwaySuggestion, MaterializedProcedure,
    ProcedureLegRecord, ProcedureOptions, ProcedureSpecChoice, ProcedureSummary,
    ProcedureVariantKey,
};
#[cfg(not(target_arch = "wasm32"))]
pub use navdb::{
    choose_best_airway_plan, describe_procedure_options, list_airway_entry_candidates,
    list_airway_exit_candidates, list_procedures, load_airway_branches, load_airway_points,
    load_procedure_concretized_items, load_procedure_legs, load_resolved_procedure_legs,
    materialize_airway_selection, materialize_procedure_selection, resolve_airway_segment,
    resolve_airway_segment_by_index, resolve_nav_ref_position, select_airway_branch,
    suggest_airways_near,
};
pub use planning::{
    activate_direct_to, activate_direct_to_leg, activate_leg, activate_next_leg,
    active_guidance_leg, change_airway_entry, change_airway_exit,
    change_procedure_enroute_transition, change_procedure_runway_transition, delete_component,
    delete_waypoint_component,
    flatten_component_to_waypoints, insert_airway_between_waypoints,
    insert_procedure_between_waypoints, interpret_path_termination, project_ui_state,
    replace_airway_component, replace_procedure_component, sequence_active_leg, suspend_sequencing,
    unsuspend_sequencing, AirwaySegment, ConcretizedNavItem, DirectToState, FlightPlan,
    FlightPlanUiState, GuidanceState, GuidanceUiView, NavRef, PathTermination, PlanLeg,
    ProcedureDiscontinuity, ProcedureKind, ProcedureLegProvenance, ProcedureSegment,
    ProcedureSegmentRole, ResolvedLeg, ResolvedLegSource, ResolvedLegUiView, RouteComponent,
    RouteComponentUiView, RouteComponentViewKind, SequencingMode, DirectToUiView,
};
pub use situation::{Situation, SituationPosition};
pub use session::{
    create_ui_session, destroy_session, get_session_snapshot, move_waypoint_in_session,
    remove_leg_in_session, restore_chart_page_state_in_session, select_airport_in_session,
    select_chart_in_session, set_situation_in_session,
    UiChartPageState, UiSessionInitResult, UiSessionSnapshot,
};
pub use state::{project_app_ui_state, AppEvent, AppState, AppUiState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPlanMutation {
    pub plan: FlightPlan,
    pub component_index: usize,
    pub selection: AirwayAutoSelection,
    pub airway: AirwaySegment,
    pub resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedurePlanMutation {
    pub plan: FlightPlan,
    pub component_index: usize,
    pub procedure: ProcedureSegment,
    pub concretized_items: Vec<ConcretizedNavItem>,
    pub resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanUiMutation {
    pub plan: FlightPlan,
    pub ui_state: FlightPlanUiState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPlanUiMutation {
    pub mutation: AirwayPlanMutation,
    pub ui_state: FlightPlanUiState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedurePlanUiMutation {
    pub mutation: ProcedurePlanMutation,
    pub ui_state: FlightPlanUiState,
}

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

pub fn load_catalog(catalog_json: &str) -> AppResult<CatalogHandle> {
    let bundle: CatalogBundle = serde_json::from_str(catalog_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse catalog json: {err}"),
    })?;
    Ok(CatalogHandle { bundle })
}

pub fn load_geometry(geometry_json: &str) -> AppResult<GeometryBundle> {
    serde_json::from_str(geometry_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse geometry json: {err}"),
    })
}

pub fn load_resource_index_chart_page_input(
    resource_index_json: &str,
) -> AppResult<Arc<ResourceIndexChartPageInput>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<ResourceIndexChartPageInput>>>> = OnceLock::new();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    resource_index_json.hash(&mut hasher);
    let key = hasher.finish();

    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("resource index cache poisoned")
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let parsed: ResourceIndexChartPageInput =
        serde_json::from_str(resource_index_json).map_err(|err| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("failed to parse resource index json: {err}"),
        })?;
    let parsed = Arc::new(parsed);
    cache
        .lock()
        .expect("resource index cache poisoned")
        .insert(key, parsed.clone());
    Ok(parsed)
}

pub fn chart_for_position(
    catalog: &CatalogHandle,
    geometry: &GeometryBundle,
    family: ChartFamilyId,
    lat: f64,
    lon: f64,
) -> AppResult<Option<ChartRecord>> {
    let point = LatLon { lat, lon };
    for chart in &catalog.bundle.charts {
        if chart.family_id != family {
            continue;
        }
        if geometry.chart_contains(chart, point) {
            return Ok(Some(chart.clone()));
        }
    }
    Ok(None)
}

pub fn build_flight_plan(plan: FlightPlan) -> AppResult<FlightPlan> {
    if plan.legs.is_empty() && plan.route_components.is_empty() && plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }

    let plan = plan.normalized();

    if plan.legs.is_empty() && plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg".to_string(),
        });
    }

    Ok(plan)
}

pub fn remove_flight_plan_leg(plan: &FlightPlan, index: usize) -> AppResult<FlightPlan> {
    if index >= plan.legs.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight plan leg index out of range: {index}"),
        });
    }

    let mut next = plan.clone();
    next.legs.remove(index);

    if next.legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }

    next.departure = next
        .legs
        .first()
        .and_then(|leg| leg.from.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.destination = next
        .legs
        .last()
        .and_then(|leg| leg.to.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.updated_at_epoch_ms += 1;
    next.version += 1;
    Ok(next)
}

pub fn move_flight_plan_waypoint(
    plan: &FlightPlan,
    waypoint_index: usize,
    delta: isize,
) -> AppResult<FlightPlan> {
    if delta == 0 {
        return Ok(plan.clone());
    }

    if plan.legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }

    let mut waypoints = Vec::with_capacity(plan.legs.len() + 1);
    waypoints.push(
        plan.legs
            .first()
            .map(|leg| leg.from.clone())
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "flight plan must contain at least one leg".to_string(),
            })?,
    );
    waypoints.extend(plan.legs.iter().map(|leg| leg.to.clone()));

    if waypoint_index >= waypoints.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight plan waypoint index out of range: {waypoint_index}"),
        });
    }

    let next_index = waypoint_index as isize + delta;
    if next_index < 0 || next_index >= waypoints.len() as isize {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "flight plan waypoint move out of range: {waypoint_index} -> {next_index}"
            ),
        });
    }

    waypoints.swap(waypoint_index, next_index as usize);

    let legs = waypoints
        .windows(2)
        .map(|pair| PlanLeg {
            from: pair[0].clone(),
            to: pair[1].clone(),
            airway: None,
        })
        .collect::<Vec<_>>();

    let mut next = plan.clone();
    next.legs = legs;
    next.departure = waypoints
        .first()
        .and_then(|waypoint| waypoint.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.destination = waypoints
        .last()
        .and_then(|waypoint| waypoint.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.updated_at_epoch_ms += 1;
    next.version += 1;
    Ok(next)
}

pub fn build_flight_plan_ui(plan: FlightPlan) -> AppResult<FlightPlanUiState> {
    let plan = build_flight_plan(plan)?;
    Ok(project_ui_state(&plan))
}

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_anchors(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
) -> AppResult<AirwayPlanMutation> {
    let selection = choose_best_airway_plan(db_path, airway_name, origin_anchor, destination_anchor)?;
    let entry = selection.entry.clone();
    let exit = selection.exit.clone();
    insert_airway_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        &entry,
        &exit,
        Some(selection),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_anchors_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = insert_airway_from_anchors(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        airway_name,
        origin_anchor,
        destination_anchor,
    )?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    selection: Option<AirwayAutoSelection>,
) -> AppResult<AirwayPlanMutation> {
    let (airway, legs) = materialize_airway_selection(db_path, entry, exit, 0)?;
    let mutation_legs = with_component_index_source(&legs, start_component_index + 1);
    let inserted = insert_airway_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        airway,
        legs,
    )?;
    let component_index = start_component_index + 1;
    Ok(AirwayPlanMutation {
        airway: component_airway(&inserted, component_index)?,
        resolved_legs: mutation_legs,
        plan: inserted,
        component_index,
        selection: selection.unwrap_or_else(|| AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry: entry.clone(),
            exit: exit.clone(),
            origin_distance_nm: entry.distance_from_anchor_nm,
            destination_distance_nm: exit.distance_from_target_nm.unwrap_or(0.0),
            total_anchor_distance_nm: entry.distance_from_anchor_nm
                + exit.distance_from_target_nm.unwrap_or(0.0),
        }),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    selection: Option<AirwayAutoSelection>,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = insert_airway_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        entry,
        exit,
        selection,
    )?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_airway_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
) -> AppResult<AirwayPlanMutation> {
    let (airway, legs) = materialize_airway_selection(db_path, entry, exit, component_index)?;
    let mutation_legs = with_component_index_source(&legs, component_index);
    let replaced = replace_airway_component(plan, component_index, airway, legs)?;
    Ok(AirwayPlanMutation {
        airway: component_airway(&replaced, component_index)?,
        resolved_legs: mutation_legs,
        plan: replaced,
        component_index,
        selection: AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry: entry.clone(),
            exit: exit.clone(),
            origin_distance_nm: entry.distance_from_anchor_nm,
            destination_distance_nm: exit.distance_from_target_nm.unwrap_or(0.0),
            total_anchor_distance_nm: entry.distance_from_anchor_nm
                + exit.distance_from_target_nm.unwrap_or(0.0),
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_airway_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = replace_airway_from_selection(db_path, plan, component_index, entry, exit)?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_procedure_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanMutation> {
    let built = materialize_procedure_selection(
        db_path,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        0,
    )?;
    let inserted = insert_procedure_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    let component_index = start_component_index + 1;

    Ok(ProcedurePlanMutation {
        procedure: component_procedure(&inserted, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: inserted,
        component_index,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_procedure_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanUiMutation> {
    let mutation = insert_procedure_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
    )?;
    Ok(project_procedure_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_procedure_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanMutation> {
    let built = materialize_procedure_selection(
        db_path,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        component_index,
    )?;
    let replaced = replace_procedure_component(
        plan,
        component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;

    Ok(ProcedurePlanMutation {
        procedure: component_procedure(&replaced, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: replaced,
        component_index,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_procedure_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanUiMutation> {
    let mutation = replace_procedure_from_selection(
        db_path,
        plan,
        component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
    )?;
    Ok(project_procedure_mutation(mutation))
}

pub fn insert_airway_materialized_ui(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation_legs = with_component_index_source(&resolved_legs, start_component_index + 1);
    let inserted = insert_airway_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        airway,
        resolved_legs,
    )?;
    let component_index = start_component_index + 1;
    Ok(project_airway_mutation(AirwayPlanMutation {
        airway: component_airway(&inserted, component_index)?,
        resolved_legs: mutation_legs,
        plan: inserted,
        component_index,
        selection,
    }))
}

pub fn replace_airway_materialized_ui(
    plan: &FlightPlan,
    component_index: usize,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation_legs = with_component_index_source(&resolved_legs, component_index);
    let replaced = replace_airway_component(plan, component_index, airway, resolved_legs)?;
    Ok(project_airway_mutation(AirwayPlanMutation {
        airway: component_airway(&replaced, component_index)?,
        resolved_legs: mutation_legs,
        plan: replaced,
        component_index,
        selection,
    }))
}

pub fn insert_procedure_materialized_ui(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<ProcedurePlanUiMutation> {
    let inserted = insert_procedure_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    let component_index = start_component_index + 1;
    Ok(project_procedure_mutation(ProcedurePlanMutation {
        procedure: component_procedure(&inserted, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: inserted,
        component_index,
    }))
}

pub fn replace_procedure_materialized_ui(
    plan: &FlightPlan,
    component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<ProcedurePlanUiMutation> {
    let replaced = replace_procedure_component(
        plan,
        component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    Ok(project_procedure_mutation(ProcedurePlanMutation {
        procedure: component_procedure(&replaced, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: replaced,
        component_index,
    }))
}

pub fn sequence_active_leg_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = sequence_active_leg(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_leg_ui(plan: &FlightPlan, leg_index: usize) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_leg(plan, leg_index)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_next_leg_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_next_leg(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn suspend_sequencing_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = suspend_sequencing(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn unsuspend_sequencing_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = unsuspend_sequencing(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_direct_to_ui(
    plan: &FlightPlan,
    from_position: LatLon,
    target: NavRef,
) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_direct_to(plan, from_position, target)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_direct_to_leg_ui(
    plan: &FlightPlan,
    from_position: LatLon,
    target_leg_id: &str,
) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_direct_to_leg(plan, from_position, target_leg_id)?;
    Ok(project_plan_mutation(plan))
}

fn with_component_index_source(legs: &[ResolvedLeg], component_index: usize) -> Vec<ResolvedLeg> {
    legs.iter()
        .cloned()
        .map(|leg| ResolvedLeg {
            source: ResolvedLegSource::RouteComponent { component_index },
            ..leg
        })
        .collect()
}

fn project_plan_mutation(plan: FlightPlan) -> FlightPlanUiMutation {
    let ui_state = project_ui_state(&plan);
    FlightPlanUiMutation { plan, ui_state }
}

fn project_airway_mutation(mutation: AirwayPlanMutation) -> AirwayPlanUiMutation {
    let ui_state = project_ui_state(&mutation.plan);
    AirwayPlanUiMutation { mutation, ui_state }
}

fn project_procedure_mutation(mutation: ProcedurePlanMutation) -> ProcedurePlanUiMutation {
    let ui_state = project_ui_state(&mutation.plan);
    ProcedurePlanUiMutation { mutation, ui_state }
}

fn component_airway(plan: &FlightPlan, component_index: usize) -> AppResult<AirwaySegment> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Airway { airway }) => Ok(airway.clone()),
        Some(_) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component at index {component_index} is not an airway"),
        }),
        None => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

fn component_procedure(plan: &FlightPlan, component_index: usize) -> AppResult<ProcedureSegment> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Procedure { procedure }) => Ok(procedure.clone()),
        Some(_) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component at index {component_index} is not a procedure"),
        }),
        None => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

pub fn plan_content_requirements(
    catalog: &CatalogHandle,
    plan: &FlightPlan,
) -> AppResult<Vec<ContentRequirement>> {
    let mut package_ids = Vec::new();

    for airport_code in plan_airport_codes(plan) {
        for plate in &catalog.bundle.plates {
            if plate.airport_id.0.eq_ignore_ascii_case(airport_code) {
                if let Some(pkg) = catalog
                    .bundle
                    .packages
                    .iter()
                    .find(|pkg| pkg.region_id == plate.region_id)
                {
                    package_ids.push(pkg.id.clone());
                }
            }
        }
    }

    package_ids.sort();
    package_ids.dedup();

    Ok(vec![ContentRequirement {
        package_ids,
        chart_ids: Vec::new(),
        plate_ids: Vec::new(),
    }])
}

fn plan_airport_codes(plan: &FlightPlan) -> Vec<&str> {
    let mut codes = Vec::new();

    for leg in &plan.legs {
        if let Some(code) = leg.from.airport_code() {
            codes.push(code);
        }
        if let Some(code) = leg.to.airport_code() {
            codes.push(code);
        }
    }

    for component in &plan.route_components {
        match component {
            RouteComponent::Waypoint { waypoint } => {
                if let Some(code) = waypoint.airport_code() {
                    codes.push(code);
                }
            }
            RouteComponent::Airway { airway } => {
                if let Some(code) = airway.entry.airport_code() {
                    codes.push(code);
                }
                if let Some(code) = airway.exit.airport_code() {
                    codes.push(code);
                }
            }
            RouteComponent::Procedure { procedure } => {
                codes.push(procedure.airport_id.0.as_str());
            }
        }
    }

    codes.sort();
    codes.dedup();
    codes
}

pub fn resolve_content_status(
    requirements: &[ContentRequirement],
    inventory: &ContentInventory,
    policy: ContentPolicy,
) -> AppResult<ContentReport> {
    let mut items = Vec::new();

    for requirement in requirements {
        for package_id in &requirement.package_ids {
            let installed = inventory
                .installed_packages
                .iter()
                .any(|pkg| &pkg.package_id == package_id && pkg.integrity_ok);

            let availability = match (installed, policy) {
                (true, ContentPolicy::StreamAllowed) => ContentAvailability::LocalAndRemote,
                (true, _) => ContentAvailability::LocalOnly,
                (false, ContentPolicy::StreamAllowed) => ContentAvailability::RemoteOnly,
                (false, _) => ContentAvailability::Unavailable,
            };

            items.push(ContentReportItem {
                label: package_id.package_name(),
                availability: AvailabilityDetail {
                    availability,
                    cycle_current: true,
                    integrity_ok: installed,
                    cached: installed,
                    offline_usable: installed,
                },
            });
        }
    }

    let fully_satisfied = items.iter().all(|item| match policy {
        ContentPolicy::StreamAllowed => !matches!(
            item.availability.availability,
            ContentAvailability::Unavailable
        ),
        _ => matches!(
            item.availability.availability,
            ContentAvailability::LocalOnly | ContentAvailability::LocalAndRemote
        ),
    });

    Ok(ContentReport {
        fully_satisfied,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "cycle": "2026-04-16",
            "catalog_revision": "2026-04-05T22:00:00Z",
            "families": [
                {
                    "id": "sectional",
                    "display_name": "VFR Sectional Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512
                }
            ],
            "regions": [
                {
                    "id": "ne",
                    "display_name": "Northeast",
                    "sort_order": 0
                }
            ],
            "packages": [
                {
                    "id": {
                        "region": "ne",
                        "family": "sectional",
                        "cycle": "2026-04-16"
                    },
                    "package_name": "NE_SEC",
                    "family_id": "sectional",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "artifact_kind": "zip",
                    "relative_url": "/2026-04-16/NE_SEC.zip",
                    "manifest_name": "NE_SEC",
                    "size_bytes": null,
                    "checksum_sha256": null
                }
            ],
            "charts": [
                {
                    "id": {
                        "family": "sectional",
                        "name": "Boston",
                        "cycle": "2026-04-16"
                    },
                    "family_id": "sectional",
                    "name": "Boston",
                    "display_name": "Boston",
                    "cycle": "2026-04-16",
                    "region_ids": ["ne"],
                    "max_zoom": 10,
                    "tile_path_template": "tiles/{chart_index}/{z}/{x}/{y}",
                    "coverage": {
                        "kind": "polygon_ref",
                        "value": {
                            "polygon_id": "sectional:boston"
                        }
                    }
                }
            ],
            "plates": [
                {
                    "id": {
                        "airport_id": "KBOS",
                        "procedure_code": "IAP-ILS-RWY-04R",
                        "page": 1,
                        "cycle": "2026-04-16"
                    },
                    "airport_id": "KBOS",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "procedure_code": "IAP-ILS-RWY-04R",
                    "display_name": "ILS OR LOC RWY 04R",
                    "kind": "approach",
                    "georeferenced": true,
                    "page_count": 1,
                    "asset_base_path": "plates/KBOS/IAP-ILS-RWY-04R"
                }
            ],
            "supplements": []
        })
        .to_string()
    }

    fn sample_geometry_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "polygons": [
                {
                    "id": "sectional:boston",
                    "points": [
                        [-72.0, 43.0],
                        [-72.0, 41.0],
                        [-69.0, 41.0],
                        [-69.0, 43.0]
                    ]
                }
            ]
        })
        .to_string()
    }

    fn sample_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KBOS to KJFK".to_string(),
            legs: vec![PlanLeg {
                from: NavRef::Airport("KBOS".to_string()),
                to: NavRef::Airport("KJFK".to_string()),
                airway: None,
            }],
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn fixture_db_path() -> &'static Path {
        Path::new("/root/aerobag-three/aerobag/ui/android-app/app/src/main/assets/nav-db/main.db")
    }

    #[test]
    fn remove_flight_plan_leg_updates_endpoints_and_version() {
        let plan = FlightPlan {
            id: "plan-1".to_string(),
            name: "NW sample".to_string(),
            legs: vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Airport("KSEA".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Airport("KSEA".to_string()),
                    to: NavRef::Airport("KPAE".to_string()),
                    airway: None,
                },
            ],
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 10,
            version: 1,
        };

        let next = remove_flight_plan_leg(&plan, 0).unwrap();

        assert_eq!(next.legs.len(), 1);
        assert_eq!(next.departure, Some(AirportId("KSEA".to_string())));
        assert_eq!(next.destination, Some(AirportId("KPAE".to_string())));
        assert_eq!(next.updated_at_epoch_ms, 11);
        assert_eq!(next.version, 2);
    }

    #[test]
    fn loads_catalog_with_structured_ids() {
        let handle = load_catalog(&sample_catalog_json()).unwrap();
        assert_eq!(handle.bundle.schema_version, 1);
        assert_eq!(handle.bundle.families[0].id, ChartFamilyId::Sectional);
        assert_eq!(handle.bundle.regions[0].id, RegionId::Ne);
    }

    #[test]
    fn finds_chart_for_point_inside_polygon() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let geometry = load_geometry(&sample_geometry_json()).unwrap();
        let chart =
            chart_for_position(&catalog, &geometry, ChartFamilyId::Sectional, 42.0, -71.0)
                .unwrap();
        assert_eq!(chart.unwrap().name, "Boston");
    }

    #[test]
    fn does_not_find_chart_for_point_outside_polygon() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let geometry = load_geometry(&sample_geometry_json()).unwrap();
        let chart =
            chart_for_position(&catalog, &geometry, ChartFamilyId::Sectional, 35.0, -71.0)
                .unwrap();
        assert!(chart.is_none());
    }

    #[test]
    fn rejects_empty_flight_plan() {
        let result = build_flight_plan(FlightPlan {
            id: "plan-1".to_string(),
            name: "Empty".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        });
        assert_eq!(result.unwrap_err().kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn deduplicates_required_packages_across_matching_legs() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let requirements = plan_content_requirements(&catalog, &sample_plan()).unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].package_ids.len(), 1);
        assert_eq!(requirements[0].package_ids[0].region, RegionId::Ne);
    }

    #[test]
    fn normalizes_legacy_legs_into_route_components_and_resolved_legs() {
        let plan = build_flight_plan(sample_plan()).unwrap();

        assert_eq!(plan.route_components.len(), 2);
        assert_eq!(plan.resolved_legs.len(), 1);
        assert_eq!(plan.legs.len(), 1);
    }

    #[test]
    fn accepts_component_only_plan_and_backfills_legacy_legs() {
        let plan = build_flight_plan(FlightPlan {
            id: "component-only".to_string(),
            name: "Component only".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
            }),
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .unwrap();

        assert_eq!(plan.resolved_legs.len(), 1);
        assert_eq!(plan.legs.len(), 1);
        assert_eq!(plan.legs[0].from.airport_code(), Some("KBOS"));
        assert_eq!(plan.legs[0].to.airport_code(), Some("KJFK"));
    }

    #[test]
    fn inserts_airway_from_origin_and_destination_anchors() {
        let plan = FlightPlan {
            id: "airway-insert".to_string(),
            name: "Airway insert".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KHIO".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KHIO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = insert_airway_from_anchors(
            fixture_db_path(),
            &plan,
            0,
            1,
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();

        assert_eq!(mutation.component_index, 1);
        assert_eq!(mutation.selection.branch_key, "V2-A");
        assert_eq!(mutation.selection.entry.nav_ref, NavRef::Navaid("SEA".to_string()));
        assert_eq!(mutation.selection.exit.nav_ref, NavRef::Fix("VAMPS".to_string()));
        assert!(matches!(mutation.plan.route_components[1], RouteComponent::Airway { .. }));
        assert!(mutation.plan.guidance.is_none());
        assert_eq!(mutation.resolved_legs.first().unwrap().from, NavRef::Navaid("SEA".to_string()));
        assert_eq!(mutation.resolved_legs.last().unwrap().to, NavRef::Fix("VAMPS".to_string()));
    }

    #[test]
    fn inserts_procedure_from_selection_returns_mutated_plan_and_component_payload() {
        let plan = FlightPlan {
            id: "procedure-insert".to_string(),
            name: "Procedure insert".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ETX".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
            }),
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = insert_procedure_from_selection(
            fixture_db_path(),
            &plan,
            0,
            1,
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        assert_eq!(mutation.component_index, 1);
        assert!(matches!(
            mutation.plan.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(mutation.procedure.procedure_id, "I04R");
        assert!(!mutation.concretized_items.is_empty());
        assert!(!mutation.resolved_legs.is_empty());
    }

    #[test]
    fn inserts_airway_from_materialized_selection_returns_projected_mutation() {
        let plan = FlightPlan {
            id: "airway-materialized".to_string(),
            name: "Airway materialized".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let selection = choose_best_airway_plan(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();
        let (airway, resolved_legs) = materialize_airway_selection(
            fixture_db_path(),
            &selection.entry,
            &selection.exit,
            0,
        )
        .unwrap();

        let mutation = insert_airway_materialized_ui(
            &plan,
            0,
            1,
            selection,
            airway,
            resolved_legs,
        )
        .unwrap();

        assert_eq!(mutation.mutation.component_index, 1);
        assert!(matches!(
            mutation.mutation.plan.route_components[1],
            RouteComponent::Airway { .. }
        ));
        assert_eq!(mutation.ui_state.components[1].kind, RouteComponentViewKind::Airway);
    }

    #[test]
    fn inserts_procedure_from_materialized_selection_returns_projected_mutation() {
        let plan = FlightPlan {
            id: "procedure-materialized".to_string(),
            name: "Procedure materialized".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ETX".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
            }),
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let built = materialize_procedure_selection(
            fixture_db_path(),
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
            0,
        )
        .unwrap();

        let mutation = insert_procedure_materialized_ui(&plan, 0, 1, built).unwrap();

        assert_eq!(mutation.mutation.component_index, 1);
        assert!(matches!(
            mutation.mutation.plan.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(mutation.ui_state.components[1].kind, RouteComponentViewKind::Procedure);
    }

    #[test]
    fn replaces_procedure_from_selection_returns_updated_atomic_component_payload() {
        let inserted = insert_procedure_from_selection(
            fixture_db_path(),
            &FlightPlan {
                id: "procedure-replace".to_string(),
                name: "Procedure replace".to_string(),
                legs: Vec::new(),
                route_components: vec![
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Fix("ETX".to_string()),
                    },
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Airport("KBOS".to_string()),
                    },
                ],
                resolved_legs: Vec::new(),
                guidance: None,
                departure: None,
                destination: Some(AirportId("KBOS".to_string())),
                alternate: None,
                cruise_altitude_ft: None,
                notes: None,
                updated_at_epoch_ms: 0,
                version: 1,
            },
            0,
            1,
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        let replaced = replace_procedure_from_selection(
            fixture_db_path(),
            &inserted.plan,
            inserted.component_index,
            "KBOS",
            "R04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        assert_eq!(replaced.component_index, inserted.component_index);
        assert_eq!(replaced.procedure.procedure_id, "R04R");
        assert!(matches!(
            replaced.plan.route_components[replaced.component_index],
            RouteComponent::Procedure { .. }
        ));
        assert!(!replaced.resolved_legs.is_empty());
    }

    #[test]
    fn content_requirements_consider_airports_from_procedure_components() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let plan = build_flight_plan(FlightPlan {
            id: "proc".to_string(),
            name: "Procedure".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KBOS".to_string()),
                    procedure_id: "IAP-ILS-RWY-04R".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: Some("04R".to_string()),
                    enroute_transition: None,
                    terminal_discontinuity: None,
                },
            }],
            resolved_legs: vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("NOONY".to_string()),
                to: NavRef::Airport("KBOS".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: None,
            }],
            guidance: None,
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .unwrap();

        let requirements = plan_content_requirements(&catalog, &plan).unwrap();

        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].package_ids.len(), 1);
        assert_eq!(requirements[0].package_ids[0].package_name(), "NE_SEC");
    }

    #[test]
    fn stream_allowed_reports_remote_content_as_satisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::StreamAllowed,
        )
        .unwrap();

        assert!(report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::RemoteOnly
        );
    }

    #[test]
    fn offline_required_marks_missing_content_unsatisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::OfflineRequired,
        )
        .unwrap();

        assert!(!report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::Unavailable
        );
    }

    #[test]
    fn move_flight_plan_waypoint_rebuilds_waypoint_sequence() {
        let plan = FlightPlan {
            id: "plan-1".to_string(),
            name: "NW sample".to_string(),
            legs: vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    airway: Some("V27".to_string()),
                },
                PlanLeg {
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Navaid("PAE".to_string()),
                    airway: Some("V27".to_string()),
                },
                PlanLeg {
                    from: NavRef::Navaid("PAE".to_string()),
                    to: NavRef::Airport("KAWO".to_string()),
                    airway: None,
                },
            ],
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KAWO".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 10,
            version: 1,
        };

        let next = move_flight_plan_waypoint(&plan, 2, -1).unwrap();

        assert_eq!(
            next.legs,
            vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("PAE".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Navaid("PAE".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Airport("KAWO".to_string()),
                    airway: None,
                },
            ]
        );
        assert_eq!(next.departure, Some(AirportId("KRNT".to_string())));
        assert_eq!(next.destination, Some(AirportId("KAWO".to_string())));
        assert_eq!(next.updated_at_epoch_ms, 11);
        assert_eq!(next.version, 2);
    }
}
