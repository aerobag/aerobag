use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use serde::{Deserialize, Serialize};

use crate::CoreResourcePolicy;
use crate::{
    chart_ident_label_for_nav_ref_symbol,
    chart_page::airport_ids_from_plan,
    data_status::{
        parse_status_action_id, project_data_status_state, DataStatusRecord, UiDataStatusState,
        UiStatusActionCommand, UiStatusSeverity,
    },
    first_guidance_detail_index_for_leg, guidance_detail_id_for_index,
    guidance_detail_id_for_leg_element,
    had_ops::{
        flight_plan_ui_state, insert_waypoint_best_position,
        materialize_airway_presentation_selection, materialize_procedure, nav_kv_page_resources,
        nav_ref_position, nav_symbol_feature, suggest_waypoint_identifiers, CoreResourceRequest,
        HadOperationOutcome, HadReadError, UiInvalidation,
    },
    live_feeds::{LiveFeedSseEvent, LiveFeedsState},
    map_follow::{MapFollowSessionState, MapFollowUiState},
    map_overlay::{FlightPlanSelectionPoint, MetarTileRecord, PointTileLayerConfig},
    map_overlay_config_from_vector_manifest_json, nav_kv_key_for_query,
    planning::NavElementUiView,
    playback::PlaybackSessionState,
    project_nav_symbol_feature, query_map_overlay_for_surface, query_map_selection_for_surface,
    state, AirportPlateAvailability, AirspaceFeaturePayload, AirspaceLabelTilePayload,
    AirspaceReferenceTilePayload, AirwayPresentationPlan, AppError, AppErrorKind, AppEvent,
    AppResult, AppState, AppUiState, FlightPlan, FlightPlanDisplayRowKind,
    FlightPlanRowActionExecution, FlightPlanRowActionId, GuidanceState, LatLon, LegDisplayElement,
    MapOverlayConfig, MapOverlayQueryResult, MapSelectionSessionAction, MapSurfaceMetrics,
    MapViewport, MetarProductPayload, MetarTilePayload, NavKvLookup, NavKvQuery, NavKvStore,
    NavRef, PlanLeg, PlaybackUiState, PointTilePayload, ProcedureDiscontinuity, ProcedureKind,
    ProcedureLoadCommand, PublicationResolver, RasterMapCatalog, RasterResourceMode,
    RasterTilePlan, ResolvedLeg, ResolvedLegSource, RouteComponentViewKind, SequencingMode,
    SituationControlInput, SituationControlMenuItem, TafProductPayload, TerrainOverlayQueryResult,
    TfrProductPayload, UiSnapshotAppState, VectorAggregateTilePayload, VectorIdentLabelStyle,
};

const WORLD_MERCATOR_MAX_LATITUDE: f64 = 85.051_128_78;

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
    pub world_basemap: UiMapLayerToggleState,
    pub vectors: UiMapLayerToggleState,
    pub metars: UiMapLayerToggleState,
    pub nexrad: UiMapLayerToggleState,
    pub terrain_warning: UiMapLayerToggleState,
    pub offline_regions: UiMapLayerToggleState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDebugState {
    pub tile_labels: bool,
    #[serde(default)]
    pub nexrad_tile_labels: bool,
    pub playback_visible: bool,
    pub fast_tiles: bool,
    pub offline_simulated_clock_buttons: bool,
    #[serde(default)]
    pub sequencing_finish_lines: bool,
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
    pub data_status_state: UiDataStatusState,
    pub debug_state: UiDebugState,
    pub raster_map: Option<crate::RasterMapUiState>,
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

#[derive(Clone)]
struct UiSession {
    app_state: AppState,
    playback: PlaybackSessionState,
    plan_preview: PlanPreviewState,
    debug_ownship_driver: DebugOwnshipDriverState,
    map_follow: MapFollowSessionState,
    guidance_leg_geometry: HashMap<String, GuidanceLegGeometry>,
    map_overlay_config: MapOverlayConfig,
    vector_manifest_loaded: bool,
    chart_page_state: UiChartPageState,
    nav_kv_store_id: Option<u32>,
    nav_kv_store: Option<NavKvStore>,
    map_layer_state: UiMapLayerState,
    data_status_records: BTreeMap<String, DataStatusRecord>,
    hushed_status_ids: BTreeSet<String>,
    data_status_state: UiDataStatusState,
    debug_state: UiDebugState,
    resource_policy: CoreResourcePolicy,
    publication_resolver: PublicationResolver,
    live_feeds: LiveFeedsState,
    raster_map_catalog: Option<RasterMapCatalog>,
    vector_tile_cache: HashMap<String, VectorAggregateTilePayload>,
    metar_tile_cache: HashMap<String, MetarTilePayload>,
    metar_payload: Option<MetarProductPayload>,
    towered_metar_station_ids: Option<HashSet<String>>,
    taf_payload: Option<TafProductPayload>,
    airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    tfr_payload: Option<TfrProductPayload>,
    terrain_source_tile_cache: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexradOverlayQueryResult {
    pub status: NexradOverlayStatus,
    #[serde(default)]
    pub tiles: Vec<NexradOverlayTile>,
    #[serde(default)]
    pub stats: NexradOverlayStats,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NexradOverlayStats {
    pub source_tile_count: usize,
    pub render_piece_count: usize,
    pub split_count: usize,
    pub max_affine_error_px: f64,
    pub level_pixel_span_px: f64,
    pub max_level_pixel_stretch_px: f64,
    pub max_stack_depth: usize,
    pub res: Option<u32>,
    pub observed_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NexradOverlayStatus {
    Hidden,
    Loading,
    Unavailable { reason: String },
    Ready { count: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexradOverlayTile {
    pub key: String,
    pub src: String,
    pub res: u32,
    pub x: u32,
    pub y: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub corners: NexradOverlayTileCorners,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexradOverlayTileCorners {
    pub nw: ScreenPoint,
    pub ne: ScreenPoint,
    pub se: ScreenPoint,
    pub sw: ScreenPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct DebugOwnshipDriverState {
    running: bool,
    active_detail_id: Option<String>,
    offset_nm: f64,
    wander_phase_rad: f64,
    last_tick_epoch_ms: Option<f64>,
    last_position: Option<LatLon>,
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
const DEBUG_OWNSHIP_DRIVER_SOURCE_ID: &str = "__debug_ownship_driver__";
const CDI_NM_PER_DOT: f64 = 1.0;
const CDI_OFFSCALE_DOTS: f64 = 2.1;
const DEBUG_OWNSHIP_DRIVER_NM_PER_SECOND: f64 = 0.36;
const DEBUG_OWNSHIP_DRIVER_REPORTED_SPEED_SCALE: f64 = 0.1;
const DEBUG_OWNSHIP_DRIVER_MAX_DT_SECONDS: f64 = 1.0;
const DEBUG_OWNSHIP_DRIVER_WANDER_NM: f64 = 0.125;
const DEBUG_OWNSHIP_DRIVER_OVERRUN_NM: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapLayerId {
    WorldBasemap,
    Vectors,
    Metars,
    Nexrad,
    TerrainWarning,
    OfflineRegions,
}

fn raster_catalog_for_layer_state(
    catalog: &RasterMapCatalog,
    layer_state: &UiMapLayerState,
) -> RasterMapCatalog {
    let mut catalog = catalog.clone();
    if !layer_state.world_basemap.visible {
        catalog
            .displayed_maps
            .retain(|view| view.map_view.chart_family != "world-basemap");
        if catalog
            .selected_map
            .as_ref()
            .is_some_and(|view| view.map_view.chart_family == "world-basemap")
        {
            catalog.selected_map = None;
        }
    }
    catalog
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

fn lock_sessions() -> MutexGuard<'static, HashMap<u32, UiSession>> {
    sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const MAP_OVERLAY_STATUS_PREFIX: &str = "map_overlay:";
const PROCEDURE_GEOMETRY_STATUS_PREFIX: &str = "procedure_geometry:";
const LIVE_FEED_METARS_STATUS_ID: &str = "live_feed:metars_unavailable";
const LIVE_FEED_NEXRAD_STATUS_ID: &str = "live_feed:nexrad_unavailable";
const LIVE_FEED_TFRS_STATUS_ID: &str = "live_feed:tfrs_unavailable";

fn sync_data_status_projection(session: &mut UiSession) {
    session.data_status_state =
        project_data_status_state(&session.data_status_records, &session.hushed_status_ids);
}

fn upsert_data_status_record(session: &mut UiSession, record: DataStatusRecord) -> bool {
    let changed = session
        .data_status_records
        .get(&record.id)
        .is_none_or(|existing| existing != &record);
    if changed {
        session
            .data_status_records
            .insert(record.id.clone(), record);
        sync_data_status_projection(session);
    }
    changed
}

fn clear_data_status_record(session: &mut UiSession, id: &str) -> bool {
    let changed = session.data_status_records.remove(id).is_some();
    if changed {
        sync_data_status_projection(session);
    }
    changed
}

fn sync_map_overlay_status_records(
    session: &mut UiSession,
    records: Vec<DataStatusRecord>,
) -> Vec<UiInvalidation> {
    let mut changed = false;
    let active_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let stale_ids = session
        .data_status_records
        .keys()
        .filter(|id| id.starts_with(MAP_OVERLAY_STATUS_PREFIX) && !active_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in stale_ids {
        changed |= session.data_status_records.remove(&id).is_some();
    }
    for record in records {
        changed |= session
            .data_status_records
            .get(&record.id)
            .is_none_or(|existing| existing != &record);
        session
            .data_status_records
            .insert(record.id.clone(), record);
    }
    if changed {
        sync_data_status_projection(session);
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn procedure_geometry_status_records_for_plan(plan: &FlightPlan) -> Vec<DataStatusRecord> {
    plan.route_components
        .iter()
        .enumerate()
        .filter_map(|(index, component)| {
            let crate::RouteComponent::Procedure { procedure } = component else {
                return None;
            };
            if procedure.data_quality.is_empty() {
                return None;
            }
            let component_id = plan
                .route_component_uids
                .get(index)
                .cloned()
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}:{index}",
                        procedure.airport_id.0, procedure.procedure_id
                    )
                });
            let transition = procedure
                .enroute_transition
                .as_deref()
                .or(procedure.runway_transition.as_deref())
                .map(|value| format!(" {value}"))
                .unwrap_or_default();
            let detail = format!(
                "Procedure geometry warning for {} {}{}:\n{}",
                procedure.airport_id.0,
                procedure.procedure_id,
                transition,
                procedure
                    .data_quality
                    .iter()
                    .map(|message| format!("- {message}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            Some(DataStatusRecord::new(
                format!("{PROCEDURE_GEOMETRY_STATUS_PREFIX}{component_id}"),
                "PROC",
                Some(procedure.procedure_id.clone()),
                UiStatusSeverity::Caution,
                true,
                detail,
            ))
        })
        .collect()
}

fn sync_procedure_geometry_status_records(session: &mut UiSession, plan: &FlightPlan) -> bool {
    let records = procedure_geometry_status_records_for_plan(plan);
    let active_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let stale_ids = session
        .data_status_records
        .keys()
        .filter(|id| id.starts_with(PROCEDURE_GEOMETRY_STATUS_PREFIX) && !active_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = false;
    for id in stale_ids {
        changed |= session.data_status_records.remove(&id).is_some();
    }
    for record in records {
        changed |= session
            .data_status_records
            .get(&record.id)
            .is_none_or(|existing| existing != &record);
        session
            .data_status_records
            .insert(record.id.clone(), record);
    }
    if changed {
        sync_data_status_projection(session);
    }
    changed
}

fn live_feed_unavailable_status_record(product: &str, detail: String) -> DataStatusRecord {
    match product {
        "metars" => DataStatusRecord::new(
            LIVE_FEED_METARS_STATUS_ID,
            "METARS",
            Some("UNAVAIL".to_string()),
            UiStatusSeverity::Unavailable,
            true,
            detail,
        ),
        "nexrad" => DataStatusRecord::new(
            LIVE_FEED_NEXRAD_STATUS_ID,
            "NEXRAD",
            Some("UNAVAIL".to_string()),
            UiStatusSeverity::Unavailable,
            true,
            detail,
        ),
        "tfrs" => DataStatusRecord::new(
            LIVE_FEED_TFRS_STATUS_ID,
            "TFRS",
            Some("UNAVAIL".to_string()),
            UiStatusSeverity::Unavailable,
            true,
            detail,
        ),
        _ => DataStatusRecord::new(
            format!("live_feed:{product}_unavailable"),
            product.to_ascii_uppercase(),
            Some("UNAVAIL".to_string()),
            UiStatusSeverity::Unavailable,
            true,
            detail,
        ),
    }
}

fn live_feed_product_from_resource_id(resource_id: &str) -> Option<&str> {
    for prefix in [
        "live_feeds/version/",
        "live_feeds/state/",
        "live_feeds/delta/",
    ] {
        if let Some(rest) = resource_id.strip_prefix(prefix) {
            return rest.split('/').next();
        }
    }
    None
}

fn record_live_feed_fetch_failure(
    session: &mut UiSession,
    resource_id: &str,
    message: &str,
) -> bool {
    if resource_id == "live_feeds/current" {
        let detail = format!("Live feed index unavailable: {message}");
        let mut changed = false;
        if session.map_layer_state.metars.visible {
            changed |= upsert_data_status_record(
                session,
                live_feed_unavailable_status_record("metars", detail.clone()),
            );
        }
        if session.map_layer_state.nexrad.visible {
            changed |= upsert_data_status_record(
                session,
                live_feed_unavailable_status_record("nexrad", detail.clone()),
            );
        }
        if session.map_layer_state.vectors.visible {
            changed |= upsert_data_status_record(
                session,
                live_feed_unavailable_status_record("tfrs", detail),
            );
        }
        return changed;
    }

    let Some(product) = live_feed_product_from_resource_id(resource_id) else {
        return false;
    };
    let label = product.to_ascii_uppercase();
    upsert_data_status_record(
        session,
        live_feed_unavailable_status_record(
            product,
            format!("{label} live feed unavailable: {message}"),
        ),
    )
}

fn sync_nexrad_status_record(
    session: &mut UiSession,
    status: &NexradOverlayStatus,
) -> Vec<UiInvalidation> {
    let changed = match status {
        NexradOverlayStatus::Hidden | NexradOverlayStatus::Ready { .. } => {
            clear_data_status_record(session, LIVE_FEED_NEXRAD_STATUS_ID)
        }
        NexradOverlayStatus::Loading => false,
        NexradOverlayStatus::Unavailable { reason } => upsert_data_status_record(
            session,
            live_feed_unavailable_status_record(
                "nexrad",
                format!("NEXRAD live feed unavailable: {reason}"),
            ),
        ),
    };
    if changed {
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn complete_nexrad_overlay_outcome(
    session: &mut UiSession,
    query: NexradOverlayQueryResult,
) -> AppResult<HadOperationOutcome> {
    let invalidations = sync_nexrad_status_record(session, &query.status);
    Ok(HadOperationOutcome::complete_with_invalidations(
        serde_json::to_value(query).map_err(internal_json_error)?,
        invalidations,
    ))
}

fn missing_active_plan_error(context: &str) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message: format!("{context} produced no active flight plan"),
    }
}

pub fn create_ui_session(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        None,
    )
}

pub fn create_ui_session_profiled(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    mark: &mut dyn FnMut(&'static str),
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        Some(mark),
    )
}

fn create_ui_session_inner(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    mut mark: Option<&mut dyn FnMut(&'static str)>,
) -> AppResult<UiSessionInitResult> {
    let map_overlay_config = uninitialized_map_overlay_config();
    let app_state = state::reduce(
        &AppState::default(),
        AppEvent::ReplaceFlightPlan(plan.clone()),
    )?;
    let app_state = register_default_situation_sources(app_state)?;
    let app_state = register_debug_ownship_driver_source(app_state)?;
    let active_plan = app_state
        .active_plan
        .clone()
        .ok_or_else(|| missing_active_plan_error("create session"))?;
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_reduce_replace_flight_plan");
    }
    let chart_page_state = derive_compact_chart_page_state(
        &active_plan,
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
    let mut data_status_records = BTreeMap::new();
    for record in procedure_geometry_status_records_for_plan(&active_plan) {
        data_status_records.insert(record.id.clone(), record);
    }
    let hushed_status_ids = BTreeSet::new();
    let data_status_state = project_data_status_state(&data_status_records, &hushed_status_ids);
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
        data_status_state: data_status_state.clone(),
        debug_state: snapshot_debug_state,
        raster_map: None,
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_sessions().insert(
        handle,
        UiSession {
            app_state,
            playback,
            plan_preview: PlanPreviewState::default(),
            debug_ownship_driver: DebugOwnshipDriverState::default(),
            map_follow,
            guidance_leg_geometry: HashMap::new(),
            map_overlay_config,
            vector_manifest_loaded: false,
            chart_page_state,
            nav_kv_store_id: None,
            nav_kv_store: None,
            map_layer_state,
            data_status_records,
            hushed_status_ids,
            data_status_state,
            debug_state,
            resource_policy: CoreResourcePolicy::InstalledPackage,
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            live_feeds: LiveFeedsState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            towered_metar_station_ids: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
        },
    );
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_store_session");
    }
    Ok(UiSessionInitResult { handle, snapshot })
}

fn uninitialized_map_overlay_config() -> MapOverlayConfig {
    let empty_point_layer = PointTileLayerConfig {
        min_zoom: 0,
        max_zoom: 0,
        available_zooms: Vec::new(),
        tile_path_template: None,
    };
    MapOverlayConfig {
        airspace_reference_tile_min_zoom: 0,
        airspace_reference_tile_max_zoom: 0,
        airspace_label_tile_min_zoom: 0,
        airspace_label_tile_max_zoom: 0,
        airport_layer: empty_point_layer.clone(),
        fix_layer: empty_point_layer.clone(),
        nav_layer: empty_point_layer,
        obstacle_layer: None,
        metar_layer: None,
    }
}

pub fn set_map_layer_visibility_in_session(
    handle: u32,
    layer_id: &str,
    visible: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    map_layer_toggle_mut(&mut session.map_layer_state, layer).visible = visible;
    if !visible {
        match layer {
            MapLayerId::Metars => {
                clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
            }
            MapLayerId::Nexrad => {
                clear_data_status_record(session, LIVE_FEED_NEXRAD_STATUS_ID);
            }
            MapLayerId::Vectors => {
                clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
            }
            MapLayerId::WorldBasemap | MapLayerId::TerrainWarning | MapLayerId::OfflineRegions => {}
        }
    }
    snapshot_for_session(session)
}

pub fn load_raster_map_catalog_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let selected_map_id = session
        .raster_map_catalog
        .as_ref()
        .map(|catalog| catalog.selected_map_id.as_str());
    let catalog = match crate::had_ops::raster_map_catalog_from_nav_kv(
        session_nav_kv_store(session)?,
        selected_map_id,
        None,
    ) {
        Ok(catalog) => catalog,
        Err(err) => return had_read_error_to_overlay_outcome(err),
    };
    session.raster_map_catalog = Some(catalog);
    session_snapshot_outcome(session)
}

pub fn set_resource_policy_in_session(
    handle: u32,
    policy: CoreResourcePolicy,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if session.resource_policy != policy {
        session.resource_policy = policy;
        session.publication_resolver.set_resource_policy(policy);
        session.raster_map_catalog = None;
    }
    snapshot_for_session(session)
}

fn raster_resource_mode_for_policy(policy: CoreResourcePolicy) -> RasterResourceMode {
    match policy {
        CoreResourcePolicy::InstalledPackage => RasterResourceMode::InstalledPackage,
        CoreResourcePolicy::PublicUnpacked => RasterResourceMode::PublicUnpacked,
    }
}

pub fn select_map_family_in_session(
    handle: u32,
    family_id: &str,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_mut() else {
        let catalog = match crate::had_ops::raster_map_catalog_from_nav_kv(
            session_nav_kv_store(session)?,
            None,
            Some(family_id),
        ) {
            Ok(catalog) => catalog,
            Err(err) => return had_read_error_to_overlay_outcome(err),
        };
        session.raster_map_catalog = Some(catalog);
        return session_snapshot_outcome(session);
    };
    crate::select_map_family_in_catalog(catalog, family_id);
    session_snapshot_outcome(session)
}

pub fn select_raster_map_in_session(
    handle: u32,
    selected_map_id: &str,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_mut() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    crate::select_map_in_catalog(catalog, selected_map_id);
    snapshot_for_session(session)
}

pub fn get_raster_tile_plan_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<RasterTilePlan> {
    let sessions = lock_sessions();
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
        resource_mode: raster_resource_mode_for_policy(session.resource_policy),
        device_pixel_ratio: 1.0,
    };
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    Ok(crate::raster_tile_plan_with_options(
        &catalog, &viewport, width_px, height_px, options,
    ))
}

pub fn get_raster_tile_plan_in_session_with_options(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    options: crate::RasterTilePlanOptions,
) -> AppResult<RasterTilePlan> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    Ok(crate::raster_tile_plan_with_options(
        &catalog, &viewport, width_px, height_px, options,
    ))
}

pub fn get_raster_tile_plan_in_session_with_display_scale(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
) -> AppResult<RasterTilePlan> {
    let sessions = lock_sessions();
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
        device_pixel_ratio,
        resource_mode: raster_resource_mode_for_policy(session.resource_policy),
    };
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    Ok(crate::raster_tile_plan_with_options(
        &catalog, &viewport, width_px, height_px, options,
    ))
}

pub fn set_map_layer_enabled_in_session(
    handle: u32,
    layer_id: &str,
    enabled: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    let toggle = map_layer_toggle_mut(&mut session.map_layer_state, layer);
    toggle.enabled = enabled;
    if !enabled {
        toggle.visible = false;
    }
    snapshot_for_session(session)
}

#[allow(dead_code)]
pub(crate) fn set_guidance_leg_geometry_in_session(
    handle: u32,
    geometries: Vec<GuidanceLegGeometry>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.guidance_leg_geometry = geometries
        .into_iter()
        .map(|geometry| (geometry.leg_id.clone(), geometry))
        .collect();
    if selected_ownship_source_kind(&session.app_state.ownship)
        == Some(crate::OwnshipSourceKind::FlightPlanSimulator)
        && session.plan_preview.pointer.is_none()
    {
        sync_plan_preview_to_active_leg(session)?;
    }
    snapshot_for_session(session)
}

pub fn sync_guidance_geometry_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let Some(plan) = session.app_state.active_plan.clone() else {
        session.guidance_leg_geometry.clear();
        return session_snapshot_outcome(session);
    };
    let route =
        match crate::had_ops::project_flight_plan_route(session_nav_kv_store(session)?, &plan) {
            Ok(route) => route,
            Err(HadReadError::NeedPages(pages)) => {
                return Ok(HadOperationOutcome::NeedResources {
                    resources: nav_kv_page_resources(pages),
                });
            }
            Err(HadReadError::Fatal(message)) => {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message,
                });
            }
        };
    session.guidance_leg_geometry = route
        .into_iter()
        .map(|segment| {
            let geometry = GuidanceLegGeometry {
                leg_id: segment.id,
                from: segment.from,
                to: segment.to,
                path: segment.path,
            };
            (geometry.leg_id.clone(), geometry)
        })
        .collect();
    if selected_ownship_source_kind(&session.app_state.ownship)
        == Some(crate::OwnshipSourceKind::FlightPlanSimulator)
        && session.plan_preview.pointer.is_none()
    {
        sync_plan_preview_to_active_leg(session)?;
    }
    session_snapshot_outcome(session)
}

pub fn project_flight_plan_route_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let Some(plan) = session.app_state.active_plan.clone() else {
        return Ok(HadOperationOutcome::complete(
            serde_json::to_value(Vec::<crate::FlightPlanRouteSegment>::new()).map_err(|err| {
                AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                }
            })?,
        ));
    };
    match crate::had_ops::project_flight_plan_route(session_nav_kv_store(session)?, &plan) {
        Ok(route) => Ok(HadOperationOutcome::complete(
            serde_json::to_value(route).map_err(|err| AppError {
                kind: AppErrorKind::Internal,
                message: err.to_string(),
            })?,
        )),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

pub fn select_airport_in_session(handle: u32, airport_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn select_chart_in_session(handle: u32, chart_id: &str) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(chart_id),
    );
    snapshot_for_session(session)
}

pub fn register_ownship_source_in_session(
    handle: u32,
    registration: crate::OwnshipSourceRegistration,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::RegisterOwnshipSource(registration),
    )?;
    snapshot_for_session(session)
}

pub fn register_ownship_source_in_session_outcome(
    handle: u32,
    registration: crate::OwnshipSourceRegistration,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::RegisterOwnshipSource(registration),
    )?;
    session_snapshot_outcome(session)
}

