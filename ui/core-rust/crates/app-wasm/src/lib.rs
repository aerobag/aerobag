// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use serde::Serialize;
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Reflect};

#[cfg(target_arch = "wasm32")]
const WEB_CORE_SETTINGS_STORAGE_KEY: &str = "aerobag.core.settings.v1";

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;

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
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
    }
    #[cfg(target_arch = "wasm32")]
    {
        performance_now()
    }
}

static NEXT_NAV_KV_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_KV_STORES: OnceLock<Mutex<HashMap<u32, StoredNavKvStore>>> = OnceLock::new();
static NEXT_NAV_DB_OPEN_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_DB_OPEN_CONTROLLERS: OnceLock<Mutex<HashMap<u32, app_core::NavDbOpenController>>> =
    OnceLock::new();
static NEXT_SESSION_SNAPSHOT_REFRESH_SCHEDULER_HANDLE: AtomicU32 = AtomicU32::new(1);
static SESSION_SNAPSHOT_REFRESH_SCHEDULERS: OnceLock<
    Mutex<HashMap<u32, app_core::SessionSnapshotRefreshScheduler>>,
> = OnceLock::new();
static NEXT_UI_SESSION_WORK_SCHEDULER_HANDLE: AtomicU32 = AtomicU32::new(1);
static UI_SESSION_WORK_SCHEDULERS: OnceLock<Mutex<HashMap<u32, app_core::UiSessionWorkScheduler>>> =
    OnceLock::new();
static LIVE_FEED_PREP_STATES: OnceLock<Mutex<HashMap<(String, String), serde_json::Value>>> =
    OnceLock::new();
static NOTAM_PROJECTION_PREPARER: OnceLock<Mutex<app_core::NotamProjectionPreparer>> =
    OnceLock::new();

struct StoredNavKvStore {
    store: app_core::NavKvStore,
    open_result: Option<app_core::NavDbOpenResult>,
}

