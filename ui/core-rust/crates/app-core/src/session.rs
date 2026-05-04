use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    chart_page::airport_ids_from_plan,
    guidance_detail_id_for_index,
    had_ops::{insert_waypoint_best_position, HadOperationOutcome, HadReadError},
    map_follow::{MapFollowSessionState, MapFollowUiState},
    map_overlay_config_from_vector_manifest_json, move_flight_plan_waypoint, nav_kv_key_for_query,
    planning::NavElementUiView,
    playback::PlaybackSessionState,
    query_map_overlay, query_map_selection, remove_flight_plan_leg, state,
    AirportPlateAvailability, AirspaceFeaturePayload, AirspaceLabelTilePayload,
    AirspaceReferenceTilePayload, AppError, AppErrorKind, AppEvent, AppResult, AppState,
    AppUiState, FlightPlan, FlightPlanRowActionExecution, FlightPlanRowActionId, LatLon,
    MapOverlayConfig, MapOverlayQueryResult, MapViewport, MetarProductPayload, MetarTilePayload,
    NavKvLookup, NavKvQuery, NavKvStore, NavRef, PlanLeg, PlaybackUiState, PointTilePayload,
    RasterMapCatalog, RasterTilePlan, SequencingMode, TerrainOverlayQueryResult, TfrProductPayload,
    UiSnapshotAppState,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiChartPageState {
    pub ordered_airport_ids: Vec<String>,
    pub recent_airport_ids: Vec<String>,
    pub selected_airport_id: String,
    pub selected_chart_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiMapLayerToggleState {
    pub visible: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiMapLayerState {
    pub vectors: UiMapLayerToggleState,
    pub metars: UiMapLayerToggleState,
    pub nexrad: UiMapLayerToggleState,
    pub terrain_warning: UiMapLayerToggleState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiCautionState {
    pub obstacle_display_limited: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDebugState {
    pub tile_labels: bool,
    pub playback_visible: bool,
    pub fast_tiles: bool,
    pub offline_simulated_clock_buttons: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionSnapshot {
    pub app_state: UiSnapshotAppState,
    pub app_ui_state: AppUiState,
    pub playback_ui_state: PlaybackUiState,
    pub map_follow_ui_state: MapFollowUiState,
    pub map_follow_target_viewport: Option<MapViewport>,
    pub chart_page_state: UiChartPageState,
    pub map_layer_state: UiMapLayerState,
    pub caution_state: UiCautionState,
    pub debug_state: UiDebugState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionInitResult {
    pub handle: u32,
    pub snapshot: UiSessionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionCreateTiming {
    pub label: &'static str,
    pub elapsed_ms: f64,
}

struct UiSession {
    app_state: AppState,
    playback: PlaybackSessionState,
    map_follow: MapFollowSessionState,
    guidance_leg_geometry: HashMap<String, GuidanceLegGeometry>,
    map_overlay_config: MapOverlayConfig,
    vector_manifest_loaded: bool,
    chart_page_state: UiChartPageState,
    nav_kv_store_id: Option<u32>,
    nav_kv_store: Option<NavKvStore>,
    map_layer_state: UiMapLayerState,
    caution_state: UiCautionState,
    debug_state: UiDebugState,
    raster_map_catalog: Option<RasterMapCatalog>,
    point_tile_cache: HashMap<String, PointTilePayload>,
    metar_tile_cache: HashMap<String, MetarTilePayload>,
    metar_payload: Option<MetarProductPayload>,
    airspace_ref_tile_cache: HashMap<String, AirspaceReferenceTilePayload>,
    airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    airspace_label_tile_cache: HashMap<String, AirspaceLabelTilePayload>,
    tfr_payload: Option<TfrProductPayload>,
}

const DIRECT_SITUATION_SOURCE_ID: &str = "__direct_situation__";
const PLAYBACK_SOURCE_ID: &str = "__playback_trace__";
const CDI_NM_PER_DOT: f64 = 1.0;
const CDI_OFFSCALE_DOTS: f64 = 2.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapLayerId {
    Vectors,
    Metars,
    Nexrad,
    TerrainWarning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceLegGeometry {
    pub leg_id: String,
    pub from: LatLon,
    pub to: LatLon,
    #[serde(default)]
    pub path: Vec<LatLon>,
}

static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<u32, UiSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<u32, UiSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn create_ui_session(
    vector_manifest_json: &str,
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        vector_manifest_json,
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        None,
    )
}

pub fn create_ui_session_profiled(
    vector_manifest_json: &str,
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    mark: &mut dyn FnMut(&'static str),
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        vector_manifest_json,
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        Some(mark),
    )
}

fn create_ui_session_inner(
    vector_manifest_json: &str,
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    mut mark: Option<&mut dyn FnMut(&'static str)>,
) -> AppResult<UiSessionInitResult> {
    let map_overlay_config = map_overlay_config_from_vector_manifest_json(vector_manifest_json)?;
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_parse_vector_manifest");
    }
    let app_state = state::reduce(
        &AppState::default(),
        AppEvent::ReplaceFlightPlan(plan.clone()),
    )?;
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_reduce_replace_flight_plan");
    }
    let chart_page_state = derive_compact_chart_page_state(
        &plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    );
    let map_layer_state = default_map_layer_state();
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_derive_chart_page_state");
    }
    let playback = PlaybackSessionState::default();
    let map_follow = MapFollowSessionState::default();
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_default_session_state");
    }
    let snapshot_app_state = state::project_ui_snapshot_app_state(&app_state);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_snapshot_app_state");
    }
    let app_ui_state = state::project_app_ui_state(&app_state);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_app_ui_state");
    }
    let playback_ui_state = playback.ui_state();
    let map_follow_ui_state = map_follow.ui_state(&app_state.ownship.render);
    let map_follow_target_viewport = map_follow.target_viewport(&app_state.ownship.render);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_other_ui_state");
    }
    let caution_state = default_caution_state();
    let debug_state = default_debug_state();
    let snapshot = UiSessionSnapshot {
        app_state: snapshot_app_state,
        app_ui_state,
        playback_ui_state,
        map_follow_ui_state,
        map_follow_target_viewport,
        chart_page_state: chart_page_state.clone(),
        map_layer_state: map_layer_state.clone(),
        caution_state: caution_state.clone(),
        debug_state: debug_state.clone(),
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    sessions().lock().expect("session store poisoned").insert(
        handle,
        UiSession {
            app_state,
            playback,
            map_follow,
            guidance_leg_geometry: HashMap::new(),
            map_overlay_config,
            vector_manifest_loaded: false,
            chart_page_state,
            nav_kv_store_id: None,
            nav_kv_store: None,
            map_layer_state,
            caution_state,
            debug_state,
            raster_map_catalog: None,
            point_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            airspace_ref_tile_cache: HashMap::new(),
            airspace_feature_cache: HashMap::new(),
            airspace_label_tile_cache: HashMap::new(),
            tfr_payload: None,
        },
    );
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_store_session");
    }
    Ok(UiSessionInitResult { handle, snapshot })
}

pub fn remove_leg_in_session(handle: u32, index: usize) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = remove_flight_plan_leg(&plan, index)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::ReplaceFlightPlan(next_plan.clone()),
    )?;
    session.guidance_leg_geometry.clear();
    session.chart_page_state = derive_compact_chart_page_state(
        &next_plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
    Ok(snapshot_for_session(session))
}