pub fn update_ownship_source_status_in_session(
    handle: u32,
    update: crate::OwnshipSourceStatusUpdate,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::UpdateOwnshipSourceStatus(update),
    )?;
    snapshot_for_session(session)
}

pub fn update_ownship_source_status_in_session_outcome(
    handle: u32,
    update: crate::OwnshipSourceStatusUpdate,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::UpdateOwnshipSourceStatus(update),
    )?;
    session_snapshot_outcome(session)
}

pub fn push_situation_sample_in_session(
    handle: u32,
    sample: crate::SituationSample,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::PushSituationSample(sample))?;
    snapshot_for_session(session)
}

pub fn push_situation_sample_in_session_outcome(
    handle: u32,
    sample: crate::SituationSample,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::PushSituationSample(sample))?;
    session_snapshot_outcome(session)
}

pub fn set_ownship_policy_in_session(
    handle: u32,
    policy: crate::OwnshipPolicy,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::SetOwnshipPolicy(policy))?;
    snapshot_for_session(session)
}

pub fn select_ownship_source_in_session(
    handle: u32,
    selection: crate::OwnshipSelectionCommand,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    if selected_source_kind == Some(crate::OwnshipSourceKind::DebugOwnshipDriver)
        && !debug_ownship_driver_available(session)
    {
        return snapshot_for_session(session);
    }
    session.app_state =
        state::reduce(&session.app_state, AppEvent::SelectOwnshipSource(selection))?;
    match selected_source_kind {
        Some(crate::OwnshipSourceKind::FlightPlanSimulator) => {
            sync_plan_preview_to_active_leg(session)?;
        }
        Some(crate::OwnshipSourceKind::DebugOwnshipDriver) => {
            session.debug_ownship_driver = DebugOwnshipDriverState::default();
            tick_debug_ownship_driver(session, 0.0)?;
        }
        _ => {}
    }
    snapshot_for_session(session)
}

pub fn select_ownship_source_in_session_outcome(
    handle: u32,
    selection: crate::OwnshipSelectionCommand,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
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
    if selected_source_kind == Some(crate::OwnshipSourceKind::DebugOwnshipDriver)
        && !debug_ownship_driver_available(session)
    {
        return session_snapshot_outcome(session);
    }
    session.app_state =
        state::reduce(&session.app_state, AppEvent::SelectOwnshipSource(selection))?;
    match selected_source_kind {
        Some(crate::OwnshipSourceKind::FlightPlanSimulator) => {
            sync_plan_preview_to_active_leg(session)?;
        }
        Some(crate::OwnshipSourceKind::DebugOwnshipDriver) => {
            session.debug_ownship_driver = DebugOwnshipDriverState::default();
            tick_debug_ownship_driver(session, 0.0)?;
        }
        _ => {}
    }
    session_snapshot_outcome(session)
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
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    situation_source_handler_for_session(session).apply_input(session, input, now_epoch_ms)?;
    snapshot_for_session(session)
}

pub fn load_playback_trace_in_session(
    handle: u32,
    source_path: &str,
    trace_json: &str,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn play_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn pause_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn seek_playback_in_session(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn set_playback_rate_in_session(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn tick_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn set_situation_in_session(
    handle: u32,
    situation: crate::Situation,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    apply_situation_to_ownship(
        session,
        DIRECT_SITUATION_SOURCE_ID,
        crate::OwnshipSourceKind::FlightPlanSimulator,
        "Plan Preview",
        situation,
        0,
    )?;
    snapshot_for_session(session)
}

pub fn set_situation_in_session_outcome(
    handle: u32,
    situation: crate::Situation,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    apply_situation_to_ownship(
        session,
        DIRECT_SITUATION_SOURCE_ID,
        crate::OwnshipSourceKind::FlightPlanSimulator,
        "Plan Preview",
        situation,
        0,
    )?;
    session_snapshot_outcome(session)
}

pub fn tick_debug_ownship_driver_in_session(
    handle: u32,
    now_epoch_ms: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    tick_debug_ownship_driver(session, now_epoch_ms)?;
    snapshot_for_session(session)
}

pub fn tick_debug_ownship_driver_in_session_outcome(
    handle: u32,
    now_epoch_ms: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    tick_debug_ownship_driver(session, now_epoch_ms)?;
    session_snapshot_outcome(session)
}

#[allow(dead_code)]
fn replace_flight_plan_in_session(handle: u32, plan: FlightPlan) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    replace_session_flight_plan(session, plan)?;
    snapshot_for_session(session)
}

pub fn activate_next_leg_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::activate_next_leg)
}

pub fn suspend_sequencing_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_guidance(handle, crate::suspend_sequencing)
}

pub fn unsuspend_sequencing_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_guidance(handle, crate::unsuspend_sequencing)
}

pub fn sequence_active_leg_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    mutate_session_flight_plan(handle, crate::sequence_active_leg)
}

fn mutate_session_flight_plan(
    handle: u32,
    mutation: impl FnOnce(&FlightPlan) -> AppResult<FlightPlan>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = mutation(&plan)?;
    replace_session_flight_plan(session, next_plan)?;
    snapshot_for_session(session)
}

fn mutate_session_guidance(
    handle: u32,
    mutation: impl FnOnce(&FlightPlan) -> AppResult<FlightPlan>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = mutation(&plan)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::ReplaceFlightPlan(next_plan))?;
    snapshot_for_session(session)
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
            Ok(HadOperationOutcome::complete(
                serde_json::to_value(snapshot).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
            ))
        }
    }
}

fn insert_waypoint_best_position_for_session(
    handle: u32,
    waypoint: NavRef,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let store = session_nav_kv_store(session)?;
    let mutation = match insert_waypoint_best_position(store, &plan, waypoint) {
        Ok(mutation) => mutation,
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    commit_session_flight_plan_with_snapshot_outcome(session, mutation.plan)
}

pub fn insert_waypoint_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    before: bool,
    waypoint: NavRef,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
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
    commit_session_flight_plan_with_snapshot_outcome(session, next_plan)
}

pub fn suggest_waypoint_identifiers_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    before: bool,
    prefix: String,
    limit: usize,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
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
                return Ok(HadOperationOutcome::NeedResources {
                    resources: nav_kv_page_resources(pages),
                })
            }
            Err(HadReadError::Fatal(message)) => {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message,
                });
            }
        };
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(suggestions).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    ))
}

pub fn preview_flight_plan_entry_in_session(
    handle: u32,
    input: String,
) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let plan = session_plan(session)?;
    let store = session_nav_kv_store(session)?;
    match crate::had_ops::preview_flight_plan_entry(store, &plan, &input) {
        Ok(preview) => Ok(HadOperationOutcome::complete(
            serde_json::to_value(preview).map_err(|err| AppError {
                kind: AppErrorKind::Internal,
                message: err.to_string(),
            })?,
        )),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

pub fn append_flight_plan_entry_in_session(
    handle: u32,
    input: String,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let mutation = {
        let store = session_nav_kv_store(session)?;
        match crate::had_ops::append_flight_plan_entry(store, &plan, &input) {
            Ok(mutation) => mutation,
            Err(HadReadError::NeedPages(pages)) => {
                return Ok(HadOperationOutcome::NeedResources {
                    resources: nav_kv_page_resources(pages),
                })
            }
            Err(HadReadError::Fatal(message)) => {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message,
                });
            }
        }
    };
    commit_session_flight_plan_with_snapshot_outcome(session, mutation.plan)
}

pub fn insert_airway_at_flight_plan_row_in_session(
    handle: u32,
    row_uid: String,
    presentation: AirwayPresentationPlan,
    entry_index: usize,
    exit_index: usize,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
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
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
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
    commit_session_flight_plan_with_snapshot_outcome(session, mutation.mutation.plan)
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
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let mut plan = session_plan(session)?;
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
    let mut airport_component_index = airport_component_index;
    if replace_component_index.is_none() && airport_component_index > 0 {
        let repaired =
            crate::materialize_airway_exit_before_component(&plan, airport_component_index)?;
        plan = repaired.0;
        airport_component_index = repaired.1;
    }
    let start_component_index = airport_component_index.checked_sub(1);
    let store = session_nav_kv_store(session)?;
    let procedure_component_index = replace_component_index
        .or(start_component_index)
        .unwrap_or(airport_component_index);
    let built = match materialize_procedure(
        store,
        &airport_id,
        &procedure_id,
        kind,
        runway_transition.as_deref(),
        enroute_transition.as_deref(),
        procedure_component_index,
    ) {
        Ok(built) => built,
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    let mutation = if let Some(replace_component_index) = replace_component_index {
        crate::replace_procedure_materialized_ui(&plan, replace_component_index, built)?
    } else if let Some(start_component_index) = start_component_index {
        crate::insert_procedure_materialized_ui(
            &plan,
            start_component_index,
            airport_component_index,
            built,
        )?
    } else {
        crate::insert_initial_procedure_materialized_ui(&plan, airport_component_index, built)?
    };
    commit_session_flight_plan_with_snapshot_outcome(session, mutation.mutation.plan)
}

pub fn load_plate_procedure_in_session(
    handle: u32,
    load_id: String,
) -> AppResult<HadOperationOutcome> {
    let command: ProcedureLoadCommand = serde_json::from_str(&load_id).map_err(|err| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("invalid procedure load id: {err}"),
    })?;
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let mut plan = session_plan(session)?;
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
    let mut airport_component_index = airport_component_index;
    if replace_component_index.is_none() && airport_component_index > 0 {
        let repaired =
            crate::materialize_airway_exit_before_component(&plan, airport_component_index)?;
        plan = repaired.0;
        airport_component_index = repaired.1;
    }
    let start_component_index = airport_component_index.checked_sub(1);
    let store = session_nav_kv_store(session)?;
    let procedure_component_index = replace_component_index
        .or(start_component_index)
        .unwrap_or(airport_component_index);
    let built = match materialize_procedure(
        store,
        &command.airport_id,
        &command.procedure_id,
        command.kind,
        command.runway_transition.as_deref(),
        command.enroute_transition.as_deref(),
        procedure_component_index,
    ) {
        Ok(built) => built,
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            });
        }
    };
    let mutation = if let Some(replace_component_index) = replace_component_index {
        crate::replace_procedure_materialized_ui(&plan, replace_component_index, built)?
    } else if let Some(start_component_index) = start_component_index {
        crate::insert_procedure_materialized_ui(
            &plan,
            start_component_index,
            airport_component_index,
            built,
        )?
    } else {
        crate::insert_initial_procedure_materialized_ui(&plan, airport_component_index, built)?
    };
    commit_session_flight_plan_with_snapshot_outcome(session, mutation.mutation.plan)
}

fn activate_direct_to_nav_ref_in_session(
    handle: u32,
    target: NavRef,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn activate_direct_to_leg_in_session(
    handle: u32,
    target_leg_index: usize,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
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
    snapshot_for_session(session)
}

pub fn perform_status_action_in_session(
    handle: u32,
    action_id: String,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let command =
        parse_status_action_id(&action_id).ok_or_else(|| invalid_status_action(&action_id))?;
    match command {
        UiStatusActionCommand::Hush(status_id) => {
            if !session.data_status_records.contains_key(&status_id) {
                return Err(invalid_status_action(&action_id));
            }
            session.hushed_status_ids.insert(status_id);
            sync_data_status_projection(session);
        }
        UiStatusActionCommand::Unhush(status_id) => {
            if !session.data_status_records.contains_key(&status_id) {
                return Err(invalid_status_action(&action_id));
            }
            session.hushed_status_ids.remove(&status_id);
            sync_data_status_projection(session);
        }
    }
    snapshot_for_session(session)
}

fn invalid_status_action(action_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown status action: {action_id}"),
    }
}

pub fn perform_flight_plan_row_action_in_session(
    handle: u32,
    row_uid: String,
    action_uid: String,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
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
    let action = crate::planning::flight_plan_row_actions(row)
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
            if row.row_kind == FlightPlanDisplayRowKind::Discontinuity
                && row.label == ProcedureDiscontinuity::Hold.display_label()
            {
                match crate::terminal_hold_start_detail_index_for_leg(&plan, leg_index) {
                    Some(detail_index) => {
                        crate::activate_leg_at_detail_index(&plan, leg_index, detail_index)?
                    }
                    None => crate::activate_leg(&plan, leg_index)?,
                }
            } else {
                crate::activate_leg(&plan, leg_index)?
            }
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
            if row.component_kind == Some(RouteComponentViewKind::Airway) && row.depth > 0 {
                let nav_ref = row.nav_ref.as_ref().ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "airway child remove row has no nav reference".to_string(),
                })?;
                crate::remove_airway_child_waypoint(&plan, row_component_index()?, nav_ref)?
            } else {
                crate::delete_component(&plan, row_component_index()?)?
            }
        }
        FlightPlanRowActionId::RemoveAllAbove => {
            if row.component_kind == Some(RouteComponentViewKind::Airway) && row.depth > 0 {
                let nav_ref = row.nav_ref.as_ref().ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "airway child remove-all-above row has no nav reference".to_string(),
                })?;
                crate::remove_all_above_airway_child_waypoint(
                    &plan,
                    row_component_index()?,
                    nav_ref,
                )?
            } else {
                crate::remove_all_above(&plan, row_component_index()?)?
            }
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
    commit_session_flight_plan_with_snapshot_outcome(session, next_plan)
}

pub fn restore_direct_to_in_session(handle: u32) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = crate::restore_direct_to(&plan)?;
    replace_session_flight_plan(session, next_plan)?;
    snapshot_for_session(session)
}

pub fn attach_nav_kv_store_to_session(
    handle: u32,
    store_id: u32,
    store: &NavKvStore,
) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.nav_kv_store_id = Some(store_id);
    session.nav_kv_store = Some(store.clone());
    session.towered_metar_station_ids = None;
    Ok(())
}

pub fn insert_nav_kv_page_for_attached_sessions(store_id: u32, page_index: u32, page_bytes: &[u8]) {
    let mut sessions = lock_sessions();
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
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.engage(viewport);
    snapshot_for_session(session)
}

pub fn disengage_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.disengage(viewport);
    snapshot_for_session(session)
}

pub fn set_map_follow_offset_in_session(
    handle: u32,
    viewport: MapViewport,
    offset_x_px: f64,
    offset_y_px: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session
        .map_follow
        .set_anchor_offset(viewport, offset_x_px, offset_y_px);
    snapshot_for_session(session)
}

pub fn sync_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.sync_for_viewport(
        &session.app_state.ownship.render,
        viewport,
        width_px,
        height_px,
    );
    snapshot_for_session(session)
}

pub fn restore_chart_page_state_in_session(
    handle: u32,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
    );
    snapshot_for_session(session)
}

pub fn set_debug_flag_in_session(
    handle: u32,
    flag_id: &str,
    enabled: bool,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    match flag_id {
        "tile_labels" => session.debug_state.tile_labels = enabled,
        "nexrad_tile_labels" => session.debug_state.nexrad_tile_labels = enabled,
        "fast_tiles" => session.debug_state.fast_tiles = enabled,
        "offline_simulated_clock_buttons" => {
            session.debug_state.offline_simulated_clock_buttons = enabled
        }
        "sequencing_finish_lines" => session.debug_state.sequencing_finish_lines = enabled,
        _ => {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: format!("unknown debug flag id: {flag_id}"),
            });
        }
    }
    snapshot_for_session(session)
}

pub fn get_session_snapshot(handle: u32) -> AppResult<UiSessionSnapshot> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    snapshot_for_session(session)
}

pub fn ingest_point_tiles_in_session(handle: u32, tiles: &[PointTilePayload]) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        let aggregate = session
            .vector_tile_cache
            .entry(crate::aggregate_vector_tile_cache_key(
                tile.z, tile.x, tile.y,
            ))
            .or_insert_with(|| empty_vector_aggregate_tile(tile.z, tile.x, tile.y));
        match tile.layer.as_str() {
            "airport" => aggregate.airports = tile.records.clone(),
            "fix" => aggregate.fixes = tile.records.clone(),
            "nav" => aggregate.navaids = tile.records.clone(),
            "obstacle" => aggregate.obstacles = tile.records.clone(),
            _ => {}
        }
    }
    Ok(())
}

pub fn ingest_airspace_ref_tiles_in_session(
    handle: u32,
    tiles: &[AirspaceReferenceTilePayload],
) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session
            .vector_tile_cache
            .entry(crate::aggregate_vector_tile_cache_key(
                tile.z, tile.x, tile.y,
            ))
            .or_insert_with(|| empty_vector_aggregate_tile(tile.z, tile.x, tile.y))
            .airspace_refs = tile.refs.clone();
    }
    Ok(())
}

