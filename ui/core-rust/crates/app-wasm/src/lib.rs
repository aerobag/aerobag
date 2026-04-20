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
pub fn classify_procedure_identifier(
    identifier: &str,
    exists_as_airport: bool,
    exists_as_navaid: bool,
    exists_as_fix: bool,
) -> Result<String, JsValue> {
    classify_procedure_identifier_json(
        identifier,
        exists_as_airport,
        exists_as_navaid,
        exists_as_fix,
    )
    .map_err(|err| JsValue::from_str(&err))
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
pub fn delete_component_ui(plan_json: &str, component_index: usize) -> Result<String, JsValue> {
    delete_component_ui_json(plan_json, component_index).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn move_component_ui(
    plan_json: &str,
    component_index: usize,
    delta: isize,
) -> Result<String, JsValue> {
    move_component_ui_json(plan_json, component_index, delta).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn insert_waypoint_ui(
    plan_json: &str,
    component_index: usize,
    before: bool,
    waypoint_json: &str,
) -> Result<String, JsValue> {
    insert_waypoint_ui_json(plan_json, component_index, before, waypoint_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn flight_plan_insert_anchor(
    plan_json: &str,
    component_index: usize,
    before: bool,
) -> Result<String, JsValue> {
    flight_plan_insert_anchor_json(plan_json, component_index, before)
        .map_err(|err| JsValue::from_str(&err))
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
    replace_airway_from_selection_ui_json(
        db_path,
        plan_json,
        component_index,
        entry_json,
        exit_json,
    )
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
    end_component_index_json: &str,
    selection_json: &str,
    airway_json: &str,
    resolved_legs_json: &str,
) -> Result<String, JsValue> {
    insert_airway_materialized_ui_json(
        plan_json,
        start_component_index,
        end_component_index_json,
        selection_json,
        airway_json,
        resolved_legs_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn prepare_airway_presentation(
    airway_name: &str,
    branches_json: &str,
    origin_position_json: &str,
    destination_position_json: &str,
) -> Result<String, JsValue> {
    prepare_airway_presentation_json(
        airway_name,
        branches_json,
        origin_position_json,
        destination_position_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn airway_spatial_tile_keys(
    anchor_position_json: &str,
    radius_nm: f64,
) -> Result<String, JsValue> {
    airway_spatial_tile_keys_json(anchor_position_json, radius_nm)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn suggest_airways_near_from_points(
    anchor_position_json: &str,
    points_json: &str,
    limit: usize,
) -> Result<String, JsValue> {
    suggest_airways_near_from_points_json(anchor_position_json, points_json, limit)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn materialize_airway_selection_from_branches(
    start_component_index: usize,
    entry_json: &str,
    exit_json: &str,
    branches_json: &str,
    origin_position_json: &str,
    destination_position_json: &str,
) -> Result<String, JsValue> {
    materialize_airway_selection_from_branches_json(
        start_component_index,
        entry_json,
        exit_json,
        branches_json,
        origin_position_json,
        destination_position_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn sort_airway_suggestions_for_ui(suggestions_json: &str) -> Result<String, JsValue> {
    sort_airway_suggestions_for_ui_json(suggestions_json).map_err(|err| JsValue::from_str(&err))
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
pub fn describe_procedure_options_from_rows(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    rows_json: &str,
) -> Result<String, JsValue> {
    describe_procedure_options_from_rows_json(airport_id, procedure_id, kind_json, rows_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn list_approach_procedures_from_match_rows(
    airport_id: &str,
    rows_json: &str,
) -> Result<String, JsValue> {
    list_approach_procedures_from_match_rows_json(airport_id, rows_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn materialize_procedure_from_records(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
    component_index: usize,
    rows_json: &str,
    legs_json: &str,
) -> Result<String, JsValue> {
    materialize_procedure_from_records_json(
        airport_id,
        procedure_id,
        kind_json,
        runway_transition_json,
        enroute_transition_json,
        component_index,
        rows_json,
        legs_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_preferred_cifp_tpp_match(rows_json: &str) -> Result<String, JsValue> {
    select_preferred_cifp_tpp_match_json(rows_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn describe_show_plate_for_procedure(rows_json: &str) -> Result<String, JsValue> {
    describe_show_plate_for_procedure_json(rows_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn describe_load_procedure_from_plate(
    plan_json: &str,
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    options_json: &str,
) -> Result<String, JsValue> {
    describe_load_procedure_from_plate_json(
        plan_json,
        airport_id,
        procedure_id,
        kind_json,
        options_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn describe_plate_procedure_load_options(
    plan_json: &str,
    candidates_json: &str,
) -> Result<String, JsValue> {
    describe_plate_procedure_load_options_json(plan_json, candidates_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn project_flight_plan_route_from_positions(
    plan_json: &str,
    position_by_key_json: &str,
) -> Result<String, JsValue> {
    project_flight_plan_route_from_positions_json(plan_json, position_by_key_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn suggest_waypoint_identifiers_from_candidates(
    plan_json: &str,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
    candidates_json: &str,
    anchor_position_json: &str,
) -> Result<String, JsValue> {
    suggest_waypoint_identifiers_from_candidates_json(
        plan_json,
        component_index,
        before,
        prefix,
        limit,
        candidates_json,
        anchor_position_json,
    )
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
pub fn derive_chart_page(resource_index_json: &str, plan_json: &str) -> Result<String, JsValue> {
    derive_chart_page_json(resource_index_json, plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn derive_chart_catalog(resource_index_json: &str) -> Result<String, JsValue> {
    derive_chart_catalog_json(resource_index_json).map_err(|err| JsValue::from_str(&err))
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
pub fn register_ownship_source_in_session(
    handle: u32,
    registration_json: &str,
) -> Result<String, JsValue> {
    register_ownship_source_in_session_json(handle, registration_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn update_ownship_source_status_in_session(
    handle: u32,
    update_json: &str,
) -> Result<String, JsValue> {
    update_ownship_source_status_in_session_json(handle, update_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn push_situation_sample_in_session(handle: u32, sample_json: &str) -> Result<String, JsValue> {
    push_situation_sample_in_session_json(handle, sample_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_ownship_source_in_session(
    handle: u32,
    selection_json: &str,
) -> Result<String, JsValue> {
    select_ownship_source_in_session_json(handle, selection_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_situation_in_session(handle: u32, situation_json: &str) -> Result<String, JsValue> {
    set_situation_in_session_json(handle, situation_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn engage_map_follow_in_session(handle: u32, viewport_json: &str) -> Result<String, JsValue> {
    engage_map_follow_in_session_json(handle, viewport_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn disengage_map_follow_in_session(
    handle: u32,
    viewport_json: &str,
) -> Result<String, JsValue> {
    disengage_map_follow_in_session_json(handle, viewport_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_map_follow_offset_in_session(
    handle: u32,
    viewport_json: &str,
    offset_x_px: f64,
    offset_y_px: f64,
) -> Result<String, JsValue> {
    set_map_follow_offset_in_session_json(handle, viewport_json, offset_x_px, offset_y_px)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn sync_map_follow_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, JsValue> {
    sync_map_follow_in_session_json(handle, viewport_json, width_px, height_px)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_guidance_leg_geometry_in_session(
    handle: u32,
    geometries_json: &str,
) -> Result<String, JsValue> {
    set_guidance_leg_geometry_in_session_json(handle, geometries_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn load_playback_trace_in_session(
    handle: u32,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, JsValue> {
    load_playback_trace_in_session_json(handle, source_path_json, trace_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn play_playback_in_session(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    play_playback_in_session_json(handle, now_epoch_ms).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn pause_playback_in_session(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    pause_playback_in_session_json(handle, now_epoch_ms).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn seek_playback_in_session(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    seek_playback_in_session_json(handle, cursor_seconds, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_playback_rate_in_session(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    set_playback_rate_in_session_json(handle, rate, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn tick_playback_in_session(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    tick_playback_in_session_json(handle, now_epoch_ms).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_flight_plan_in_session(handle: u32, plan_json: &str) -> Result<String, JsValue> {
    replace_flight_plan_in_session_json(handle, plan_json).map_err(|err| JsValue::from_str(&err))
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
pub fn ingest_point_tiles_in_session(handle: u32, tiles_json: &str) -> Result<(), JsValue> {
    ingest_point_tiles_in_session_json(handle, tiles_json).map_err(|err| JsValue::from_str(&err))
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
pub fn get_terrain_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, JsValue> {
    get_terrain_overlay_in_session_json(handle, viewport_json, width_px, height_px)
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

#[wasm_bindgen]
pub fn render_terrain_overlay_tile_in_session(
    handle: u32,
    terrain_tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, JsValue> {
    app_core::render_terrain_overlay_tile_in_session(
        handle,
        terrain_tile_bytes,
        aircraft_altitude_ft
            .is_finite()
            .then_some(aircraft_altitude_ft),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn render_terrain_overlay_tiles_in_session(
    handle: u32,
    packed_terrain_tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, JsValue> {
    app_core::render_terrain_overlay_tiles_in_session(
        handle,
        packed_terrain_tile_bytes,
        aircraft_altitude_ft
            .is_finite()
            .then_some(aircraft_altitude_ft),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn render_terrain_warning_raw_rgba(
    terrain_tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, JsValue> {
    app_core::render_terrain_warning_raw_rgba_from_tiles(
        &[terrain_tile_bytes],
        aircraft_altitude_ft,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn render_terrain_warning_raw_rgba_from_packed_tiles(
    packed_terrain_tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, JsValue> {
    let tile_bytes = unpack_packed_terrain_tile_bytes(packed_terrain_tile_bytes)
        .map_err(|err| JsValue::from_str(&err))?;
    let tile_refs = tile_bytes
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    app_core::render_terrain_warning_raw_rgba_from_tiles(
        &tile_refs,
        aircraft_altitude_ft,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

fn unpack_packed_terrain_tile_bytes(packed_terrain_tile_bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if packed_terrain_tile_bytes.len() < 4 {
        return Err("packed terrain tile bytes missing count".to_string());
    }
    let mut cursor = 0;
    let read_u32 = |bytes: &[u8], cursor: &mut usize| -> Result<u32, String> {
        let end = *cursor + 4;
        let chunk = bytes
            .get(*cursor..end)
            .ok_or_else(|| "packed terrain tile bytes truncated".to_string())?;
        *cursor = end;
        Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    };
    let count = read_u32(packed_terrain_tile_bytes, &mut cursor)? as usize;
    let mut lengths = Vec::with_capacity(count);
    for _ in 0..count {
        lengths.push(read_u32(packed_terrain_tile_bytes, &mut cursor)? as usize);
    }
    let mut tiles = Vec::with_capacity(count);
    for length in lengths {
        let end = cursor + length;
        let tile = packed_terrain_tile_bytes
            .get(cursor..end)
            .ok_or_else(|| "packed terrain tile payload truncated".to_string())?
            .to_vec();
        cursor = end;
        tiles.push(tile);
    }
    Ok(tiles)
}

fn load_catalog_json(catalog_json: &str) -> Result<String, String> {
    let handle = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&handle).map_err(|err| err.to_string())
}

fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn flight_plan_insert_anchor_json(
    plan_json: &str,
    component_index: usize,
    before: bool,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let anchor = component_insert_anchor(&plan, component_index, before)?;
    serde_json::to_string(&anchor).map_err(|err| err.to_string())
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct AirwaySpatialPoint {
    airway_name: String,
    branch_key: String,
    sequence: i32,
    position: app_core::LatLon,
    nav_ref: app_core::NavRef,
}

#[derive(serde::Serialize)]
struct MaterializedAirwayResponse {
    selection: app_core::AirwayAutoSelection,
    airway: app_core::AirwaySegment,
    #[serde(rename = "resolvedLegs")]
    resolved_legs: Vec<app_core::ResolvedLeg>,
}

fn airway_spatial_tile_keys_json(
    anchor_position_json: &str,
    radius_nm: f64,
) -> Result<String, String> {
    let anchor_position: app_core::LatLon =
        serde_json::from_str(anchor_position_json).map_err(|err| err.to_string())?;
    let bounds = search_bounds(anchor_position, radius_nm);
    let min_lat = bounds.min_lat.floor() as i32;
    let max_lat = bounds.max_lat.floor() as i32;
    let min_lon = bounds.min_lon.floor() as i32;
    let max_lon = bounds.max_lon.floor() as i32;
    let mut keys = Vec::new();
    for lat_tile in min_lat..=max_lat {
        for lon_tile in min_lon..=max_lon {
            keys.push(format!("airway/spatial/{lat_tile}/{lon_tile}"));
        }
    }
    serde_json::to_string(&keys).map_err(|err| err.to_string())
}

fn suggest_airways_near_from_points_json(
    anchor_position_json: &str,
    points_json: &str,
    limit: usize,
) -> Result<String, String> {
    if limit == 0 {
        return Ok("[]".to_string());
    }
    let anchor_position: app_core::LatLon =
        serde_json::from_str(anchor_position_json).map_err(|err| err.to_string())?;
    let points: Vec<AirwaySpatialPoint> =
        serde_json::from_str(points_json).map_err(|err| err.to_string())?;
    let mut seen = std::collections::HashMap::<String, app_core::AirwaySuggestion>::new();
    for point in points {
        let distance_from_anchor_nm =
            app_core::flight_leg_distance_nm(anchor_position, point.position);
        let suggestion = app_core::AirwaySuggestion {
            airway_name: point.airway_name.clone(),
            nearest_branch_key: Some(point.branch_key),
            nearest_nav_ref: point.nav_ref,
            nearest_sequence: point.sequence,
            distance_from_anchor_nm,
        };
        match seen.get(&point.airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(point.airway_name, suggestion);
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
    serde_json::to_string(&suggestions).map_err(|err| err.to_string())
}

fn materialize_airway_selection_from_branches_json(
    start_component_index: usize,
    entry_json: &str,
    exit_json: &str,
    branches_json: &str,
    origin_position_json: &str,
    destination_position_json: &str,
) -> Result<String, String> {
    let entry: app_core::AirwayEntryCandidate =
        serde_json::from_str(entry_json).map_err(|err| err.to_string())?;
    let exit: app_core::AirwayExitCandidate =
        serde_json::from_str(exit_json).map_err(|err| err.to_string())?;
    let branches: Vec<app_core::AirwayBranch> =
        serde_json::from_str(branches_json).map_err(|err| err.to_string())?;
    let origin_position: app_core::LatLon =
        serde_json::from_str(origin_position_json).map_err(|err| err.to_string())?;
    let destination_position: Option<app_core::LatLon> =
        serde_json::from_str(destination_position_json).map_err(|err| err.to_string())?;
    let (airway, resolved_legs) =
        materialize_airway_from_branches(start_component_index, &entry, &exit, &branches)?;
    let entry_position = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .and_then(|branch| branch.points.get(entry.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| "selected airway entry point is not on branch".to_string())?;
    let exit_position = branches
        .iter()
        .find(|branch| branch.branch_key == exit.branch_key)
        .and_then(|branch| branch.points.get(exit.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| "selected airway exit point is not on branch".to_string())?;
    let origin_distance_nm = app_core::flight_leg_distance_nm(origin_position, entry_position);
    let destination_distance_nm = destination_position
        .map(|position| app_core::flight_leg_distance_nm(position, exit_position))
        .unwrap_or(0.0);
    let response = MaterializedAirwayResponse {
        selection: app_core::AirwayAutoSelection {
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

fn materialize_airway_from_branches(
    component_index: usize,
    entry: &app_core::AirwayEntryCandidate,
    exit: &app_core::AirwayExitCandidate,
    branches: &[app_core::AirwayBranch],
) -> Result<(app_core::AirwaySegment, Vec<app_core::ResolvedLeg>), String> {
    if entry.airway_name != exit.airway_name || entry.branch_key != exit.branch_key {
        return Err(format!(
            "entry airway {} branch {} does not match exit airway {} branch {}",
            entry.airway_name, entry.branch_key, exit.airway_name, exit.branch_key
        ));
    }
    let branch = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .ok_or_else(|| {
            format!(
                "unknown airway branch {} {}",
                entry.airway_name, entry.branch_key
            )
        })?;
    let entry_point = branch.points.get(entry.branch_point_index).ok_or_else(|| {
        format!(
            "entry index {} is out of bounds for airway {} branch {}",
            entry.branch_point_index, entry.airway_name, entry.branch_key
        )
    })?;
    let exit_point = branch.points.get(exit.branch_point_index).ok_or_else(|| {
        format!(
            "exit index {} is out of bounds for airway {} branch {}",
            exit.branch_point_index, exit.airway_name, exit.branch_key
        )
    })?;
    if entry.branch_point_index == exit.branch_point_index {
        return Err("airway entry and exit cannot be the same point".to_string());
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
        .map(|(index, pair)| app_core::ResolvedLeg {
            id: format!("airway-{}-{index}", branch.branch_key),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: app_core::ResolvedLegSource::RouteComponent { component_index },
            procedure_provenance: None,
        })
        .collect::<Vec<_>>();
    Ok((
        app_core::AirwaySegment {
            name: branch.display_name.clone(),
            branch_key: Some(branch.branch_key.clone()),
            entry: entry_point.nav_ref.clone(),
            exit: exit_point.nav_ref.clone(),
        },
        resolved_legs,
    ))
}

#[derive(Debug, Clone, Copy)]
struct SearchBounds {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

fn search_bounds(anchor: app_core::LatLon, radius_nm: f64) -> SearchBounds {
    let lat_delta = radius_nm / 60.0;
    let lon_delta = radius_nm / (60.0 * anchor.lat.to_radians().cos().abs().max(0.1));
    SearchBounds {
        min_lat: anchor.lat - lat_delta,
        max_lat: anchor.lat + lat_delta,
        min_lon: anchor.lon - lon_delta,
        max_lon: anchor.lon + lon_delta,
    }
}

#[derive(serde::Deserialize)]
struct WaypointIdentifierCandidate {
    identifier: String,
    nav_ref: app_core::NavRef,
    kind: String,
    city: String,
    state: String,
    facility_name: String,
    position: app_core::LatLon,
}

fn suggest_waypoint_identifiers_from_candidates_json(
    plan_json: &str,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
    candidates_json: &str,
    anchor_position_json: &str,
) -> Result<String, String> {
    if limit == 0 {
        return Ok("[]".to_string());
    }
    let prefix = prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() {
        return Ok("[]".to_string());
    }
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let _anchor = component_insert_anchor(&plan, component_index, before)?;
    let anchor_position: app_core::LatLon =
        serde_json::from_str(anchor_position_json).map_err(|err| err.to_string())?;
    let mut suggestions = serde_json::from_str::<Vec<WaypointIdentifierCandidate>>(candidates_json)
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter(|candidate| {
            candidate
                .identifier
                .trim()
                .to_ascii_uppercase()
                .starts_with(&prefix)
        })
        .map(|candidate| app_core::WaypointIdentifierSuggestion {
            identifier: candidate.identifier,
            nav_ref: candidate.nav_ref,
            kind: candidate.kind.clone(),
            display_name: waypoint_identifier_display_name(
                &candidate.kind,
                &candidate.city,
                &candidate.state,
                &candidate.facility_name,
            ),
            distance_from_anchor_nm: app_core::flight_leg_distance_nm(
                anchor_position,
                candidate.position,
            ),
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
    serde_json::to_string(&suggestions).map_err(|err| err.to_string())
}

fn project_flight_plan_route_from_positions_json(
    plan_json: &str,
    position_by_key_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    let position_by_key: std::collections::HashMap<String, app_core::LatLon> =
        serde_json::from_str(position_by_key_json).map_err(|err| err.to_string())?;
    let ui_state = app_core::project_ui_state(&plan);
    let route = plan
        .resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| {
            let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
                (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
            });
            let from = position_for_nav_ref(&leg.from, procedure_airport_id, &position_by_key)?;
            let to = position_for_nav_ref(&leg.to, procedure_airport_id, &position_by_key)?;
            Ok(app_core::FlightPlanRouteSegment {
                id: leg.id.clone(),
                from,
                to,
                distance_nm: app_core::flight_leg_distance_nm(from, to),
                course_deg: app_core::flight_leg_course_deg(from, to),
                status: route_status_for_leg(&ui_state, leg_index),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::to_string(&route).map_err(|err| err.to_string())
}

fn position_for_nav_ref(
    nav_ref: &app_core::NavRef,
    procedure_airport_id: Option<&str>,
    position_by_key: &std::collections::HashMap<String, app_core::LatLon>,
) -> Result<app_core::LatLon, String> {
    if let app_core::NavRef::LatLon(position) = nav_ref {
        return Ok(*position);
    }
    let key = navref_position_key(nav_ref, procedure_airport_id)?;
    position_by_key
        .get(&key)
        .copied()
        .ok_or_else(|| format!("HAD missing required navref position key: {key}"))
}

fn navref_position_key(
    nav_ref: &app_core::NavRef,
    procedure_airport_id: Option<&str>,
) -> Result<String, String> {
    match nav_ref {
        app_core::NavRef::Airport(code) => Ok(format!(
            "navref/position/airport/{}",
            had_upper_key_component(code)
        )),
        app_core::NavRef::Navaid(code) => Ok(format!(
            "navref/position/navaid/{}",
            had_upper_key_component(code)
        )),
        app_core::NavRef::Fix(code)
            if procedure_airport_id.is_some()
                && code.trim().to_ascii_uppercase().starts_with("RW") =>
        {
            Ok(format!(
                "navref/position/runway/{}/{}",
                had_upper_key_component(procedure_airport_id.unwrap_or_default()),
                had_upper_key_component(code),
            ))
        }
        app_core::NavRef::Fix(code) => Ok(format!(
            "navref/position/fix/{}",
            had_upper_key_component(code)
        )),
        app_core::NavRef::LatLon(_) => {
            Err("LatLon nav refs do not have HAD position keys".to_string())
        }
    }
}

fn had_upper_key_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.trim().to_ascii_uppercase().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn route_status_for_leg(
    ui_state: &app_core::FlightPlanUiState,
    leg_index: usize,
) -> app_core::FlightPlanRouteSegmentStatus {
    let Some(guidance) = ui_state.guidance.as_ref() else {
        return app_core::FlightPlanRouteSegmentStatus::Remaining;
    };
    if let Some(active_leg_index) = guidance.active_leg_index {
        return if leg_index < active_leg_index {
            app_core::FlightPlanRouteSegmentStatus::Completed
        } else if leg_index == active_leg_index {
            app_core::FlightPlanRouteSegmentStatus::Active
        } else {
            app_core::FlightPlanRouteSegmentStatus::Remaining
        };
    }
    app_core::FlightPlanRouteSegmentStatus::Remaining
}

fn component_insert_anchor(
    plan: &app_core::FlightPlan,
    component_index: usize,
    before: bool,
) -> Result<app_core::NavRef, String> {
    let plan = plan.clone().normalized();
    let component = plan
        .route_components
        .get(component_index)
        .ok_or_else(|| format!("component index out of bounds: {component_index}"))?;
    let waypoint = match component {
        app_core::RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        app_core::RouteComponent::Airway { airway } => {
            if before {
                Some(airway.entry.clone())
            } else {
                Some(airway.exit.clone())
            }
        }
        app_core::RouteComponent::Procedure { .. } => {
            let mut legs = plan.resolved_legs.iter().filter(|leg| {
                matches!(
                    leg.source,
                    app_core::ResolvedLegSource::RouteComponent { component_index: index } if index == component_index
                )
            });
            if before {
                legs.next().map(|leg| leg.from.clone())
            } else {
                legs.last().map(|leg| leg.to.clone())
            }
        }
    };
    waypoint.ok_or_else(|| "selected component has no waypoint anchor".to_string())
}

fn nav_ref_kind_order(nav_ref: &app_core::NavRef) -> usize {
    match nav_ref {
        app_core::NavRef::Navaid(_) => 0,
        app_core::NavRef::Airport(_) => 1,
        app_core::NavRef::Fix(_) => 2,
        app_core::NavRef::LatLon(_) => 3,
    }
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

fn remove_flight_plan_leg_json(plan_json: &str, index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::remove_flight_plan_leg(&plan, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
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

fn delete_component_ui_json(plan_json: &str, component_index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation =
        app_core::delete_component_ui(&plan, component_index).map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn move_component_ui_json(
    plan_json: &str,
    component_index: usize,
    delta: isize,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation = app_core::move_component_ui(&plan, component_index, delta)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&mutation).map_err(|err| err.to_string())
}

fn insert_waypoint_ui_json(
    plan_json: &str,
    component_index: usize,
    before: bool,
    waypoint_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let waypoint: app_core::NavRef =
        serde_json::from_str(waypoint_json).map_err(|err| err.to_string())?;
    let mutation = app_core::insert_waypoint_ui(&plan, component_index, before, waypoint)
        .map_err(|err| err.to_string())?;
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
    let mutation =
        app_core::activate_direct_to_leg_ui(&plan, app_core::LatLon { lat, lon }, target_leg_id)
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
    end_component_index_json: &str,
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
    let end_component_index: Option<usize> =
        serde_json::from_str(end_component_index_json).map_err(|err| err.to_string())?;
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

fn prepare_airway_presentation_json(
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

fn sort_airway_suggestions_for_ui_json(suggestions_json: &str) -> Result<String, String> {
    let suggestions: Vec<app_core::AirwaySuggestion> =
        serde_json::from_str(suggestions_json).map_err(|err| err.to_string())?;
    let sorted = app_core::sort_airway_suggestions_for_ui(suggestions);
    serde_json::to_string(&sorted).map_err(|err| err.to_string())
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

fn describe_procedure_options_from_rows_json(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    rows_json: &str,
) -> Result<String, String> {
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let rows: Vec<app_core::ProcedureDistinctRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let options =
        app_core::describe_procedure_options_from_rows(airport_id, procedure_id, kind, rows)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&options).map_err(|err| err.to_string())
}

fn list_approach_procedures_from_match_rows_json(
    airport_id: &str,
    rows_json: &str,
) -> Result<String, String> {
    let rows: Vec<app_core::CifpTppMatchRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let procedures = app_core::list_approach_procedures_from_match_rows(airport_id, rows)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&procedures).map_err(|err| err.to_string())
}

fn materialize_procedure_from_records_json(
    airport_id: &str,
    procedure_id: &str,
    kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
    component_index: usize,
    rows_json: &str,
    legs_json: &str,
) -> Result<String, String> {
    let kind: app_core::ProcedureKind =
        serde_json::from_str(kind_json).map_err(|err| err.to_string())?;
    let runway_transition: Option<String> =
        serde_json::from_str(runway_transition_json).map_err(|err| err.to_string())?;
    let enroute_transition: Option<String> =
        serde_json::from_str(enroute_transition_json).map_err(|err| err.to_string())?;
    let rows: Vec<app_core::ProcedureDistinctRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let legs: Vec<app_core::ProcedureLegMaterializationRecord> =
        serde_json::from_str(legs_json).map_err(|err| err.to_string())?;
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

fn select_preferred_cifp_tpp_match_json(rows_json: &str) -> Result<String, String> {
    let rows: Vec<app_core::CifpTppMatchRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let matched = app_core::select_preferred_cifp_tpp_match(rows);
    serde_json::to_string(&matched).map_err(|err| err.to_string())
}

fn describe_show_plate_for_procedure_json(rows_json: &str) -> Result<String, String> {
    let rows: Vec<app_core::CifpTppMatchRow> =
        serde_json::from_str(rows_json).map_err(|err| err.to_string())?;
    let matched = app_core::describe_show_plate_for_procedure(rows);
    serde_json::to_string(&matched).map_err(|err| err.to_string())
}

fn describe_load_procedure_from_plate_json(
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

fn describe_plate_procedure_load_options_json(
    plan_json: &str,
    candidates_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let candidates: Vec<app_core::PlateProcedureLoadCandidateInput> =
        serde_json::from_str(candidates_json).map_err(|err| err.to_string())?;
    let described = app_core::describe_plate_procedure_load_options(&plan, candidates)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&described).map_err(|err| err.to_string())
}

fn classify_procedure_identifier_json(
    identifier: &str,
    exists_as_airport: bool,
    exists_as_navaid: bool,
    exists_as_fix: bool,
) -> Result<String, String> {
    let nav_ref = app_core::classify_procedure_identifier(
        identifier,
        exists_as_airport,
        exists_as_navaid,
        exists_as_fix,
    );
    serde_json::to_string(&nav_ref).map_err(|err| err.to_string())
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

fn derive_chart_page_json(resource_index_json: &str, plan_json: &str) -> Result<String, String> {
    let resource_index = app_core::load_resource_index_chart_page_input(resource_index_json)
        .map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let chart_page = app_core::derive_chart_page(&resource_index, &plan);
    serde_json::to_string(&chart_page).map_err(|err| err.to_string())
}

fn derive_chart_catalog_json(resource_index_json: &str) -> Result<String, String> {
    let resource_index = app_core::load_resource_index_chart_page_input(resource_index_json)
        .map_err(|err| err.to_string())?;
    let chart_catalog = app_core::build_chart_catalog(&resource_index);
    serde_json::to_string(&chart_catalog).map_err(|err| err.to_string())
}

fn derive_chart_page_state_json(
    resource_index_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let resource_index = app_core::load_resource_index_chart_page_input(resource_index_json)
        .map_err(|err| err.to_string())?;
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
    let airport_id: String =
        serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_airport_in_session(handle, &airport_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn register_ownship_source_in_session_json(
    handle: u32,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::register_ownship_source_in_session(handle, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn update_ownship_source_status_in_session_json(
    handle: u32,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::update_ownship_source_status_in_session(handle, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn push_situation_sample_in_session_json(handle: u32, sample_json: &str) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::push_situation_sample_in_session(handle, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn select_ownship_source_in_session_json(
    handle: u32,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_ownship_source_in_session(handle, selection)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_situation_in_session_json(handle: u32, situation_json: &str) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::set_situation_in_session(handle, situation).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn engage_map_follow_in_session_json(handle: u32, viewport_json: &str) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let update =
        app_core::engage_map_follow_in_session(handle, viewport).map_err(|err| err.to_string())?;
    serde_json::to_string(&update).map_err(|err| err.to_string())
}

fn disengage_map_follow_in_session_json(
    handle: u32,
    viewport_json: &str,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::disengage_map_follow_in_session(handle, viewport)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_map_follow_offset_in_session_json(
    handle: u32,
    viewport_json: &str,
    offset_x_px: f64,
    offset_y_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::set_map_follow_offset_in_session(handle, viewport, offset_x_px, offset_y_px)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn sync_map_follow_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::sync_map_follow_in_session(handle, viewport, width_px, height_px)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_guidance_leg_geometry_in_session_json(
    handle: u32,
    geometries_json: &str,
) -> Result<String, String> {
    let geometries: Vec<app_core::GuidanceLegGeometry> =
        serde_json::from_str(geometries_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_guidance_leg_geometry_in_session(handle, geometries)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn load_playback_trace_in_session_json(
    handle: u32,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, String> {
    let source_path: String =
        serde_json::from_str(source_path_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::load_playback_trace_in_session(handle, &source_path, trace_json)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn play_playback_in_session_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot =
        app_core::play_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn pause_playback_in_session_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot =
        app_core::pause_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn seek_playback_in_session_json(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let snapshot = app_core::seek_playback_in_session(handle, cursor_seconds, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_playback_rate_in_session_json(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let snapshot = app_core::set_playback_rate_in_session(handle, rate, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn tick_playback_in_session_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot =
        app_core::tick_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn replace_flight_plan_in_session_json(handle: u32, plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::replace_flight_plan_in_session(handle, plan).map_err(|err| err.to_string())?;
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

fn ingest_point_tiles_in_session_json(handle: u32, tiles_json: &str) -> Result<(), String> {
    let tiles: Vec<app_core::PointTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_point_tiles_in_session(handle, &tiles).map_err(|err| err.to_string())
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

fn get_terrain_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_terrain_overlay_in_session(handle, viewport, width_px, height_px)
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
                    "id": "sec",
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
                        "family": "sec",
                        "cycle": "2026-04-16"
                    },
                    "package_name": "NE_SEC",
                    "family_id": "sec",
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
                        "family": "sec",
                        "name": "Boston",
                        "cycle": "2026-04-16"
                    },
                    "family_id": "sec",
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
                ] {
                    let path = std::path::PathBuf::from(candidate);
                    if path.is_file() {
                        return path.to_string_lossy().into_owned();
                    }
                }
                for root in [
                    "/root/aerobag-artifacts/published-unpacked",
                    "/root/aerobag-artifacts/cache/nodes",
                    "/root/aerobag-artifacts/private-work",
                ] {
                    if let Some(path) = find_fixture_nav_db(std::path::Path::new(root)) {
                        return path.to_string_lossy().into_owned();
                    }
                }
                panic!("unable to locate nav database fixture");
            })
            .as_str()
    }

    fn find_fixture_nav_db(root: &std::path::Path) -> Option<std::path::PathBuf> {
        let entries = std::fs::read_dir(root).ok()?;
        for entry in entries {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = find_fixture_nav_db(&path) {
                    return Some(found);
                }
                continue;
            }
            if path.file_name().is_some_and(|name| name == "main.db")
                && path
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .is_some_and(|name| name == "output" || name == "data_2604")
            {
                return Some(path);
            }
        }
        None
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
        assert!(matches!(
            next.ui_state.components[1].kind,
            app_core::RouteComponentViewKind::Airway
        ));
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
        assert!(matches!(
            next.ui_state.components[1].kind,
            app_core::RouteComponentViewKind::Procedure
        ));
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
        assert!(
            refreshed
                .last_content_report
                .as_ref()
                .unwrap()
                .fully_satisfied
        );
    }
}