pub fn set_map_layer_visibility_in_session(
    handle: u32,
    layer_id: &str,
    visible: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    map_layer_toggle_mut(&mut session.map_layer_state, layer).visible = visible;
    Ok(snapshot_for_session(session))
}

pub fn set_raster_map_catalog_in_session(
    handle: u32,
    catalog: RasterMapCatalog,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.raster_map_catalog = Some(catalog);
    Ok(snapshot_for_session(session))
}

pub fn select_map_in_session(handle: u32, selected_map_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_mut() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    crate::select_map_in_catalog(catalog, selected_map_id);
    Ok(snapshot_for_session(session))
}

pub fn get_raster_tile_plan_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<RasterTilePlan> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    let options = crate::RasterTilePlanOptions {
        max_tile_display_multiplier: if session.debug_state.fast_tiles {
            2.0
        } else {
            1.0
        },
    };
    Ok(crate::raster_tile_plan_with_options(
        catalog, &viewport, width_px, height_px, options,
    ))
}

pub fn get_raster_tile_plan_in_session_with_options(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    options: crate::RasterTilePlanOptions,
) -> AppResult<RasterTilePlan> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    Ok(crate::raster_tile_plan_with_options(
        catalog, &viewport, width_px, height_px, options,
    ))
}

pub fn set_map_layer_enabled_in_session(
    handle: u32,
    layer_id: &str,
    enabled: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    let toggle = map_layer_toggle_mut(&mut session.map_layer_state, layer);
    toggle.enabled = enabled;
    if !enabled {
        toggle.visible = false;
    }
    Ok(snapshot_for_session(session))
}

pub fn move_waypoint_in_session(
    handle: u32,
    waypoint_index: usize,
    delta: isize,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = move_flight_plan_waypoint(&plan, waypoint_index, delta)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::ReplaceFlightPlan(next_plan.clone()),
    )?;
    session.guidance_leg_geometry.clear();
    session.chart_page_state = derive_compact_chart_page_state(
        &next_plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
    Ok(snapshot_for_session(session))
}

pub fn set_guidance_leg_geometry_in_session(
    handle: u32,
    geometries: Vec<GuidanceLegGeometry>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.guidance_leg_geometry = geometries
        .into_iter()
        .map(|geometry| (geometry.leg_id.clone(), geometry))
        .collect();
    Ok(snapshot_for_session(session))
}

pub fn select_airport_in_session(handle: u32, airport_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let mut recent_airport_ids = vec![airport_id.to_string()];
    recent_airport_ids.extend(
        session
            .chart_page_state
            .recent_airport_ids
            .iter()
            .filter(|id| id.as_str() != airport_id)
            .cloned(),
    );
    session.chart_page_state =
        derive_compact_chart_page_state(&plan, &recent_airport_ids, Some(airport_id), None);
    Ok(snapshot_for_session(session))
}

pub fn select_chart_in_session(handle: u32, chart_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(chart_id),
    );
    Ok(snapshot_for_session(session))
}

pub fn register_ownship_source_in_session(
    handle: u32,
    registration: crate::OwnshipSourceRegistration,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::RegisterOwnshipSource(registration),
    )?;
    Ok(snapshot_for_session(session))
}

pub fn update_ownship_source_status_in_session(
    handle: u32,
    update: crate::OwnshipSourceStatusUpdate,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::UpdateOwnshipSourceStatus(update),
    )?;
    Ok(snapshot_for_session(session))
}

pub fn push_situation_sample_in_session(
    handle: u32,
    sample: crate::SituationSample,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::PushSituationSample(sample))?;
    Ok(snapshot_for_session(session))
}

pub fn set_ownship_policy_in_session(
    handle: u32,
    policy: crate::OwnshipPolicy,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::SetOwnshipPolicy(policy))?;
    Ok(snapshot_for_session(session))
}

pub fn select_ownship_source_in_session(
    handle: u32,
    selection: crate::OwnshipSelectionCommand,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state =
        state::reduce(&session.app_state, AppEvent::SelectOwnshipSource(selection))?;
    Ok(snapshot_for_session(session))
}

pub fn load_playback_trace_in_session(
    handle: u32,
    source_path: &str,
    trace_json: &str,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let situation = session
        .playback
        .load_trace_json(source_path.to_string(), trace_json)?;
    apply_situation_to_ownship(
        session,
        PLAYBACK_SOURCE_ID,
        crate::OwnshipSourceKind::AdsbTrackPlayback,
        "ADS-B Trace Playback",
        situation,
        0,
    )?;
    Ok(snapshot_for_session(session))
}

pub fn play_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Some(situation) = session.playback.play(now_epoch_ms) {
        apply_situation_to_ownship(
            session,
            PLAYBACK_SOURCE_ID,
            crate::OwnshipSourceKind::AdsbTrackPlayback,
            "ADS-B Trace Playback",
            situation,
            now_epoch_ms as i64,
        )?;
    }
    Ok(snapshot_for_session(session))
}

pub fn pause_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Some(situation) = session.playback.pause(now_epoch_ms) {
        apply_situation_to_ownship(
            session,
            PLAYBACK_SOURCE_ID,
            crate::OwnshipSourceKind::AdsbTrackPlayback,
            "ADS-B Trace Playback",
            situation,
            now_epoch_ms as i64,
        )?;
    }
    Ok(snapshot_for_session(session))
}

pub fn seek_playback_in_session(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Some(situation) = session.playback.seek(cursor_seconds, now_epoch_ms) {
        apply_situation_to_ownship(
            session,
            PLAYBACK_SOURCE_ID,
            crate::OwnshipSourceKind::AdsbTrackPlayback,
            "ADS-B Trace Playback",
            situation,
            now_epoch_ms as i64,
        )?;
    }
    Ok(snapshot_for_session(session))
}

pub fn set_playback_rate_in_session(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Some(situation) = session.playback.set_rate(rate, now_epoch_ms) {
        apply_situation_to_ownship(
            session,
            PLAYBACK_SOURCE_ID,
            crate::OwnshipSourceKind::AdsbTrackPlayback,
            "ADS-B Trace Playback",
            situation,
            now_epoch_ms as i64,
        )?;
    }
    Ok(snapshot_for_session(session))
}

pub fn tick_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Some(situation) = session.playback.tick(now_epoch_ms) {
        apply_situation_to_ownship(
            session,
            PLAYBACK_SOURCE_ID,
            crate::OwnshipSourceKind::AdsbTrackPlayback,
            "ADS-B Trace Playback",
            situation,
            now_epoch_ms as i64,
        )?;
    }
    Ok(snapshot_for_session(session))
}

pub fn set_situation_in_session(
    handle: u32,
    situation: crate::Situation,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    apply_situation_to_ownship(
        session,
        DIRECT_SITUATION_SOURCE_ID,
        crate::OwnshipSourceKind::LiveNetworkTrack,
        "Direct Situation",
        situation,
        0,
    )?;
    Ok(snapshot_for_session(session))
}

