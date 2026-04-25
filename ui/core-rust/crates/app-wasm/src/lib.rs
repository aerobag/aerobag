use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

use serde::Serialize;
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
}

#[derive(Serialize)]
struct ProfileTiming {
    label: &'static str,
    elapsed_ms: f64,
}

#[derive(Serialize)]
struct ProfiledResult<T> {
    result: T,
    timings: Vec<ProfileTiming>,
}

struct Profiler {
    last: f64,
    timings: Vec<ProfileTiming>,
}

impl Profiler {
    fn new() -> Self {
        Self {
            last: now_ms(),
            timings: Vec::new(),
        }
    }

    fn mark(&mut self, label: &'static str) {
        let now = now_ms();
        self.timings.push(ProfileTiming {
            label,
            elapsed_ms: now - self.last,
        });
        self.last = now;
    }
}

fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        date_now()
    }
}

static NEXT_NAV_KV_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_KV_STORES: OnceLock<Mutex<HashMap<u32, app_core::NavKvStore>>> = OnceLock::new();

fn nav_kv_stores() -> &'static Mutex<HashMap<u32, app_core::NavKvStore>> {
    NAV_KV_STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn nav_kv_open(root_bytes: &[u8]) -> Result<u32, JsValue> {
    let root = app_core::NavKvRoot::parse(root_bytes).map_err(|err| JsValue::from_str(&err))?;
    let handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    nav_kv_stores()
        .lock()
        .expect("nav kv store poisoned")
        .insert(handle, app_core::NavKvStore::new(root));
    Ok(handle)
}

