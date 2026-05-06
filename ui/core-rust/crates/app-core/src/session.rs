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
    guidance_detail_id_for_index, guidance_detail_id_for_leg_element,
    had_ops::{
        insert_waypoint_best_position, materialize_airway_presentation_selection,
        materialize_procedure, suggest_waypoint_identifiers, HadOperationOutcome, HadReadError,
    },
    map_follow::{MapFollowSessionState, MapFollowUiState},
    map_overlay_config_from_vector_manifest_json, nav_kv_key_for_query,
    planning::NavElementUiView,
    playback::PlaybackSessionState,
    query_map_overlay, query_map_selection, state, AirportPlateAvailability,
    AirspaceFeaturePayload, AirspaceLabelTilePayload, AirspaceReferenceTilePayload,
    AirwayPresentationPlan, AppError, AppErrorKind, AppEvent, AppResult, AppState, AppUiState,
    FlightPlan, FlightPlanRowActionExecution, FlightPlanRowActionId, LatLon, MapOverlayConfig,
    MapOverlayQueryResult, MapSelectionSessionAction, MapViewport, MetarProductPayload,
    MetarTilePayload, NavKvLookup, NavKvQuery, NavKvStore, NavRef, PlanLeg, PlaybackUiState,
    PointTilePayload, ProcedureKind, ProcedureLoadCommand, RasterMapCatalog, RasterTilePlan,
    ResolvedLeg, ResolvedLegSource, RouteComponentViewKind, SequencingMode, SituationControlInput,
    TafProductPayload, TerrainOverlayQueryResult, TfrProductPayload, UiSnapshotAppState,
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
    plan_preview: PlanPreviewState,
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
    taf_payload: Option<TafProductPayload>,
    airspace_ref_tile_cache: HashMap<String, AirspaceReferenceTilePayload>,
    airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    airspace_label_tile_cache: HashMap<String, AirspaceLabelTilePayload>,
    tfr_payload: Option<TfrProductPayload>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct PlanPreviewState {
    pointer: Option<PlanPreviewPointer>,
}

#[derive(Debug, Clone, PartialEq)]
struct PlanPreviewPointer {
    row_uid: String,
    offset_nm: f64,
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
    let app_state = register_replay_source(app_state)?;
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
    let snapshot_debug_state = debug_state_for_app_state(&debug_state, &app_state);
    let snapshot = UiSessionSnapshot {
        app_state: snapshot_app_state,
        app_ui_state,
        playback_ui_state,
        map_follow_ui_state,
        map_follow_target_viewport,
        chart_page_state: chart_page_state.clone(),
        map_layer_state: map_layer_state.clone(),
        caution_state: caution_state.clone(),
        debug_state: snapshot_debug_state,
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    sessions().lock().expect("session store poisoned").insert(
        handle,
        UiSession {
            app_state,
            playback,
            plan_preview: PlanPreviewState::default(),
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
            taf_payload: None,
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
    if session.app_state.ownship.resolved.active_source_kind
        == Some(crate::OwnshipSourceKind::FlightPlanSimulator)
        && session.plan_preview.pointer.is_none()
    {
        sync_plan_preview_to_active_leg(session)?;
    }
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
    let selected_source_kind = match &selection {
        crate::OwnshipSelectionCommand::Source { source_id } => session
            .app_state
            .ownship
            .sources
            .iter()
            .find(|source| source.source_id == *source_id)
            .map(|source| source.source_kind),
        crate::OwnshipSelectionCommand::Auto => None,
    };
    session.app_state =
        state::reduce(&session.app_state, AppEvent::SelectOwnshipSource(selection))?;
    if selected_source_kind == Some(crate::OwnshipSourceKind::FlightPlanSimulator) {
        sync_plan_preview_to_active_leg(session)?;
    }
    Ok(snapshot_for_session(session))
}

fn is_replay_ownship_source(kind: crate::OwnshipSourceKind) -> bool {
    matches!(
        kind,
        crate::OwnshipSourceKind::GpxPlayback
            | crate::OwnshipSourceKind::AdsbTrackPlayback
            | crate::OwnshipSourceKind::LiveNetworkTrack
    )
}

pub fn apply_situation_control_input_in_session(
    handle: u32,
    input: SituationControlInput,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let active_source_kind = session.app_state.ownship.resolved.active_source_kind;
    match active_source_kind {
        Some(crate::OwnshipSourceKind::FlightPlanSimulator) => {
            apply_plan_preview_input(session, input)?;
        }
        Some(
            crate::OwnshipSourceKind::GpxPlayback
            | crate::OwnshipSourceKind::AdsbTrackPlayback
            | crate::OwnshipSourceKind::LiveNetworkTrack,
        ) => {
            let delta_seconds = match input {
                SituationControlInput::SkipBackward => -600.0,
                SituationControlInput::FastRewind => -30.0,
                SituationControlInput::FastForward => 30.0,
                SituationControlInput::SkipForward => 600.0,
            };
            if let Some(situation) = session.playback.jog(delta_seconds, now_epoch_ms) {
                apply_situation_to_ownship(
                    session,
                    PLAYBACK_SOURCE_ID,
                    crate::OwnshipSourceKind::AdsbTrackPlayback,
                    "ADS-B Trace Playback",
                    situation,
                    now_epoch_ms as i64,
                )?;
            }
        }
        _ => {}
    }
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
        crate::OwnshipSourceKind::FlightPlanSimulator,
        "Plan Preview",
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

pub fn activate_next_leg_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::activate_next_leg)
}

pub fn suspend_sequencing_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::suspend_sequencing)
}