fn nav_kv_stores() -> &'static Mutex<HashMap<u32, StoredNavKvStore>> {
    NAV_KV_STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_nav_kv_stores() -> MutexGuard<'static, HashMap<u32, StoredNavKvStore>> {
    nav_kv_stores()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn session_snapshot_refresh_schedulers(
) -> &'static Mutex<HashMap<u32, app_core::SessionSnapshotRefreshScheduler>> {
    SESSION_SNAPSHOT_REFRESH_SCHEDULERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_session_snapshot_refresh_schedulers(
) -> MutexGuard<'static, HashMap<u32, app_core::SessionSnapshotRefreshScheduler>> {
    session_snapshot_refresh_schedulers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_ui_session_work_schedulers(
) -> MutexGuard<'static, HashMap<u32, app_core::UiSessionWorkScheduler>> {
    UI_SESSION_WORK_SCHEDULERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_live_feed_prep_states() -> MutexGuard<'static, HashMap<(String, String), serde_json::Value>>
{
    LIVE_FEED_PREP_STATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_notam_projection_preparer() -> MutexGuard<'static, app_core::NotamProjectionPreparer> {
    NOTAM_PROJECTION_PREPARER
        .get_or_init(|| Mutex::new(app_core::NotamProjectionPreparer::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(target_arch = "wasm32")]
struct WebCoreSettingsStorage;

#[cfg(target_arch = "wasm32")]
impl app_core::SettingsStorage for WebCoreSettingsStorage {
    fn read_settings(&self) -> app_core::AppResult<Option<Vec<u8>>> {
        read_web_core_settings()
            .map(|value| value.map(String::into_bytes))
            .map_err(|message| app_core::AppError {
                kind: app_core::AppErrorKind::Internal,
                message,
            })
    }

    fn write_settings(&self, bytes: &[u8]) -> app_core::AppResult<()> {
        let value = std::str::from_utf8(bytes).map_err(|err| app_core::AppError {
            kind: app_core::AppErrorKind::Internal,
            message: err.to_string(),
        })?;
        write_web_core_settings(value).map_err(|message| app_core::AppError {
            kind: app_core::AppErrorKind::Internal,
            message,
        })
    }
}

fn web_core_settings_storage() -> Option<app_core::SettingsStorageHandle> {
    #[cfg(not(target_arch = "wasm32"))]
    return None;

    #[cfg(target_arch = "wasm32")]
    Some(Arc::new(WebCoreSettingsStorage))
}

#[cfg(target_arch = "wasm32")]
fn web_local_storage() -> Option<JsValue> {
    let global = js_sys::global();
    let storage = Reflect::get(&global, &JsValue::from_str("localStorage")).ok()?;
    if storage.is_null() || storage.is_undefined() {
        None
    } else {
        Some(storage)
    }
}

#[cfg(target_arch = "wasm32")]
fn read_web_core_settings() -> Result<Option<String>, String> {
    let value = if let Some(storage) = web_local_storage() {
        let get_item = Reflect::get(&storage, &JsValue::from_str("getItem"))
            .map_err(|err| format!("localStorage.getItem lookup failed: {err:?}"))?
            .dyn_into::<Function>()
            .map_err(|_| "localStorage.getItem is not callable".to_string())?;
        get_item
            .call1(&storage, &JsValue::from_str(WEB_CORE_SETTINGS_STORAGE_KEY))
            .map_err(|err| format!("localStorage.getItem failed: {err:?}"))?
    } else {
        Reflect::get(
            &js_sys::global(),
            &JsValue::from_str("__aerobagCoreSettingsJson"),
        )
        .map_err(|err| format!("worker core settings lookup failed: {err:?}"))?
    };
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        value
            .as_string()
            .map(Some)
            .ok_or_else(|| "localStorage core settings value is not a string".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn write_web_core_settings(value: &str) -> Result<(), String> {
    if let Some(storage) = web_local_storage() {
        let set_item = Reflect::get(&storage, &JsValue::from_str("setItem"))
            .map_err(|err| format!("localStorage.setItem lookup failed: {err:?}"))?
            .dyn_into::<Function>()
            .map_err(|_| "localStorage.setItem is not callable".to_string())?;
        set_item
            .call2(
                &storage,
                &JsValue::from_str(WEB_CORE_SETTINGS_STORAGE_KEY),
                &JsValue::from_str(value),
            )
            .map(|_| ())
            .map_err(|err| format!("localStorage.setItem failed: {err:?}"))
    } else {
        Reflect::set(
            &js_sys::global(),
            &JsValue::from_str("__aerobagCoreSettingsJson"),
            &JsValue::from_str(value),
        )
        .map(|_| ())
        .map_err(|err| format!("worker core settings write failed: {err:?}"))
    }
}

fn nav_db_open_controllers() -> &'static Mutex<HashMap<u32, app_core::NavDbOpenController>> {
    NAV_DB_OPEN_CONTROLLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_nav_db_open_controllers() -> MutexGuard<'static, HashMap<u32, app_core::NavDbOpenController>>
{
    nav_db_open_controllers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn nav_db_open_controller_create(
    candidates_json: &str,
    now_epoch_ms: i64,
) -> Result<u32, JsValue> {
    let candidates: Vec<app_core::NavDbArtifactCandidate> =
        serde_json::from_str(candidates_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let handle = NEXT_NAV_DB_OPEN_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_nav_db_open_controllers().insert(
        handle,
        app_core::NavDbOpenController::new_at_epoch_ms(candidates, now_epoch_ms),
    );
    Ok(handle)
}

#[wasm_bindgen]
pub fn nav_db_open_controller_step(handle: u32) -> Result<String, JsValue> {
    let mut controllers = lock_nav_db_open_controllers();
    let controller = controllers.get_mut(&handle).ok_or_else(|| {
        JsValue::from_str(&format!("invalid nav db open controller handle: {handle}"))
    })?;
    serde_json::to_string(&controller.step().map_err(|err| JsValue::from_str(&err))?)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn nav_db_open_controller_ingest_resource(
    handle: u32,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), JsValue> {
    let mut controllers = lock_nav_db_open_controllers();
    let controller = controllers.get_mut(&handle).ok_or_else(|| {
        JsValue::from_str(&format!("invalid nav db open controller handle: {handle}"))
    })?;
    controller
        .ingest_resource(resource_id, resource_bytes)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn nav_db_open_controller_finish(handle: u32) -> Result<String, JsValue> {
    #[derive(Serialize)]
    struct FinishResult {
        nav_kv_handle: u32,
        open_result: app_core::NavDbOpenResult,
    }

    let mut controllers = lock_nav_db_open_controllers();
    let mut controller = controllers.remove(&handle).ok_or_else(|| {
        JsValue::from_str(&format!("invalid nav db open controller handle: {handle}"))
    })?;
    let outcome = controller.step().map_err(|err| JsValue::from_str(&err))?;
    let app_core::HadOperationOutcome::Complete { result, .. } = outcome else {
        return Err(JsValue::from_str("nav db open controller is not complete"));
    };
    let open_result: app_core::NavDbOpenResult =
        serde_json::from_value(result).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let store = controller
        .selected_store()
        .ok_or_else(|| JsValue::from_str("nav db open controller has no selected store"))?
        .clone();
    let nav_kv_handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_nav_kv_stores().insert(
        nav_kv_handle,
        StoredNavKvStore {
            store,
            open_result: Some(open_result.clone()),
        },
    );
    serde_json::to_string(&FinishResult {
        nav_kv_handle,
        open_result,
    })
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn nav_db_open_controller_destroy(handle: u32) {
    let _ = lock_nav_db_open_controllers().remove(&handle);
}

#[wasm_bindgen]
pub fn nav_kv_prefetch_pages(handle: u32) -> Result<String, JsValue> {
    let stores = lock_nav_kv_stores();
    let store = stores
        .get(&handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {handle}")))?;
    serde_json::to_string(&store.store.missing_prefetch_pages())
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn resolve_nav_db_artifact_candidates_in_session(handle: u32) -> Result<String, JsValue> {
    let outcome = app_core::resolve_nav_db_artifact_candidates_in_session(handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    app_core::serialize_publication_outcome(outcome).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn resolve_metar_manifest_in_session(handle: u32) -> Result<String, JsValue> {
    let outcome = app_core::resolve_metar_manifest_in_session(handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    app_core::serialize_publication_outcome(outcome).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn resolve_chart_asset_resource_in_session(
    handle: u32,
    chart_id: &str,
    asset_kind: &str,
) -> Result<String, JsValue> {
    let outcome = app_core::resolve_chart_asset_resource_in_session(handle, chart_id, asset_kind)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    app_core::serialize_publication_outcome(outcome).map_err(|err| JsValue::from_str(&err))
}

fn nav_kv_insert_page(handle: u32, page_index: u32, page_bytes: &[u8]) -> Result<(), JsValue> {
    let mut stores = lock_nav_kv_stores();
    let store = stores
        .get_mut(&handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {handle}")))?;
    store.store.insert_page(page_index, page_bytes.to_vec());
    app_core::insert_nav_kv_page_for_attached_sessions(handle, page_index, page_bytes);
    Ok(())
}

#[wasm_bindgen]
pub fn nav_kv_insert_resource(
    handle: u32,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), JsValue> {
    let page_index =
        app_core::nav_kv_page_index_from_resource_id(resource_id).ok_or_else(|| {
            JsValue::from_str(&format!("unsupported nav kv resource id: {resource_id}"))
        })?;
    let decoded_bytes = app_core::decode_nav_db_page_resource_bytes(resource_id, resource_bytes)
        .map_err(|err| JsValue::from_str(&err))?;
    nav_kv_insert_page(handle, page_index, decoded_bytes.as_ref())
}

#[wasm_bindgen]
pub fn nav_kv_destroy(handle: u32) {
    let _ = lock_nav_kv_stores().remove(&handle);
}

#[wasm_bindgen]
pub fn install_rust_debug_logger() {
    app_core::set_core_debug_logger(Some(log_core_debug_to_js));
    app_core::set_core_clock_ms(Some(core_clock_ms_to_js));
}

#[wasm_bindgen]
pub fn create_session_snapshot_refresh_scheduler() -> u32 {
    let handle = NEXT_SESSION_SNAPSHOT_REFRESH_SCHEDULER_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_session_snapshot_refresh_schedulers()
        .insert(handle, app_core::SessionSnapshotRefreshScheduler::default());
    handle
}

#[wasm_bindgen]
pub fn destroy_session_snapshot_refresh_scheduler(handle: u32) {
    let _ = lock_session_snapshot_refresh_schedulers().remove(&handle);
}

#[wasm_bindgen]
pub fn create_ui_session_work_scheduler() -> u32 {
    let handle = NEXT_UI_SESSION_WORK_SCHEDULER_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_ui_session_work_schedulers().insert(handle, app_core::UiSessionWorkScheduler::default());
    handle
}

#[wasm_bindgen]
pub fn destroy_ui_session_work_scheduler(handle: u32) {
    let _ = lock_ui_session_work_schedulers().remove(&handle);
}

#[wasm_bindgen]
pub fn ui_session_work_scheduler_request(
    handle: u32,
    request_json: &str,
) -> Result<String, JsValue> {
    let request: app_core::UiSessionWorkRequest =
        serde_json::from_str(request_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let mut schedulers = lock_ui_session_work_schedulers();
    let scheduler = schedulers.get_mut(&handle).ok_or_else(|| {
        JsValue::from_str(&format!(
            "invalid ui session work scheduler handle: {handle}"
        ))
    })?;
    serde_json::to_string(&scheduler.request(request))
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn ui_session_work_scheduler_complete(handle: u32, request_id: u32) -> Result<String, JsValue> {
    let mut schedulers = lock_ui_session_work_schedulers();
    let decision = match schedulers.get_mut(&handle) {
        Some(scheduler) => scheduler.complete(u64::from(request_id)),
        None => app_core::UiSessionWorkCompletionDecision {
            result_action: app_core::UiSessionWorkResultAction::Drop {
                reason: "scheduler_destroyed".to_string(),
            },
            next: None,
        },
    };
    serde_json::to_string(&decision).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn session_snapshot_refresh_scheduler_request(
    handle: u32,
    priority_json: &str,
    reason: &str,
) -> Result<String, JsValue> {
    let priority: app_core::SessionSnapshotRefreshPriority =
        serde_json::from_str(priority_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    session_snapshot_refresh_scheduler_decision(handle, |scheduler| {
        scheduler.request(now_ms().max(0.0).round() as u64, priority, reason)
    })
}

#[wasm_bindgen]
pub fn session_snapshot_refresh_scheduler_viewport_gesture_active_changed(
    handle: u32,
    active: bool,
) -> Result<String, JsValue> {
    session_snapshot_refresh_scheduler_decision(handle, |scheduler| {
        scheduler.viewport_gesture_active_changed(now_ms().max(0.0).round() as u64, active)
    })
}

#[wasm_bindgen]
pub fn session_snapshot_refresh_scheduler_viewport_activity(
    handle: u32,
) -> Result<String, JsValue> {
    session_snapshot_refresh_scheduler_decision(handle, |scheduler| {
        scheduler.viewport_activity(now_ms().max(0.0).round() as u64)
    })
}

#[wasm_bindgen]
pub fn session_snapshot_refresh_scheduler_refresh_completed(
    handle: u32,
) -> Result<String, JsValue> {
    session_snapshot_refresh_scheduler_decision(handle, |scheduler| {
        scheduler.refresh_completed(now_ms().max(0.0).round() as u64)
    })
}

#[wasm_bindgen]
pub fn session_snapshot_refresh_scheduler_poll(handle: u32) -> Result<String, JsValue> {
    session_snapshot_refresh_scheduler_decision(handle, |scheduler| {
        scheduler.poll(now_ms().max(0.0).round() as u64)
    })
}

fn session_snapshot_refresh_scheduler_decision(
    handle: u32,
    work: impl FnOnce(
        &mut app_core::SessionSnapshotRefreshScheduler,
    ) -> app_core::SessionSnapshotRefreshDecision,
) -> Result<String, JsValue> {
    let mut schedulers = lock_session_snapshot_refresh_schedulers();
    let scheduler = schedulers.get_mut(&handle).ok_or_else(|| {
        JsValue::from_str(&format!(
            "invalid session snapshot refresh scheduler handle: {handle}"
        ))
    })?;
    serde_json::to_string(&work(scheduler)).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn core_clock_ms_to_js() -> f64 {
    performance_now()
}

#[cfg(not(target_arch = "wasm32"))]
fn core_clock_ms_to_js() -> f64 {
    0.0
}

fn log_core_debug_to_js(tag: &str, data: &serde_json::Value) {
    if let Ok(data_json) = serde_json::to_string(data) {
        emit_rust_debug_log(tag, &data_json);
    }
}

#[cfg(target_arch = "wasm32")]
fn emit_rust_debug_log(tag: &str, data_json: &str) {
    let global = js_sys::global();
    let Ok(callback) = Reflect::get(&global, &JsValue::from_str("__aerobagRustDebugLog")) else {
        return;
    };
    if !callback.is_function() {
        return;
    }
    let function = Function::from(callback);
    let _ = function.call2(
        &JsValue::NULL,
        &JsValue::from_str(tag),
        &JsValue::from_str(data_json),
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_rust_debug_log(_tag: &str, _data_json: &str) {}

#[wasm_bindgen]
pub fn core_had_operation(nav_kv_handle: u32, operation_json: &str) -> Result<String, JsValue> {
    let operation: app_core::HadOperation =
        serde_json::from_str(operation_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let stores = lock_nav_kv_stores();
    let store = stores
        .get(&nav_kv_handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {nav_kv_handle}")))?;
    let outcome = app_core::run_had_operation(&store.store, operation)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn sync_live_feeds_in_session(session_handle: u32) -> Result<String, JsValue> {
    let outcome = app_core::sync_live_feeds_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn configure_live_feed_source_in_session(
    session_handle: u32,
    source_root_url: &str,
) -> Result<(), JsValue> {
    app_core::configure_live_feed_source_in_session(session_handle, source_root_url)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn configure_data_sources_in_session(
    session_handle: u32,
    cycle_data_base_url: &str,
    live_feeds_base_url: &str,
    debug_log_sink_url: Option<String>,
) -> Result<String, JsValue> {
    let outcome = app_core::configure_data_sources_in_session(
        session_handle,
        cycle_data_base_url,
        live_feeds_base_url,
        debug_log_sink_url.as_deref(),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn live_feed_events_url(source_root_url: &str) -> Result<String, JsValue> {
    app_core::live_feed_events_url(source_root_url)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn live_feed_status_url(source_root_url: &str) -> Result<String, JsValue> {
    app_core::live_feed_status_url(source_root_url)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn live_feed_runtime_decision_in_session(
    session_handle: u32,
    input_json: &str,
) -> Result<String, JsValue> {
    let input: app_core::LiveFeedRuntimeInput =
        serde_json::from_str(input_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(
        &app_core::live_feed_runtime_decision_in_session(session_handle, input)
            .map_err(|err| JsValue::from_str(&err.to_string()))?,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn refresh_live_feed_current_in_session(session_handle: u32) -> Result<String, JsValue> {
    let outcome = app_core::refresh_live_feed_current_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn drain_session_resource_effects(session_handle: u32) -> Result<String, JsValue> {
    let effects = app_core::drain_session_resource_effects(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&effects).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn ingest_live_feed_sse_event_in_session(
    session_handle: u32,
    event_json: &str,
) -> Result<String, JsValue> {
    let event: app_core::LiveFeedSseEvent =
        serde_json::from_str(event_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::ingest_live_feed_sse_event_in_session_at_epoch_ms(
        session_handle,
        &event,
        now_ms() as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn ingest_live_feed_sse_events_in_session(
    session_handle: u32,
    events_json: &str,
) -> Result<String, JsValue> {
    let events: Vec<app_core::LiveFeedSseEvent> =
        serde_json::from_str(events_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::ingest_live_feed_sse_events_in_session_at_epoch_ms(
        session_handle,
        &events,
        now_ms() as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn report_live_feed_connection_event_in_session(
    session_handle: u32,
    event_json: &str,
) -> Result<String, JsValue> {
    let event: app_core::LiveFeedConnectionEvent =
        serde_json::from_str(event_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let snapshot = app_core::report_live_feed_connection_event_in_session(
        session_handle,
        event,
        now_ms() as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_map_selection_action_in_session(
    session_handle: u32,
    action_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let outcome = app_core::perform_map_selection_action_in_session(
        session_handle,
        action_json.to_string(),
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_flight_plan_command_in_session(
    session_handle: u32,
    command_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let command: app_core::FlightPlanSessionCommand =
        serde_json::from_str(command_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome =
        app_core::perform_flight_plan_command_in_session(session_handle, command, now_epoch_ms)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_time_display_action_in_session(
    session_handle: u32,
    action_id: &str,
) -> Result<String, JsValue> {
    let outcome =
        app_core::perform_time_display_action_in_session(session_handle, action_id.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_flight_plan_column_action_in_session(
    session_handle: u32,
    action_id: &str,
) -> Result<String, JsValue> {
    let outcome = app_core::perform_flight_plan_column_action_in_session(
        session_handle,
        action_id.to_string(),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn query_flight_plan_in_session(
    session_handle: u32,
    query_json: &str,
) -> Result<String, JsValue> {
    let query: app_core::FlightPlanSessionQuery =
        serde_json::from_str(query_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::query_flight_plan_in_session(session_handle, query)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_status_action_in_session(
    session_handle: u32,
    action_id: &str,
) -> Result<String, JsValue> {
    let snapshot =
        app_core::session::perform_status_action_in_session(session_handle, action_id.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_ownship_text_action_in_session(
    session_handle: u32,
    action_id: &str,
    value: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let outcome = app_core::perform_ownship_text_action_in_session(
        session_handle,
        action_id,
        value,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_settings_action_in_session(
    session_handle: u32,
    action_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let action: app_core::UiSettingsAction =
        serde_json::from_str(action_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let snapshot =
        app_core::perform_settings_action_in_session(session_handle, action, now_epoch_ms)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn complete_cloud_authorization_in_session(
    session_handle: u32,
    request_id: u64,
    response_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let response: app_core::CloudAuthorizationResponse =
        serde_json::from_str(response_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::complete_cloud_authorization_in_session(
        session_handle,
        request_id,
        response,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn take_cloud_authorization_request_in_session(
    session_handle: u32,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let request =
        app_core::take_cloud_authorization_request_in_session(session_handle, now_epoch_ms)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&request).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_cloud_ui_action_in_session(
    session_handle: u32,
    action_id_json: &str,
    fields_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let action_id: app_core::CloudUiActionId =
        serde_json::from_str(action_id_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let fields: Vec<app_core::CloudUiFieldValue> =
        serde_json::from_str(fields_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::perform_cloud_ui_action_in_session(
        session_handle,
        action_id,
        fields,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn record_offline_package_preferences_in_session(
    session_handle: u32,
    preferences_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    app_core::record_offline_package_preferences_in_session(
        session_handle,
        preferences_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    get_session_snapshot_at_epoch_ms_paged_json(session_handle, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn take_cloud_provider_request_in_session(
    session_handle: u32,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let request = app_core::take_cloud_provider_request_in_session(session_handle, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&request).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn complete_cloud_provider_request_in_session(
    session_handle: u32,
    request_id: u64,
    response_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let response: app_core::CloudHttpResponse =
        serde_json::from_str(response_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::complete_cloud_provider_request_in_session(
        session_handle,
        request_id,
        response,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn cloud_event_stream_plan_in_session(session_handle: u32) -> Result<String, JsValue> {
    let plan = app_core::cloud_event_stream_plan_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&plan).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn report_cloud_event_stream_event_in_session(
    session_handle: u32,
    event_json: &str,
    now_epoch_ms: i64,
) -> Result<String, JsValue> {
    let event: app_core::CloudEventStreamEvent =
        serde_json::from_str(event_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome =
        app_core::report_cloud_event_stream_event_in_session(session_handle, event, now_epoch_ms)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn attach_nav_kv_store_to_session(
    nav_kv_handle: u32,
    session_handle: u32,
) -> Result<(), JsValue> {
    let stores = lock_nav_kv_stores();
    let store = stores
        .get(&nav_kv_handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {nav_kv_handle}")))?;
    app_core::attach_nav_kv_store_to_session_with_open_result(
        session_handle,
        nav_kv_handle,
        &store.store,
        store.open_result.as_ref(),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn advance_nav_kv_store_in_session(
    nav_kv_handle: u32,
    session_handle: u32,
    installed_package_ids_json: &str,
) -> Result<String, JsValue> {
    let installed_package_ids: Vec<String> = serde_json::from_str(installed_package_ids_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let stores = lock_nav_kv_stores();
    let stored = stores
        .get(&nav_kv_handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {nav_kv_handle}")))?;
    let open_result = stored
        .open_result
        .as_ref()
        .ok_or_else(|| JsValue::from_str("candidate nav kv store has no artifact identity"))?;
    let outcome = app_core::advance_nav_kv_store_in_session_with_open_result(
        session_handle,
        nav_kv_handle,
        &stored.store,
        open_result,
        installed_package_ids,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(debug_assertions)]
#[wasm_bindgen]
pub fn startup_smoke_test() -> Result<(), JsValue> {
    let init = app_core::create_ui_session_at_epoch_ms(
        app_core::FlightPlan::default(),
        &[],
        None,
        None,
        0,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let default_aircraft_json = include_bytes!(
        "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/cessna-172-generic.json"
    );
    let default_aircraft: app_core::AircraftDefinition =
        serde_json::from_slice(default_aircraft_json)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let default_aircraft_key = format!(
        "aircraft/definition/{}",
        default_aircraft
            .content_hash()
            .map_err(|err| JsValue::from_str(&err))?
    );
    let store = app_core::nav_kv_store_for_smoke_test(
        &[
            (
                "package/by-id/NAV_DB_SMOKE",
                br#"{
                "id": "NAV_DB_SMOKE",
                "family_id": "nav-db",
                "expiration_date": "2020-01-01"
            }"#,
            ),
            (default_aircraft_key.as_str(), default_aircraft_json),
        ],
        4_096,
    );
    let nav_kv_handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_nav_kv_stores().insert(
        nav_kv_handle,
        StoredNavKvStore {
            store,
            open_result: None,
        },
    );
    attach_nav_kv_store_to_session(nav_kv_handle, init.handle)?;
    sync_guidance_geometry_in_session(init.handle)?;
    get_session_snapshot_paged(init.handle)?;
    lock_nav_kv_stores().remove(&nav_kv_handle);
    app_core::destroy_session(init.handle);
    Ok(())
}

#[wasm_bindgen]
pub fn situation_ring_candidates_json() -> Result<String, JsValue> {
    serde_json::to_string(&app_core::situation_ring_candidates())
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn create_ui_session(
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    create_ui_session_json(
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn create_ui_session_profiled(
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    create_ui_session_profiled_json(
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn session_diagnostics(handle: u32) -> Result<String, JsValue> {
    let diagnostics =
        app_core::session_diagnostics(handle).map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&diagnostics).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn set_resource_policy_in_session(handle: u32, policy_json: &str) -> Result<String, JsValue> {
    set_resource_policy_in_session_json(handle, policy_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn configure_platform_capabilities_in_session(
    handle: u32,
    capabilities_json: &str,
) -> Result<String, JsValue> {
    let capabilities: app_core::PlatformCapabilities = serde_json::from_str(capabilities_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let snapshot = app_core::configure_platform_capabilities_in_session(
        handle,
        capabilities,
        web_core_settings_storage(),
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn accept_disclaimer_in_session(handle: u32, agreement_id: &str) -> Result<String, JsValue> {
    let snapshot = app_core::accept_disclaimer_in_session(handle, agreement_id)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn select_airport_in_session(handle: u32, airport_id_json: &str) -> Result<String, JsValue> {
    select_airport_in_session_json(handle, airport_id_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn open_chart_airport_in_session(
    handle: u32,
    airport_id_json: &str,
    chart_id_json: &str,
) -> Result<String, JsValue> {
    let airport_id: String =
        serde_json::from_str(airport_id_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let chart_id: Option<String> =
        serde_json::from_str(chart_id_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::open_chart_airport_in_session(handle, &airport_id, chart_id.as_deref())
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn register_ownship_source_in_session_paged(
    handle: u32,
    registration_json: &str,
) -> Result<String, JsValue> {
    register_ownship_source_in_session_paged_json(handle, registration_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn update_ownship_source_status_in_session_paged(
    handle: u32,
    update_json: &str,
) -> Result<String, JsValue> {
    update_ownship_source_status_in_session_paged_json(handle, update_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn push_situation_sample_in_session_paged(
    handle: u32,
    sample_json: &str,
) -> Result<String, JsValue> {
    push_situation_sample_in_session_paged_json(handle, sample_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_ownship_source_in_session_paged(
    handle: u32,
    selection_json: &str,
) -> Result<String, JsValue> {
    select_ownship_source_in_session_paged_json(handle, selection_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn apply_situation_control_input_in_session(
    handle: u32,
    input_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    apply_situation_control_input_in_session_json(handle, input_json, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_situation_in_session_paged(
    handle: u32,
    situation_json: &str,
) -> Result<String, JsValue> {
    set_situation_in_session_paged_json(handle, situation_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn tick_bad_autopilot_in_session_paged(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    tick_bad_autopilot_in_session_paged_json(handle, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
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
pub fn sync_guidance_geometry_in_session(handle: u32) -> Result<String, JsValue> {
    sync_guidance_geometry_in_session_json(handle).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn project_flight_plan_route_in_session(handle: u32) -> Result<String, JsValue> {
    project_flight_plan_route_in_session_json(handle).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn load_playback_trace_in_session_paged(
    handle: u32,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, JsValue> {
    load_playback_trace_in_session_paged_json(handle, source_path_json, trace_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn play_playback_in_session_paged(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    play_playback_in_session_paged_json(handle, now_epoch_ms).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn pause_playback_in_session_paged(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    pause_playback_in_session_paged_json(handle, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn seek_playback_in_session_paged(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    seek_playback_in_session_paged_json(handle, cursor_seconds, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_playback_rate_in_session_paged(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    set_playback_rate_in_session_paged_json(handle, rate, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn tick_playback_in_session_paged(handle: u32, now_epoch_ms: f64) -> Result<String, JsValue> {
    tick_playback_in_session_paged_json(handle, now_epoch_ms).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_chart_in_session(handle: u32, chart_id_json: &str) -> Result<String, JsValue> {
    select_chart_in_session_json(handle, chart_id_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_chart_reference_in_session(
    handle: u32,
    family_id_json: &str,
    suggested_chart_ids_json: &str,
) -> Result<String, JsValue> {
    select_chart_reference_in_session_json(handle, family_id_json, suggested_chart_ids_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_map_layer_visibility_in_session_paged(
    handle: u32,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, JsValue> {
    set_map_layer_visibility_in_session_paged_json(handle, layer_id_json, visible)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_map_layer_enabled_in_session_paged(
    handle: u32,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, JsValue> {
    set_map_layer_enabled_in_session_paged_json(handle, layer_id_json, enabled)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn load_raster_map_catalog_in_session(handle: u32) -> Result<String, JsValue> {
    load_raster_map_catalog_in_session_json(handle).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_map_family_in_session(handle: u32, family_id_json: &str) -> Result<String, JsValue> {
    select_map_family_in_session_json(handle, family_id_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn select_raster_map_in_session(
    handle: u32,
    selected_map_id_json: &str,
) -> Result<String, JsValue> {
    select_raster_map_in_session_json(handle, selected_map_id_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_session_snapshot_paged(handle: u32) -> Result<String, JsValue> {
    get_session_snapshot_paged_json(handle).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_session_snapshot_at_epoch_ms_paged(
    handle: u32,
    epoch_ms: i64,
) -> Result<String, JsValue> {
    get_session_snapshot_at_epoch_ms_paged_json(handle, epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn maintain_nav_db_in_session_at_epoch_ms(
    handle: u32,
    epoch_ms: i64,
) -> Result<String, JsValue> {
    let outcome = app_core::maintain_nav_db_in_session_at_epoch_ms(handle, epoch_ms)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
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
pub fn ingest_resource_in_session(
    handle: u32,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), JsValue> {
    app_core::ingest_resource_in_session_at_epoch_ms(
        handle,
        resource_id,
        resource_bytes,
        now_ms() as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn ingest_prepared_live_feed_resource_in_session(
    handle: u32,
    resource_id: &str,
    prepared_resource_bytes: &[u8],
) -> Result<(), JsValue> {
    app_core::ingest_prepared_live_feed_resource_in_session(
        handle,
        resource_id,
        prepared_resource_bytes,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn prepare_live_feed_resource(
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let request = prepared_live_feed_request(resource_id)?;
    let (next_state, prepared) = match &request {
        PreparedLiveFeedRequest::State { product, .. } if product == "notams" => {
            app_core::prepare_notam_live_feed_state_resource_with_work(
                resource_id,
                resource_bytes,
                &mut lock_notam_projection_preparer(),
                &mut app_core::BackgroundNotamWork::default(),
                &mut app_core::NotamApplyWork::default(),
            )
            .map(|prepared| (serde_json::Value::Null, prepared))
        }
        PreparedLiveFeedRequest::State { .. } => {
            app_core::prepare_live_feed_state_resource(resource_id, resource_bytes)
        }
        PreparedLiveFeedRequest::Delta { product, .. } if product == "notams" => {
            app_core::prepare_notam_live_feed_delta_resource_with_work(
                resource_id,
                resource_bytes,
                &mut lock_notam_projection_preparer(),
                &mut app_core::BackgroundNotamWork::default(),
                &mut app_core::NotamApplyWork::default(),
            )
            .map(|prepared| (serde_json::Value::Null, prepared))
        }
        PreparedLiveFeedRequest::Delta {
            product,
            from_version,
            ..
        } => {
            let states = lock_live_feed_prep_states();
            let state = states
                .get(&(product.clone(), from_version.clone()))
                .ok_or_else(|| {
                    JsValue::from_str(&format!(
                        "live-feed preparer has no {product} {from_version} state for delta"
                    ))
                })?;
            app_core::prepare_live_feed_delta_resource(resource_id, state, resource_bytes)
        }
    }
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let mut states = lock_live_feed_prep_states();
    if let PreparedLiveFeedRequest::Delta {
        product,
        from_version,
        ..
    } = &request
    {
        states.remove(&(product.clone(), from_version.clone()));
    }
    if request.product() != "notams" {
        states.insert(request.target_key(), next_state);
    }
    Ok(prepared)
}

#[wasm_bindgen]
pub fn should_prepare_live_feed_resource(resource_id: &str) -> bool {
    app_core::should_prepare_live_feed_resource(resource_id)
}

#[wasm_bindgen]
pub fn reset_live_feed_preparer() {
    lock_live_feed_prep_states().clear();
    lock_notam_projection_preparer().reset();
}

enum PreparedLiveFeedRequest {
    State {
        product: String,
        version: String,
    },
    Delta {
        product: String,
        from_version: String,
        to_version: String,
    },
}

impl PreparedLiveFeedRequest {
    fn product(&self) -> &str {
        match self {
            Self::State { product, .. } | Self::Delta { product, .. } => product,
        }
    }

    fn target_key(&self) -> (String, String) {
        match self {
            Self::State { product, version } => (product.clone(), version.clone()),
            Self::Delta {
                product,
                to_version,
                ..
            } => (product.clone(), to_version.clone()),
        }
    }
}

fn prepared_live_feed_request(resource_id: &str) -> Result<PreparedLiveFeedRequest, JsValue> {
    let parts = resource_id.split('/').collect::<Vec<_>>();
    if parts.first() != Some(&"live_feeds") {
        return Err(JsValue::from_str(&format!(
            "unsupported live-feed prep resource: {resource_id}"
        )));
    }
    let product = parts.get(2).copied().unwrap_or_default();
    if !app_core::supports_prepared_live_feed(product) {
        return Err(JsValue::from_str(&format!(
            "unsupported prepared live-feed product: {product}"
        )));
    }
    match parts.as_slice() {
        ["live_feeds", "state", _, version] => Ok(PreparedLiveFeedRequest::State {
            product: product.to_string(),
            version: (*version).to_string(),
        }),
        ["live_feeds", "delta", _, from_version, to_version] => {
            Ok(PreparedLiveFeedRequest::Delta {
                product: product.to_string(),
                from_version: (*from_version).to_string(),
                to_version: (*to_version).to_string(),
            })
        }
        _ => Err(JsValue::from_str(&format!(
            "unsupported live-feed prep resource: {resource_id}"
        ))),
    }
}

#[wasm_bindgen]
pub fn report_session_resource_failure_in_session(
    handle: u32,
    resource_id: &str,
    message: &str,
) -> Result<String, JsValue> {
    let snapshot =
        app_core::report_session_resource_failure_in_session(handle, resource_id, message)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn report_session_resource_failure_in_session_at_epoch_ms(
    handle: u32,
    resource_id: &str,
    message: &str,
    epoch_ms: f64,
) -> Result<String, JsValue> {
    let snapshot = app_core::report_session_resource_failure_in_session_at_epoch_ms(
        handle,
        resource_id,
        message,
        epoch_ms as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn get_map_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_map_overlay_in_session_json(handle, viewport_json, width_px, height_px, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_map_selection_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    click_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_map_selection_in_session_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        click_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_map_selection_distance_in_session(
    handle: u32,
    target_json: &str,
) -> Result<String, JsValue> {
    get_map_selection_distance_in_session_json(handle, target_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_map_selection_for_nav_ref_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    nav_ref_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_map_selection_for_nav_ref_in_session_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        nav_ref_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_terrain_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_terrain_overlay_in_session_json(handle, viewport_json, width_px, height_px, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_scheduled_terrain_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    decoded_cache_keys_json: &str,
    in_flight_cache_keys_json: &str,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_scheduled_terrain_overlay_in_session_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        decoded_cache_keys_json,
        in_flight_cache_keys_json,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_nexrad_overlay_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_nexrad_overlay_in_session_json(handle, viewport_json, width_px, height_px, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_raster_tile_plan_in_session(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_raster_tile_plan_in_session_json(handle, viewport_json, width_px, height_px, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn get_raster_tile_plan_in_session_with_display_scale(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    get_raster_tile_plan_in_session_with_display_scale_json(
        handle,
        viewport_json,
        width_px,
        height_px,
        device_pixel_ratio,
        now_epoch_ms,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn restore_chart_page_state_in_session(
    handle: u32,
    recent_airport_ids_json: &str,
    plate_target_airport_id_json: &str,
    selected_airport_id_json: &str,
    selected_reference_family_id_json: &str,
    selected_chart_id_json: &str,
    suggested_chart_ids_json: &str,
) -> Result<String, JsValue> {
    restore_chart_page_state_in_session_json(
        handle,
        recent_airport_ids_json,
        plate_target_airport_id_json,
        selected_airport_id_json,
        selected_reference_family_id_json,
        selected_chart_id_json,
        suggested_chart_ids_json,
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
pub fn render_terrain_overlay_tile_by_key_in_session(
    handle: u32,
    terrain_tile_key: &str,
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, JsValue> {
    app_core::render_terrain_overlay_tile_by_key_in_session(
        handle,
        terrain_tile_key,
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

fn create_ui_session_json(
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
    now_epoch_ms: f64,
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
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    let serialized = serde_json::to_string(&result).map_err(|err| err.to_string())?;
    app_core::record_session_serialized_payload_bytes(result.handle, serialized.len());
    Ok(serialized)
}

fn create_ui_session_profiled_json(
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let mut profiler = Profiler::new();
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    profiler.mark("parse_recent_airports_json");
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    profiler.mark("parse_selected_ids_json");
    let result = app_core::create_ui_session_profiled_at_epoch_ms(
        app_core::FlightPlan::empty(),
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
        now_epoch_ms as i64,
        &mut |label| profiler.mark(label),
    )
    .map_err(|err| err.to_string())?;
    profiler.mark("app_core_create_ui_session_total");
    let handle = result.handle;
    let envelope = ProfiledResult {
        result,
        timings: profiler.timings,
    };
    let serialized = serde_json::to_string(&envelope).map_err(|err| err.to_string())?;
    app_core::record_session_serialized_payload_bytes(handle, serialized.len());
    Ok(serialized)
}

fn set_resource_policy_in_session_json(handle: u32, policy_json: &str) -> Result<String, String> {
    let policy: String = serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let policy = resource_policy_from_wire(&policy)?;
    let snapshot =
        app_core::set_resource_policy_in_session(handle, policy).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn resource_policy_from_wire(policy: &str) -> Result<app_core::CoreResourcePolicy, String> {
    match policy {
        "public_unpacked" => Ok(app_core::CoreResourcePolicy::PublicUnpacked),
        "installed_package" => Ok(app_core::CoreResourcePolicy::InstalledPackage),
        other => Err(format!("unknown resource policy: {other}")),
    }
}

fn select_airport_in_session_json(handle: u32, airport_id_json: &str) -> Result<String, String> {
    let airport_id: String =
        serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_airport_in_session(handle, &airport_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn register_ownship_source_in_session_paged_json(
    handle: u32,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let outcome = app_core::register_ownship_source_in_session(handle, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn update_ownship_source_status_in_session_paged_json(
    handle: u32,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let outcome = app_core::update_ownship_source_status_in_session(handle, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn push_situation_sample_in_session_paged_json(
    handle: u32,
    sample_json: &str,
) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let outcome = app_core::push_situation_sample_in_session(handle, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn select_ownship_source_in_session_paged_json(
    handle: u32,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_ownship_source_in_session(handle, selection)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn apply_situation_control_input_in_session_json(
    handle: u32,
    input_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let input: app_core::SituationControlInput =
        serde_json::from_str(input_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::apply_situation_control_input_in_session(handle, input, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_situation_in_session_paged_json(
    handle: u32,
    situation_json: &str,
) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let outcome =
        app_core::set_situation_in_session(handle, situation).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn tick_bad_autopilot_in_session_paged_json(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::tick_bad_autopilot_in_session(handle, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

fn sync_guidance_geometry_in_session_json(handle: u32) -> Result<String, String> {
    let outcome =
        app_core::sync_guidance_geometry_in_session(handle).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn project_flight_plan_route_in_session_json(handle: u32) -> Result<String, String> {
    let outcome =
        app_core::project_flight_plan_route_in_session(handle).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn load_playback_trace_in_session_paged_json(
    handle: u32,
    source_path_json: &str,
    trace_json: &str,
) -> Result<String, String> {
    let source_path: String =
        serde_json::from_str(source_path_json).map_err(|err| err.to_string())?;
    let outcome = app_core::load_playback_trace_in_session(handle, &source_path, trace_json)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn play_playback_in_session_paged_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let outcome =
        app_core::play_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn pause_playback_in_session_paged_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let outcome =
        app_core::pause_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn seek_playback_in_session_paged_json(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::seek_playback_in_session(handle, cursor_seconds, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn set_playback_rate_in_session_paged_json(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::set_playback_rate_in_session(handle, rate, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn tick_playback_in_session_paged_json(handle: u32, now_epoch_ms: f64) -> Result<String, String> {
    let outcome =
        app_core::tick_playback_in_session(handle, now_epoch_ms).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn select_chart_in_session_json(handle: u32, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_chart_in_session(handle, &chart_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn select_chart_reference_in_session_json(
    handle: u32,
    family_id_json: &str,
    suggested_chart_ids_json: &str,
) -> Result<String, String> {
    let family_id: String = serde_json::from_str(family_id_json).map_err(|err| err.to_string())?;
    let suggested_chart_ids: Vec<String> =
        serde_json::from_str(suggested_chart_ids_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_chart_reference_in_session(handle, &family_id, &suggested_chart_ids)
            .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_map_layer_visibility_in_session_paged_json(
    handle: u32,
    layer_id_json: &str,
    visible: bool,
) -> Result<String, String> {
    let layer_id: app_core::MapLayerId =
        serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::set_map_layer_visibility_in_session(handle, layer_id, visible)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn set_map_layer_enabled_in_session_paged_json(
    handle: u32,
    layer_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let layer_id: app_core::MapLayerId =
        serde_json::from_str(layer_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::set_map_layer_enabled_in_session(handle, layer_id, enabled)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn load_raster_map_catalog_in_session_json(handle: u32) -> Result<String, String> {
    let outcome =
        app_core::load_raster_map_catalog_in_session(handle).map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn select_map_family_in_session_json(handle: u32, family_id_json: &str) -> Result<String, String> {
    let family_id: String = serde_json::from_str(family_id_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_map_family_in_session(handle, &family_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn select_raster_map_in_session_json(
    handle: u32,
    selected_map_id_json: &str,
) -> Result<String, String> {
    let selected_map_id: String =
        serde_json::from_str(selected_map_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::select_raster_map_in_session(handle, &selected_map_id)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn get_session_snapshot_paged_json(handle: u32) -> Result<String, String> {
    get_session_snapshot_at_epoch_ms_paged_json(handle, 0)
}

fn get_session_snapshot_at_epoch_ms_paged_json(
    handle: u32,
    epoch_ms: i64,
) -> Result<String, String> {
    let total_started_at = now_ms();
    let core_started_at = now_ms();
    let outcome = app_core::get_session_snapshot_at_epoch_ms(handle, epoch_ms)
        .map_err(|err| err.to_string())?;
    let core_ms = now_ms() - core_started_at;
    let serialize_started_at = now_ms();
    let serialized = serde_json::to_string(&outcome).map_err(|err| err.to_string())?;
    let serialize_ms = now_ms() - serialize_started_at;
    app_core::record_session_serialized_payload_bytes(handle, serialized.len());
    app_core::core_debug_log(
        "session.snapshot.wasm",
        &serde_json::json!({
            "total_ms": (now_ms() - total_started_at).round() as u64,
            "core_ms": core_ms.round() as u64,
            "serialize_ms": serialize_ms.round() as u64,
            "json_bytes": serialized.len(),
        }),
    );
    Ok(serialized)
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

fn get_map_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let total_started_at = now_ms();
    let parse_started_at = now_ms();
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let parse_ms = now_ms() - parse_started_at;
    let core_started_at = now_ms();
    let overlay = app_core::get_map_overlay_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    let core_ms = now_ms() - core_started_at;
    let serialize_started_at = now_ms();
    let serialized = serde_json::to_string(&overlay).map_err(|err| err.to_string())?;
    let serialize_ms = now_ms() - serialize_started_at;
    app_core::core_debug_log(
        "map.overlay.wasm",
        &serde_json::json!({
            "total_ms": (now_ms() - total_started_at).round() as u64,
            "parse_ms": parse_ms.round() as u64,
            "core_ms": core_ms.round() as u64,
            "serialize_ms": serialize_ms.round() as u64,
            "json_bytes": serialized.len(),
        }),
    );
    Ok(serialized)
}

fn get_map_selection_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    click_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let click: app_core::LatLon =
        serde_json::from_str(click_json).map_err(|err| err.to_string())?;
    let selection = app_core::get_map_selection_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        click,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&selection).map_err(|err| err.to_string())
}

fn get_map_selection_distance_in_session_json(
    handle: u32,
    target_json: &str,
) -> Result<String, String> {
    let target: app_core::LatLon =
        serde_json::from_str(target_json).map_err(|err| err.to_string())?;
    let distance = app_core::get_map_selection_distance_in_session(handle, target)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&distance).map_err(|err| err.to_string())
}

fn get_map_selection_for_nav_ref_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    nav_ref_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let nav_ref: app_core::NavRef =
        serde_json::from_str(nav_ref_json).map_err(|err| err.to_string())?;
    let selection =
        app_core::get_map_selection_for_nav_ref_in_session_with_point_display_scale_at_epoch_ms(
            handle,
            viewport,
            width_px,
            height_px,
            nav_ref,
            1.0,
            now_epoch_ms as i64,
        )
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&selection).map_err(|err| err.to_string())
}

fn get_terrain_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_terrain_overlay_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

fn get_scheduled_terrain_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    decoded_cache_keys_json: &str,
    in_flight_cache_keys_json: &str,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let decoded_cache_keys: BTreeSet<String> =
        serde_json::from_str(decoded_cache_keys_json).map_err(|err| err.to_string())?;
    let in_flight_cache_keys: BTreeSet<String> =
        serde_json::from_str(in_flight_cache_keys_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_scheduled_terrain_overlay_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        &decoded_cache_keys,
        &in_flight_cache_keys,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

fn get_raster_tile_plan_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan = app_core::get_raster_tile_plan_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn get_nexrad_overlay_in_session_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let overlay = app_core::get_nexrad_overlay_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&overlay).map_err(|err| err.to_string())
}

fn get_raster_tile_plan_in_session_with_display_scale_json(
    handle: u32,
    viewport_json: &str,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let viewport: app_core::MapViewport =
        serde_json::from_str(viewport_json).map_err(|err| err.to_string())?;
    let plan = app_core::get_raster_tile_plan_in_session_with_display_scale_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        device_pixel_ratio,
        now_epoch_ms as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn restore_chart_page_state_in_session_json(
    handle: u32,
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
        handle,
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

fn destroy_session_json(handle: u32) {
    app_core::destroy_session(handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_session_work_scheduler_bridge_preserves_core_decisions() {
        let handle = create_ui_session_work_scheduler();
        let first = app_core::UiSessionWorkRequest {
            id: 1,
            kind: app_core::UiSessionWorkKind::MapOverlay,
            coalesce_key: Some("map_overlay".to_string()),
            requested_at_ms: 10,
        };
        let second = app_core::UiSessionWorkRequest {
            id: 2,
            kind: app_core::UiSessionWorkKind::TerrainOverlay,
            coalesce_key: Some("terrain_overlay".to_string()),
            requested_at_ms: 20,
        };

        let first_decision: app_core::UiSessionWorkRequestDecision = serde_json::from_str(
            &ui_session_work_scheduler_request(
                handle,
                &serde_json::to_string(&first).expect("serialize first request"),
            )
            .expect("schedule first request"),
        )
        .expect("decode first decision");
        assert!(matches!(
            first_decision,
            app_core::UiSessionWorkRequestDecision::Start { request } if request == first
        ));

        let second_decision: app_core::UiSessionWorkRequestDecision = serde_json::from_str(
            &ui_session_work_scheduler_request(
                handle,
                &serde_json::to_string(&second).expect("serialize second request"),
            )
            .expect("schedule second request"),
        )
        .expect("decode second decision");
        assert_eq!(
            second_decision,
            app_core::UiSessionWorkRequestDecision::Queued {
                replaced_request_id: None,
            }
        );

        let completion: app_core::UiSessionWorkCompletionDecision = serde_json::from_str(
            &ui_session_work_scheduler_complete(handle, 1).expect("complete first request"),
        )
        .expect("decode completion");
        assert_eq!(completion.next, Some(second));
        destroy_ui_session_work_scheduler(handle);
    }

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
}
