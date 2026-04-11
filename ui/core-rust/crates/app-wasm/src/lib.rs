use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn load_catalog(catalog_json: &str) -> Result<String, JsValue> {
    load_catalog_json(catalog_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn build_flight_plan(plan_json: &str) -> Result<String, JsValue> {
    build_flight_plan_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn build_flight_plan_ui(plan_json: &str) -> Result<String, JsValue> {
    build_flight_plan_ui_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn activate_leg_ui(plan_json: &str, leg_index: usize) -> Result<String, JsValue> {
    activate_leg_ui_json(plan_json, leg_index).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn activate_next_leg_ui(plan_json: &str) -> Result<String, JsValue> {
    activate_next_leg_ui_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn suspend_sequencing_ui(plan_json: &str) -> Result<String, JsValue> {
    suspend_sequencing_ui_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn unsuspend_sequencing_ui(plan_json: &str) -> Result<String, JsValue> {
    unsuspend_sequencing_ui_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn sequence_active_leg_ui(plan_json: &str) -> Result<String, JsValue> {
    sequence_active_leg_ui_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn activate_direct_to_leg_ui(
    plan_json: &str,
    lat: f64,
    lon: f64,
    target_leg_id: &str,
) -> Result<String, JsValue> {
    activate_direct_to_leg_ui_json(plan_json, lat, lon, target_leg_id)
        .map_err(|err| JsValue::from_str(&err))
}

#[cfg(not(target_arch = "wasm32"))]
#[wasm_bindgen]
pub fn insert_airway_from_anchors_ui(
    db_path: &str,
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor_json: &str,
    destination_anchor_json: &str,
) -> Result<String, JsValue> {
    insert_airway_from_anchors_ui_json(
        db_path,
        plan_json,
        start_component_index,
        end_component_index,
        airway_name,
        origin_anchor_json,
        destination_anchor_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[cfg(not(target_arch = "wasm32"))]
#[wasm_bindgen]
pub fn replace_airway_from_selection_ui(
    db_path: &str,
    plan_json: &str,
    component_index: usize,
    entry_json: &str,
    exit_json: &str,
) -> Result<String, JsValue> {
    replace_airway_from_selection_ui_json(db_path, plan_json, component_index, entry_json, exit_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[cfg(not(target_arch = "wasm32"))]
#[wasm_bindgen]
pub fn insert_procedure_from_selection_ui(
    db_path: &str,
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, JsValue> {
    insert_procedure_from_selection_ui_json(
        db_path,
        plan_json,
        start_component_index,
        end_component_index,
        airport_id,
        procedure_id,
        kind_json,
        runway_transition_json,
        enroute_transition_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[cfg(not(target_arch = "wasm32"))]
#[wasm_bindgen]
pub fn replace_procedure_from_selection_ui(
    db_path: &str,
    plan_json: &str,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, JsValue> {
    replace_procedure_from_selection_ui_json(
        db_path,
        plan_json,
        component_index,
        airport_id,
        procedure_id,
        kind_json,
        runway_transition_json,
        enroute_transition_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn remove_flight_plan_leg(plan_json: &str, index: usize) -> Result<String, JsValue> {
    remove_flight_plan_leg_json(plan_json, index).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn insert_airway_materialized_ui(
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    selection_json: &str,
    airway_json: &str,
    resolved_legs_json: &str,
) -> Result<String, JsValue> {
    insert_airway_materialized_ui_json(
        plan_json,
        start_component_index,
        end_component_index,
        selection_json,
        airway_json,
        resolved_legs_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_airway_materialized_ui(
    plan_json: &str,
    component_index: usize,
    selection_json: &str,
    airway_json: &str,
    resolved_legs_json: &str,
) -> Result<String, JsValue> {
    replace_airway_materialized_ui_json(
        plan_json,
        component_index,
        selection_json,
        airway_json,
        resolved_legs_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn insert_procedure_materialized_ui(
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    built_json: &str,
) -> Result<String, JsValue> {
    insert_procedure_materialized_ui_json(
        plan_json,
        start_component_index,
        end_component_index,
        built_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_procedure_materialized_ui(
    plan_json: &str,
    component_index: usize,
    built_json: &str,
) -> Result<String, JsValue> {
    replace_procedure_materialized_ui_json(plan_json, component_index, built_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_flight_plan_state(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, JsValue> {
    replace_flight_plan_state_json(state_json, catalog_json, plan_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_flight_plan_ui_state(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, JsValue> {
    replace_flight_plan_ui_state_json(state_json, catalog_json, plan_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_content_policy_state(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, JsValue> {
    set_content_policy_state_json(state_json, catalog_json, policy_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_content_policy_ui_state(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, JsValue> {
    set_content_policy_ui_state_json(state_json, catalog_json, policy_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn refresh_content_state(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, JsValue> {
    refresh_content_state_json(state_json, catalog_json, inventory_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn refresh_content_ui_state(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, JsValue> {
    refresh_content_ui_state_json(state_json, catalog_json, inventory_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn chart_for_position(
    catalog_json: &str,
    geometry_json: &str,
    family_json: &str,
    lat: f64,
    lon: f64,
) -> Result<String, JsValue> {
    chart_for_position_json(catalog_json, geometry_json, family_json, lat, lon)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn derive_chart_page(resource_index_json: &str, plan_json: &str) -> Result<String, JsValue> {
    derive_chart_page_json(resource_index_json, plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn derive_chart_page_state(
    resource_index_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    derive_chart_page_state_json(
        resource_index_json,
        plan_json,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn create_ui_session(
    catalog_json: &str,
    chart_catalog_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    create_ui_session_json(
        catalog_json,
        chart_catalog_json,
        plan_json,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn remove_leg_in_session(handle: u32, index: usize) -> Result<String, JsValue> {
    remove_leg_in_session_json(handle, index).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn move_waypoint_in_session(
    handle: u32,
    waypoint_index: usize,
    delta: isize,
) -> Result<String, JsValue> {
    move_waypoint_in_session_json(handle, waypoint_index, delta)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_airport_in_session(handle: u32, airport_id_json: &str) -> Result<String, JsValue> {
    select_airport_in_session_json(handle, airport_id_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_situation_in_session(handle: u32, situation_json: &str) -> Result<String, JsValue> {
    set_situation_in_session_json(handle, situation_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_chart_in_session(handle: u32, chart_id_json: &str) -> Result<String, JsValue> {
    select_chart_in_session_json(handle, chart_id_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_session_snapshot(handle: u32) -> Result<String, JsValue> {
    get_session_snapshot_json(handle).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn ingest_fix_tiles_in_session(handle: u32, tiles_json: &str) -> Result<(), JsValue> {
    ingest_fix_tiles_in_session_json(handle, tiles_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_map_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, JsValue> {
    get_map_overlay_in_session_json(handle, viewport_json, width_px, height_px)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn restore_chart_page_state_in_session(
    handle: u32,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    restore_chart_page_state_in_session_json(
        handle,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn destroy_session(handle: u32) {
    destroy_session_json(handle)
}

fn load_catalog_json(catalog_json: &str) -> Result<String, String> {
    let handle =
        app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&handle).map_err(|err| err.to_string())
}

fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn remove_flight_plan_leg_json(plan_json: &str, index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::remove_flight_plan_leg(&plan, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn build_flight_plan_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let ui = app_core::build_flight_plan_ui(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&ui).map_err(|err| err.to_string())
}

fn activate_leg_ui_json(plan_json: &str, leg_index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::activate_leg_ui(&plan, leg_index).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn activate_next_leg_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::activate_next_leg_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn suspend_sequencing_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::suspend_sequencing_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn unsuspend_sequencing_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::unsuspend_sequencing_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn sequence_active_leg_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::sequence_active_leg_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn activate_direct_to_leg_ui_json(
    plan_json: &str,
    lat: f64,
    lon: f64,
    target_leg_id: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::activate_direct_to_leg_ui(
        &plan,
        app_core::LatLon { lat, lon },
        target_leg_id,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn insert_airway_from_anchors_ui_json(
    db_path: &str,
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor_json: &str,
    destination_anchor_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let origin_anchor: app_core::NavRef =
        serde_json::from_str(origin_anchor_json).map_err(|err| err.to_string())?;
    let destination_anchor: app_core::NavRef =
        serde_json::from_str(destination_anchor_json).map_err(|err| err.to_string())?;
    let mutation = app_core::insert_airway_from_anchors_ui(
        std::path::Path::new(db_path),
        &plan,
        start_component_index,
        end_component_index,
        airway_name,
        &origin_anchor,
        &destination_anchor,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn replace_airway_from_selection_ui_json(
    db_path: &str,
    plan_json: &str,
    component_index: usize,
    entry_json: &str,
    exit_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let entry: app_core::AirwayEntryCandidate =
        serde_json::from_str(entry_json).map_err(|err| err.to_string())?;
    let exit: app_core::AirwayExitCandidate =
        serde_json::from_str(exit_json).map_err(|err| err.to_string())?;
    let mutation = app_core::replace_airway_from_selection_ui(
        std::path::Path::new(db_path),
        &plan,
        component_index,
        &entry,
        &exit,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn insert_procedure_from_selection_ui_json(
    db_path: &str,
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let runway_transition: Option<String> =
        serde_json::from_str(runway_transition_json).map_err(|err| err.to_string())?;
    let enroute_transition: Option<String> =
        serde_json::from_str(enroute_transition_json).map_err(|err| err.to_string())?;
    let mutation = app_core::insert_procedure_from_selection_ui(
        std::path::Path::new(db_path),
        &plan,
        start_component_index,
        end_component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition.as_deref(),
        enroute_transition.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn replace_procedure_from_selection_ui_json(
    db_path: &str,
    plan_json: &str,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let runway_transition: Option<String> =
        serde_json::from_str(runway_transition_json).map_err(|err| err.to_string())?;
    let enroute_transition: Option<String> =
        serde_json::from_str(enroute_transition_json).map_err(|err| err.to_string())?;
    let mutation = app_core::replace_procedure_from_selection_ui(
        std::path::Path::new(db_path),
        &plan,
        component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition.as_deref(),
        enroute_transition.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn insert_airway_materialized_ui_json(
    plan_json: &str,
    start_component_index: usize,
    end_component_index: usize,
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

fn replace_airway_materialized_ui_json(
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

fn insert_procedure_materialized_ui_json(
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

fn replace_procedure_materialized_ui_json(
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

fn replace_flight_plan_state_json(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::ReplaceFlightPlan(plan),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn replace_flight_plan_ui_state_json(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::ReplaceFlightPlan(plan),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

fn set_content_policy_state_json(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let policy: app_core::ContentPolicy =
        serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::SetContentPolicy(policy),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn set_content_policy_ui_state_json(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let policy: app_core::ContentPolicy =
        serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::SetContentPolicy(policy),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

fn refresh_content_state_json(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let inventory: app_core::ContentInventory =
        serde_json::from_str(inventory_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::RefreshContent { inventory },
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn refresh_content_ui_state_json(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let inventory: app_core::ContentInventory =
        serde_json::from_str(inventory_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::RefreshContent { inventory },
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::project_app_ui_state(&next)).map_err(|err| err.to_string())
}

fn chart_for_position_json(
    catalog_json: &str,
    geometry_json: &str,
    family_json: &str,
    lat: f64,
    lon: f64,
) -> Result<String, String> {
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let geometry: app_core::GeometryBundle =
        serde_json::from_str(geometry_json).map_err(|err| err.to_string())?;
    let family: app_core::ChartFamilyId =
        serde_json::from_str(family_json).map_err(|err| err.to_string())?;
    let chart =
        app_core::chart_for_position(&catalog, &geometry, family, lat, lon).map_err(|err| err.to_string())?;
    serde_json::to_string(&chart).map_err(|err| err.to_string())
}

fn derive_chart_page_json(
    resource_index_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let resource_index =
        app_core::load_resource_index_chart_page_input(resource_index_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let chart_page = app_core::derive_chart_page(&resource_index, &plan);
    serde_json::to_string(&chart_page).map_err(|err| err.to_string())
}

fn derive_chart_page_state_json(
    resource_index_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let resource_index =
        app_core::load_resource_index_chart_page_input(resource_index_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let state = app_core::derive_chart_page_state(
        &resource_index,
        &plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    );
    serde_json::to_string(&state).map_err(|err| err.to_string())
}

fn create_ui_session_json(
    catalog_json: &str,
    chart_catalog_json: &str,
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
        catalog_json,
        chart_catalog_json,
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

fn remove_leg_in_session_json(handle: u32, index: usize) -> Result<String, String> {
    let snapshot = app_core::remove_leg_in_session(handle, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn move_waypoint_in_session_json(
    handle: u32,
    waypoint_index: usize,
    delta: isize,
) -> Result<String, String> {
    let snapshot = app_core::move_waypoint_in_session(handle, waypoint_index, delta)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn select_airport_in_session_json(handle: u32, airport_id_json: &str) -> Result<String, String> {
    let airport_id: String = serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_airport_in_session(handle, &airport_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_situation_in_session_json(handle: u32, situation_json: &str) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::set_situation_in_session(handle, situation).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn select_chart_in_session_json(handle: u32, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_chart_in_session(handle, &chart_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn get_session_snapshot_json(handle: u32) -> Result<String, String> {
    let snapshot = app_core::get_session_snapshot(handle).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn ingest_fix_tiles_in_session_json(handle: u32, tiles_json: &str) -> Result<(), String> {
    let tiles: Vec<app_core::PointTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_fix_tiles_in_session(handle, &tiles).map_err(|err| err.to_string())
}

fn get_map_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_map_overlay_in_session(handle, viewport, width_px, height_px)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

fn restore_chart_page_state_in_session_json(
    handle: u32,
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
        handle,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn destroy_session_json(handle: u32) {
    app_core::destroy_session(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

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

    fn empty_state_json() -> String {
        serde_json::to_string(&app_core::AppState::default()).unwrap()
    }

    fn sample_geometry_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "polygons": [
                {
                    "id": "sectional:boston",
                    "points": [
                        [-72.0, 41.0],
                        [-70.0, 41.0],
                        [-70.0, 43.0],
                        [-72.0, 43.0],
                        [-72.0, 41.0]
                    ]
                }
            ]
        })
        .to_string()
    }

    fn sample_plan_json() -> String {
        serde_json::json!({
            "id": "plan-1",
            "name": "KBOS local",
            "legs": [
                {
                    "from": {"Airport": "KBOS"},
                    "to": {"Airport": "KBOS"},
                    "airway": null
                }
            ],
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

    fn fixture_db_path() -> &'static str {
        static DB_PATH: OnceLock<String> = OnceLock::new();
        DB_PATH
            .get_or_init(|| {
                if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_NAV_DB") {
                    let path = std::path::PathBuf::from(value);
                    if path.is_file() {
                        return path.to_string_lossy().into_owned();
                    }
                }
                for candidate in [
                    "/root/aerobag-three/ui-target-flightplan/android/assets/nav-db/main.db",
                    "/root/aerobag-three/ui-target/android/assets/nav-db/main.db",
                    "/root/aerobag-artifacts/product-builds/shared/work/data/output/main.db",
                ] {
                    let path = std::path::PathBuf::from(candidate);
                    if path.is_file() {
                        return path.to_string_lossy().into_owned();
                    }
                }
                panic!("unable to locate nav database fixture");
            })
            .as_str()
    }

    #[test]
    fn replace_flight_plan_state_json_populates_requirements() {
        let next_json = replace_flight_plan_state_json(
            &empty_state_json(),
            &sample_catalog_json(),
            &sample_plan_json(),
        )
        .unwrap();
        let next: app_core::AppState = serde_json::from_str(&next_json).unwrap();

        assert!(next.active_plan.is_some());
        assert_eq!(next.last_content_requirements.len(), 1);
    }

    #[test]
    fn build_flight_plan_ui_json_returns_projected_plan_view() {
        let ui_json = build_flight_plan_ui_json(&sample_plan_json()).unwrap();
        let ui: app_core::FlightPlanUiState = serde_json::from_str(&ui_json).unwrap();

        assert!(!ui.components.is_empty());
        assert_eq!(ui.components[0].summary, "KBOS");
    }

    #[test]
    fn replace_flight_plan_ui_state_json_returns_projected_app_view() {
        let next_json = replace_flight_plan_ui_state_json(
            &empty_state_json(),
            &sample_catalog_json(),
            &sample_plan_json(),
        )
        .unwrap();
        let next: app_core::AppUiState = serde_json::from_str(&next_json).unwrap();

        assert!(next.active_plan.is_some());
        assert_eq!(next.last_content_requirements.len(), 1);
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

        assert_eq!(next.ui_state.guidance.as_ref().unwrap().active_leg_index, Some(1));
        assert_eq!(next.ui_state.guidance.as_ref().unwrap().active_component_index, Some(1));
    }

    #[test]
    fn insert_airway_from_anchors_ui_json_returns_projected_grouped_edit() {
        let plan_json = serde_json::json!({
            "id": "airway-insert",
            "name": "Airway insert",
            "legs": [],
            "route_components": [
                {"kind":"waypoint","waypoint":{"Airport":"KRNT"}},
                {"kind":"waypoint","waypoint":{"Airport":"KUAO"}},
                {"kind":"waypoint","waypoint":{"Airport":"KHIO"}}
            ],
            "resolved_legs": [],
            "guidance": null,
            "departure": "KRNT",
            "destination": "KHIO",
            "alternate": null,
            "cruise_altitude_ft": null,
            "notes": null,
            "updated_at_epoch_ms": 0,
            "version": 1
        })
        .to_string();

        let next_json = insert_airway_from_anchors_ui_json(
            fixture_db_path(),
            &plan_json,
            0,
            1,
            "V2",
            &serde_json::to_string(&app_core::NavRef::Airport("KRNT".to_string())).unwrap(),
            &serde_json::to_string(&app_core::NavRef::Airport("KUAO".to_string())).unwrap(),
        )
        .unwrap();
        let next: app_core::AirwayPlanUiMutation = serde_json::from_str(&next_json).unwrap();

        assert_eq!(next.mutation.component_index, 1);
        assert!(matches!(next.ui_state.components[1].kind, app_core::RouteComponentViewKind::Airway));
    }

    #[test]
    fn insert_procedure_from_selection_ui_json_returns_projected_grouped_edit() {
        let plan_json = serde_json::json!({
            "id": "procedure-insert",
            "name": "Procedure insert",
            "legs": [],
            "route_components": [
                {"kind":"waypoint","waypoint":{"Fix":"ETX"}},
                {"kind":"waypoint","waypoint":{"Airport":"KBOS"}}
            ],
            "resolved_legs": [],
            "guidance": null,
            "departure": null,
            "destination": "KBOS",
            "alternate": null,
            "cruise_altitude_ft": null,
            "notes": null,
            "updated_at_epoch_ms": 0,
            "version": 1
        })
        .to_string();

        let next_json = insert_procedure_from_selection_ui_json(
            fixture_db_path(),
            &plan_json,
            0,
            1,
            "KBOS",
            "I04R",
            &serde_json::to_string(&app_core::ProcedureKind::Approach).unwrap(),
            &serde_json::to_string(&Option::<String>::None).unwrap(),
            &serde_json::to_string(&Some("GOSHI".to_string())).unwrap(),
        )
        .unwrap();
        let next: app_core::ProcedurePlanUiMutation = serde_json::from_str(&next_json).unwrap();

        assert_eq!(next.mutation.component_index, 1);
        assert!(matches!(next.ui_state.components[1].kind, app_core::RouteComponentViewKind::Procedure));
    }

    #[test]
    fn stream_allowed_policy_survives_json_boundary() {
        let with_plan_json = replace_flight_plan_state_json(
            &empty_state_json(),
            &sample_catalog_json(),
            &sample_plan_json(),
        )
        .unwrap();

        let web_state_json = set_content_policy_state_json(
            &with_plan_json,
            &sample_catalog_json(),
            &serde_json::to_string(&app_core::ContentPolicy::StreamAllowed).unwrap(),
        )
        .unwrap();

        let refreshed_json = refresh_content_state_json(
            &web_state_json,
            &sample_catalog_json(),
            &serde_json::json!({
                "installed_packages": [],
                "cached_tilesets": [],
                "cached_plates": []
            })
            .to_string(),
        )
        .unwrap();

        let refreshed: app_core::AppState = serde_json::from_str(&refreshed_json).unwrap();
        assert!(refreshed.last_content_report.as_ref().unwrap().fully_satisfied);
    }

    #[test]
    fn chart_for_position_json_returns_matching_chart() {
        let chart_json = chart_for_position_json(
            &sample_catalog_json(),
            &sample_geometry_json(),
            &serde_json::to_string(&app_core::ChartFamilyId::Sectional).unwrap(),
            42.0,
            -71.0,
        )
        .unwrap();
        let chart: Option<app_core::ChartRecord> = serde_json::from_str(&chart_json).unwrap();

        assert_eq!(chart.unwrap().display_name, "Boston");
    }

    #[test]
    fn chart_for_position_json_returns_null_outside_coverage() {
        let chart_json = chart_for_position_json(
            &sample_catalog_json(),
            &sample_geometry_json(),
            &serde_json::to_string(&app_core::ChartFamilyId::Sectional).unwrap(),
            35.0,
            -71.0,
        )
        .unwrap();
        let chart: Option<app_core::ChartRecord> = serde_json::from_str(&chart_json).unwrap();

        assert!(chart.is_none());
    }
}
