use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Reflect};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

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

fn benchmark_now_ms() -> f64 {
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
static NAV_KV_STORES: OnceLock<Mutex<HashMap<u32, app_core::NavKvStore>>> = OnceLock::new();
static NEXT_NAV_DB_OPEN_HANDLE: AtomicU32 = AtomicU32::new(1);
static NAV_DB_OPEN_CONTROLLERS: OnceLock<Mutex<HashMap<u32, app_core::NavDbOpenController>>> =
    OnceLock::new();
static METAR_LIVE_FEED_PREP_STATE: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();
static NEXT_SESSION_SNAPSHOT_REFRESH_SCHEDULER_HANDLE: AtomicU32 = AtomicU32::new(1);
static SESSION_SNAPSHOT_REFRESH_SCHEDULERS: OnceLock<
    Mutex<HashMap<u32, app_core::SessionSnapshotRefreshScheduler>>,
> = OnceLock::new();

fn nav_kv_stores() -> &'static Mutex<HashMap<u32, app_core::NavKvStore>> {
    NAV_KV_STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_nav_kv_stores() -> MutexGuard<'static, HashMap<u32, app_core::NavKvStore>> {
    nav_kv_stores()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_metar_live_feed_prep_state() -> MutexGuard<'static, Option<serde_json::Value>> {
    METAR_LIVE_FEED_PREP_STATE
        .get_or_init(|| Mutex::new(None))
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
pub fn nav_db_open_controller_create(candidates_json: &str) -> Result<u32, JsValue> {
    let candidates: Vec<app_core::NavDbArtifactCandidate> =
        serde_json::from_str(candidates_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let handle = NEXT_NAV_DB_OPEN_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_nav_db_open_controllers().insert(handle, app_core::NavDbOpenController::new(candidates));
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
    lock_nav_kv_stores().insert(nav_kv_handle, store);
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
    serde_json::to_string(&store.missing_prefetch_pages())
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
pub fn resolve_package_member_in_session(
    handle: u32,
    package_id: &str,
    member_path: &str,
) -> Result<String, JsValue> {
    let outcome = app_core::resolve_package_member_in_session(handle, package_id, member_path)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    app_core::serialize_publication_outcome(outcome).map_err(|err| JsValue::from_str(&err))
}

fn nav_kv_insert_page(handle: u32, page_index: u32, page_bytes: &[u8]) -> Result<(), JsValue> {
    let mut stores = lock_nav_kv_stores();
    let store = stores
        .get_mut(&handle)
        .ok_or_else(|| JsValue::from_str(&format!("invalid nav kv handle: {handle}")))?;
    store.insert_page(page_index, page_bytes.to_vec());
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
    nav_kv_insert_page(handle, page_index, resource_bytes)
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

#[derive(Debug, Serialize)]
struct MetarBakeoffReport {
    rounds: u32,
    fixture_json_bytes: usize,
    metar_count: usize,
    pirep_count: usize,
    candidates: Vec<MetarBakeoffCandidate>,
}

#[derive(Debug, Serialize)]
struct MetarBakeoffCandidate {
    name: &'static str,
    serializer: &'static str,
    indexing_strategy: &'static str,
    encoded_bytes: usize,
    encode_ms: f64,
    avg_decode_install_ms: f64,
    min_decode_install_ms: f64,
    max_decode_install_ms: f64,
    checksum: u64,
    tile_count: usize,
    tile_ref_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedMetarFeed {
    version_label: String,
    generated_at_utc: Option<String>,
    records: Vec<PreparedMetarRecord>,
    tiles: Vec<PreparedMetarTile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedMetarRecord {
    station_id: String,
    raw_text: String,
    observed_at_utc: Option<String>,
    flight_category: Option<String>,
    cloud_symbol: Option<String>,
    longitude: f64,
    latitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedMetarTile {
    z: u32,
    x: u32,
    y: u32,
    record_indexes: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct TileBuildStats {
    tile_count: usize,
    tile_ref_count: usize,
    checksum: u64,
}

#[derive(Debug, Clone, Copy)]
struct BakeoffMetarRecordView<'a> {
    station_id: &'a str,
    longitude: f64,
    latitude: f64,
}

#[derive(Debug, Deserialize)]
struct BakeoffMetarRecordDelta {
    from_version: String,
    to_version: String,
    #[serde(default)]
    top_level_removed: Vec<String>,
    #[serde(default)]
    top_level_changed: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    removed: Vec<String>,
    #[serde(default)]
    changed: serde_json::Map<String, serde_json::Value>,
}

#[wasm_bindgen]
pub fn metar_bakeoff_run(
    state_json: &str,
    from_state_json: &str,
    delta_json: &str,
    rounds: u32,
) -> Result<String, JsValue> {
    let rounds = rounds.clamp(1, 200);
    let payload: app_core::MetarProductPayload =
        serde_json::from_str(state_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let metar_count = payload.metars_by_station.len();
    let pirep_count = payload.pireps.len();
    let prepared = prepare_metar_feed(&payload);
    let mut candidates = Vec::new();

    candidates.push(benchmark_candidate(
        "current_delta_value_hash_install",
        "serde-json-delta",
        "late-indexed",
        delta_json.len(),
        0.0,
        rounds,
        || {
            let from_state: serde_json::Value =
                serde_json::from_str(from_state_json).map_err(|err| err.to_string())?;
            let _from_hash = bakeoff_canonical_json_sha256(&from_state)?;
            let delta: BakeoffMetarRecordDelta =
                serde_json::from_str(delta_json).map_err(|err| err.to_string())?;
            let next_state = apply_bakeoff_metar_record_delta(&from_state, &delta)?;
            let _next_hash = bakeoff_canonical_json_sha256(&next_state)?;
            let payload: app_core::MetarProductPayload =
                serde_json::from_value(next_state).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_payload(&payload))
        },
    )?);

    candidates.push(benchmark_candidate(
        "serde-json x late-indexed",
        "serde-json",
        "late-indexed",
        state_json.len(),
        0.0,
        rounds,
        || {
            let value: serde_json::Value =
                serde_json::from_str(state_json).map_err(|err| err.to_string())?;
            let payload: app_core::MetarProductPayload =
                serde_json::from_value(value).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_payload(&payload))
        },
    )?);

    candidates.push(benchmark_candidate(
        "serde-json-typed x late-indexed",
        "serde-json-typed",
        "late-indexed",
        state_json.len(),
        0.0,
        rounds,
        || {
            let payload: app_core::MetarProductPayload =
                serde_json::from_str(state_json).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_payload(&payload))
        },
    )?);

    let started = benchmark_now_ms();
    let bincode_bytes =
        bincode::serialize(&payload).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let bincode_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "serde-bincode x late-indexed",
        "serde-bincode",
        "late-indexed",
        bincode_bytes.len(),
        bincode_encode_ms,
        rounds,
        || {
            let payload: app_core::MetarProductPayload =
                bincode::deserialize(&bincode_bytes).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_payload(&payload))
        },
    )?);

    let started = benchmark_now_ms();
    let postcard_bytes =
        postcard::to_allocvec(&payload).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let postcard_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "serde-postcard x late-indexed",
        "serde-postcard",
        "late-indexed",
        postcard_bytes.len(),
        postcard_encode_ms,
        rounds,
        || {
            let payload: app_core::MetarProductPayload =
                postcard::from_bytes(&postcard_bytes).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_payload(&payload))
        },
    )?);

    let started = benchmark_now_ms();
    let custom_raw_bytes =
        encode_custom_metar_records(&prepared).map_err(|err| JsValue::from_str(&err))?;
    let custom_raw_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "custom-bin x late-indexed",
        "custom-bin",
        "late-indexed",
        custom_raw_bytes.len(),
        custom_raw_encode_ms,
        rounds,
        || decode_custom_metar_records_late_stats(&custom_raw_bytes),
    )?);

    let started = benchmark_now_ms();
    let prepared_json =
        serde_json::to_string(&prepared).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let prepared_json_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "serde-json x early-indexed",
        "serde-json",
        "early-indexed",
        prepared_json.len(),
        prepared_json_encode_ms,
        rounds,
        || {
            let value: serde_json::Value =
                serde_json::from_str(&prepared_json).map_err(|err| err.to_string())?;
            let prepared: PreparedMetarFeed =
                serde_json::from_value(value).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_prepared_feed(&prepared))
        },
    )?);

    candidates.push(benchmark_candidate(
        "serde-json-typed x early-indexed",
        "serde-json-typed",
        "early-indexed",
        prepared_json.len(),
        prepared_json_encode_ms,
        rounds,
        || {
            let prepared: PreparedMetarFeed =
                serde_json::from_str(&prepared_json).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_prepared_feed(&prepared))
        },
    )?);

    let started = benchmark_now_ms();
    let prepared_bincode_bytes =
        bincode::serialize(&prepared).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let prepared_bincode_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "serde-bincode x early-indexed",
        "serde-bincode",
        "early-indexed",
        prepared_bincode_bytes.len(),
        prepared_bincode_encode_ms,
        rounds,
        || {
            let prepared: PreparedMetarFeed =
                bincode::deserialize(&prepared_bincode_bytes).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_prepared_feed(&prepared))
        },
    )?);

    let started = benchmark_now_ms();
    let prepared_postcard_bytes =
        postcard::to_allocvec(&prepared).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let prepared_postcard_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "serde-postcard x early-indexed",
        "serde-postcard",
        "early-indexed",
        prepared_postcard_bytes.len(),
        prepared_postcard_encode_ms,
        rounds,
        || {
            let prepared: PreparedMetarFeed =
                postcard::from_bytes(&prepared_postcard_bytes).map_err(|err| err.to_string())?;
            Ok(build_metar_tile_stats_from_prepared_feed(&prepared))
        },
    )?);

    let started = benchmark_now_ms();
    let prepared_bytes =
        encode_prepared_metar_feed(&prepared).map_err(|err| JsValue::from_str(&err))?;
    let prepared_encode_ms = benchmark_now_ms() - started;
    candidates.push(benchmark_candidate(
        "custom-bin x early-indexed",
        "custom-bin",
        "early-indexed",
        prepared_bytes.len(),
        prepared_encode_ms,
        rounds,
        || decode_prepared_metar_index_stats(&prepared_bytes),
    )?);

    serde_json::to_string(&MetarBakeoffReport {
        rounds,
        fixture_json_bytes: state_json.len(),
        metar_count,
        pirep_count,
        candidates,
    })
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