pub fn unsuspend_sequencing_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::unsuspend_sequencing)
}

pub fn sequence_active_leg_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::sequence_active_leg)
}

fn mutate_session_flight_plan(
    handle: u32,
    mutation: impl FnOnce(&FlightPlan) -> AppResult<FlightPlan>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = mutation(&plan)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn perform_map_selection_action_in_session(
    handle: u32,
    action_json: String,
) -> AppResult<HadOperationOutcome> {
    let action: MapSelectionSessionAction =
        serde_json::from_str(&action_json).map_err(|err| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("invalid map selection session action: {err}"),
        })?;
    match action {
        MapSelectionSessionAction::InsertWaypointBestPosition { nav_ref } => {
            insert_waypoint_best_position_for_session(handle, nav_ref)
        }
        MapSelectionSessionAction::ActivateDirectToNavRef { nav_ref } => {
            let snapshot = activate_direct_to_nav_ref_in_session(handle, nav_ref)?;
            Ok(HadOperationOutcome::Complete {
                result: serde_json::to_value(snapshot).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
            })
        }
    }
}

fn insert_waypoint_best_position_for_session(
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

pub fn insert_waypoint_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    before: bool,
    waypoint: NavRef,
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
            message: format!("flight-plan insert target is stale: {row_uid}"),
        })?;
    let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "flight-plan insert row has no route component uid".to_string(),
    })?;
    let component_index = plan
        .route_component_uids
        .iter()
        .position(|uid| uid == component_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight-plan insert target component is stale: {component_uid}"),
        })?;
    let next_plan = crate::insert_waypoint(&plan, component_index, before, waypoint)?;
    replace_session_flight_plan(session, next_plan)?;
    Ok(snapshot_for_session(session))
}

pub fn suggest_waypoint_identifiers_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    before: bool,
    prefix: String,
    limit: usize,
) -> AppResult<HadOperationOutcome> {
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
            message: format!("flight-plan waypoint suggestion target is stale: {row_uid}"),
        })?;
    let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "flight-plan waypoint suggestion row has no route component uid".to_string(),
    })?;
    let component_index = plan
        .route_component_uids
        .iter()
        .position(|uid| uid == component_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight-plan waypoint suggestion component is stale: {component_uid}"),
        })?;
    let store = session_nav_kv_store(session)?;
    let suggestions =
        match suggest_waypoint_identifiers(store, &plan, component_index, before, &prefix, limit) {
            Ok(suggestions) => suggestions,
            Err(HadReadError::NeedPages(pages)) => {
                return Ok(HadOperationOutcome::NeedPages { pages })
            }
            Err(HadReadError::Fatal(message)) => {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message,
                });
            }
        };
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(suggestions).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