pub fn ingest_airspace_features_in_session(
    handle: u32,
    features: &[AirspaceFeaturePayload],
) -> AppResult<()> {
    let mut sessions = lock_sessions();
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
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    for tile in tiles {
        session
            .vector_tile_cache
            .entry(crate::aggregate_vector_tile_cache_key(
                tile.z, tile.x, tile.y,
            ))
            .or_insert_with(|| empty_vector_aggregate_tile(tile.z, tile.x, tile.y))
            .airspace_labels = tile.labels.clone();
    }
    Ok(())
}

fn empty_vector_aggregate_tile(z: u32, x: u32, y: u32) -> VectorAggregateTilePayload {
    VectorAggregateTilePayload {
        schema_version: 1,
        z,
        x,
        y,
        airports: Vec::new(),
        fixes: Vec::new(),
        navaids: Vec::new(),
        obstacles: Vec::new(),
        airspace_refs: Vec::new(),
        airspace_labels: Vec::new(),
    }
}

pub fn ingest_tfrs_in_session(handle: u32, payload: &TfrProductPayload) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.tfr_payload = Some(payload.clone());
    clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
    Ok(())
}

pub fn ingest_tafs_in_session(handle: u32, payload: &TafProductPayload) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.taf_payload = Some(payload.clone());
    Ok(())
}

pub fn sync_live_feeds_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    Ok(session.live_feeds.sync_outcome_with_invalidations())
}

pub fn ingest_live_feed_sse_event_in_session(
    handle: u32,
    event: &LiveFeedSseEvent,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.live_feeds.ingest_sse_event(event.clone())
}

pub fn ingest_live_feed_sse_events_in_session(
    handle: u32,
    events: &[LiveFeedSseEvent],
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let affected = session
        .live_feeds
        .ingest_sse_events(events.iter().cloned())?;
    Ok(session
        .live_feeds
        .sync_products_outcome_with_invalidations(affected.iter().map(String::as_str)))
}

pub fn ingest_resource_in_session(handle: u32, resource_id: &str, bytes: &[u8]) -> AppResult<()> {
    if LiveFeedsState::handles_resource(resource_id) {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        session.live_feeds.ingest_resource(resource_id, bytes)?;
        install_live_feed_payloads(session)?;
        return Ok(());
    }
    if resource_id.starts_with("publication/") {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        session
            .publication_resolver
            .ingest_resource(resource_id, bytes)
            .map_err(|message| AppError {
                kind: AppErrorKind::InvalidManifest,
                message,
            })?;
        return Ok(());
    }
    if let Some(rest) = resource_id.strip_prefix("terrain/source/") {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        session
            .terrain_source_tile_cache
            .insert(rest.to_string(), bytes.to_vec());
        return Ok(());
    }
    Err(AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unsupported session resource id: {resource_id}"),
    })
}

pub fn report_session_resource_failure_in_session(
    handle: u32,
    resource_id: &str,
    message: &str,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if LiveFeedsState::handles_resource(resource_id) {
        record_live_feed_fetch_failure(session, resource_id, message);
    }
    snapshot_for_session(session)
}

fn install_live_feed_payloads(session: &mut UiSession) -> AppResult<()> {
    if let Some(metars_value) = session.live_feeds.product_state_manifest("metars").cloned() {
        match serde_json::from_value::<MetarProductPayload>(metars_value) {
            Ok(payload) => {
                session.metar_payload = Some(payload);
                rebuild_metar_tile_cache(session);
                clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
            }
            Err(err) => {
                session.metar_tile_cache.clear();
                session.metar_payload = None;
                upsert_data_status_record(
                    session,
                    live_feed_unavailable_status_record(
                        "metars",
                        format!("METAR live feed unavailable: failed to parse state: {err}"),
                    ),
                );
            }
        }
    }
    if let Some(tfrs_value) = session.live_feeds.product_state_manifest("tfrs").cloned() {
        match serde_json::from_value::<TfrProductPayload>(tfrs_value) {
            Ok(payload) => {
                session.tfr_payload = Some(payload);
                clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
            }
            Err(err) => {
                session.tfr_payload = None;
                upsert_data_status_record(
                    session,
                    live_feed_unavailable_status_record(
                        "tfrs",
                        format!("TFR live feed unavailable: failed to parse state: {err}"),
                    ),
                );
            }
        }
    }
    Ok(())
}

fn metar_tile_cache_for_live_feed(
    payload: &MetarProductPayload,
    layer: Option<&crate::map_overlay::PointTileLayerConfig>,
    important_station_ids: &HashSet<String>,
) -> HashMap<String, MetarTilePayload> {
    let Some(layer) = layer else {
        return HashMap::new();
    };
    let mut cache = HashMap::new();
    for zoom in &layer.available_zooms {
        for record in payload.metars_by_station.values() {
            if *zoom == layer.min_zoom && !important_station_ids.contains(&record.station_id) {
                continue;
            }
            let Some((x, y)) = metar_tile_xy(record.latitude, record.longitude, *zoom) else {
                continue;
            };
            let key = crate::tile_key("metars", *zoom, x, y);
            cache
                .entry(key)
                .or_insert_with(|| MetarTilePayload {
                    schema_version: 1,
                    layer: "metars".to_string(),
                    z: *zoom,
                    x,
                    y,
                    records: Vec::new(),
                })
                .records
                .push(MetarTileRecord {
                    kind: "metar".to_string(),
                    id: record.station_id.clone(),
                });
        }
    }
    cache
}

fn rebuild_metar_tile_cache(session: &mut UiSession) {
    if let Some(payload) = session.metar_payload.as_ref() {
        let empty = HashSet::new();
        let important_station_ids = session.towered_metar_station_ids.as_ref().unwrap_or(&empty);
        session.metar_tile_cache = metar_tile_cache_for_live_feed(
            payload,
            session.map_overlay_config.metar_layer.as_ref(),
            important_station_ids,
        );
    } else {
        session.metar_tile_cache.clear();
    }
}

fn ensure_metar_station_importance_loaded(session: &mut UiSession) -> Result<(), HadReadError> {
    if session.towered_metar_station_ids.is_some() {
        return Ok(());
    }
    let Some(store) = session.nav_kv_store.as_ref() else {
        session.towered_metar_station_ids = Some(HashSet::new());
        return Ok(());
    };
    let station_ids = towered_metar_station_ids_from_nav_db(store)?;
    session.towered_metar_station_ids = Some(station_ids);
    rebuild_metar_tile_cache(session);
    Ok(())
}

fn towered_metar_station_ids_from_nav_db(
    store: &NavKvStore,
) -> Result<HashSet<String>, HadReadError> {
    let bytes = match store
        .keys_with_prefix_lookup("navref/symbol/airport/")
        .map_err(HadReadError::Fatal)?
    {
        NavKvLookup::Hit(bytes) => bytes,
        NavKvLookup::MissingKey => return Ok(HashSet::new()),
        NavKvLookup::MissingPages(pages) => return Err(HadReadError::NeedPages(pages)),
    };
    let keys = String::from_utf8(bytes).map_err(|err| {
        HadReadError::Fatal(format!("HAD navref airport key list decode failed: {err}"))
    })?;
    let keys = keys.lines().map(str::to_string).collect::<Vec<_>>();
    let needed_pages = store
        .missing_pages_for_keys(&keys)
        .map_err(HadReadError::Fatal)?;
    if !needed_pages.is_empty() {
        return Err(HadReadError::NeedPages(needed_pages));
    }
    let mut station_ids = HashSet::new();
    for key in keys {
        let bytes = match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
            NavKvLookup::Hit(bytes) => bytes,
            NavKvLookup::MissingKey => continue,
            NavKvLookup::MissingPages(pages) => return Err(HadReadError::NeedPages(pages)),
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| {
            HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))
        })?;
        if value
            .get("towered")
            .and_then(|towered| towered.as_bool())
            .unwrap_or(false)
        {
            if let Some(station_id) = key.strip_prefix("navref/symbol/airport/") {
                station_ids.insert(station_id.trim().to_ascii_uppercase());
            }
        }
    }
    Ok(station_ids)
}

fn metar_tile_xy(lat: f64, lon: f64, zoom: u32) -> Option<(u32, u32)> {
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    let scale = 2_u32.checked_pow(zoom)?;
    let scale_f64 = scale as f64;
    let x = (((lon + 180.0) / 360.0) * scale_f64).floor();
    let clamped_lat = lat.clamp(-WORLD_MERCATOR_MAX_LATITUDE, WORLD_MERCATOR_MAX_LATITUDE);
    let y = ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0
        * scale_f64)
        .floor();
    Some((
        positive_mod_i64(x as i64, scale as i64) as u32,
        (y as i64).clamp(0, scale as i64 - 1) as u32,
    ))
}

fn positive_mod_i64(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

fn internal_json_error(err: serde_json::Error) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message: err.to_string(),
    }
}

fn had_read_error_to_overlay_outcome(err: HadReadError) -> AppResult<HadOperationOutcome> {
    match err {
        HadReadError::NeedPages(pages) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
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
    install_vector_manifest_config(session, &manifest_json).map_err(HadReadError::Fatal)?;
    Ok(())
}

fn install_vector_manifest_config(
    session: &mut UiSession,
    manifest_json: &str,
) -> Result<(), String> {
    session.map_overlay_config = map_overlay_config_from_vector_manifest_json(&manifest_json)
        .map_err(|err| err.to_string())?;
    rebuild_metar_tile_cache(session);
    session.vector_manifest_loaded = true;
    Ok(())
}

fn ensure_vector_inputs_loaded(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
) -> Result<(), HadReadError> {
    ensure_vector_manifest_loaded(session)?;
    for _ in 0..8 {
        let overlay = query_map_overlay_for_surface(
            metrics,
            &session.map_overlay_config,
            true,
            false,
            &[],
            ownship_overlay_context(session).as_ref(),
            &session.vector_tile_cache,
            &session.metar_tile_cache,
            session.metar_payload.as_ref(),
            &session.airspace_feature_cache,
            session.tfr_payload.as_ref(),
        );
        let needed_vector_inputs =
            overlay.needed_vector_tiles.len() + overlay.needed_airspace_features.len();
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

        let mut vector_tiles = Vec::new();
        for tile in overlay.needed_vector_tiles {
            let payload = read_attached_json_optional::<VectorAggregateTilePayload>(
                store,
                NavKvQuery::VectorTile {
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                },
            )?
            .unwrap_or(VectorAggregateTilePayload {
                schema_version: 1,
                z: tile.z,
                x: tile.x,
                y: tile.y,
                airports: Vec::new(),
                fixes: Vec::new(),
                navaids: Vec::new(),
                obstacles: Vec::new(),
                airspace_refs: Vec::new(),
                airspace_labels: Vec::new(),
            });
            vector_tiles.push(payload);
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

        for tile in vector_tiles {
            session.vector_tile_cache.insert(
                crate::aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y),
                tile,
            );
            loaded_any = true;
        }
        for feature in features {
            session
                .airspace_feature_cache
                .insert(feature.id.clone(), feature);
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
        .needed_vector_tiles
        .iter()
        .filter_map(|tile| {
            nav_kv_key_for_query(&NavKvQuery::VectorTile {
                z: tile.z,
                x: tile.x,
                y: tile.y,
            })
        })
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
        .collect()
}

pub fn get_map_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    get_map_overlay_in_session_with_point_display_scale(handle, viewport, width_px, height_px, 1.0)
}

pub fn get_map_overlay_in_session_with_point_display_scale(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, point_display_scale);
    if !session.map_layer_state.vectors.visible
        && !session.map_layer_state.metars.visible
        && !session.map_layer_state.offline_regions.visible
    {
        let invalidations = sync_map_overlay_status_records(session, Vec::new());
        return Ok(HadOperationOutcome::complete_with_invalidations(
            serde_json::to_value(empty_map_overlay_query()).map_err(internal_json_error)?,
            invalidations,
        ));
    }
    if session.map_layer_state.vectors.visible {
        if let Err(err) = ensure_vector_inputs_loaded(session, &metrics) {
            return had_read_error_to_overlay_outcome(err);
        }
    }
    if session.map_layer_state.metars.visible && session.metar_payload.is_some() {
        if let Err(err) = ensure_metar_station_importance_loaded(session) {
            return had_read_error_to_overlay_outcome(err);
        }
    }
    let offline_region_records = if session.map_layer_state.offline_regions.visible {
        let store = session.nav_kv_store.as_ref().ok_or_else(|| AppError {
            kind: AppErrorKind::Internal,
            message: "session missing nav kv store for offline regions overlay".to_string(),
        })?;
        match read_attached_json_optional::<crate::OfflineRegionCatalog>(
            store,
            NavKvQuery::OfflineRegionCatalog,
        ) {
            Ok(Some(catalog)) => catalog.regions,
            Ok(None) => Vec::new(),
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
    } else {
        Vec::new()
    };
    let mut overlay = query_map_overlay_for_surface(
        &metrics,
        &session.map_overlay_config,
        session.map_layer_state.vectors.visible,
        session.map_layer_state.metars.visible,
        &offline_region_records,
        ownship_overlay_context(session).as_ref(),
        &session.vector_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        &session.airspace_feature_cache,
        session.tfr_payload.as_ref(),
    );
    if session.map_layer_state.vectors.visible {
        overlay.flight_plan_features =
            match flight_plan_overlay_features(session, &viewport, width_px, height_px) {
                Ok(features) => features,
                Err(err) => return had_read_error_to_overlay_outcome(err),
            };
    }
    let resources = weather_overlay_resources(session, &overlay);
    if !resources.is_empty() {
        return Ok(HadOperationOutcome::NeedResources { resources });
    }
    let invalidations =
        sync_map_overlay_status_records(session, std::mem::take(&mut overlay.data_status_records));
    Ok(HadOperationOutcome::complete_with_invalidations(
        serde_json::to_value(overlay).map_err(internal_json_error)?,
        invalidations,
    ))
}

fn flight_plan_overlay_features(
    session: &UiSession,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Result<Vec<crate::VisibleMapFeature>, HadReadError> {
    let points = flight_plan_selection_points(session)?;
    let Some(plan) = session.app_state.active_plan.as_ref() else {
        return Ok(Vec::new());
    };
    let plan = crate::build_flight_plan(plan.clone()).map_err(|err| {
        HadReadError::Fatal(format!("failed to build flight plan overlay plan: {err}"))
    })?;
    let active_target = crate::active_guidance_leg(&plan).map(|leg| leg.to);
    Ok(points
        .into_iter()
        .map(|point| {
            let mut symbol = point.symbol;
            symbol.label = chart_ident_label_for_nav_ref_symbol(&point.nav_ref, &symbol);
            let label_style = if active_target.as_ref() == Some(&point.nav_ref) {
                VectorIdentLabelStyle::ActiveFlightPlan
            } else {
                VectorIdentLabelStyle::FlightPlan
            };
            project_nav_symbol_feature(
                format!("flight-plan:{}", nav_ref_overlay_key(&point.nav_ref)),
                symbol,
                point.position,
                viewport,
                width_px,
                height_px,
                label_style,
            )
        })
        .collect())
}

fn flight_plan_selection_points(
    session: &UiSession,
) -> Result<Vec<FlightPlanSelectionPoint>, HadReadError> {
    let Some(plan) = session.app_state.active_plan.as_ref() else {
        return Ok(Vec::new());
    };
    let plan = crate::build_flight_plan(plan.clone()).map_err(|err| {
        HadReadError::Fatal(format!("failed to build flight plan overlay plan: {err}"))
    })?;
    let Some(store) = session.nav_kv_store.as_ref() else {
        return Ok(Vec::new());
    };
    let mut nav_refs = Vec::<(NavRef, Option<String>)>::new();
    for leg in &plan.resolved_legs {
        let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
            (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.clone())
        });
        push_unique_nav_ref(&mut nav_refs, &leg.from, procedure_airport_id.clone());
        push_unique_nav_ref(&mut nav_refs, &leg.to, procedure_airport_id);
    }
    if let Some(direct_to) = plan
        .guidance
        .as_ref()
        .and_then(|guidance| guidance.direct_to.as_ref())
    {
        push_unique_nav_ref(&mut nav_refs, &direct_to.start, None);
        push_unique_nav_ref(&mut nav_refs, &direct_to.target, None);
    }

    let mut features = Vec::new();
    for (nav_ref, procedure_airport_id) in nav_refs {
        let Some(symbol) = nav_symbol_feature(store, &nav_ref)? else {
            continue;
        };
        let position = nav_ref_position(store, &nav_ref, procedure_airport_id.as_deref())?;
        features.push(FlightPlanSelectionPoint {
            nav_ref,
            symbol,
            position,
        });
    }
    Ok(features)
}

fn push_unique_nav_ref(
    nav_refs: &mut Vec<(NavRef, Option<String>)>,
    nav_ref: &NavRef,
    procedure_airport_id: Option<String>,
) {
    if matches!(nav_ref, NavRef::LatLon(_) | NavRef::Spot(_)) {
        return;
    }
    if !nav_refs.iter().any(|(existing, _)| existing == nav_ref) {
        nav_refs.push((nav_ref.clone(), procedure_airport_id));
    }
}

fn nav_ref_overlay_key(nav_ref: &NavRef) -> String {
    serde_json::to_string(nav_ref).unwrap_or_else(|_| format!("{nav_ref:?}"))
}

fn weather_overlay_resources(
    session: &UiSession,
    overlay: &MapOverlayQueryResult,
) -> Vec<CoreResourceRequest> {
    let _ = session;
    let _ = overlay;
    Vec::new()
}

fn extend_package_resource_requests(
    session: &UiSession,
    resources: &mut Vec<CoreResourceRequest>,
    target_resource_id: &str,
    package_id: &str,
    member_path: &str,
    optional: bool,
) {
    match session.publication_resolver.package_resource_requests(
        target_resource_id,
        package_id,
        member_path,
        optional,
    ) {
        Ok(mut requested) => resources.append(&mut requested),
        Err(_) if optional => {}
        Err(message) => resources.push(CoreResourceRequest::unavailable(
            target_resource_id,
            message,
            optional,
        )),
    }
}

fn dedupe_resource_requests(resources: Vec<CoreResourceRequest>) -> Vec<CoreResourceRequest> {
    let mut by_id = HashMap::new();
    for resource in resources {
        by_id.entry(resource.id.clone()).or_insert(resource);
    }
    by_id.into_values().collect()
}

pub fn get_map_selection_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    click: LatLon,
) -> AppResult<HadOperationOutcome> {
    get_map_selection_in_session_with_point_display_scale(
        handle, viewport, width_px, height_px, click, 1.0,
    )
}

pub fn get_map_selection_in_session_with_point_display_scale(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    click: LatLon,
    point_display_scale: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, point_display_scale);
    if let Err(err) = ensure_vector_inputs_loaded(session, &metrics) {
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
    let offline_region_records = if session.map_layer_state.offline_regions.visible {
        match read_attached_json_optional::<crate::OfflineRegionCatalog>(
            store,
            NavKvQuery::OfflineRegionCatalog,
        ) {
            Ok(Some(catalog)) => catalog.regions,
            Ok(None) => Vec::new(),
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
    } else {
        Vec::new()
    };
    let flight_plan_points = match flight_plan_selection_points(session) {
        Ok(points) => points,
        Err(err) => return had_read_error_to_overlay_outcome(err),
    };
    let selection = query_map_selection_for_surface(
        &metrics,
        &session.map_overlay_config,
        plan,
        click,
        &session.vector_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        session.taf_payload.as_ref(),
        &offline_region_records,
        &session.airspace_feature_cache,
        session.tfr_payload.as_ref(),
        &flight_plan_points,
        &mut availability,
    );
    if !missing_pages.is_empty() {
        return Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(missing_pages),
        });
    }
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(selection).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    ))
}

pub fn get_terrain_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    if !session.map_layer_state.terrain_warning.visible {
        let result = TerrainOverlayQueryResult {
            status: crate::TerrainOverlayStatus::Hidden,
            tile_requests: Vec::new(),
        };
        return Ok(HadOperationOutcome::complete(
            serde_json::to_value(result).map_err(internal_json_error)?,
        ));
    }
    let kinematics = session.app_state.ownship.resolved.kinematics.as_ref();
    let has_position = kinematics.is_some_and(|kinematics| {
        kinematics.position.lat.is_finite() && kinematics.position.lon.is_finite()
    });
    let has_altitude = ownship_terrain_altitude_ft(session).is_some();
    let query =
        crate::query_terrain_overlay(&viewport, width_px, height_px, has_position, has_altitude);
    let resources = terrain_overlay_resources(session, &query);
    if !resources.is_empty() {
        return Ok(HadOperationOutcome::NeedResources { resources });
    }
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(query).map_err(internal_json_error)?,
    ))
}