fn benchmark_candidate(
    name: &'static str,
    serializer: &'static str,
    indexing_strategy: &'static str,
    encoded_bytes: usize,
    encode_ms: f64,
    rounds: u32,
    mut work: impl FnMut() -> Result<TileBuildStats, String>,
) -> Result<MetarBakeoffCandidate, JsValue> {
    let mut elapsed = Vec::with_capacity(rounds as usize);
    let mut checksum = 0_u64;
    let mut tile_count = 0;
    let mut tile_ref_count = 0;
    for round in 0..rounds {
        let started = benchmark_now_ms();
        let stats = work().map_err(|err| JsValue::from_str(&format!("{name}: {err}")))?;
        elapsed.push(benchmark_now_ms() - started);
        checksum = checksum_mix(checksum, stats.checksum ^ u64::from(round));
        tile_count = stats.tile_count;
        tile_ref_count = stats.tile_ref_count;
    }
    let total = elapsed.iter().sum::<f64>();
    let min = elapsed.iter().copied().fold(f64::INFINITY, f64::min);
    let max = elapsed.iter().copied().fold(0.0_f64, f64::max);
    Ok(MetarBakeoffCandidate {
        name,
        serializer,
        indexing_strategy,
        encoded_bytes,
        encode_ms: round_ms(encode_ms),
        avg_decode_install_ms: round_ms(total / rounds as f64),
        min_decode_install_ms: round_ms(min),
        max_decode_install_ms: round_ms(max),
        checksum,
        tile_count,
        tile_ref_count,
    })
}