pub fn insert_airway_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    presentation: AirwayPresentationPlan,
    entry_index: usize,
    exit_index: usize,
) -> AppResult<HadOperationOutcome> {
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
            message: format!("flight-plan airway insert target is stale: {row_uid}"),
        })?;
    let origin_anchor = row.origin_anchor.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "airway insert row has no origin anchor".to_string(),
    })?;
    let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "airway insert row has no route component uid".to_string(),
    })?;
    let start_component_index = plan
        .route_component_uids
        .iter()
        .position(|uid| uid == component_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("airway insert target component is stale: {component_uid}"),
        })?;
    let end_component_index = if row.destination_anchor.is_some() {
        Some(start_component_index + 1)
    } else {
        None
    };
    let store = session_nav_kv_store(session)?;
    let materialized = match materialize_airway_presentation_selection(
        store,
        start_component_index,
        presentation,
        entry_index,
        exit_index,
        &origin_anchor,
        row.destination_anchor.as_ref(),
    ) {
        Ok(materialized) => materialized,
        Err(HadReadError::NeedPages(pages)) => return Ok(HadOperationOutcome::NeedPages { pages }),
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    let mutation = crate::insert_airway_materialized_ui(
        &plan,
        start_component_index,
        end_component_index,
        materialized.selection,
        materialized.airway,
        materialized.resolved_legs,
    )?;
    replace_session_flight_plan(session, mutation.mutation.plan)?;
    let snapshot = snapshot_for_session(session);
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(snapshot).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

pub fn select_procedure_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    airport_id: String,
    procedure_id: String,
    kind: ProcedureKind,
    runway_transition: Option<String>,
    enroute_transition: Option<String>,
) -> AppResult<HadOperationOutcome> {
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
            message: format!("flight-plan procedure target is stale: {row_uid}"),
        })?;
    let row_airport_id = row.chart_airport_id.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "procedure target row has no airport".to_string(),
    })?;
    if row_airport_id != airport_id {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "procedure airport mismatch: row has {row_airport_id}, requested {airport_id}"
            ),
        });
    }
    let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "procedure target row has no route component uid".to_string(),
    })?;
    let airport_component_index = plan
        .route_component_uids
        .iter()
        .position(|uid| uid == component_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("procedure target component is stale: {component_uid}"),
        })?;
    let replace_component_index =
        airport_component_index.checked_sub(1).and_then(|index| {
            match plan.route_components.get(index) {
                Some(crate::RouteComponent::Procedure { procedure })
                    if procedure.kind == kind && procedure.airport_id.0 == airport_id =>
                {
                    Some(index)
                }
                _ => None,
            }
        });
    let start_component_index = airport_component_index
        .checked_sub(1)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "procedure insert target has no preceding component".to_string(),
        })?;
    let store = session_nav_kv_store(session)?;
    let built = match materialize_procedure(
        store,
        &airport_id,
        &procedure_id,
        kind,
        runway_transition.as_deref(),
        enroute_transition.as_deref(),
        airport_component_index,
    ) {
        Ok(built) => built,
        Err(HadReadError::NeedPages(pages)) => return Ok(HadOperationOutcome::NeedPages { pages }),
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    let mutation = if let Some(replace_component_index) = replace_component_index {
        crate::replace_procedure_materialized_ui(&plan, replace_component_index, built)?
    } else {
        crate::insert_procedure_materialized_ui(
            &plan,
            start_component_index,
            airport_component_index,
            built,
        )?
    };
    replace_session_flight_plan(session, mutation.mutation.plan)?;
    let snapshot = snapshot_for_session(session);
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(snapshot).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

pub fn load_plate_procedure_in_session(
    handle: u32,
    load_id: String,
) -> AppResult<HadOperationOutcome> {
    let command: ProcedureLoadCommand = serde_json::from_str(&load_id).map_err(|err| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("invalid procedure load id: {err}"),
    })?;
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let ui = crate::project_ui_state(&plan);
    let row = ui
        .display_rows
        .iter()
        .find(|row| row.uid == command.row_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("procedure load target row is stale: {}", command.row_uid),
        })?;
    let row_airport_id = row.chart_airport_id.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "procedure load target row has no airport".to_string(),
    })?;
    if row_airport_id != command.airport_id {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "procedure load airport mismatch: row has {row_airport_id}, requested {}",
                command.airport_id
            ),
        });
    }
    let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "procedure load target row has no route component uid".to_string(),
    })?;
    let airport_component_index = plan
        .route_component_uids
        .iter()
        .position(|uid| uid == component_uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("procedure load target component is stale: {component_uid}"),
        })?;
    let replace_component_index =
        airport_component_index.checked_sub(1).and_then(|index| {
            match plan.route_components.get(index) {
                Some(crate::RouteComponent::Procedure { procedure })
                    if procedure.kind == command.kind
                        && procedure.airport_id.0.trim() == command.airport_id.trim() =>
                {
                    Some(index)
                }
                _ => None,
            }
        });
    let start_component_index = airport_component_index
        .checked_sub(1)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "procedure load target has no preceding component".to_string(),
        })?;
    let store = session_nav_kv_store(session)?;
    let built = match materialize_procedure(
        store,
        &command.airport_id,
        &command.procedure_id,
        command.kind,
        command.runway_transition.as_deref(),
        command.enroute_transition.as_deref(),
        airport_component_index,
    ) {
        Ok(built) => built,
        Err(HadReadError::NeedPages(pages)) => return Ok(HadOperationOutcome::NeedPages { pages }),
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    let mutation = if let Some(replace_component_index) = replace_component_index {
        crate::replace_procedure_materialized_ui(&plan, replace_component_index, built)?
    } else {
        crate::insert_procedure_materialized_ui(
            &plan,
            start_component_index,
            airport_component_index,
            built,
        )?
    };
    replace_session_flight_plan(session, mutation.mutation.plan)?;
    let snapshot = snapshot_for_session(session);
    Ok(HadOperationOutcome::Complete {
        result: serde_json::to_value(snapshot).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    })
}