pub fn get_nexrad_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if !session.map_layer_state.nexrad.visible {
        return complete_nexrad_overlay_outcome(
            session,
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Hidden,
                tiles: Vec::new(),
                stats: NexradOverlayStats::default(),
            },
        );
    }
    if let HadOperationOutcome::NeedResources { resources } = session.live_feeds.sync_outcome() {
        return Ok(HadOperationOutcome::NeedResources { resources });
    }
    let Some(manifest) = session.live_feeds.product_state_manifest("nexrad").cloned() else {
        return complete_nexrad_overlay_outcome(
            session,
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Unavailable {
                    reason: "NEXRAD product is missing from the live feed index".to_string(),
                },
                tiles: Vec::new(),
                stats: NexradOverlayStats::default(),
            },
        );
    };
    let query = match nexrad_overlay_query(&manifest, &viewport, width_px, height_px) {
        Ok(query) => query,
        Err(err) => NexradOverlayQueryResult {
            status: NexradOverlayStatus::Unavailable {
                reason: err.to_string(),
            },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        },
    };
    complete_nexrad_overlay_outcome(session, query)
}

#[derive(Debug, Deserialize)]
struct NexradSourceGridManifest {
    state_id: String,
    observed_at_utc: Option<String>,
    source_grid: NexradSourceGrid,
    levels: Vec<NexradSourceGridLevel>,
    tile_size: u32,
    tile_path_template: String,
}

#[derive(Debug, Deserialize)]
struct NexradSourceGrid {
    geo_transform: [f64; 6],
}

#[derive(Debug, Clone, Deserialize)]
struct NexradSourceGridLevel {
    res: u32,
    width: u32,
    height: u32,
    tile_cols: u32,
    tile_rows: u32,
}

fn nexrad_overlay_query(
    manifest: &serde_json::Value,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<NexradOverlayQueryResult> {
    if width_px <= 0.0 || height_px <= 0.0 {
        return Ok(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Ready { count: 0 },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        });
    }
    let manifest: NexradSourceGridManifest =
        serde_json::from_value(manifest.clone()).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse NEXRAD source-grid manifest: {err}"),
        })?;
    if manifest.tile_path_template != "tiles/res{res}/{x}/{y}.png" {
        return Ok(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Unavailable {
                reason: format!(
                    "unsupported NEXRAD tile path template: {}",
                    manifest.tile_path_template
                ),
            },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        });
    }
    let viewport_bounds = viewport_lat_lon_bounds(viewport, width_px, height_px);
    let [origin_lon, pixel_lon, _rot_x, origin_lat, _rot_y, pixel_lat] =
        manifest.source_grid.geo_transform;
    let observed_at_utc = manifest.observed_at_utc.clone();
    if pixel_lon == 0.0 || pixel_lat == 0.0 {
        return Ok(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Unavailable {
                reason: "NEXRAD source grid has invalid geo transform".to_string(),
            },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        });
    }
    let Some(level) = nexrad_level_for_viewport(
        &manifest.levels,
        viewport,
        width_px,
        height_px,
        pixel_lon,
        pixel_lat,
    ) else {
        return Ok(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Unavailable {
                reason: "NEXRAD manifest has no resolution levels".to_string(),
            },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        });
    };
    let level_scale = 2_f64.powi(level.res as i32);
    let level_pixel_lon = pixel_lon * level_scale;
    let level_pixel_lat = pixel_lat * level_scale;
    let tile_span_lon = level_pixel_lon.abs() * manifest.tile_size as f64;
    let tile_span_lat = level_pixel_lat.abs() * manifest.tile_size as f64;
    let grid_west = origin_lon.min(origin_lon + level_pixel_lon * level.width as f64);
    let grid_east = origin_lon.max(origin_lon + level_pixel_lon * level.width as f64);
    let grid_south = origin_lat.min(origin_lat + level_pixel_lat * level.height as f64);
    let grid_north = origin_lat.max(origin_lat + level_pixel_lat * level.height as f64);
    let west = viewport_bounds.west.max(grid_west);
    let east = viewport_bounds.east.min(grid_east);
    let south = viewport_bounds.south.max(grid_south);
    let north = viewport_bounds.north.min(grid_north);
    if west >= east || south >= north {
        return Ok(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Ready { count: 0 },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
        });
    }

    let x_start = (((west - grid_west) / tile_span_lon).floor() as i64)
        .clamp(0, level.tile_cols as i64 - 1) as u32;
    let x_end = (((east - grid_west) / tile_span_lon).floor() as i64)
        .clamp(0, level.tile_cols as i64 - 1) as u32;
    let y_start = (((grid_north - north) / tile_span_lat).floor() as i64)
        .clamp(0, level.tile_rows as i64 - 1) as u32;
    let y_end = (((grid_north - south) / tile_span_lat).floor() as i64)
        .clamp(0, level.tile_rows as i64 - 1) as u32;

    const NEXRAD_RENDER_MAX_SOURCE_SLICE_PX: u32 = 256;
    const NEXRAD_RENDER_MAX_AFFINE_ERROR_PX: f64 = 1.0;
    const NEXRAD_MAX_LEVEL_PIXEL_STRETCH_PX: f64 = 1.5;
    let mut tiles = Vec::new();
    let mut stats = NexradOverlayStats {
        source_tile_count: ((x_end - x_start + 1) as usize) * ((y_end - y_start + 1) as usize),
        level_pixel_span_px: nexrad_level_screen_pixel_span(
            viewport, width_px, height_px, pixel_lon, pixel_lat, level.res,
        ),
        max_level_pixel_stretch_px: NEXRAD_MAX_LEVEL_PIXEL_STRETCH_PX,
        res: Some(level.res),
        observed_at_utc,
        ..NexradOverlayStats::default()
    };
    for y in y_start..=y_end {
        for x in x_start..=x_end {
            let tile_level_x0 = x * manifest.tile_size;
            let tile_level_y0 = y * manifest.tile_size;
            let image_width = manifest.tile_size.min(level.width - tile_level_x0);
            let image_height = manifest.tile_size.min(level.height - tile_level_y0);
            let src = format!(
                "/live-feeds/states/nexrad/{}/tiles/res{}/{}/{}.png",
                manifest.state_id, level.res, x, y
            );
            let mut stack = vec![(0, 0, image_width, image_height)];
            while let Some((source_x, source_y, source_width, source_height)) = stack.pop() {
                stats.max_stack_depth = stats.max_stack_depth.max(stack.len() + 1);
                let affine_error_px = nexrad_render_piece_affine_error_px(
                    viewport,
                    width_px,
                    height_px,
                    origin_lon,
                    level_pixel_lon,
                    origin_lat,
                    level_pixel_lat,
                    tile_level_x0 + source_x,
                    tile_level_y0 + source_y,
                    source_width,
                    source_height,
                );
                if source_width > 1
                    && (source_width > NEXRAD_RENDER_MAX_SOURCE_SLICE_PX
                        || affine_error_px > NEXRAD_RENDER_MAX_AFFINE_ERROR_PX)
                    && source_width >= source_height
                {
                    stats.split_count += 1;
                    let left_width = source_width / 2;
                    let right_width = source_width - left_width;
                    stack.push((source_x + left_width, source_y, right_width, source_height));
                    stack.push((source_x, source_y, left_width, source_height));
                    continue;
                }
                if source_height > 1
                    && (source_height > NEXRAD_RENDER_MAX_SOURCE_SLICE_PX
                        || affine_error_px > NEXRAD_RENDER_MAX_AFFINE_ERROR_PX)
                {
                    stats.split_count += 1;
                    let top_height = source_height / 2;
                    let bottom_height = source_height - top_height;
                    stack.push((source_x, source_y + top_height, source_width, bottom_height));
                    stack.push((source_x, source_y, source_width, top_height));
                    continue;
                }

                stats.render_piece_count += 1;
                stats.max_affine_error_px = stats.max_affine_error_px.max(affine_error_px);
                let level_x0 = tile_level_x0 + source_x;
                let level_x1 = level_x0 + source_width;
                let level_y0 = tile_level_y0 + source_y;
                let level_y1 = level_y0 + source_height;
                let lon0 = origin_lon + level_pixel_lon * level_x0 as f64;
                let lon1 = origin_lon + level_pixel_lon * level_x1 as f64;
                let lat0 = origin_lat + level_pixel_lat * level_y0 as f64;
                let lat1 = origin_lat + level_pixel_lat * level_y1 as f64;
                let west = lon0.min(lon1).max(grid_west);
                let east = lon0.max(lon1).min(grid_east);
                let south = lat0.min(lat1).max(grid_south);
                let north = lat0.max(lat1).min(grid_north);
                let corners = NexradOverlayTileCorners {
                    nw: screen_point_for_lat_lon(viewport, width_px, height_px, north, west),
                    ne: screen_point_for_lat_lon(viewport, width_px, height_px, north, east),
                    se: screen_point_for_lat_lon(viewport, width_px, height_px, south, east),
                    sw: screen_point_for_lat_lon(viewport, width_px, height_px, south, west),
                };
                tiles.push(NexradOverlayTile {
                    key: format!(
                        "nexrad/{}/res{}/{}/{}/{}/{}",
                        manifest.state_id, level.res, x, y, source_x, source_y
                    ),
                    src: src.clone(),
                    res: level.res,
                    x,
                    y,
                    source_x,
                    source_y,
                    source_width,
                    source_height,
                    image_width,
                    image_height,
                    corners,
                });
            }
        }
    }

    Ok(NexradOverlayQueryResult {
        status: NexradOverlayStatus::Ready { count: tiles.len() },
        stats,
        tiles,
    })
}

fn nexrad_render_piece_affine_error_px(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    origin_lon: f64,
    level_pixel_lon: f64,
    origin_lat: f64,
    level_pixel_lat: f64,
    level_x0: u32,
    level_y0: u32,
    source_width: u32,
    source_height: u32,
) -> f64 {
    let lon0 = origin_lon + level_pixel_lon * level_x0 as f64;
    let lon1 = origin_lon + level_pixel_lon * (level_x0 + source_width) as f64;
    let lat0 = origin_lat + level_pixel_lat * level_y0 as f64;
    let lat1 = origin_lat + level_pixel_lat * (level_y0 + source_height) as f64;
    let west = lon0.min(lon1);
    let east = lon0.max(lon1);
    let south = lat0.min(lat1);
    let north = lat0.max(lat1);
    let true_center = screen_point_for_lat_lon(
        viewport,
        width_px,
        height_px,
        (north + south) * 0.5,
        (west + east) * 0.5,
    );
    let nw = screen_point_for_lat_lon(viewport, width_px, height_px, north, west);
    let ne = screen_point_for_lat_lon(viewport, width_px, height_px, north, east);
    let se = screen_point_for_lat_lon(viewport, width_px, height_px, south, east);
    let sw = screen_point_for_lat_lon(viewport, width_px, height_px, south, west);
    let affine_center = ScreenPoint {
        x: (nw.x + ne.x + se.x + sw.x) * 0.25,
        y: (nw.y + ne.y + se.y + sw.y) * 0.25,
    };
    ((true_center.x - affine_center.x).powi(2) + (true_center.y - affine_center.y).powi(2)).sqrt()
}

fn nexrad_level_for_viewport(
    levels: &[NexradSourceGridLevel],
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    source_pixel_lon: f64,
    source_pixel_lat: f64,
) -> Option<NexradSourceGridLevel> {
    const NEXRAD_MAX_PIXEL_STRETCH: f64 = 1.5;
    levels
        .iter()
        .filter(|level| {
            nexrad_level_screen_pixel_span(
                viewport,
                width_px,
                height_px,
                source_pixel_lon,
                source_pixel_lat,
                level.res,
            ) <= NEXRAD_MAX_PIXEL_STRETCH
        })
        .max_by_key(|level| level.res)
        .or_else(|| levels.iter().min_by_key(|level| level.res))
        .cloned()
}

fn nexrad_level_screen_pixel_span(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    source_pixel_lon: f64,
    source_pixel_lat: f64,
    res: u32,
) -> f64 {
    let scale = 2_f64.powi(res as i32);
    let center = screen_point_for_lat_lon(
        viewport,
        width_px,
        height_px,
        viewport.center.lat,
        viewport.center.lon,
    );
    let horizontal = screen_point_for_lat_lon(
        viewport,
        width_px,
        height_px,
        viewport.center.lat,
        viewport.center.lon + source_pixel_lon * scale,
    );
    let vertical = screen_point_for_lat_lon(
        viewport,
        width_px,
        height_px,
        viewport.center.lat + source_pixel_lat * scale,
        viewport.center.lon,
    );
    (horizontal.x - center.x)
        .abs()
        .max((vertical.y - center.y).abs())
}

fn viewport_lat_lon_bounds(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> crate::GeoBounds {
    let center_world = lat_lon_to_world_xy(viewport.center.lat, viewport.center.lon);
    let scale = scale_for_map_zoom(viewport.zoom);
    let corners = [
        world_to_lat_lon(
            center_world.0 - width_px / 2.0 / scale,
            center_world.1 - height_px / 2.0 / scale,
        ),
        world_to_lat_lon(
            center_world.0 + width_px / 2.0 / scale,
            center_world.1 - height_px / 2.0 / scale,
        ),
        world_to_lat_lon(
            center_world.0 + width_px / 2.0 / scale,
            center_world.1 + height_px / 2.0 / scale,
        ),
        world_to_lat_lon(
            center_world.0 - width_px / 2.0 / scale,
            center_world.1 + height_px / 2.0 / scale,
        ),
    ];
    crate::GeoBounds {
        south: corners
            .iter()
            .map(|point| point.lat)
            .fold(f64::INFINITY, f64::min),
        west: corners
            .iter()
            .map(|point| point.lon)
            .fold(f64::INFINITY, f64::min),
        north: corners
            .iter()
            .map(|point| point.lat)
            .fold(f64::NEG_INFINITY, f64::max),
        east: corners
            .iter()
            .map(|point| point.lon)
            .fold(f64::NEG_INFINITY, f64::max),
    }
}

fn screen_point_for_lat_lon(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    lat: f64,
    lon: f64,
) -> ScreenPoint {
    let center_world = lat_lon_to_world_xy(viewport.center.lat, viewport.center.lon);
    let point_world = lat_lon_to_world_xy(lat, lon);
    let scale = scale_for_map_zoom(viewport.zoom);
    ScreenPoint {
        x: (point_world.0 - center_world.0) * scale + width_px / 2.0,
        y: (point_world.1 - center_world.1) * scale + height_px / 2.0,
    }
}

fn scale_for_map_zoom(zoom: f64) -> f64 {
    2.0_f64.powf(zoom)
}

fn lat_lon_to_world_xy(lat: f64, lon: f64) -> (f64, f64) {
    const WORLD_SIZE: f64 = 256.0;
    const MAX_LATITUDE: f64 = 85.051_128_78;
    let clamped_lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    let lat_rad = clamped_lat.to_radians();
    (
        ((lon + 180.0) / 360.0) * WORLD_SIZE,
        ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0) * WORLD_SIZE,
    )
}

fn world_to_lat_lon(world_x: f64, world_y: f64) -> LatLon {
    const WORLD_SIZE: f64 = 256.0;
    let lon = world_x / WORLD_SIZE * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * world_y / WORLD_SIZE);
    let lat = n.sinh().atan().to_degrees();
    LatLon { lat, lon }
}

fn terrain_overlay_resources(
    session: &UiSession,
    query: &TerrainOverlayQueryResult,
) -> Vec<CoreResourceRequest> {
    let mut resources = Vec::new();
    for request in &query.tile_requests {
        for source_tile in terrain_source_tiles(request) {
            let key = terrain_source_tile_cache_key(&source_tile.product_id, &source_tile.path);
            if session.terrain_source_tile_cache.contains_key(&key) {
                continue;
            }
            extend_package_resource_requests(
                session,
                &mut resources,
                &format!("terrain/source/{key}"),
                &source_tile.product_id,
                &source_tile.path,
                false,
            );
        }
    }
    dedupe_resource_requests(resources)
}

fn terrain_source_tiles(
    request: &crate::TerrainOverlayTileRequest,
) -> Vec<crate::TerrainOverlaySourceTile> {
    if request.source_tiles.is_empty() {
        vec![crate::TerrainOverlaySourceTile {
            product_id: request.product_id.clone(),
            path: request.path.clone(),
        }]
    } else {
        request.source_tiles.clone()
    }
}

fn terrain_source_tile_cache_key(product_id: &str, path: &str) -> String {
    format!("{product_id}/{path}")
}

pub fn render_terrain_overlay_tile_in_session(
    handle: u32,
    tile_bytes: &[u8],
    aircraft_altitude_ft: Option<f64>,
) -> AppResult<Vec<u8>> {
    let sessions = lock_sessions();
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

pub fn render_terrain_overlay_tile_by_key_in_session(
    handle: u32,
    tile_key: &str,
    aircraft_altitude_ft: Option<f64>,
) -> AppResult<Vec<u8>> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let altitude_ft = aircraft_altitude_ft
        .filter(|altitude| altitude.is_finite())
        .or_else(|| ownship_terrain_altitude_ft(session))
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "ownship altitude unavailable for terrain overlay".to_string(),
        })?;
    let Some(path) = tile_key.strip_prefix("terrain/") else {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("invalid terrain tile key {tile_key}"),
        });
    };
    let tile_bytes = session
        .terrain_source_tile_cache
        .iter()
        .filter_map(|(source_key, bytes)| source_key.strip_suffix(path).map(|_| bytes.as_slice()))
        .collect::<Vec<_>>();
    if tile_bytes.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("terrain tile bytes missing for {tile_key}"),
        });
    }
    crate::render_terrain_warning_raw_rgba_from_tiles(&tile_bytes, altitude_ft).map_err(|err| {
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
    let sessions = lock_sessions();
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
    let _ = lock_sessions().remove(&handle);
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
            let airport: crate::chart_page::PlateAirportRecord = serde_json::from_slice(&bytes)
                .map_err(|err| {
                    HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))
                })?;
            Ok(AirportPlateAvailability {
                plates: !airport.chart_ids.is_empty(),
                csup: airport.chart_ids.iter().any(|chart_id| {
                    chart_id
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("csup:"))
                }),
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
    let normalized_plan = session
        .app_state
        .active_plan
        .clone()
        .ok_or_else(|| missing_active_plan_error("replace flight plan"))?;
    session.guidance_leg_geometry.clear();
    session.chart_page_state = derive_compact_chart_page_state(
        &normalized_plan,
        &session.chart_page_state.recent_airport_ids,
        Some(&session.chart_page_state.selected_airport_id),
        Some(&session.chart_page_state.selected_chart_id),
    );
    sync_procedure_geometry_status_records(session, &normalized_plan);
    Ok(())
}

fn commit_session_flight_plan_with_snapshot_outcome(
    session: &mut UiSession,
    plan: FlightPlan,
) -> AppResult<HadOperationOutcome> {
    let mut candidate = session.clone();
    replace_session_flight_plan(&mut candidate, plan)?;
    match try_snapshot_for_session(&candidate) {
        Ok(snapshot) => {
            *session = candidate;
            serde_json::to_value(snapshot)
                .map(HadOperationOutcome::complete)
                .map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })
        }
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn snapshot_for_session(session: &UiSession) -> AppResult<UiSessionSnapshot> {
    try_snapshot_for_session(session).map_err(|err| match err {
        HadReadError::NeedPages(pages) => AppError {
            kind: AppErrorKind::Internal,
            message: format!(
                "session snapshot requires nav-kv resources in non-paged API: {:?}",
                nav_kv_page_resources(pages)
            ),
        },
        HadReadError::Fatal(message) => AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        },
    })
}

