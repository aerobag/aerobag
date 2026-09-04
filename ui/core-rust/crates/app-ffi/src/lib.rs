// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub use app_core::*;
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::jstring;
use jni::sys::{jboolean, jbyteArray};
use jni::{JNIEnv, JavaVM};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(target_os = "android")]
use std::ffi::CString;
#[cfg(target_os = "android")]
use std::io::Write;
#[cfg(target_os = "android")]
use std::os::raw::{c_char, c_int};
#[cfg(target_os = "android")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "android")]
const ANDROID_LOG_INFO: c_int = 4;
#[cfg(target_os = "android")]
static ANDROID_CORE_PERF_LOGCAT_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
static GPS_CAPTURE_LOG_PATH: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

pub fn install_core_debug_logger() {
    app_core::set_core_debug_logger(Some(log_core_debug));
    app_core::set_core_clock_ms(Some(monotonic_clock_ms));
}

pub fn set_core_perf_debug_logging_enabled(enabled: bool) {
    app_core::set_core_verbose_perf_logs(enabled);
    #[cfg(target_os = "android")]
    ANDROID_CORE_PERF_LOGCAT_ENABLED.store(enabled, Ordering::Relaxed);
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn monotonic_clock_ms() -> f64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1_000.0
}

#[cfg(target_os = "android")]
fn log_core_debug(tag: &str, data: &serde_json::Value) {
    append_gps_capture_log_record(tag, data);

    if !ANDROID_CORE_PERF_LOGCAT_ENABLED.load(Ordering::Relaxed)
        || !(tag.starts_with("map.overlay.")
            || tag == "session.operation"
            || tag == "live_feed.install.session"
            || tag == "session.snapshot.total"
            || tag == "session.snapshot.core"
            || tag == "session.update.total")
    {
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

pub fn create_ui_session_json(
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
    let result = app_core::create_ui_session_at_epoch_ms(
        app_core::FlightPlan::empty(),
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    let serialized = serde_json::to_string(&result).map_err(|err| err.to_string())?;
    app_core::record_session_serialized_payload_bytes(result.handle, serialized.len());
    Ok(serialized)
}

pub fn session_diagnostics_json(handle: u64) -> Result<String, String> {
    let diagnostics =
        app_core::session_diagnostics(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&diagnostics).map_err(|err| err.to_string())
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

pub fn load_offline_package_library_cache_in_session_json(
    handle: u64,
    library_cache_json: &str,
) -> Result<String, String> {
    let cache: app_core::OfflinePackagesLibraryCache =
        serde_json::from_str(library_cache_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::load_offline_package_library_cache_in_session(handle as u32, cache)
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

pub fn navigation_page_state_json(capabilities_json: &str) -> Result<String, String> {
    let capabilities: app_core::PlatformCapabilities =
        serde_json::from_str(capabilities_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&app_core::navigation_page_state_for_platform(&capabilities))
        .map_err(|err| err.to_string())
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

pub fn perform_flight_plan_command_in_session_json(
    handle: u64,
    command_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let command: app_core::FlightPlanSessionCommand =
        serde_json::from_str(command_json).map_err(|err| err.to_string())?;
    let outcome =
        app_core::perform_flight_plan_command_in_session(handle as u32, command, now_epoch_ms)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_time_display_action_in_session_json(
    handle: u64,
    action_id: &str,
) -> Result<String, String> {
    let outcome =
        app_core::perform_time_display_action_in_session(handle as u32, action_id.to_string())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_flight_data_banner_cell_action_in_session_json(
    handle: u64,
    cell_id: &str,
) -> Result<String, String> {
    let outcome = app_core::perform_flight_data_banner_cell_action_in_session(
        handle as u32,
        cell_id.to_string(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_flight_plan_column_action_in_session_json(
    handle: u64,
    action_id: &str,
) -> Result<String, String> {
    let outcome = app_core::perform_flight_plan_column_action_in_session(
        handle as u32,
        action_id.to_string(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn query_flight_plan_in_session_json(handle: u64, query_json: &str) -> Result<String, String> {
    let query: app_core::FlightPlanSessionQuery =
        serde_json::from_str(query_json).map_err(|err| err.to_string())?;
    let outcome = app_core::query_flight_plan_in_session(handle as u32, query)
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

pub fn status_action_decision_in_session_json(
    handle: u64,
    action_id: &str,
) -> Result<String, String> {
    let decision =
        app_core::session::status_action_decision_in_session(handle as u32, action_id.to_string())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&decision).map_err(|err| err.to_string())
}

pub fn perform_ownship_text_action_in_session_json(
    handle: u64,
    action_id: &str,
    value: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::perform_ownship_text_action_in_session(
        handle as u32,
        action_id,
        value,
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_settings_action_in_session_json(
    handle: u64,
    action_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let action: app_core::UiSettingsAction =
        serde_json::from_str(action_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::perform_settings_action_in_session(handle as u32, action, now_epoch_ms)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn perform_aircraft_library_action_in_session_json(
    handle: u64,
    action_id: &str,
    source_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::perform_aircraft_library_action_in_session(
        handle as u32,
        action_id,
        Some(source_json),
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn perform_cloud_ui_action_in_session_json(
    handle: u64,
    action_id_json: &str,
    fields_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let action_id: app_core::CloudUiActionId =
        serde_json::from_str(action_id_json).map_err(|err| err.to_string())?;
    let fields: Vec<app_core::CloudUiFieldValue> =
        serde_json::from_str(fields_json).map_err(|err| err.to_string())?;
    let outcome = app_core::perform_cloud_ui_action_in_session(
        handle as u32,
        action_id,
        fields,
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn record_offline_package_preferences_in_session_json(
    handle: u64,
    preferences_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::record_offline_package_preferences_in_session(
        handle as u32,
        preferences_json,
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn take_cloud_provider_request_in_session_json(
    handle: u64,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let request = app_core::take_cloud_provider_request_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&request).map_err(|err| err.to_string())
}

pub fn complete_cloud_provider_request_in_session_json(
    handle: u64,
    request_id: u64,
    response_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let response: app_core::CloudHttpResponse =
        serde_json::from_str(response_json).map_err(|err| err.to_string())?;
    let outcome = app_core::complete_cloud_provider_request_in_session(
        handle as u32,
        request_id,
        response,
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn cloud_event_stream_plan_in_session_json(handle: u64) -> Result<String, String> {
    let plan = app_core::cloud_event_stream_plan_in_session(handle as u32)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn report_cloud_event_stream_event_in_session_json(
    handle: u64,
    event_json: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let event: app_core::CloudEventStreamEvent =
        serde_json::from_str(event_json).map_err(|err| err.to_string())?;
    let outcome =
        app_core::report_cloud_event_stream_event_in_session(handle as u32, event, now_epoch_ms)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn accept_disclaimer_in_session_json(
    handle: u64,
    agreement_id: &str,
) -> Result<String, String> {
    let snapshot = app_core::accept_disclaimer_in_session(handle as u32, agreement_id)
        .map_err(|err| err.to_string())?;
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

pub fn map_selection_action_decision_in_session_json(
    handle: u64,
    action_uid: &str,
) -> Result<String, String> {
    let decision =
        app_core::map_selection_action_decision_in_session(handle as u32, action_uid.to_string())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&decision).map_err(|err| err.to_string())
}

pub fn flight_plan_row_action_decision_in_session_json(
    handle: u64,
    row_uid: &str,
    action_uid: &str,
) -> Result<String, String> {
    let decision = app_core::flight_plan_row_action_decision_in_session(
        handle as u32,
        row_uid.to_string(),
        action_uid.to_string(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&decision).map_err(|err| err.to_string())
}

pub fn perform_map_selection_ui_action_in_session_json(
    handle: u64,
    action_uid: &str,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::perform_map_selection_ui_action_in_session(
        handle as u32,
        action_uid.to_string(),
        now_epoch_ms,
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

pub fn open_chart_airport_in_session_json(
    handle: u64,
    airport_id_json: &str,
    chart_id_json: &str,
) -> Result<String, String> {
    let airport_id: String =
        serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let chart_id: Option<String> =
        serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let outcome =
        app_core::open_chart_airport_in_session(handle as u32, &airport_id, chart_id.as_deref())
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn register_ownship_source_in_session_paged_json(
    handle: u64,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let outcome = app_core::register_ownship_source_in_session(handle as u32, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn update_ownship_source_status_in_session_paged_json(
    handle: u64,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let outcome = app_core::update_ownship_source_status_in_session(handle as u32, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn push_situation_sample_in_session_paged_json(
    handle: u64,
    sample_json: &str,
) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let outcome = app_core::push_situation_sample_in_session(handle as u32, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn select_ownship_source_in_session_paged_json(
    handle: u64,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_ownship_source_in_session(handle as u32, selection)
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

pub fn set_ownship_source_sleeping_in_session_json(
    handle: u64,
    source_id: &str,
    sleeping: bool,
    now_epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::set_ownship_source_sleeping_in_session(
        handle as u32,
        source_id,
        sleeping,
        now_epoch_ms,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn load_playback_trace_in_session_paged_json(
    handle: u64,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, String> {
    let source_path: String =
        serde_json::from_str(source_path_json).map_err(|err| err.to_string())?;
    let outcome = app_core::load_playback_trace_in_session(handle as u32, &source_path, trace_json)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn play_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::play_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn pause_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::pause_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn seek_playback_in_session_paged_json(
    handle: u64,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::seek_playback_in_session(handle as u32, cursor_seconds, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn set_playback_rate_in_session_paged_json(
    handle: u64,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::set_playback_rate_in_session(handle as u32, rate, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn tick_playback_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::tick_playback_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn tick_bad_autopilot_in_session_paged_json(
    handle: u64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::tick_bad_autopilot_in_session(handle as u32, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn select_chart_in_session_json(handle: u64, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_chart_in_session(handle as u32, &chart_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_chart_reference_in_session_json(
    handle: u64,
    family_id_json: &str,
    suggested_chart_ids_json: &str,
) -> Result<String, String> {
    let family_id: String = serde_json::from_str(family_id_json).map_err(|err| err.to_string())?;
    let suggested_chart_ids: Vec<String> =
        serde_json::from_str(suggested_chart_ids_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_chart_reference_in_session(
        handle as u32,
        &family_id,
        &suggested_chart_ids,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn set_map_layer_visibility_in_session_paged_json(
    handle: u64,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, String> {
    let layer_id: app_core::MapLayerId =
        serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::set_map_layer_visibility_in_session(handle as u32, layer_id, visible)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn set_map_layer_enabled_in_session_paged_json(
    handle: u64,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let layer_id: app_core::MapLayerId =
        serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::set_map_layer_enabled_in_session(handle as u32, layer_id, enabled)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

pub fn get_session_snapshot_paged_json(handle: u64) -> Result<String, String> {
    let outcome = app_core::get_session_snapshot(handle as u32).map_err(|err| err.to_string())?;
    let serialized = serde_json::to_string(&outcome).map_err(|err| err.to_string())?;
    app_core::record_session_serialized_payload_bytes(handle as u32, serialized.len());
    Ok(serialized)
}

pub fn get_session_snapshot_at_epoch_ms_paged_json(
    handle: u64,
    epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::get_session_snapshot_at_epoch_ms(handle as u32, epoch_ms)
        .map_err(|err| err.to_string())?;
    let serialized = serde_json::to_string(&outcome).map_err(|err| err.to_string())?;
    app_core::record_session_serialized_payload_bytes(handle as u32, serialized.len());
    Ok(serialized)
}

pub fn maintain_nav_db_in_session_at_epoch_ms_json(
    handle: u64,
    epoch_ms: i64,
) -> Result<String, String> {
    let outcome = app_core::maintain_nav_db_in_session_at_epoch_ms(handle as u32, epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn restore_chart_page_state_in_session_json(
    handle: u64,
    recent_airport_ids_json: &str,
    plate_target_airport_id_json: &str,
    selected_airport_id_json: &str,
    selected_reference_family_id_json: &str,
    selected_chart_id_json: &str,
    suggested_chart_ids_json: &str,
) -> Result<String, String> {
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let plate_target_airport_id: Option<String> =
        serde_json::from_str(plate_target_airport_id_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_reference_family_id: Option<String> =
        serde_json::from_str(selected_reference_family_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let suggested_chart_ids: Vec<String> =
        serde_json::from_str(suggested_chart_ids_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::restore_chart_page_state_in_session(
        handle as u32,
        &recent_airport_ids,
        plate_target_airport_id.as_deref(),
        selected_airport_id.as_deref(),
        selected_reference_family_id.as_deref(),
        selected_chart_id.as_deref(),
        &suggested_chart_ids,
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

pub fn report_session_resource_failure_in_session_json(
    handle: u64,
    resource_id: &str,
    message: &str,
) -> Result<String, String> {
    let outcome = app_core::report_session_resource_failure_in_session_at_epoch_ms(
        handle as u32,
        resource_id,
        message,
        now_epoch_ms(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn drain_session_resource_effects_json(handle: u64) -> Result<String, String> {
    let effects =
        app_core::drain_session_resource_effects(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&effects).map_err(|err| err.to_string())
}

pub fn sync_live_feeds_in_session_json(handle: u64) -> Result<String, String> {
    let outcome =
        app_core::sync_live_feeds_in_session(handle as u32).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

pub fn configure_live_feed_source_in_session_json(
    handle: u64,
    source_root_url: &str,
) -> Result<String, String> {
    app_core::configure_live_feed_source_in_session(handle as u32, source_root_url)
        .map_err(|err| err.to_string())?;
    Ok("null".to_string())
}

pub fn configure_data_sources_in_session_json(
    handle: u64,
    cycle_data_base_url: &str,
    live_feeds_base_url: &str,
    debug_log_sink_url: Option<&str>,
) -> Result<String, String> {
    let outcome = app_core::configure_data_sources_in_session(
        handle as u32,
        cycle_data_base_url,
        live_feeds_base_url,
        debug_log_sink_url,
    )
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

pub fn report_live_feed_acquisition_phase_in_session_json(
    handle: u64,
    product: &str,
    phase: &str,
) -> Result<String, String> {
    let phase = match phase {
        "idle" => app_core::WindsAloftAcquisitionPhase::Idle,
        "requested" => app_core::WindsAloftAcquisitionPhase::Requested,
        "downloading" => app_core::WindsAloftAcquisitionPhase::Downloading,
        "installing" => app_core::WindsAloftAcquisitionPhase::Installing,
        _ => return Err(format!("unsupported live-feed acquisition phase: {phase}")),
    };
    let outcome =
        app_core::report_live_feed_acquisition_phase_in_session(handle as u32, product, phase)
            .map_err(|error| error.to_string())?;
    serde_json::to_string(&outcome).map_err(|error| error.to_string())
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

pub fn get_map_selection_distance_in_session_json(
    handle: u64,
    target_json: &str,
) -> Result<String, String> {
    let target: app_core::LatLon =
        serde_json::from_str(target_json).map_err(|err| err.to_string())?;
    let distance = app_core::get_map_selection_distance_in_session(handle as u32, target)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&distance).map_err(|err| err.to_string())
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

pub fn get_raster_tile_plan_in_session_with_display_scale_json(
    handle: u64,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan = app_core::get_raster_tile_plan_in_session_with_display_scale_at_epoch_ms(
        handle as u32,
        viewport,
        width_px,
        height_px,
        device_pixel_ratio,
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
static NEXT_SESSION_SNAPSHOT_REFRESH_SCHEDULER_HANDLE: AtomicU32 = AtomicU32::new(1);
static SESSION_SNAPSHOT_REFRESH_SCHEDULERS: OnceLock<
    Mutex<HashMap<u32, app_core::SessionSnapshotRefreshScheduler>>,
> = OnceLock::new();

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

fn prepare_then_with_live_feed_caches<T, R>(
    prepare: impl FnOnce() -> Result<T, String>,
    commit: impl FnOnce(&mut HashMap<u32, app_core::LiveFeedCache>, T) -> Result<R, String>,
) -> Result<R, String> {
    // Keep package hashing, archive inspection, and projection construction out
    // of the process-wide cache registry lock. The closure boundary is covered
    // by a concurrency regression test below.
    let prepared = prepare()?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    commit(&mut caches, prepared)
}

fn ui_session_work_schedulers() -> &'static Mutex<HashMap<u32, app_core::UiSessionWorkScheduler>> {
    UI_SESSION_WORK_SCHEDULERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_snapshot_refresh_schedulers(
) -> &'static Mutex<HashMap<u32, app_core::SessionSnapshotRefreshScheduler>> {
    SESSION_SNAPSHOT_REFRESH_SCHEDULERS.get_or_init(|| Mutex::new(HashMap::new()))
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

pub fn create_live_feed_cache_json(
    source_root_url: &str,
    installed_states_json: Option<&str>,
) -> Result<u64, String> {
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
            app_core::LiveFeedCache::with_source_root_url_and_installed(
                source_root_url,
                installed_states,
            )
            .map_err(|err| err.to_string())?,
        );
    Ok(handle as u64)
}

pub fn live_feed_events_url_json(source_root_url: &str) -> Result<String, String> {
    app_core::live_feed_events_url(source_root_url).map_err(|err| err.to_string())
}

pub fn live_feed_status_url_json(source_root_url: &str) -> Result<String, String> {
    app_core::live_feed_status_url(source_root_url).map_err(|err| err.to_string())
}

pub fn normalize_live_feed_source_root_url_json(source_root_url: &str) -> Result<String, String> {
    app_core::normalize_live_feed_source_root_url(source_root_url).map_err(|err| err.to_string())
}

pub fn live_feed_cache_runtime_decision_json(
    handle: u64,
    input_json: &str,
) -> Result<String, String> {
    let input: app_core::LiveFeedRuntimeInput =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.runtime_decision(input)).map_err(|err| err.to_string())
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

pub fn create_session_snapshot_refresh_scheduler() -> Result<u64, String> {
    let handle = NEXT_SESSION_SNAPSHOT_REFRESH_SCHEDULER_HANDLE.fetch_add(1, Ordering::Relaxed);
    session_snapshot_refresh_schedulers()
        .lock()
        .map_err(|_| "session snapshot refresh scheduler store poisoned".to_string())?
        .insert(handle, app_core::SessionSnapshotRefreshScheduler::default());
    Ok(handle as u64)
}

fn session_snapshot_refresh_scheduler_decision_json(
    handle: u64,
    work: impl FnOnce(
        &mut app_core::SessionSnapshotRefreshScheduler,
    ) -> app_core::SessionSnapshotRefreshDecision,
) -> Result<String, String> {
    let mut schedulers = session_snapshot_refresh_schedulers()
        .lock()
        .map_err(|_| "session snapshot refresh scheduler store poisoned".to_string())?;
    let scheduler = schedulers
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid session snapshot refresh scheduler handle: {handle}"))?;
    serde_json::to_string(&work(scheduler)).map_err(|err| err.to_string())
}

pub fn session_snapshot_refresh_scheduler_request_json(
    handle: u64,
    now_ms: u64,
    priority_json: &str,
    reason: &str,
) -> Result<String, String> {
    let priority: app_core::SessionSnapshotRefreshPriority =
        serde_json::from_str(priority_json).map_err(|err| err.to_string())?;
    session_snapshot_refresh_scheduler_decision_json(handle, |scheduler| {
        scheduler.request(now_ms, priority, reason)
    })
}

pub fn session_snapshot_refresh_scheduler_viewport_gesture_active_changed_json(
    handle: u64,
    now_ms: u64,
    active: bool,
) -> Result<String, String> {
    session_snapshot_refresh_scheduler_decision_json(handle, |scheduler| {
        scheduler.viewport_gesture_active_changed(now_ms, active)
    })
}

pub fn session_snapshot_refresh_scheduler_viewport_activity_json(
    handle: u64,
    now_ms: u64,
) -> Result<String, String> {
    session_snapshot_refresh_scheduler_decision_json(handle, |scheduler| {
        scheduler.viewport_activity(now_ms)
    })
}

pub fn session_snapshot_refresh_scheduler_refresh_completed_json(
    handle: u64,
    now_ms: u64,
) -> Result<String, String> {
    session_snapshot_refresh_scheduler_decision_json(handle, |scheduler| {
        scheduler.refresh_completed(now_ms)
    })
}

pub fn session_snapshot_refresh_scheduler_poll_json(
    handle: u64,
    now_ms: u64,
) -> Result<String, String> {
    session_snapshot_refresh_scheduler_decision_json(handle, |scheduler| scheduler.poll(now_ms))
}

pub fn destroy_session_snapshot_refresh_scheduler(handle: u64) -> Result<(), String> {
    session_snapshot_refresh_schedulers()
        .lock()
        .map_err(|_| "session snapshot refresh scheduler store poisoned".to_string())?
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
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.missing_requests_at_epoch_ms(epoch_ms))
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_apply_session_policy(
    handle: u64,
    session_handle: u64,
) -> Result<(), String> {
    let directive =
        app_core::live_feed_cache_acquisition_directive_in_session(session_handle as u32)
            .map_err(|error| error.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache.apply_acquisition_directive(directive);
    Ok(())
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
    if matches!(
        &request.kind,
        app_core::LiveFeedCacheRequestKind::Full { .. }
    ) {
        let plan = {
            let caches = live_feed_caches()
                .lock()
                .map_err(|_| "live feed cache store poisoned".to_string())?;
            caches
                .get(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .full_install_plan(&app_core::live_feed_product_registry(), &request)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "full request produced no install plan".to_string())?
        };
        // Blob hashing, archive inspection, and any offline package preparation
        // run without the cache registry lock. Only the validated-state swap is locked.
        return prepare_then_with_live_feed_caches(
            || {
                plan.install(app_core::LiveFeedFetchedPayload::Bytes(bytes.to_vec()))
                    .map_err(|error| error.to_string())
            },
            |caches, installed| {
                let summary = installed.summary();
                caches
                    .get_mut(&(handle as u32))
                    .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                    .commit_prepared_full_install(&request, installed)
                    .map_err(|error| error.to_string())?;
                serde_json::to_string(&Some(summary)).map_err(|error| error.to_string())
            },
        );
    }
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

pub fn live_feed_cache_retained_summaries_json(
    handle: u64,
    product: &str,
) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(&cache.retained_summaries(product)).map_err(|err| err.to_string())
}

pub fn live_feed_cache_release_persisted_payload_bytes(
    handle: u64,
    product: &str,
    version: &str,
) -> Result<(), String> {
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .release_persisted_payload_bytes(product, version)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_ingest_installed_payload_bytes(
    handle: u64,
    summary_json: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let summary: app_core::LiveFeedInstalledSummary =
        serde_json::from_str(summary_json).map_err(|err| err.to_string())?;
    // Hashing and parsing a persisted package can be expensive. Prepare it before
    // taking the cache registry lock; committing the validated state is tiny.
    prepare_then_with_live_feed_caches(
        || {
            app_core::prepare_installed_payload_bytes(
                &app_core::live_feed_product_registry(),
                &summary,
                bytes,
            )
            .map_err(|err| err.to_string())
        },
        |caches, installed| {
            let cache = caches
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
            cache.ingest_prepared_installed_state(installed);
            Ok(())
        },
    )
}

pub fn live_feed_cache_ingest_persisted_nav_kv_package_descriptor(
    handle: u64,
    summary_json: &str,
    manifest: Vec<u8>,
    root: Vec<u8>,
) -> Result<(), String> {
    let summary: app_core::LiveFeedInstalledSummary =
        serde_json::from_str(summary_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
        .ingest_persisted_nav_kv_package_descriptor(
            &app_core::live_feed_product_registry(),
            &summary,
            manifest,
            root,
        )
        .map_err(|error| error.to_string())
}

pub fn live_feed_cache_ingest_persisted_notam_resource_descriptor(
    handle: u64,
    manifest_json: &str,
    prepared: Vec<u8>,
) -> Result<(), String> {
    let manifest: app_core::LiveFeedResourceManifest =
        serde_json::from_str(manifest_json).map_err(|err| err.to_string())?;
    prepare_then_with_live_feed_caches(
        || {
            app_core::live_feed_cache::prepare_persisted_notam_resource_descriptor(
                manifest, prepared,
            )
            .map_err(|error| error.to_string())
        },
        |caches, descriptor| {
            caches
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .commit_prepared_resource_restoration(descriptor);
            Ok(())
        },
    )
}

pub fn live_feed_cache_notam_resources_require_hydration(
    handle: u64,
    version: &str,
) -> Result<bool, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
        .notam_resources_require_hydration(version)
        .map_err(|error| error.to_string())
}

pub fn live_feed_cache_installed_payload_bytes(
    handle: u64,
    product: &str,
    version: &str,
) -> Result<Vec<u8>, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .installed_payload_bytes(product, version)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_resource_manifest_json(
    handle: u64,
    product: &str,
) -> Result<String, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    serde_json::to_string(
        &cache
            .resource_manifest(product)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

pub fn live_feed_cache_resource_bytes(
    handle: u64,
    product: &str,
    blob_sha256: &str,
) -> Result<Vec<u8>, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .resource_bytes(product, blob_sha256)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_begin_restoring_resources(
    handle: u64,
    manifest_json: &str,
) -> Result<(), String> {
    let manifest: app_core::LiveFeedResourceManifest =
        serde_json::from_str(manifest_json).map_err(|err| err.to_string())?;
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .begin_restoring_resources(manifest)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_restore_resource_bytes(
    handle: u64,
    product: &str,
    blob_sha256: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache
        .restore_resource_bytes(product, blob_sha256, bytes)
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_abort_restoring_resources(handle: u64, product: &str) -> Result<(), String> {
    let mut caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    let cache = caches
        .get_mut(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
    cache.abort_restoring_resources(product);
    Ok(())
}

pub fn live_feed_cache_finish_restoring_resources(
    handle: u64,
    product: &str,
) -> Result<(), String> {
    let plan = {
        let mut caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get_mut(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .take_resource_restoration_plan(product)
            .map_err(|error| error.to_string())?
    };
    prepare_then_with_live_feed_caches(
        || {
            plan.prepare(&app_core::live_feed_product_registry())
                .map_err(|error| error.to_string())
        },
        |caches, prepared| {
            caches
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .commit_prepared_resource_restoration(prepared);
            Ok(())
        },
    )
}

pub fn live_feed_cache_finish_hydrating_notam_resources(handle: u64) -> Result<(), String> {
    let plan = {
        let mut caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get_mut(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .take_resource_restoration_plan("notams")
            .map_err(|error| error.to_string())?
    };
    prepare_then_with_live_feed_caches(
        || {
            plan.prepare_notam_hydration(&app_core::live_feed_product_registry())
                .map_err(|error| error.to_string())
        },
        |caches, (installed, preparer)| {
            caches
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .commit_notam_resource_hydration(installed, preparer)
                .map_err(|error| error.to_string())
        },
    )
}

pub fn live_feed_cache_install_product_in_session_json(
    handle: u64,
    session_handle: u64,
    product: &str,
    version: &str,
) -> Result<String, String> {
    let installed = {
        let caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .install_candidate_for_main(product, version)
            .map_err(|err| err.to_string())?
    };
    let snapshot = match app_core::install_validated_live_feed_installed_state_in_session(
        session_handle as u32,
        &installed,
    ) {
        Ok(snapshot) => {
            live_feed_caches()
                .lock()
                .map_err(|_| "live feed cache store poisoned".to_string())?
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .acknowledge_install_candidate(product, &installed.version)
                .map_err(|err| err.to_string())?;
            snapshot
        }
        Err(error) => {
            if let Ok(mut caches) = live_feed_caches().lock() {
                if let Some(cache) = caches.get_mut(&(handle as u32)) {
                    cache.reject_install_candidate(product);
                }
            }
            return Err(error.to_string());
        }
    };
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn live_feed_cache_prepared_install_candidate(
    handle: u64,
    product: &str,
    version: &str,
) -> Result<Vec<u8>, String> {
    let caches = live_feed_caches()
        .lock()
        .map_err(|_| "live feed cache store poisoned".to_string())?;
    caches
        .get(&(handle as u32))
        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
        .prepared_install_candidate(product, version)
        .map(|candidate| candidate.unwrap_or_default())
        .map_err(|err| err.to_string())
}

pub fn live_feed_cache_install_prepared_product_in_session_json(
    handle: u64,
    session_handle: u64,
    product: &str,
    version: &str,
    prepared_bytes: &[u8],
) -> Result<String, String> {
    let installed = {
        let caches = live_feed_caches()
            .lock()
            .map_err(|_| "live feed cache store poisoned".to_string())?;
        caches
            .get(&(handle as u32))
            .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
            .install_candidate_state(product, version)
            .map_err(|err| err.to_string())?
    };
    let snapshot = match app_core::install_prepared_live_feed_cache_product_in_session(
        session_handle as u32,
        &installed,
        prepared_bytes,
    ) {
        Ok(snapshot) => {
            live_feed_caches()
                .lock()
                .map_err(|_| "live feed cache store poisoned".to_string())?
                .get_mut(&(handle as u32))
                .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?
                .acknowledge_install_candidate(product, &installed.version)
                .map_err(|err| err.to_string())?;
            snapshot
        }
        Err(error) => {
            if let Ok(mut caches) = live_feed_caches().lock() {
                if let Some(cache) = caches.get_mut(&(handle as u32)) {
                    cache.reject_install_candidate(product);
                }
            }
            return Err(error.to_string());
        }
    };
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
    create_nav_db_open_controller(candidates)
}

pub fn nav_db_open_controller_create_from_installed_artifacts_json(
    installed_artifacts_json: &str,
    library_cache_json: Option<&str>,
) -> Result<u64, String> {
    let installed: Vec<app_core::InstalledArtifact> =
        serde_json::from_str(installed_artifacts_json).map_err(|err| err.to_string())?;
    let library_cache = library_cache_json
        .filter(|json| !json.trim().is_empty())
        .map(|json| {
            serde_json::from_str::<app_core::OfflinePackagesLibraryCache>(json)
                .map_err(|err| err.to_string())
        })
        .transpose()?;
    let candidates = app_core::nav_db_artifact_candidates_from_installed_artifacts(
        &installed,
        library_cache.as_ref(),
    )?;
    create_nav_db_open_controller(candidates)
}

fn create_nav_db_open_controller(
    candidates: Vec<app_core::NavDbArtifactCandidate>,
) -> Result<u64, String> {
    let handle = NEXT_NAV_DB_OPEN_HANDLE.fetch_add(1, Ordering::Relaxed);
    nav_db_open_controllers()
        .lock()
        .map_err(|_| "nav db open controller store poisoned".to_string())?
        .insert(
            handle,
            app_core::NavDbOpenController::new_at_epoch_ms(candidates, now_epoch_ms()),
        );
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

pub fn debug_drop_nav_kv_pages_for_attached_sessions(handle: u64) -> Result<(), String> {
    let stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    if !stores.contains_key(&(handle as u32)) {
        return Err(format!("invalid nav kv handle: {handle}"));
    }
    app_core::debug_drop_nav_kv_pages_for_attached_sessions(handle as u32);
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
    app_core::attach_nav_kv_store_to_session_with_open_result(
        session_handle as u32,
        nav_kv_handle as u32,
        &store.store,
        store.open_result.as_ref(),
    )
    .map_err(|err| err.to_string())
}

pub fn advance_nav_kv_store_in_session_json(
    nav_kv_handle: u64,
    session_handle: u64,
    installed_package_ids_json: &str,
) -> Result<String, String> {
    let installed_package_ids: Vec<String> =
        serde_json::from_str(installed_package_ids_json).map_err(|err| err.to_string())?;
    let stores = nav_kv_stores()
        .lock()
        .map_err(|_| "nav kv store poisoned".to_string())?;
    let stored = stores
        .get(&(nav_kv_handle as u32))
        .ok_or_else(|| format!("invalid nav kv handle: {nav_kv_handle}"))?;
    let open_result = stored
        .open_result
        .as_ref()
        .ok_or_else(|| "candidate nav kv store has no artifact identity".to_string())?;
    let outcome = app_core::advance_nav_kv_store_in_session_with_open_result(
        session_handle as u32,
        nav_kv_handle as u32,
        &stored.store,
        open_result,
        installed_package_ids,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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
    now_epoch_ms: i64,
    discovery_jsons: Vec<String>,
    bundle_jsons_by_filename: BTreeMap<String, String>,
    installed: Vec<app_core::InstalledArtifact>,
}

#[derive(Deserialize)]
struct OfflinePackagesReduceInputWire {
    state: app_core::OfflinePackagesState,
    event: app_core::OfflinePackagesEvent,
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
#[serde(tag = "kind", rename_all = "snake_case")]
enum OfflinePackagesControllerEventWire {
    EnsureLibrary,
    RefreshLibraryRequested,
    LibraryRefreshSucceeded(OfflinePackagesControllerLibraryRefreshSucceededWire),
    LibraryRefreshFailed {
        message: String,
    },
    PackagesEvent {
        event: app_core::OfflinePackagesEvent,
    },
    ApplySynchronizedPreferences {
        preferences_json: String,
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
    preferences_for_cloud_json: Option<String>,
    installed_metadata_updates: Vec<app_core::InstalledArtifactMetadataUpdate>,
}

#[derive(Deserialize)]
struct CurrentArtifactsDiscoveryInputWire {
    publication_root_url: String,
    current_artifacts_json: String,
}

pub fn plan_offline_packages_from_bundle_json(input_json: &str) -> Result<String, String> {
    let input: BundlePackageManagementInputWire =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let bundle = app_core::decode_bundle_manifest(&input.bundle_json)?;
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
        .map(|payload| app_core::decode_current_artifacts_manifest(&payload))
        .collect::<Result<Vec<_>, _>>()?;
    let bundle_manifests_by_filename = input
        .bundle_jsons_by_filename
        .into_iter()
        .map(|(filename, payload)| {
            app_core::decode_bundle_manifest(&payload).map(|bundle| (filename, bundle))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let result = app_core::initialize_offline_packages(&app_core::OfflinePackagesInitInput {
        state: input.state,
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
        .map(|payload| app_core::decode_current_artifacts_manifest(&payload))
        .collect::<Result<Vec<_>, _>>()?;
    let bundle_manifests_by_filename = input
        .bundle_jsons_by_filename
        .into_iter()
        .map(|(filename, payload)| {
            app_core::decode_bundle_manifest(&payload).map(|bundle| (filename, bundle))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let result = app_core::reduce_offline_packages(&app_core::OfflinePackagesReduceInput {
        state: input.state,
        event: input.event,
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
    let (event, library_cache_changed) = match input.event {
        OfflinePackagesControllerEventWire::EnsureLibrary => (
            app_core::OfflinePackagesControllerEvent::EnsureLibrary,
            false,
        ),
        OfflinePackagesControllerEventWire::RefreshLibraryRequested => (
            app_core::OfflinePackagesControllerEvent::RefreshLibraryRequested,
            false,
        ),
        OfflinePackagesControllerEventWire::LibraryRefreshSucceeded(payload) => {
            let discovery_manifests = payload
                .discovery_jsons
                .into_iter()
                .map(|json| app_core::decode_current_artifacts_manifest(&json))
                .collect::<Result<Vec<_>, _>>()?;
            let bundle_manifests_by_filename = payload
                .bundle_jsons_by_filename
                .into_iter()
                .map(|(filename, json)| {
                    app_core::decode_bundle_manifest(&json).map(|bundle| (filename, bundle))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            (
                app_core::OfflinePackagesControllerEvent::LibraryRefreshSucceeded {
                    fetched_at_epoch_ms: payload.fetched_at_epoch_ms,
                    discovery_manifests,
                    bundle_manifests_by_filename,
                },
                true,
            )
        }
        OfflinePackagesControllerEventWire::LibraryRefreshFailed { message } => (
            app_core::OfflinePackagesControllerEvent::LibraryRefreshFailed { message },
            false,
        ),
        OfflinePackagesControllerEventWire::PackagesEvent { event } => (
            app_core::OfflinePackagesControllerEvent::PackagesEvent { event },
            false,
        ),
        OfflinePackagesControllerEventWire::ApplySynchronizedPreferences { preferences_json } => (
            app_core::OfflinePackagesControllerEvent::ApplySynchronizedPreferences {
                preferences: serde_json::from_str(&preferences_json)
                    .map_err(|err| err.to_string())?,
            },
            false,
        ),
        OfflinePackagesControllerEventWire::SyncRequested => (
            app_core::OfflinePackagesControllerEvent::SyncRequested,
            false,
        ),
        OfflinePackagesControllerEventWire::SyncProgressObserved { progress } => (
            app_core::OfflinePackagesControllerEvent::SyncProgressObserved { progress },
            false,
        ),
        OfflinePackagesControllerEventWire::SyncFinished { summary } => (
            app_core::OfflinePackagesControllerEvent::SyncFinished { summary },
            false,
        ),
    };
    // Decode every fallible caller-controlled payload before transferring ownership
    // of the opaque controller out of the handle table.
    let mut controllers = offline_packages_controllers()
        .lock()
        .map_err(|_| "offline packages controller store poisoned".to_string())?;
    let state = controllers
        .remove(&(handle as u32))
        .ok_or_else(|| format!("invalid offline packages controller handle: {handle}"))?;
    let installed = input.installed.clone();
    let result = app_core::reduce_offline_packages_controller_owned(
        app_core::OfflinePackagesControllerInput {
            state: Some(state),
            package_source_base_url: input.package_source_base_url,
            discovery_filenames: input.discovery_filenames,
            now_epoch_ms: input.now_epoch_ms,
            installed: input.installed,
            storage: input.storage,
            event,
        },
    );
    let packages_state_json = result
        .state
        .packages_state
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())?;
    let library_cache_json = if library_cache_changed {
        result
            .state
            .library_cache
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| err.to_string())?
    } else {
        None
    };
    let preferences_for_cloud_json = result
        .preferences_for_cloud
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())?;
    let installed_metadata_updates = result
        .state
        .library_cache
        .as_ref()
        .map(|cache| {
            app_core::installed_artifact_metadata_updates(
                &cache.bundle_manifests_by_filename,
                &installed,
            )
        })
        .unwrap_or_default();
    let wire = OfflinePackagesControllerResultWire {
        packages_state_json,
        library_cache_json,
        ui_state: result.ui_state,
        command: result.command,
        preferences_for_cloud_json,
        installed_metadata_updates,
    };
    controllers.insert(handle as u32, result.state);
    serde_json::to_string(&wire).map_err(|err| err.to_string())
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setCorePerfDebugLoggingEnabled(
    _env: JNIEnv,
    _class: JClass,
    enabled: bool,
) {
    set_core_perf_debug_logging_enabled(enabled);
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createSessionSnapshotRefreshScheduler(
    mut env: JNIEnv,
    _class: JClass,
) -> i64 {
    match create_session_snapshot_refresh_scheduler() {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionSnapshotRefreshSchedulerRequestJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_ms: i64,
    priority_json: JString,
    reason: JString,
) -> jstring {
    let result = (|| {
        let priority_json = get_java_string(&mut env, priority_json)?;
        let reason = get_java_string(&mut env, reason)?;
        session_snapshot_refresh_scheduler_request_json(
            handle as u64,
            now_ms.max(0) as u64,
            &priority_json,
            &reason,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionSnapshotRefreshSchedulerRefreshCompletedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        session_snapshot_refresh_scheduler_refresh_completed_json(
            handle as u64,
            now_ms.max(0) as u64,
        ),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionSnapshotRefreshSchedulerViewportGestureActiveChangedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_ms: i64,
    active: bool,
) -> jstring {
    return_string(
        &mut env,
        session_snapshot_refresh_scheduler_viewport_gesture_active_changed_json(
            handle as u64,
            now_ms.max(0) as u64,
            active,
        ),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionSnapshotRefreshSchedulerViewportActivityJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        session_snapshot_refresh_scheduler_viewport_activity_json(
            handle as u64,
            now_ms.max(0) as u64,
        ),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionSnapshotRefreshSchedulerPollJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        session_snapshot_refresh_scheduler_poll_json(handle as u64, now_ms.max(0) as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_destroySessionSnapshotRefreshScheduler(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = destroy_session_snapshot_refresh_scheduler(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createLiveFeedCache(
    mut env: JNIEnv,
    _class: JClass,
    source_root_url: JString,
    installed_states_json: JString,
) -> i64 {
    match get_java_string(&mut env, source_root_url).and_then(|source_root_url| {
        get_java_string(&mut env, installed_states_json).and_then(|installed_states| {
            create_live_feed_cache_json(&source_root_url, Some(&installed_states))
        })
    }) {
        Ok(handle) => handle as i64,
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_configureDataSourcesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    cycle_data_base_url: JString,
    live_feeds_base_url: JString,
    debug_log_sink_url: JString,
) -> jstring {
    let result = (|| {
        let cycle_data_base_url = get_java_string(&mut env, cycle_data_base_url)?;
        let live_feeds_base_url = get_java_string(&mut env, live_feeds_base_url)?;
        let debug_log_sink_url = get_java_string(&mut env, debug_log_sink_url)?;
        configure_data_sources_in_session_json(
            handle as u64,
            &cycle_data_base_url,
            &live_feeds_base_url,
            (!debug_log_sink_url.is_empty()).then_some(debug_log_sink_url.as_str()),
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedEventsUrl(
    mut env: JNIEnv,
    _class: JClass,
    source_root_url: JString,
) -> jstring {
    let result =
        get_java_string(&mut env, source_root_url).and_then(|url| live_feed_events_url_json(&url));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedStatusUrl(
    mut env: JNIEnv,
    _class: JClass,
    source_root_url: JString,
) -> jstring {
    let result =
        get_java_string(&mut env, source_root_url).and_then(|url| live_feed_status_url_json(&url));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_normalizeLiveFeedSourceRootUrl(
    mut env: JNIEnv,
    _class: JClass,
    source_root_url: JString,
) -> jstring {
    let result = get_java_string(&mut env, source_root_url)
        .and_then(|url| normalize_live_feed_source_root_url_json(&url));
    return_string(&mut env, result)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheRuntimeDecisionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    input_json: JString,
) -> jstring {
    let result = get_java_string(&mut env, input_json)
        .and_then(|input| live_feed_cache_runtime_decision_json(handle as u64, &input));
    return_string(&mut env, result)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheRetainedSummariesJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) -> jstring {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_retained_summaries_json(handle as u64, &product));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheReleasePersistedPayloadBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    version: JString,
) {
    let result = (|| {
        let product = get_java_string(&mut env, product)?;
        let version = get_java_string(&mut env, version)?;
        live_feed_cache_release_persisted_payload_bytes(handle as u64, &product, &version)
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheIngestPersistedNavKvPackageDescriptor(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    summary_json: JString,
    manifest_bytes: JByteArray,
    root_bytes: JByteArray,
) {
    let result = (|| {
        let summary = get_java_string(&mut env, summary_json)?;
        let manifest = get_java_byte_array(&mut env, manifest_bytes)?;
        let root = get_java_byte_array(&mut env, root_bytes)?;
        live_feed_cache_ingest_persisted_nav_kv_package_descriptor(
            handle as u64,
            &summary,
            manifest,
            root,
        )
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheIngestPersistedNotamResourceDescriptor(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    manifest_json: JString,
    prepared_bytes: JByteArray,
) {
    let result = (|| {
        let manifest = get_java_string(&mut env, manifest_json)?;
        let prepared = get_java_byte_array(&mut env, prepared_bytes)?;
        live_feed_cache_ingest_persisted_notam_resource_descriptor(
            handle as u64,
            &manifest,
            prepared,
        )
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheNotamResourcesRequireHydration(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    version: JString,
) -> jboolean {
    let result = get_java_string(&mut env, version).and_then(|version| {
        live_feed_cache_notam_resources_require_hydration(handle as u64, &version)
    });
    match result {
        Ok(required) => u8::from(required),
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstalledPayloadBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    version: JString,
) -> jbyteArray {
    let result = get_java_string(&mut env, product).and_then(|product| {
        get_java_string(&mut env, version).and_then(|version| {
            live_feed_cache_installed_payload_bytes(handle as u64, &product, &version)
        })
    });
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheResourceManifestJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) -> jstring {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_resource_manifest_json(handle as u64, &product));
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheResourceBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    blob_sha256: JString,
) -> jbyteArray {
    let result = get_java_string(&mut env, product).and_then(|product| {
        get_java_string(&mut env, blob_sha256).and_then(|blob_sha256| {
            live_feed_cache_resource_bytes(handle as u64, &product, &blob_sha256)
        })
    });
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheBeginRestoringResources(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    manifest_json: JString,
) {
    let result = get_java_string(&mut env, manifest_json).and_then(|manifest_json| {
        live_feed_cache_begin_restoring_resources(handle as u64, &manifest_json)
    });
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheRestoreResourceBytes(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    blob_sha256: JString,
    resource_bytes: JByteArray,
) {
    let result = (|| {
        let product = get_java_string(&mut env, product)?;
        let blob_sha256 = get_java_string(&mut env, blob_sha256)?;
        let bytes = get_java_byte_array(&mut env, resource_bytes)?;
        live_feed_cache_restore_resource_bytes(handle as u64, &product, &blob_sha256, &bytes)
    })();
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheAbortRestoringResources(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_abort_restoring_resources(handle as u64, &product));
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheFinishRestoringResources(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
) {
    let result = get_java_string(&mut env, product)
        .and_then(|product| live_feed_cache_finish_restoring_resources(handle as u64, &product));
    if let Err(message) = result {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheFinishHydratingNotamResources(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = live_feed_cache_finish_hydrating_notam_resources(handle as u64) {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstallProductInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    session_handle: i64,
    product: JString,
    version: JString,
) -> jstring {
    let result = get_java_string(&mut env, product).and_then(|product| {
        get_java_string(&mut env, version).and_then(|version| {
            live_feed_cache_install_product_in_session_json(
                handle as u64,
                session_handle as u64,
                &product,
                &version,
            )
        })
    });
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCachePreparedInstallCandidate(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    version: JString,
) -> jbyteArray {
    let result = get_java_string(&mut env, product).and_then(|product| {
        get_java_string(&mut env, version).and_then(|version| {
            live_feed_cache_prepared_install_candidate(handle as u64, &product, &version)
        })
    });
    return_byte_array(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheInstallPreparedProductInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    session_handle: i64,
    product: JString,
    version: JString,
    prepared_bytes: JByteArray,
) -> jstring {
    let result = (|| {
        let product = get_java_string(&mut env, product)?;
        let version = get_java_string(&mut env, version)?;
        let prepared_bytes = get_java_byte_array(&mut env, prepared_bytes)?;
        live_feed_cache_install_prepared_product_in_session_json(
            handle as u64,
            session_handle as u64,
            &product,
            &version,
            &prepared_bytes,
        )
    })();
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_liveFeedCacheApplySessionPolicy(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    session_handle: i64,
) {
    if let Err(message) = live_feed_cache_apply_session_policy(handle as u64, session_handle as u64)
    {
        let _ = env.throw_new("java/lang/RuntimeException", message);
    }
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_createUiSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        create_ui_session_json(
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_sessionDiagnosticsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, session_diagnostics_json(handle as u64))
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_loadOfflinePackageLibraryCacheInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    library_cache_json: JString,
) -> jstring {
    let result = (|| {
        let library_cache_json = get_java_string(&mut env, library_cache_json)?;
        load_offline_package_library_cache_in_session_json(handle as u64, &library_cache_json)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navigationPageStateJson(
    mut env: JNIEnv,
    _class: JClass,
    capabilities_json: JString,
) -> jstring {
    let result = (|| {
        let capabilities_json = get_java_string(&mut env, capabilities_json)?;
        navigation_page_state_json(&capabilities_json)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performFlightPlanCommandInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    command_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let command_json = get_java_string(&mut env, command_json)?;
        perform_flight_plan_command_in_session_json(handle as u64, &command_json, now_epoch_ms)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performTimeDisplayActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        perform_time_display_action_in_session_json(handle as u64, &action_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performFlightDataBannerCellActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    cell_id: JString,
) -> jstring {
    let result = (|| {
        let cell_id = get_java_string(&mut env, cell_id)?;
        perform_flight_data_banner_cell_action_in_session_json(handle as u64, &cell_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performFlightPlanColumnActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        perform_flight_plan_column_action_in_session_json(handle as u64, &action_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_queryFlightPlanInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    query_json: JString,
) -> jstring {
    let result = (|| {
        let query_json = get_java_string(&mut env, query_json)?;
        query_flight_plan_in_session_json(handle as u64, &query_json)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_statusActionDecisionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        status_action_decision_in_session_json(handle as u64, &action_id)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performOwnshipTextActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
    value: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        let value = get_java_string(&mut env, value)?;
        perform_ownship_text_action_in_session_json(handle as u64, &action_id, &value, now_epoch_ms)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performSettingsActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let action_json = get_java_string(&mut env, action_json)?;
        perform_settings_action_in_session_json(handle as u64, &action_json, now_epoch_ms)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performAircraftLibraryActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id: JString,
    source_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let action_id = get_java_string(&mut env, action_id)?;
        let source_json = get_java_string(&mut env, source_json)?;
        perform_aircraft_library_action_in_session_json(
            handle as u64,
            &action_id,
            &source_json,
            now_epoch_ms,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performCloudUiActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_id_json: JString,
    fields_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let action_id_json = get_java_string(&mut env, action_id_json)?;
        let fields_json = get_java_string(&mut env, fields_json)?;
        perform_cloud_ui_action_in_session_json(
            handle as u64,
            &action_id_json,
            &fields_json,
            now_epoch_ms,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_recordOfflinePackagePreferencesInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    preferences_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let preferences_json = get_java_string(&mut env, preferences_json)?;
        record_offline_package_preferences_in_session_json(
            handle as u64,
            &preferences_json,
            now_epoch_ms,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_takeCloudProviderRequestInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        take_cloud_provider_request_in_session_json(handle as u64, now_epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_completeCloudProviderRequestInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    request_id: i64,
    response_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let response_json = get_java_string(&mut env, response_json)?;
        complete_cloud_provider_request_in_session_json(
            handle as u64,
            request_id as u64,
            &response_json,
            now_epoch_ms,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_cloudEventStreamPlanInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(
        &mut env,
        cloud_event_stream_plan_in_session_json(handle as u64),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_reportCloudEventStreamEventInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    event_json: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let event_json = get_java_string(&mut env, event_json)?;
        report_cloud_event_stream_event_in_session_json(handle as u64, &event_json, now_epoch_ms)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_syncGuidanceGeometryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = sync_guidance_geometry_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_projectFlightPlanRouteInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    let result = project_flight_plan_route_in_session_json(handle as u64);
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_mapSelectionActionDecisionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_uid: JString,
) -> jstring {
    let result = (|| {
        let action_uid = get_java_string(&mut env, action_uid)?;
        map_selection_action_decision_in_session_json(handle as u64, &action_uid)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_flightPlanRowActionDecisionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    row_uid: JString,
    action_uid: JString,
) -> jstring {
    let result = (|| {
        let row_uid = get_java_string(&mut env, row_uid)?;
        let action_uid = get_java_string(&mut env, action_uid)?;
        flight_plan_row_action_decision_in_session_json(handle as u64, &row_uid, &action_uid)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_performMapSelectionUiActionInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    action_uid: JString,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let action_uid = get_java_string(&mut env, action_uid)?;
        perform_map_selection_ui_action_in_session_json(handle as u64, &action_uid, now_epoch_ms)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_openChartAirportInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    airport_id_json: JString,
    chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let airport_id = get_java_string(&mut env, airport_id_json)?;
        let chart_id = get_java_string(&mut env, chart_id_json)?;
        open_chart_airport_in_session_json(handle as u64, &airport_id, &chart_id)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setOwnshipSourceSleepingInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    source_id: JString,
    sleeping: bool,
    now_epoch_ms: i64,
) -> jstring {
    let result = (|| {
        let source_id = get_java_string(&mut env, source_id)?;
        set_ownship_source_sleeping_in_session_json(
            handle as u64,
            &source_id,
            sleeping,
            now_epoch_ms,
        )
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_tickBadAutopilotInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    now_epoch_ms: f64,
) -> jstring {
    return_string(
        &mut env,
        tick_bad_autopilot_in_session_paged_json(handle as u64, now_epoch_ms),
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_selectChartReferenceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    family_id_json: JString,
    suggested_chart_ids_json: JString,
) -> jstring {
    let result = (|| {
        let family_id = get_java_string(&mut env, family_id_json)?;
        let suggested_chart_ids = get_java_string(&mut env, suggested_chart_ids_json)?;
        select_chart_reference_in_session_json(handle as u64, &family_id, &suggested_chart_ids)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setMapLayerVisibilityInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    layer_id_json: JString,
    visible: bool,
) -> jstring {
    let result = (|| {
        let layer_id = get_java_string(&mut env, layer_id_json)?;
        set_map_layer_visibility_in_session_paged_json(handle as u64, &layer_id, visible)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_setMapLayerEnabledInSessionPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    layer_id_json: JString,
    enabled: bool,
) -> jstring {
    let result = (|| {
        let layer_id = get_java_string(&mut env, layer_id_json)?;
        set_map_layer_enabled_in_session_paged_json(handle as u64, &layer_id, enabled)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getSessionSnapshotPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, get_session_snapshot_paged_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getSessionSnapshotAtEpochMsPagedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        get_session_snapshot_at_epoch_ms_paged_json(handle as u64, epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_maintainNavDbInSessionAtEpochMsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    epoch_ms: i64,
) -> jstring {
    return_string(
        &mut env,
        maintain_nav_db_in_session_at_epoch_ms_json(handle as u64, epoch_ms),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_restoreChartPageStateInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    recent_airport_ids_json: JString,
    plate_target_airport_id_json: JString,
    selected_airport_id_json: JString,
    selected_reference_family_id_json: JString,
    selected_chart_id_json: JString,
    suggested_chart_ids_json: JString,
) -> jstring {
    let result = (|| {
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let plate_target_airport_id = get_java_string(&mut env, plate_target_airport_id_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_reference_family_id =
            get_java_string(&mut env, selected_reference_family_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        let suggested_chart_ids = get_java_string(&mut env, suggested_chart_ids_json)?;
        restore_chart_page_state_in_session_json(
            handle as u64,
            &recent_airport_ids,
            &plate_target_airport_id,
            &selected_airport_id,
            &selected_reference_family_id,
            &selected_chart_id,
            &suggested_chart_ids,
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_reportSessionResourceFailureInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    resource_id: JString,
    message: JString,
) -> jstring {
    let result = (|| {
        let resource_id = get_java_string(&mut env, resource_id)?;
        let message = get_java_string(&mut env, message)?;
        report_session_resource_failure_in_session_json(handle as u64, &resource_id, &message)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_drainSessionResourceEffectsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, drain_session_resource_effects_json(handle as u64))
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_reportLiveFeedAcquisitionPhaseInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    product: JString,
    phase: JString,
) -> jstring {
    let result = get_java_string(&mut env, product).and_then(|product| {
        get_java_string(&mut env, phase).and_then(|phase| {
            report_live_feed_acquisition_phase_in_session_json(handle as u64, &product, &phase)
        })
    });
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getMapSelectionDistanceInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    target_json: JString,
) -> jstring {
    let result = (|| {
        let target = get_java_string(&mut env, target_json)?;
        get_map_selection_distance_in_session_json(handle as u64, &target)
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_getRasterTilePlanInSessionWithDisplayScaleJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    viewport_json: JString,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
) -> jstring {
    let result = (|| {
        let viewport = get_java_string(&mut env, viewport_json)?;
        get_raster_tile_plan_in_session_with_display_scale_json(
            handle as u64,
            &viewport,
            width_px,
            height_px,
            device_pixel_ratio,
        )
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_navDbOpenControllerCreateFromInstalledArtifacts(
    mut env: JNIEnv,
    _class: JClass,
    installed_artifacts_json: JString,
    library_cache_json: JString,
) -> i64 {
    let result = (|| {
        let installed_artifacts_json = get_java_string(&mut env, installed_artifacts_json)?;
        let library_cache_json = get_java_string(&mut env, library_cache_json)?;
        nav_db_open_controller_create_from_installed_artifacts_json(
            &installed_artifacts_json,
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_debugDropNavKvPagesForAttachedSessions(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    if let Err(message) = debug_drop_nav_kv_pages_for_attached_sessions(handle as u64) {
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
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_advanceNavKvStoreInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    nav_kv_handle: i64,
    session_handle: i64,
    installed_package_ids_json: JString,
) -> jstring {
    let result = (|| {
        let installed_package_ids_json = get_java_string(&mut env, installed_package_ids_json)?;
        advance_nav_kv_store_in_session_json(
            nav_kv_handle as u64,
            session_handle as u64,
            &installed_package_ids_json,
        )
    })();
    return_string(&mut env, result)
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
    fn expensive_live_feed_preparation_does_not_hold_registry_lock() {
        let handle = create_live_feed_cache_json("https://feeds.example.test", None).unwrap();
        let (preparation_started_tx, preparation_started_rx) = std::sync::mpsc::sync_channel(0);
        let (release_preparation_tx, release_preparation_rx) = std::sync::mpsc::sync_channel(0);
        let preparation = std::thread::spawn(move || {
            prepare_then_with_live_feed_caches(
                || {
                    preparation_started_tx.send(()).unwrap();
                    release_preparation_rx.recv().unwrap();
                    Ok(())
                },
                |caches, ()| {
                    caches
                        .get(&(handle as u32))
                        .ok_or_else(|| format!("invalid live feed cache handle: {handle}"))?;
                    Ok(())
                },
            )
        });
        preparation_started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("preparation should start");

        let (query_finished_tx, query_finished_rx) = std::sync::mpsc::sync_channel(0);
        let query = std::thread::spawn(move || {
            query_finished_tx
                .send(live_feed_cache_missing_requests_json(handle))
                .unwrap();
        });
        let query_result = query_finished_rx.recv_timeout(std::time::Duration::from_secs(2));
        release_preparation_tx.send(()).unwrap();
        preparation.join().unwrap().unwrap();
        query.join().unwrap();

        query_result
            .expect("cache query must complete while package preparation is blocked")
            .expect("cache query should succeed");
        destroy_live_feed_cache_json(handle).unwrap();
    }

    #[test]
    fn navigation_policy_is_available_before_a_ui_session_exists() {
        let state: app_core::UiNavigationPageState = serde_json::from_str(
            &navigation_page_state_json(r#"{"offline_packages":{}}"#).unwrap(),
        )
        .unwrap();

        assert!(state
            .options
            .iter()
            .any(|option| option.id == app_core::UiNavigationPageId::OfflinePackages));
    }

    #[test]
    fn malformed_offline_event_does_not_destroy_controller() {
        let handle = create_offline_packages_controller_json(None, None).unwrap();
        let input = |event: serde_json::Value| {
            serde_json::json!({
                "package_source_base_url": "https://packages.example.test/",
                "discovery_filenames": [],
                "now_epoch_ms": 1_000,
                "installed": [],
                "event": event,
            })
            .to_string()
        };
        let malformed = input(serde_json::json!({
            "kind": "apply_synchronized_preferences",
            "preferences_json": "not json",
        }));

        dispatch_offline_packages_controller_json(handle, &malformed)
            .expect_err("malformed nested payload must fail");

        let valid = input(serde_json::json!({"kind": "ensure_library"}));
        let result = dispatch_offline_packages_controller_json(handle, &valid);
        assert!(
            result.is_ok(),
            "controller was lost after malformed input: {result:?}"
        );
        destroy_offline_packages_controller_json(handle).unwrap();
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

    #[test]
    fn session_snapshot_refresh_ffi_uses_shared_core_scheduler_policy() {
        let handle = create_session_snapshot_refresh_scheduler().unwrap();
        let requested: app_core::SessionSnapshotRefreshDecision = serde_json::from_str(
            &session_snapshot_refresh_scheduler_request_json(
                handle,
                1_000,
                r#""low_priority""#,
                "invalidation",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            requested,
            app_core::SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 250,
                reason: "invalidation".to_string(),
            }
        );

        let started: app_core::SessionSnapshotRefreshDecision = serde_json::from_str(
            &session_snapshot_refresh_scheduler_poll_json(handle, 1_250).unwrap(),
        )
        .unwrap();
        assert_eq!(
            started,
            app_core::SessionSnapshotRefreshDecision::Start {
                reason: "invalidation".to_string(),
            }
        );
        destroy_session_snapshot_refresh_scheduler(handle).unwrap();
    }
}