pub fn replace_flight_plan_in_session(
    handle: u32,
    plan: FlightPlan,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    replace_session_flight_plan(session, plan)?;
    Ok(snapshot_for_session(session))
}

pub fn insert_waypoint_best_position_in_session(
    handle: u32,
    waypoint: NavRef,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let store = session_nav_kv_store(session)?;
    let mutation = match insert_waypoint_best_position(store, &plan, waypoint) {
        Ok(mutation) => mutation,
        Err(HadReadError::NeedPages(pages)) => return Ok(HadOperationOutcome::NeedPages { pages }),
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    replace_session_flight_plan(session, mutation.plan)?;
    let snapshot = snapshot_for_session(session);
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(snapshot).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

pub fn activate_direct_to_nav_ref_in_session(
    handle: u32,
    target: NavRef,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let from_position = session
        .app_state
        .ownship
        .render
        .position
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot activate direct-to without ownship position".to_string(),
        })?;
    let next_plan = crate::activate_direct_to(&plan, from_position, target)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn activate_direct_to_leg_in_session(
    handle: u32,
    target_leg_index: usize,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let target_leg_id = plan
        .resolved_legs
        .get(target_leg_index)
        .map(|leg| leg.id.clone())
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("invalid direct-to leg index: {target_leg_index}"),
        })?;
    let from_position = session
        .app_state
        .ownship
        .render
        .position
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot activate direct-to without ownship position".to_string(),
        })?;
    let next_plan = crate::activate_direct_to_leg(&plan, from_position, &target_leg_id)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn perform_flight_plan_row_action_in_session(
    handle: u32,
    row_uid: String,
    action_uid: String,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let ui = crate::project_ui_state(&plan);
    let row = ui
        .display_rows
        .iter()
        .find(|row| row.uid == row_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight-plan row action target is stale: {row_uid}"),
        })?;
    let action = row
        .actions
        .iter()
        .find(|action| action.uid == action_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("flight-plan row action is unavailable: {action_uid}"),
        })?;
    if !action.enabled {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("flight-plan row action is disabled: {action_uid}"),
        });
    }
    if action.execution != FlightPlanRowActionExecution::CoreSession {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("flight-plan row action is UI-controller owned: {action_uid}"),
        });
    }

    let from_position = session
        .app_state
        .ownship
        .render
        .position
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot activate direct-to without ownship position".to_string(),
        })?;
    let next_plan = match &action.id {
        FlightPlanRowActionId::DirectTo => {
            if let Some(target_leg_index) = row.leg_index {
                let target_leg_id = plan
                    .resolved_legs
                    .get(target_leg_index)
                    .map(|leg| leg.id.clone())
                    .ok_or_else(|| AppError {
                        kind: AppErrorKind::InvalidFlightPlan,
                        message: format!("invalid direct-to leg index: {target_leg_index}"),
                    })?;
                crate::activate_direct_to_leg(&plan, from_position, &target_leg_id)?
            } else {
                let target = row.nav_ref.clone().ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "direct-to row has no nav reference".to_string(),
                })?;
                crate::activate_direct_to(&plan, from_position, target)?
            }
        }
        _ => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("unsupported core flight-plan row action: {action_uid}"),
            });
        }
    };
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn restore_direct_to_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = crate::restore_direct_to(&plan)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn attach_nav_kv_store_to_session(
    handle: u32,
    store_id: u32,
    store: &NavKvStore,
) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.nav_kv_store_id = Some(store_id);
    session.nav_kv_store = Some(store.clone());
    Ok(())
}

pub fn insert_nav_kv_page_for_attached_sessions(store_id: u32, page_index: u32, page_bytes: &[u8]) {
    let mut sessions = sessions().lock().expect("session store poisoned");
    for session in sessions.values_mut() {
        if session.nav_kv_store_id == Some(store_id) {
            if let Some(store) = session.nav_kv_store.as_mut() {
                store.insert_page(page_index, page_bytes.to_vec());
            }
        }
    }
}

pub fn remove_top_level_waypoint_by_nav_ref_in_session(
    handle: u32,
    nav_ref: NavRef,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = crate::remove_top_level_waypoint_by_nav_ref(&plan, &nav_ref)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn engage_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.engage(viewport);
    Ok(snapshot_for_session(session))
}

pub fn disengage_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.disengage(viewport);
    Ok(snapshot_for_session(session))
}

pub fn set_map_follow_offset_in_session(
    handle: u32,
    viewport: MapViewport,
    offset_x_px: f64,
    offset_y_px: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session
        .map_follow
        .set_anchor_offset(viewport, offset_x_px, offset_y_px);
    Ok(snapshot_for_session(session))
}

pub fn sync_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.sync_for_viewport(
        &session.app_state.ownship.render,
        viewport,
        width_px,
        height_px,
    );
    Ok(snapshot_for_session(session))
}

pub fn restore_chart_page_state_in_session(
    handle: u32,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    );
    Ok(snapshot_for_session(session))
}

pub fn set_debug_flag_in_session(
    handle: u32,
    flag_id: &str,
    enabled: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    match flag_id {
        "tile_labels" => session.debug_state.tile_labels = enabled,
        "playback_visible" => session.debug_state.playback_visible = enabled,
        "fast_tiles" => session.debug_state.fast_tiles = enabled,
        "offline_simulated_clock_buttons" => {
            session.debug_state.offline_simulated_clock_buttons = enabled
        }
        _ => {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: format!("unknown debug flag id: {flag_id}"),
            });
        }
    }
    Ok(snapshot_for_session(session))
}

pub fn get_session_snapshot(handle: u32) -> AppResult<UiSessionSnapshot> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    Ok(snapshot_for_session(session))
}

pub fn ingest_point_tiles_in_session(handle: u32, tiles: &[PointTilePayload]) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session.point_tile_cache.insert(
            crate::tile_key(&tile.layer, tile.z, tile.x, tile.y),
            tile.clone(),
        );
    }
    Ok(())
}

pub fn ingest_metar_tiles_in_session(handle: u32, tiles: &[MetarTilePayload]) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session.metar_tile_cache.insert(
            crate::tile_key(&tile.layer, tile.z, tile.x, tile.y),
            tile.clone(),
        );
    }
    Ok(())
}

pub fn ingest_airspace_ref_tiles_in_session(
    handle: u32,
    tiles: &[AirspaceReferenceTilePayload],
) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session.airspace_ref_tile_cache.insert(
            crate::airspace_ref_tile_key(tile.z, tile.x, tile.y),
            tile.clone(),
        );
    }
    Ok(())
}

pub fn ingest_airspace_features_in_session(
    handle: u32,
    features: &[AirspaceFeaturePayload],
) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for feature in features {
        session
            .airspace_feature_cache
            .insert(feature.id.clone(), feature.clone());
    }
    Ok(())
}