fn session_snapshot_outcome(session: &UiSession) -> AppResult<HadOperationOutcome> {
    match try_snapshot_for_session(session) {
        Ok(snapshot) => serde_json::to_value(snapshot)
            .map(HadOperationOutcome::complete)
            .map_err(|err| AppError {
                kind: AppErrorKind::Internal,
                message: err.to_string(),
            }),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn try_snapshot_for_session(session: &UiSession) -> Result<UiSessionSnapshot, HadReadError> {
    let app_ui_state = project_session_app_ui_state(session)?;
    let debug_state = debug_state_for_app_state(&session.debug_state, &session.app_state);
    Ok(UiSessionSnapshot {
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
        data_status_state: session.data_status_state.clone(),
        debug_state,
        raster_map: session
            .raster_map_catalog
            .as_ref()
            .and_then(crate::raster_map_ui_state),
    })
}

fn debug_state_for_app_state(debug_state: &UiDebugState, app_state: &AppState) -> UiDebugState {
    let mut next = debug_state.clone();
    next.playback_visible = situation_source_handler_for_ownship(&app_state.ownship).is_replay();
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

fn register_default_situation_sources(app_state: AppState) -> AppResult<AppState> {
    let app_state = state::reduce(
        &app_state,
        AppEvent::RegisterOwnshipSource(crate::OwnshipSourceRegistration {
            source_id: crate::OwnshipSourceId(DIRECT_SITUATION_SOURCE_ID.to_string()),
            source_kind: crate::OwnshipSourceKind::FlightPlanSimulator,
            display_name: "Plan Preview".to_string(),
            selectable: true,
            auto_eligible: false,
        }),
    )?;
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

fn register_debug_ownship_driver_source(app_state: AppState) -> AppResult<AppState> {
    let app_state = state::reduce(
        &app_state,
        AppEvent::RegisterOwnshipSource(crate::OwnshipSourceRegistration {
            source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            source_kind: crate::OwnshipSourceKind::DebugOwnshipDriver,
            display_name: "Bad Autopilot".to_string(),
            selectable: true,
            auto_eligible: true,
        }),
    )?;
    state::reduce(
        &app_state,
        AppEvent::UpdateOwnshipSourceStatus(crate::OwnshipSourceStatusUpdate {
            source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            connection_state: crate::SourceConnectionState::Connected,
            enabled: true,
            status_label: "Ready".to_string(),
        }),
    )
}

fn default_map_layer_state() -> UiMapLayerState {
    UiMapLayerState {
        world_basemap: UiMapLayerToggleState {
            visible: true,
            enabled: true,
        },
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
        offline_regions: UiMapLayerToggleState {
            visible: false,
            enabled: true,
        },
    }
}

#[cfg(test)]
fn default_data_status_state() -> UiDataStatusState {
    UiDataStatusState { boxes: Vec::new() }
}

fn default_debug_state() -> UiDebugState {
    UiDebugState {
        tile_labels: false,
        nexrad_tile_labels: false,
        playback_visible: false,
        fast_tiles: false,
        offline_simulated_clock_buttons: false,
        sequencing_finish_lines: false,
    }
}

fn parse_map_layer_id(layer_id: &str) -> AppResult<MapLayerId> {
    match layer_id {
        "vectors" => Ok(MapLayerId::Vectors),
        "world_basemap" => Ok(MapLayerId::WorldBasemap),
        "metars" => Ok(MapLayerId::Metars),
        "nexrad" => Ok(MapLayerId::Nexrad),
        "terrain_warning" => Ok(MapLayerId::TerrainWarning),
        "offline_regions" => Ok(MapLayerId::OfflineRegions),
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
        MapLayerId::WorldBasemap => &mut map_layer_state.world_basemap,
        MapLayerId::Vectors => &mut map_layer_state.vectors,
        MapLayerId::Metars => &mut map_layer_state.metars,
        MapLayerId::Nexrad => &mut map_layer_state.nexrad,
        MapLayerId::TerrainWarning => &mut map_layer_state.terrain_warning,
        MapLayerId::OfflineRegions => &mut map_layer_state.offline_regions,
    }
}

fn empty_map_overlay_query() -> MapOverlayQueryResult {
    MapOverlayQueryResult {
        needed_vector_tiles: Vec::new(),
        needed_metar_tiles: Vec::new(),
        needed_airspace_features: Vec::new(),
        needed_metars: false,
        needed_tfrs: false,
        data_status_records: Vec::new(),
        visible_features: Vec::new(),
        flight_plan_features: Vec::new(),
        visible_metars: Vec::new(),
        visible_pireps: Vec::new(),
        airspace_paths: Vec::new(),
        tfr_paths: Vec::new(),
        airspace_labels: Vec::new(),
        offline_regions: Vec::new(),
    }
}

fn project_session_app_ui_state(session: &UiSession) -> Result<AppUiState, HadReadError> {
    let mut app_ui_state = state::project_app_ui_state(&session.app_state);
    project_debug_ownship_driver_availability(session, &mut app_ui_state);
    if let (Some(store), Some(position)) = (
        session.nav_kv_store.as_ref(),
        app_ui_state.ownship.render.position,
    ) {
        app_ui_state.ownship.render.magnetic_variation_deg =
            crate::had_ops::magnetic_variation_degrees_optional(store, position)?;
    }
    app_ui_state.ownship.controls.situation_controls = project_situation_controls(session);
    if let Some(active_plan) = app_ui_state.active_plan.as_mut() {
        if let Some(guidance) = active_plan.guidance.as_mut() {
            guidance.nav_element =
                project_active_leg_nav_element(session, session.nav_kv_store.as_ref())?;
        }
    }
    if let (Some(store), Some(plan), Some(active_plan)) = (
        session.nav_kv_store.as_ref(),
        session.app_state.active_plan.clone(),
        app_ui_state.active_plan.as_ref(),
    ) {
        app_ui_state.active_plan = Some(flight_plan_ui_state(
            store,
            plan,
            active_plan.clone(),
            crate::FlightDataComputer::new(app_ui_state.ownship.render.speed_kt),
        )?);
    }
    app_ui_state.flight_data_banner = project_flight_data_banner(session, &app_ui_state)?;
    Ok(app_ui_state)
}

fn project_flight_data_banner(
    session: &UiSession,
    app_ui_state: &AppUiState,
) -> Result<crate::FlightDataBannerModel, HadReadError> {
    let ownship = &app_ui_state.ownship.render;
    let position = ownship.position;
    let store = session.nav_kv_store.as_ref();
    let flight_data_computer = crate::FlightDataComputer::new(ownship.speed_kt);

    let altitude_ft = ownship.altitude_msl_ft.or(ownship.pressure_altitude_ft);
    let track_magnetic_deg = match (store, position, ownship.orientation_deg) {
        (Some(store), Some(position), Some(true_course_deg)) => {
            crate::had_ops::true_to_magnetic_course_deg_optional(store, true_course_deg, position)?
        }
        _ => None,
    };

    let mut desired_track_magnetic_deg = None;
    let mut waypoint_distance_nm = None;
    let mut final_distance_nm = None;

    if let (Some(plan), Some(active_leg)) = (
        session.app_state.active_plan.as_ref(),
        session
            .app_state
            .active_plan
            .as_ref()
            .and_then(crate::active_guidance_leg),
    ) {
        if let Some(geometry) =
            active_leg_geometry(plan, &active_leg, &session.guidance_leg_geometry)
        {
            desired_track_magnetic_deg = active_display_course_deg(&geometry, position, store)?;
            waypoint_distance_nm =
                position.map(|position| crate::great_circle_distance_nm(position, geometry.to));
        }

        if let (Some(position), Some(guidance)) = (position, plan.guidance.as_ref()) {
            let records = plan_preview_legs(plan, &session.guidance_leg_geometry);
            let active_index = guidance
                .active_leg_index
                .min(records.len().saturating_sub(1));
            if let Some(record) = records.get(active_index) {
                let active_remaining_nm =
                    crate::great_circle_distance_nm(position, record.geometry.to);
                let later_nm: f64 = records
                    .iter()
                    .skip(active_index + 1)
                    .map(|record| record.distance_nm)
                    .sum();
                final_distance_nm = Some(active_remaining_nm + later_nm);
            }
        }
    }

    Ok(flight_data_computer.banner(crate::FlightDataBannerInput {
        altitude_ft,
        track_magnetic_deg,
        desired_track_magnetic_deg,
        waypoint_distance_nm,
        final_distance_nm,
    }))
}

fn project_debug_ownship_driver_availability(session: &UiSession, app_ui_state: &mut AppUiState) {
    let available = debug_ownship_driver_available(session);
    for source in &mut app_ui_state.ownship.controls.sources {
        if source.source_kind == crate::OwnshipSourceKind::DebugOwnshipDriver {
            source.enabled = available;
            if !available {
                source.tone = crate::ownship::OwnshipControlTone::Unavailable;
                source.status_label = "No active leg".to_string();
            }
        }
    }
}

fn debug_ownship_driver_available(session: &UiSession) -> bool {
    active_guidance_detail_geometry(session).is_some()
}

fn project_situation_controls(session: &UiSession) -> Vec<SituationControlMenuItem> {
    situation_source_handler_for_session(session).menu_items(session)
}

trait SessionSituationSourceHandler {
    fn is_replay(&self) -> bool {
        false
    }

    fn apply_input(
        &self,
        _session: &mut UiSession,
        _input: SituationControlInput,
        _now_epoch_ms: f64,
    ) -> AppResult<()> {
        Ok(())
    }

    fn input_enabled(&self, _session: &UiSession, _input: SituationControlInput) -> bool {
        false
    }

    fn menu_items(&self, session: &UiSession) -> Vec<SituationControlMenuItem> {
        vec![
            SituationControlMenuItem {
                input: SituationControlInput::SkipBackward,
                label: "⏮".to_string(),
                enabled: self.input_enabled(session, SituationControlInput::SkipBackward),
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastRewind,
                label: "⏪".to_string(),
                enabled: self.input_enabled(session, SituationControlInput::FastRewind),
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastForward,
                label: "⏩".to_string(),
                enabled: self.input_enabled(session, SituationControlInput::FastForward),
            },
            SituationControlMenuItem {
                input: SituationControlInput::SkipForward,
                label: "⏭".to_string(),
                enabled: self.input_enabled(session, SituationControlInput::SkipForward),
            },
        ]
    }
}

struct NullSituationSourceHandler;
struct LiveSituationSourceHandler;
struct ReplaySituationSourceHandler;
struct PlanPreviewSituationSourceHandler;

impl SessionSituationSourceHandler for NullSituationSourceHandler {}
impl SessionSituationSourceHandler for LiveSituationSourceHandler {}

impl SessionSituationSourceHandler for ReplaySituationSourceHandler {
    fn is_replay(&self) -> bool {
        true
    }

    fn apply_input(
        &self,
        session: &mut UiSession,
        input: SituationControlInput,
        now_epoch_ms: f64,
    ) -> AppResult<()> {
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
        Ok(())
    }

    fn input_enabled(&self, session: &UiSession, input: SituationControlInput) -> bool {
        let ui_state = session.playback.ui_state();
        match input {
            SituationControlInput::SkipBackward | SituationControlInput::FastRewind => {
                ui_state.cursor_seconds > 1e-6
            }
            SituationControlInput::FastForward | SituationControlInput::SkipForward => {
                ui_state.duration_seconds - ui_state.cursor_seconds > 1e-6
            }
        }
    }
}

impl SessionSituationSourceHandler for PlanPreviewSituationSourceHandler {
    fn apply_input(
        &self,
        session: &mut UiSession,
        input: SituationControlInput,
        _now_epoch_ms: f64,
    ) -> AppResult<()> {
        apply_plan_preview_input(session, input)
    }

    fn input_enabled(&self, session: &UiSession, input: SituationControlInput) -> bool {
        let (can_rewind, can_forward) = plan_preview_control_bounds(session);
        match input {
            SituationControlInput::SkipBackward | SituationControlInput::FastRewind => can_rewind,
            SituationControlInput::FastForward | SituationControlInput::SkipForward => can_forward,
        }
    }
}

static NULL_SITUATION_SOURCE_HANDLER: NullSituationSourceHandler = NullSituationSourceHandler;
static LIVE_SITUATION_SOURCE_HANDLER: LiveSituationSourceHandler = LiveSituationSourceHandler;
static REPLAY_SITUATION_SOURCE_HANDLER: ReplaySituationSourceHandler = ReplaySituationSourceHandler;
static PLAN_PREVIEW_SITUATION_SOURCE_HANDLER: PlanPreviewSituationSourceHandler =
    PlanPreviewSituationSourceHandler;

fn situation_source_handler_for_session(
    session: &UiSession,
) -> &'static dyn SessionSituationSourceHandler {
    situation_source_handler_for_ownship(&session.app_state.ownship)
}

fn situation_source_handler_for_ownship(
    ownship: &crate::OwnshipState,
) -> &'static dyn SessionSituationSourceHandler {
    match selected_ownship_source_kind(ownship) {
        Some(crate::OwnshipSourceKind::FlightPlanSimulator) => {
            &PLAN_PREVIEW_SITUATION_SOURCE_HANDLER
        }
        Some(kind) if is_replay_ownship_source(kind) => &REPLAY_SITUATION_SOURCE_HANDLER,
        Some(_) => &LIVE_SITUATION_SOURCE_HANDLER,
        None => &NULL_SITUATION_SOURCE_HANDLER,
    }
}

fn project_active_leg_nav_element(
    session: &UiSession,
    store: Option<&NavKvStore>,
) -> Result<NavElementUiView, HadReadError> {
    let Some(plan) = session.app_state.active_plan.as_ref() else {
        return Ok(NavElementUiView::default());
    };
    let Some(active_leg) = crate::active_guidance_leg(plan) else {
        return Ok(NavElementUiView::default());
    };
    let Some(geometry) = active_leg_geometry(plan, &active_leg, &session.guidance_leg_geometry)
    else {
        return Ok(NavElementUiView {
            active_leg_summary: format!(
                "{} -> {}",
                nav_ref_label(&active_leg.from),
                nav_ref_label(&active_leg.to)
            ),
            cdi_indicator_dots: None,
            cdi_offscale_readout: None,
        });
    };
    let position = session.app_state.ownship.render.position;
    let course_deg = active_display_course_deg(&geometry, position, store)?;
    let cdi_indicator_dots = session
        .app_state
        .ownship
        .render
        .position
        .map(|position| cdi_dots_for_guidance_geometry(&geometry, position));
    let cdi_offscale_readout = cdi_indicator_dots.and_then(cdi_offscale_readout);
    let active_leg_summary = active_guidance_leg_summary(plan, &active_leg);

    let active_leg_summary = if let Some(course_deg) = course_deg {
        format!(
            "{} CRS {}",
            active_leg_summary,
            crate::flight_data::format_course_degrees(course_deg)
        )
    } else {
        active_leg_summary
    };

    Ok(NavElementUiView {
        active_leg_summary,
        cdi_indicator_dots,
        cdi_offscale_readout,
    })
}

fn active_guidance_leg_summary(plan: &FlightPlan, active_leg: &PlanLeg) -> String {
    if active_guidance_detail_is_terminal_hold(plan) {
        "HOLD".to_string()
    } else {
        format!(
            "{} -> {}",
            nav_ref_label(&active_leg.from),
            nav_ref_label(&active_leg.to)
        )
    }
}

fn active_guidance_detail_is_terminal_hold(plan: &FlightPlan) -> bool {
    plan.guidance.as_ref().is_some_and(|guidance| {
        guidance.active_detail_index.is_some_and(|detail_index| {
            terminal_hold_detail_range(plan, guidance.active_leg_index).is_some_and(
                |(hold_start, hold_end)| detail_index >= hold_start && detail_index <= hold_end,
            )
        })
    })
}

fn active_leg_geometry(
    plan: &FlightPlan,
    active_leg: &PlanLeg,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    if let (Some(from), Some(to)) = (
        nav_ref_embedded_position(&active_leg.from),
        nav_ref_embedded_position(&active_leg.to),
    ) {
        return Some(GuidanceLegGeometry {
            leg_id: "__latlon_leg__".to_string(),
            from,
            to,
            path: vec![from, to],
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
                    NavRef::LatLon(position) | NavRef::Spot(position) => *position,
                    _ => geometry.from,
                };
                let to = match &active_leg.to {
                    NavRef::LatLon(position) | NavRef::Spot(position) => *position,
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
    pointer_key: String,
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
        row_uid: record.pointer_key.clone(),
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
                .any(|record| record.pointer_key == pointer.row_uid)
        });
    if !pointer_on_plan
        && matches!(
            input,
            SituationControlInput::SkipBackward | SituationControlInput::SkipForward
        )
    {
        let record = records[0].clone();
        session.plan_preview.pointer = Some(PlanPreviewPointer {
            row_uid: record.pointer_key.clone(),
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
        row_uid: record.pointer_key.clone(),
        offset_nm,
    });
    apply_plan_preview_pointer(session, record, offset_nm)
}

const PLAN_PREVIEW_FAST_STEP_NM: f64 = 20.0;

fn plan_preview_control_bounds(session: &UiSession) -> (bool, bool) {
    let records = session
        .app_state
        .active_plan
        .as_ref()
        .map(|plan| plan_preview_legs(plan, &session.guidance_leg_geometry))
        .unwrap_or_default();
    if records.is_empty() {
        return (false, false);
    }
    let Some(pointer) = session.plan_preview.pointer.as_ref() else {
        return (false, true);
    };
    let Some(record_index) = records
        .iter()
        .position(|record| record.pointer_key == pointer.row_uid)
    else {
        return (false, true);
    };
    let offset_nm = pointer
        .offset_nm
        .clamp(0.0, records[record_index].distance_nm);
    let at_start = record_index == 0 && offset_nm <= 1e-6;
    let at_end =
        record_index + 1 == records.len() && offset_nm >= records[record_index].distance_nm - 1e-6;
    (!at_start, !at_end)
}

fn resolve_plan_preview_pointer(
    state: &PlanPreviewState,
    records: &[PlanPreviewLeg],
) -> (usize, f64) {
    let Some(pointer) = state.pointer.as_ref() else {
        return (0, 0.0);
    };
    records
        .iter()
        .position(|record| record.pointer_key == pointer.row_uid)
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

fn tick_debug_ownship_driver(session: &mut UiSession, now_epoch_ms: f64) -> AppResult<()> {
    if selected_ownship_source_kind(&session.app_state.ownship)
        != Some(crate::OwnshipSourceKind::DebugOwnshipDriver)
    {
        return Ok(());
    }
    session.debug_ownship_driver.running = true;

    let Some((detail_id, geometry)) = active_guidance_detail_geometry(session) else {
        session.debug_ownship_driver.last_tick_epoch_ms = Some(now_epoch_ms);
        return Ok(());
    };
    let distance_nm = geometry_distance_nm(&geometry);
    if distance_nm <= f64::EPSILON {
        session.debug_ownship_driver.last_tick_epoch_ms = Some(now_epoch_ms);
        return Ok(());
    }

    let dt_seconds = session
        .debug_ownship_driver
        .last_tick_epoch_ms
        .map(|last_tick| {
            ((now_epoch_ms - last_tick) / 1000.0).clamp(0.0, DEBUG_OWNSHIP_DRIVER_MAX_DT_SECONDS)
        })
        .unwrap_or(0.0);
    session.debug_ownship_driver.last_tick_epoch_ms = Some(now_epoch_ms);

    if session.debug_ownship_driver.active_detail_id.as_deref() != Some(detail_id.as_str()) {
        session.debug_ownship_driver.active_detail_id = Some(detail_id);
        session.debug_ownship_driver.offset_nm = 0.0;
    }

    session.debug_ownship_driver.offset_nm = (session.debug_ownship_driver.offset_nm
        + dt_seconds * DEBUG_OWNSHIP_DRIVER_NM_PER_SECOND)
        .min(distance_nm + DEBUG_OWNSHIP_DRIVER_OVERRUN_NM);
    session.debug_ownship_driver.wander_phase_rad += dt_seconds * 0.7;

    let offset_nm = session.debug_ownship_driver.offset_nm;
    let heading = heading_along_geometry(&geometry, offset_nm)
        .unwrap_or_else(|| bearing_degrees(geometry.from, geometry.to));
    let base_position = position_along_geometry_with_overrun(&geometry, offset_nm);
    let wander_nm =
        DEBUG_OWNSHIP_DRIVER_WANDER_NM * session.debug_ownship_driver.wander_phase_rad.sin();
    let position = project_nm_from(base_position, heading + 90.0, wander_nm);
    let motion_heading = session
        .debug_ownship_driver
        .last_position
        .filter(|last_position| crate::great_circle_distance_nm(*last_position, position) > 1e-4)
        .map(|last_position| bearing_degrees(last_position, position))
        .unwrap_or(heading);
    session.debug_ownship_driver.last_position = Some(position);

    apply_situation_to_ownship(
        session,
        DEBUG_OWNSHIP_DRIVER_SOURCE_ID,
        crate::OwnshipSourceKind::DebugOwnshipDriver,
        "Bad Autopilot",
        crate::Situation {
            position: crate::SituationPosition::LatLon {
                lat: position.lat,
                lon: position.lon,
            },
            orientation_deg: Some(motion_heading),
            speed_kt: Some(
                DEBUG_OWNSHIP_DRIVER_NM_PER_SECOND
                    * 3600.0
                    * DEBUG_OWNSHIP_DRIVER_REPORTED_SPEED_SCALE,
            ),
            altitude_msl_ft: None,
        },
        now_epoch_ms as i64,
    )
}

fn active_guidance_detail_geometry(session: &UiSession) -> Option<(String, GuidanceLegGeometry)> {
    let plan = session.app_state.active_plan.as_ref()?;
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        let active_leg = crate::active_guidance_leg(plan)?;
        let geometry = active_leg_geometry(plan, &active_leg, &session.guidance_leg_geometry)?;
        return Some((geometry.leg_id.clone(), geometry));
    }
    let active_detail_index = active_guidance_detail_index_for_motion(plan, guidance)?;
    active_guidance_detail_geometry_for_index(
        plan,
        active_detail_index,
        &session.guidance_leg_geometry,
    )
}

fn active_guidance_detail_index_for_motion(
    plan: &FlightPlan,
    guidance: &GuidanceState,
) -> Option<usize> {
    match guidance.sequencing_mode {
        SequencingMode::FollowPlan => guidance
            .active_detail_index
            .or_else(|| first_guidance_detail_index_for_leg(plan, guidance.active_leg_index)),
        SequencingMode::Suspended => {
            let active_detail_index = guidance.active_detail_index?;
            terminal_hold_detail_range(plan, guidance.active_leg_index)
                .filter(|(hold_start, hold_end)| {
                    active_detail_index >= *hold_start && active_detail_index <= *hold_end
                })
                .map(|_| active_detail_index)
        }
        SequencingMode::DirectTo => None,
    }
}

fn active_guidance_detail_geometry_for_index(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<(String, GuidanceLegGeometry)> {
    let mut current_index = 0usize;
    for leg in &plan.resolved_legs {
        let detail_count = crate::guidance_detail_count_for_leg(leg);
        if active_detail_index < current_index + detail_count {
            let element_index = active_detail_index - current_index;
            let detail_id = guidance_detail_id_for_leg_element(leg, element_index);
            let geometry = geometry_by_leg_id
                .get(&detail_id)
                .cloned()
                .or_else(|| geometry_by_leg_id.get(&leg.id).cloned())
                .or_else(|| geometry_for_resolved_leg(leg, geometry_by_leg_id))?;
            return Some((detail_id, geometry));
        }
        current_index += detail_count;
    }
    None
}

fn plan_preview_legs(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Vec<PlanPreviewLeg> {
    let mut component_leg_counts = HashMap::<usize, usize>::new();
    for leg in &plan.resolved_legs {
        match leg.source {
            ResolvedLegSource::RouteComponent { component_index }
            | ResolvedLegSource::SyntheticBridge {
                from_component_index: component_index,
                ..
            } => {
                *component_leg_counts.entry(component_index).or_insert(0) += 1;
            }
            ResolvedLegSource::LegacyPlanLeg { .. } => {}
        }
    }
    plan.resolved_legs
        .iter()
        .filter_map(|leg| {
            let pointer_key = pointer_key_for_preview_leg(plan, leg, &component_leg_counts)?;
            let geometry = geometry_for_resolved_leg(leg, geometry_by_leg_id)?;
            let distance_nm = geometry_distance_nm(&geometry);
            Some(PlanPreviewLeg {
                pointer_key,
                geometry,
                distance_nm,
            })
        })
        .collect()
}

fn pointer_key_for_preview_leg(
    plan: &FlightPlan,
    leg: &ResolvedLeg,
    component_leg_counts: &HashMap<usize, usize>,
) -> Option<String> {
    match leg.source {
        ResolvedLegSource::RouteComponent { component_index }
        | ResolvedLegSource::SyntheticBridge {
            from_component_index: component_index,
            ..
        } => {
            if component_leg_counts
                .get(&component_index)
                .copied()
                .unwrap_or(0)
                > 1
            {
                Some(format!("guidance-leg:{}", leg.id))
            } else {
                plan.route_component_uids.get(component_index).cloned()
            }
        }
        ResolvedLegSource::LegacyPlanLeg { leg_index } => Some(format!("legacy:{leg_index}:from")),
    }
}

fn geometry_for_resolved_leg(
    leg: &ResolvedLeg,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    let detail_count = crate::guidance_detail_count_for_leg(leg);
    if detail_count > 1 {
        let mut detail_geometries = Vec::with_capacity(detail_count);
        for element_index in 0..detail_count {
            detail_geometries.push(
                geometry_by_leg_id.get(&guidance_detail_id_for_leg_element(leg, element_index))?,
            );
        }
        let from = detail_geometries.first()?.from;
        let to = detail_geometries.last()?.to;
        let mut path = Vec::new();
        for geometry in detail_geometries {
            for point in geometry_points(geometry) {
                if path.last().copied() != Some(point) {
                    path.push(point);
                }
            }
        }
        return Some(GuidanceLegGeometry {
            leg_id: leg.id.clone(),
            from,
            to,
            path,
        });
    }

    if let Some(geometry) = geometry_by_leg_id
        .get(&guidance_detail_id_for_leg_element(leg, 0))
        .or_else(|| geometry_by_leg_id.get(&leg.id))
    {
        return Some(geometry.clone());
    }
    if let (Some(from), Some(to)) = (
        nav_ref_embedded_position(&leg.from),
        nav_ref_embedded_position(&leg.to),
    ) {
        return Some(GuidanceLegGeometry {
            leg_id: leg.id.clone(),
            from,
            to,
            path: vec![from, to],
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

fn position_along_geometry_with_overrun(geometry: &GuidanceLegGeometry, offset_nm: f64) -> LatLon {
    let distance_nm = geometry_distance_nm(geometry);
    if offset_nm <= distance_nm {
        return position_along_geometry(geometry, offset_nm);
    }
    let endpoint = position_along_geometry(geometry, distance_nm);
    let heading = heading_along_geometry(geometry, distance_nm)
        .unwrap_or_else(|| bearing_degrees(geometry.from, geometry.to));
    project_nm_from(endpoint, heading, offset_nm - distance_nm)
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

fn active_display_course_deg(
    geometry: &GuidanceLegGeometry,
    position: Option<LatLon>,
    store: Option<&NavKvStore>,
) -> Result<Option<f64>, HadReadError> {
    let (from, to) = position
        .and_then(|position| nearest_guidance_segment(geometry, position))
        .unwrap_or((geometry.from, geometry.to));
    let true_course_deg = bearing_degrees(from, to);
    let Some(store) = store else {
        return Ok(None);
    };
    let variation_position = position.unwrap_or_else(|| midpoint_for_variation(from, to));
    crate::had_ops::true_to_magnetic_course_deg_optional(store, true_course_deg, variation_position)
}

fn midpoint_for_variation(from: LatLon, to: LatLon) -> LatLon {
    crate::great_circle_intermediate(from, to, 0.5)
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

fn project_nm_from(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    let angular_distance = distance_nm / 3440.065;
    let bearing_rad = bearing_deg.to_radians();
    let lat1 = origin.lat.to_radians();
    let lon1 = origin.lon.to_radians();
    let lat2 = (lat1.sin() * angular_distance.cos()
        + lat1.cos() * angular_distance.sin() * bearing_rad.cos())
    .asin();
    let lon2 = lon1
        + (bearing_rad.sin() * angular_distance.sin() * lat1.cos())
            .atan2(angular_distance.cos() - lat1.sin() * lat2.sin());
    LatLon {
        lat: lat2.to_degrees(),
        lon: lon2.to_degrees(),
    }
}

fn nav_ref_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(code) | NavRef::Navaid(code) | NavRef::Fix(code) => code.clone(),
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => {
            identifier.clone()
        }
        NavRef::LatLon(position) => format!("{:.4},{:.4}", position.lat, position.lon),
        NavRef::Spot(_) => "SPOT".to_string(),
    }
}

fn nav_ref_embedded_position(nav_ref: &NavRef) -> Option<LatLon> {
    match nav_ref {
        NavRef::LatLon(position) | NavRef::Spot(position) => Some(*position),
        _ => None,
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
    sequence_guidance_by_ownship_position(session)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct SequencingFinishPlane {
    point: LatLon,
    normal_course_deg: f64,
}

#[derive(Debug, Clone, Copy)]
enum SequencingFinishCriterion {
    Plane(SequencingFinishPlane),
    ArcSector {
        center: LatLon,
        finish_point: LatLon,
        finish_bearing_deg: f64,
        untraveled_mid_bearing_deg: f64,
        clockwise: bool,
    },
}

fn sequence_guidance_by_ownship_position(session: &mut UiSession) -> AppResult<()> {
    let Some(position) = session.app_state.ownship.render.position else {
        return Ok(());
    };
    for _ in 0..16 {
        let Some(plan) = session.app_state.active_plan.as_ref() else {
            return Ok(());
        };
        let Some(guidance) = plan.guidance.as_ref() else {
            return Ok(());
        };
        let Some(active_detail_index) = active_guidance_detail_index_for_motion(plan, guidance)
        else {
            return Ok(());
        };
        let suspended_hold = guidance.sequencing_mode == SequencingMode::Suspended;
        let Some(finish_criterion) = active_detail_finish_criterion(
            plan,
            active_detail_index,
            &session.guidance_leg_geometry,
            suspended_hold,
        ) else {
            return Ok(());
        };
        if !position_satisfies_finish_criterion(position, finish_criterion) {
            return Ok(());
        }
        let next_plan = if suspended_hold {
            sequence_suspended_terminal_hold_detail(plan)?
        } else {
            crate::sequence_active_detail(plan)?
        };
        session.app_state =
            state::reduce(&session.app_state, AppEvent::ReplaceFlightPlan(next_plan))?;
    }
    Ok(())
}

fn active_detail_finish_criterion(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
    wrap_terminal_hold: bool,
) -> Option<SequencingFinishCriterion> {
    let current_id = guidance_detail_id_for_index(plan, active_detail_index)?;
    let current = geometry_by_leg_id.get(&current_id)?;
    if let Some(arc_criterion) = active_detail_arc_finish_criterion(plan, active_detail_index) {
        return Some(arc_criterion);
    }
    let current_course = terminal_course_for_guidance_geometry(current)?;
    let next_detail_index = if wrap_terminal_hold {
        next_terminal_hold_detail_index(plan, active_detail_index)
    } else {
        active_detail_index.checked_add(1)
    };
    let next_course = next_detail_index
        .and_then(|detail_index| guidance_detail_id_for_index(plan, detail_index))
        .and_then(|next_id| geometry_by_leg_id.get(&next_id))
        .and_then(initial_course_for_guidance_geometry)
        .unwrap_or(current_course);
    Some(SequencingFinishCriterion::Plane(SequencingFinishPlane {
        point: current.to,
        normal_course_deg: finish_line_normal_course_deg(current_course, next_course),
    }))
}

fn active_detail_arc_finish_criterion(
    plan: &FlightPlan,
    active_detail_index: usize,
) -> Option<SequencingFinishCriterion> {
    let detail = crate::planning::guidance_detail_ref_by_index(plan, active_detail_index)?;
    let leg = plan.resolved_legs.get(detail.leg_index)?;
    let element = leg
        .procedure_provenance
        .as_ref()?
        .display_path
        .as_ref()?
        .elements
        .get(detail.element_index)?;
    let LegDisplayElement::Arc {
        center,
        start: _,
        end,
        clockwise,
        sweep_degrees,
        ..
    } = element
    else {
        return None;
    };
    let sweep = sweep_degrees.abs().min(360.0);
    let untraveled_sweep = 360.0 - sweep;
    if untraveled_sweep <= 1e-6 {
        return None;
    }
    let finish_bearing_deg = bearing_degrees(*center, *end);
    let untraveled_mid_bearing_deg = if *clockwise {
        normalize_course_degrees(finish_bearing_deg + untraveled_sweep / 2.0)
    } else {
        normalize_course_degrees(finish_bearing_deg - untraveled_sweep / 2.0)
    };
    Some(SequencingFinishCriterion::ArcSector {
        center: *center,
        finish_point: *end,
        finish_bearing_deg,
        untraveled_mid_bearing_deg,
        clockwise: *clockwise,
    })
}

fn sequence_suspended_terminal_hold_detail(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot sequence suspended hold without guidance state".to_string(),
    })?;
    let active_detail_index = guidance.active_detail_index.ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "suspended hold requires an active guidance detail".to_string(),
    })?;
    let (hold_start, hold_end) = terminal_hold_detail_range(plan, guidance.active_leg_index)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "suspended guidance detail is not in a terminal hold".to_string(),
        })?;
    if active_detail_index < hold_start || active_detail_index > hold_end {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "suspended guidance detail is outside the terminal hold".to_string(),
        });
    }
    let next_detail_index = if active_detail_index >= hold_end {
        hold_start
    } else {
        active_detail_index + 1
    };

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            active_detail_index: Some(next_detail_index),
            display_split_leg_id: guidance.display_split_leg_id.clone(),
            sequencing_mode: SequencingMode::Suspended,
            direct_to: None,
            suspend_reason: guidance.suspend_reason,
        }),
        ..plan.clone()
    })
}