fn round_ms(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn build_metar_tile_stats_from_payload(payload: &app_core::MetarProductPayload) -> TileBuildStats {
    build_metar_tile_stats_from_records(
        &payload.version_label,
        payload
            .metars_by_station
            .values()
            .map(|record| BakeoffMetarRecordView {
                station_id: &record.station_id,
                longitude: record.longitude,
                latitude: record.latitude,
            }),
    )
}

fn build_metar_tile_stats_from_records<'a>(
    version_label: &str,
    records: impl IntoIterator<Item = BakeoffMetarRecordView<'a>>,
) -> TileBuildStats {
    let mut tiles = std::collections::BTreeMap::<(u32, u32, u32), Vec<u64>>::new();
    let mut checksum = checksum_str(version_label);
    let records = records.into_iter().collect::<Vec<_>>();
    for zoom in [5_u32, 6, 7] {
        for record in &records {
            let Some((x, y)) = metar_bakeoff_tile_xy(record.latitude, record.longitude, zoom)
            else {
                continue;
            };
            tiles
                .entry((zoom, x, y))
                .or_default()
                .push(checksum_str(record.station_id));
        }
    }
    let mut tile_ref_count = 0_usize;
    for ((z, x, y), refs) in &tiles {
        checksum = checksum_mix(checksum, u64::from(*z));
        checksum = checksum_mix(checksum, u64::from(*x));
        checksum = checksum_mix(checksum, u64::from(*y));
        tile_ref_count += refs.len();
        for station_checksum in refs {
            checksum = checksum_mix(checksum, *station_checksum);
        }
    }
    TileBuildStats {
        tile_count: tiles.len(),
        tile_ref_count,
        checksum,
    }
}

fn build_metar_tile_stats_from_prepared_feed(feed: &PreparedMetarFeed) -> TileBuildStats {
    let mut checksum = checksum_str(&feed.version_label);
    let mut tile_ref_count = 0_usize;
    for tile in &feed.tiles {
        checksum = checksum_mix(checksum, u64::from(tile.z));
        checksum = checksum_mix(checksum, u64::from(tile.x));
        checksum = checksum_mix(checksum, u64::from(tile.y));
        tile_ref_count += tile.record_indexes.len();
        for index in &tile.record_indexes {
            let Some(record) = feed.records.get(*index as usize) else {
                continue;
            };
            checksum = checksum_mix(checksum, checksum_str(&record.station_id));
        }
    }
    TileBuildStats {
        tile_count: feed.tiles.len(),
        tile_ref_count,
        checksum,
    }
}

