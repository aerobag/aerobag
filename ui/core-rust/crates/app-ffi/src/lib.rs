pub use app_core::*;
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::jbyteArray;
use jni::sys::jstring;
use jni::{JNIEnv, JavaVM};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(target_os = "android")]
use std::ffi::CString;
#[cfg(target_os = "android")]
use std::io::Write;
#[cfg(target_os = "android")]
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "android")]
const ANDROID_LOG_INFO: c_int = 4;
#[cfg(target_os = "android")]
const ANDROID_CORE_DEBUG_LOGCAT_ENABLED: bool = false;

#[cfg(target_os = "android")]
static GPS_CAPTURE_LOG_PATH: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

pub fn install_core_debug_logger() {
    app_core::set_core_debug_logger(Some(log_core_debug));
    app_core::set_core_clock_ms(Some(now_epoch_ms_f64));
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn now_epoch_ms_f64() -> f64 {
    now_epoch_ms() as f64
}

#[cfg(target_os = "android")]
fn log_core_debug(tag: &str, data: &serde_json::Value) {
    append_gps_capture_log_record(tag, data);

    if !ANDROID_CORE_DEBUG_LOGCAT_ENABLED {
        return;
    }
    let Ok(tag) = CString::new(tag) else {
        return;
    };
    let Ok(text) = CString::new(data.to_string()) else {
        return;
    };
    unsafe {
        let _ = __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), text.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn log_core_debug(_tag: &str, _data: &serde_json::Value) {}

#[cfg(target_os = "android")]
fn set_gps_capture_log_path(path: Option<String>) {
    *GPS_CAPTURE_LOG_PATH
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = path;
}

#[cfg(not(target_os = "android"))]
fn set_gps_capture_log_path(_path: Option<String>) {}

#[cfg(target_os = "android")]
fn append_gps_capture_log_record(tag: &str, data: &serde_json::Value) {
    if !tag.starts_with("ownship.gps_capture.") {
        return;
    }
    let path = GPS_CAPTURE_LOG_PATH
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let Some(path) = path else {
        return;
    };
    let record = serde_json::json!({
        "logged_at_epoch_ms": now_epoch_ms(),
        "tag": tag,
        "data": data,
    });
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    if writeln!(file, "{record}").is_ok() {
        let _ = file.sync_data();
    }
}

pub fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn empty_flight_plan_json() -> Result<String, String> {
    serde_json::to_string(&app_core::FlightPlan::empty()).map_err(|err| err.to_string())
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
    let result = app_core::create_ui_session_at_epoch_ms(
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn set_resource_policy_in_session_json(
    handle: u64,
    policy_json: &str,
) -> Result<String, String> {
    let policy: String = serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let policy = resource_policy_from_wire(&policy)?;
    let snapshot = app_core::set_resource_policy_in_session(handle as u32, policy)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn configure_platform_capabilities_in_session_json(
    handle: u64,
    capabilities_json: &str,
) -> Result<String, String> {
    let capabilities: app_core::PlatformCapabilities =
        serde_json::from_str(capabilities_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::configure_platform_capabilities_in_session(handle as u32, capabilities, None)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_installed_package_ids_in_session_json(
    handle: u64,
    package_ids_json: &str,
) -> Result<String, String> {
    let package_ids: Vec<String> =
        serde_json::from_str(package_ids_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_installed_package_ids_in_session(handle as u32, package_ids)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn resource_policy_from_wire(policy: &str) -> Result<app_core::CoreResourcePolicy, String> {
    match policy {
        "public_unpacked" => Ok(app_core::CoreResourcePolicy::PublicUnpacked),
        "installed_package" => Ok(app_core::CoreResourcePolicy::InstalledPackage),
        other => Err(format!("unknown resource policy: {other}")),
    }
}

pub fn perform_flight_plan_row_action_in_session_json(
    handle: u64,
    row_uid: &str,
    action_uid: &str,
) -> Result<String, String> {
    let outcome = app_core::perform_flight_plan_row_action_in_session(
        handle as u32,
        row_uid.to_string(),
        action_uid.to_string(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_status_action_in_session_json(
    handle: u64,
    action_id: &str,
) -> Result<String, String> {
    let snapshot = app_core::perform_status_action_in_session(handle as u32, action_id.to_string())
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn perform_settings_action_in_session_json(
    handle: u64,
    action_json: &str,
) -> Result<String, String> {
    let action: app_core::UiSettingsAction =
        serde_json::from_str(action_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::perform_settings_action_in_session(handle as u32, action)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn accept_disclaimer_in_session_json(
    handle: u64,
    agreement_id: &str,
) -> Result<String, String> {
    let snapshot = app_core::accept_disclaimer_in_session(handle as u32, agreement_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn load_plate_procedure_in_session_json(handle: u64, load_id: &str) -> Result<String, String> {
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

pub fn sync_guidance_geometry_in_session_json(handle: u64) -> Result<String, String> {
    let outcome = app_core::sync_guidance_geometry_in_session(handle as u32)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn project_flight_plan_route_in_session_json(handle: u64) -> Result<String, String> {
    let outcome = app_core::project_flight_plan_route_in_session(handle as u32)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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
    let outcome = app_core::insert_waypoint_at_flight_plan_row_in_session(
        handle as u32,
        row_uid.to_string(),
        before,
        waypoint,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn preview_flight_plan_entry_in_session_json(
    handle: u64,
    input: &str,
) -> Result<String, String> {
    let outcome = app_core::preview_flight_plan_entry_in_session(handle as u32, input.to_string())
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn append_flight_plan_entry_in_session_json(
    handle: u64,
    input: &str,
) -> Result<String, String> {
    let outcome = app_core::append_flight_plan_entry_in_session(handle as u32, input.to_string())
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

pub fn register_ownship_source_in_session_paged_json(
    handle: u64,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let outcome = app_core::register_ownship_source_in_session_outcome(handle as u32, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn update_ownship_source_status_in_session_paged_json(
    handle: u64,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let outcome = app_core::update_ownship_source_status_in_session_outcome(handle as u32, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn push_situation_sample_in_session_paged_json(
    handle: u64,
    sample_json: &str,
) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let outcome = app_core::push_situation_sample_in_session_outcome(handle as u32, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn select_ownship_source_in_session_paged_json(
    handle: u64,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_ownship_source_in_session_outcome(handle as u32, selection)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn load_playback_trace_in_session_paged_json(
    handle: u64,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, String> {
    let source_path: String =
        serde_json::from_str(source_path_json).map_err(|err| err.to_string())?;
    let outcome =
        app_core::load_playback_trace_in_session_outcome(handle as u32, &source_path, trace_json)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn play_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::play_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn play_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::play_playback_in_session_outcome(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn pause_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::pause_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn pause_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::pause_playback_in_session_outcome(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn seek_playback_in_session_paged_json(
    handle: u64,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome =
        app_core::seek_playback_in_session_outcome(handle as u32, cursor_seconds, now_epoch_ms)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn set_playback_rate_in_session_paged_json(
    handle: u64,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::set_playback_rate_in_session_outcome(handle as u32, rate, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn tick_playback_in_session_json(handle: u64, now_epoch_ms: f64) -> Result<String, String> {
    let snapshot = app_core::tick_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn tick_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::tick_playback_in_session_outcome(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn select_raster_map_in_session_json(
    handle: u64,
    selected_map_id_json: &str,
) -> Result<String, String> {
    let selected_map_id: String =
        serde_json::from_str(selected_map_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_raster_map_in_session(handle as u32, &selected_map_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn get_session_snapshot_json(handle: u64) -> Result<String, String> {
    let snapshot = app_core::get_session_snapshot(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn get_session_snapshot_at_epoch_ms_json(handle: u64, epoch_ms: i64) -> Result<String, String> {
    let snapshot = app_core::get_session_snapshot_at_epoch_ms(handle as u32, epoch_ms)
        .map_err(|err| err.to_string())?;
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

pub fn ingest_resource_in_session_bytes(
    handle: u64,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<String, String> {
    app_core::ingest_resource_in_session_at_epoch_ms(
        handle as u32,
        resource_id,
        resource_bytes,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn sync_live_feeds_in_session_json(handle: u64) -> Result<String, String> {
    let outcome =
        app_core::sync_live_feeds_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn live_feed_runtime_decision_json(input_json: &str) -> Result<String, String> {
    let input: app_core::LiveFeedRuntimeInput =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::live_feed_runtime_decision(input))
        .map_err(|err| err.to_string())
}

pub fn refresh_live_feed_current_in_session_json(handle: u64) -> Result<String, String> {
    let outcome = app_core::refresh_live_feed_current_in_session(handle as u32)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn ingest_live_feed_sse_events_in_session_json(
    handle: u64,
    events_json: &str,
) -> Result<String, String> {
    let events: Vec<app_core::LiveFeedSseEvent> =
        serde_json::from_str(events_json).map_err(|err| err.to_string())?;
    let outcome = app_core::ingest_live_feed_sse_events_in_session(handle as u32, &events)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn report_live_feed_connection_event_in_session_json(
    handle: u64,
    event_json: &str,
) -> Result<String, String> {
    let event: app_core::LiveFeedConnectionEvent =
        serde_json::from_str(event_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::report_live_feed_connection_event_in_session(
        handle as u32,
        event,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn get_map_overlay_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    get_map_overlay_in_session_with_point_display_scale_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        1.0,
    )
}

pub fn get_map_overlay_in_session_with_point_display_scale_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_map_overlay_in_session_with_point_display_scale_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        point_display_scale,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn get_map_selection_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    click_json: &str,
) -> Result<String, String> {
    get_map_selection_in_session_with_point_display_scale_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        click_json,
        1.0,
    )
}

pub fn get_map_selection_in_session_with_point_display_scale_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    click_json: &str,
    point_display_scale: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let click: app_core::LatLon =
        serde_json::from_str(click_json).map_err(|err| err.to_string())?;
    let selection = app_core::get_map_selection_in_session_with_point_display_scale_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        click,
        point_display_scale,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&selection).map_err(|err| err.to_string())
}

pub fn get_map_selection_for_nav_ref_in_session_with_point_display_scale_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    nav_ref_json: &str,
    point_display_scale: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let nav_ref: app_core::NavRef =
        serde_json::from_str(nav_ref_json).map_err(|err| err.to_string())?;
    let selection =
        app_core::get_map_selection_for_nav_ref_in_session_with_point_display_scale_at_epoch_ms(
            handle as u32,
            viewport,
            width_px,
            height_px,
            nav_ref,
            point_display_scale,
            now_epoch_ms(),
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
    let overlay = app_core::get_terrain_overlay_in_session_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn get_scheduled_terrain_overlay_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    decoded_cache_keys_json: &str,
    in_flight_cache_keys_json: &str,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let decoded_cache_keys: BTreeSet<String> =
        serde_json::from_str(decoded_cache_keys_json).map_err(|err| err.to_string())?;
    let in_flight_cache_keys: BTreeSet<String> =
        serde_json::from_str(in_flight_cache_keys_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_scheduled_terrain_overlay_in_session_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        &decoded_cache_keys,
        &in_flight_cache_keys,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn get_nexrad_overlay_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_nexrad_overlay_in_session_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

pub fn resolve_chart_asset_resource_in_session_json(
    handle: u64,
    chart_id: &str,
    asset_kind: &str,
) -> Result<String, String> {
    let outcome =
        app_core::resolve_chart_asset_resource_in_session(handle as u32, chart_id, asset_kind)
            .map_err(|err| err.to_string())?;
    app_core::serialize_publication_outcome(outcome)
}

pub fn nexrad_tile_bytes_in_session(handle: u64, src: &str) -> Result<Vec<u8>, String> {
    app_core::nexrad_tile_bytes_in_session(handle as u32, src).map_err(|err| err.to_string())
}

pub fn prepare_nexrad_tile_in_session_json(handle: u64, src: &str) -> Result<String, String> {
    let outcome = app_core::prepare_nexrad_tile_in_session(handle as u32, src)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn get_raster_tile_plan_in_session_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan = app_core::get_raster_tile_plan_in_session_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        now_epoch_ms(),
    )
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
    let plan = app_core::get_raster_tile_plan_in_session_with_options_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        app_core::RasterTilePlanOptions {
            max_tile_display_multiplier,
            ..app_core::RasterTilePlanOptions::default()
        },
        now_epoch_ms(),
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

pub fn render_terrain_overlay_tile_by_key_in_session_bytes(
    handle: u64,
    tile_key: &str,
    aircraft_altitude_ft: Option<f64>,
) -> Result<Vec<u8>, String> {
    app_core::render_terrain_overlay_tile_by_key_in_session(
        handle as u32,
        tile_key,
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
static NAV_KV_STORES: OnceLock<Mutex<HashMap<u32, StoredNavKvStore>>> = OnceLock::new();
static NEXT_NAV_DB_OPEN_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_DB_OPEN_CONTROLLERS: OnceLock<Mutex<HashMap<u32, app_core::NavDbOpenController>>> =
    OnceLock::new();
static NEXT_OFFLINE_PACKAGES_CONTROLLER_HANDLE: AtomicU32 = AtomicU32::new(1);
static OFFLINE_PACKAGES_CONTROLLERS: OnceLock<
    Mutex<HashMap<u32, app_core::OfflinePackagesControllerState>>,
> = OnceLock::new();
static NEXT_LIVE_FEED_CACHE_HANDLE: AtomicU32 = AtomicU32::new(1);
static LIVE_FEED_CACHES: OnceLock<Mutex<HashMap<u32, app_core::LiveFeedCache>>> = OnceLock::new();
static NEXT_UI_SESSION_WORK_SCHEDULER_HANDLE: AtomicU32 = AtomicU32::new(1);
static UI_SESSION_WORK_SCHEDULERS: OnceLock<Mutex<HashMap<u32, app_core::UiSessionWorkScheduler>>> =
    OnceLock::new();

struct StoredNavKvStore {
    store: app_core::NavKvStore,
    open_result: Option<app_core::NavDbOpenResult>,
}

fn nav_kv_stores() -> &'static Mutex<HashMap<u32, StoredNavKvStore>> {
    NAV_KV_STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn nav_db_open_controllers() -> &'static Mutex<HashMap<u32, app_core::NavDbOpenController>> {
    NAV_DB_OPEN_CONTROLLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn offline_packages_controllers(
) -> &'static Mutex<HashMap<u32, app_core::OfflinePackagesControllerState>> {
    OFFLINE_PACKAGES_CONTROLLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn live_feed_caches() -> &'static Mutex<HashMap<u32, app_core::LiveFeedCache>> {
    LIVE_FEED_CACHES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ui_session_work_schedulers() -> &'static Mutex<HashMap<u32, app_core::UiSessionWorkScheduler>> {
    UI_SESSION_WORK_SCHEDULERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn create_offline_packages_controller_json(
    packages_state_json: Option<&str>,
    library_cache_json: Option<&str>,
) -> Result<u64, String> {
    let packages_state = packages_state_json
        .filter(|json| !json.trim().is_empty())
        .map(|json| {
            serde_json::from_str::<app_core::OfflinePackagesState>(json)
                .map_err(|err| err.to_string())
        })
        .transpose()?;
    let library_cache = library_cache_json
        .filter(|json| !json.trim().is_empty())
        .map(|json| {
            serde_json::from_str::<app_core::OfflinePackagesLibraryCache>(json)
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
                library_cache,
                ..Default::default()
            },
        );
    Ok(handle as u64)
}

pub fn create_live_feed_cache_json(installed_states_json: Option<&str>) -> Result<u64, String> {
    let installed_states = installed_states_json
        .filter(|json| !json.trim().is_empty())
        .map(|json| {
            serde_json::from_str::<Vec<app_core::LiveFeedInstalledState>>(json)
                .map_err(|err| err.to_string())
        })
        .transpose()?
        .unwrap_or_default();
    let handle = NEXT_LIVE_FEED_CACHE_HANDLE.fetch_add(1, Ordering::Relaxed);
    live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?
        .insert(
            handle,
            app_core::LiveFeedCache::with_installed(installed_states),
        );
    Ok(handle as u64)
}

pub fn create_ui_session_work_scheduler() -> Result<u64, String> {
    let handle = NEXT_UI_SESSION_WORK_SCHEDULER_HANDLE.fetch_add(1, Ordering::Relaxed);
    ui_session_work_schedulers()
        .lock()
        .map_err(|_| "ui session work scheduler store poisoned".to_string())?
        .insert(handle, app_core::UiSessionWorkScheduler::default());
    Ok(handle as u64)
}

pub fn ui_session_work_scheduler_request_json(
    handle: u64,
    request_json: &str,
) -> Result<String, String> {
    let request: app_core::UiSessionWorkRequest =
        serde_json::from_str(request_json).map_err(|err| err.to_string())?;
    let mut schedulers = ui_session_work_schedulers()
        .lock()
        .map_err(|_| "ui session work scheduler store poisoned".to_string())?;
    let scheduler = schedulers
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid ui session work scheduler handle: {handle}"))?;
    serde_json::to_string(&scheduler.request(request)).map_err(|err| err.to_string())
}

pub fn ui_session_work_scheduler_complete_json(
    handle: u64,
    request_id: u64,
) -> Result<String, String> {
    let mut schedulers = ui_session_work_schedulers()
        .lock()
        .map_err(|_| "ui session work scheduler store poisoned".to_string())?;
    let decision = match schedulers.get_mut(&(handle as u32)) {
        Some(scheduler) => scheduler.complete(request_id),
        None => app_core::UiSessionWorkCompletionDecision {
            result_action: app_core::UiSessionWorkResultAction::Drop {
                reason: "scheduler_destroyed".to_string(),
            },
            next: None,
        },
    };
    serde_json::to_string(&decision).map_err(|err| err.to_string())
}

pub fn destroy_ui_session_work_scheduler(handle: u64) -> Result<(), String> {
    ui_session_work_schedulers()
        .lock()
        .map_err(|_| "ui session work scheduler store poisoned".to_string())?
        .remove(&(handle as u32));
    Ok(())
}

pub fn live_feed_cache_missing_requests_json(handle: u64) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.missing_requests()).map_err(|err| err.to_string())
}

pub fn live_feed_cache_missing_requests_at_epoch_ms_json(
    handle: u64,
    epoch_ms: i64,
) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.missing_requests_at_epoch_ms(epoch_ms))
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_current_refresh_requests_at_epoch_ms_json(
    handle: u64,
    epoch_ms: i64,
) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.current_refresh_requests_at_epoch_ms(epoch_ms))
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_record_request_failure(
    handle: u64,
    request_id: &str,
    epoch_ms: i64,
) -> Result<(), String> {
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache.record_request_failure(request_id, epoch_ms);
    Ok(())
}

pub fn live_feed_cache_install_fetched_bytes_json(
    handle: u64,
    request_json: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let request: app_core::LiveFeedCacheRequest =
        serde_json::from_str(request_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    let installed = cache
        .install_fetched_payload(
            &app_core::live_feed_product_registry(),
            &request,
            app_core::LiveFeedFetchedPayload::Bytes(bytes.to_vec()),
        )
        .map_err(|err| err.to_string())?
        .map(|state| state.summary());
    serde_json::to_string(&installed).map_err(|err| err.to_string())
}

pub fn live_feed_cache_ingest_sse_event_json(
    handle: u64,
    event_json: &str,
) -> Result<String, String> {
    let event: app_core::LiveFeedSseEvent =
        serde_json::from_str(event_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(
        &cache
            .ingest_sse_event(&event)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

pub fn live_feed_cache_installed_summary_json(
    handle: u64,
    product: &str,
) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.installed_summary(product)).map_err(|err| err.to_string())
}

pub fn live_feed_cache_ingest_installed_payload_bytes(
    handle: u64,
    summary_json: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let summary: app_core::LiveFeedInstalledSummary =
        serde_json::from_str(summary_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .ingest_installed_payload_bytes(&app_core::live_feed_product_registry(), &summary, bytes)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_installed_payload_bytes(
    handle: u64,
    product: &str,
) -> Result<Vec<u8>, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .installed_payload_bytes(product)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_install_product_in_session_json(
    handle: u64,
    session_handle: u64,
    product: &str,
) -> Result<String, String> {
    let installed = {
        let caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .installed(product)
            .cloned()
            .ok_or_else(|| format!("{product} is not installed"))?
    };
    let snapshot =
        app_core::install_live_feed_installed_state_in_session(session_handle as u32, &installed)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn live_feed_cache_sync_catalog_in_session_json(
    handle: u64,
    session_handle: u64,
) -> Result<String, String> {
    let live_feeds = {
        let caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .live_feeds_state()
            .clone()
    };
    let snapshot = app_core::sync_live_feed_catalog_in_session(session_handle as u32, &live_feeds)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn destroy_live_feed_cache_json(handle: u64) -> Result<(), String> {
    live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?
        .remove(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    Ok(())
}

pub fn destroy_offline_packages_controller_json(handle: u64) -> Result<(), String> {
    offline_packages_controllers()
        .lock()
        .map_err(|_| "offline packages controller store poisoned".to_string())?
        .remove(&(handle as u32))
        .ok_or_else(|| format!("invalid offline packages controller handle: {handle}"))?;
    Ok(())
}

pub fn nav_db_open_controller_create_json(candidates_json: &str) -> Result<u64, String> {
    let candidates: Vec<app_core::NavDbArtifactCandidate> =
        serde_json::from_str(candidates_json).map_err(|err| err.to_string())?;
    let handle = NEXT_NAV_DB_OPEN_HANDLE.fetch_add(1, Ordering::Relaxed);
    nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?
        .insert(handle, app_core::NavDbOpenController::new(candidates));
    Ok(handle as u64)
}

pub fn nav_db_open_controller_step_json(handle: u64) -> Result<String, String> {
    let mut controllers = nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?;
    let controller = controllers
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid nav db open controller handle: {handle}"))?;
    serde_json::to_string(&controller.step()?).map_err(|err| err.to_string())
}

pub fn nav_db_open_controller_ingest_resource_bytes(
    handle: u64,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), String> {
    let mut controllers = nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?;
    let controller = controllers
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid nav db open controller handle: {handle}"))?;
    controller.ingest_resource(resource_id, resource_bytes)
}

pub fn nav_db_open_controller_finish_json(handle: u64) -> Result<String, String> {
    #[derive(Serialize)]
    struct FinishResult {
        nav_kv_handle: u64,
        open_result: app_core::NavDbOpenResult,
    }

    let mut controllers = nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?;
    let mut controller = controllers
        .remove(&(handle as u32))
        .ok_or_else(|| format!("invalid nav db open controller handle: {handle}"))?;
    let outcome = controller.step()?;
    let app_core::HadOperationOutcome::Complete { result, .. } = outcome else {
        return Err("nav db open controller is not complete".to_string());
    };
    let open_result: app_core::NavDbOpenResult =
        serde_json::from_value(result).map_err(|err| err.to_string())?;
    let store = controller
        .selected_store()
        .ok_or_else(|| "nav db open controller has no selected store".to_string())?
        .clone();
    let nav_kv_handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?
        .insert(
            nav_kv_handle,
            StoredNavKvStore {
                store,
                open_result: Some(open_result.clone()),
            },
        );
    serde_json::to_string(&FinishResult {
        nav_kv_handle: nav_kv_handle as u64,
        open_result,
    })
    .map_err(|err| err.to_string())
}

pub fn nav_db_open_controller_statuses_json(handle: u64) -> Result<String, String> {
    let controllers = nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?;
    let controller = controllers
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid nav db open controller handle: {handle}"))?;
    serde_json::to_string(&controller.statuses()).map_err(|err| err.to_string())
}

pub fn nav_db_open_controller_destroy_handle(handle: u64) {
    if let Ok(mut controllers) = nav_db_open_controllers().lock() {
        let _ = controllers.remove(&(handle as u32));
    }
}

fn nav_kv_insert_page_bytes(handle: u64, page_index: u32, page_bytes: &[u8]) -> Result<(), String> {
    let mut stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    let store = stores
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid nav kv handle: {handle}"))?;
    store.store.insert_page(page_index, page_bytes.to_vec());
    app_core::insert_nav_kv_page_for_attached_sessions(handle as u32, page_index, page_bytes);
    Ok(())
}

pub fn nav_kv_insert_resource_bytes(
    handle: u64,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), String> {
    let page_index = app_core::nav_kv_page_index_from_resource_id(resource_id)
        .ok_or_else(|| format!("unsupported nav kv resource id: {resource_id}"))?;
    let decoded_bytes = app_core::decode_nav_db_page_resource_bytes(resource_id, resource_bytes)?;
    nav_kv_insert_page_bytes(handle, page_index, decoded_bytes.as_ref())
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
    app_core::attach_nav_kv_store_to_session_with_open_result(
        session_handle as u32,
        nav_kv_handle as u32,
        &store.store,
        store.open_result.as_ref(),
    )
    .map_err(|err| err.to_string())
}

pub fn nav_kv_destroy_handle(handle: u64) {
    if let Ok(mut stores) = nav_kv_stores().lock() {
        let _ = stores.remove(&(handle as u32));
    }
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
    let outcome =
        app_core::run_had_operation(&store.store, operation).map_err(|err| err.to_string())?;
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
    #[serde(default)]
    storage: Option<app_core::OfflinePackagesStorageInfo>,
    event: OfflinePackagesControllerEventWire,
}

#[derive(Serialize)]
struct OfflinePackagesControllerResultWire {
    packages_state_json: Option<String>,
    library_cache_json: Option<String>,
    ui_state: app_core::OfflinePackagesControllerUiState,
    command: Option<app_core::OfflinePackagesControllerCommand>,
}

#[derive(Deserialize)]
struct CurrentArtifactsDiscoveryInputWire {
    publication_root_url: String,
    current_artifacts_json: String,
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

pub fn plan_current_artifacts_discovery_json(input_json: &str) -> Result<String, String> {
    let input: CurrentArtifactsDiscoveryInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let plan = app_core::plan_current_artifacts_discovery(
        &input.publication_root_url,
        &input.current_artifacts_json,
    )?;
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
            storage: input.storage,
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
        library_cache_json: result
            .state
            .library_cache
            .as_ref()
            .map(|cache| serde_json::to_string(cache))
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

struct JniSettingsStore {
    vm: JavaVM,
    store: GlobalRef,
}

impl app_core::SettingsStorage for JniSettingsStore {
    fn read_settings(&self) -> app_core::AppResult<Option<Vec<u8>>> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })?;
        let value = env
            .call_method(self.store.as_obj(), "readSettings", "()[B", &[])
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })?
            .l()
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })?;
        if value.is_null() {
            return Ok(None);
        }
        env.convert_byte_array(JByteArray::from(value))
            .map(Some)
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })
    }

    fn write_settings(&self, bytes: &[u8]) -> app_core::AppResult<()> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })?;
        let array = env
            .byte_array_from_slice(bytes)
            .map_err(|err| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message: err.to_string(),
            })?;
        let array = JObject::from(array);
        env.call_method(
            self.store.as_obj(),
            "writeSettings",
            "([B)V",
            &[JValue::Object(&array)],
        )
        .map(|_| ())
        .map_err(|err| app_core::AppError {
            kind: app_core::AppErrorKind::Internal,
            message: err.to_string(),
        })
    }
}

fn settings_store_from_java(
    env: &mut JNIEnv,
    store: JObject,
) -> Result<Option<app_core::SettingsStorageHandle>, String> {
    if store.is_null() {
        return Ok(None);
    }
    let vm = env.get_java_vm().map_err(|err| err.to_string())?;
    let store = env.new_global_ref(store).map_err(|err| err.to_string())?;
    Ok(Some(Arc::new(JniSettingsStore { vm, store })))
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_installCoreDebugLogger(
    _env: JNIEnv,
    _class: JClass,
) {
    install_core_debug_logger();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_configureGpsCaptureLogPath(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) {
    match get_java_string(&mut env, path) {
        Ok(path) if !path.is_empty() => set_gps_capture_log_path(Some(path)),
        _ => set_gps_capture_log_path(None),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_situationRingCandidatesJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_planOfflinePackagesFromBundleJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_planCurrentArtifactsDiscoveryJson(
    mut env: JNIEnv,
    _class: JClass,
    input_json: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input_json)?;
        plan_current_artifacts_discovery_json(&input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_initializeOfflinePackagesJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_reduceOfflinePackagesJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createOfflinePackagesController(
    mut env: JNIEnv,
    _class: JClass,
    packages_state_json: JString,
    library_cache_json: JString,
) -> i64 {
    let result = (|| {
        let packages_state_json = get_java_string(&mut env, packages_state_json)?;
        let library_cache_json = get_java_string(&mut env, library_cache_json)?;
        create_offline_packages_controller_json(
            Some(&packages_state_json),
            Some(&library_cache_json),
        )
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_dispatchOfflinePackagesControllerJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_destroyOfflinePackagesController(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = destroy_offline_packages_controller_json(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createUiSessionWorkScheduler(
    mut env: JNIEnv,
    _class: JClass,
) -> i64 {
    match create_ui_session_work_scheduler() {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_uiSessionWorkSchedulerRequestJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    request_json: JString,
) -> jstring {
    let result = (|| {
        let request = get_java_string(&mut env, request_json)?;
        ui_session_work_scheduler_request_json(handle as u64, &request)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_uiSessionWorkSchedulerCompleteJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    request_id: i64,
) -> jstring {
    return_string(
        &mut env,
        ui_session_work_scheduler_complete_json(handle as u64, request_id as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_destroyUiSessionWorkScheduler(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = destroy_ui_session_work_scheduler(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createLiveFeedCache(
    mut env: JNIEnv,
    _class: JClass,
    installed_states_json: JString,
) -> i64 {
    match get_java_string(&mut env, installed_states_json)
        .and_then(|installed_states| create_live_feed_cache_json(Some(&installed_states)))
    {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheMissingRequestsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(
        &mut env,
        live_feed_cache_missing_requests_json(handle as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheMissingRequestsAtEpochMsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        live_feed_cache_missing_requests_at_epoch_ms_json(handle as u64, epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheCurrentRefreshRequestsAtEpochMsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        live_feed_cache_current_refresh_requests_at_epoch_ms_json(handle as u64, epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheRecordRequestFailure(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    request_id: JString,
    epoch_ms: i64,
) {
    let result = get_java_string(&mut env, request_id).and_then(|request| {
        live_feed_cache_record_request_failure(handle as u64, &request, epoch_ms)
    });
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstallFetchedBytesJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    request_json: JString,
    payload_bytes: JByteArray,
) -> jstring {
    let result = (|| {
        let request = get_java_string(&mut env, request_json)?;
        let bytes = get_java_byte_array(&mut env, payload_bytes)?;
        live_feed_cache_install_fetched_bytes_json(handle as u64, &request, &bytes)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheIngestSseEventJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    event_json: JString,
) -> jstring {
    let result = get_java_string(&mut env, event_json)
        .and_then(|event| live_feed_cache_ingest_sse_event_json(handle as u64, &event));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstalledSummaryJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) -> jstring {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_installed_summary_json(handle as u64, &product));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheIngestInstalledPayloadBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    summary_json: JString,
    payload_bytes: JByteArray,
) {
    let result = (|| {
        let summary = get_java_string(&mut env, summary_json)?;
        let bytes = get_java_byte_array(&mut env, payload_bytes)?;
        live_feed_cache_ingest_installed_payload_bytes(handle as u64, &summary, &bytes)
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstalledPayloadBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) -> jbyteArray {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_installed_payload_bytes(handle as u64, &product));
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstallProductInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    session_handle: i64,
    product: JString,
) -> jstring {
    let result = get_java_string(&mut env, product).and_then(|product| {
        live_feed_cache_install_product_in_session_json(
            handle as u64,
            session_handle as u64,
            &product,
        )
    });
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheSyncCatalogInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    session_handle: i64,
) -> jstring {
    return_string(
        &mut env,
        live_feed_cache_sync_catalog_in_session_json(handle as u64, session_handle as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_destroyLiveFeedCache(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = destroy_live_feed_cache_json(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_emptyFlightPlanJson(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    return_string(&mut env, empty_flight_plan_json())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_prepareAirwayPresentationJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createUiSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        create_ui_session_json(
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setResourcePolicyInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let policy_json = get_java_string(&mut env, policy_json)?;
        set_resource_policy_in_session_json(handle as u64, &policy_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_configurePlatformCapabilitiesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    capabilities_json: JString,
    settings_store: JObject,
) -> jstring {
    let result = (|| {
        let capabilities_json = get_java_string(&mut env, capabilities_json)?;
        let capabilities: app_core::PlatformCapabilities =
            serde_json::from_str(&capabilities_json).map_err(|err| err.to_string())?;
        let settings_storage = settings_store_from_java(&mut env, settings_store)?;
        let snapshot = app_core::configure_platform_capabilities_in_session(
            handle as u32,
            capabilities,
            settings_storage,
        )
        .map_err(|err| err.to_string())?;
        serde_json::to_string(&snapshot).map_err(|err| err.to_string())
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setInstalledPackageIdsInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    package_ids_json: JString,
) -> jstring {
    let result = (|| {
        let package_ids_json = get_java_string(&mut env, package_ids_json)?;
        set_installed_package_ids_in_session_json(handle as u64, &package_ids_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performFlightPlanRowActionInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performStatusActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        perform_status_action_in_session_json(handle as u64, &action_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performSettingsActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_json: JString,
) -> jstring {
    let result = (|| {
        let action_json = get_java_string(&mut env, action_json)?;
        perform_settings_action_in_session_json(handle as u64, &action_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_acceptDisclaimerInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    agreement_id: JString,
) -> jstring {
    let result = (|| {
        let agreement_id = get_java_string(&mut env, agreement_id)?;
        accept_disclaimer_in_session_json(handle as u64, &agreement_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_loadPlateProcedureInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_activateNextLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| activate_next_leg_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_suspendSequencingInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| suspend_sequencing_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_unsuspendSequencingInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| unsuspend_sequencing_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sequenceActiveLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| sequence_active_leg_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_syncGuidanceGeometryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| sync_guidance_geometry_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_projectFlightPlanRouteInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = (|| project_flight_plan_route_in_session_json(handle as u64))();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performMapSelectionActionInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_insertWaypointAtFlightPlanRowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_suggestWaypointIdentifiersAtFlightPlanRowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_previewFlightPlanEntryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    input: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input)?;
        preview_flight_plan_entry_in_session_json(handle as u64, &input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_appendFlightPlanEntryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    input: JString,
) -> jstring {
    let result = (|| {
        let input = get_java_string(&mut env, input)?;
        append_flight_plan_entry_in_session_json(handle as u64, &input)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_insertAirwayAtFlightPlanRowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectProcedureAtFlightPlanRowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectAirportInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_registerOwnshipSourceInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_registerOwnshipSourceInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    registration_json: JString,
) -> jstring {
    let result = (|| {
        let registration_json = get_java_string(&mut env, registration_json)?;
        register_ownship_source_in_session_paged_json(handle as u64, &registration_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_updateOwnshipSourceStatusInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_updateOwnshipSourceStatusInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    update_json: JString,
) -> jstring {
    let result = (|| {
        let update_json = get_java_string(&mut env, update_json)?;
        update_ownship_source_status_in_session_paged_json(handle as u64, &update_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_pushSituationSampleInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_pushSituationSampleInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    sample_json: JString,
) -> jstring {
    let result = (|| {
        let sample_json = get_java_string(&mut env, sample_json)?;
        push_situation_sample_in_session_paged_json(handle as u64, &sample_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectOwnshipSourceInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectOwnshipSourceInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    selection_json: JString,
) -> jstring {
    let result = (|| {
        let selection_json = get_java_string(&mut env, selection_json)?;
        select_ownship_source_in_session_paged_json(handle as u64, &selection_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_applySituationControlInputInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_engageMapFollowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_disengageMapFollowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setMapFollowOffsetInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_loadPlaybackTraceInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_loadPlaybackTraceInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    source_path_json: JString,
    trace_json: JString,
) -> jstring {
    let result = (|| {
        let source_path_json = get_java_string(&mut env, source_path_json)?;
        let trace_json = get_java_string(&mut env, trace_json)?;
        load_playback_trace_in_session_paged_json(handle as u64, &source_path_json, &trace_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_playPlaybackInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_playPlaybackInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        play_playback_in_session_paged_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_pausePlaybackInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_pausePlaybackInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        pause_playback_in_session_paged_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_seekPlaybackInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_seekPlaybackInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        seek_playback_in_session_paged_json(handle as u64, cursor_seconds, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setPlaybackRateInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setPlaybackRateInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    rate: f64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        set_playback_rate_in_session_paged_json(handle as u64, rate, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_tickPlaybackInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_tickPlaybackInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        tick_playback_in_session_paged_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectChartInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setMapLayerVisibilityInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setMapLayerEnabledInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setDebugFlagInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_loadRasterMapCatalogInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = load_raster_map_catalog_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectMapFamilyInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectRasterMapInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    selected_map_id_json: JString,
) -> jstring {
    let result = (|| {
        let selected_map_id = get_java_string(&mut env, selected_map_id_json)?;
        select_raster_map_in_session_json(handle as u64, &selected_map_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getSessionSnapshotJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, get_session_snapshot_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getSessionSnapshotAtEpochMsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        get_session_snapshot_at_epoch_ms_json(handle as u64, epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_restoreChartPageStateInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestPointTilesInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestAirspaceRefTilesInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestAirspaceFeaturesInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestAirspaceLabelTilesInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestResourceInSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    resource_id: JString,
    resource_bytes: JByteArray,
) -> jstring {
    let result = (|| {
        let resource_id = get_java_string(&mut env, resource_id)?;
        let bytes = get_java_byte_array(&mut env, resource_bytes)?;
        ingest_resource_in_session_bytes(handle as u64, &resource_id, &bytes)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_syncLiveFeedsInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = sync_live_feeds_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedRuntimeDecisionJson(
    mut env: JNIEnv,
    _class: JClass,
    input_json: JString,
) -> jstring {
    let result = get_java_string(&mut env, input_json)
        .and_then(|input| live_feed_runtime_decision_json(&input));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_refreshLiveFeedCurrentInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = refresh_live_feed_current_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_ingestLiveFeedSseEventsInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    events_json: JString,
) -> jstring {
    let result = get_java_string(&mut env, events_json)
        .and_then(|events| ingest_live_feed_sse_events_in_session_json(handle as u64, &events));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_reportLiveFeedConnectionEventInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    event_json: JString,
) -> jstring {
    let result = get_java_string(&mut env, event_json)
        .and_then(|event| report_live_feed_connection_event_in_session_json(handle as u64, &event));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapOverlayInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapOverlayInSessionWithPointDisplayScaleJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_map_overlay_in_session_with_point_display_scale_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            point_display_scale,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapSelectionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    click_json: JString,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        let click = get_java_string(&mut env, click_json)?;
        get_map_selection_in_session_json(handle as u64, &viewport, width_px, height_px, &click)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapSelectionInSessionWithPointDisplayScaleJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    click_json: JString,
    point_display_scale: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        let click = get_java_string(&mut env, click_json)?;
        get_map_selection_in_session_with_point_display_scale_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            &click,
            point_display_scale,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapSelectionForNavRefInSessionWithPointDisplayScaleJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    nav_ref_json: JString,
    point_display_scale: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        let nav_ref = get_java_string(&mut env, nav_ref_json)?;
        get_map_selection_for_nav_ref_in_session_with_point_display_scale_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            &nav_ref,
            point_display_scale,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getTerrainOverlayInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getScheduledTerrainOverlayInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    decoded_cache_keys_json: JString,
    in_flight_cache_keys_json: JString,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        let decoded_cache_keys = get_java_string(&mut env, decoded_cache_keys_json)?;
        let in_flight_cache_keys = get_java_string(&mut env, in_flight_cache_keys_json)?;
        get_scheduled_terrain_overlay_in_session_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            &decoded_cache_keys,
            &in_flight_cache_keys,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getRasterTilePlanInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getNexradOverlayInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_nexrad_overlay_in_session_json(handle as u64, &viewport, width_px, height_px)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_resolveChartAssetResourceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    chart_id: JString,
    asset_kind: JString,
) -> jstring {
    let result = (|| {
        let chart_id = get_java_string(&mut env, chart_id)?;
        let asset_kind = get_java_string(&mut env, asset_kind)?;
        resolve_chart_asset_resource_in_session_json(handle as u64, &chart_id, &asset_kind)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getRasterTilePlanInSessionWithOptionsJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_renderTerrainOverlayTileInSession(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_renderTerrainOverlayTileByKeyInSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    tile_key: JString,
    aircraft_altitude_ft: f64,
) -> jbyteArray {
    let result = (|| {
        let tile_key = get_java_string(&mut env, tile_key)?;
        render_terrain_overlay_tile_by_key_in_session_bytes(
            handle as u64,
            &tile_key,
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_nexradTileBytesInSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    src: JString,
) -> jbyteArray {
    let result = get_java_string(&mut env, src)
        .and_then(|src| nexrad_tile_bytes_in_session(handle as u64, &src));
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_prepareNexradTileInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    src: JString,
) -> jstring {
    let result = get_java_string(&mut env, src)
        .and_then(|src| prepare_nexrad_tile_in_session_json(handle as u64, &src));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_renderTerrainOverlayTilesInSession(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_syncMapFollowInSessionJson(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_destroySession(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    destroy_session_json(handle as u64)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerCreate(
    mut env: JNIEnv,
    _class: JClass,
    candidates_json: JString,
) -> i64 {
    match get_java_string(&mut env, candidates_json)
        .and_then(|candidates| nav_db_open_controller_create_json(&candidates))
    {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerStep(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, nav_db_open_controller_step_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerIngestResource(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    resource_id: JString,
    resource_bytes: JByteArray,
) {
    let result = (|| {
        let resource_id = get_java_string(&mut env, resource_id)?;
        let bytes = get_java_byte_array(&mut env, resource_bytes)?;
        nav_db_open_controller_ingest_resource_bytes(handle as u64, &resource_id, &bytes)
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerFinish(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, nav_db_open_controller_finish_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerStatuses(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(
        &mut env,
        nav_db_open_controller_statuses_json(handle as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    nav_db_open_controller_destroy_handle(handle as u64)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navKvInsertResource(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    resource_id: JString,
    resource_bytes: JByteArray,
) {
    let result = (|| {
        let resource_id = get_java_string(&mut env, resource_id)?;
        let bytes = get_java_byte_array(&mut env, resource_bytes)?;
        nav_kv_insert_resource_bytes(handle as u64, &resource_id, &bytes)
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navKvDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    nav_kv_destroy_handle(handle as u64)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_attachNavKvStoreToSession(
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_coreHadOperation(
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

    #[test]
    fn empty_flight_plan_json_returns_core_default_plan() {
        let plan_json = empty_flight_plan_json().unwrap();
        let plan: app_core::FlightPlan = serde_json::from_str(&plan_json).unwrap();

        assert!(plan.route_components.is_empty());
        assert!(plan.resolved_legs.is_empty());
    }

    #[test]
    fn stale_ui_session_work_completion_after_destroy_drops_result() {
        let handle = create_ui_session_work_scheduler().unwrap();
        let request = app_core::UiSessionWorkRequest {
            id: 1,
            kind: app_core::UiSessionWorkKind::MapOverlay,
            coalesce_key: Some("map_overlay".to_string()),
            requested_at_ms: 1_000,
        };
        let request_json = serde_json::to_string(&request).unwrap();
        let decision: app_core::UiSessionWorkRequestDecision = serde_json::from_str(
            &ui_session_work_scheduler_request_json(handle, &request_json).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            decision,
            app_core::UiSessionWorkRequestDecision::Start { .. }
        ));

        destroy_ui_session_work_scheduler(handle).unwrap();

        let completion: app_core::UiSessionWorkCompletionDecision =
            serde_json::from_str(&ui_session_work_scheduler_complete_json(handle, 1).unwrap())
                .unwrap();
        assert_eq!(
            completion,
            app_core::UiSessionWorkCompletionDecision {
                result_action: app_core::UiSessionWorkResultAction::Drop {
                    reason: "scheduler_destroyed".to_string(),
                },
                next: None,
            }
        );
    }

    #[test]
    fn ui_session_work_request_after_destroy_stays_loud() {
        let handle = create_ui_session_work_scheduler().unwrap();
        destroy_ui_session_work_scheduler(handle).unwrap();
        let request = app_core::UiSessionWorkRequest {
            id: 1,
            kind: app_core::UiSessionWorkKind::MapOverlay,
            coalesce_key: Some("map_overlay".to_string()),
            requested_at_ms: 1_000,
        };
        let request_json = serde_json::to_string(&request).unwrap();

        let error = ui_session_work_scheduler_request_json(handle, &request_json)
            .expect_err("requesting work after destroy should remain an error");
        assert!(error.contains("invalid ui session work scheduler handle"));
    }
}