pub fn ingest_airspace_label_tiles_in_session(
    handle: u32,
    tiles: &[AirspaceLabelTilePayload],
) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session.airspace_label_tile_cache.insert(
            crate::airspace_label_tile_key(tile.z, tile.x, tile.y),
            tile.clone(),
        );
    }
    Ok(())
}

pub fn ingest_tfrs_in_session(handle: u32, payload: &TfrProductPayload) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.tfr_payload = Some(payload.clone());
    Ok(())
}

pub fn ingest_metars_in_session(handle: u32, payload: &MetarProductPayload) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.metar_payload = Some(payload.clone());
    Ok(())
}

fn internal_json_error(err: serde_json::Error) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message: err.to_string(),
    }
}

fn had_read_error_to_overlay_outcome(err: HadReadError) -> AppResult<HadOperationOutcome> {
    match err {
        HadReadError::NeedPages(mut pages) => {
            pages.sort_unstable();
            pages.dedup();
            Ok(HadOperationOutcome::NeedPages { pages })
        }
        HadReadError::Fatal(message) => Err(AppError {
            kind: AppErrorKind::Internal,
            message,
        }),
    }
}

fn read_attached_json_optional<T: for<'de> Deserialize<'de>>(
    store: &NavKvStore,
    query: NavKvQuery,
) -> Result<Option<T>, HadReadError> {
    let Some(key) = nav_kv_key_for_query(&query) else {
        return Ok(None);
    };
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Ok(None),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
}

fn read_attached_json_required<T: for<'de> Deserialize<'de>>(
    store: &NavKvStore,
    query: NavKvQuery,
    family: &str,
) -> Result<T, HadReadError> {
    read_attached_json_optional(store, query.clone())?.ok_or_else(|| {
        let key = nav_kv_key_for_query(&query).unwrap_or_else(|| "<no-key>".to_string());
        HadReadError::Fatal(format!("HAD missing required {family} key: {key}"))
    })
}

fn ensure_vector_manifest_loaded(session: &mut UiSession) -> Result<(), HadReadError> {
    if session.vector_manifest_loaded {
        return Ok(());
    }
    let manifest_json = {
        let store = session.nav_kv_store.as_ref().ok_or_else(|| {
            HadReadError::Fatal("session missing nav kv store for vector overlay".to_string())
        })?;
        read_attached_json_required::<serde_json::Value>(
            store,
            NavKvQuery::VectorManifest,
            "vector manifest",
        )?
    };
    let manifest_json = serde_json::to_string(&manifest_json)
        .map_err(|err| HadReadError::Fatal(err.to_string()))?;
    let previous_config = session.map_overlay_config.clone();
    let mut next_config = map_overlay_config_from_vector_manifest_json(&manifest_json)
        .map_err(|err| HadReadError::Fatal(err.to_string()))?;
    next_config.metar_layer = previous_config.metar_layer;
    next_config.obstacle_layer = previous_config.obstacle_layer;
    session.map_overlay_config = next_config;
    session.vector_manifest_loaded = true;
    Ok(())
}

fn ensure_vector_inputs_loaded(
    session: &mut UiSession,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Result<(), HadReadError> {
    ensure_vector_manifest_loaded(session)?;
    for _ in 0..8 {
        let overlay = query_map_overlay(
            viewport,
            width_px,
            height_px,
            &session.map_overlay_config,
            true,
            false,
            ownship_overlay_context(session).as_ref(),
            &session.point_tile_cache,
            &session.metar_tile_cache,
            session.metar_payload.as_ref(),
            &session.airspace_ref_tile_cache,
            &session.airspace_feature_cache,
            &session.airspace_label_tile_cache,
            session.tfr_payload.as_ref(),
        );
        let needed_vector_inputs = overlay.needed_point_tiles.len()
            + overlay.needed_airspace_ref_tiles.len()
            + overlay.needed_airspace_features.len()
            + overlay.needed_airspace_label_tiles.len();
        if needed_vector_inputs == 0 {
            return Ok(());
        }

        let mut loaded_any = false;
        let store = session.nav_kv_store.as_ref().ok_or_else(|| {
            HadReadError::Fatal("session missing nav kv store for vector overlay".to_string())
        })?;
        let needed_pages = store
            .missing_pages_for_keys(&vector_input_keys(&overlay))
            .map_err(HadReadError::Fatal)?;
        if !needed_pages.is_empty() {
            return Err(HadReadError::NeedPages(needed_pages));
        }

        let mut point_tiles = Vec::new();
        for tile in overlay.needed_point_tiles {
            let payload = read_attached_json_optional::<PointTilePayload>(
                store,
                NavKvQuery::VectorPointTile {
                    layer: tile.layer.clone(),
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                },
            )?
            .unwrap_or(PointTilePayload {
                schema_version: 1,
                layer: tile.layer,
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: Vec::new(),
            });
            point_tiles.push(payload);
        }

        let mut ref_tiles = Vec::new();
        for tile in overlay.needed_airspace_ref_tiles {
            let payload = read_attached_json_optional::<AirspaceReferenceTilePayload>(
                store,
                NavKvQuery::VectorAirspaceRefTile {
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                },
            )?
            .unwrap_or(AirspaceReferenceTilePayload {
                schema_version: 1,
                layer: "airspace".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                refs: Vec::new(),
            });
            ref_tiles.push(payload);
        }

        let mut features = Vec::new();
        for feature in overlay.needed_airspace_features {
            let payload = read_attached_json_required::<AirspaceFeaturePayload>(
                store,
                NavKvQuery::VectorAirspaceFeature { id: feature.id },
                "vector airspace feature",
            )?;
            features.push(payload);
        }

        let mut label_tiles = Vec::new();
        for tile in overlay.needed_airspace_label_tiles {
            let payload = read_attached_json_optional::<AirspaceLabelTilePayload>(
                store,
                NavKvQuery::VectorAirspaceLabelTile {
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                },
            )?
            .unwrap_or(AirspaceLabelTilePayload {
                schema_version: 1,
                layer: "airspace-labels".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                labels: Vec::new(),
            });
            label_tiles.push(payload);
        }

        for tile in point_tiles {
            session
                .point_tile_cache
                .insert(crate::tile_key(&tile.layer, tile.z, tile.x, tile.y), tile);
            loaded_any = true;
        }
        for tile in ref_tiles {
            session
                .airspace_ref_tile_cache
                .insert(crate::airspace_ref_tile_key(tile.z, tile.x, tile.y), tile);
            loaded_any = true;
        }
        for feature in features {
            session
                .airspace_feature_cache
                .insert(feature.id.clone(), feature);
            loaded_any = true;
        }
        for tile in label_tiles {
            session
                .airspace_label_tile_cache
                .insert(crate::airspace_label_tile_key(tile.z, tile.x, tile.y), tile);
            loaded_any = true;
        }
        if !loaded_any {
            return Ok(());
        }
    }
    Err(HadReadError::Fatal(
        "vector overlay did not converge after loading HAD inputs".to_string(),
    ))
}

fn vector_input_keys(overlay: &MapOverlayQueryResult) -> Vec<String> {
    overlay
        .needed_point_tiles
        .iter()
        .filter_map(|tile| {
            nav_kv_key_for_query(&NavKvQuery::VectorPointTile {
                layer: tile.layer.clone(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
            })
        })
        .chain(overlay.needed_airspace_ref_tiles.iter().filter_map(|tile| {
            nav_kv_key_for_query(&NavKvQuery::VectorAirspaceRefTile {
                z: tile.z,
                x: tile.x,
                y: tile.y,
            })
        }))
        .chain(
            overlay
                .needed_airspace_features
                .iter()
                .filter_map(|feature| {
                    nav_kv_key_for_query(&NavKvQuery::VectorAirspaceFeature {
                        id: feature.id.clone(),
                    })
                }),
        )
        .chain(
            overlay
                .needed_airspace_label_tiles
                .iter()
                .filter_map(|tile| {
                    nav_kv_key_for_query(&NavKvQuery::VectorAirspaceLabelTile {
                        z: tile.z,
                        x: tile.x,
                        y: tile.y,
                    })
                }),
        )
        .collect()
}

pub fn get_map_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if !session.map_layer_state.vectors.visible && !session.map_layer_state.metars.visible {
        session.caution_state.obstacle_display_limited = false;
        return Ok(HadOperationOutcome::Complete {
            result: serde_json::to_value(empty_map_overlay_query()).map_err(internal_json_error)?,
        });
    }
    if session.map_layer_state.vectors.visible {
        if let Err(err) = ensure_vector_inputs_loaded(session, &viewport, width_px, height_px) {
            return had_read_error_to_overlay_outcome(err);
        }
    }
    let overlay = query_map_overlay(
        &viewport,
        width_px,
        height_px,
        &session.map_overlay_config,
        session.map_layer_state.vectors.visible,
        session.map_layer_state.metars.visible,
        ownship_overlay_context(session).as_ref(),
        &session.point_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        &session.airspace_ref_tile_cache,
        &session.airspace_feature_cache,
        &session.airspace_label_tile_cache,
        session.tfr_payload.as_ref(),
    );
    session.caution_state.obstacle_display_limited = overlay
        .warnings
        .iter()
        .any(|warning| warning.code == "vector_display_feature_limit");
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(overlay).map_err(internal_json_error)?,
    })
}