fn terminal_hold_detail_range(plan: &FlightPlan, leg_index: usize) -> Option<(usize, usize)> {
    let hold_start = crate::terminal_hold_start_detail_index_for_leg(plan, leg_index)?;
    let leg = plan.resolved_legs.get(leg_index)?;
    let first_detail = first_guidance_detail_index_for_leg(plan, leg_index)?;
    let detail_count = crate::guidance_detail_count_for_leg(leg);
    detail_count
        .checked_sub(1)
        .map(|last_offset| (hold_start, first_detail + last_offset))
}

fn next_terminal_hold_detail_index(plan: &FlightPlan, active_detail_index: usize) -> Option<usize> {
    let active_detail = crate::planning::guidance_detail_ref_by_index(plan, active_detail_index)?;
    let (hold_start, hold_end) = terminal_hold_detail_range(plan, active_detail.leg_index)?;
    if active_detail_index < hold_start || active_detail_index > hold_end {
        return active_detail_index.checked_add(1);
    }
    Some(if active_detail_index >= hold_end {
        hold_start
    } else {
        active_detail_index + 1
    })
}

fn initial_course_for_guidance_geometry(geometry: &GuidanceLegGeometry) -> Option<f64> {
    geometry_points(geometry)
        .windows(2)
        .find(|segment| crate::great_circle_distance_nm(segment[0], segment[1]) > f64::EPSILON)
        .map(|segment| bearing_degrees(segment[0], segment[1]))
}

fn terminal_course_for_guidance_geometry(geometry: &GuidanceLegGeometry) -> Option<f64> {
    geometry_points(geometry)
        .windows(2)
        .rev()
        .find(|segment| crate::great_circle_distance_nm(segment[0], segment[1]) > f64::EPSILON)
        .map(|segment| bearing_degrees(segment[0], segment[1]))
}

fn finish_line_normal_course_deg(inbound_course_deg: f64, outbound_course_deg: f64) -> f64 {
    let inbound = inbound_course_deg.to_radians();
    let outbound = outbound_course_deg.to_radians();
    let x = inbound.sin() + outbound.sin();
    let y = inbound.cos() + outbound.cos();
    if x.hypot(y) < 1e-9 {
        normalize_course_degrees(inbound_course_deg)
    } else {
        normalize_course_degrees(x.atan2(y).to_degrees())
    }
}

fn normalize_course_degrees(course_deg: f64) -> f64 {
    course_deg.rem_euclid(360.0)
}

fn signed_distance_beyond_finish_plane(
    position: LatLon,
    finish_plane: SequencingFinishPlane,
) -> f64 {
    let lat_rad = finish_plane.point.lat.to_radians();
    let east_nm = (position.lon - finish_plane.point.lon).to_radians() * lat_rad.cos() * 3440.065;
    let north_nm = (position.lat - finish_plane.point.lat).to_radians() * 3440.065;
    let normal = finish_plane.normal_course_deg.to_radians();
    east_nm * normal.sin() + north_nm * normal.cos()
}

fn position_satisfies_finish_criterion(
    position: LatLon,
    finish_criterion: SequencingFinishCriterion,
) -> bool {
    match finish_criterion {
        SequencingFinishCriterion::Plane(finish_plane) => {
            crate::great_circle_distance_nm(position, finish_plane.point) <= 10.0
                && signed_distance_beyond_finish_plane(position, finish_plane) > 0.05
        }
        SequencingFinishCriterion::ArcSector {
            center,
            finish_point,
            finish_bearing_deg,
            untraveled_mid_bearing_deg,
            clockwise,
        } => {
            if crate::great_circle_distance_nm(position, finish_point) > 10.0 {
                return false;
            }
            let bearing = bearing_degrees(center, position);
            bearing_is_in_arc_finish_sector(
                bearing,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                clockwise,
            )
        }
    }
}

fn bearing_is_in_arc_finish_sector(
    bearing_deg: f64,
    finish_bearing_deg: f64,
    untraveled_mid_bearing_deg: f64,
    clockwise: bool,
) -> bool {
    let sector_width = if clockwise {
        clockwise_delta_degrees(finish_bearing_deg, untraveled_mid_bearing_deg)
    } else {
        clockwise_delta_degrees(untraveled_mid_bearing_deg, finish_bearing_deg)
    };
    let distance_into_sector = if clockwise {
        clockwise_delta_degrees(finish_bearing_deg, bearing_deg)
    } else {
        clockwise_delta_degrees(bearing_deg, finish_bearing_deg)
    };
    distance_into_sector <= sector_width + 1e-6
}

