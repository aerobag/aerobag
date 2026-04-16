use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    derive_chart_page_state_from_catalog, load_catalog, move_flight_plan_waypoint, remove_flight_plan_leg,
    map_follow::{MapFollowSessionState, MapFollowUiState},
    playback::PlaybackSessionState, query_map_overlay, state, AppError, AppErrorKind, AppEvent, AppResult, AppState, AppUiState,
    CatalogHandle, DerivedChartCatalog, DerivedChartPageState, FlightPlan, MapOverlayQueryResult,
    MapViewport, PlaybackUiState, PointTilePayload, UiSnapshotAppState,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiChartPageState {
    pub ordered_airport_ids: Vec<String>,
    pub recent_airport_ids: Vec<String>,
    pub selected_airport_id: String,
    pub selected_chart_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionSnapshot {
    pub app_state: UiSnapshotAppState,
    pub app_ui_state: AppUiState,
    pub playback_ui_state: PlaybackUiState,
    pub map_follow_ui_state: MapFollowUiState,
    pub map_follow_target_viewport: Option<MapViewport>,
    pub chart_page_state: UiChartPageState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionInitResult {
    pub handle: u32,
    pub chart_catalog: DerivedChartCatalog,
    pub snapshot: UiSessionSnapshot,
}

struct UiSession {
    catalog: CatalogHandle,
    chart_catalog: DerivedChartCatalog,
    app_state: AppState,
    playback: PlaybackSessionState,
    map_follow: MapFollowSessionState,
    chart_page_state: DerivedChartPageState,
    point_tile_cache: HashMap<String, PointTilePayload>,
}

const DIRECT_SITUATION_SOURCE_ID: &str = "__direct_situation__";
const PLAYBACK_SOURCE_ID: &str = "__playback_trace__";

static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<u32, UiSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<u32, UiSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn create_ui_session(
    catalog_json: &str,
    chart_catalog_json: &str,
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
) -> AppResult<UiSessionInitResult> {
    let catalog = load_catalog(catalog_json)?;
    let chart_catalog: DerivedChartCatalog =
        serde_json::from_str(chart_catalog_json).map_err(|err| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("failed to parse chart catalog json: {err}"),
        })?;
    let app_state = state::reduce(&AppState::default(), AppEvent::ReplaceFlightPlan(plan.clone()), &catalog)?;
    let chart_page_state = derive_chart_page_state_from_catalog(
        &chart_catalog,
        &plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    );
    let playback = PlaybackSessionState::default();
    let map_follow = MapFollowSessionState::default();
    let snapshot = UiSessionSnapshot {
        app_state: state::project_ui_snapshot_app_state(&app_state),
        app_ui_state: state::project_app_ui_state(&app_state),
        playback_ui_state: playback.ui_state(),
        map_follow_ui_state: map_follow.ui_state(&app_state.ownship.render),
        map_follow_target_viewport: map_follow.target_viewport(&app_state.ownship.render),
        chart_page_state: compact_chart_page_state(&chart_page_state),
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    sessions().lock().expect("session store poisoned").insert(
        handle,
        UiSession {
            catalog,
            chart_catalog: chart_catalog.clone(),
            app_state,
            playback,
            map_follow,
            chart_page_state,
            point_tile_cache: HashMap::new(),
        },
    );
    Ok(UiSessionInitResult {
        handle,
        chart_catalog: chart_catalog.clone(),
        snapshot,
    })
}

pub fn remove_leg_in_session(handle: u32, index: usize) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = remove_flight_plan_leg(&plan, index)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::ReplaceFlightPlan(next_plan.clone()),
        &session.catalog,
    )?;
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
        &next_plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
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
        &session.catalog,
    )?;
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
        &next_plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
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
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
        &plan,
        &recent_airport_ids,
        Some(airport_id),
        None,
    );
    Ok(snapshot_for_session(session))
}

pub fn select_chart_in_session(handle: u32, chart_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
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
        &session.catalog,
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
        &session.catalog,
    )?;
    Ok(snapshot_for_session(session))
}

pub fn push_situation_sample_in_session(
    handle: u32,
    sample: crate::SituationSample,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::PushSituationSample(sample),
        &session.catalog,
    )?;
    Ok(snapshot_for_session(session))
}

pub fn set_ownship_policy_in_session(
    handle: u32,
    policy: crate::OwnshipPolicy,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::SetOwnshipPolicy(policy),
        &session.catalog,
    )?;
    Ok(snapshot_for_session(session))
}

pub fn select_ownship_source_in_session(
    handle: u32,
    selection: crate::OwnshipSelectionCommand,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::SelectOwnshipSource(selection),
        &session.catalog,
    )?;
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
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::ReplaceFlightPlan(plan.clone()),
        &session.catalog,
    )?;
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
        &plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
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
    session
        .map_follow
        .sync_for_viewport(&session.app_state.ownship.render, viewport, width_px, height_px);
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
    session.chart_page_state = derive_chart_page_state_from_catalog(
        &session.chart_catalog,
        &plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    );
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
        session
            .point_tile_cache
            .insert(crate::tile_key(&tile.layer, tile.z, tile.x, tile.y), tile.clone());
    }
    Ok(())
}

pub fn get_map_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<MapOverlayQueryResult> {
    let sessions = sessions().lock().expect("session store poisoned");
    let session = session_ref(&sessions, handle)?;
    Ok(query_map_overlay(
        &viewport,
        width_px,
        height_px,
        &session.point_tile_cache,
    ))
}

pub fn destroy_session(handle: u32) {
    let _ = sessions().lock().expect("session store poisoned").remove(&handle);
}

fn session_ref(
    sessions: &HashMap<u32, UiSession>,
    handle: u32,
) -> AppResult<&UiSession> {
    sessions.get(&handle).ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: format!("invalid ui session handle: {handle}"),
    })
}

fn session_mut(
    sessions: &mut HashMap<u32, UiSession>,
    handle: u32,
) -> AppResult<&mut UiSession> {
    sessions.get_mut(&handle).ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: format!("invalid ui session handle: {handle}"),
    })
}

fn session_plan(session: &UiSession) -> AppResult<FlightPlan> {
    session.app_state.active_plan.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::Internal,
        message: "session missing active plan".to_string(),
    })
}

fn snapshot_for_session(session: &UiSession) -> UiSessionSnapshot {
    UiSessionSnapshot {
        app_state: state::project_ui_snapshot_app_state(&session.app_state),
        app_ui_state: state::project_app_ui_state(&session.app_state),
        playback_ui_state: session.playback.ui_state(),
        map_follow_ui_state: session.map_follow.ui_state(&session.app_state.ownship.render),
        map_follow_target_viewport: session.map_follow.target_viewport(&session.app_state.ownship.render),
        chart_page_state: compact_chart_page_state(&session.chart_page_state),
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
        &session.catalog,
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
        &session.catalog,
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
            altitude_msl_ft: None,
            pressure_altitude_ft: None,
        }),
        &session.catalog,
    )?;
    Ok(())
}

fn compact_chart_page_state(state: &DerivedChartPageState) -> UiChartPageState {
    UiChartPageState {
        ordered_airport_ids: state.airports.iter().map(|airport| airport.id.clone()).collect(),
        recent_airport_ids: state.recent_airport_ids.clone(),
        selected_airport_id: state.selected_airport_id.clone(),
        selected_chart_id: state.selected_chart_id.clone(),
    }
}