pub fn get_map_selection_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    click: LatLon,
    hit_radius_px: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    if let Err(err) = ensure_vector_inputs_loaded(session, &viewport, width_px, height_px) {
        return had_read_error_to_overlay_outcome(err);
    }
    let plan = session.app_state.active_plan.as_ref();
    let store = session_nav_kv_store(session)?;
    let mut missing_pages = Vec::new();
    let mut availability = |airport_id: &str| match airport_plate_availability(store, airport_id) {
        Ok(availability) => availability,
        Err(HadReadError::NeedPages(pages)) => {
            missing_pages.extend(pages);
            AirportPlateAvailability::default()
        }
        Err(HadReadError::Fatal(_)) => AirportPlateAvailability::default(),
    };
    let selection = query_map_selection(
        &viewport,
        width_px,
        height_px,
        &session.map_overlay_config,
        plan,
        click,
        hit_radius_px,
        &session.point_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        &session.airspace_feature_cache,
        session.tfr_payload.as_ref(),
        &mut availability,
    );
    if !missing_pages.is_empty() {
        missing_pages.sort_unstable();
        missing_pages.dedup();
        return Ok(HadOperationOutcome::NeedPages {
            pages: missing_pages,
        });
    }
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(selection).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

pub fn get_terrain_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<TerrainOverlayQueryResult> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    if !session.map_layer_state.terrain_warning.visible {
        return Ok(TerrainOverlayQueryResult {
            status: crate::TerrainOverlayStatus::Hidden,
            tile_requests: Vec::new(),
        });
    }
    let kinematics = session.app_state.ownship.resolved.kinematics.as_ref();
    let has_position = kinematics.is_some_and(|kinematics| {
        kinematics.position.lat.is_finite() && kinematics.position.lon.is_finite()
    });
    let has_altitude = ownship_terrain_altitude_ft(session).is_some();
    Ok(crate::query_terrain_overlay(
        &viewport,
        width_px,
        height_px,
        has_position,
        has_altitude,
    ))
}

pub fn render_terrain_overlay_tile_in_session(
    handle: u32,
    tile_bytes: &[u8],
    aircraft_altitude_ft: Option<f64>,
) -> AppResult<Vec<u8>> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    let altitude_ft = aircraft_altitude_ft
        .filter(|altitude| altitude.is_finite())
        .or_else(|| ownship_terrain_altitude_ft(session))
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "ownship altitude unavailable for terrain overlay".to_string(),
        })?;
    crate::render_terrain_warning_raw_rgba_from_tiles(&[tile_bytes], altitude_ft).map_err(|err| {
        AppError {
            kind: AppErrorKind::InvalidManifest,
            message: err,
        }
    })
}

pub fn render_terrain_overlay_tiles_in_session(
    handle: u32,
    packed_tile_bytes: &[u8],
    aircraft_altitude_ft: Option<f64>,
) -> AppResult<Vec<u8>> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    let altitude_ft = aircraft_altitude_ft
        .filter(|altitude| altitude.is_finite())
        .or_else(|| ownship_terrain_altitude_ft(session))
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "ownship altitude unavailable for terrain overlay".to_string(),
        })?;
    let unpacked_tiles =
        unpack_packed_terrain_tiles(packed_tile_bytes).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: err,
        })?;
    let tile_refs = unpacked_tiles.iter().map(Vec::as_slice).collect::<Vec<_>>();
    crate::render_terrain_warning_raw_rgba_from_tiles(&tile_refs, altitude_ft).map_err(|err| {
        AppError {
            kind: AppErrorKind::InvalidManifest,
            message: err,
        }
    })
}

fn unpack_packed_terrain_tiles(packed_tile_bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
        let end = *cursor + 4;
        let chunk = bytes
            .get(*cursor..end)
            .ok_or_else(|| "truncated packed terrain tile header".to_string())?;
        *cursor = end;
        Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    }

    let mut cursor = 0;
    let tile_count = read_u32(packed_tile_bytes, &mut cursor)? as usize;
    let mut tiles = Vec::with_capacity(tile_count);
    for _ in 0..tile_count {
        let byte_count = read_u32(packed_tile_bytes, &mut cursor)? as usize;
        let end = cursor + byte_count;
        let tile_bytes = packed_tile_bytes
            .get(cursor..end)
            .ok_or_else(|| "truncated packed terrain tile body".to_string())?;
        tiles.push(tile_bytes.to_vec());
        cursor = end;
    }
    if cursor != packed_tile_bytes.len() {
        return Err("packed terrain tile payload has trailing bytes".to_string());
    }
    Ok(tiles)
}

pub fn destroy_session(handle: u32) {
    let _ = sessions()
        .lock()
        .expect("session store poisoned")
        .remove(&handle);
}

fn ownship_terrain_altitude_ft(session: &UiSession) -> Option<f64> {
    session
        .app_state
        .ownship
        .resolved
        .kinematics
        .as_ref()
        .and_then(|kinematics| {
            kinematics
                .altitude_msl_ft
                .or(kinematics.pressure_altitude_ft)
        })
}