fn activate_direct_to_nav_ref_in_session(
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
    let row_component_index = || -> AppResult<usize> {
        let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight-plan row has no route component uid".to_string(),
        })?;
        plan.route_component_uids
            .iter()
            .position(|uid| uid == component_uid)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!("flight-plan row component is stale: {component_uid}"),
            })
    };

    let next_plan = match &action.id {
        FlightPlanRowActionId::ActivateLeg => {
            let leg_index = row.leg_index.ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "activate-leg row has no leg index".to_string(),
            })?;
            crate::activate_leg(&plan, leg_index)?
        }
        FlightPlanRowActionId::DirectTo => {
            let from_position =
                session
                    .app_state
                    .ownship
                    .render
                    .position
                    .ok_or_else(|| AppError {
                        kind: AppErrorKind::UnsupportedOperation,
                        message: "cannot activate direct-to without ownship position".to_string(),
                    })?;
            if row.component_kind == Some(RouteComponentViewKind::Waypoint)
                && row.component_uid.is_some()
            {
                crate::activate_direct_to_component(&plan, from_position, row_component_index()?)?
            } else if let Some(target_leg_index) = row.leg_index {
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
        FlightPlanRowActionId::Remove
        | FlightPlanRowActionId::RemoveAirway
        | FlightPlanRowActionId::RemoveProcedure => {
            crate::delete_component(&plan, row_component_index()?)?
        }
        FlightPlanRowActionId::RemoveAllAbove => {
            crate::remove_all_above(&plan, row_component_index()?)?
        }
        FlightPlanRowActionId::MoveUp => crate::move_component(&plan, row_component_index()?, -1)?,
        FlightPlanRowActionId::MoveDown => crate::move_component(&plan, row_component_index()?, 1)?,
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
            crate::tile_key("metars", tile.z, tile.x, tile.y),
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

pub fn ingest_tafs_in_session(handle: u32, payload: &TafProductPayload) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.taf_payload = Some(payload.clone());
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
        session.taf_payload.as_ref(),
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
    let debug_state = debug_state_for_app_state(&session.debug_state, &session.app_state);
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
        debug_state,
    }
}

fn debug_state_for_app_state(debug_state: &UiDebugState, app_state: &AppState) -> UiDebugState {
    let mut next = debug_state.clone();
    next.playback_visible =
        selected_ownship_source_kind(&app_state.ownship).is_some_and(is_replay_ownship_source);
    next
}

fn selected_ownship_source_kind(ownship: &crate::OwnshipState) -> Option<crate::OwnshipSourceKind> {
    match &ownship.policy.selection {
        crate::OwnshipSelectionPolicy::Manual { source_id } => ownship
            .sources
            .iter()
            .find(|source| source.source_id == *source_id)
            .map(|source| source.source_kind),
        crate::OwnshipSelectionPolicy::Auto => ownship.resolved.active_source_kind,
    }
}