fn apply_bakeoff_metar_record_delta(
    from_state: &serde_json::Value,
    delta: &BakeoffMetarRecordDelta,
) -> Result<serde_json::Value, String> {
    let from_version = from_state
        .get("version_label")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "live feed state missing version_label".to_string())?;
    if from_version != delta.from_version {
        return Err(format!(
            "delta starts at {}, but local state is {from_version}",
            delta.from_version
        ));
    }
    let mut result = from_state.clone();
    {
        let result_object = result
            .as_object_mut()
            .ok_or_else(|| "live feed state must be a JSON object".to_string())?;
        for key in &delta.top_level_removed {
            result_object.remove(key);
        }
        for (key, value) in &delta.top_level_changed {
            result_object.insert(key.clone(), value.clone());
        }
    }
    let record_count = {
        let records = result
            .get_mut("metars_by_station")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| "state missing metars_by_station object".to_string())?;
        for station_id in &delta.removed {
            records.remove(station_id);
        }
        for (station_id, record) in &delta.changed {
            records.insert(station_id.clone(), record.clone());
        }
        records.len()
    };
    let version = result
        .get_mut("version_label")
        .ok_or_else(|| "live feed state missing version_label".to_string())?;
    *version = serde_json::Value::String(delta.to_version.clone());
    if let Some(count) = result.get_mut("metar_count") {
        *count = serde_json::json!(record_count);
    }
    Ok(result)
}

fn bakeoff_canonical_json_sha256(value: &serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|err| err.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn prepare_metar_feed(payload: &app_core::MetarProductPayload) -> PreparedMetarFeed {
    let mut source_records = payload.metars_by_station.values().collect::<Vec<_>>();
    source_records.sort_by(|left, right| left.station_id.cmp(&right.station_id));
    let records = source_records
        .iter()
        .map(|record| PreparedMetarRecord {
            station_id: record.station_id.clone(),
            raw_text: record.raw_text.clone(),
            observed_at_utc: record.observed_at_utc.clone(),
            flight_category: record.flight_category.clone(),
            cloud_symbol: record
                .clouds
                .as_ref()
                .and_then(|clouds| clouds.symbol.clone()),
            longitude: record.longitude,
            latitude: record.latitude,
        })
        .collect::<Vec<_>>();
    let mut tiles = std::collections::BTreeMap::<(u32, u32, u32), Vec<u32>>::new();
    for zoom in [5_u32, 6, 7] {
        for (record_index, record) in records.iter().enumerate() {
            let Some((x, y)) = metar_bakeoff_tile_xy(record.latitude, record.longitude, zoom)
            else {
                continue;
            };
            tiles
                .entry((zoom, x, y))
                .or_default()
                .push(record_index as u32);
        }
    }
    PreparedMetarFeed {
        version_label: payload.version_label.clone(),
        generated_at_utc: payload.generated_at_utc.map(|value| value.to_rfc3339()),
        records,
        tiles: tiles
            .into_iter()
            .map(|((z, x, y), record_indexes)| PreparedMetarTile {
                z,
                x,
                y,
                record_indexes,
            })
            .collect(),
    }
}

fn encode_prepared_metar_feed(feed: &PreparedMetarFeed) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(b"ABMF");
    write_u32(&mut out, 1);
    write_string(&mut out, &feed.version_label)?;
    write_option_string(&mut out, feed.generated_at_utc.as_deref())?;
    write_u32(&mut out, checked_u32(feed.records.len(), "record count")?);
    for record in &feed.records {
        write_string(&mut out, &record.station_id)?;
        write_string(&mut out, &record.raw_text)?;
        write_option_string(&mut out, record.observed_at_utc.as_deref())?;
        write_option_string(&mut out, record.flight_category.as_deref())?;
        write_option_string(&mut out, record.cloud_symbol.as_deref())?;
        write_f64(&mut out, record.longitude);
        write_f64(&mut out, record.latitude);
    }
    write_u32(&mut out, checked_u32(feed.tiles.len(), "tile count")?);
    for tile in &feed.tiles {
        write_u32(&mut out, tile.z);
        write_u32(&mut out, tile.x);
        write_u32(&mut out, tile.y);
        write_u32(
            &mut out,
            checked_u32(tile.record_indexes.len(), "tile record count")?,
        );
        for index in &tile.record_indexes {
            write_u32(&mut out, *index);
        }
    }
    Ok(out)
}

fn encode_custom_metar_records(feed: &PreparedMetarFeed) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(b"ABMR");
    write_u32(&mut out, 1);
    write_string(&mut out, &feed.version_label)?;
    write_option_string(&mut out, feed.generated_at_utc.as_deref())?;
    write_u32(&mut out, checked_u32(feed.records.len(), "record count")?);
    for record in &feed.records {
        write_string(&mut out, &record.station_id)?;
        write_string(&mut out, &record.raw_text)?;
        write_option_string(&mut out, record.observed_at_utc.as_deref())?;
        write_option_string(&mut out, record.flight_category.as_deref())?;
        write_option_string(&mut out, record.cloud_symbol.as_deref())?;
        write_f64(&mut out, record.longitude);
        write_f64(&mut out, record.latitude);
    }
    Ok(out)
}