fn ownship_overlay_context(session: &UiSession) -> Option<crate::ObstacleOverlayContext> {
    let kinematics = session.app_state.ownship.resolved.kinematics.as_ref()?;
    Some(crate::ObstacleOverlayContext {
        position: kinematics.position,
        track_deg_true: kinematics.track_deg_true,
        ground_speed_kt: kinematics.ground_speed_kt,
        altitude_ft: kinematics
            .altitude_msl_ft
            .or(kinematics.pressure_altitude_ft),
    })
}

fn session_ref(sessions: &HashMap<u32, UiSession>, handle: u32) -> AppResult<&UiSession> {
    sessions.get(&handle).ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: format!("invalid ui session handle: {handle}"),
    })
}

fn session_mut(sessions: &mut HashMap<u32, UiSession>, handle: u32) -> AppResult<&mut UiSession> {
    sessions.get_mut(&handle).ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: format!("invalid ui session handle: {handle}"),
    })
}

fn session_plan(session: &UiSession) -> AppResult<FlightPlan> {
    session
        .app_state
        .active_plan
        .clone()
        .ok_or_else(|| AppError {
            kind: AppErrorKind::Internal,
            message: "session missing active plan".to_string(),
        })
}

fn session_nav_kv_store(session: &UiSession) -> AppResult<&NavKvStore> {
    session.nav_kv_store.as_ref().ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: "session missing nav kv store".to_string(),
    })
}

fn airport_plate_availability(
    store: &NavKvStore,
    airport_id: &str,
) -> Result<AirportPlateAvailability, HadReadError> {
    let key = nav_kv_key_for_query(&NavKvQuery::PlateAirport {
        airport_id: airport_id.to_ascii_uppercase(),
    })
    .ok_or_else(|| HadReadError::Fatal("invalid plate airport query".to_string()))?;
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => {
            let airport: crate::DerivedChartAirport =
                serde_json::from_slice(&bytes).map_err(|err| {
                    HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))
                })?;
            Ok(AirportPlateAvailability {
                plates: !airport.charts.is_empty(),
                csup: airport
                    .charts
                    .iter()
                    .any(|chart| chart.kind == "csup" || chart.folder_category == "csup"),
            })
        }
        NavKvLookup::MissingKey => Ok(AirportPlateAvailability::default()),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
}

fn replace_session_flight_plan(session: &mut UiSession, plan: FlightPlan) -> AppResult<()> {
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::ReplaceFlightPlan(plan.clone()),
    )?;
    session.guidance_leg_geometry.clear();
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
    Ok(())
}

fn snapshot_for_session(session: &UiSession) -> UiSessionSnapshot {
    let app_ui_state = project_session_app_ui_state(session);
    UiSessionSnapshot {
        app_state: state::project_ui_snapshot_app_state(&session.app_state),
        app_ui_state,
        playback_ui_state: session.playback.ui_state(),
        map_follow_ui_state: session
            .map_follow
            .ui_state(&session.app_state.ownship.render),
        map_follow_target_viewport: session
            .map_follow
            .target_viewport(&session.app_state.ownship.render),
        chart_page_state: session.chart_page_state.clone(),
        map_layer_state: session.map_layer_state.clone(),
        caution_state: session.caution_state.clone(),
        debug_state: session.debug_state.clone(),
    }
}

fn default_map_layer_state() -> UiMapLayerState {
    UiMapLayerState {
        vectors: UiMapLayerToggleState {
            visible: true,
            enabled: true,
        },
        metars: UiMapLayerToggleState {
            visible: true,
            enabled: true,
        },
        nexrad: UiMapLayerToggleState {
            visible: false,
            enabled: true,
        },
        terrain_warning: UiMapLayerToggleState {
            visible: true,
            enabled: true,
        },
    }
}

fn default_caution_state() -> UiCautionState {
    UiCautionState {
        obstacle_display_limited: false,
    }
}

fn default_debug_state() -> UiDebugState {
    UiDebugState {
        tile_labels: false,
        playback_visible: false,
        fast_tiles: false,
        offline_simulated_clock_buttons: false,
    }
}

fn parse_map_layer_id(layer_id: &str) -> AppResult<MapLayerId> {
    match layer_id {
        "vectors" => Ok(MapLayerId::Vectors),
        "metars" => Ok(MapLayerId::Metars),
        "nexrad" => Ok(MapLayerId::Nexrad),
        "terrain_warning" => Ok(MapLayerId::TerrainWarning),
        _ => Err(AppError {
            kind: AppErrorKind::Internal,
            message: format!("unknown map layer id: {layer_id}"),
        }),
    }
}

fn map_layer_toggle_mut(
    map_layer_state: &mut UiMapLayerState,
    layer_id: MapLayerId,
) -> &mut UiMapLayerToggleState {
    match layer_id {
        MapLayerId::Vectors => &mut map_layer_state.vectors,
        MapLayerId::Metars => &mut map_layer_state.metars,
        MapLayerId::Nexrad => &mut map_layer_state.nexrad,
        MapLayerId::TerrainWarning => &mut map_layer_state.terrain_warning,
    }
}

fn empty_map_overlay_query() -> MapOverlayQueryResult {
    MapOverlayQueryResult {
        needed_point_tiles: Vec::new(),
        needed_metar_tiles: Vec::new(),
        needed_airspace_ref_tiles: Vec::new(),
        needed_airspace_features: Vec::new(),
        needed_airspace_label_tiles: Vec::new(),
        needed_metars: false,
        needed_tfrs: false,
        visible_features: Vec::new(),
        visible_metars: Vec::new(),
        airspace_paths: Vec::new(),
        tfr_paths: Vec::new(),
        airspace_labels: Vec::new(),
        warnings: Vec::new(),
    }
}

fn project_session_app_ui_state(session: &UiSession) -> AppUiState {
    let mut app_ui_state = state::project_app_ui_state(&session.app_state);
    if let Some(active_plan) = app_ui_state.active_plan.as_mut() {
        if let Some(guidance) = active_plan.guidance.as_mut() {
            guidance.nav_element = project_active_leg_nav_element(session);
        }
    }
    app_ui_state
}

fn project_active_leg_nav_element(session: &UiSession) -> NavElementUiView {
    let Some(plan) = session.app_state.active_plan.as_ref() else {
        return NavElementUiView::default();
    };
    let Some(active_leg) = crate::active_guidance_leg(plan) else {
        return NavElementUiView::default();
    };
    let Some(geometry) = active_leg_geometry(plan, &active_leg, &session.guidance_leg_geometry)
    else {
        return NavElementUiView {
            active_leg_summary: format!(
                "{} -> {}",
                nav_ref_label(&active_leg.from),
                nav_ref_label(&active_leg.to)
            ),
            cdi_indicator_dots: None,
            cdi_offscale_readout: None,
        };
    };
    let course_deg = session
        .app_state
        .ownship
        .render
        .position
        .and_then(|position| active_course_deg(&geometry, position))
        .unwrap_or_else(|| bearing_degrees(geometry.from, geometry.to));
    let cdi_indicator_dots = session
        .app_state
        .ownship
        .render
        .position
        .map(|position| cdi_dots_for_guidance_geometry(&geometry, position));
    let cdi_offscale_readout = cdi_indicator_dots.and_then(cdi_offscale_readout);

    NavElementUiView {
        active_leg_summary: format!(
            "{} -> {} CRS {:03.0}",
            nav_ref_label(&active_leg.from),
            nav_ref_label(&active_leg.to),
            course_deg.round().rem_euclid(360.0),
        ),
        cdi_indicator_dots,
        cdi_offscale_readout,
    }
}