fn register_replay_source(app_state: AppState) -> AppResult<AppState> {
    state::reduce(
        &app_state,
        AppEvent::RegisterOwnshipSource(crate::OwnshipSourceRegistration {
            source_id: crate::OwnshipSourceId(PLAYBACK_SOURCE_ID.to_string()),
            source_kind: crate::OwnshipSourceKind::AdsbTrackPlayback,
            display_name: "Replay".to_string(),
            selectable: true,
            auto_eligible: false,
        }),
    )
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
        visible_pireps: Vec::new(),
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

#[derive(Debug, Clone)]
struct PlanPreviewLeg {
    from_row_uid: String,
    geometry: GuidanceLegGeometry,
    distance_nm: f64,
}

fn sync_plan_preview_to_active_leg(session: &mut UiSession) -> AppResult<()> {
    let Some(plan) = session.app_state.active_plan.as_ref() else {
        return Ok(());
    };
    let leg_index = plan
        .guidance
        .as_ref()
        .map(|guidance| guidance.active_leg_index)
        .unwrap_or(0);
    let Some(record) = plan_preview_legs(plan, &session.guidance_leg_geometry)
        .get(leg_index)
        .cloned()
    else {
        return Ok(());
    };
    session.plan_preview.pointer = Some(PlanPreviewPointer {
        row_uid: record.from_row_uid.clone(),
        offset_nm: 0.0,
    });
    apply_plan_preview_pointer(session, record, 0.0)
}

fn apply_plan_preview_input(
    session: &mut UiSession,
    input: SituationControlInput,
) -> AppResult<()> {
    let records = session
        .app_state
        .active_plan
        .as_ref()
        .map(|plan| plan_preview_legs(plan, &session.guidance_leg_geometry))
        .unwrap_or_default();
    if records.is_empty() {
        return Ok(());
    }
    let pointer_on_plan = session
        .plan_preview
        .pointer
        .as_ref()
        .is_some_and(|pointer| {
            records
                .iter()
                .any(|record| record.from_row_uid == pointer.row_uid)
        });
    if !pointer_on_plan
        && matches!(
            input,
            SituationControlInput::SkipBackward | SituationControlInput::SkipForward
        )
    {
        let record = records[0].clone();
        session.plan_preview.pointer = Some(PlanPreviewPointer {
            row_uid: record.from_row_uid.clone(),
            offset_nm: 0.0,
        });
        return apply_plan_preview_pointer(session, record, 0.0);
    }
    let (record_index, mut offset_nm) =
        resolve_plan_preview_pointer(&session.plan_preview, &records);
    let mut next_index = record_index;
    let distance_nm = records[record_index].distance_nm;
    match input {
        SituationControlInput::SkipBackward => {
            if offset_nm > 1e-6 {
                offset_nm = 0.0;
            } else if record_index > 0 {
                next_index = record_index - 1;
                offset_nm = 0.0;
            }
        }
        SituationControlInput::SkipForward => {
            if offset_nm < distance_nm - 1e-6 {
                offset_nm = distance_nm;
            } else if record_index + 1 < records.len() {
                next_index = record_index + 1;
                offset_nm = records[next_index].distance_nm;
            }
        }
        SituationControlInput::FastRewind => {
            if offset_nm > 1e-6 {
                offset_nm = (offset_nm - PLAN_PREVIEW_FAST_STEP_NM).max(0.0);
            } else if record_index > 0 {
                next_index = record_index - 1;
                offset_nm = (records[next_index].distance_nm - PLAN_PREVIEW_FAST_STEP_NM).max(0.0);
            }
        }
        SituationControlInput::FastForward => {
            if offset_nm < distance_nm - 1e-6 {
                offset_nm = (offset_nm + PLAN_PREVIEW_FAST_STEP_NM).min(distance_nm);
            } else if record_index + 1 < records.len() {
                next_index = record_index + 1;
                offset_nm = PLAN_PREVIEW_FAST_STEP_NM.min(records[next_index].distance_nm);
            }
        }
    }
    let record = records[next_index].clone();
    session.plan_preview.pointer = Some(PlanPreviewPointer {
        row_uid: record.from_row_uid.clone(),
        offset_nm,
    });
    apply_plan_preview_pointer(session, record, offset_nm)
}

const PLAN_PREVIEW_FAST_STEP_NM: f64 = 20.0;

fn resolve_plan_preview_pointer(
    state: &PlanPreviewState,
    records: &[PlanPreviewLeg],
) -> (usize, f64) {
    let Some(pointer) = state.pointer.as_ref() else {
        return (0, 0.0);
    };
    records
        .iter()
        .position(|record| record.from_row_uid == pointer.row_uid)
        .map(|index| {
            (
                index,
                pointer.offset_nm.clamp(0.0, records[index].distance_nm),
            )
        })
        .unwrap_or((0, 0.0))
}

fn apply_plan_preview_pointer(
    session: &mut UiSession,
    record: PlanPreviewLeg,
    offset_nm: f64,
) -> AppResult<()> {
    let position = position_along_geometry(&record.geometry, offset_nm);
    let heading = heading_along_geometry(&record.geometry, offset_nm)
        .unwrap_or_else(|| bearing_degrees(record.geometry.from, record.geometry.to));
    apply_situation_to_ownship(
        session,
        DIRECT_SITUATION_SOURCE_ID,
        crate::OwnshipSourceKind::FlightPlanSimulator,
        "Plan Preview",
        crate::Situation {
            position: crate::SituationPosition::LatLon {
                lat: position.lat,
                lon: position.lon,
            },
            orientation_deg: Some(heading),
            speed_kt: Some(0.0),
            altitude_msl_ft: None,
        },
        0,
    )
}

fn plan_preview_legs(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Vec<PlanPreviewLeg> {
    plan.resolved_legs
        .iter()
        .filter_map(|leg| {
            let from_row_uid = row_uid_for_leg_start(plan, leg)?;
            let geometry = geometry_for_resolved_leg(leg, geometry_by_leg_id)?;
            let distance_nm = geometry_distance_nm(&geometry);
            Some(PlanPreviewLeg {
                from_row_uid,
                geometry,
                distance_nm,
            })
        })
        .collect()
}

fn row_uid_for_leg_start(plan: &FlightPlan, leg: &ResolvedLeg) -> Option<String> {
    match leg.source {
        ResolvedLegSource::RouteComponent { component_index }
        | ResolvedLegSource::SyntheticBridge {
            from_component_index: component_index,
            ..
        } => plan.route_component_uids.get(component_index).cloned(),
        ResolvedLegSource::LegacyPlanLeg { leg_index } => Some(format!("legacy:{leg_index}:from")),
    }
}

fn geometry_for_resolved_leg(
    leg: &ResolvedLeg,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    if let Some(geometry) = geometry_by_leg_id
        .get(&guidance_detail_id_for_leg_element(leg, 0))
        .or_else(|| geometry_by_leg_id.get(&leg.id))
    {
        return Some(geometry.clone());
    }
    if let (NavRef::LatLon(from), NavRef::LatLon(to)) = (&leg.from, &leg.to) {
        return Some(GuidanceLegGeometry {
            leg_id: leg.id.clone(),
            from: *from,
            to: *to,
            path: vec![*from, *to],
        });
    }
    None
}

fn geometry_points(geometry: &GuidanceLegGeometry) -> Vec<LatLon> {
    if geometry.path.len() >= 2 {
        geometry.path.clone()
    } else {
        vec![geometry.from, geometry.to]
    }
}

fn geometry_distance_nm(geometry: &GuidanceLegGeometry) -> f64 {
    geometry_points(geometry)
        .windows(2)
        .map(|segment| crate::great_circle_distance_nm(segment[0], segment[1]))
        .sum()
}

fn position_along_geometry(geometry: &GuidanceLegGeometry, offset_nm: f64) -> LatLon {
    let points = geometry_points(geometry);
    let mut remaining_nm = offset_nm.max(0.0);
    for segment in points.windows(2) {
        let from = segment[0];
        let to = segment[1];
        let distance_nm = crate::great_circle_distance_nm(from, to);
        if distance_nm <= f64::EPSILON {
            continue;
        }
        if remaining_nm <= distance_nm {
            return crate::great_circle_intermediate(from, to, remaining_nm / distance_nm);
        }
        remaining_nm -= distance_nm;
    }
    points.last().copied().unwrap_or(geometry.to)
}

fn heading_along_geometry(geometry: &GuidanceLegGeometry, offset_nm: f64) -> Option<f64> {
    let points = geometry_points(geometry);
    let mut remaining_nm = offset_nm.max(0.0);
    for segment in points.windows(2) {
        let from = segment[0];
        let to = segment[1];
        let distance_nm = crate::great_circle_distance_nm(from, to);
        if distance_nm <= f64::EPSILON {
            continue;
        }
        if remaining_nm <= distance_nm {
            return Some(bearing_degrees(from, to));
        }
        remaining_nm -= distance_nm;
    }
    points
        .windows(2)
        .last()
        .map(|segment| bearing_degrees(segment[0], segment[1]))
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
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => {
            identifier.clone()
        }
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
            horizontal_accuracy_m: None,
            vertical_accuracy_m: None,
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
        ResolvedLeg, ResolvedLegSource, RouteComponent, SequencingMode, Situation,
        SituationPosition, SituationSample,
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
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
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

    fn sample_duplicate_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-dup".to_string(),
            name: "KRNT SEA KPAE KRNT".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Airport("KPAE".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Airport("KPAE".to_string()),
                    to: NavRef::Airport("KRNT".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
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
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRNT".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn lat_lon_preview_plan() -> FlightPlan {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.0,
        };
        let c = LatLon {
            lat: 41.0,
            lon: -119.0,
        };
        FlightPlan {
            id: "plan-preview".to_string(),
            name: "A B C".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(a),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(b),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(c),
                },
            ],
            route_component_uids: vec![
                "row-a".to_string(),
                "row-b".to_string(),
                "row-c".to_string(),
            ],
            route_component_uid_counter: 3,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "leg-a-b".to_string(),
                    from: NavRef::LatLon(a),
                    to: NavRef::LatLon(b),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-b-c".to_string(),
                    from: NavRef::LatLon(b),
                    to: NavRef::LatLon(c),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 1,
                active_detail_index: Some(1),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn short_lat_lon_preview_plan() -> FlightPlan {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.95,
        };
        let c = LatLon {
            lat: 40.05,
            lon: -119.95,
        };
        FlightPlan {
            id: "short-plan-preview".to_string(),
            name: "A B C".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(a),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(b),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(c),
                },
            ],
            route_component_uids: vec![
                "short-row-a".to_string(),
                "short-row-b".to_string(),
                "short-row-c".to_string(),
            ],
            route_component_uid_counter: 3,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "short-leg-a-b".to_string(),
                    from: NavRef::LatLon(a),
                    to: NavRef::LatLon(b),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "short-leg-b-c".to_string(),
                    from: NavRef::LatLon(b),
                    to: NavRef::LatLon(c),
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
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn select_plan_preview(handle: u32) -> UiSessionSnapshot {
        set_situation_in_session(
            handle,
            Situation {
                position: SituationPosition::LatLon { lat: 0.0, lon: 0.0 },
                orientation_deg: None,
                speed_kt: None,
                altitude_msl_ft: None,
            },
        )
        .expect("register plan preview");
        select_ownship_source_in_session(
            handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DIRECT_SITUATION_SOURCE_ID.to_string()),
            },
        )
        .expect("select plan preview")
    }

    fn ownship_position(snapshot: &UiSessionSnapshot) -> LatLon {
        snapshot
            .app_ui_state
            .ownship
            .render
            .position
            .expect("ownship position")
    }

    fn assert_near(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-4, "{left} != {right}");
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
    fn replay_source_is_selectable_and_controls_playback_panel_visibility() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            sample_guided_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        assert!(
            init.snapshot
                .app_ui_state
                .ownship
                .controls
                .sources
                .iter()
                .any(|source| {
                    source.source_id.0 == PLAYBACK_SOURCE_ID
                        && source.source_kind == OwnshipSourceKind::AdsbTrackPlayback
                        && source.label == "Replay"
                }),
            "Replay must be available in the ownship source tray",
        );
        assert!(
            !init.snapshot.debug_state.playback_visible,
            "playback panel starts hidden until Replay is active",
        );

        let replay = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId(PLAYBACK_SOURCE_ID.to_string()),
            },
        )
        .expect("select Replay");
        assert!(replay.debug_state.playback_visible);

        let gps = push_situation_sample_in_session(
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
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
            },
        )
        .expect("push gps sample");
        assert!(
            gps.debug_state.playback_visible,
            "pushing GPS must not change a manual Replay selection",
        );

        let gps = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId("test-gps".to_string()),
            },
        )
        .expect("select GPS");
        assert!(
            !gps.debug_state.playback_visible,
            "playback panel hides as soon as Replay is not the active source",
        );
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
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
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
    fn row_action_direct_to_targets_clicked_duplicate_row_uid() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            sample_duplicate_waypoint_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        let positioned = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 47.5,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
            },
        )
        .expect("push sample");
        let ui = positioned
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("plan ui");
        let clicked_row = ui
            .display_rows
            .iter()
            .find(|row| row.component_index == Some(3))
            .expect("second KRNT row");
        let direct_to_action = clicked_row
            .actions
            .iter()
            .find(|action| action.id == FlightPlanRowActionId::DirectTo)
            .expect("direct-to action");

        let after_direct_to = perform_flight_plan_row_action_in_session(
            init.handle,
            clicked_row.uid.clone(),
            direct_to_action.uid.clone(),
        )
        .expect("direct-to row action");
        let guidance = after_direct_to
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");

        assert_eq!(guidance.active_to_row_uid.as_ref(), Some(&clicked_row.uid));
        assert_eq!(guidance.active_from_row_uid, None);
        assert_eq!(
            guidance
                .direct_to
                .as_ref()
                .and_then(|direct_to| direct_to.target_component_uid.as_ref()),
            clicked_row.component_uid.as_ref()
        );
    }

    #[test]
    fn row_action_activate_leg_works_after_on_plan_direct_to() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            sample_guided_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 47.5,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
            },
        )
        .expect("push sample");

        let after_direct_to =
            activate_direct_to_nav_ref_in_session(init.handle, NavRef::Fix("VPDUB".to_string()))
                .expect("direct-to on-plan fix");
        let target_row = after_direct_to
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("plan ui")
            .display_rows
            .iter()
            .find(|row| row.nav_ref == Some(NavRef::Airport("KVCB".to_string())))
            .expect("destination row")
            .clone();
        let activate_leg = target_row
            .actions
            .iter()
            .find(|action| action.id == FlightPlanRowActionId::ActivateLeg)
            .expect("activate-leg action");
        assert!(
            target_row.enabled,
            "on-plan direct-to must not disable underlying rows"
        );
        assert!(
            activate_leg.enabled,
            "on-plan direct-to must leave activate-leg available"
        );

        let after_activate_leg = perform_flight_plan_row_action_in_session(
            init.handle,
            target_row.uid,
            activate_leg.uid.clone(),
        )
        .expect("activate leg after direct-to");
        let guidance = after_activate_leg
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");

        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert!(guidance.direct_to.is_none());
        assert_eq!(guidance.active_leg_index, 1);
    }

    #[test]
    fn plan_preview_enters_at_active_leg_start() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            lat_lon_preview_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        let snapshot = select_plan_preview(init.handle);
        let position = ownship_position(&snapshot);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.0);
    }

    #[test]
    fn plan_preview_controls_stop_at_waypoints_and_plan_ends() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            lat_lon_preview_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        select_plan_preview(init.handle);

        let after_skip_end = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip to active leg end");
        let position = ownship_position(&after_skip_end);
        assert_near(position.lat, 41.0);
        assert_near(position.lon, -119.0);

        let after_past_end = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip past end");
        assert_near(ownship_position(&after_past_end).lat, 41.0);
        assert_near(ownship_position(&after_past_end).lon, -119.0);

        let after_skip_start = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipBackward,
            0.0,
        )
        .expect("skip to active leg start");
        let position = ownship_position(&after_skip_start);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.0);

        let after_previous = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipBackward,
            0.0,
        )
        .expect("skip to previous waypoint");
        let position = ownship_position(&after_previous);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -120.0);
    }

    #[test]
    fn plan_preview_skip_from_off_plan_returns_to_first_waypoint() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            lat_lon_preview_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        select_plan_preview(init.handle);
        {
            let mut sessions = sessions().lock().expect("session store poisoned");
            session_mut(&mut sessions, init.handle)
                .expect("session")
                .plan_preview
                .pointer = Some(PlanPreviewPointer {
                row_uid: "missing-row".to_string(),
                offset_nm: 12.0,
            });
        }

        let snapshot = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip from off-plan");
        let position = ownship_position(&snapshot);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -120.0);
    }

    #[test]
    fn plan_preview_skip_forward_from_waypoint_moves_to_next_waypoint() {
        let mut plan = lat_lon_preview_plan();
        plan.guidance.as_mut().expect("guidance").active_leg_index = 0;
        let init = create_ui_session(minimal_vector_manifest_json(), plan, &[], None, None)
            .expect("create session");
        select_plan_preview(init.handle);

        let at_intermediate_waypoint = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip to intermediate waypoint");
        let position = ownship_position(&at_intermediate_waypoint);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.0);

        let next_waypoint = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip to next waypoint");
        let position = ownship_position(&next_waypoint);
        assert_near(position.lat, 41.0);
        assert_near(position.lon, -119.0);
    }

    #[test]
    fn plan_preview_fast_forward_from_waypoint_enters_next_leg() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            short_lat_lon_preview_plan(),
            &[],
            None,
            None,
        )
        .expect("create session");
        select_plan_preview(init.handle);

        let at_intermediate_waypoint = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::FastForward,
            0.0,
        )
        .expect("fast-forward to intermediate waypoint");
        let position = ownship_position(&at_intermediate_waypoint);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.95);

        let down_next_leg = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::FastForward,
            0.0,
        )
        .expect("fast-forward into next leg");
        let position = ownship_position(&down_next_leg);
        assert!(
            position.lat > 40.0 && position.lat <= 40.05,
            "expected fast-forward to move onto the outbound leg, got {position:?}",
        );
        assert_near(position.lon, -119.95);
    }

    #[test]
    fn plan_preview_pointer_follows_row_uid_after_reorder() {
        let mut plan = lat_lon_preview_plan();
        plan.guidance.as_mut().expect("guidance").active_leg_index = 0;
        let init = create_ui_session(minimal_vector_manifest_json(), plan, &[], None, None)
            .expect("create session");
        select_plan_preview(init.handle);
        apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::FastForward,
            0.0,
        )
        .expect("advance on first leg");

        let mut reordered = lat_lon_preview_plan();
        reordered.route_components.swap(0, 1);
        reordered.route_component_uids.swap(0, 1);
        reordered.resolved_legs = vec![ResolvedLeg {
            id: "leg-a-c".to_string(),
            from: NavRef::LatLon(LatLon {
                lat: 40.0,
                lon: -120.0,
            }),
            to: NavRef::LatLon(LatLon {
                lat: 41.0,
                lon: -119.0,
            }),
            source: ResolvedLegSource::RouteComponent { component_index: 1 },
            procedure_provenance: None,
        }];
        replace_flight_plan_in_session(init.handle, reordered).expect("replace reordered plan");
        let snapshot = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::FastForward,
            0.0,
        )
        .expect("advance after reorder");
        let position = ownship_position(&snapshot);
        assert!(
            position.lat > 40.0,
            "expected pointer to stay on row-a leg after reorder"
        );
        assert!(
            position.lon > -120.0,
            "expected pointer to stay on row-a leg after reorder"
        );
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
