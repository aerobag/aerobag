pub use app_core::*;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

pub fn load_catalog_json(catalog_json: &str) -> Result<String, String> {
    let handle =
        app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&handle).map_err(|err| err.to_string())
}

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

pub fn build_flight_plan_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let ui = app_core::build_flight_plan_ui(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&ui).map_err(|err| err.to_string())
}

pub fn activate_leg_ui_json(plan_json: &str, leg_index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::activate_leg_ui(&plan, leg_index).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn activate_next_leg_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::activate_next_leg_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn suspend_sequencing_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::suspend_sequencing_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn unsuspend_sequencing_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::unsuspend_sequencing_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn sequence_active_leg_ui_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::sequence_active_leg_ui(&plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

pub fn activate_direct_to_leg_ui_json(
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

pub fn insert_airway_from_anchors_ui_json(
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

pub fn replace_airway_from_selection_ui_json(
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

pub fn insert_procedure_from_selection_ui_json(
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

pub fn replace_procedure_from_selection_ui_json(
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

pub fn replace_flight_plan_state_json(
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

pub fn replace_flight_plan_ui_state_json(
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

pub fn set_content_policy_state_json(
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

pub fn set_content_policy_ui_state_json(
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

pub fn refresh_content_state_json(
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

pub fn refresh_content_ui_state_json(
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

pub fn derive_chart_page_json(
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

pub fn derive_chart_page_state_json(
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

pub fn create_ui_session_json(
    catalog_json: &str,
    resource_index_json: &str,
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
        resource_index_json,
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn remove_leg_in_session_json(handle: u64, index: usize) -> Result<String, String> {
    let snapshot = app_core::remove_leg_in_session(handle as u32, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn move_waypoint_in_session_json(
    handle: u64,
    waypoint_index: usize,
    delta: isize,
) -> Result<String, String> {
    let snapshot = app_core::move_waypoint_in_session(handle as u32, waypoint_index, delta)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_airport_in_session_json(handle: u64, airport_id_json: &str) -> Result<String, String> {
    let airport_id: String = serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_airport_in_session(handle as u32, &airport_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_situation_in_session_json(handle: u64, situation_json: &str) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::set_situation_in_session(handle as u32, situation).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_chart_in_session_json(handle: u64, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_chart_in_session(handle as u32, &chart_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
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
    app_core::ingest_point_tiles_in_session(handle as u32, &tiles).map_err(|err| err.to_string())?;
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
    let overlay = app_core::get_map_overlay_in_session(handle as u32, viewport, width_px, height_px)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn destroy_session_json(handle: u64) {
    app_core::destroy_session(handle as u32);
}

fn get_java_string(env: &mut JNIEnv, value: JString) -> Result<String, String> {
    env.get_string(&value)
        .map(|s| s.into())
        .map_err(|err| err.to_string())
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

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_state_json(&state, &catalog, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_ui_state_json(&state, &catalog, &plan)
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
    catalog_json: JString,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let policy = get_java_string(&mut env, policy_json)?;
        set_content_policy_state_json(&state, &catalog, &policy)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setContentPolicyUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let policy = get_java_string(&mut env, policy_json)?;
        set_content_policy_ui_state_json(&state, &catalog, &policy)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_refreshContentStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    inventory_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let inventory = get_java_string(&mut env, inventory_json)?;
        refresh_content_state_json(&state, &catalog, &inventory)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_refreshContentUiStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    inventory_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let inventory = get_java_string(&mut env, inventory_json)?;
        refresh_content_ui_state_json(&state, &catalog, &inventory)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_deriveChartPageJson(
    mut env: JNIEnv,
    _class: JClass,
    resource_index_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        derive_chart_page_json(&resource_index, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_deriveChartPageStateJson(
    mut env: JNIEnv,
    _class: JClass,
    resource_index_json: JString,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        derive_chart_page_state_json(
            &resource_index,
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_createUiSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    catalog_json: JString,
    resource_index_json: JString,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let catalog = get_java_string(&mut env, catalog_json)?;
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        create_ui_session_json(
            &catalog,
            &resource_index,
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_removeLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    index: i32,
) -> jstring {
    return_string(&mut env, remove_leg_in_session_json(handle as u64, index as usize))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_moveWaypointInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    waypoint_index: i32,
    delta: i32,
) -> jstring {
    return_string(
        &mut env,
        move_waypoint_in_session_json(handle as u64, waypoint_index as usize, delta as isize),
    )
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
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setSituationInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    situation_json: JString,
) -> jstring {
    let result = (|| {
        let situation = get_java_string(&mut env, situation_json)?;
        set_situation_in_session_json(handle as u64, &situation)
    })();
    return_string(&mut env, result)
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
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_destroySession(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    destroy_session_json(handle as u64)
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
                    "tile_path_template": "tiles/{chart_index}/{z}/{x}/{y}"
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

}