fn decode_custom_metar_records_late_stats(bytes: &[u8]) -> Result<TileBuildStats, String> {
    let mut cursor = ByteCursor::new(bytes);
    if cursor.read_bytes(4)? != b"ABMR" {
        return Err("bad custom METAR record magic".to_string());
    }
    let schema = cursor.read_u32()?;
    if schema != 1 {
        return Err(format!("unsupported custom METAR record schema {schema}"));
    }
    let version_label = cursor.read_string_lossy()?;
    let _generated_at_utc = cursor.read_option_string_lossy()?;
    let record_count = cursor.read_u32()? as usize;
    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        let station_id = cursor.read_string_lossy()?;
        let _raw_text_checksum = cursor.skip_string_with_checksum()?;
        let _observed_at_utc_checksum = cursor.skip_option_string_with_checksum()?;
        let _flight_category_checksum = cursor.skip_option_string_with_checksum()?;
        let _cloud_symbol_checksum = cursor.skip_option_string_with_checksum()?;
        let longitude = cursor.read_f64()?;
        let latitude = cursor.read_f64()?;
        records.push((station_id, longitude, latitude));
    }
    if !cursor.is_finished() {
        return Err("trailing bytes in custom METAR record table".to_string());
    }
    Ok(build_metar_tile_stats_from_records(
        &version_label,
        records
            .iter()
            .map(|(station_id, longitude, latitude)| BakeoffMetarRecordView {
                station_id,
                longitude: *longitude,
                latitude: *latitude,
            }),
    ))
}

fn decode_prepared_metar_index_stats(bytes: &[u8]) -> Result<TileBuildStats, String> {
    let mut cursor = ByteCursor::new(bytes);
    if cursor.read_bytes(4)? != b"ABMF" {
        return Err("bad prepared METAR magic".to_string());
    }
    let schema = cursor.read_u32()?;
    if schema != 1 {
        return Err(format!("unsupported prepared METAR schema {schema}"));
    }
    let version_label = cursor.read_string_lossy()?;
    let mut checksum = checksum_str(&version_label);
    let _generated_at_utc = cursor.read_option_string_lossy()?;
    let record_count = cursor.read_u32()? as usize;
    let mut record_offsets = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        record_offsets.push(cursor.position() as u32);
        checksum = checksum_mix(checksum, cursor.skip_string_with_checksum()?);
        checksum = checksum_mix(checksum, cursor.skip_string_with_checksum()?);
        checksum = checksum_mix(checksum, cursor.skip_option_string_with_checksum()?);
        checksum = checksum_mix(checksum, cursor.skip_option_string_with_checksum()?);
        checksum = checksum_mix(checksum, cursor.skip_option_string_with_checksum()?);
        checksum = checksum_mix(checksum, cursor.read_f64()?.to_bits());
        checksum = checksum_mix(checksum, cursor.read_f64()?.to_bits());
    }
    let tile_count = cursor.read_u32()? as usize;
    let mut tile_ref_count = 0_usize;
    for _ in 0..tile_count {
        checksum = checksum_mix(checksum, cursor.read_u32()? as u64);
        checksum = checksum_mix(checksum, cursor.read_u32()? as u64);
        checksum = checksum_mix(checksum, cursor.read_u32()? as u64);
        let refs = cursor.read_u32()? as usize;
        tile_ref_count += refs;
        for _ in 0..refs {
            let index = cursor.read_u32()? as usize;
            let offset = record_offsets
                .get(index)
                .ok_or_else(|| format!("tile referenced bad record index {index}"))?;
            checksum = checksum_mix(checksum, *offset as u64);
        }
    }
    if !cursor.is_finished() {
        return Err("trailing bytes in prepared METAR table".to_string());
    }
    Ok(TileBuildStats {
        tile_count,
        tile_ref_count,
        checksum,
    })
}