fn active_leg_geometry(
    plan: &FlightPlan,
    active_leg: &PlanLeg,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    if let (NavRef::LatLon(from), NavRef::LatLon(to)) = (&active_leg.from, &active_leg.to) {
        return Some(GuidanceLegGeometry {
            leg_id: "__latlon_leg__".to_string(),
            from: *from,
            to: *to,
            path: vec![*from, *to],
        });
    }
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        return guidance.direct_to.as_ref().and_then(|direct_to| {
            let geometry = if direct_to.target_leg_id.is_some() {
                guidance
                    .active_detail_index
                    .and_then(|detail_index| guidance_detail_id_for_index(plan, detail_index))
                    .and_then(|detail_id| geometry_by_leg_id.get(&detail_id))
            } else {
                geometry_by_leg_id.get("direct-to")
            }?;
            Some({
                let from = match &active_leg.from {
                    NavRef::LatLon(position) => *position,
                    _ => geometry.from,
                };
                let to = match &active_leg.to {
                    NavRef::LatLon(position) => *position,
                    _ => geometry.to,
                };
                GuidanceLegGeometry {
                    leg_id: geometry.leg_id.clone(),
                    from,
                    to,
                    path: if geometry.path.is_empty() {
                        vec![from, to]
                    } else {
                        geometry.path.clone()
                    },
                }
            })
        });
    }
    guidance
        .active_detail_index
        .and_then(|detail_index| guidance_detail_id_for_index(plan, detail_index))
        .and_then(|detail_id| geometry_by_leg_id.get(&detail_id))
        .cloned()
}

fn cdi_dots_for_leg(from: LatLon, to: LatLon, position: LatLon) -> f32 {
    if crate::great_circle_distance_nm(from, to) <= f64::EPSILON {
        return 0.0;
    }
    (crate::cross_track_left_nm(from, to, position) / CDI_NM_PER_DOT) as f32
}

fn cdi_dots_for_guidance_geometry(geometry: &GuidanceLegGeometry, position: LatLon) -> f32 {
    let (from, to) =
        nearest_guidance_segment(geometry, position).unwrap_or((geometry.from, geometry.to));
    cdi_dots_for_leg(from, to, position)
}

fn active_course_deg(geometry: &GuidanceLegGeometry, position: LatLon) -> Option<f64> {
    nearest_guidance_segment(geometry, position).map(|(from, to)| bearing_degrees(from, to))
}

fn nearest_guidance_segment(
    geometry: &GuidanceLegGeometry,
    position: LatLon,
) -> Option<(LatLon, LatLon)> {
    let points = if geometry.path.len() >= 2 {
        geometry.path.clone()
    } else {
        vec![geometry.from, geometry.to]
    };
    let mut best: Option<(f64, LatLon, LatLon)> = None;
    for segment in points.windows(2) {
        let from = segment[0];
        let to = segment[1];
        if crate::great_circle_distance_nm(from, to) <= f64::EPSILON {
            continue;
        }
        let distance_sq = distance_sq_to_segment_nm(from, to, position);
        if best
            .as_ref()
            .map(|(best_distance_sq, _, _)| distance_sq < *best_distance_sq)
            .unwrap_or(true)
        {
            best = Some((distance_sq, from, to));
        }
    }
    best.map(|(_, from, to)| (from, to))
}

fn distance_sq_to_segment_nm(from: LatLon, to: LatLon, position: LatLon) -> f64 {
    let (from_x, from_y) = local_offset_nm(position, from);
    let (to_x, to_y) = local_offset_nm(position, to);
    let delta_x = to_x - from_x;
    let delta_y = to_y - from_y;
    let segment_length_sq = delta_x * delta_x + delta_y * delta_y;
    if segment_length_sq <= f64::EPSILON {
        return from_x * from_x + from_y * from_y;
    }
    let projection = (-(from_x * delta_x + from_y * delta_y) / segment_length_sq).clamp(0.0, 1.0);
    let nearest_x = from_x + projection * delta_x;
    let nearest_y = from_y + projection * delta_y;
    nearest_x * nearest_x + nearest_y * nearest_y
}

fn local_offset_nm(origin: LatLon, point: LatLon) -> (f64, f64) {
    let lat_nm = (point.lat - origin.lat) * 60.0;
    let lon_nm = (point.lon - origin.lon) * 60.0 * origin.lat.to_radians().cos();
    (lon_nm, lat_nm)
}

fn cdi_offscale_readout(cdi_indicator_dots: f32) -> Option<String> {
    let dots = f64::from(cdi_indicator_dots);
    if dots.abs() <= CDI_OFFSCALE_DOTS {
        return None;
    }
    let distance = dots.abs() * CDI_NM_PER_DOT;
    let distance_label = if distance >= 10.0 {
        format!("{distance:.0}nm")
    } else {
        format!("{distance:.1}nm")
    };
    if dots > 0.0 {
        Some(format!("{distance_label}\u{2192}"))
    } else {
        Some(format!("\u{2190}{distance_label}"))
    }
}

fn bearing_degrees(from: LatLon, to: LatLon) -> f64 {
    crate::initial_course_deg(from, to)
}

fn nav_ref_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(code) | NavRef::Navaid(code) | NavRef::Fix(code) => code.clone(),
        NavRef::LatLon(position) => format!("{:.4},{:.4}", position.lat, position.lon),
    }
}

fn apply_situation_to_ownship(
    session: &mut UiSession,
    source_id: &str,
    source_kind: crate::OwnshipSourceKind,
    display_name: &str,
    situation: crate::Situation,
    timestamp_epoch_ms: i64,
) -> AppResult<()> {
    let source_id = crate::OwnshipSourceId(source_id.to_string());
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::RegisterOwnshipSource(crate::OwnshipSourceRegistration {
            source_id: source_id.clone(),
            source_kind,
            display_name: display_name.to_string(),
            selectable: true,
            auto_eligible: true,
        }),
    )?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::SetOwnshipPolicy(crate::OwnshipPolicy {
            selection: crate::OwnshipSelectionPolicy::Manual {
                source_id: source_id.clone(),
            },
            source_priority: vec![source_id.clone()],
            allow_auto_replay: true,
            allow_auto_simulated: true,
        }),
    )?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::PushSituationSample(crate::SituationSample {
            source_id,
            source_kind,
            event_time_epoch_ms: timestamp_epoch_ms,
            received_time_epoch_ms: timestamp_epoch_ms,
            position: situation.position.lat_lon(),
            track_deg_true: situation.orientation_deg,
            heading_deg_true: None,
            ground_speed_kt: situation.speed_kt,
            altitude_msl_ft: situation.altitude_msl_ft,
            pressure_altitude_ft: None,
        }),
    )?;
    Ok(())
}