fn clockwise_delta_degrees(from_deg: f64, to_deg: f64) -> f64 {
    (normalize_course_degrees(to_deg) - normalize_course_degrees(from_deg)).rem_euclid(360.0)
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
        AirportId, FlightPlan, GuidanceState, LegDisplayElement, LegDisplayPath,
        LegDisplayPathStyle, NavRef, OwnshipSourceId, OwnshipSourceKind, PathTermination,
        ProcedureLegProvenance, ProcedureSegmentRole, ResolvedLeg, ResolvedLegSource,
        RouteComponent, SequencingMode, Situation, SituationPosition, SituationSample,
    };

    #[test]
    fn spot_cdi_label_omits_coordinates() {
        assert_eq!(
            nav_ref_label(&NavRef::Spot(LatLon {
                lat: 47.626,
                lon: -122.194,
            })),
            "SPOT"
        );
    }

    fn minimal_vector_manifest_json() -> &'static str {
        r#"{
            "point_layers": {
                "airport": { "available_zooms": [9] },
                "fix": { "available_zooms": [9] },
                "nav": { "available_zooms": [9] }
            },
            "airspace": {
                "reference_tile_min_zoom": 0,
                "reference_tile_max_zoom": 0,
                "label_tile_min_zoom": 0,
                "label_tile_max_zoom": 0
            }
        }"#
    }

    #[test]
    fn selecting_procedure_for_initial_airport_reaches_nav_lookup_without_predecessor_error() {
        let plan = FlightPlan {
            id: "khvr-only".to_string(),
            name: "KHVR".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KHVR".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KHVR".to_string())),
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        let row_uid = init
            .snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("active plan")
            .display_rows
            .iter()
            .find(|row| row.label == "KHVR")
            .expect("KHVR row")
            .uid
            .clone();

        let err = select_procedure_at_flight_plan_row_in_session(
            init.handle,
            row_uid,
            "KHVR".to_string(),
            "R26".to_string(),
            crate::ProcedureKind::Approach,
            None,
            Some("ISITE".to_string()),
        )
        .expect_err("missing nav store should still fail");

        assert!(
            err.message.contains("nav kv store"),
            "initial procedure selection should not fail predecessor repair first: {err:?}"
        );
    }

    #[test]
    fn missing_live_feed_products_do_not_abort_vector_overlay_resources() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        ingest_resource_in_session(
            init.handle,
            "publication/current_artifacts",
            br#"{
                "schema_version": 1,
                "artifact_roots": {
                    "packaged": "published_packaged",
                    "unpacked": "published_unpacked"
                },
                "bundles": []
            }"#,
        )
        .expect("ingest current artifacts");
        let mut overlay = empty_map_overlay_query();
        overlay.needed_metars = true;
        overlay.needed_tfrs = true;
        overlay.needed_metar_tiles.push(crate::VectorTileRequest {
            layer: "metars".to_string(),
            z: 6,
            x: 8,
            y: 22,
        });

        let requests = {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            weather_overlay_resources(session, &overlay)
        };
        assert!(
            requests
                .iter()
                .all(|request| !matches!(request.source, crate::CoreResourceSource::Unavailable { .. })),
            "missing live-feed products must not poison unrelated overlay resource loading: {requests:?}"
        );
        assert!(requests.is_empty());
    }

    #[test]
    fn pending_live_feed_updates_do_not_block_vector_overlay_resources() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session
                .live_feeds
                .ingest_resource(
                    "live_feeds/current",
                    br#"{
                        "products": {
                            "metars": {
                                "current": "v1",
                                "version_manifest_url": "versions/metars/v1.json",
                                "state_url": "states/metars/v1.json",
                                "state_sha256": "unused"
                            },
                            "tfrs": {
                                "current": "v1",
                                "version_manifest_url": "versions/tfrs/v1.json",
                                "state_url": "states/tfrs/v1.json",
                                "state_sha256": "unused"
                            }
                        }
                    }"#,
                )
                .expect("current manifest");
        }
        let mut overlay = empty_map_overlay_query();
        overlay.needed_metars = true;
        overlay.needed_tfrs = true;

        let requests = {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            weather_overlay_resources(session, &overlay)
        };

        assert!(
            requests.is_empty(),
            "map overlay should render available vectors/weather without waiting for live-feed freshness: {requests:?}"
        );
    }

    #[test]
    fn live_feed_metars_build_their_own_tile_index() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
                    "airport": { "available_zooms": [9] },
                    "fix": { "available_zooms": [9] },
                    "nav": { "available_zooms": [9] },
                    "metars": {
                        "min_zoom": 5,
                        "max_zoom": 7,
                        "available_zooms": [5, 6, 7],
                        "tile_path_template": "unused-by-live-feeds"
                    }
                },
                "airspace": {
                    "reference_tile_min_zoom": 0,
                    "reference_tile_max_zoom": 0,
                    "label_tile_min_zoom": 0,
                    "label_tile_max_zoom": 0
                }
            }"#,
        )
        .expect("metar layer config");
        let mut metars_by_station = HashMap::new();
        metars_by_station.insert(
            "KAAA".to_string(),
            crate::MetarRecord {
                raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KAAA".to_string(),
                flight_category: Some("MVFR".to_string()),
                clouds: Some(crate::map_overlay::MetarClouds {
                    symbol: Some("SCT".to_string()),
                }),
                longitude: 0.0,
                latitude: 0.0,
            },
        );
        let payload = MetarProductPayload {
            schema_version: 3,
            version_label: "v1".to_string(),
            metar_count: Some(1),
            metars_by_station,
            pireps: Vec::new(),
        };
        let metar_tile_cache = metar_tile_cache_for_live_feed(
            &payload,
            config.metar_layer.as_ref(),
            &HashSet::from(["KAAA".to_string()]),
        );
        let result = crate::query_map_overlay(
            &MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 8.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            240.0,
            240.0,
            &config,
            false,
            true,
            &[],
            None,
            &HashMap::new(),
            &metar_tile_cache,
            Some(&payload),
            &HashMap::new(),
            None,
        );

        assert_eq!(result.visible_metars.len(), 1);
        assert_eq!(result.visible_metars[0].station_id, "KAAA");
        assert_eq!(result.visible_metars[0].flight_category, "mvfr");
    }

    #[test]
    fn live_metar_layer_survives_vector_manifest_without_weather_layers() {
        let mut session = UiSession {
            app_state: register_default_situation_sources(AppState::default()).expect("app state"),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            debug_ownship_driver: DebugOwnshipDriverState::default(),
            map_follow: MapFollowSessionState::default(),
            guidance_leg_geometry: HashMap::new(),
            map_overlay_config: map_overlay_config_from_vector_manifest_json(
                minimal_vector_manifest_json(),
            )
            .expect("bootstrap manifest"),
            vector_manifest_loaded: false,
            chart_page_state: derive_compact_chart_page_state(
                &FlightPlan::default(),
                &[],
                None,
                None,
            ),
            nav_kv_store_id: None,
            nav_kv_store: None,
            map_layer_state: default_map_layer_state(),
            data_status_records: BTreeMap::new(),
            hushed_status_ids: BTreeSet::new(),
            data_status_state: default_data_status_state(),
            debug_state: default_debug_state(),
            resource_policy: CoreResourcePolicy::InstalledPackage,
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            live_feeds: LiveFeedsState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            towered_metar_station_ids: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
        };
        let mut metars_by_station = HashMap::new();
        metars_by_station.insert(
            "KAAA".to_string(),
            crate::MetarRecord {
                raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KAAA".to_string(),
                flight_category: Some("MVFR".to_string()),
                clouds: None,
                longitude: 0.0,
                latitude: 0.0,
            },
        );
        session.metar_payload = Some(MetarProductPayload {
            schema_version: 3,
            version_label: "v1".to_string(),
            metar_count: Some(1),
            metars_by_station,
            pireps: Vec::new(),
        });
        session.towered_metar_station_ids = Some(HashSet::from(["KAAA".to_string()]));
        let manifest = serde_json::json!({
            "point_layers": {
                "airport": {
                    "available_zooms": [9]
                },
                "fix": {
                    "available_zooms": [9]
                },
                "nav": {
                    "available_zooms": [9]
                }
            },
            "airspace": {
                "reference_tile_min_zoom": 0,
                "reference_tile_max_zoom": 0,
                "label_tile_min_zoom": 0,
                "label_tile_max_zoom": 0
            }
        });
        install_vector_manifest_config(&mut session, &manifest.to_string())
            .expect("load vector manifest");

        assert!(session.map_overlay_config.metar_layer.is_some());
        assert!(!session.metar_tile_cache.is_empty());
    }

    #[test]
    fn live_metar_low_zoom_keeps_only_important_stations() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
                    "airport": { "available_zooms": [9] },
                    "fix": { "available_zooms": [9] },
                    "nav": { "available_zooms": [9] },
                    "metars": {
                        "min_zoom": 5,
                        "max_zoom": 7,
                        "available_zooms": [5, 6, 7],
                        "tile_path_template": "unused-by-live-feeds"
                    }
                },
                "airspace": {
                    "reference_tile_min_zoom": 0,
                    "reference_tile_max_zoom": 0,
                    "label_tile_min_zoom": 0,
                    "label_tile_max_zoom": 0
                }
            }"#,
        )
        .expect("metar layer config");
        let mut metars_by_station = HashMap::new();
        for (station_id, lat, lon) in [("KAAA", 0.0, 0.0), ("KBBB", 0.1, 0.1)] {
            metars_by_station.insert(
                station_id.to_string(),
                crate::MetarRecord {
                    raw_text: format!("METAR {station_id} 010000Z 00000KT 10SM SCT020 10/08 A3000"),
                    observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                    station_id: station_id.to_string(),
                    flight_category: Some("VFR".to_string()),
                    clouds: None,
                    longitude: lon,
                    latitude: lat,
                },
            );
        }
        let payload = MetarProductPayload {
            schema_version: 3,
            version_label: "v1".to_string(),
            metar_count: Some(2),
            metars_by_station,
            pireps: Vec::new(),
        };

        let cache = metar_tile_cache_for_live_feed(
            &payload,
            config.metar_layer.as_ref(),
            &HashSet::from(["KAAA".to_string()]),
        );
        let low_zoom_records = cache
            .values()
            .filter(|tile| tile.z == 5)
            .flat_map(|tile| tile.records.iter())
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();
        let high_zoom_records = cache
            .values()
            .filter(|tile| tile.z == 7)
            .flat_map(|tile| tile.records.iter())
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(low_zoom_records, vec!["KAAA"]);
        assert!(high_zoom_records.contains(&"KAAA"));
        assert!(high_zoom_records.contains(&"KBBB"));
    }

    #[test]
    fn metar_station_importance_comes_from_nav_db_towered_airports() {
        let store = crate::navkv::nav_kv_store_for_test(
            &[
                (
                    "navref/symbol/airport/KAAA",
                    br#"{"kind":"airport","id":"KAAA","towered":true}"#,
                ),
                (
                    "navref/symbol/airport/KBBB",
                    br#"{"kind":"airport","id":"KBBB","towered":false}"#,
                ),
                (
                    "navref/symbol/airport/KCCC",
                    br#"{"kind":"airport","id":"KCCC"}"#,
                ),
            ],
            256,
        );

        let station_ids =
            towered_metar_station_ids_from_nav_db(&store).expect("towered station ids");

        assert_eq!(station_ids, HashSet::from(["KAAA".to_string()]));
    }

    #[test]
    fn malformed_live_tfr_state_records_data_status_without_failing_overlay() {
        let mut session = UiSession {
            app_state: register_default_situation_sources(AppState::default()).expect("app state"),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            debug_ownship_driver: DebugOwnshipDriverState::default(),
            map_follow: MapFollowSessionState::default(),
            guidance_leg_geometry: HashMap::new(),
            map_overlay_config: map_overlay_config_from_vector_manifest_json(
                minimal_vector_manifest_json(),
            )
            .expect("bootstrap manifest"),
            vector_manifest_loaded: false,
            chart_page_state: derive_compact_chart_page_state(
                &FlightPlan::default(),
                &[],
                None,
                None,
            ),
            nav_kv_store_id: None,
            nav_kv_store: None,
            map_layer_state: default_map_layer_state(),
            data_status_records: BTreeMap::new(),
            hushed_status_ids: BTreeSet::new(),
            data_status_state: default_data_status_state(),
            debug_state: default_debug_state(),
            resource_policy: CoreResourcePolicy::InstalledPackage,
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            live_feeds: LiveFeedsState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            towered_metar_station_ids: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
        };
        let bad_tfr_state = serde_json::json!({
            "schema_version": 1,
            "version_label": "bad",
            "notam_count": 1,
            "area_group_count": 1,
            "areas": [{
                "notam_id": "6/0001",
                "area_index": 0,
                "schedule_fragments": [],
                "upper_limit": { "value_text": "100", "unit": "MSL" },
                "lower_limit": { "value_text": "SFC", "unit": "SFC" },
                "polygon": [
                    { "lat": 47.0, "lon": -122.0 },
                    { "lat": 47.0, "lon": -121.9 },
                    { "lat": 47.1, "lon": -121.9 }
                ]
            }]
        });
        let bad_tfr_state_sha = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(
                serde_json::to_string(&bad_tfr_state)
                    .expect("state json")
                    .as_bytes(),
            );
            format!("{:x}", hasher.finalize())
        };
        session
            .live_feeds
            .ingest_resource(
                "live_feeds/current",
                format!(
                    r#"{{
                    "products": {{
                        "tfrs": {{
                            "current": "bad",
                            "version_manifest_url": "versions/tfrs/bad.json",
                            "state_url": "states/tfrs/bad.json",
                            "state_sha256": "{}"
                        }}
                    }}
                }}"#,
                    bad_tfr_state_sha
                )
                .as_bytes(),
            )
            .expect("current manifest");
        session
            .live_feeds
            .ingest_resource(
                "live_feeds/version/tfrs/bad",
                format!(
                    r#"{{
                    "product": "tfrs",
                    "version": "bad",
                    "state": {{
                        "url": "states/tfrs/bad.json",
                        "state_sha256": "{}"
                    }}
                }}"#,
                    bad_tfr_state_sha
                )
                .as_bytes(),
            )
            .expect("version manifest");
        session
            .live_feeds
            .ingest_resource(
                "live_feeds/state/tfrs/bad",
                &serde_json::to_vec(&bad_tfr_state).expect("state json"),
            )
            .expect("state manifest");

        install_live_feed_payloads(&mut session).expect("install should degrade");

        assert!(session.tfr_payload.is_none());
        assert!(session
            .data_status_records
            .values()
            .any(|record| record.id == LIVE_FEED_TFRS_STATUS_ID
                && record.detail.contains("summary_text")));
    }

    #[test]
    fn failed_live_feed_current_records_nexrad_caution_when_nexrad_layer_visible() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            FlightPlan::default(),
            &[],
            None,
            None,
        )
        .expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");

        let snapshot = report_session_resource_failure_in_session(
            init.handle,
            "live_feeds/current",
            "failed to fetch /live-feeds/current.json: 404",
        )
        .expect("report failure");

        let nexrad = snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == LIVE_FEED_NEXRAD_STATUS_ID)
            .expect("nexrad caution");
        assert_eq!(nexrad.label, "NEXRAD");
        assert!(nexrad.drives_caution);
        assert!(nexrad.detail.contains("Live feed index unavailable"));
    }

    #[test]
    fn hiding_nexrad_layer_clears_nexrad_caution() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            FlightPlan::default(),
            &[],
            None,
            None,
        )
        .expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        report_session_resource_failure_in_session(init.handle, "live_feeds/current", "404")
            .expect("report failure");

        let snapshot =
            set_map_layer_visibility_in_session(init.handle, "nexrad", false).expect("hide nexrad");

        assert!(!snapshot
            .data_status_state
            .boxes
            .iter()
            .any(|box_| box_.id == LIVE_FEED_NEXRAD_STATUS_ID));
    }

    #[test]
    fn visible_nexrad_without_product_state_records_caution() {
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            FlightPlan::default(),
            &[],
            None,
            None,
        )
        .expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        ingest_resource_in_session(init.handle, "live_feeds/current", br#"{"products":{}}"#)
            .expect("ingest empty current manifest");

        let outcome = get_nexrad_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon {
                    lat: 47.0,
                    lon: -122.0,
                },
                zoom: 8.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1024.0,
            768.0,
        )
        .expect("query nexrad");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete nexrad query");
        };
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("nexrad result");
        assert!(matches!(
            query.status,
            NexradOverlayStatus::Unavailable { .. }
        ));
        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        assert!(snapshot
            .data_status_state
            .boxes
            .iter()
            .any(|box_| box_.id == LIVE_FEED_NEXRAD_STATUS_ID
                && box_.detail.contains("missing from the live feed index")));
    }

    #[test]
    fn procedure_data_quality_in_flight_plan_drives_caution() {
        let mut plan = FlightPlan {
            id: "procedure-quality".to_string(),
            name: "Procedure quality".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "RNAV-A".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: Some("TRANS".to_string()),
                    terminal_discontinuity: None,
                    data_quality: vec!["Procedure encoding is suspicious; read plate.".to_string()],
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: Some(AirportId("KAAA".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let init = create_ui_session(
            minimal_vector_manifest_json(),
            plan.clone(),
            &[],
            None,
            None,
        )
        .expect("create session");

        let warning = init
            .snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == "procedure_geometry:row-proc")
            .expect("procedure geometry caution");
        assert_eq!(warning.label, "PROC");
        assert_eq!(warning.value.as_deref(), Some("RNAV-A"));
        assert!(warning.drives_caution);
        assert!(warning.detail.contains("KAAA RNAV-A TRANS"));
        assert!(warning.detail.contains("Procedure encoding is suspicious"));

        plan.route_components.clear();
        plan.route_component_uids.clear();
        let snapshot =
            replace_flight_plan_in_session(init.handle, plan).expect("replace procedure plan");

        assert!(!snapshot
            .data_status_state
            .boxes
            .iter()
            .any(|box_| box_.id.starts_with(PROCEDURE_GEOMETRY_STATUS_PREFIX)));
    }

    fn complete_session_snapshot(outcome: HadOperationOutcome) -> UiSessionSnapshot {
        match outcome {
            HadOperationOutcome::Complete { result, .. } => {
                serde_json::from_value(result).expect("session snapshot outcome")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("unexpected session snapshot resource request: {resources:?}")
            }
        }
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

    fn enabled_situation_controls(snapshot: &UiSessionSnapshot) -> Vec<SituationControlInput> {
        snapshot
            .app_ui_state
            .ownship
            .controls
            .situation_controls
            .iter()
            .filter(|control| control.enabled)
            .map(|control| control.input)
            .collect()
    }

    fn assert_enabled_situation_controls(
        snapshot: &UiSessionSnapshot,
        expected: &[SituationControlInput],
    ) {
        assert_eq!(enabled_situation_controls(snapshot), expected);
    }

    fn ownship_source_label_index(snapshot: &UiSessionSnapshot, label: &str) -> usize {
        snapshot
            .app_ui_state
            .ownship
            .controls
            .sources
            .iter()
            .position(|source| source.label == label)
            .unwrap_or_else(|| panic!("missing ownship source label {label:?}"))
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
    fn cdi_course_formatter_uses_aviation_north() {
        assert_eq!(crate::flight_data::format_course_degrees(0.0), "360");
        assert_eq!(crate::flight_data::format_course_degrees(359.6), "360");
        assert_eq!(crate::flight_data::format_course_degrees(0.4), "360");
        assert_eq!(crate::flight_data::format_course_degrees(1.0), "001");
        assert_eq!(crate::flight_data::format_course_degrees(-1.0), "359");
    }

    #[test]
    fn cdi_display_course_uses_local_east_magnetic_declination() {
        let store = crate::navkv::nav_kv_store_for_test(
            &[
                ("magvar/48/-110", b"14"),
                ("magvar/48/-109", b"14"),
                ("magvar/49/-110", b"14"),
                ("magvar/49/-109", b"14"),
            ],
            256,
        );
        let position = LatLon {
            lat: 48.54,
            lon: -109.76,
        };

        assert_eq!(
            crate::flight_data::format_course_degrees(
                crate::had_ops::true_to_magnetic_course_deg_optional(&store, 271.0, position)
                    .unwrap()
                    .unwrap()
            ),
            "257"
        );
    }

    fn empty_test_plan() -> FlightPlan {
        FlightPlan {
            id: "empty-test-plan".to_string(),
            name: "Empty".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn ownship_render_exposes_magnetic_variation_for_compass_rose() {
        let store = crate::navkv::nav_kv_store_for_test(
            &[
                ("magvar/48/-110", b"14"),
                ("magvar/48/-109", b"14"),
                ("magvar/49/-110", b"14"),
                ("magvar/49/-109", b"14"),
            ],
            256,
        );
        let init = create_ui_session(empty_test_plan(), &[], None, None).expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 48.54,
                    lon: -109.76,
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

        assert_eq!(
            snapshot.app_ui_state.ownship.render.magnetic_variation_deg,
            Some(14.0)
        );
    }

    #[test]
    fn ownship_render_omits_magnetic_variation_without_nav_kv_store() {
        let init = create_ui_session(empty_test_plan(), &[], None, None).expect("create session");

        let snapshot = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 48.54,
                    lon: -109.76,
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

        assert_eq!(
            snapshot.app_ui_state.ownship.render.magnetic_variation_deg,
            None
        );
    }

    #[test]
    fn ownship_render_omits_magnetic_variation_when_had_keys_are_missing() {
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let init = create_ui_session(empty_test_plan(), &[], None, None).expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 37.5,
                    lon: -122.4,
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
        .expect("missing magvar should not abort ownship sample projection");

        assert_eq!(
            snapshot.app_ui_state.ownship.render.magnetic_variation_deg,
            None
        );
    }

    #[test]
    fn magnetic_variation_wraps_dateline_through_had_keys() {
        let store = crate::navkv::nav_kv_store_for_test(
            &[
                ("magvar/0/-180", b"9"),
                ("magvar/0/-179", b"3"),
                ("magvar/1/-180", b"9"),
                ("magvar/1/-179", b"3"),
            ],
            256,
        );

        assert_eq!(
            crate::had_ops::magnetic_variation_degrees_optional(
                &store,
                LatLon {
                    lat: 0.0,
                    lon: 180.0,
                }
            )
            .unwrap(),
            Some(9.0)
        );
    }

    #[test]
    fn magnetic_variation_reports_had_pages_when_not_faulted_in() {
        let store = crate::navkv::nav_kv_store_without_pages_for_test(
            &[
                ("magvar/48/-110", b"14"),
                ("magvar/48/-109", b"14"),
                ("magvar/49/-110", b"14"),
                ("magvar/49/-109", b"14"),
            ],
            256,
        );

        let err = crate::had_ops::magnetic_variation_degrees_optional(
            &store,
            LatLon {
                lat: 48.54,
                lon: -109.76,
            },
        )
        .expect_err("missing magvar pages should fault");

        assert!(
            matches!(&err, HadReadError::NeedPages(pages) if !pages.is_empty()),
            "expected HAD page fault, got {err:?}"
        );
    }

    #[test]
    fn replay_source_is_selectable_and_controls_playback_panel_visibility() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
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
            ownship_source_label_index(&init.snapshot, "Plan\nPreview")
                < ownship_source_label_index(&init.snapshot, "Replay"),
            "Plan Preview should sort before Replay",
        );
        assert!(
            !init.snapshot.debug_state.playback_visible,
            "playback panel starts hidden until Replay is active",
        );
        assert_enabled_situation_controls(&init.snapshot, &[]);

        let replay = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId(PLAYBACK_SOURCE_ID.to_string()),
            },
        )
        .expect("select Replay");
        assert!(replay.debug_state.playback_visible);
        assert_enabled_situation_controls(&replay, &[]);

        let replay_with_trace = load_playback_trace_in_session(
            init.handle,
            "test-trace.json",
            r#"{"trace":[[0.0,10.0,20.0,0,100.0,90.0],[120.0,10.1,20.1,0,100.0,90.0]]}"#,
        )
        .expect("load replay trace");
        assert_enabled_situation_controls(
            &replay_with_trace,
            &[
                SituationControlInput::FastForward,
                SituationControlInput::SkipForward,
            ],
        );

        let replay_at_end =
            seek_playback_in_session(init.handle, 120.0, 0.0).expect("seek replay to end");
        assert_enabled_situation_controls(
            &replay_at_end,
            &[
                SituationControlInput::SkipBackward,
                SituationControlInput::FastRewind,
            ],
        );

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
        assert_enabled_situation_controls(
            &gps,
            &[
                SituationControlInput::SkipBackward,
                SituationControlInput::FastRewind,
            ],
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
        assert!(
            ownship_source_label_index(&gps, "GPS")
                < ownship_source_label_index(&gps, "Plan\nPreview")
                && ownship_source_label_index(&gps, "Plan\nPreview")
                    < ownship_source_label_index(&gps, "Replay"),
            "GPS, Plan Preview, and Replay should keep their relative menu order",
        );
        assert_enabled_situation_controls(&gps, &[]);
    }

    #[test]
    fn session_projects_cdi_from_injected_guidance_geometry() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
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
        let init = create_ui_session(sample_duplicate_waypoint_plan(), &[], None, None)
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
        let direct_to_action = crate::planning::flight_plan_row_actions(clicked_row)
            .find(|action| action.id == FlightPlanRowActionId::DirectTo)
            .expect("direct-to action");

        let after_direct_to = perform_flight_plan_row_action_in_session(
            init.handle,
            clicked_row.uid.clone(),
            direct_to_action.uid.clone(),
        )
        .expect("direct-to row action");
        let after_direct_to = complete_session_snapshot(after_direct_to);
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
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
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
        let activate_leg = crate::planning::flight_plan_row_actions(&target_row)
            .find(|action| action.id == FlightPlanRowActionId::ActivateLeg)
            .expect("activate-leg action")
            .clone();
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
        let after_activate_leg = complete_session_snapshot(after_activate_leg);
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
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        let snapshot = select_plan_preview(init.handle);
        let position = ownship_position(&snapshot);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.0);
    }

    #[test]
    fn plan_preview_controls_stop_at_waypoints_and_plan_ends() {
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        let selected = select_plan_preview(init.handle);
        assert_enabled_situation_controls(
            &selected,
            &[
                SituationControlInput::SkipBackward,
                SituationControlInput::FastRewind,
                SituationControlInput::FastForward,
                SituationControlInput::SkipForward,
            ],
        );

        let after_skip_end = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip to active leg end");
        let position = ownship_position(&after_skip_end);
        assert_near(position.lat, 41.0);
        assert_near(position.lon, -119.0);
        assert_enabled_situation_controls(
            &after_skip_end,
            &[
                SituationControlInput::SkipBackward,
                SituationControlInput::FastRewind,
            ],
        );

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
        assert_enabled_situation_controls(
            &after_skip_start,
            &[
                SituationControlInput::SkipBackward,
                SituationControlInput::FastRewind,
                SituationControlInput::FastForward,
                SituationControlInput::SkipForward,
            ],
        );

        let after_previous = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipBackward,
            0.0,
        )
        .expect("skip to previous waypoint");
        let position = ownship_position(&after_previous);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -120.0);
        assert_enabled_situation_controls(
            &after_previous,
            &[
                SituationControlInput::FastForward,
                SituationControlInput::SkipForward,
            ],
        );
    }

    #[test]
    fn plan_preview_skip_from_off_plan_returns_to_first_waypoint() {
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        select_plan_preview(init.handle);
        {
            let mut sessions = lock_sessions();
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
    fn plan_preview_recovers_after_newer_bad_autopilot_samples() {
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        select_plan_preview(init.handle);
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");
        tick_debug_ownship_driver_in_session(init.handle, 10_000.0).expect("tick bad autopilot");

        let snapshot = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DIRECT_SITUATION_SOURCE_ID.to_string()),
            },
        )
        .expect("select plan preview after bad autopilot");
        assert_eq!(
            snapshot.app_ui_state.ownship.render.mode,
            crate::OwnshipMode::Simulated
        );
        let position = ownship_position(&snapshot);
        assert_near(position.lat, 40.0);
        assert_near(position.lon, -119.0);

        let after_skip = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            11_000.0,
        )
        .expect("skip plan preview after bad autopilot");
        assert_near(ownship_position(&after_skip).lat, 41.0);
        assert_near(ownship_position(&after_skip).lon, -119.0);
    }

    #[test]
    fn plan_preview_skip_forward_from_waypoint_moves_to_next_waypoint() {
        let mut plan = lat_lon_preview_plan();
        plan.guidance.as_mut().expect("guidance").active_leg_index = 0;
        let init = create_ui_session(plan, &[], None, None).expect("create session");
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
        let init = create_ui_session(short_lat_lon_preview_plan(), &[], None, None)
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
    fn plan_preview_keys_multi_leg_components_by_guidance_leg() {
        let plan = FlightPlan {
            id: "multi-leg-procedure-preview".to_string(),
            name: "multi-leg procedure preview".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(LatLon {
                        lat: 40.0,
                        lon: -120.0,
                    }),
                },
                RouteComponent::Procedure {
                    procedure: crate::ProcedureSegment {
                        airport_id: crate::AirportId("KAAA".to_string()),
                        procedure_id: "TEST".to_string(),
                        kind: ProcedureKind::Approach,
                        runway_transition: None,
                        enroute_transition: None,
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
            ],
            route_component_uids: vec!["row-a".to_string(), "row-proc".to_string()],
            route_component_uid_counter: 2,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "proc-1".to_string(),
                    from: NavRef::LatLon(LatLon {
                        lat: 40.0,
                        lon: -120.0,
                    }),
                    to: NavRef::LatLon(LatLon {
                        lat: 40.0,
                        lon: -119.0,
                    }),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-2".to_string(),
                    from: NavRef::LatLon(LatLon {
                        lat: 40.0,
                        lon: -119.0,
                    }),
                    to: NavRef::LatLon(LatLon {
                        lat: 41.0,
                        lon: -119.0,
                    }),
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
        };

        let records = plan_preview_legs(&plan, &HashMap::new());

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].pointer_key, "guidance-leg:proc-1");
        assert_eq!(records[1].pointer_key, "guidance-leg:proc-2");
    }

    #[test]
    fn plan_preview_uses_whole_guidance_leg_for_multi_element_procedure_leg() {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.8,
        };
        let c = LatLon {
            lat: 40.1,
            lon: -119.7,
        };
        let d = LatLon {
            lat: 40.2,
            lon: -119.6,
        };
        let leg = ResolvedLeg {
            id: "proc-multi-element".to_string(),
            from: NavRef::LatLon(a),
            to: NavRef::LatLon(d),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KAAA".to_string(),
                procedure_id: "TEST".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment { start: a, end: b },
                        LegDisplayElement::Segment { start: b, end: c },
                        LegDisplayElement::Segment { start: c, end: d },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            id: "multi-element-procedure-preview".to_string(),
            name: "multi-element procedure preview".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![leg.clone()],
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
        };
        let geometry_by_leg_id = HashMap::from([
            (
                guidance_detail_id_for_leg_element(&leg, 0),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
            ),
            (
                guidance_detail_id_for_leg_element(&leg, 1),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ),
            (
                guidance_detail_id_for_leg_element(&leg, 2),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 2),
                    from: c,
                    to: d,
                    path: vec![c, d],
                },
            ),
        ]);

        let records = plan_preview_legs(&plan, &geometry_by_leg_id);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].geometry.from, a);
        assert_eq!(records[0].geometry.to, d);
        assert_eq!(records[0].geometry.path, vec![a, b, c, d]);
        assert!(records[0].distance_nm > crate::great_circle_distance_nm(a, b));
    }

    #[test]
    fn plan_preview_skip_forward_reaches_multi_element_guidance_leg_end() {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.8,
        };
        let c = LatLon {
            lat: 40.1,
            lon: -119.7,
        };
        let d = LatLon {
            lat: 40.2,
            lon: -119.6,
        };
        let leg = ResolvedLeg {
            id: "proc-multi-element-live".to_string(),
            from: NavRef::LatLon(a),
            to: NavRef::LatLon(d),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KAAA".to_string(),
                procedure_id: "TEST".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment { start: a, end: b },
                        LegDisplayElement::Segment { start: b, end: c },
                        LegDisplayElement::Segment { start: c, end: d },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            id: "multi-element-procedure-preview-live".to_string(),
            name: "multi-element procedure preview live".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![leg.clone()],
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
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 2),
                    from: c,
                    to: d,
                    path: vec![c, d],
                },
            ],
        )
        .expect("install guidance geometry");
        select_plan_preview(init.handle);

        let snapshot = apply_situation_control_input_in_session(
            init.handle,
            SituationControlInput::SkipForward,
            0.0,
        )
        .expect("skip to guidance leg end");

        let position = ownship_position(&snapshot);
        assert_near(position.lat, d.lat);
        assert_near(position.lon, d.lon);
    }

    #[test]
    fn debug_ownship_driver_advances_guidance_detail_in_core() {
        let mut plan = short_lat_lon_preview_plan();
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.999,
        };
        let c = LatLon {
            lat: 40.001,
            lon: -119.999,
        };
        plan.resolved_legs = vec![
            ResolvedLeg {
                id: "driver-leg-a-b".to_string(),
                from: NavRef::LatLon(a),
                to: NavRef::LatLon(b),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "driver-leg-b-c".to_string(),
                from: NavRef::LatLon(b),
                to: NavRef::LatLon(c),
                source: ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: None,
            },
        ];
        plan.guidance = Some(GuidanceState {
            active_leg_index: 0,
            active_detail_index: Some(0),
            display_split_leg_id: None,
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        });

        let init = create_ui_session(plan, &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: "driver-leg-a-b#0".to_string(),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: "driver-leg-b-c".to_string(),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ],
        )
        .expect("install guidance geometry");
        let mut snapshot = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");
        for now_epoch_ms in [
            1_000.0, 2_000.0, 3_000.0, 4_000.0, 5_000.0, 6_000.0, 7_000.0, 8_000.0, 9_000.0,
            10_000.0,
        ] {
            snapshot = tick_debug_ownship_driver_in_session(init.handle, now_epoch_ms)
                .expect("tick driver");
        }
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_detail_index, Some(1));
        assert_eq!(guidance.active_leg_index, 1);
        assert!(
            snapshot
                .app_ui_state
                .ownship
                .render
                .position
                .expect("driver position")
                .lat
                > b.lat,
            "driver must continue along the next leg even when it only has coarse geometry"
        );
    }

    #[test]
    fn ownship_sequencing_does_not_skip_over_large_hold_entry_arc() {
        let center = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let arc_start = project_nm_from(center, 0.0, 1.0);
        let arc_end = project_nm_from(center, 210.0, 1.0);
        let inbound_end = project_nm_from(arc_end, 60.0, 2.0);
        let outbound_start = project_nm_from(arc_start, 270.0, 2.0);
        let just_past_outbound_finish = project_nm_from(arc_start, 90.0, 0.2);
        let leg = ResolvedLeg {
            id: "hold-entry".to_string(),
            from: NavRef::LatLon(outbound_start),
            to: NavRef::LatLon(inbound_end),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KAAA".to_string(),
                procedure_id: "TEST".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment {
                            start: outbound_start,
                            end: arc_start,
                        },
                        LegDisplayElement::Arc {
                            center,
                            radius_nm: 1.0,
                            start: arc_start,
                            end: arc_end,
                            clockwise: true,
                            sweep_degrees: 210.0,
                        },
                        LegDisplayElement::Segment {
                            start: arc_end,
                            end: inbound_end,
                        },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            id: "large-hold-entry-arc".to_string(),
            name: "large hold entry arc".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![leg.clone()],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: Some(leg.id.clone()),
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
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 0),
                    from: outbound_start,
                    to: arc_start,
                    path: vec![outbound_start, arc_start],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 1),
                    from: arc_start,
                    to: arc_end,
                    path: vec![
                        arc_start,
                        project_nm_from(center, 70.0, 1.0),
                        project_nm_from(center, 140.0, 1.0),
                        arc_end,
                    ],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&leg, 2),
                    from: arc_end,
                    to: inbound_end,
                    path: vec![arc_end, inbound_end],
                },
            ],
        )
        .expect("install guidance geometry");

        set_situation_in_session(
            init.handle,
            crate::Situation {
                position: crate::SituationPosition::LatLon {
                    lat: just_past_outbound_finish.lat,
                    lon: just_past_outbound_finish.lon,
                },
                orientation_deg: Some(90.0),
                speed_kt: Some(120.0),
                altitude_msl_ft: None,
            },
        )
        .expect("push ownship sample");
        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");

        assert_eq!(
            guidance.active_detail_index,
            Some(1),
            "crossing the outbound finish line should activate the 210-degree arc, not skip to the inbound leg"
        );
    }

    #[test]
    fn debug_ownship_driver_flies_terminal_hold_until_unsuspended() {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.98,
        };
        let c = LatLon {
            lat: 40.01,
            lon: -119.98,
        };
        let d = LatLon {
            lat: 40.01,
            lon: -120.0,
        };
        let e = LatLon {
            lat: 39.99,
            lon: -120.0,
        };
        let f = LatLon {
            lat: 40.0,
            lon: -119.80,
        };
        let hold_leg = ResolvedLeg {
            id: "proc-terminal-hold".to_string(),
            from: NavRef::LatLon(a),
            to: NavRef::LatLon(b),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KAAA".to_string(),
                procedure_id: "TEST".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment { start: a, end: b },
                        LegDisplayElement::Segment { start: b, end: c },
                        LegDisplayElement::Segment { start: c, end: d },
                        LegDisplayElement::Segment { start: d, end: e },
                        LegDisplayElement::Segment { start: e, end: b },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let exit_leg = ResolvedLeg {
            id: "after-hold".to_string(),
            from: NavRef::LatLon(b),
            to: NavRef::LatLon(f),
            source: ResolvedLegSource::RouteComponent { component_index: 1 },
            procedure_provenance: None,
        };
        let plan = FlightPlan {
            id: "bad-ap-terminal-hold".to_string(),
            name: "bad ap terminal hold".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Procedure {
                    procedure: crate::ProcedureSegment {
                        airport_id: AirportId("KAAA".to_string()),
                        procedure_id: "TEST".to_string(),
                        kind: ProcedureKind::Approach,
                        runway_transition: None,
                        enroute_transition: None,
                        terminal_discontinuity: Some(ProcedureDiscontinuity::Hold),
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(f),
                },
            ],
            route_component_uids: vec!["row-proc".to_string(), "row-exit".to_string()],
            route_component_uid_counter: 2,
            resolved_legs: vec![hold_leg.clone(), exit_leg.clone()],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: Some(hold_leg.id.clone()),
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
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&hold_leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&hold_leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&hold_leg, 2),
                    from: c,
                    to: d,
                    path: vec![c, d],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&hold_leg, 3),
                    from: d,
                    to: e,
                    path: vec![d, e],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&hold_leg, 4),
                    from: e,
                    to: b,
                    path: vec![e, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(&exit_leg, 0),
                    from: b,
                    to: f,
                    path: vec![b, f],
                },
            ],
        )
        .expect("install guidance geometry");
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");

        let mut snapshot =
            tick_debug_ownship_driver_in_session(init.handle, 1_000.0).expect("tick driver");
        for second in 2..=4 {
            let now_epoch_ms = f64::from(second) * 1000.0;
            snapshot = tick_debug_ownship_driver_in_session(init.handle, now_epoch_ms)
                .expect("tick driver");
        }
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(1));
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        let nav_element = {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            project_active_leg_nav_element(session, None).expect("nav element")
        };
        assert_eq!(nav_element.active_leg_summary, "HOLD");
        assert!(
            nav_element.cdi_indicator_dots.is_some(),
            "hold CDI should present deviation from active hold detail"
        );

        for second in 5..=8 {
            let now_epoch_ms = f64::from(second) * 1000.0;
            snapshot = tick_debug_ownship_driver_in_session(init.handle, now_epoch_ms)
                .expect("tick suspended hold");
        }
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_leg_index, 0);
        assert!(guidance.active_detail_index.unwrap_or(0) >= 2);
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);

        unsuspend_sequencing_in_session(init.handle).expect("unsuspend hold");
        for second in 9..=30 {
            let now_epoch_ms = f64::from(second) * 1000.0;
            snapshot = tick_debug_ownship_driver_in_session(init.handle, now_epoch_ms)
                .expect("tick unsuspended hold");
        }
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn debug_ownship_driver_falls_back_to_guidance_leg_geometry() {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.999,
        };
        let c = LatLon {
            lat: 40.001,
            lon: -119.999,
        };
        let leg = ResolvedLeg {
            id: "driver-multi-leg".to_string(),
            from: NavRef::LatLon(a),
            to: NavRef::LatLon(c),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KAAA".to_string(),
                procedure_id: "TEST".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment { start: a, end: b },
                        LegDisplayElement::Segment { start: b, end: c },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            id: "bad-ap-aggregate-geometry".to_string(),
            name: "bad ap aggregate geometry".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![leg.clone()],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
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
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![GuidanceLegGeometry {
                leg_id: "driver-multi-leg".to_string(),
                from: a,
                to: c,
                path: vec![a, b, c],
            }],
        )
        .expect("install aggregate guidance geometry");
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DEBUG_OWNSHIP_DRIVER_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");

        let snapshot =
            tick_debug_ownship_driver_in_session(init.handle, 1_000.0).expect("tick driver");
        assert!(
            snapshot.app_ui_state.ownship.render.position.is_some(),
            "bad autopilot must keep moving when active detail geometry falls back to leg geometry"
        );
    }

    #[test]
    fn plan_preview_pointer_follows_row_uid_after_reorder() {
        let mut plan = lat_lon_preview_plan();
        plan.guidance.as_mut().expect("guidance").active_leg_index = 0;
        let init = create_ui_session(plan, &[], None, None).expect("create session");
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

    #[test]
    fn nexrad_mesh_error_budget_splits_coarse_source_grid_pieces() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 48.0,
                lon: -101.0,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let full_res3_error = nexrad_render_piece_affine_error_px(
            &viewport, 1024.0, 768.0, -130.0, 0.08, 55.0, -0.08, 300, 80, 64, 64,
        );
        let split_res3_error = nexrad_render_piece_affine_error_px(
            &viewport, 1024.0, 768.0, -130.0, 0.08, 55.0, -0.08, 300, 80, 32, 32,
        );

        assert!(
            full_res3_error > 1.0,
            "coarse source-grid pieces must exceed the mesh error budget"
        );
        assert!(
            split_res3_error < full_res3_error,
            "subdividing source-grid pieces must reduce affine error"
        );
    }

    fn nexrad_test_levels() -> Vec<NexradSourceGridLevel> {
        (0..=3)
            .map(|res| NexradSourceGridLevel {
                res,
                width: 7000 >> res,
                height: 3500 >> res,
                tile_cols: 1,
                tile_rows: 1,
            })
            .collect()
    }

    #[test]
    fn nexrad_level_selection_chooses_cheapest_level_with_bounded_pixel_stretch() {
        let levels = nexrad_test_levels();
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 4.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let selected = nexrad_level_for_viewport(&levels, &viewport, 1024.0, 768.0, 0.01, -0.01)
            .expect("selected level");

        assert_eq!(selected.res, 3);
        assert!(
            nexrad_level_screen_pixel_span(&viewport, 1024.0, 768.0, 0.01, -0.01, selected.res)
                <= 1.5
        );
    }

    #[test]
    fn nexrad_level_selection_rejects_cheapest_level_when_it_would_overstretch_pixels() {
        let levels = nexrad_test_levels();
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 4.25,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let selected = nexrad_level_for_viewport(&levels, &viewport, 1024.0, 768.0, 0.01, -0.01)
            .expect("selected level");

        assert_eq!(selected.res, 2);
        assert!(nexrad_level_screen_pixel_span(&viewport, 1024.0, 768.0, 0.01, -0.01, 3) > 1.5);
    }

    #[test]
    fn nexrad_level_selection_uses_finest_level_when_all_available_levels_would_stretch() {
        let levels = nexrad_test_levels();
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 12.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let selected = nexrad_level_for_viewport(&levels, &viewport, 1024.0, 768.0, 0.01, -0.01)
            .expect("selected level");

        assert_eq!(selected.res, 0);
    }
}