struct ByteCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn is_finished(&self) -> bool {
        self.position == self.bytes.len()
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(len)
            .ok_or_else(|| "prepared METAR cursor overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "prepared METAR table ended early".to_string())?;
        self.position = end;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_string_lossy(&mut self) -> Result<String, String> {
        let bytes = self.read_length_prefixed_bytes()?;
        Ok(String::from_utf8_lossy(bytes).to_string())
    }

    fn read_option_string_lossy(&mut self) -> Result<Option<String>, String> {
        let present = self.read_bytes(1)?[0];
        match present {
            0 => Ok(None),
            1 => self.read_string_lossy().map(Some),
            _ => Err(format!("bad optional string tag {present}")),
        }
    }

    fn skip_string_with_checksum(&mut self) -> Result<u64, String> {
        let bytes = self.read_length_prefixed_bytes()?;
        Ok(checksum_bytes(bytes))
    }

    fn skip_option_string_with_checksum(&mut self) -> Result<u64, String> {
        let present = self.read_bytes(1)?[0];
        match present {
            0 => Ok(0),
            1 => self.skip_string_with_checksum(),
            _ => Err(format!("bad optional string tag {present}")),
        }
    }

    fn read_length_prefixed_bytes(&mut self) -> Result<&'a [u8], String> {
        let len = self.read_u32()? as usize;
        self.read_bytes(len)
    }
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    write_u32(out, checked_u32(value.len(), "string length")?);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_option_string(out: &mut Vec<u8>, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(value) => {
            out.push(1);
            write_string(out, value)
        }
        None => {
            out.push(0);
            Ok(())
        }
    }
}

fn checked_u32(value: usize, label: &str) -> Result<u32, String> {
    value
        .try_into()
        .map_err(|_| format!("{label} {value} exceeds u32"))
}

fn metar_bakeoff_tile_xy(lat: f64, lon: f64, zoom: u32) -> Option<(u32, u32)> {
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    let scale = 2_u32.checked_pow(zoom)?;
    let scale_f64 = scale as f64;
    let x = (((lon + 180.0) / 360.0) * scale_f64).floor();
    let clamped_lat = lat.clamp(-85.0511287798066, 85.0511287798066);
    let y = ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0
        * scale_f64)
        .floor();
    Some((
        positive_mod_i64_bakeoff(x as i64, scale as i64) as u32,
        (y as i64).clamp(0, scale as i64 - 1) as u32,
    ))
}