#[wasm_bindgen]
pub fn nav_kv_insert_page(handle: u32, page_index: u32, page_bytes: &[u8]) -> Result<(), JsValue> {
    let mut stores = nav_kv_stores().lock().expect("nav kv store poisoned");
    let store = stores
        .get_mut(&handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {handle}")))?;
    store.insert_page(page_index, page_bytes.to_vec());
    Ok(())
}

#[wasm_bindgen]
pub fn nav_kv_destroy(handle: u32) {
    let _ = nav_kv_stores()
        .lock()
        .expect("nav kv store poisoned")
        .remove(&handle);
}

#[wasm_bindgen]
pub fn core_had_operation(nav_kv_handle: u32, operation_json: &str) -> Result<String, JsValue> {
    let operation: app_core::HadOperation =
        serde_json::from_str(operation_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let stores = nav_kv_stores().lock().expect("nav kv store poisoned");
    let store = stores
        .get(&nav_kv_handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {nav_kv_handle}")))?;
    let outcome = app_core::run_had_operation(store, operation)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn situation_ring_candidates_json() -> Result<String, JsValue> {
    serde_json::to_string(&app_core::situation_ring_candidates())
        .map_err(|err| JsValue::from_str(&err.to_string()))
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
pub fn remove_all_above_ui(plan_json: &str, component_index: usize) -> Result<String, JsValue> {
    remove_all_above_ui_json(plan_json, component_index).map_err(|err| JsValue::from_str(&err))
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
pub fn create_ui_session(
    vector_manifest_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    create_ui_session_json(
        vector_manifest_json,
        plan_json,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn create_ui_session_profiled(
    vector_manifest_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    create_ui_session_profiled_json(
        vector_manifest_json,
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
pub fn set_map_layer_visibility_in_session(
    handle: u32,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, JsValue> {
    set_map_layer_visibility_in_session_json(handle, layer_id_json, visible)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_map_layer_enabled_in_session(
    handle: u32,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, JsValue> {
    set_map_layer_enabled_in_session_json(handle, layer_id_json, enabled)
        .map_err(|err| JsValue::from_str(&err))
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
pub fn ingest_airspace_ref_tiles_in_session(handle: u32, tiles_json: &str) -> Result<(), JsValue> {
    ingest_airspace_ref_tiles_in_session_json(handle, tiles_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn ingest_airspace_features_in_session(
    handle: u32,
    features_json: &str,
) -> Result<(), JsValue> {
    ingest_airspace_features_in_session_json(handle, features_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn ingest_airspace_label_tiles_in_session(
    handle: u32,
    tiles_json: &str,
) -> Result<(), JsValue> {
    ingest_airspace_label_tiles_in_session_json(handle, tiles_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn ingest_tfrs_in_session(handle: u32, payload_json: &str) -> Result<(), JsValue> {
    ingest_tfrs_in_session_json(handle, payload_json).map_err(|err| JsValue::from_str(&err))
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
    let tile_refs = tile_bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
    app_core::render_terrain_warning_raw_rgba_from_tiles(&tile_refs, aircraft_altitude_ft)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

fn unpack_packed_terrain_tile_bytes(
    packed_terrain_tile_bytes: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
        let end = *cursor + 4;
        let chunk = bytes
            .get(*cursor..end)
            .ok_or_else(|| "packed terrain tile bytes truncated".to_string())?;
        *cursor = end;
        Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    }

    let mut cursor = 0;
    let count = read_u32(packed_terrain_tile_bytes, &mut cursor)? as usize;
    let mut tiles = Vec::with_capacity(count);
    for _ in 0..count {
        let length = read_u32(packed_terrain_tile_bytes, &mut cursor)? as usize;
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

fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
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

fn remove_all_above_ui_json(plan_json: &str, component_index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let mutation =
        app_core::remove_all_above_ui(&plan, component_index).map_err(|err| err.to_string())?;
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

fn create_ui_session_json(
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

fn create_ui_session_profiled_json(
    vector_manifest_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let mut profiler = Profiler::new();
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    profiler.mark("parse_plan_json");
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    profiler.mark("parse_recent_airports_json");
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    profiler.mark("parse_selected_ids_json");
    let result = app_core::create_ui_session_profiled(
        vector_manifest_json,
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
        &mut |label| profiler.mark(label),
    )
    .map_err(|err| err.to_string())?;
    profiler.mark("app_core_create_ui_session_total");
    let envelope = ProfiledResult {
        result,
        timings: profiler.timings,
    };
    serde_json::to_string(&envelope).map_err(|err| err.to_string())
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

fn set_map_layer_visibility_in_session_json(
    handle: u32,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, String> {
    let layer_id: String = serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_map_layer_visibility_in_session(handle, &layer_id, visible)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_map_layer_enabled_in_session_json(
    handle: u32,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let layer_id: String = serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_map_layer_enabled_in_session(handle, &layer_id, enabled)
        .map_err(|err| err.to_string())?;
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

fn ingest_airspace_ref_tiles_in_session_json(handle: u32, tiles_json: &str) -> Result<(), String> {
    let tiles: Vec<app_core::AirspaceReferenceTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_ref_tiles_in_session(handle, &tiles).map_err(|err| err.to_string())
}

fn ingest_airspace_features_in_session_json(
    handle: u32,
    features_json: &str,
) -> Result<(), String> {
    let features: Vec<app_core::AirspaceFeaturePayload> =
        serde_json::from_str(features_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_features_in_session(handle, &features).map_err(|err| err.to_string())
}

fn ingest_airspace_label_tiles_in_session_json(
    handle: u32,
    tiles_json: &str,
) -> Result<(), String> {
    let tiles: Vec<app_core::AirspaceLabelTilePayload> =
        serde_json::from_str(tiles_json).map_err(|err| err.to_string())?;
    app_core::ingest_airspace_label_tiles_in_session(handle, &tiles).map_err(|err| err.to_string())
}

fn ingest_tfrs_in_session_json(handle: u32, payload_json: &str) -> Result<(), String> {
    let payload: app_core::TfrProductPayload =
        serde_json::from_str(payload_json).map_err(|err| err.to_string())?;
    app_core::ingest_tfrs_in_session(handle, &payload).map_err(|err| err.to_string())
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

    #[test]
    fn unpacks_interleaved_packed_terrain_tiles() {
        let mut packed = Vec::new();
        packed.extend_from_slice(&2u32.to_le_bytes());
        packed.extend_from_slice(&3u32.to_le_bytes());
        packed.extend_from_slice(&[1, 2, 3]);
        packed.extend_from_slice(&2u32.to_le_bytes());
        packed.extend_from_slice(&[4, 5]);

        assert_eq!(
            unpack_packed_terrain_tile_bytes(&packed).unwrap(),
            vec![vec![1, 2, 3], vec![4, 5]]
        );
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

}
