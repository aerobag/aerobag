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
    query_map_overlay, state, AppError, AppErrorKind, AppEvent, AppResult, AppState,
    CatalogHandle, DerivedChartCatalog, DerivedChartPageState, FlightPlan, MapOverlayQueryResult,
    MapViewport, PointTilePayload,
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
    pub app_state: AppState,
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
    chart_page_state: DerivedChartPageState,
    fix_tile_cache: HashMap<String, PointTilePayload>,
}

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
    let snapshot = UiSessionSnapshot { app_state: app_state.clone(), chart_page_state: compact_chart_page_state(&chart_page_state) };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    sessions().lock().expect("session store poisoned").insert(
        handle,
        UiSession {
            catalog,
            chart_catalog: chart_catalog.clone(),
            app_state,
            chart_page_state,
            fix_tile_cache: HashMap::new(),
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

pub fn set_situation_in_session(
    handle: u32,
    situation: crate::Situation,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::SetSituation(situation),
        &session.catalog,
    )?;
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

pub fn ingest_fix_tiles_in_session(handle: u32, tiles: &[PointTilePayload]) -> AppResult<()> {
    let mut sessions = sessions().lock().expect("session store poisoned");
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session
            .fix_tile_cache
            .insert(crate::tile_key(tile.z, tile.x, tile.y), tile.clone());
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
        &session.fix_tile_cache,
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
        app_state: session.app_state.clone(),
        chart_page_state: compact_chart_page_state(&session.chart_page_state),
    }
}

fn compact_chart_page_state(state: &DerivedChartPageState) -> UiChartPageState {
    UiChartPageState {
        ordered_airport_ids: state.airports.iter().map(|airport| airport.id.clone()).collect(),
        recent_airport_ids: state.recent_airport_ids.clone(),
        selected_airport_id: state.selected_airport_id.clone(),
        selected_chart_id: state.selected_chart_id.clone(),
    }
}