fn positive_mod_i64_bakeoff(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

fn checksum_str(value: &str) -> u64 {
    checksum_bytes(value.as_bytes())
}

fn checksum_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn checksum_mix(left: u64, right: u64) -> u64 {
    (left ^ right)
        .rotate_left(13)
        .wrapping_mul(0x9e3779b185ebca87)
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
    let outcome = app_core::run_had_operation(store, operation)
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
    let outcome = app_core::ingest_live_feed_sse_event_in_session(session_handle, &event)
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
    let outcome = app_core::ingest_live_feed_sse_events_in_session(session_handle, &events)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn prepare_metar_live_feed_resource(
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let mut current_state = lock_metar_live_feed_prep_state();
    let (next_state, prepared) = if resource_id.starts_with("live_feeds/state/metars/") {
        app_core::prepare_metar_live_feed_state_resource(resource_id, resource_bytes)
    } else if resource_id.starts_with("live_feeds/delta/metars/") {
        let state = current_state.as_ref().ok_or_else(|| {
            JsValue::from_str("METAR live-feed preparer has no current state for delta")
        })?;
        app_core::prepare_metar_live_feed_delta_resource(resource_id, state, resource_bytes)
    } else {
        return Err(JsValue::from_str(&format!(
            "unsupported METAR live-feed prep resource: {resource_id}"
        )));
    }
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    *current_state = Some(next_state);
    Ok(prepared)
}

#[wasm_bindgen]
pub fn reset_metar_live_feed_preparer() {
    *lock_metar_live_feed_prep_state() = None;
}

#[wasm_bindgen]
pub fn perform_map_selection_action_in_session(
    session_handle: u32,
    action_json: &str,
) -> Result<String, JsValue> {
    let outcome =
        app_core::perform_map_selection_action_in_session(session_handle, action_json.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn insert_waypoint_at_flight_plan_row_in_session(
    session_handle: u32,
    row_uid: &str,
    before: bool,
    waypoint_json: &str,
) -> Result<String, JsValue> {
    let waypoint: app_core::NavRef =
        serde_json::from_str(waypoint_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::session::insert_waypoint_at_flight_plan_row_in_session(
        session_handle,
        row_uid.to_string(),
        before,
        waypoint,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn suggest_waypoint_identifiers_at_flight_plan_row_in_session(
    session_handle: u32,
    row_uid: &str,
    before: bool,
    prefix: &str,
    limit: usize,
) -> Result<String, JsValue> {
    let outcome = app_core::session::suggest_waypoint_identifiers_at_flight_plan_row_in_session(
        session_handle,
        row_uid.to_string(),
        before,
        prefix.to_string(),
        limit,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn preview_flight_plan_entry_in_session(
    session_handle: u32,
    input: &str,
) -> Result<String, JsValue> {
    let outcome =
        app_core::session::preview_flight_plan_entry_in_session(session_handle, input.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn append_flight_plan_entry_in_session(
    session_handle: u32,
    input: &str,
) -> Result<String, JsValue> {
    let outcome =
        app_core::session::append_flight_plan_entry_in_session(session_handle, input.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn insert_airway_at_flight_plan_row_in_session(
    session_handle: u32,
    row_uid: &str,
    presentation_json: &str,
    entry_index: usize,
    exit_index: usize,
) -> Result<String, JsValue> {
    let presentation: app_core::AirwayPresentationPlan = serde_json::from_str(presentation_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::session::insert_airway_at_flight_plan_row_in_session(
        session_handle,
        row_uid.to_string(),
        presentation,
        entry_index,
        exit_index,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn select_procedure_at_flight_plan_row_in_session(
    session_handle: u32,
    row_uid: &str,
    airport_id: &str,
    procedure_id: &str,
    procedure_kind_json: &str,
    runway_transition_json: &str,
    enroute_transition_json: &str,
) -> Result<String, JsValue> {
    let procedure_kind: app_core::ProcedureKind = serde_json::from_str(procedure_kind_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let runway_transition: Option<String> = serde_json::from_str(runway_transition_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let enroute_transition: Option<String> = serde_json::from_str(enroute_transition_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let outcome = app_core::session::select_procedure_at_flight_plan_row_in_session(
        session_handle,
        row_uid.to_string(),
        airport_id.to_string(),
        procedure_id.to_string(),
        procedure_kind,
        runway_transition,
        enroute_transition,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn load_plate_procedure_in_session(
    session_handle: u32,
    load_id: &str,
) -> Result<String, JsValue> {
    let outcome =
        app_core::session::load_plate_procedure_in_session(session_handle, load_id.to_string())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&outcome).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn restore_direct_to_in_session(session_handle: u32) -> Result<String, JsValue> {
    let snapshot = app_core::session::restore_direct_to_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn perform_flight_plan_row_action_in_session(
    session_handle: u32,
    row_uid: &str,
    action_uid: &str,
) -> Result<String, JsValue> {
    let outcome = app_core::session::perform_flight_plan_row_action_in_session(
        session_handle,
        row_uid.to_string(),
        action_uid.to_string(),
    )
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
pub fn activate_next_leg_in_session(session_handle: u32) -> Result<String, JsValue> {
    let snapshot = app_core::activate_next_leg_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn suspend_sequencing_in_session(session_handle: u32) -> Result<String, JsValue> {
    let snapshot = app_core::suspend_sequencing_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn unsuspend_sequencing_in_session(session_handle: u32) -> Result<String, JsValue> {
    let snapshot = app_core::unsuspend_sequencing_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn sequence_active_leg_in_session(session_handle: u32) -> Result<String, JsValue> {
    let snapshot = app_core::sequence_active_leg_in_session(session_handle)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
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
    app_core::attach_nav_kv_store_to_session(session_handle, nav_kv_handle, store)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[cfg(debug_assertions)]
#[wasm_bindgen]
pub fn startup_smoke_test() -> Result<(), JsValue> {
    let init = app_core::create_ui_session_at_epoch_ms(
        app_core::FlightPlan::default(),
        &[],
        None,
        None,
        now_ms() as i64,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))?;
    let store = app_core::nav_kv_store_for_smoke_test(
        &[(
            "package/by-id/NAV_DB_SMOKE",
            br#"{
                "id": "NAV_DB_SMOKE",
                "family_id": "nav-db",
                "expiration_date": "2020-01-01"
            }"#,
        )],
        256,
    );
    let nav_kv_handle = NEXT_NAV_KV_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_nav_kv_stores().insert(nav_kv_handle, store);
    attach_nav_kv_store_to_session(nav_kv_handle, init.handle)?;
    sync_guidance_geometry_in_session(init.handle)?;
    get_session_snapshot(init.handle)?;
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
pub fn build_flight_plan(plan_json: &str) -> Result<String, JsValue> {
    build_flight_plan_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn empty_flight_plan_json() -> Result<String, JsValue> {
    serde_json::to_string(&app_core::FlightPlan::empty())
        .map_err(|err| JsValue::from_str(&err.to_string()))
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
pub fn create_ui_session(
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    create_ui_session_json(
        plan_json,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn create_ui_session_profiled(
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, JsValue> {
    create_ui_session_profiled_json(
        plan_json,
        recent_airport_ids_json,
        selected_airport_id_json,
        selected_chart_id_json,
    )
    .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_resource_policy_in_session(handle: u32, policy_json: &str) -> Result<String, JsValue> {
    set_resource_policy_in_session_json(handle, policy_json).map_err(|err| JsValue::from_str(&err))
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
pub fn register_ownship_source_in_session_paged(
    handle: u32,
    registration_json: &str,
) -> Result<String, JsValue> {
    register_ownship_source_in_session_paged_json(handle, registration_json)
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
pub fn update_ownship_source_status_in_session_paged(
    handle: u32,
    update_json: &str,
) -> Result<String, JsValue> {
    update_ownship_source_status_in_session_paged_json(handle, update_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn push_situation_sample_in_session(handle: u32, sample_json: &str) -> Result<String, JsValue> {
    push_situation_sample_in_session_json(handle, sample_json)
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
pub fn select_ownship_source_in_session(
    handle: u32,
    selection_json: &str,
) -> Result<String, JsValue> {
    select_ownship_source_in_session_json(handle, selection_json)
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
pub fn set_situation_in_session(handle: u32, situation_json: &str) -> Result<String, JsValue> {
    set_situation_in_session_json(handle, situation_json).map_err(|err| JsValue::from_str(&err))
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
pub fn tick_debug_ownship_driver_in_session(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    tick_debug_ownship_driver_in_session_json(handle, now_epoch_ms)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn tick_debug_ownship_driver_in_session_paged(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, JsValue> {
    tick_debug_ownship_driver_in_session_paged_json(handle, now_epoch_ms)
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
pub fn set_debug_flag_in_session(
    handle: u32,
    flag_id_json: &str,
    enabled: bool,
) -> Result<String, JsValue> {
    set_debug_flag_in_session_json(handle, flag_id_json, enabled)
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
pub fn ingest_resource_in_session(
    handle: u32,
    resource_id: &str,
    resource_bytes: &[u8],
) -> Result<(), JsValue> {
    app_core::ingest_resource_in_session(handle, resource_id, resource_bytes)
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn ingest_prepared_metar_live_feed_resource_in_session(
    handle: u32,
    resource_id: &str,
    prepared_resource_bytes: &[u8],
) -> Result<(), JsValue> {
    app_core::ingest_prepared_metar_live_feed_resource_in_session(
        handle,
        resource_id,
        prepared_resource_bytes,
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
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

fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
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
        now_ms() as i64,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

fn create_ui_session_profiled_json(
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
    let result = app_core::create_ui_session_profiled_at_epoch_ms(
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
        now_ms() as i64,
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

fn register_ownship_source_in_session_paged_json(
    handle: u32,
    registration_json: &str,
) -> Result<String, String> {
    let registration: app_core::OwnshipSourceRegistration =
        serde_json::from_str(registration_json).map_err(|err| err.to_string())?;
    let outcome = app_core::register_ownship_source_in_session_outcome(handle, registration)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

fn update_ownship_source_status_in_session_paged_json(
    handle: u32,
    update_json: &str,
) -> Result<String, String> {
    let update: app_core::OwnshipSourceStatusUpdate =
        serde_json::from_str(update_json).map_err(|err| err.to_string())?;
    let outcome = app_core::update_ownship_source_status_in_session_outcome(handle, update)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn push_situation_sample_in_session_json(handle: u32, sample_json: &str) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::push_situation_sample_in_session(handle, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn push_situation_sample_in_session_paged_json(
    handle: u32,
    sample_json: &str,
) -> Result<String, String> {
    let sample: app_core::SituationSample =
        serde_json::from_str(sample_json).map_err(|err| err.to_string())?;
    let outcome = app_core::push_situation_sample_in_session_outcome(handle, sample)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
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

fn select_ownship_source_in_session_paged_json(
    handle: u32,
    selection_json: &str,
) -> Result<String, String> {
    let selection: app_core::OwnshipSelectionCommand =
        serde_json::from_str(selection_json).map_err(|err| err.to_string())?;
    let outcome = app_core::select_ownship_source_in_session_outcome(handle, selection)
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

fn set_situation_in_session_json(handle: u32, situation_json: &str) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::set_situation_in_session(handle, situation).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn set_situation_in_session_paged_json(
    handle: u32,
    situation_json: &str,
) -> Result<String, String> {
    let situation: app_core::Situation =
        serde_json::from_str(situation_json).map_err(|err| err.to_string())?;
    let outcome = app_core::set_situation_in_session_outcome(handle, situation)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

fn tick_debug_ownship_driver_in_session_json(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let snapshot = app_core::tick_debug_ownship_driver_in_session(handle, now_epoch_ms)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

fn tick_debug_ownship_driver_in_session_paged_json(
    handle: u32,
    now_epoch_ms: f64,
) -> Result<String, String> {
    let outcome = app_core::tick_debug_ownship_driver_in_session_outcome(handle, now_epoch_ms)
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

fn set_debug_flag_in_session_json(
    handle: u32,
    flag_id_json: &str,
    enabled: bool,
) -> Result<String, String> {
    let flag_id: String = serde_json::from_str(flag_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::set_debug_flag_in_session(handle, &flag_id, enabled)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
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

fn get_session_snapshot_json(handle: u32) -> Result<String, String> {
    let total_started_at = now_ms();
    let core_started_at = now_ms();
    let snapshot = app_core::get_session_snapshot(handle).map_err(|err| err.to_string())?;
    let core_ms = now_ms() - core_started_at;
    let serialize_started_at = now_ms();
    let serialized = serde_json::to_string(&snapshot).map_err(|err| err.to_string())?;
    let serialize_ms = now_ms() - serialize_started_at;
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
}
