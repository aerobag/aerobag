pub use app_core::*;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::jbyteArray;
use jni::sys::jstring;
use jni::JNIEnv;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

pub fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn remove_flight_plan_leg_json(plan_json: &str, index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::remove_flight_plan_leg(&plan, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn activate_leg_ui_json(plan_json: &str, leg_index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = project_plan_mutation(
        app_core::activate_leg(&plan, leg_index).map_err(|err| err.to_string())?,
    );
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn project_plan_mutation(plan: app_core::FlightPlan) -> app_core::FlightPlanUiMutation {
    let ui_state = app_core::project_ui_state(&plan);
    app_core::FlightPlanUiMutation { plan, ui_state }
}

pub fn activate_direct_to_leg_ui_json(
    plan_json: &str,
    lat: f64,
    lon: f64,
    target_leg_id: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation =
        app_core::activate_direct_to_leg_ui(&plan, app_core::LatLon { lat, lon }, target_leg_id)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn replace_flight_plan_state_json(state_json: &str, plan_json: &str) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::ReplaceFlightPlan(plan))
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

pub fn replace_flight_plan_ui_state_json(
    state_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::ReplaceFlightPlan(plan))
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

pub fn set_content_policy_state_json(
    state_json: &str,
    policy_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let policy: app_core::ContentPolicy =
        serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::SetContentPolicy(policy))
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

pub fn set_content_policy_ui_state_json(
    state_json: &str,
    policy_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let policy: app_core::ContentPolicy =
        serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::SetContentPolicy(policy))
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

pub fn refresh_content_state_json(
    state_json: &str,
    inventory_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let inventory: app_core::ContentInventory =
        serde_json::from_str(inventory_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::RefreshContent { inventory })
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

pub fn refresh_content_ui_state_json(
    state_json: &str,
    inventory_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let inventory: app_core::ContentInventory =
        serde_json::from_str(inventory_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(&state, app_core::AppEvent::RefreshContent { inventory })
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

pub fn prepare_airway_presentation_json(
    airway_name: &str,
    branches_json: &str,
    origin_position_json: &str,
    destination_position_json: &str,
) -> Result<String, String> {
    let branches: Vec<app_core::AirwayBranch> =
        serde_json::from_str(branches_json).map_err(|err| err.to_string())?;
    let origin_position: app_core::LatLon =
        serde_json::from_str(origin_position_json).map_err(|err| err.to_string())?;
    let destination_position: Option<app_core::LatLon> =
        serde_json::from_str(destination_position_json).map_err(|err| err.to_string())?;
    let presentation = app_core::prepare_airway_presentation(
        airway_name,
        branches,
        origin_position,
        destination_position,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&presentation).map_err(|err| err.to_string())
}

pub fn sort_airway_suggestions_for_ui_json(suggestions_json: &str) -> Result<String, String> {
    let suggestions: Vec<app_core::AirwaySuggestion> =
        serde_json::from_str(suggestions_json).map_err(|err| err.to_string())?;
    let sorted = app_core::sort_airway_suggestions_for_ui(suggestions);
    serde_json::to_string(&sorted).map_err(|err| err.to_string())
}

pub fn insert_airway_materialized_ui_json(
    plan_json: &str,
    start_component_index: usize,
    end_component_index_json: &str,
    selection_json: &str,
    airway_json: &str,
    resolved_legs_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let end_component_index: Option<usize> =
        serde_json::from_str(end_component_index_json).map_err(|err| err.to_string())?;
    let selection: app_core::AirwayAutoSelection =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let airway: app_core::AirwaySegment =
        serde_json::from_str(airway_json).map_err(|err| err.to_string())?;
    let resolved_legs: Vec<app_core::ResolvedLeg> =
        serde_json::from_str(resolved_legs_json).map_err(|err| err.to_string())?;
    let mutation = app_core::insert_airway_materialized_ui(
        &plan,
        start_component_index,
        end_component_index,
        selection,
        airway,
        resolved_legs,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn replace_airway_materialized_ui_json(
    plan_json: &str,
    component_index: usize,
    selection_json: &str,
    airway_json: &str,
    resolved_legs_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let selection: app_core::AirwayAutoSelection =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let airway: app_core::AirwaySegment =
        serde_json::from_str(airway_json).map_err(|err| err.to_string())?;
    let resolved_legs: Vec<app_core::ResolvedLeg> =
        serde_json::from_str(resolved_legs_json).map_err(|err| err.to_string())?;
    let mutation = app_core::replace_airway_materialized_ui(
        &plan,
        component_index,
        selection,
        airway,
        resolved_legs,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn insert_procedure_materialized_ui_json(
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    built_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let built: app_core::MaterializedProcedure =
        serde_json::from_str(built_json).map_err(|err| err.to_string())?;
    let mutation = app_core::insert_procedure_materialized_ui(
        &plan,
        start_component_index,
        end_component_index,
        built,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn replace_procedure_materialized_ui_json(
    plan_json: &str,
    component_index: usize,
    built_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let built: app_core::MaterializedProcedure =
        serde_json::from_str(built_json).map_err(|err| err.to_string())?;
    let mutation = app_core::replace_procedure_materialized_ui(&plan, component_index, built)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn select_preferred_cifp_tpp_match_json(rows_json: &str) -> Result<String, String> {
    let rows: Vec<app_core::CifpTppMatchRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let matched = app_core::select_preferred_cifp_tpp_match(rows);
    serde_json::to_string(&matched).map_err(|err| err.to_string())
}

pub fn describe_load_procedure_from_plate_json(
    plan_json: &str,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    options_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let options: app_core::ProcedureOptions =
        serde_json::from_str(options_json).map_err(|err| err.to_string())?;
    let description = app_core::describe_load_procedure_from_plate(
        &plan,
        airport_id,
        procedure_id,
        kind,
        options,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&description).map_err(|err| err.to_string())
}

pub fn create_ui_session_json(
    vector_manifest_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let result = app_core::create_ui_session(
        vector_manifest_json,
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn replace_flight_plan_in_session_json(handle: u64, plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::replace_flight_plan_in_session(handle as u32, plan)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn perform_flight_plan_row_action_in_session_json(
    handle: u64,
    row_uid: &str,
    action_uid: &str,
) -> Result<String, String> {
    let snapshot = app_core::perform_flight_plan_row_action_in_session(
        handle as u32,
        row_uid.to_string(),
        action_uid.to_string(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn load_plate_procedure_in_session_json(
    handle: u64,
    load_id: &str,
) -> Result<String, String> {
    let outcome =
        app_core::session::load_plate_procedure_in_session(handle as u32, load_id.to_string())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn activate_next_leg_in_session_json(handle: u64) -> Result<String, String> {
    let snapshot =
        app_core::activate_next_leg_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn suspend_sequencing_in_session_json(handle: u64) -> Result<String, String> {
    let snapshot =
        app_core::suspend_sequencing_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn unsuspend_sequencing_in_session_json(handle: u64) -> Result<String, String> {
    let snapshot =
        app_core::unsuspend_sequencing_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn sequence_active_leg_in_session_json(handle: u64) -> Result<String, String> {
    let snapshot =
        app_core::sequence_active_leg_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_guidance_leg_geometry_in_session_json(
    handle: u64,
    geometries_json: &str,
) -> Result<String, String> {
    let geometries: Vec<app_core::GuidanceLegGeometry> =
        serde_json::from_str(geometries_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_guidance_leg_geometry_in_session(handle as u32, geometries)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn perform_map_selection_action_in_session_json(
    handle: u64,
    action_json: &str,
) -> Result<String, String> {
    let outcome =
        app_core::perform_map_selection_action_in_session(handle as u32, action_json.to_string())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn insert_waypoint_at_flight_plan_row_in_session_json(
    handle: u64,
    row_uid: &str,
    before: bool,
    waypoint_json: &str,
) -> Result<String, String> {
    let waypoint: app_core::NavRef =
        serde_json::from_str(waypoint_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::insert_waypoint_at_flight_plan_row_in_session(
        handle as u32,
        row_uid.to_string(),
        before,
        waypoint,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn suggest_waypoint_identifiers_at_flight_plan_row_in_session_json(
    handle: u64,
    row_uid: &str,
    before: bool,
    prefix: &str,
    limit: usize,
) -> Result<String, String> {
    let outcome = app_core::suggest_waypoint_identifiers_at_flight_plan_row_in_session(
        handle as u32,
        row_uid.to_string(),
        before,
        prefix.to_string(),
        limit,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn insert_airway_at_flight_plan_row_in_session_json(
    handle: u64,
    row_uid: &str,
    presentation_json: &str,
    entry_index: usize,
    exit_index: usize,
) -> Result<String, String> {
    let presentation: app_core::AirwayPresentationPlan =
        serde_json::from_str(presentation_json).map_err(|err| err.to_string())?;
    let outcome = app_core::insert_airway_at_flight_plan_row_in_session(
        handle as u32,
        row_uid.to_string(),
        presentation,
        entry_index,
        exit_index,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn select_procedure_at_flight_plan_row_in_session_json(
    handle: u64,
    row_uid: &str,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, String> {
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let runway_transition: Option<String> =
        serde_json::from_str(runway_transition_json).map_err(|err| err.to_string())?;
    let enroute_transition: Option<String> =
        serde_json::from_str(enroute_transition_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_procedure_at_flight_plan_row_in_session(
        handle as u32,
        row_uid.to_string(),
        airport_id.to_string(),
        procedure_id.to_string(),
        kind,
        runway_transition,
        enroute_transition,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn select_airport_in_session_json(
    handle: u64,
    airport_id_json: &str,
) -> Result<String, String> {
    let airport_id: String =
        serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_airport_in_session(handle as u32, &airport_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn register_ownship_source_in_session_json(
    handle: u64,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::register_ownship_source_in_session(handle as u32, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn update_ownship_source_status_in_session_json(
    handle: u64,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::update_ownship_source_status_in_session(handle as u32, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn push_situation_sample_in_session_json(
    handle: u64,
    sample_json: &str,
) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::push_situation_sample_in_session(handle as u32, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_ownship_source_in_session_json(
    handle: u64,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_ownship_source_in_session(handle as u32, selection)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn apply_situation_control_input_in_session_json(
    handle: u64,
    input_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let input: app_core::SituationControlInput =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::apply_situation_control_input_in_session(handle as u32, input, now_epoch_ms)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn engage_map_follow_in_session_json(
    handle: u64,
    viewport_json: &str,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::engage_map_follow_in_session(handle as u32, viewport)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn disengage_map_follow_in_session_json(
    handle: u64,
    viewport_json: &str,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::disengage_map_follow_in_session(handle as u32, viewport)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_map_follow_offset_in_session_json(
    handle: u64,
    viewport_json: &str,
    offset_x_px: f64,
    offset_y_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_map_follow_offset_in_session(
        handle as u32,
        viewport,
        offset_x_px,
        offset_y_px,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn load_playback_trace_in_session_json(
    handle: u64,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, String> {
    let source_path: String =
        serde_json::from_str(source_path_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::load_playback_trace_in_session(handle as u32, &source_path, trace_json)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn play_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::play_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn pause_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::pause_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn seek_playback_in_session_json(
    handle: u64,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let snapshot = app_core::seek_playback_in_session(handle as u32, cursor_seconds, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_playback_rate_in_session_json(
    handle: u64,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let snapshot = app_core::set_playback_rate_in_session(handle as u32, rate, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn tick_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::tick_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_chart_in_session_json(handle: u64, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_chart_in_session(handle as u32, &chart_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_map_layer_visibility_in_session_json(
    handle: u64,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, String> {
    let layer_id: String = serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_map_layer_visibility_in_session(handle as u32, &layer_id, visible)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_map_layer_enabled_in_session_json(
    handle: u64,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let layer_id: String = serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_map_layer_enabled_in_session(handle as u32, &layer_id, enabled)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_debug_flag_in_session_json(
    handle: u64,
    flag_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let flag_id: String = serde_json::from_str(flag_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_debug_flag_in_session(handle as u32, &flag_id, enabled)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn load_raster_map_catalog_in_session_json(handle: u64) -> Result<String, String> {
    let outcome = app_core::load_raster_map_catalog_in_session(handle as u32)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn select_map_family_in_session_json(
    handle: u64,
    family_id_json: &str,
) -> Result<String, String> {
    let family_id: String = serde_json::from_str(family_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_map_family_in_session(handle as u32, &family_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn get_session_snapshot_json(handle: u64) -> Result<String, String> {
    let snapshot = app_core::get_session_snapshot(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn restore_chart_page_state_in_session_json(
    handle: u64,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::restore_chart_page_state_in_session(
        handle as u32,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn ingest_point_tiles_in_session_json(handle: u64, tiles_json: &str) -> Result<String, String> {
    let tiles: Vec<app_core::PointTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_point_tiles_in_session(handle as u32, &tiles)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_metar_tiles_in_session_json(handle: u64, tiles_json: &str) -> Result<String, String> {
    let tiles: Vec<app_core::MetarTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_metar_tiles_in_session(handle as u32, &tiles)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_airspace_ref_tiles_in_session_json(
    handle: u64,
    tiles_json: &str,
) -> Result<String, String> {
    let tiles: Vec<app_core::AirspaceReferenceTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_ref_tiles_in_session(handle as u32, &tiles)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_airspace_features_in_session_json(
    handle: u64,
    features_json: &str,
) -> Result<String, String> {
    let features: Vec<app_core::AirspaceFeaturePayload> =
        serde_json::from_str(features_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_features_in_session(handle as u32, &features)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_airspace_label_tiles_in_session_json(
    handle: u64,
    tiles_json: &str,
) -> Result<String, String> {
    let tiles: Vec<app_core::AirspaceLabelTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_label_tiles_in_session(handle as u32, &tiles)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_metars_in_session_json(handle: u64, payload_json: &str) -> Result<String, String> {
    let payload: app_core::MetarProductPayload =
        serde_json::from_str(payload_json).map_err(|err| err.to_string())?;
    app_core::ingest_metars_in_session(handle as u32, &payload).map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn ingest_tfrs_in_session_json(handle: u64, payload_json: &str) -> Result<String, String> {
    let payload: app_core::TfrProductPayload =
        serde_json::from_str(payload_json).map_err(|err| err.to_string())?;
    app_core::ingest_tfrs_in_session(handle as u32, &payload).map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn get_map_overlay_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay =
        app_core::get_map_overlay_in_session(handle as u32, viewport, width_px, height_px)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn get_map_selection_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    click_json: &str,
    hit_radius_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let click: app_core::LatLon =
        serde_json::from_str(click_json).map_err(|err| err.to_string())?;
    let selection = app_core::get_map_selection_in_session(
        handle as u32,
        viewport,
        width_px,
        height_px,
        click,
        hit_radius_px,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&selection).map_err(|err| err.to_string())
}

pub fn get_terrain_overlay_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay =
        app_core::get_terrain_overlay_in_session(handle as u32, viewport, width_px, height_px)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn get_raster_tile_plan_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan =
        app_core::get_raster_tile_plan_in_session(handle as u32, viewport, width_px, height_px)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn get_raster_tile_plan_in_session_with_options_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    max_tile_display_multiplier: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan = app_core::get_raster_tile_plan_in_session_with_options(
        handle as u32,
        viewport,
        width_px,
        height_px,
        app_core::RasterTilePlanOptions {
            max_tile_display_multiplier,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn render_terrain_overlay_tile_in_session_bytes(
    handle: u64,
    tile_bytes: &[u8],
    aircraft_altitude_ft: Option<f64>,
) -> Result<Vec<u8>, String> {
    app_core::render_terrain_overlay_tile_in_session(
        handle as u32,
        tile_bytes,
        aircraft_altitude_ft,
    )
    .map_err(|err| err.to_string())
}

pub fn render_terrain_overlay_tiles_in_session_bytes(
    handle: u64,
    packed_tile_bytes: &[u8],
    aircraft_altitude_ft: Option<f64>,
) -> Result<Vec<u8>, String> {
    app_core::render_terrain_overlay_tiles_in_session(
        handle as u32,
        packed_tile_bytes,
        aircraft_altitude_ft,
    )
    .map_err(|err| err.to_string())
}

pub fn sync_map_follow_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::sync_map_follow_in_session(handle as u32, viewport, width_px, height_px)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn destroy_session_json(handle: u64) {
    app_core::destroy_session(handle as u32);
}

static NEXT_NAV_KV_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_KV_STORES: OnceLock<Mutex<HashMap<u32, app_core::NavKvStore>>> = OnceLock::new();
static NEXT_OFFLINE_PACKAGES_CONTROLLER_HANDLE: AtomicU32 = AtomicU32::new(1);
static OFFLINE_PACKAGES_CONTROLLERS: OnceLock<
    Mutex<HashMap<u32, app_core::OfflinePackagesControllerState>>,
> = OnceLock::new();

fn nav_kv_stores() -> &'static Mutex<HashMap<u32, app_core::NavKvStore>> {
    NAV_KV_STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn offline_packages_controllers(
) -> &'static Mutex<HashMap<u32, app_core::OfflinePackagesControllerState>> {
    OFFLINE_PACKAGES_CONTROLLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn create_offline_packages_controller_json(
    packages_state_json: Option<&str>,
) -> Result<u64, String> {
    let packages_state = packages_state_json
        .filter(|json| !json.trim().is_empty())
        .map(|json| {
            serde_json::from_str::<app_core::OfflinePackagesState>(json)
                .map_err(|err| err.to_string())
        })
        .transpose()?;
    let handle = NEXT_OFFLINE_PACKAGES_CONTROLLER_HANDLE.fetch_add(1, Ordering::Relaxed);
    offline_packages_controllers()
        .lock()
        .map_err(|_| "offline packages controller store poisoned".to_string())?
        .insert(
            handle,
            app_core::OfflinePackagesControllerState {
                packages_state,
                ..Default::default()
            },
        );
    Ok(handle as u64)
}

pub fn destroy_offline_packages_controller_json(handle: u64) -> Result<(), String> {
    offline_packages_controllers()
        .lock()
        .map_err(|_| "offline packages controller store poisoned".to_string())?
        .remove(&(handle as u32))
        .ok_or_else(|| format!("invalid offline packages controller handle: {handle}"))?;
    Ok(())
}

pub fn nav_kv_open_bytes(root_bytes: &[u8]) -> Result<u64, String> {
    let root = app_core::NavKvRoot::parse(root_bytes)?;
    let handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?
        .insert(handle, app_core::NavKvStore::new(root));
    Ok(handle as u64)
}

pub fn nav_kv_insert_page_bytes(
    handle: u64,
    page_index: u32,
    page_bytes: &[u8],
) -> Result<(), String> {
    let mut stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    let store = stores
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid nav kv handle: {handle}"))?;
    store.insert_page(page_index, page_bytes.to_vec());
    app_core::insert_nav_kv_page_for_attached_sessions(handle as u32, page_index, page_bytes);
    Ok(())
}

pub fn attach_nav_kv_store_to_session_json(
    nav_kv_handle: u64,
    session_handle: u64,
) -> Result<(), String> {
    let stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    let store = stores
        .get(&(nav_kv_handle as u32))
        .ok_or_else(|| format!("invalid nav kv handle: {nav_kv_handle}"))?;
    app_core::attach_nav_kv_store_to_session(session_handle as u32, nav_kv_handle as u32, store)
        .map_err(|err| err.to_string())
}

pub fn nav_kv_destroy_handle(handle: u64) {
    let _ = nav_kv_stores()
        .lock()
        .expect("nav kv store poisoned")
        .remove(&(handle as u32));
}

pub fn core_had_operation_json(handle: u64, operation_json: &str) -> Result<String, String> {
    let operation: app_core::HadOperation =
        serde_json::from_str(operation_json).map_err(|err| err.to_string())?;
    let stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    let store = stores
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid nav kv handle: {handle}"))?;
    let outcome = app_core::run_had_operation(store, operation).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

#[derive(Deserialize)]
struct BundlePackageManagementInputWire {
    now_epoch_ms: i64,
    preferences: app_core::OfflinePackagePreferences,
    bundle_json: String,
    installed: Vec<app_core::InstalledArtifact>,
}

#[derive(Deserialize)]
struct OfflinePackagesInitInputWire {
    state: Option<app_core::OfflinePackagesState>,
    region_ids: Vec<String>,
    product_ids: Vec<String>,
    now_epoch_ms: i64,
    discovery_jsons: Vec<String>,
    bundle_jsons_by_filename: BTreeMap<String, String>,
    installed: Vec<app_core::InstalledArtifact>,
}

#[derive(Deserialize)]
struct OfflinePackagesReduceInputWire {
    state: app_core::OfflinePackagesState,
    event: app_core::OfflinePackagesEvent,
    region_ids: Vec<String>,
    product_ids: Vec<String>,
    now_epoch_ms: i64,
    discovery_jsons: Vec<String>,
    bundle_jsons_by_filename: BTreeMap<String, String>,
    installed: Vec<app_core::InstalledArtifact>,
}

#[derive(Deserialize)]
struct OfflinePackagesControllerLibraryRefreshSucceededWire {
    fetched_at_epoch_ms: i64,
    discovery_jsons: Vec<String>,
    bundle_jsons_by_filename: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct OfflinePackagesControllerInstalledArtifactHealthObservedWire {
    unreadable_installed_filename_messages: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OfflinePackagesControllerEventWire {
    EnsureLibrary,
    RefreshLibraryRequested,
    LibraryRefreshSucceeded(OfflinePackagesControllerLibraryRefreshSucceededWire),
    LibraryRefreshFailed {
        message: String,
    },
    InstalledArtifactHealthObserved(OfflinePackagesControllerInstalledArtifactHealthObservedWire),
    PackagesEvent {
        event: app_core::OfflinePackagesEvent,
    },
    SyncRequested,
    SyncProgressObserved {
        progress: app_core::OfflinePackagesSyncProgress,
    },
    SyncFinished {
        summary: app_core::OfflinePackagesSyncSummary,
    },
}

#[derive(Deserialize)]
struct OfflinePackagesControllerInputWire {
    package_source_base_url: String,
    discovery_filenames: Vec<String>,
    region_ids: Vec<String>,
    product_ids: Vec<String>,
    now_epoch_ms: i64,
    installed: Vec<app_core::InstalledArtifact>,
    event: OfflinePackagesControllerEventWire,
}

#[derive(Serialize)]
struct OfflinePackagesControllerResultWire {
    packages_state_json: Option<String>,
    ui_state: app_core::OfflinePackagesControllerUiState,
    command: Option<app_core::OfflinePackagesControllerCommand>,
}

pub fn plan_offline_packages_from_bundle_json(input_json: &str) -> Result<String, String> {
    let input: BundlePackageManagementInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let bundle: app_core::BundleManifest =
        serde_json::from_str(&input.bundle_json).map_err(|err| err.to_string())?;
    let plan = app_core::plan_offline_packages(&app_core::PackageManagementInput {
        now_epoch_ms: input.now_epoch_ms,
        preferences: input.preferences,
        bundle,
        installed: input.installed,
        forced_gc_installed_filenames: Vec::new(),
        suppressed_fetch_filenames: Vec::new(),
    });
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn initialize_offline_packages_json(input_json: &str) -> Result<String, String> {
    let input: OfflinePackagesInitInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let discovery_manifests = input
        .discovery_jsons
        .into_iter()
        .map(|payload| serde_json::from_str::<app_core::CurrentArtifactsManifest>(&payload))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let bundle_manifests_by_filename = input
        .bundle_jsons_by_filename
        .into_iter()
        .map(|(filename, payload)| {
            serde_json::from_str::<app_core::BundleManifest>(&payload)
                .map(|bundle| (filename, bundle))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|err| err.to_string())?;
    let result = app_core::initialize_offline_packages(&app_core::OfflinePackagesInitInput {
        state: input.state,
        region_ids: input.region_ids,
        product_ids: input.product_ids,
        now_epoch_ms: input.now_epoch_ms,
        discovery_manifests,
        bundle_manifests_by_filename,
        installed: input.installed,
        forced_gc_installed_filenames: Vec::new(),
        suppressed_fetch_filenames: Vec::new(),
    });
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn reduce_offline_packages_json(input_json: &str) -> Result<String, String> {
    let input: OfflinePackagesReduceInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let discovery_manifests = input
        .discovery_jsons
        .into_iter()
        .map(|payload| serde_json::from_str::<app_core::CurrentArtifactsManifest>(&payload))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let bundle_manifests_by_filename = input
        .bundle_jsons_by_filename
        .into_iter()
        .map(|(filename, payload)| {
            serde_json::from_str::<app_core::BundleManifest>(&payload)
                .map(|bundle| (filename, bundle))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|err| err.to_string())?;
    let result = app_core::reduce_offline_packages(&app_core::OfflinePackagesReduceInput {
        state: input.state,
        event: input.event,
        region_ids: input.region_ids,
        product_ids: input.product_ids,
        now_epoch_ms: input.now_epoch_ms,
        discovery_manifests,
        bundle_manifests_by_filename,
        installed: input.installed,
        forced_gc_installed_filenames: Vec::new(),
        suppressed_fetch_filenames: Vec::new(),
    });
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn dispatch_offline_packages_controller_json(
    handle: u64,
    input_json: &str,
) -> Result<String, String> {
    let input: OfflinePackagesControllerInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let mut controllers = offline_packages_controllers()
        .lock()
        .map_err(|_| "offline packages controller store poisoned".to_string())?;
    let state = controllers
        .get(&(handle as u32))
        .cloned()
        .ok_or_else(|| format!("invalid offline packages controller handle: {handle}"))?;
    let event = match input.event {
        OfflinePackagesControllerEventWire::EnsureLibrary => {
            app_core::OfflinePackagesControllerEvent::EnsureLibrary
        }
        OfflinePackagesControllerEventWire::RefreshLibraryRequested => {
            app_core::OfflinePackagesControllerEvent::RefreshLibraryRequested
        }
        OfflinePackagesControllerEventWire::LibraryRefreshSucceeded(payload) => {
            let discovery_manifests = payload
                .discovery_jsons
                .into_iter()
                .map(|json| serde_json::from_str::<app_core::CurrentArtifactsManifest>(&json))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| err.to_string())?;
            let bundle_manifests_by_filename = payload
                .bundle_jsons_by_filename
                .into_iter()
                .map(|(filename, json)| {
                    serde_json::from_str::<app_core::BundleManifest>(&json)
                        .map(|bundle| (filename, bundle))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|err| err.to_string())?;
            app_core::OfflinePackagesControllerEvent::LibraryRefreshSucceeded {
                fetched_at_epoch_ms: payload.fetched_at_epoch_ms,
                discovery_manifests,
                bundle_manifests_by_filename,
            }
        }
        OfflinePackagesControllerEventWire::LibraryRefreshFailed { message } => {
            app_core::OfflinePackagesControllerEvent::LibraryRefreshFailed { message }
        }
        OfflinePackagesControllerEventWire::InstalledArtifactHealthObserved(payload) => {
            app_core::OfflinePackagesControllerEvent::InstalledArtifactHealthObserved {
                unreadable_installed_filename_messages: payload
                    .unreadable_installed_filename_messages,
            }
        }
        OfflinePackagesControllerEventWire::PackagesEvent { event } => {
            app_core::OfflinePackagesControllerEvent::PackagesEvent { event }
        }
        OfflinePackagesControllerEventWire::SyncRequested => {
            app_core::OfflinePackagesControllerEvent::SyncRequested
        }
        OfflinePackagesControllerEventWire::SyncProgressObserved { progress } => {
            app_core::OfflinePackagesControllerEvent::SyncProgressObserved { progress }
        }
        OfflinePackagesControllerEventWire::SyncFinished { summary } => {
            app_core::OfflinePackagesControllerEvent::SyncFinished { summary }
        }
    };
    let result =
        app_core::reduce_offline_packages_controller(&app_core::OfflinePackagesControllerInput {
            state: Some(state),
            package_source_base_url: input.package_source_base_url,
            discovery_filenames: input.discovery_filenames,
            region_ids: input.region_ids,
            product_ids: input.product_ids,
            now_epoch_ms: input.now_epoch_ms,
            installed: input.installed,
            event,
        });
    controllers.insert(handle as u32, result.state.clone());
    serde_json::to_string(&OfflinePackagesControllerResultWire {
        packages_state_json: result
            .state
            .packages_state
            .as_ref()
            .map(|state| serde_json::to_string(state))
            .transpose()
            .map_err(|err| err.to_string())?,
        ui_state: result.ui_state,
        command: result.command,
    })
    .map_err(|err| err.to_string())
}

fn get_java_string(env: &mut JNIEnv, value: JString) -> Result<String, String> {
    env.get_string(&value)
        .map(|s| s.into())
        .map_err(|err| err.to_string())
}

fn get_java_byte_array(env: &mut JNIEnv, value: JByteArray) -> Result<Vec<u8>, String> {
    env.convert_byte_array(value).map_err(|err| err.to_string())
}

fn return_string(env: &mut JNIEnv, value: Result<String, String>) -> jstring {
    match value {
        Ok(text) => env
            .new_string(text)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            std::ptr::null_mut()
        }
    }
}

fn return_byte_array(env: &mut JNIEnv, value: Result<Vec<u8>, String>) -> jbyteArray {
    match value {
        Ok(bytes) => env
            .byte_array_from_slice(&bytes)
            .map(|array| array.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_situationRingCandidatesJson(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    return_string(
        &mut env,
        serde_json::to_string(&app_core::situation_ring_candidates())
            .map_err(|err| err.to_string()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_planOfflinePackagesFromBundleJson(
    mut env: JNIEnv,
    _class: JClass,
    input_json: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input_json)?;
        plan_offline_packages_from_bundle_json(&input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_initializeOfflinePackagesJson(
    mut env: JNIEnv,
    _class: JClass,
    input_json: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input_json)?;
        initialize_offline_packages_json(&input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_reduceOfflinePackagesJson(
    mut env: JNIEnv,
    _class: JClass,
    input_json: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input_json)?;
        reduce_offline_packages_json(&input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_createOfflinePackagesController(
    mut env: JNIEnv,
    _class: JClass,
    packages_state_json: JString,
) -> i64 {
    let result = (|| {
        let packages_state_json = get_java_string(&mut env, packages_state_json)?;
        create_offline_packages_controller_json(Some(&packages_state_json))
    })();
    match result {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_dispatchOfflinePackagesControllerJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    input_json: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input_json)?;
        dispatch_offline_packages_controller_json(handle as u64, &input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_destroyOfflinePackagesController(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = destroy_offline_packages_controller_json(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_state_json(&state, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_ui_state_json(&state, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_removeFlightPlanLegJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    index: i32,
) -> jstring {
    let result = (|| {
        let plan = get_java_string(&mut env, plan_json)?;
        remove_flight_plan_leg_json(&plan, index as usize)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setContentPolicyStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let policy = get_java_string(&mut env, policy_json)?;
        set_content_policy_state_json(&state, &policy)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setContentPolicyUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let policy = get_java_string(&mut env, policy_json)?;
        set_content_policy_ui_state_json(&state, &policy)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_refreshContentStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    inventory_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let inventory = get_java_string(&mut env, inventory_json)?;
        refresh_content_state_json(&state, &inventory)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_refreshContentUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    inventory_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let inventory = get_java_string(&mut env, inventory_json)?;
        refresh_content_ui_state_json(&state, &inventory)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_activateLegUiJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    leg_index: i32,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        activate_leg_ui_json(&plan_json, leg_index as usize)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_prepareAirwayPresentationJson(
    mut env: JNIEnv,
    _class: JClass,
    airway_name: JString,
    branches_json: JString,
    origin_position_json: JString,
    destination_position_json: JString,
) -> jstring {
    let result = (|| {
        let airway_name = get_java_string(&mut env, airway_name)?;
        let branches_json = get_java_string(&mut env, branches_json)?;
        let origin_position_json = get_java_string(&mut env, origin_position_json)?;
        let destination_position_json = get_java_string(&mut env, destination_position_json)?;
        prepare_airway_presentation_json(
            &airway_name,
            &branches_json,
            &origin_position_json,
            &destination_position_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_sortAirwaySuggestionsForUiJson(
    mut env: JNIEnv,
    _class: JClass,
    suggestions_json: JString,
) -> jstring {
    let result = (|| {
        let suggestions_json = get_java_string(&mut env, suggestions_json)?;
        sort_airway_suggestions_for_ui_json(&suggestions_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_insertAirwayMaterializedUiJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    start_component_index: i32,
    end_component_index_json: JString,
    selection_json: JString,
    airway_json: JString,
    resolved_legs_json: JString,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        let end_component_index_json = get_java_string(&mut env, end_component_index_json)?;
        let selection_json = get_java_string(&mut env, selection_json)?;
        let airway_json = get_java_string(&mut env, airway_json)?;
        let resolved_legs_json = get_java_string(&mut env, resolved_legs_json)?;
        insert_airway_materialized_ui_json(
            &plan_json,
            start_component_index as usize,
            &end_component_index_json,
            &selection_json,
            &airway_json,
            &resolved_legs_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceAirwayMaterializedUiJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    component_index: i32,
    selection_json: JString,
    airway_json: JString,
    resolved_legs_json: JString,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        let selection_json = get_java_string(&mut env, selection_json)?;
        let airway_json = get_java_string(&mut env, airway_json)?;
        let resolved_legs_json = get_java_string(&mut env, resolved_legs_json)?;
        replace_airway_materialized_ui_json(
            &plan_json,
            component_index as usize,
            &selection_json,
            &airway_json,
            &resolved_legs_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_insertProcedureMaterializedUiJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    start_component_index: i32,
    end_component_index: i32,
    built_json: JString,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        let built_json = get_java_string(&mut env, built_json)?;
        insert_procedure_materialized_ui_json(
            &plan_json,
            start_component_index as usize,
            end_component_index as usize,
            &built_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceProcedureMaterializedUiJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    component_index: i32,
    built_json: JString,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        let built_json = get_java_string(&mut env, built_json)?;
        replace_procedure_materialized_ui_json(&plan_json, component_index as usize, &built_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_createUiSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    vector_manifest_json: JString,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let vector_manifest = get_java_string(&mut env, vector_manifest_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        create_ui_session_json(
            &vector_manifest,
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let plan_json = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_in_session_json(handle as u64, &plan_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_performFlightPlanRowActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    action_uid: JString,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let action_uid = get_java_string(&mut env, action_uid)?;
        perform_flight_plan_row_action_in_session_json(handle as u64, &row_uid, &action_uid)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_loadPlateProcedureInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    load_id: JString,
) -> jstring {
    let result = (|| {
        let load_id = get_java_string(&mut env, load_id)?;
        load_plate_procedure_in_session_json(handle as u64, &load_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_activateNextLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| activate_next_leg_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_suspendSequencingInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| suspend_sequencing_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_unsuspendSequencingInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| unsuspend_sequencing_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_sequenceActiveLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| sequence_active_leg_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setGuidanceLegGeometryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    geometries_json: JString,
) -> jstring {
    let result = (|| {
        let geometries_json = get_java_string(&mut env, geometries_json)?;
        set_guidance_leg_geometry_in_session_json(handle as u64, &geometries_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_performMapSelectionActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_json: JString,
) -> jstring {
    let result = (|| {
        let action = get_java_string(&mut env, action_json)?;
        perform_map_selection_action_in_session_json(handle as u64, &action)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_insertWaypointAtFlightPlanRowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    before: bool,
    waypoint_json: JString,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let waypoint_json = get_java_string(&mut env, waypoint_json)?;
        insert_waypoint_at_flight_plan_row_in_session_json(
            handle as u64,
            &row_uid,
            before,
            &waypoint_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_suggestWaypointIdentifiersAtFlightPlanRowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    before: bool,
    prefix: JString,
    limit: i32,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let prefix = get_java_string(&mut env, prefix)?;
        suggest_waypoint_identifiers_at_flight_plan_row_in_session_json(
            handle as u64,
            &row_uid,
            before,
            &prefix,
            limit as usize,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_insertAirwayAtFlightPlanRowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    presentation_json: JString,
    entry_index: i32,
    exit_index: i32,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let presentation_json = get_java_string(&mut env, presentation_json)?;
        insert_airway_at_flight_plan_row_in_session_json(
            handle as u64,
            &row_uid,
            &presentation_json,
            entry_index as usize,
            exit_index as usize,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectProcedureAtFlightPlanRowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    airport_id: JString,
    procedure_id: JString,
    kind_json: JString,
    runway_transition_json: JString,
    enroute_transition_json: JString,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let airport_id = get_java_string(&mut env, airport_id)?;
        let procedure_id = get_java_string(&mut env, procedure_id)?;
        let kind_json = get_java_string(&mut env, kind_json)?;
        let runway_transition_json = get_java_string(&mut env, runway_transition_json)?;
        let enroute_transition_json = get_java_string(&mut env, enroute_transition_json)?;
        select_procedure_at_flight_plan_row_in_session_json(
            handle as u64,
            &row_uid,
            &airport_id,
            &procedure_id,
            &kind_json,
            &runway_transition_json,
            &enroute_transition_json,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectAirportInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    airport_id_json: JString,
) -> jstring {
    let result = (|| {
        let airport_id = get_java_string(&mut env, airport_id_json)?;
        select_airport_in_session_json(handle as u64, &airport_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_registerOwnshipSourceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    registration_json: JString,
) -> jstring {
    let result = (|| {
        let registration_json = get_java_string(&mut env, registration_json)?;
        register_ownship_source_in_session_json(handle as u64, &registration_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_updateOwnshipSourceStatusInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    update_json: JString,
) -> jstring {
    let result = (|| {
        let update_json = get_java_string(&mut env, update_json)?;
        update_ownship_source_status_in_session_json(handle as u64, &update_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_pushSituationSampleInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    sample_json: JString,
) -> jstring {
    let result = (|| {
        let sample_json = get_java_string(&mut env, sample_json)?;
        push_situation_sample_in_session_json(handle as u64, &sample_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectOwnshipSourceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    selection_json: JString,
) -> jstring {
    let result = (|| {
        let selection_json = get_java_string(&mut env, selection_json)?;
        select_ownship_source_in_session_json(handle as u64, &selection_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_applySituationControlInputInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    input_json: JString,
    now_epoch_ms: f64,
) -> jstring {
    let result = (|| {
        let input_json = get_java_string(&mut env, input_json)?;
        apply_situation_control_input_in_session_json(handle as u64, &input_json, now_epoch_ms)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_engageMapFollowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
) -> jstring {
    let result = (|| {
        let viewport_json = get_java_string(&mut env, viewport_json)?;
        engage_map_follow_in_session_json(handle as u64, &viewport_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_disengageMapFollowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
) -> jstring {
    let result = (|| {
        let viewport_json = get_java_string(&mut env, viewport_json)?;
        disengage_map_follow_in_session_json(handle as u64, &viewport_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setMapFollowOffsetInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    offset_x_px: f64,
    offset_y_px: f64,
) -> jstring {
    let result = (|| {
        let viewport_json = get_java_string(&mut env, viewport_json)?;
        set_map_follow_offset_in_session_json(
            handle as u64,
            &viewport_json,
            offset_x_px,
            offset_y_px,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_loadPlaybackTraceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    source_path_json: JString,
    trace_json: JString,
) -> jstring {
    let result = (|| {
        let source_path_json = get_java_string(&mut env, source_path_json)?;
        let trace_json = get_java_string(&mut env, trace_json)?;
        load_playback_trace_in_session_json(handle as u64, &source_path_json, &trace_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_playPlaybackInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        play_playback_in_session_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_pausePlaybackInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        pause_playback_in_session_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_seekPlaybackInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        seek_playback_in_session_json(handle as u64, cursor_seconds, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setPlaybackRateInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    rate: f64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        set_playback_rate_in_session_json(handle as u64, rate, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_tickPlaybackInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        tick_playback_in_session_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectChartInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let chart_id = get_java_string(&mut env, chart_id_json)?;
        select_chart_in_session_json(handle as u64, &chart_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setMapLayerVisibilityInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    layer_id_json: JString,
    visible: bool,
) -> jstring {
    let result = (|| {
        let layer_id = get_java_string(&mut env, layer_id_json)?;
        set_map_layer_visibility_in_session_json(handle as u64, &layer_id, visible)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setMapLayerEnabledInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    layer_id_json: JString,
    enabled: bool,
) -> jstring {
    let result = (|| {
        let layer_id = get_java_string(&mut env, layer_id_json)?;
        set_map_layer_enabled_in_session_json(handle as u64, &layer_id, enabled)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setDebugFlagInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    flag_id_json: JString,
    enabled: bool,
) -> jstring {
    let result = (|| {
        let flag_id = get_java_string(&mut env, flag_id_json)?;
        set_debug_flag_in_session_json(handle as u64, &flag_id, enabled)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_loadRasterMapCatalogInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = load_raster_map_catalog_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectMapFamilyInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    family_id_json: JString,
) -> jstring {
    let result = (|| {
        let family_id = get_java_string(&mut env, family_id_json)?;
        select_map_family_in_session_json(handle as u64, &family_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getSessionSnapshotJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, get_session_snapshot_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_restoreChartPageStateInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        restore_chart_page_state_in_session_json(
            handle as u64,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestPointTilesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tiles_json: JString,
) -> jstring {
    let result = (|| {
        let tiles = get_java_string(&mut env, tiles_json)?;
        ingest_point_tiles_in_session_json(handle as u64, &tiles)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestMetarTilesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tiles_json: JString,
) -> jstring {
    let result = (|| {
        let tiles = get_java_string(&mut env, tiles_json)?;
        ingest_metar_tiles_in_session_json(handle as u64, &tiles)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestAirspaceRefTilesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tiles_json: JString,
) -> jstring {
    let result = (|| {
        let tiles = get_java_string(&mut env, tiles_json)?;
        ingest_airspace_ref_tiles_in_session_json(handle as u64, &tiles)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestAirspaceFeaturesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    features_json: JString,
) -> jstring {
    let result = (|| {
        let features = get_java_string(&mut env, features_json)?;
        ingest_airspace_features_in_session_json(handle as u64, &features)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestAirspaceLabelTilesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tiles_json: JString,
) -> jstring {
    let result = (|| {
        let tiles = get_java_string(&mut env, tiles_json)?;
        ingest_airspace_label_tiles_in_session_json(handle as u64, &tiles)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestTfrsInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    payload_json: JString,
) -> jstring {
    let result = (|| {
        let payload = get_java_string(&mut env, payload_json)?;
        ingest_tfrs_in_session_json(handle as u64, &payload)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_ingestMetarsInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    payload_json: JString,
) -> jstring {
    let result = (|| {
        let payload = get_java_string(&mut env, payload_json)?;
        ingest_metars_in_session_json(handle as u64, &payload)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getMapOverlayInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_map_overlay_in_session_json(handle as u64, &viewport, width_px, height_px)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getMapSelectionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    click_json: JString,
    hit_radius_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        let click = get_java_string(&mut env, click_json)?;
        get_map_selection_in_session_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            &click,
            hit_radius_px,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getTerrainOverlayInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_terrain_overlay_in_session_json(handle as u64, &viewport, width_px, height_px)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getRasterTilePlanInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_raster_tile_plan_in_session_json(handle as u64, &viewport, width_px, height_px)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getRasterTilePlanInSessionWithOptionsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    max_tile_display_multiplier: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_raster_tile_plan_in_session_with_options_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            max_tile_display_multiplier,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_renderTerrainOverlayTileInSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tile_bytes: JByteArray,
    aircraft_altitude_ft: f64,
) -> jbyteArray {
    let result = (|| {
        let tile_bytes = get_java_byte_array(&mut env, tile_bytes)?;
        render_terrain_overlay_tile_in_session_bytes(
            handle as u64,
            &tile_bytes,
            if aircraft_altitude_ft.is_finite() {
                Some(aircraft_altitude_ft)
            } else {
                None
            },
        )
    })();
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_renderTerrainOverlayTilesInSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    packed_tile_bytes: JByteArray,
    aircraft_altitude_ft: f64,
) -> jbyteArray {
    let result = (|| {
        let packed_tile_bytes = get_java_byte_array(&mut env, packed_tile_bytes)?;
        render_terrain_overlay_tiles_in_session_bytes(
            handle as u64,
            &packed_tile_bytes,
            if aircraft_altitude_ft.is_finite() {
                Some(aircraft_altitude_ft)
            } else {
                None
            },
        )
    })();
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_syncMapFollowInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        sync_map_follow_in_session_json(handle as u64, &viewport, width_px, height_px)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_destroySession(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    destroy_session_json(handle as u64)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_navKvOpen(
    mut env: JNIEnv,
    _class: JClass,
    root_bytes: JByteArray,
) -> i64 {
    match get_java_byte_array(&mut env, root_bytes).and_then(|bytes| nav_kv_open_bytes(&bytes)) {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_navKvInsertPage(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    page_index: i32,
    page_bytes: JByteArray,
) {
    let result = get_java_byte_array(&mut env, page_bytes)
        .and_then(|bytes| nav_kv_insert_page_bytes(handle as u64, page_index as u32, &bytes));
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_navKvDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    nav_kv_destroy_handle(handle as u64)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_attachNavKvStoreToSession(
    mut env: JNIEnv,
    _class: JClass,
    nav_kv_handle: i64,
    session_handle: i64,
) {
    if let Err(message) =
        attach_nav_kv_store_to_session_json(nav_kv_handle as u64, session_handle as u64)
    {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_coreHadOperation(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    operation_json: JString,
) -> jstring {
    let result = (|| {
        let operation_json = get_java_string(&mut env, operation_json)?;
        core_had_operation_json(handle as u64, &operation_json)
    })();
    return_string(&mut env, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state_json() -> String {
        serde_json::to_string(&app_core::AppState::default()).unwrap()
    }

    fn sample_plan_json() -> String {
        serde_json::json!({
            "id": "plan-1",
            "name": "KBOS local",
            "legs": [],
            "route_components": [
                {
                    "kind": "waypoint",
                    "waypoint": {"Airport": "KBOS"}
                },
                {
                    "kind": "waypoint",
                    "waypoint": {"Airport": "KBOS"}
                }
            ],
            "resolved_legs": [
                {
                    "id": "component-0-1",
                    "from": {"Airport": "KBOS"},
                    "to": {"Airport": "KBOS"},
                    "source": {"kind": "route_component", "component_index": 0},
                    "procedure_provenance": null
                }
            ],
            "guidance": null,
            "departure": "KBOS",
            "destination": "KBOS",
            "alternate": null,
            "cruise_altitude_ft": 3000,
            "notes": null,
            "updated_at_epoch_ms": 0,
            "version": 1
        })
        .to_string()
    }

    #[test]
    fn replace_flight_plan_state_json_sets_active_plan() {
        let next_json =
            replace_flight_plan_state_json(&empty_state_json(), &sample_plan_json()).unwrap();
        let next: app_core::AppState = serde_json::from_str(&next_json).unwrap();

        assert!(next.active_plan.is_some());
    }

    #[test]
    fn replace_flight_plan_ui_state_json_returns_projected_app_view() {
        let next_json =
            replace_flight_plan_ui_state_json(&empty_state_json(), &sample_plan_json()).unwrap();
        let next: app_core::AppUiState = serde_json::from_str(&next_json).unwrap();

        assert!(next.active_plan.is_some());
    }

    #[test]
    fn activate_leg_ui_json_returns_projected_mutation() {
        let plan_json = serde_json::json!({
            "id": "plan-2",
            "name": "Guided",
            "legs": [],
            "route_components": [
                {"kind":"waypoint","waypoint":{"Airport":"KRNT"}},
                {"kind":"waypoint","waypoint":{"Navaid":"SEA"}},
                {"kind":"waypoint","waypoint":{"Airport":"KUAO"}}
            ],
            "resolved_legs": [
                {"id":"component-0-1","from":{"Airport":"KRNT"},"to":{"Navaid":"SEA"},"source":{"kind":"route_component","component_index":0}},
                {"id":"component-1-2","from":{"Navaid":"SEA"},"to":{"Airport":"KUAO"},"source":{"kind":"route_component","component_index":1}}
            ],
            "guidance": {"active_leg_index":0,"sequencing_mode":"follow_plan","direct_to":null},
            "departure": "KRNT",
            "destination": "KUAO",
            "alternate": null,
            "cruise_altitude_ft": null,
            "notes": null,
            "updated_at_epoch_ms": 0,
            "version": 1
        })
        .to_string();

        let next_json = activate_leg_ui_json(&plan_json, 1).unwrap();
        let next: app_core::FlightPlanUiMutation = serde_json::from_str(&next_json).unwrap();

        assert_eq!(
            next.ui_state.guidance.as_ref().unwrap().active_leg_index,
            Some(1)
        );
        assert_eq!(
            next.ui_state
                .guidance
                .as_ref()
                .unwrap()
                .active_component_index,
            Some(1)
        );
    }

    #[test]
    fn stream_allowed_policy_survives_json_boundary() {
        let with_plan_json =
            replace_flight_plan_state_json(&empty_state_json(), &sample_plan_json()).unwrap();

        let web_state_json = set_content_policy_state_json(
            &with_plan_json,
            &serde_json::to_string(&app_core::ContentPolicy::StreamAllowed).unwrap(),
        )
        .unwrap();

        let refreshed_json = refresh_content_state_json(
            &web_state_json,
            &serde_json::json!({
                "installed_packages": [],
                "cached_tilesets": [],
                "cached_plates": []
            })
            .to_string(),
        )
        .unwrap();

        let refreshed: app_core::AppState = serde_json::from_str(&refreshed_json).unwrap();
        assert_eq!(
            refreshed.content_policy,
            app_core::ContentPolicy::StreamAllowed
        );
        assert!(refreshed.last_content_report.is_none());
    }
}