fn derive_compact_chart_page_state(
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> UiChartPageState {
    let mut ordered_airport_ids = Vec::new();
    for airport_id in
        compact_chart_page_airport_candidates(plan, stored_recent_airport_ids, candidate_airport_id)
    {
        if !ordered_airport_ids
            .iter()
            .any(|existing| existing == &airport_id)
        {
            ordered_airport_ids.push(airport_id);
        }
    }
    let mut recent_airport_ids = Vec::new();
    for airport_id in stored_recent_airport_ids {
        if ordered_airport_ids
            .iter()
            .any(|existing| existing == airport_id)
            && !recent_airport_ids
                .iter()
                .any(|existing| existing == airport_id)
        {
            recent_airport_ids.push(airport_id.clone());
        }
    }
    for airport_id in &ordered_airport_ids {
        if !recent_airport_ids
            .iter()
            .any(|existing| existing == airport_id)
        {
            recent_airport_ids.push(airport_id.clone());
        }
    }
    let selected_airport_id = candidate_airport_id
        .filter(|airport_id| {
            ordered_airport_ids
                .iter()
                .any(|existing| existing == *airport_id)
        })
        .map(str::to_string)
        .or_else(|| recent_airport_ids.first().cloned())
        .or_else(|| ordered_airport_ids.first().cloned())
        .unwrap_or_default();
    UiChartPageState {
        ordered_airport_ids,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id: candidate_chart_id.unwrap_or_default().to_string(),
    }
}

fn compact_chart_page_airport_candidates(
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
) -> Vec<String> {
    let mut airport_ids = Vec::new();
    if let Some(candidate_airport_id) = candidate_airport_id
        .map(str::trim)
        .filter(|airport_id| !airport_id.is_empty())
    {
        airport_ids.push(candidate_airport_id.to_ascii_uppercase());
    }
    for airport_id in stored_recent_airport_ids {
        let airport_id = airport_id.trim();
        if !airport_id.is_empty() {
            airport_ids.push(airport_id.to_ascii_uppercase());
        }
    }
    airport_ids.extend(airport_ids_from_plan(plan));

    let mut unique_airport_ids = Vec::new();
    for airport_id in airport_ids {
        if !unique_airport_ids
            .iter()
            .any(|existing| existing == &airport_id)
        {
            unique_airport_ids.push(airport_id);
        }
    }
    unique_airport_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AirportId, FlightPlan, GuidanceState, NavRef, OwnshipSourceId, OwnshipSourceKind,
        ResolvedLeg, ResolvedLegSource, RouteComponent, SequencingMode, SituationSample,
    };

    fn minimal_vector_manifest_json() -> &'static str {
        r#"{
            "point_layers": {},
            "airspace": {
                "reference_tile_min_zoom": 0,
                "reference_tile_max_zoom": 0,
                "label_tile_min_zoom": 0,
                "label_tile_max_zoom": 0
            }
        }"#
    }

    fn sample_guided_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KPAO VPDUB KVCB".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("VPDUB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KVCB".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KPAO".to_string()),
                    to: NavRef::Fix("VPDUB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Fix("VPDUB".to_string()),
                    to: NavRef::Airport("KVCB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KPAO".to_string())),
            destination: Some(AirportId("KVCB".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn straight_leg_cdi_is_signed_against_course_direction() {
        let from = LatLon {
            lat: 47.0,
            lon: -122.0,
        };
        let to = LatLon {
            lat: 48.0,
            lon: -122.0,
        };
        let right_of_course = LatLon {
            lat: 47.5,
            lon: -121.985,
        };
        let left_of_course = LatLon {
            lat: 47.5,
            lon: -122.015,
        };

        assert!(cdi_dots_for_leg(from, to, right_of_course) < 0.0);
        assert!(cdi_dots_for_leg(from, to, left_of_course) > 0.0);
    }

    #[test]
    fn cdi_offscale_readout_reports_core_owned_distance() {
        assert_eq!(cdi_offscale_readout(2.0), None);
        assert_eq!(
            cdi_offscale_readout(6.84),
            Some("6.8nm\u{2192}".to_string())
        );
        assert_eq!(
            cdi_offscale_readout(-6.84),
            Some("\u{2190}6.8nm".to_string())
        );
        assert_eq!(
            cdi_offscale_readout(9.95),
            Some("9.9nm\u{2192}".to_string())
        );
        assert_eq!(cdi_offscale_readout(10.0), Some("10nm\u{2192}".to_string()));
        assert_eq!(cdi_offscale_readout(11.4), Some("11nm\u{2192}".to_string()));
    }

    #[test]
    fn session_projects_cdi_from_injected_guidance_geometry() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            sample_guided_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        let after_geometry = set_guidance_leg_geometry_in_session(
            init.handle,
            vec![GuidanceLegGeometry {
                leg_id: "component-0-1#0".to_string(),
                from: LatLon {
                    lat: 37.461_111,
                    lon: -122.115_056,
                },
                to: LatLon {
                    lat: 38.377_625,
                    lon: -121.958_806,
                },
                path: vec![
                    LatLon {
                        lat: 37.461_111,
                        lon: -122.115_056,
                    },
                    LatLon {
                        lat: 38.377_625,
                        lon: -121.958_806,
                    },
                ],
            }],
        )
        .expect("set geometry");
        assert_eq!(
            after_geometry
                .app_ui_state
                .active_plan
                .as_ref()
                .and_then(|plan| plan.guidance.as_ref())
                .and_then(|guidance| guidance.nav_element.cdi_indicator_dots),
            None
        );
        let after_position = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 37.6,
                    lon: -122.05,
                }),
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
            },
        )
        .expect("push sample");
        let dots = after_position
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .and_then(|guidance| guidance.nav_element.cdi_indicator_dots);
        assert!(dots.is_some(), "expected CDI dots after ownship update");
    }

    #[test]
    fn remove_top_level_waypoint_by_nav_ref_in_session_updates_plan() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            sample_guided_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");

        let after_remove = remove_top_level_waypoint_by_nav_ref_in_session(
            init.handle,
            NavRef::Fix("VPDUB".to_string()),
        )
        .expect("remove waypoint");
        let active_plan = after_remove
            .app_state
            .active_plan
            .as_ref()
            .expect("active plan");

        assert_eq!(active_plan.route_components.len(), 2);
        assert!(!crate::flight_plan_contains_nav_ref(
            active_plan,
            &NavRef::Fix("VPDUB".to_string())
        ));
    }

    #[test]
    fn straight_leg_bearing_uses_geographic_course() {
        let from = LatLon {
            lat: 47.0,
            lon: -122.0,
        };
        let north = LatLon {
            lat: 48.0,
            lon: -122.0,
        };
        let east = LatLon {
            lat: 47.0,
            lon: -121.0,
        };

        assert!((bearing_degrees(from, north) - 0.0).abs() < 0.1);
        assert!((bearing_degrees(from, east) - 89.6).abs() < 0.5);
    }
}
