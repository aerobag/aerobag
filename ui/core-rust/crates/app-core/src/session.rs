use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Read,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::CoreResourcePolicy;
use crate::{
    chart_ident_label_for_nav_ref_symbol,
    chart_page::airport_ids_from_plan,
    data_status::{
        parse_status_action_id, project_data_status_state, DataStatusRecord, UiDataStatusPageFact,
        UiDataStatusPageRow, UiDataStatusPageState, UiDataStatusPageTimeDisplay, UiDataStatusState,
        UiStatusActionCommand, UiStatusSeverity,
    },
    first_guidance_detail_index_for_leg,
    freshness::{
        cycle_product_is_expired, evaluate_age, format_age, parse_utc_instant, FreshnessSeverity,
        FreshnessViolation, DATA_FRESHNESS_POLICIES,
    },
    guidance_detail_id_for_index, guidance_detail_id_for_leg_element,
    had_ops::{
        flight_plan_ui_state, insert_waypoint_best_position,
        materialize_airway_presentation_selection, materialize_procedure, nav_kv_page_resources,
        nav_ref_position, nav_symbol_feature, suggest_waypoint_identifiers, CoreResourceRequest,
        CoreResourceSource, HadOperationOutcome, HadReadError, UiInvalidation,
    },
    live_feeds::{LiveFeedSseEvent, LiveFeedsState},
    map_follow::{MapFollowSessionState, MapFollowUiState},
    map_overlay::{
        nearest_available_layer_zoom, obstacle_layer_config_from_live_manifest_value,
        vector_overlay_input_requests, visible_obstacle_tile_window, FlightPlanSelectionPoint,
        MetarTileRecord, PointTileLayerConfig, VectorOverlayInputRequests,
    },
    map_overlay_config_from_vector_manifest_json, nav_kv_key_for_query,
    planning::NavElementUiView,
    playback::PlaybackSessionState,
    project_nav_symbol_feature,
    publication::PublicationResolver,
    query_map_overlay_for_surface, query_map_selection_for_surface, state,
    AirportPlateAvailability, AirspaceFeaturePayload, AirspaceLabelTilePayload,
    AirspaceReferenceTilePayload, AirwayPresentationPlan, AppError, AppErrorKind, AppEvent,
    AppResult, AppState, AppUiState, BundlePackageArtifact, FlightPlan, FlightPlanDisplayRowKind,
    FlightPlanRowActionExecution, FlightPlanRowActionId, GuidanceState, LatLon, LegDisplayElement,
    MapOverlayConfig, MapOverlayQueryResult, MapSelectionSessionAction, MapSurfaceMetrics,
    MapViewport, MetarProductPayload, MetarTilePayload, NavDbOpenResult, NavKvLookup,
    NavKvPageProbeStats, NavKvQuery, NavKvRoot, NavKvStore, NavRef, PlanLeg, PlaybackUiState,
    PointTilePayload, ProcedureDiscontinuity, ProcedureKind, ProcedureLoadCommand,
    RasterMapCatalog, RasterResourceMode, RasterTilePlan, ResolvedLeg, ResolvedLegSource,
    RouteComponentViewKind, SequencingMode, SituationControlInput, SituationControlMenuItem,
    TafProductPayload, TerrainOverlayQueryResult, TfrProductPayload, UiSnapshotAppState,
    VectorAggregateTilePayload, VectorIdentLabelStyle,
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
    pub data_status_page_state: UiDataStatusPageState,
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
    nav_db_artifact: Option<AttachedNavDbArtifact>,
    map_layer_state: UiMapLayerState,
    data_status_records: BTreeMap<String, DataStatusRecord>,
    hushed_status_ids: BTreeSet<String>,
    data_status_state: UiDataStatusState,
    debug_state: UiDebugState,
    resource_policy: CoreResourcePolicy,
    publication_resolver: PublicationResolver,
    current_artifacts_checked_epoch_ms: Option<i64>,
    cycle_product_freshness: CycleProductFreshnessState,
    live_feeds: LiveFeedsState,
    live_feed_connection: LiveFeedConnectionSessionState,
    raster_map_catalog: Option<RasterMapCatalog>,
    vector_tile_cache: HashMap<String, VectorAggregateTilePayload>,
    metar_tile_cache: HashMap<String, MetarTilePayload>,
    metar_payload: Option<MetarProductPayload>,
    prepared_metar_feed: Option<crate::PreparedMetarLiveFeed>,
    important_metar_station_ids: Option<HashSet<String>>,
    metar_station_importance_status: Option<DataStatusRecord>,
    obstacle_had: Option<LiveObstacleHadState>,
    obstacle_tile_cache: HashMap<String, PointTilePayload>,
    taf_payload: Option<TafProductPayload>,
    airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    tfr_payload: Option<TfrProductPayload>,
    nexrad_installed: Option<LiveNexradInstalledState>,
    terrain_source_tile_cache: HashMap<String, Vec<u8>>,
    pending_resource_effects: Vec<UiSessionResourceEffect>,
    wall_clock_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachedNavDbArtifact {
    package_id: String,
    filename: String,
    contract_id: Option<String>,
    cycle: Option<String>,
    cycle_version: Option<String>,
    effective_date: Option<String>,
    expiration_date: Option<String>,
    warning_text: Option<String>,
}

impl From<&NavDbOpenResult> for AttachedNavDbArtifact {
    fn from(result: &NavDbOpenResult) -> Self {
        Self {
            package_id: result.selected_package_id.clone(),
            filename: result.selected_filename.clone(),
            contract_id: result.selected_contract_id.clone(),
            cycle: result.selected_cycle.clone(),
            cycle_version: result.selected_cycle_version.clone(),
            effective_date: result.selected_effective_date.clone(),
            expiration_date: result.selected_expiration_date.clone(),
            warning_text: result.selected_warning_text.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedConnectionEventKind {
    Connecting,
    Connected,
    Message,
    Error,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedConnectionEvent {
    pub kind: LiveFeedConnectionEventKind,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveFeedConnectionMode {
    Unknown,
    Connecting,
    Connected,
    Error,
    Closed,
}

impl Default for LiveFeedConnectionMode {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LiveFeedConnectionSessionState {
    mode: LiveFeedConnectionMode,
    last_state_change_epoch_ms: Option<i64>,
    last_heard_epoch_ms: Option<i64>,
    last_error_epoch_ms: Option<i64>,
    last_error_message: Option<String>,
}

#[derive(Clone, Default)]
struct CycleProductFreshnessState {
    dirty: bool,
    missing_nav_kv_pages: BTreeSet<u32>,
}

#[derive(Clone)]
struct LiveNexradInstalledState {
    version: String,
    manifest: serde_json::Value,
    members: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSessionResourceEffect {
    pub resource: CoreResourceRequest,
    #[serde(default)]
    pub after_success_invalidations: Vec<UiInvalidation>,
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
    pub observed_at_utc: Option<DateTime<Utc>>,
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

const PROCEDURE_GEOMETRY_STATUS_PREFIX: &str = "procedure_geometry:";
const LIVE_FEED_METARS_STATUS_ID: &str = "live_feed:metars_unavailable";
const LIVE_FEED_NEXRAD_STATUS_ID: &str = "live_feed:nexrad_unavailable";
const LIVE_FEED_TFRS_STATUS_ID: &str = "live_feed:tfrs_unavailable";
const LIVE_FEED_OBSTACLES_STATUS_ID: &str = "live_feed:obstacles_unavailable";
const CYCLE_DISPLAYED_CHART_STATUS_ID: &str = "cycle:displayed_chart_invalid";
const CYCLE_NAV_DB_STATUS_ID: &str = "cycle:nav_db_expired";
const VECTOR_INPUTS_STATUS_ID: &str = "map_overlay:vector_inputs_loading";
const PACKAGE_UI_WARNING_STATUS_PREFIX: &str = "package_ui_warning:";
const METAR_STATION_IMPORTANCE_STATUS_ID: &str = "map_overlay:metar_station_importance_unavailable";
const TERRAIN_STATUS_ID: &str = "terrain:warning_unavailable";
const LIVE_OBSTACLE_HAD_RESOURCE_PREFIX: &str = "live_obstacle_had/";

#[derive(Debug, Deserialize)]
struct MetarImportantStationsPayload {
    schema_version: u32,
    station_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NavDbPackageRecord {
    id: String,
    family_id: String,
    #[serde(default)]
    effective_date: Option<String>,
    #[serde(default)]
    expiration_date: Option<String>,
    #[serde(default)]
    warning_text: Option<String>,
    #[serde(default)]
    ui_warning: Option<PackageUiWarningRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct PackageUiWarningRecord {
    severity: UiStatusSeverity,
    label: String,
    #[serde(default)]
    value: Option<String>,
    detail: String,
}

#[derive(Debug, Clone)]
struct LiveObstacleHadState {
    version: String,
    state_url: String,
    root_member_path: String,
    page_path_template: String,
    page_count: u32,
    state_sha256: String,
    store: Option<NavKvStore>,
}

#[derive(Debug, Deserialize)]
struct LiveObstacleHadManifest {
    schema_version: u32,
    product_id: String,
    version_label: String,
    encoding: String,
    root: String,
    page_path_template: String,
    page_count: u32,
    state_sha256: String,
}

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

fn enqueue_session_resource_effect(
    session: &mut UiSession,
    resource: CoreResourceRequest,
    after_success_invalidations: impl IntoIterator<Item = UiInvalidation>,
) {
    let mut invalidations = after_success_invalidations.into_iter().collect::<Vec<_>>();
    if let Some(existing) = session
        .pending_resource_effects
        .iter_mut()
        .find(|effect| effect.resource.id == resource.id)
    {
        for invalidation in invalidations.drain(..) {
            if !existing.after_success_invalidations.contains(&invalidation) {
                existing.after_success_invalidations.push(invalidation);
            }
        }
        return;
    }
    session
        .pending_resource_effects
        .push(UiSessionResourceEffect {
            resource,
            after_success_invalidations: invalidations,
        });
}

pub fn drain_session_resource_effects(handle: u32) -> AppResult<Vec<UiSessionResourceEffect>> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    Ok(std::mem::take(&mut session.pending_resource_effects))
}

fn sync_layer_unavailable_status_record(
    session: &mut UiSession,
    visible_and_unavailable: bool,
    status_id: &str,
    record: DataStatusRecord,
) -> Vec<UiInvalidation> {
    let changed = if visible_and_unavailable {
        upsert_data_status_record(session, record)
    } else {
        clear_data_status_record(session, status_id)
    };
    if changed {
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn sync_live_feed_product_status_record(
    session: &mut UiSession,
    visible: bool,
    payload_loaded: bool,
    product: &str,
    generic_detail: &str,
    observed_utc: Option<DateTime<Utc>>,
    freshness_policy: crate::freshness::AgeFreshnessPolicy,
) -> Vec<UiInvalidation> {
    let status_id = live_feed_unavailable_status_record(product, String::new()).id;
    if !visible {
        let changed = clear_data_status_record(session, &status_id);
        return if changed {
            vec![UiInvalidation::SessionSnapshot]
        } else {
            Vec::new()
        };
    }
    if payload_loaded {
        let label = live_feed_unavailable_status_record(product, String::new()).label;
        let changed = sync_live_feed_age_status_record(
            session,
            true,
            &status_id,
            &label,
            &label,
            observed_utc,
            freshness_policy,
        );
        return if changed {
            vec![UiInvalidation::SessionSnapshot]
        } else {
            Vec::new()
        };
    }
    if session.data_status_records.contains_key(&status_id) {
        return Vec::new();
    }
    sync_layer_unavailable_status_record(
        session,
        true,
        &status_id,
        live_feed_unavailable_status_record(product, generic_detail.to_string()),
    )
}

fn sync_live_feed_overlay_status_records(session: &mut UiSession) -> Vec<UiInvalidation> {
    let mut invalidations = Vec::new();
    let metars_visible = session.map_layer_state.metars.visible;
    let metars_status = metar_live_feed_status_source(session);
    invalidations.extend(sync_live_feed_product_status_record(
        session,
        metars_visible,
        metars_status.loaded,
        "metars",
        "METAR live feed unavailable: no current METAR product is loaded",
        metars_status.observed_utc,
        DATA_FRESHNESS_POLICIES.live_feeds.metars,
    ));

    let tfrs_visible = session.map_layer_state.vectors.visible;
    let tfrs_observed_utc = session
        .tfr_payload
        .as_ref()
        .and_then(|payload| payload.generated_at_utc);
    let tfrs_loaded = session.tfr_payload.is_some();
    invalidations.extend(sync_live_feed_product_status_record(
        session,
        tfrs_visible,
        tfrs_loaded,
        "tfrs",
        "TFR live feed unavailable: no current TFR product is loaded",
        tfrs_observed_utc,
        DATA_FRESHNESS_POLICIES.live_feeds.tfrs,
    ));

    let obstacles_visible = session.map_layer_state.vectors.visible;
    let obstacles_observed_utc = session
        .live_feeds
        .product_state_manifest("obstacles")
        .and_then(json_generated_at_utc);
    let obstacles_loaded = session.obstacle_had.is_some();
    invalidations.extend(sync_live_feed_product_status_record(
        session,
        obstacles_visible,
        obstacles_loaded,
        "obstacles",
        "Obstacle live feed unavailable: no current obstacle product is loaded",
        obstacles_observed_utc,
        DATA_FRESHNESS_POLICIES.live_feeds.obstacles,
    ));

    dedupe_invalidations(&mut invalidations);
    invalidations
}

fn json_generated_at_utc(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("generated_at_utc")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_utc_instant)
}

#[derive(Debug, Clone)]
struct LiveFeedProductStatusSource {
    loaded: bool,
    observed_utc: Option<DateTime<Utc>>,
    loaded_version: Option<String>,
}

fn metar_live_feed_status_source(session: &UiSession) -> LiveFeedProductStatusSource {
    if let Some(payload) = session.metar_payload.as_ref() {
        return LiveFeedProductStatusSource {
            loaded: true,
            observed_utc: payload.generated_at_utc,
            loaded_version: Some(payload.version_label.clone()),
        };
    }
    LiveFeedProductStatusSource {
        loaded: false,
        observed_utc: None,
        loaded_version: session
            .live_feeds
            .product_loaded_version("metars")
            .map(str::to_string),
    }
}

fn terrain_status_record(detail: String) -> DataStatusRecord {
    DataStatusRecord::new(
        TERRAIN_STATUS_ID,
        "TERRAIN",
        Some("UNAVAIL".to_string()),
        UiStatusSeverity::Unavailable,
        true,
        detail,
    )
}

fn terrain_unavailable_detail(status: &crate::TerrainOverlayStatus) -> Option<String> {
    match status {
        crate::TerrainOverlayStatus::Hidden | crate::TerrainOverlayStatus::Ready { .. } => None,
        crate::TerrainOverlayStatus::NoPosition => {
            Some("Terrain warning unavailable: ownship position is unavailable.".to_string())
        }
        crate::TerrainOverlayStatus::NoAltitude => {
            Some("Terrain warning unavailable: ownship altitude is unavailable.".to_string())
        }
        crate::TerrainOverlayStatus::TooManyTiles { count } => Some(format!(
            "Terrain warning unavailable: viewport requires {count} terrain tiles."
        )),
        crate::TerrainOverlayStatus::Unavailable { reason } => {
            Some(format!("Terrain warning unavailable: {reason}"))
        }
    }
}

fn sync_terrain_status_record(
    session: &mut UiSession,
    status: &crate::TerrainOverlayStatus,
) -> Vec<UiInvalidation> {
    let changed = if session.map_layer_state.terrain_warning.visible {
        match terrain_unavailable_detail(status) {
            Some(detail) => upsert_data_status_record(session, terrain_status_record(detail)),
            None => clear_data_status_record(session, TERRAIN_STATUS_ID),
        }
    } else {
        clear_data_status_record(session, TERRAIN_STATUS_ID)
    };
    if changed {
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn complete_terrain_overlay_outcome_with_invalidations(
    session: &mut UiSession,
    query: TerrainOverlayQueryResult,
    mut invalidations: Vec<UiInvalidation>,
) -> AppResult<HadOperationOutcome> {
    invalidations.extend(sync_terrain_status_record(session, &query.status));
    dedupe_invalidations(&mut invalidations);
    Ok(HadOperationOutcome::complete_with_invalidations(
        serde_json::to_value(query).map_err(internal_json_error)?,
        invalidations,
    ))
}

fn dedupe_invalidations(invalidations: &mut Vec<UiInvalidation>) {
    invalidations.sort_by_key(|invalidation| format!("{invalidation:?}"));
    invalidations.dedup();
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
        "obstacles" => DataStatusRecord::new(
            LIVE_FEED_OBSTACLES_STATUS_ID,
            "OBSTACLES",
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
                live_feed_unavailable_status_record("tfrs", detail.clone()),
            );
            changed |= upsert_data_status_record(
                session,
                live_feed_unavailable_status_record("obstacles", detail),
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
    query: &NexradOverlayQueryResult,
) -> Vec<UiInvalidation> {
    let nexrad_visible = session.map_layer_state.nexrad.visible;
    let changed = match &query.status {
        NexradOverlayStatus::Hidden => {
            clear_data_status_record(session, LIVE_FEED_NEXRAD_STATUS_ID)
        }
        NexradOverlayStatus::Ready { .. } => sync_live_feed_age_status_record(
            session,
            nexrad_visible,
            LIVE_FEED_NEXRAD_STATUS_ID,
            "NEXRAD",
            "NEXRAD",
            query.stats.observed_at_utc,
            DATA_FRESHNESS_POLICIES.live_feeds.nexrad,
        ),
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

fn complete_nexrad_overlay_outcome_with_invalidations(
    session: &mut UiSession,
    query: NexradOverlayQueryResult,
    mut invalidations: Vec<UiInvalidation>,
) -> AppResult<HadOperationOutcome> {
    invalidations.extend(sync_nexrad_status_record(session, &query));
    dedupe_invalidations(&mut invalidations);
    Ok(HadOperationOutcome::complete_with_invalidations(
        serde_json::to_value(query).map_err(internal_json_error)?,
        invalidations,
    ))
}

fn utc_from_epoch_ms(epoch_ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(epoch_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

fn session_wall_clock_utc(session: &UiSession) -> DateTime<Utc> {
    utc_from_epoch_ms(session.wall_clock_epoch_ms)
}

fn advance_session_wall_clock(session: &mut UiSession, epoch_ms: i64) {
    session.wall_clock_epoch_ms = session.wall_clock_epoch_ms.max(epoch_ms);
}

fn freshness_status_severity(violation: FreshnessViolation) -> UiStatusSeverity {
    match violation.severity {
        FreshnessSeverity::Info => UiStatusSeverity::Info,
        FreshnessSeverity::Warning => UiStatusSeverity::Warning,
    }
}

fn live_feed_stale_status_record(
    status_id: &str,
    label: &str,
    product_name: &str,
    observed_utc: DateTime<Utc>,
    violation: FreshnessViolation,
) -> DataStatusRecord {
    DataStatusRecord::new(
        status_id,
        label,
        Some("OLD".to_string()),
        freshness_status_severity(violation),
        true,
        format!(
            "{product_name} data is {} old; source timestamp {observed_utc}.",
            format_age(violation.age_ms)
        ),
    )
}

fn sync_live_feed_age_status_record(
    session: &mut UiSession,
    visible: bool,
    status_id: &str,
    label: &str,
    product_name: &str,
    observed_utc: Option<DateTime<Utc>>,
    policy: crate::freshness::AgeFreshnessPolicy,
) -> bool {
    if !visible {
        return clear_data_status_record(session, status_id);
    }
    let Some(observed_utc) = observed_utc else {
        return clear_data_status_record(session, status_id);
    };
    let now_utc = session_wall_clock_utc(session);
    if let Some(violation) = evaluate_age(policy, observed_utc, now_utc) {
        upsert_data_status_record(
            session,
            live_feed_stale_status_record(status_id, label, product_name, observed_utc, violation),
        )
    } else {
        clear_data_status_record(session, status_id)
    }
}

#[derive(Default)]
struct CycleProductFreshnessSync {
    changed: bool,
    missing_nav_kv_pages: BTreeSet<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ChartValidityViolationKind {
    NotYetEffective,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChartValidityViolation {
    family_label: &'static str,
    family_sort_key: u8,
    kind: ChartValidityViolationKind,
}

fn sync_displayed_chart_validity_freshness(session: &mut UiSession) -> CycleProductFreshnessSync {
    let mut sync = CycleProductFreshnessSync::default();
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        sync.changed = clear_data_status_record(session, CYCLE_DISPLAYED_CHART_STATUS_ID);
        return sync;
    };
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    let now_utc = session_wall_clock_utc(session);
    let mut seen = BTreeSet::new();
    let mut violations = Vec::new();
    for option in &catalog.displayed_maps {
        let (family_label, family_sort_key) =
            chart_family_status_label(&option.map_view.chart_family);
        collect_chart_validity_violations(
            &mut violations,
            &mut seen,
            family_label,
            family_sort_key,
            option.map_view.package_effective_date.as_deref(),
            option.map_view.package_expiration_date.as_deref(),
            now_utc,
        );
        if let Some(wide_angle) = option.map_view.wide_angle.as_ref() {
            collect_chart_validity_violations(
                &mut violations,
                &mut seen,
                family_label,
                family_sort_key,
                wide_angle.package_effective_date.as_deref(),
                wide_angle.package_expiration_date.as_deref(),
                now_utc,
            );
        }
    }
    if violations.is_empty() {
        sync.changed = clear_data_status_record(session, CYCLE_DISPLAYED_CHART_STATUS_ID);
        return sync;
    }
    sync.changed = upsert_data_status_record(
        session,
        DataStatusRecord::new(
            CYCLE_DISPLAYED_CHART_STATUS_ID,
            "CHART",
            Some(chart_validity_value(&violations).to_string()),
            UiStatusSeverity::Warning,
            true,
            chart_validity_detail(&violations),
        ),
    );
    sync
}

fn collect_chart_validity_violations(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    effective_date: Option<&str>,
    expiration_date: Option<&str>,
    now_utc: DateTime<Utc>,
) {
    if let Some(effective_date) = effective_date {
        if let Some(effective_utc) = parse_utc_instant(effective_date) {
            if now_utc < effective_utc {
                push_chart_validity_violation(
                    violations,
                    seen,
                    family_label,
                    family_sort_key,
                    ChartValidityViolationKind::NotYetEffective,
                );
            }
        }
    }
    if let Some(expiration_date) = expiration_date {
        if let Some(expiration_utc) = parse_utc_instant(expiration_date) {
            if cycle_product_is_expired(expiration_utc, now_utc) {
                push_chart_validity_violation(
                    violations,
                    seen,
                    family_label,
                    family_sort_key,
                    ChartValidityViolationKind::Expired,
                );
            }
        }
    }
}

fn push_chart_validity_violation(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    kind: ChartValidityViolationKind,
) {
    let key = (family_sort_key, kind);
    if seen.insert(key) {
        violations.push(ChartValidityViolation {
            family_label,
            family_sort_key,
            kind,
        });
    }
}

fn chart_family_status_label(chart_family: &str) -> (&'static str, u8) {
    match chart_family {
        "tac" => ("TAC", 10),
        "sec" => ("Sectional", 20),
        "enr-l" => ("IFR-Low", 30),
        "enr-h" => ("IFR-High", 40),
        "world-basemap" => ("World basemap", 50),
        "shaded-relief" => ("Shaded relief", 60),
        _ => ("Chart", 250),
    }
}

fn chart_validity_value(violations: &[ChartValidityViolation]) -> &'static str {
    let kinds = violations
        .iter()
        .map(|violation| violation.kind)
        .collect::<BTreeSet<_>>();
    if kinds.len() > 1 {
        return "INVALID";
    }
    match kinds.first() {
        Some(ChartValidityViolationKind::Expired) => "EXPIRED",
        Some(ChartValidityViolationKind::NotYetEffective) => "EARLY",
        None => "INVALID",
    }
}

fn chart_validity_detail(violations: &[ChartValidityViolation]) -> String {
    let mut families = violations
        .iter()
        .map(|violation| (violation.family_sort_key, violation.family_label))
        .collect::<Vec<_>>();
    families.sort_unstable();
    families.dedup();
    let family_list = families
        .into_iter()
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} charts {}.",
        family_list,
        chart_validity_condition(violations)
    )
}

fn chart_validity_condition(violations: &[ChartValidityViolation]) -> &'static str {
    let kinds = violations
        .iter()
        .map(|violation| violation.kind)
        .collect::<BTreeSet<_>>();
    if kinds.len() > 1 {
        return "not valid";
    }
    match kinds.first() {
        Some(ChartValidityViolationKind::Expired) => "expired",
        Some(ChartValidityViolationKind::NotYetEffective) => "not valid yet",
        None => "not valid",
    }
}

fn sync_nav_db_expiration_freshness(session: &mut UiSession) -> CycleProductFreshnessSync {
    let mut sync = CycleProductFreshnessSync::default();
    let Some(artifact) = session.nav_db_artifact.as_ref() else {
        sync.changed = clear_data_status_record(session, CYCLE_NAV_DB_STATUS_ID);
        return sync;
    };
    let now_utc = session_wall_clock_utc(session);
    let Some(expiration_date) = artifact.expiration_date.as_deref() else {
        sync.changed = clear_data_status_record(session, CYCLE_NAV_DB_STATUS_ID);
        return sync;
    };
    let Some(expiration_utc) = parse_utc_instant(expiration_date) else {
        sync.changed = clear_data_status_record(session, CYCLE_NAV_DB_STATUS_ID);
        return sync;
    };
    if !cycle_product_is_expired(expiration_utc, now_utc) {
        sync.changed = clear_data_status_record(session, CYCLE_NAV_DB_STATUS_ID);
        return sync;
    };
    sync.changed = upsert_data_status_record(
        session,
        DataStatusRecord::new(
            CYCLE_NAV_DB_STATUS_ID,
            "NAV DB",
            Some("EXPIRED".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "Currently attached nav-db/vector package {} expired at {expiration_date} UTC.",
                artifact.package_id
            ),
        ),
    );
    sync
}

fn mark_cycle_product_freshness_dirty(session: &mut UiSession) {
    session.cycle_product_freshness.dirty = true;
    session.cycle_product_freshness.missing_nav_kv_pages.clear();
}

fn sync_cycle_product_freshness_status_records_if_needed(
    session: &mut UiSession,
) -> Vec<UiInvalidation> {
    if !session.cycle_product_freshness.dirty {
        return Vec::new();
    }
    sync_cycle_product_freshness_status_records(session)
}

fn structured_package_warning_status_record(
    package_id: &str,
    label: String,
    value: Option<String>,
    severity: UiStatusSeverity,
    detail: String,
) -> DataStatusRecord {
    DataStatusRecord::new(
        format!("{PACKAGE_UI_WARNING_STATUS_PREFIX}{package_id}"),
        label,
        value,
        severity,
        matches!(
            severity,
            UiStatusSeverity::Caution | UiStatusSeverity::Warning | UiStatusSeverity::Unavailable
        ),
        detail,
    )
}

fn warning_text_package_status_record(
    package_id: &str,
    family_id: &str,
    warning_text: &str,
) -> DataStatusRecord {
    DataStatusRecord::new(
        format!("{PACKAGE_UI_WARNING_STATUS_PREFIX}{package_id}"),
        package_warning_label(family_id),
        Some("WARNING".to_string()),
        UiStatusSeverity::Warning,
        true,
        warning_text.to_string(),
    )
}

fn nav_db_package_warning_status_record(package: &NavDbPackageRecord) -> Option<DataStatusRecord> {
    if let Some(warning) = package.ui_warning.as_ref() {
        return Some(structured_package_warning_status_record(
            &package.id,
            warning.label.clone(),
            warning.value.clone(),
            warning.severity,
            warning.detail.clone(),
        ));
    }
    package.warning_text.as_ref().map(|warning_text| {
        warning_text_package_status_record(&package.id, &package.family_id, warning_text)
    })
}

fn bundle_package_warning_status_record(
    package: &BundlePackageArtifact,
) -> Option<DataStatusRecord> {
    if let Some(warning) = package.ui_warning.as_ref() {
        return Some(structured_package_warning_status_record(
            &package.id,
            warning.label.clone(),
            warning.value.clone(),
            warning.severity,
            warning.detail.clone(),
        ));
    }
    package.warning_text.as_ref().map(|warning_text| {
        warning_text_package_status_record(&package.id, &package.family_id, warning_text)
    })
}

fn attached_nav_db_warning_status_record(
    artifact: &AttachedNavDbArtifact,
) -> Option<DataStatusRecord> {
    artifact.warning_text.as_ref().map(|warning_text| {
        DataStatusRecord::new(
            format!("{PACKAGE_UI_WARNING_STATUS_PREFIX}{}", artifact.package_id),
            "NAV DB",
            Some("WARNING".to_string()),
            UiStatusSeverity::Warning,
            true,
            warning_text.clone(),
        )
    })
}

fn package_warning_label(family_id: &str) -> String {
    match family_id {
        "nav-db" => "NAV DB".to_string(),
        "sec" => "Sectional".to_string(),
        "tac" => "TAC".to_string(),
        "enr-l" => "IFR-L".to_string(),
        "enr-h" => "IFR-H".to_string(),
        "tpp" => "TPP".to_string(),
        "csup" => "CSup".to_string(),
        "terrain" => "Terrain".to_string(),
        "shaded-relief" => "Shaded relief".to_string(),
        "world-basemap" => "World basemap".to_string(),
        "geo" => "Geodesy".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn nav_db_package_records(session: &UiSession) -> Vec<NavDbPackageRecord> {
    let Some(store) = session.nav_kv_store.as_ref() else {
        return Vec::new();
    };
    let mut records = store
        .keys_with_prefix("package/by-id/")
        .into_iter()
        .filter_map(|key| {
            let Ok(NavKvLookup::Hit(bytes)) = store.get_bytes(&key) else {
                return None;
            };
            serde_json::from_slice::<NavDbPackageRecord>(&bytes).ok()
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    records.dedup_by(|left, right| left.id == right.id);
    records
}

fn package_warning_status_records(session: &UiSession) -> Vec<DataStatusRecord> {
    let mut records = BTreeMap::new();
    for package in session
        .publication_resolver
        .loaded_bundle_packages()
        .filter(|package| crate::package_management::package_contract_is_supported(package))
    {
        if let Some(record) = bundle_package_warning_status_record(package) {
            records.insert(record.id.clone(), record);
        }
    }
    for package in nav_db_package_records(session) {
        if let Some(record) = nav_db_package_warning_status_record(&package) {
            records.insert(record.id.clone(), record);
        }
    }
    if let Some(record) = session
        .nav_db_artifact
        .as_ref()
        .and_then(attached_nav_db_warning_status_record)
    {
        records.insert(record.id.clone(), record);
    }
    records.into_values().collect()
}

fn sync_package_ui_warning_status_records(session: &mut UiSession) -> bool {
    let records = package_warning_status_records(session);
    let active_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let stale_ids = session
        .data_status_records
        .keys()
        .filter(|id| id.starts_with(PACKAGE_UI_WARNING_STATUS_PREFIX) && !active_ids.contains(*id))
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

fn sync_cycle_product_freshness_status_records(session: &mut UiSession) -> Vec<UiInvalidation> {
    let selected = sync_displayed_chart_validity_freshness(session);
    let nav_db = sync_nav_db_expiration_freshness(session);
    session.cycle_product_freshness = CycleProductFreshnessState {
        dirty: false,
        missing_nav_kv_pages: nav_db.missing_nav_kv_pages,
    };
    let changed =
        selected.changed | nav_db.changed | sync_package_ui_warning_status_records(session);
    if changed {
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn project_data_status_page_state(session: &UiSession) -> UiDataStatusPageState {
    let metars_status = metar_live_feed_status_source(session);
    let mut rows = vec![
        cycle_package_group_status_page_row(
            session,
            "cycle:charts",
            "Charts",
            "charts",
            &[
                ("sec", "Sectional", 10),
                ("tac", "TAC", 20),
                ("enr-l", "IFR-L", 30),
                ("enr-h", "IFR-H", 40),
            ],
        ),
        cycle_package_group_status_page_row(
            session,
            "cycle:airport_docs",
            "Airport docs",
            "airport docs",
            &[("tpp", "TPP", 10), ("csup", "CSup", 20)],
        ),
        static_package_group_status_page_row(
            session,
            "static:base_data",
            "Static data",
            &[
                ("terrain", "Terrain", 10),
                ("shaded-relief", "Shaded relief", 20),
                ("world-basemap", "World basemap", 30),
                ("geo", "Geodesy", 40),
            ],
        ),
        nav_db_status_page_row(session),
    ];
    rows.extend(package_warning_status_page_rows(session));
    rows.extend([
        publication_status_page_row(session),
        live_feed_connection_status_page_row(session),
        live_feed_product_status_page_row(
            session,
            "metars",
            "METARs",
            metars_status.loaded,
            metars_status.observed_utc,
            DATA_FRESHNESS_POLICIES.live_feeds.metars,
            metars_status.loaded_version,
        ),
        live_feed_product_status_page_row(
            session,
            "nexrad",
            "NEXRAD",
            nexrad_status_manifest(session).is_some(),
            nexrad_status_manifest(session).and_then(json_observed_at_utc),
            DATA_FRESHNESS_POLICIES.live_feeds.nexrad,
            session
                .nexrad_installed
                .as_ref()
                .map(|installed| installed.version.clone())
                .or_else(|| {
                    session
                        .live_feeds
                        .product_loaded_version("nexrad")
                        .map(str::to_string)
                }),
        ),
        live_feed_product_status_page_row(
            session,
            "tfrs",
            "TFRs",
            session.tfr_payload.is_some(),
            session
                .tfr_payload
                .as_ref()
                .and_then(|payload| payload.generated_at_utc),
            DATA_FRESHNESS_POLICIES.live_feeds.tfrs,
            session
                .live_feeds
                .product_loaded_version("tfrs")
                .map(str::to_string),
        ),
        live_feed_product_status_page_row(
            session,
            "obstacles",
            "Obstacles",
            session.obstacle_had.is_some()
                || session
                    .live_feeds
                    .product_state_manifest("obstacles")
                    .is_some(),
            session
                .live_feeds
                .product_state_manifest("obstacles")
                .and_then(json_generated_at_utc),
            DATA_FRESHNESS_POLICIES.live_feeds.obstacles,
            session
                .obstacle_had
                .as_ref()
                .map(|had| had.version.clone())
                .or_else(|| {
                    session
                        .live_feeds
                        .product_loaded_version("obstacles")
                        .map(str::to_string)
                }),
        ),
    ]);
    UiDataStatusPageState {
        title: "Data status".to_string(),
        summary: data_status_page_summary(&rows),
        rows,
    }
}

fn package_warning_status_page_rows(session: &UiSession) -> Vec<UiDataStatusPageRow> {
    let mut records = package_warning_status_records(session);
    records.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.id.cmp(&right.id))
    });
    records
        .into_iter()
        .map(|record| {
            status_page_row(
                record.id,
                record.label,
                record.value.unwrap_or_else(|| "WARNING".to_string()),
                record.severity,
                record.detail,
                Vec::new(),
            )
        })
        .collect()
}

fn data_status_page_summary(rows: &[UiDataStatusPageRow]) -> String {
    let warnings = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Warning))
        .count();
    let unavailable = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Unavailable))
        .count();
    let cautions = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Caution))
        .count();
    if warnings + unavailable + cautions == 0 {
        return "All tracked data is usable.".to_string();
    }
    let mut parts = Vec::new();
    if warnings > 0 {
        parts.push(format!("{warnings} warning{}", plural_s(warnings)));
    }
    if cautions > 0 {
        parts.push(format!("{cautions} caution{}", plural_s(cautions)));
    }
    if unavailable > 0 {
        parts.push(format!(
            "{unavailable} unavailable source{}",
            plural_s(unavailable)
        ));
    }
    parts.join(", ")
}

fn cycle_package_group_status_page_row(
    session: &UiSession,
    id: &str,
    label: &str,
    noun: &str,
    families: &[(&'static str, &'static str, u8)],
) -> UiDataStatusPageRow {
    let packages = package_group_packages(session, families);
    if packages.is_empty() {
        let severity = if session.nav_kv_store.is_none() {
            UiStatusSeverity::Unavailable
        } else {
            UiStatusSeverity::Info
        };
        return status_page_row(
            id,
            label,
            "MISSING",
            severity,
            format!("No {noun} package rows are present in the attached nav-db."),
            Vec::new(),
        );
    }

    let now_utc = session_wall_clock_utc(session);
    let mut seen_violations = BTreeSet::new();
    let mut violations = Vec::new();
    let mut family_set = BTreeSet::new();
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut missing_expiration_families = BTreeSet::new();

    for package in &packages {
        let Some((_, family_label, family_sort_key)) = family_spec_for_package(families, package)
        else {
            continue;
        };
        family_set.insert((family_sort_key, family_label));
        collect_chart_validity_violations(
            &mut violations,
            &mut seen_violations,
            family_label,
            family_sort_key,
            package.effective_date.as_deref(),
            package.expiration_date.as_deref(),
            now_utc,
        );
        if let Some(effective_utc) = package
            .effective_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            latest_effective = Some(
                latest_effective
                    .map(|current| current.max(effective_utc))
                    .unwrap_or(effective_utc),
            );
        }
        if let Some(expiration_utc) = package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            earliest_expiration = Some(
                earliest_expiration
                    .map(|current| current.min(expiration_utc))
                    .unwrap_or(expiration_utc),
            );
        } else {
            missing_expiration_families.insert((family_sort_key, family_label));
        }
    }

    let family_list = status_family_list(family_set.iter().copied());
    if !violations.is_empty() {
        return status_page_row(
            id,
            label,
            chart_validity_value(&violations),
            UiStatusSeverity::Warning,
            package_validity_detail(&family_list, noun, &violations),
            vec![status_fact("Products", family_list)],
        );
    }

    let mut facts = vec![
        status_fact("Products", family_list.clone()),
        status_fact("Packages", packages.len().to_string()),
    ];
    if let Some(effective) = latest_effective {
        facts.push(status_time_fact(
            "Effective",
            effective,
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(expiration) = earliest_expiration {
        facts.push(status_time_fact(
            "Expires",
            expiration,
            UiDataStatusPageTimeDisplay::Until,
        ));
        return status_page_row(
            id,
            label,
            "OK",
            UiStatusSeverity::Ok,
            format!(
                "{family_list} {noun} valid until {}.",
                format_status_utc(expiration)
            ),
            facts,
        );
    }
    if !missing_expiration_families.is_empty() {
        facts.push(status_fact(
            "Missing dates",
            status_family_list(missing_expiration_families.iter().copied()),
        ));
    }
    status_page_row(
        id,
        label,
        "UNKNOWN",
        UiStatusSeverity::Info,
        format!("{family_list} {noun} validity metadata is not available."),
        facts,
    )
}

fn static_package_group_status_page_row(
    session: &UiSession,
    id: &str,
    label: &str,
    families: &[(&'static str, &'static str, u8)],
) -> UiDataStatusPageRow {
    let packages = package_group_packages(session, families);
    if packages.is_empty() {
        let severity = if session.nav_kv_store.is_none() {
            UiStatusSeverity::Unavailable
        } else {
            UiStatusSeverity::Info
        };
        return status_page_row(
            id,
            label,
            "MISSING",
            severity,
            "No static package rows are present in the attached nav-db.",
            Vec::new(),
        );
    }

    let now_utc = session_wall_clock_utc(session);
    let mut newest_by_family: BTreeMap<u8, (&'static str, DateTime<Utc>)> = BTreeMap::new();
    let mut family_set = BTreeSet::new();
    for package in &packages {
        let Some((_, family_label, family_sort_key)) = family_spec_for_package(families, package)
        else {
            continue;
        };
        family_set.insert((family_sort_key, family_label));
        if let Some(effective_utc) = package
            .effective_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            newest_by_family
                .entry(family_sort_key)
                .and_modify(|(_, current)| *current = (*current).max(effective_utc))
                .or_insert((family_label, effective_utc));
        }
    }

    let family_list = status_family_list(family_set.iter().copied());
    let mut facts = vec![
        status_fact("Products", family_list.clone()),
        status_fact("Packages", packages.len().to_string()),
    ];
    for (_, (family_label, effective_utc)) in &newest_by_family {
        facts.push(status_time_fact(
            *family_label,
            *effective_utc,
            UiDataStatusPageTimeDisplay::Old,
        ));
    }
    if newest_by_family.is_empty() {
        return status_page_row(
            id,
            label,
            "LOADED",
            UiStatusSeverity::Info,
            format!("{family_list} packages are loaded, but source age metadata is not available."),
            facts,
        );
    }
    let oldest = newest_by_family
        .values()
        .map(|(_, effective)| *effective)
        .min()
        .unwrap_or(now_utc);
    status_page_row(
        id,
        label,
        "OK",
        UiStatusSeverity::Ok,
        format!(
            "{family_list} source data dates back to {}.",
            format_status_utc(oldest)
        ),
        facts,
    )
}

fn package_group_packages(
    session: &UiSession,
    families: &[(&'static str, &'static str, u8)],
) -> Vec<NavDbPackageRecord> {
    let family_ids = families
        .iter()
        .map(|(family_id, _, _)| *family_id)
        .collect::<BTreeSet<_>>();
    nav_db_package_records(session)
        .into_iter()
        .filter(|package| family_ids.contains(package.family_id.as_str()))
        .collect()
}

fn family_spec_for_package<'a>(
    families: &'a [(&'static str, &'static str, u8)],
    package: &NavDbPackageRecord,
) -> Option<(&'static str, &'static str, u8)> {
    families
        .iter()
        .find(|(family_id, _, _)| *family_id == package.family_id.as_str())
        .copied()
}

fn package_validity_detail(
    family_list: &str,
    noun: &str,
    violations: &[ChartValidityViolation],
) -> String {
    format!(
        "{family_list} {noun} {}.",
        chart_validity_condition(violations)
    )
}

fn nav_db_status_page_row(session: &UiSession) -> UiDataStatusPageRow {
    let Some(artifact) = session.nav_db_artifact.as_ref() else {
        return status_page_row(
            "nav_db",
            "NAV DB",
            "MISSING",
            UiStatusSeverity::Unavailable,
            "No nav-db package is attached.",
            Vec::new(),
        );
    };
    let now_utc = session_wall_clock_utc(session);
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut expired = false;
    let mut not_yet_effective = false;
    if let Some(effective_utc) = artifact
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant)
    {
        latest_effective = Some(effective_utc);
        not_yet_effective |= now_utc < effective_utc;
    }
    if let Some(expiration_utc) = artifact
        .expiration_date
        .as_deref()
        .and_then(parse_utc_instant)
    {
        earliest_expiration = Some(expiration_utc);
        expired |= cycle_product_is_expired(expiration_utc, now_utc);
    }
    let mut facts = vec![
        status_fact("Package", artifact.package_id.clone()),
        status_fact("File", artifact.filename.clone()),
    ];
    if let Some(cycle) = artifact.cycle.as_deref() {
        facts.push(status_fact("Cycle", cycle));
    }
    if let Some(version) = artifact.cycle_version.as_deref() {
        facts.push(status_fact("Cycle version", version));
    }
    if let Some(contract_id) = artifact.contract_id.as_deref() {
        facts.push(status_fact("Contract", contract_id));
    }
    if let Some(effective) = latest_effective {
        facts.push(status_time_fact(
            "Effective",
            effective,
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(expiration) = earliest_expiration {
        facts.push(status_time_fact(
            "Expires",
            expiration,
            UiDataStatusPageTimeDisplay::Until,
        ));
    }
    if expired || not_yet_effective {
        let value = match (expired, not_yet_effective) {
            (true, true) => "INVALID",
            (true, false) => "EXPIRED",
            (false, true) => "EARLY",
            (false, false) => "INVALID",
        };
        let condition = match (expired, not_yet_effective) {
            (true, true) => "not valid",
            (true, false) => "expired",
            (false, true) => "not valid yet",
            (false, false) => "not valid",
        };
        return status_page_row(
            "nav_db",
            "NAV DB",
            value,
            UiStatusSeverity::Warning,
            format!(
                "Attached nav-db package {} is {condition}.",
                artifact.package_id
            ),
            facts,
        );
    }
    if let Some(expiration) = earliest_expiration {
        return status_page_row(
            "nav_db",
            "NAV DB",
            "OK",
            UiStatusSeverity::Ok,
            format!("NAV DB valid until {}.", format_status_utc(expiration)),
            facts,
        );
    }
    status_page_row(
        "nav_db",
        "NAV DB",
        "UNKNOWN",
        UiStatusSeverity::Info,
        "NAV DB package metadata does not include an expiration date.",
        facts,
    )
}

fn publication_status_page_row(session: &UiSession) -> UiDataStatusPageRow {
    let Some(current_artifacts) = session.publication_resolver.current_artifacts() else {
        return status_page_row(
            "publication:current_artifacts",
            "Package library",
            "NEVER",
            UiStatusSeverity::Info,
            "current_artifacts.json has not been checked in this session.",
            Vec::new(),
        );
    };
    let mut facts = Vec::new();
    facts.push(status_fact(
        "Bundles",
        current_artifacts.bundles.len().to_string(),
    ));
    facts.push(status_fact(
        "Loaded manifests",
        session
            .publication_resolver
            .loaded_bundle_manifest_count()
            .to_string(),
    ));
    if let Some(as_of) = current_artifacts
        .as_of_utc
        .as_deref()
        .and_then(parse_utc_instant)
    {
        facts.push(status_time_fact(
            "Published",
            as_of,
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(checked_at) = session.current_artifacts_checked_epoch_ms {
        let checked_utc = utc_from_epoch_ms(checked_at);
        facts.push(status_time_fact(
            "Checked",
            checked_utc,
            UiDataStatusPageTimeDisplay::Ago,
        ));
        return status_page_row(
            "publication:current_artifacts",
            "Package library",
            "OK",
            UiStatusSeverity::Ok,
            format!(
                "current_artifacts.json checked at {}.",
                format_status_utc(checked_utc)
            ),
            facts,
        );
    }
    status_page_row(
        "publication:current_artifacts",
        "Package library",
        "LOADED",
        UiStatusSeverity::Ok,
        "current_artifacts.json is loaded; check time was not recorded.",
        facts,
    )
}

fn live_feed_connection_status_page_row(session: &UiSession) -> UiDataStatusPageRow {
    let connection = &session.live_feed_connection;
    let mut facts = Vec::new();
    if let Some(last_heard) = connection.last_heard_epoch_ms {
        facts.push(status_time_fact(
            "Last server event",
            utc_from_epoch_ms(last_heard),
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(last_error) = connection.last_error_epoch_ms {
        facts.push(status_time_fact(
            "Last error",
            utc_from_epoch_ms(last_error),
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(message) = connection.last_error_message.as_deref() {
        facts.push(status_fact("Error", message.to_string()));
    }
    let (value, severity, detail) = match connection.mode {
        LiveFeedConnectionMode::Unknown => (
            "UNKNOWN",
            UiStatusSeverity::Info,
            "No live-feed connection state has been reported.".to_string(),
        ),
        LiveFeedConnectionMode::Connecting => (
            "CONNECTING",
            UiStatusSeverity::Info,
            "The live-feed event stream is connecting.".to_string(),
        ),
        LiveFeedConnectionMode::Connected => {
            let heard = connection
                .last_heard_epoch_ms
                .map(|epoch| {
                    format!(
                        " Last server event was at {}.",
                        format_status_utc(utc_from_epoch_ms(epoch))
                    )
                })
                .unwrap_or_default();
            (
                "CONNECTED",
                UiStatusSeverity::Ok,
                format!("The live-feed event stream is connected.{heard}"),
            )
        }
        LiveFeedConnectionMode::Error => (
            "ERROR",
            UiStatusSeverity::Unavailable,
            connection
                .last_error_message
                .as_ref()
                .map(|message| format!("The live-feed event stream reported an error: {message}."))
                .unwrap_or_else(|| "The live-feed event stream reported an error.".to_string()),
        ),
        LiveFeedConnectionMode::Closed => (
            "CLOSED",
            UiStatusSeverity::Unavailable,
            "The live-feed event stream is closed.".to_string(),
        ),
    };
    status_page_row(
        "live_feed:connection",
        "Live-feed connection",
        value,
        severity,
        detail,
        facts,
    )
}

fn live_feed_product_status_page_row(
    session: &UiSession,
    product: &str,
    label: &str,
    loaded: bool,
    observed_utc: Option<DateTime<Utc>>,
    policy: crate::freshness::AgeFreshnessPolicy,
    loaded_version: Option<String>,
) -> UiDataStatusPageRow {
    let now_utc = session_wall_clock_utc(session);
    let mut facts = Vec::new();
    if let Some(version) = loaded_version {
        facts.push(status_fact("Version", version));
    }
    if let Some(observed) = observed_utc {
        facts.push(status_time_fact(
            "Observed",
            observed,
            UiDataStatusPageTimeDisplay::Old,
        ));
    }
    if !loaded {
        let detail = if session.live_feeds.current_loaded() {
            if session.live_feeds.has_product_current_version(product) {
                format!("{label} is listed in the live-feed index but no current state is loaded.")
            } else {
                format!("{label} is not listed in the live-feed index.")
            }
        } else {
            "The live-feed index has not loaded.".to_string()
        };
        return status_page_row(
            format!("live_feed:{product}"),
            label,
            "MISSING",
            UiStatusSeverity::Unavailable,
            detail,
            facts,
        );
    }
    let Some(observed_utc) = observed_utc else {
        return status_page_row(
            format!("live_feed:{product}"),
            label,
            "LOADED",
            UiStatusSeverity::Info,
            format!("{label} is loaded, but no source timestamp is available."),
            facts,
        );
    };
    if let Some(violation) = evaluate_age(policy, observed_utc, now_utc) {
        return status_page_row(
            format!("live_feed:{product}"),
            label,
            "OLD",
            freshness_status_severity(violation),
            format!(
                "{label} data timestamp is {}.",
                format_status_utc(observed_utc)
            ),
            facts,
        );
    }
    status_page_row(
        format!("live_feed:{product}"),
        label,
        "OK",
        UiStatusSeverity::Ok,
        format!(
            "{label} data timestamp is {}.",
            format_status_utc(observed_utc)
        ),
        facts,
    )
}

fn nexrad_status_manifest(session: &UiSession) -> Option<&serde_json::Value> {
    session
        .nexrad_installed
        .as_ref()
        .map(|installed| &installed.manifest)
        .or_else(|| session.live_feeds.product_state_manifest("nexrad"))
}

fn json_observed_at_utc(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("observed_at_utc")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_utc_instant)
}

fn status_page_row(
    id: impl Into<String>,
    label: impl Into<String>,
    value: impl Into<String>,
    severity: UiStatusSeverity,
    detail: impl Into<String>,
    facts: Vec<UiDataStatusPageFact>,
) -> UiDataStatusPageRow {
    UiDataStatusPageRow {
        id: id.into(),
        label: label.into(),
        value: value.into(),
        severity,
        detail: detail.into(),
        facts,
    }
}

fn status_fact(label: impl Into<String>, value: impl Into<String>) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: value.into(),
        time_utc: None,
        time_display: None,
    }
}

fn status_time_fact(
    label: impl Into<String>,
    instant: DateTime<Utc>,
    display: UiDataStatusPageTimeDisplay,
) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: format_status_utc(instant),
        time_utc: Some(format_status_rfc3339(instant)),
        time_display: Some(display),
    }
}

fn status_family_list<'a>(families: impl IntoIterator<Item = (u8, &'a str)>) -> String {
    families
        .into_iter()
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_status_utc(instant: DateTime<Utc>) -> String {
    instant.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn format_status_rfc3339(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
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
    create_ui_session_at_epoch_ms(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        0,
    )
}

pub fn create_ui_session_at_epoch_ms(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    wall_clock_epoch_ms: i64,
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        wall_clock_epoch_ms,
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
    create_ui_session_profiled_at_epoch_ms(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        0,
        mark,
    )
}

pub fn create_ui_session_profiled_at_epoch_ms(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    wall_clock_epoch_ms: i64,
    mark: &mut dyn FnMut(&'static str),
) -> AppResult<UiSessionInitResult> {
    create_ui_session_inner(
        plan,
        recent_airport_ids,
        selected_airport_id,
        selected_chart_id,
        wall_clock_epoch_ms,
        Some(mark),
    )
}

fn create_ui_session_inner(
    plan: FlightPlan,
    recent_airport_ids: &[String],
    selected_airport_id: Option<&str>,
    selected_chart_id: Option<&str>,
    wall_clock_epoch_ms: i64,
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
    let data_status_page_state = default_data_status_page_state();
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
        data_status_page_state,
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
            nav_db_artifact: None,
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
            current_artifacts_checked_epoch_ms: None,
            cycle_product_freshness: CycleProductFreshnessState {
                dirty: true,
                ..CycleProductFreshnessState::default()
            },
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_feed: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms,
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
    match layer {
        MapLayerId::Metars | MapLayerId::Vectors => {
            sync_live_feed_overlay_status_records(session);
        }
        MapLayerId::Nexrad if !visible => {
            clear_data_status_record(session, LIVE_FEED_NEXRAD_STATUS_ID);
        }
        MapLayerId::TerrainWarning if !visible => {
            clear_data_status_record(session, TERRAIN_STATUS_ID);
        }
        MapLayerId::Nexrad
        | MapLayerId::TerrainWarning
        | MapLayerId::WorldBasemap
        | MapLayerId::OfflineRegions => {}
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
    sync_cycle_product_freshness_status_records(session);
    session_snapshot_outcome(session)
}

pub fn resolve_nav_db_artifact_candidates_in_session(
    handle: u32,
) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    session
        .publication_resolver
        .resolve_nav_db_artifact_candidates()
        .map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message,
        })
}

pub fn resolve_package_member_in_session(
    handle: u32,
    package_id: &str,
    member_path: &str,
) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    session
        .publication_resolver
        .resolve_package_resource(package_id, member_path)
        .map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message,
        })
}

pub fn resolve_metar_manifest_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    resolve_package_member_in_session(handle, "metars", "manifest.json")
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
        sync_cycle_product_freshness_status_records(session);
        return session_snapshot_outcome(session);
    };
    crate::select_map_family_in_catalog(catalog, family_id);
    sync_cycle_product_freshness_status_records(session);
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
    sync_cycle_product_freshness_status_records(session);
    snapshot_for_session(session)
}

pub fn get_raster_tile_plan_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<RasterTilePlan> {
    get_raster_tile_plan_in_session_at_epoch_ms(handle, viewport, width_px, height_px, 0)
}

pub fn get_raster_tile_plan_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    epoch_ms: i64,
) -> AppResult<RasterTilePlan> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
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
    let mut plan =
        crate::raster_tile_plan_with_options(&catalog, &viewport, width_px, height_px, options);
    resolve_raster_tile_plan_public_urls(session, &mut plan)?;
    Ok(plan)
}

pub fn get_raster_tile_plan_in_session_with_options(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    options: crate::RasterTilePlanOptions,
) -> AppResult<RasterTilePlan> {
    get_raster_tile_plan_in_session_with_options_at_epoch_ms(
        handle, viewport, width_px, height_px, options, 0,
    )
}

pub fn get_raster_tile_plan_in_session_with_options_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    options: crate::RasterTilePlanOptions,
    epoch_ms: i64,
) -> AppResult<RasterTilePlan> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    let mut plan =
        crate::raster_tile_plan_with_options(&catalog, &viewport, width_px, height_px, options);
    resolve_raster_tile_plan_public_urls(session, &mut plan)?;
    Ok(plan)
}

pub fn get_raster_tile_plan_in_session_with_display_scale(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
) -> AppResult<RasterTilePlan> {
    get_raster_tile_plan_in_session_with_display_scale_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        device_pixel_ratio,
        0,
    )
}

pub fn get_raster_tile_plan_in_session_with_display_scale_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    device_pixel_ratio: f64,
    epoch_ms: i64,
) -> AppResult<RasterTilePlan> {
    let total_started_at = crate::core_clock_ms();
    let lock_started_at = crate::core_clock_ms();
    let mut sessions = lock_sessions();
    let lock_ms = elapsed_ms(lock_started_at);
    let session = session_mut(&mut sessions, handle)?;
    let advance_started_at = crate::core_clock_ms();
    advance_session_wall_clock(session, epoch_ms);
    let advance_ms = elapsed_ms(advance_started_at);
    let freshness_ms = 0;
    let Some(catalog) = session.raster_map_catalog.as_ref() else {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "session missing raster map catalog".to_string(),
        });
    };
    let source_displayed_maps = catalog.displayed_maps.len();
    let source_available_maps = catalog.available_maps.len();
    let options = crate::RasterTilePlanOptions {
        max_tile_display_multiplier: if session.debug_state.fast_tiles {
            2.0
        } else {
            1.0
        },
        device_pixel_ratio,
        resource_mode: raster_resource_mode_for_policy(session.resource_policy),
    };
    let catalog_started_at = crate::core_clock_ms();
    let catalog = raster_catalog_for_layer_state(catalog, &session.map_layer_state);
    let catalog_filter_ms = elapsed_ms(catalog_started_at);
    let displayed_maps = catalog.displayed_maps.len();
    let mut plan =
        crate::raster_tile_plan_with_options(&catalog, &viewport, width_px, height_px, options);
    resolve_raster_tile_plan_public_urls(session, &mut plan)?;
    let session_total_ms = elapsed_ms(total_started_at);
    if let Some(timing) = plan.debug_timing.as_mut() {
        timing.session_total_ms = Some(session_total_ms);
        timing.session_lock_ms = Some(lock_ms);
        timing.session_advance_ms = Some(advance_ms);
        timing.session_freshness_ms = Some(freshness_ms);
        timing.session_catalog_filter_ms = Some(catalog_filter_ms);
        timing.session_source_displayed_maps = Some(source_displayed_maps);
        timing.session_source_available_maps = Some(source_available_maps);
        timing.session_displayed_maps = Some(displayed_maps);
    }
    Ok(plan)
}

fn resolve_raster_tile_plan_public_urls(
    session: &UiSession,
    plan: &mut RasterTilePlan,
) -> AppResult<()> {
    if session.resource_policy != CoreResourcePolicy::PublicUnpacked {
        return Ok(());
    }
    let mut resolved_urls = HashMap::<(String, String), String>::new();
    for tile in &mut plan.tiles {
        resolve_raster_tile_source_public_url(session, &mut tile.primary, &mut resolved_urls)?;
        for fallback in &mut tile.fallbacks {
            resolve_raster_tile_source_public_url(session, fallback, &mut resolved_urls)?;
        }
    }
    Ok(())
}

fn resolve_raster_tile_source_public_url(
    session: &UiSession,
    source: &mut crate::RasterTileSource,
    resolved_urls: &mut HashMap<(String, String), String>,
) -> AppResult<()> {
    let crate::RasterTileResource::PublicUnpacked {
        package_name,
        member_path,
    } = &source.resource
    else {
        return Ok(());
    };
    let key = (package_name.clone(), member_path.clone());
    let url = if let Some(url) = resolved_urls.get(&key) {
        url.clone()
    } else {
        let url = session
            .publication_resolver
            .package_member_public_url(package_name, member_path)
            .map_err(|message| AppError {
                kind: AppErrorKind::InvalidManifest,
                message,
            })?;
        resolved_urls.insert(key, url.clone());
        url
    };
    source.resource = crate::RasterTileResource::ResolvedPublicUrl { url };
    Ok(())
}

fn elapsed_ms(started_at: Option<f64>) -> u64 {
    let Some(started_at) = started_at else {
        return 0;
    };
    let Some(now_ms) = crate::core_clock_ms() else {
        return 0;
    };
    (now_ms - started_at).max(0.0).round() as u64
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
    let started = crate::CoreDebugTimer::start();
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    match sync_guidance_geometry_for_session(session, &started) {
        Ok(()) => {
            crate::core_debug_log(
                "plan.guidance.sync.core_phase",
                &serde_json::json!({
                    "phase": "snapshot_start",
                    "total_ms": started.elapsed_ms(),
                }),
            );
            session_snapshot_outcome(session)
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

fn sync_guidance_geometry_for_session(
    session: &mut UiSession,
    started: &crate::CoreDebugTimer,
) -> Result<(), HadReadError> {
    let Some(plan) = session.app_state.active_plan.clone() else {
        session.guidance_leg_geometry.clear();
        crate::core_debug_log(
            "plan.guidance.sync.core_phase",
            &serde_json::json!({
                "phase": "empty_plan",
                "elapsed_ms": started.elapsed_ms(),
            }),
        );
        return Ok(());
    };
    crate::core_debug_log(
        "plan.guidance.sync.core_phase",
        &serde_json::json!({
            "phase": "start",
            "resolved_leg_count": plan.resolved_legs.len(),
            "component_count": plan.route_components.len(),
        }),
    );
    let route_started = crate::CoreDebugTimer::start();
    let Some(store) = session.nav_kv_store.as_ref() else {
        session.guidance_leg_geometry.clear();
        crate::core_debug_log(
            "plan.guidance.sync.core_phase",
            &serde_json::json!({
                "phase": "missing_nav_kv_store",
                "total_ms": started.elapsed_ms(),
            }),
        );
        return Ok(());
    };
    let route = match crate::had_ops::project_flight_plan_route(store, &plan) {
        Ok(route) => route,
        Err(HadReadError::NeedPages(pages)) => {
            crate::core_debug_log(
                "plan.guidance.sync.core_phase",
                &serde_json::json!({
                    "phase": "route_need_pages",
                    "resource_count": pages.len(),
                    "pages": pages,
                    "elapsed_ms": route_started.elapsed_ms(),
                    "total_ms": started.elapsed_ms(),
                }),
            );
            return Err(HadReadError::NeedPages(pages));
        }
        Err(HadReadError::Fatal(message)) => {
            return Err(HadReadError::Fatal(message));
        }
    };
    crate::core_debug_log(
        "plan.guidance.sync.core_phase",
        &serde_json::json!({
            "phase": "route_done",
            "route_segment_count": route.len(),
            "elapsed_ms": route_started.elapsed_ms(),
            "total_ms": started.elapsed_ms(),
        }),
    );
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
        sync_plan_preview_to_active_leg(session).map_err(|err| HadReadError::Fatal(err.message))?;
    }
    Ok(())
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
            activate_direct_to_nav_ref_in_session_outcome(handle, nav_ref)
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
    commit_session_flight_plan_with_invalidations_outcome(session, mutation.plan)
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
    commit_session_flight_plan_with_invalidations_outcome(session, next_plan)
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
    commit_session_flight_plan_with_invalidations_outcome(session, mutation.plan)
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
    commit_session_flight_plan_with_invalidations_outcome(session, mutation.mutation.plan)
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
    let started = crate::CoreDebugTimer::start();
    let trace = serde_json::json!({
        "row_uid": &row_uid,
        "airport_id": &airport_id,
        "procedure_id": &procedure_id,
        "kind": &kind,
        "runway_transition": runway_transition.as_ref(),
        "enroute_transition": enroute_transition.as_ref(),
    });
    crate::core_debug_log(
        "plan.procedure.select.core_phase",
        &serde_json::json!({
            "phase": "start",
            "trace": trace,
        }),
    );
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let mut plan = session_plan(session)?;
    let ui = crate::project_ui_state(&plan);
    crate::core_debug_log(
        "plan.procedure.select.core_phase",
        &serde_json::json!({
            "phase": "project_ui_state",
            "elapsed_ms": started.elapsed_ms(),
        }),
    );
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
    crate::core_debug_log(
        "plan.procedure.select.core_phase",
        &serde_json::json!({
            "phase": "target_resolved",
            "airport_component_index": airport_component_index,
            "replace_component_index": replace_component_index,
            "elapsed_ms": started.elapsed_ms(),
        }),
    );
    let start_component_index = airport_component_index.checked_sub(1);
    let store = session_nav_kv_store(session)?;
    let procedure_component_index = replace_component_index
        .or(start_component_index)
        .unwrap_or(airport_component_index);
    let materialize_started = crate::CoreDebugTimer::start();
    let built = match materialize_procedure(
        store,
        &airport_id,
        &procedure_id,
        kind,
        runway_transition.as_deref(),
        enroute_transition.as_deref(),
        procedure_component_index,
    ) {
        Ok(built) => {
            crate::core_debug_log(
                "plan.procedure.select.core_phase",
                &serde_json::json!({
                    "phase": "materialize_done",
                    "elapsed_ms": materialize_started.elapsed_ms(),
                    "total_ms": started.elapsed_ms(),
                }),
            );
            built
        }
        Err(HadReadError::NeedPages(pages)) => {
            crate::core_debug_log(
                "plan.procedure.select.core_phase",
                &serde_json::json!({
                    "phase": "materialize_need_pages",
                    "resource_count": pages.len(),
                    "pages": pages,
                    "elapsed_ms": materialize_started.elapsed_ms(),
                    "total_ms": started.elapsed_ms(),
                }),
            );
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
    let mutation_started = crate::CoreDebugTimer::start();
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
    crate::core_debug_log(
        "plan.procedure.select.core_phase",
        &serde_json::json!({
            "phase": "mutation_done",
            "elapsed_ms": mutation_started.elapsed_ms(),
            "total_ms": started.elapsed_ms(),
        }),
    );
    let commit_started = crate::CoreDebugTimer::start();
    let outcome =
        commit_session_flight_plan_with_invalidations_outcome(session, mutation.mutation.plan);
    crate::core_debug_log(
        "plan.procedure.select.core_phase",
        &serde_json::json!({
            "phase": "commit_done",
            "ok": outcome.is_ok(),
            "elapsed_ms": commit_started.elapsed_ms(),
            "total_ms": started.elapsed_ms(),
        }),
    );
    outcome
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
    commit_session_flight_plan_with_invalidations_outcome(session, mutation.mutation.plan)
}

fn activate_direct_to_nav_ref_in_session_outcome(
    handle: u32,
    target: NavRef,
) -> AppResult<HadOperationOutcome> {
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
    commit_session_flight_plan_with_invalidations_outcome(session, next_plan)
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
    commit_session_flight_plan_with_invalidations_outcome(session, next_plan)
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
    attach_nav_kv_store_to_session_with_open_result(handle, store_id, store, None)
}

pub fn attach_nav_kv_store_to_session_with_open_result(
    handle: u32,
    store_id: u32,
    store: &NavKvStore,
    open_result: Option<&NavDbOpenResult>,
) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.nav_kv_store_id = Some(store_id);
    session.nav_kv_store = Some(store.clone());
    session.nav_db_artifact = open_result.map(AttachedNavDbArtifact::from);
    session.important_metar_station_ids = None;
    session.metar_station_importance_status = None;
    rebuild_metar_tile_cache(session);
    sync_cycle_product_freshness_status_records(session);
    Ok(())
}

pub fn insert_nav_kv_page_for_attached_sessions(store_id: u32, page_index: u32, page_bytes: &[u8]) {
    let mut sessions = lock_sessions();
    for session in sessions.values_mut() {
        if session.nav_kv_store_id == Some(store_id) {
            if let Some(store) = session.nav_kv_store.as_mut() {
                store.insert_page(page_index, page_bytes.to_vec());
            }
            if session
                .cycle_product_freshness
                .missing_nav_kv_pages
                .remove(&page_index)
            {
                session.cycle_product_freshness.dirty = true;
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
    get_session_snapshot_at_epoch_ms(handle, 0)
}

pub fn get_session_snapshot_at_epoch_ms(
    handle: u32,
    epoch_ms: i64,
) -> AppResult<UiSessionSnapshot> {
    let total_started_at = crate::core_clock_ms();
    let lock_started_at = crate::core_clock_ms();
    let mut sessions = lock_sessions();
    let lock_ms = elapsed_ms(lock_started_at);
    let lookup_started_at = crate::core_clock_ms();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    let lookup_ms = elapsed_ms(lookup_started_at);
    let cycle_freshness_started_at = crate::core_clock_ms();
    sync_cycle_product_freshness_status_records_if_needed(session);
    let cycle_freshness_ms = elapsed_ms(cycle_freshness_started_at);
    let live_feed_status_started_at = crate::core_clock_ms();
    sync_live_feed_overlay_status_records(session);
    let live_feed_status_ms = elapsed_ms(live_feed_status_started_at);
    let status_record_count = session.data_status_records.len();
    let pending_resource_effect_count = session.pending_resource_effects.len();
    let snapshot_started_at = crate::core_clock_ms();
    let result = snapshot_for_session(session);
    let snapshot_ms = elapsed_ms(snapshot_started_at);
    crate::core_debug_log(
        "session.snapshot.total",
        &serde_json::json!({
            "total_ms": elapsed_ms(total_started_at),
            "lock_ms": lock_ms,
            "lookup_ms": lookup_ms,
            "cycle_freshness_ms": cycle_freshness_ms,
            "live_feed_status_ms": live_feed_status_ms,
            "snapshot_ms": snapshot_ms,
            "status_record_count": status_record_count,
            "pending_resource_effect_count": pending_resource_effect_count,
            "status": if result.is_ok() { "ok" } else { "error" },
        }),
    );
    result
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
            _ => {}
        }
    }
    sync_live_feed_overlay_status_records(session);
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
        airspace_refs: Vec::new(),
        airspace_labels: Vec::new(),
    }
}

pub fn ingest_tfrs_in_session(handle: u32, payload: &TfrProductPayload) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.tfr_payload = Some(payload.clone());
    sync_live_feed_overlay_status_records(session);
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
    ingest_live_feed_sse_event_in_session_at_epoch_ms(handle, event, 0)
}

pub fn ingest_live_feed_sse_event_in_session_at_epoch_ms(
    handle: u32,
    event: &LiveFeedSseEvent,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    record_live_feed_connection_event(
        session,
        LiveFeedConnectionEvent {
            kind: LiveFeedConnectionEventKind::Message,
            message: None,
        },
        epoch_ms,
    );
    session.live_feeds.ingest_sse_event(event.clone())
}

pub fn ingest_live_feed_sse_events_in_session(
    handle: u32,
    events: &[LiveFeedSseEvent],
) -> AppResult<HadOperationOutcome> {
    ingest_live_feed_sse_events_in_session_at_epoch_ms(handle, events, 0)
}

pub fn ingest_live_feed_sse_events_in_session_at_epoch_ms(
    handle: u32,
    events: &[LiveFeedSseEvent],
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if !events.is_empty() {
        record_live_feed_connection_event(
            session,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Message,
                message: None,
            },
            epoch_ms,
        );
    }
    let affected = session
        .live_feeds
        .ingest_sse_events(events.iter().cloned())?;
    Ok(session
        .live_feeds
        .sync_products_outcome_with_invalidations(affected.iter().map(String::as_str)))
}

pub fn ingest_resource_in_session(handle: u32, resource_id: &str, bytes: &[u8]) -> AppResult<()> {
    ingest_resource_in_session_at_epoch_ms(handle, resource_id, bytes, 0)
}

pub fn ingest_resource_in_session_at_epoch_ms(
    handle: u32,
    resource_id: &str,
    bytes: &[u8],
    epoch_ms: i64,
) -> AppResult<()> {
    if LiveFeedsState::handles_resource(resource_id) {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        if resource_id == "live_feeds/current" {
            record_live_feed_connection_event(
                session,
                LiveFeedConnectionEvent {
                    kind: LiveFeedConnectionEventKind::Message,
                    message: None,
                },
                epoch_ms,
            );
        }
        session.live_feeds.ingest_resource(resource_id, bytes)?;
        install_live_feed_payloads(session)?;
        return Ok(());
    }
    if resource_id.starts_with(LIVE_OBSTACLE_HAD_RESOURCE_PREFIX) {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        ingest_live_obstacle_had_resource(session, resource_id, bytes)?;
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
        if resource_id == "publication/current_artifacts" {
            advance_session_wall_clock(session, epoch_ms);
            session.current_artifacts_checked_epoch_ms = Some(session.wall_clock_epoch_ms);
        }
        mark_cycle_product_freshness_dirty(session);
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

pub fn report_live_feed_connection_event_in_session(
    handle: u32,
    event: LiveFeedConnectionEvent,
    epoch_ms: i64,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    record_live_feed_connection_event(session, event, epoch_ms);
    snapshot_for_session(session)
}

fn record_live_feed_connection_event(
    session: &mut UiSession,
    event: LiveFeedConnectionEvent,
    epoch_ms: i64,
) {
    advance_session_wall_clock(session, epoch_ms);
    let at = session.wall_clock_epoch_ms;
    match event.kind {
        LiveFeedConnectionEventKind::Connecting => {
            session.live_feed_connection.mode = LiveFeedConnectionMode::Connecting;
            session.live_feed_connection.last_state_change_epoch_ms = Some(at);
        }
        LiveFeedConnectionEventKind::Connected => {
            session.live_feed_connection.mode = LiveFeedConnectionMode::Connected;
            session.live_feed_connection.last_state_change_epoch_ms = Some(at);
            session.live_feed_connection.last_error_message = None;
        }
        LiveFeedConnectionEventKind::Message => {
            session.live_feed_connection.mode = LiveFeedConnectionMode::Connected;
            session.live_feed_connection.last_state_change_epoch_ms = session
                .live_feed_connection
                .last_state_change_epoch_ms
                .or(Some(at));
            session.live_feed_connection.last_heard_epoch_ms = Some(at);
            session.live_feed_connection.last_error_message = None;
        }
        LiveFeedConnectionEventKind::Error => {
            session.live_feed_connection.mode = LiveFeedConnectionMode::Error;
            session.live_feed_connection.last_state_change_epoch_ms = Some(at);
            session.live_feed_connection.last_error_epoch_ms = Some(at);
            session.live_feed_connection.last_error_message = event.message;
        }
        LiveFeedConnectionEventKind::Closed => {
            session.live_feed_connection.mode = LiveFeedConnectionMode::Closed;
            session.live_feed_connection.last_state_change_epoch_ms = Some(at);
        }
    }
}

pub fn ingest_prepared_metar_live_feed_resource_in_session(
    handle: u32,
    resource_id: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let envelope = crate::decode_prepared_metar_live_feed(bytes)?;
    let envelope_version = envelope.version.clone();
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session
        .live_feeds
        .ingest_prepared_metar_live_feed(resource_id, &envelope)?;
    if session.live_feeds.product_loaded_version("metars") != Some(envelope_version.as_str()) {
        return Ok(());
    }
    install_prepared_metar_live_feed(session, envelope)?;
    clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
    Ok(())
}

pub fn install_live_feed_installed_state_in_session(
    handle: u32,
    installed: &crate::LiveFeedInstalledState,
) -> AppResult<UiSessionSnapshot> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    install_live_feed_installed_state(session, installed)?;
    snapshot_for_session(session)
}

fn install_live_feed_installed_state(
    session: &mut UiSession,
    installed: &crate::LiveFeedInstalledState,
) -> AppResult<()> {
    match (&*installed.product, &installed.payload) {
        ("metars", crate::LiveFeedInstalledPayload::Json { bytes }) => {
            let payload: MetarProductPayload =
                serde_json::from_slice(bytes).map_err(|err| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to parse installed METAR live feed: {err}"),
                })?;
            session.metar_payload = Some(payload);
            session.prepared_metar_feed = None;
            rebuild_metar_tile_cache(session);
            clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
        }
        ("tfrs", crate::LiveFeedInstalledPayload::Json { bytes }) => {
            let payload: TfrProductPayload =
                serde_json::from_slice(bytes).map_err(|err| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to parse installed TFR live feed: {err}"),
                })?;
            session.tfr_payload = Some(payload);
            clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
        }
        (
            "obstacles",
            crate::LiveFeedInstalledPayload::NavKv {
                manifest,
                root,
                pages,
            },
        ) => {
            let manifest_value: serde_json::Value =
                serde_json::from_slice(manifest).map_err(|err| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to parse installed obstacle live feed: {err}"),
                })?;
            let store = nav_kv_store_from_installed_live_feed(
                "obstacles",
                &installed.version,
                &installed.state_sha256,
                root,
                pages,
            )?;
            install_live_obstacle_had_with_store(
                session,
                manifest_value,
                installed.version.clone(),
                format!(
                    "android-installed/obstacles/{}/manifest.json",
                    installed.version
                ),
                Some(store),
            )?;
        }
        ("nexrad", crate::LiveFeedInstalledPayload::Opaque { bytes }) => {
            session.nexrad_installed = Some(read_installed_nexrad_package(
                &installed.version,
                &installed.state_sha256,
                bytes,
            )?);
            clear_data_status_record(session, LIVE_FEED_NEXRAD_STATUS_ID);
        }
        ("winds-aloft", crate::LiveFeedInstalledPayload::Json { .. }) => {}
        (product, payload) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidManifest,
                message: format!(
                    "installed live feed {product} has incompatible payload kind {}",
                    payload.kind_name()
                ),
            });
        }
    }
    Ok(())
}

fn install_prepared_metar_live_feed(
    session: &mut UiSession,
    envelope: crate::PreparedMetarLiveFeedEnvelope,
) -> AppResult<()> {
    let generated_at_utc = envelope
        .feed
        .generated_at_utc
        .as_deref()
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&chrono::Utc))
                .map_err(|err| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to parse prepared METAR generated_at_utc: {err}"),
                })
        })
        .transpose()?;
    let metars_by_station = envelope
        .feed
        .records
        .iter()
        .cloned()
        .map(|record| (record.station_id.clone(), record))
        .collect::<HashMap<_, _>>();
    session.metar_payload = Some(MetarProductPayload {
        schema_version: 3,
        version_label: envelope.feed.version_label.clone(),
        generated_at_utc,
        metar_count: Some(envelope.feed.records.len() as u32),
        metars_by_station,
        pireps: Vec::new(),
    });
    session.prepared_metar_feed = Some(envelope.feed);
    rebuild_metar_tile_cache(session);
    Ok(())
}

fn read_installed_nexrad_package(
    version: &str,
    state_sha256: &str,
    bytes: &[u8],
) -> AppResult<LiveNexradInstalledState> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|err| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("failed to read installed NEXRAD package: {err}"),
    })?;
    let mut members = HashMap::new();
    for index in 0..archive.len() {
        let mut member = archive.by_index(index).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to read installed NEXRAD package member {index}: {err}"),
        })?;
        if member.is_dir() {
            continue;
        }
        let Some(name) = member
            .enclosed_name()
            .map(|path| path.to_string_lossy().to_string())
        else {
            return Err(AppError {
                kind: AppErrorKind::InvalidManifest,
                message: "installed NEXRAD package contains unsafe member path".to_string(),
            });
        };
        let mut member_bytes = Vec::new();
        member
            .read_to_end(&mut member_bytes)
            .map_err(|err| AppError {
                kind: AppErrorKind::InvalidManifest,
                message: format!("failed to read installed NEXRAD package member {name}: {err}"),
            })?;
        members.insert(name, member_bytes);
    }
    let manifest_bytes = members.get("manifest.json").ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: "installed NEXRAD package is missing manifest.json".to_string(),
    })?;
    let manifest: serde_json::Value =
        serde_json::from_slice(manifest_bytes).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse installed NEXRAD manifest: {err}"),
        })?;
    let state_id = manifest
        .get("state_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(version);
    if state_id != version {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "installed NEXRAD manifest state_id {state_id} did not match {version}"
            ),
        });
    }
    let actual_state_sha256 = canonical_json_sha256_value(&manifest)?;
    if actual_state_sha256 != state_sha256 {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "installed NEXRAD state hash mismatch: expected {state_sha256}, got {actual_state_sha256}"
            ),
        });
    }
    Ok(LiveNexradInstalledState {
        version: version.to_string(),
        manifest,
        members,
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
        sync_live_feed_overlay_status_records(session);
    } else if resource_id.starts_with("terrain/source/")
        && session.map_layer_state.terrain_warning.visible
    {
        upsert_data_status_record(
            session,
            terrain_status_record(format!(
                "Terrain warning unavailable: failed to load terrain source {resource_id}: {message}"
            )),
        );
    } else if resource_id.starts_with(LIVE_OBSTACLE_HAD_RESOURCE_PREFIX) {
        upsert_data_status_record(
            session,
            live_feed_unavailable_status_record(
                "obstacles",
                format!("Obstacle live feed unavailable: failed to fetch HAD resource: {message}"),
            ),
        );
    }
    snapshot_for_session(session)
}

fn ingest_live_obstacle_had_resource(
    session: &mut UiSession,
    resource_id: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let Some((version, member)) = live_obstacle_had_resource_parts(resource_id) else {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("invalid obstacle HAD resource id: {resource_id}"),
        });
    };
    let Some(had) = session.obstacle_had.as_mut() else {
        return Ok(());
    };
    if had.version != version {
        return Ok(());
    }
    if member == "root" {
        let root = NavKvRoot::parse(bytes).map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse obstacle HAD root: {message}"),
        })?;
        if root.page_count() != had.page_count {
            return Err(AppError {
                kind: AppErrorKind::InvalidManifest,
                message: format!(
                    "obstacle HAD root page_count {} did not match manifest page_count {}",
                    root.page_count(),
                    had.page_count
                ),
            });
        }
        had.store = Some(NavKvStore::new(root));
        session.obstacle_tile_cache.clear();
        clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
        return Ok(());
    }
    let Some(page_text) = member.strip_prefix("page/") else {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("invalid obstacle HAD resource id: {resource_id}"),
        });
    };
    let page_index = page_text.parse::<u32>().map_err(|err| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("invalid obstacle HAD page resource id {resource_id}: {err}"),
    })?;
    if page_index >= had.page_count {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "obstacle HAD page {page_index} exceeds page_count {}",
                had.page_count
            ),
        });
    }
    let Some(store) = had.store.as_mut() else {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("obstacle HAD page arrived before root: {resource_id}"),
        });
    };
    store.insert_page(page_index, bytes.to_vec());
    clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
    Ok(())
}

fn install_live_feed_payloads(session: &mut UiSession) -> AppResult<()> {
    if session.live_feeds.current_loaded() {
        if !session.live_feeds.has_product_current_version("metars") {
            session.metar_tile_cache.clear();
            session.metar_payload = None;
            session.prepared_metar_feed = None;
            clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
        }
        if !session.live_feeds.has_product_current_version("tfrs") {
            session.tfr_payload = None;
            clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
        }
        if !session.live_feeds.has_product_current_version("obstacles") {
            session.obstacle_had = None;
            session.obstacle_tile_cache.clear();
            session.map_overlay_config.obstacle_layer = None;
            clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
        }
    }
    let loaded_metars_version = session.live_feeds.product_loaded_version("metars");
    let metars_installed = session
        .metar_payload
        .as_ref()
        .and_then(|payload| loaded_metars_version.map(|version| payload.version_label == version))
        .unwrap_or(false);
    if !metars_installed {
        if let Some(metars_value) = session.live_feeds.product_state_manifest("metars").cloned() {
            match serde_json::from_value::<MetarProductPayload>(metars_value) {
                Ok(payload) => {
                    session.metar_payload = Some(payload);
                    session.prepared_metar_feed = None;
                    rebuild_metar_tile_cache(session);
                    clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
                }
                Err(err) => {
                    session.metar_tile_cache.clear();
                    session.metar_payload = None;
                    session.prepared_metar_feed = None;
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
    }
    let loaded_tfrs_version = session.live_feeds.product_loaded_version("tfrs");
    let tfrs_installed = session
        .tfr_payload
        .as_ref()
        .and_then(|payload| loaded_tfrs_version.map(|version| payload.version_label == version))
        .unwrap_or(false);
    if !tfrs_installed {
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
    }
    let loaded_obstacles_version = session.live_feeds.product_loaded_version("obstacles");
    let obstacles_installed = session
        .obstacle_had
        .as_ref()
        .and_then(|had| loaded_obstacles_version.map(|version| had.version == version))
        .unwrap_or(false);
    if !obstacles_installed {
        if let Some(obstacles_value) = session
            .live_feeds
            .product_state_manifest("obstacles")
            .cloned()
        {
            if let Err(err) = install_live_obstacle_had(session, obstacles_value) {
                session.obstacle_had = None;
                session.obstacle_tile_cache.clear();
                session.map_overlay_config.obstacle_layer = None;
                upsert_data_status_record(
                    session,
                    live_feed_unavailable_status_record(
                        "obstacles",
                        format!("Obstacle live feed unavailable: failed to parse state: {err}"),
                    ),
                );
            }
        }
    }
    sync_live_feed_overlay_status_records(session);
    Ok(())
}

fn install_live_obstacle_had(
    session: &mut UiSession,
    manifest_value: serde_json::Value,
) -> AppResult<()> {
    let manifest: LiveObstacleHadManifest = serde_json::from_value(manifest_value.clone())
        .map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse obstacle live feed HAD manifest: {err}"),
        })?;
    let Some(version) = session
        .live_feeds
        .product_loaded_version("obstacles")
        .map(str::to_string)
    else {
        return Ok(());
    };
    let Some(state_url) = session
        .live_feeds
        .product_state_url("obstacles")
        .map(str::to_string)
    else {
        return Ok(());
    };
    install_live_obstacle_had_with_parsed_manifest(
        session,
        manifest_value,
        manifest,
        version,
        state_url,
        None,
    )
}

fn install_live_obstacle_had_with_store(
    session: &mut UiSession,
    manifest_value: serde_json::Value,
    version: String,
    state_url: String,
    store: Option<NavKvStore>,
) -> AppResult<()> {
    let manifest: LiveObstacleHadManifest = serde_json::from_value(manifest_value.clone())
        .map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse obstacle live feed HAD manifest: {err}"),
        })?;
    install_live_obstacle_had_with_parsed_manifest(
        session,
        manifest_value,
        manifest,
        version,
        state_url,
        store,
    )
}

fn install_live_obstacle_had_with_parsed_manifest(
    session: &mut UiSession,
    manifest_value: serde_json::Value,
    manifest: LiveObstacleHadManifest,
    version: String,
    state_url: String,
    store: Option<NavKvStore>,
) -> AppResult<()> {
    if manifest.schema_version != 1 {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "unsupported obstacle live feed HAD manifest schema_version {}",
                manifest.schema_version
            ),
        });
    }
    if manifest.product_id != "obstacles" {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "obstacle live feed HAD manifest contained product_id {}",
                manifest.product_id
            ),
        });
    }
    if manifest.encoding != format!("had-nav-kv-v{}", had_nav_kv::VERSION) {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "unsupported obstacle live feed HAD encoding {}",
                manifest.encoding
            ),
        });
    }
    if manifest.version_label != version {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "obstacle live feed HAD manifest version {} did not match loaded version {version}",
                manifest.version_label
            ),
        });
    }
    let obstacle_layer = obstacle_layer_config_from_live_manifest_value(manifest_value)?;
    let same_state = session.obstacle_had.as_ref().is_some_and(|existing| {
        existing.version == version
            && existing.state_url == state_url
            && existing.root_member_path == manifest.root
            && existing.page_path_template == manifest.page_path_template
            && existing.page_count == manifest.page_count
            && existing.state_sha256 == manifest.state_sha256
    });
    let preserve_store = store.or_else(|| {
        if same_state {
            session
                .obstacle_had
                .as_ref()
                .and_then(|existing| existing.store.clone())
        } else {
            None
        }
    });
    session.map_overlay_config.obstacle_layer = Some(obstacle_layer);
    session.obstacle_had = Some(LiveObstacleHadState {
        version,
        state_url,
        root_member_path: manifest.root,
        page_path_template: manifest.page_path_template,
        page_count: manifest.page_count,
        state_sha256: manifest.state_sha256,
        store: preserve_store,
    });
    if !same_state {
        session.obstacle_tile_cache.clear();
    }
    clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
    Ok(())
}

fn nav_kv_store_from_installed_live_feed(
    product: &str,
    version: &str,
    state_sha256: &str,
    root: &[u8],
    pages: &[Vec<u8>],
) -> AppResult<NavKvStore> {
    let root = NavKvRoot::parse(root).map_err(|message| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("failed to parse installed {product} HAD root: {message}"),
    })?;
    if root.page_count() as usize != pages.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "installed {product} HAD root page_count {} did not match payload page_count {}",
                root.page_count(),
                pages.len()
            ),
        });
    }
    let actual = root
        .canonical_sha256(|page| pages.get(page as usize).cloned())
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to hash installed {product} HAD {version}"),
        })?;
    if actual != state_sha256 {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "installed {product} HAD {version} hash mismatch: expected {state_sha256}, got {actual}"
            ),
        });
    }
    let mut store = NavKvStore::new(root);
    for (page_index, page) in pages.iter().enumerate() {
        store.insert_page(page_index as u32, page.clone());
    }
    Ok(store)
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

fn metar_tile_cache_for_prepared_live_feed(
    feed: &crate::PreparedMetarLiveFeed,
    layer: Option<&crate::map_overlay::PointTileLayerConfig>,
    important_station_ids: &HashSet<String>,
) -> Option<HashMap<String, MetarTilePayload>> {
    let Some(layer) = layer else {
        return Some(HashMap::new());
    };
    let available_zooms = layer
        .available_zooms
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let feed_zooms = feed.tiles.iter().map(|tile| tile.z).collect::<HashSet<_>>();
    if !available_zooms.is_subset(&feed_zooms) {
        return None;
    }
    let mut cache = HashMap::new();
    for tile in &feed.tiles {
        if !available_zooms.contains(&tile.z) {
            continue;
        }
        let mut records = Vec::new();
        for index in &tile.record_indexes {
            let Some(record) = feed.records.get(*index as usize) else {
                continue;
            };
            if tile.z == layer.min_zoom && !important_station_ids.contains(&record.station_id) {
                continue;
            }
            records.push(MetarTileRecord {
                kind: "metar".to_string(),
                id: record.station_id.clone(),
            });
        }
        if records.is_empty() {
            continue;
        }
        let key = crate::tile_key("metars", tile.z, tile.x, tile.y);
        cache.insert(
            key,
            MetarTilePayload {
                schema_version: 1,
                layer: "metars".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records,
            },
        );
    }
    Some(cache)
}

fn rebuild_metar_tile_cache(session: &mut UiSession) {
    if let Some(feed) = session.prepared_metar_feed.as_ref() {
        let empty = HashSet::new();
        let important_station_ids = session
            .important_metar_station_ids
            .as_ref()
            .unwrap_or(&empty);
        if let Some(cache) = metar_tile_cache_for_prepared_live_feed(
            feed,
            session.map_overlay_config.metar_layer.as_ref(),
            important_station_ids,
        ) {
            session.metar_tile_cache = cache;
            return;
        }
    }
    if let Some(payload) = session.metar_payload.as_ref() {
        let empty = HashSet::new();
        let important_station_ids = session
            .important_metar_station_ids
            .as_ref()
            .unwrap_or(&empty);
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
    if session.important_metar_station_ids.is_some() {
        return Ok(());
    }
    let Some(store) = session.nav_kv_store.as_ref() else {
        session.important_metar_station_ids = Some(HashSet::new());
        session.metar_station_importance_status = Some(metar_station_importance_status_record(
            "Station importance unavailable",
            UiStatusSeverity::Caution,
            "METAR low-zoom station filtering could not load because no nav-db store is attached.",
        ));
        rebuild_metar_tile_cache(session);
        return Ok(());
    };
    let Some(payload) = read_attached_json_optional::<MetarImportantStationsPayload>(
        store,
        NavKvQuery::MetarImportantStations,
    )?
    else {
        session.important_metar_station_ids = Some(HashSet::new());
        session.metar_station_importance_status = Some(metar_station_importance_status_record(
            "Station importance missing",
            UiStatusSeverity::Caution,
            "METAR low-zoom station filtering could not find weather/metar-important-stations in nav-db. Low-zoom METARs are hidden until the current nav-db provides that record.",
        ));
        rebuild_metar_tile_cache(session);
        return Ok(());
    };
    if payload.schema_version != 1 {
        return Err(HadReadError::Fatal(format!(
            "unsupported METAR important station schema version {}",
            payload.schema_version
        )));
    }
    let station_ids = payload
        .station_ids
        .into_iter()
        .map(|station_id| station_id.trim().to_ascii_uppercase())
        .filter(|station_id| !station_id.is_empty())
        .collect::<HashSet<_>>();
    session.important_metar_station_ids = Some(station_ids);
    session.metar_station_importance_status = None;
    rebuild_metar_tile_cache(session);
    Ok(())
}

fn metar_importance_required_for_viewport(session: &UiSession, viewport: &MapViewport) -> bool {
    if !session.map_layer_state.metars.visible || session.metar_payload.is_none() {
        return false;
    }
    let Some(layer) = session.map_overlay_config.metar_layer.as_ref() else {
        return false;
    };
    let desired_zoom = if viewport.zoom.is_finite() && viewport.zoom > 0.0 {
        viewport.zoom.floor() as u32
    } else {
        0
    };
    nearest_available_layer_zoom(layer, desired_zoom) == layer.min_zoom
}

fn metar_station_importance_status_record(
    value: impl Into<String>,
    severity: UiStatusSeverity,
    detail: impl Into<String>,
) -> DataStatusRecord {
    DataStatusRecord::new(
        METAR_STATION_IMPORTANCE_STATUS_ID,
        "METAR",
        Some(value.into()),
        severity,
        false,
        detail.into(),
    )
}

fn try_ensure_metar_station_importance_loaded(session: &mut UiSession) -> Option<DataStatusRecord> {
    if session.important_metar_station_ids.is_some() {
        return session.metar_station_importance_status.clone();
    }
    match ensure_metar_station_importance_loaded(session) {
        Ok(()) => session.metar_station_importance_status.clone(),
        Err(HadReadError::NeedPages(pages)) => {
            for resource in nav_kv_page_resources(pages) {
                enqueue_session_resource_effect(session, resource, [UiInvalidation::MapOverlay]);
            }
            rebuild_metar_tile_cache(session);
            Some(metar_station_importance_status_record(
                "Station importance loading",
                UiStatusSeverity::Info,
                "METAR low-zoom station filtering is waiting for weather/metar-important-stations. Low-zoom METARs are hidden until that record is available; map vectors and high-zoom METARs remain available.",
            ))
        }
        Err(HadReadError::Fatal(message)) => {
            session.important_metar_station_ids = Some(HashSet::new());
            session.metar_station_importance_status = Some(metar_station_importance_status_record(
                "Station importance failed",
                UiStatusSeverity::Caution,
                format!("METAR low-zoom station filtering failed: {message}"),
            ));
            rebuild_metar_tile_cache(session);
            session.metar_station_importance_status.clone()
        }
    }
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

fn canonical_json_sha256_value(value: &serde_json::Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(internal_json_error)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn live_feed_state_member_address(state_url: &str, member_path: &str) -> String {
    let base = state_url
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or("");
    let member = member_path.trim_start_matches('/');
    if base.is_empty() {
        format!("/live-feeds/{member}")
    } else {
        format!("/live-feeds/{base}/{member}")
    }
}

fn live_obstacle_had_root_resource(had: &LiveObstacleHadState) -> CoreResourceRequest {
    CoreResourceRequest::public_url(
        format!(
            "{}{}{}",
            LIVE_OBSTACLE_HAD_RESOURCE_PREFIX, had.version, "/root"
        ),
        live_feed_state_member_address(&had.state_url, &had.root_member_path),
        false,
    )
}

fn live_obstacle_had_page_member_path(had: &LiveObstacleHadState, page: u32) -> String {
    had.page_path_template
        .replace("{page:04}", &format!("{page:04}"))
        .replace("{page}", &page.to_string())
}

fn live_obstacle_had_page_resource(had: &LiveObstacleHadState, page: u32) -> CoreResourceRequest {
    let member_path = live_obstacle_had_page_member_path(had, page);
    CoreResourceRequest::public_url(
        format!(
            "{}{}/page/{page:04}",
            LIVE_OBSTACLE_HAD_RESOURCE_PREFIX, had.version
        ),
        live_feed_state_member_address(&had.state_url, &member_path),
        false,
    )
}

fn live_obstacle_had_page_resources(
    had: &LiveObstacleHadState,
    pages: Vec<u32>,
) -> Vec<CoreResourceRequest> {
    let mut pages = pages;
    pages.sort_unstable();
    pages.dedup();
    pages
        .into_iter()
        .map(|page| live_obstacle_had_page_resource(had, page))
        .collect()
}

fn live_obstacle_had_resource_parts(resource_id: &str) -> Option<(&str, &str)> {
    let rest = resource_id.strip_prefix(LIVE_OBSTACLE_HAD_RESOURCE_PREFIX)?;
    rest.split_once('/')
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
        NavKvLookup::MissingPages(pages) => {
            crate::core_debug_log(
                "core.had.key_page_fault",
                &serde_json::json!({
                    "key": key,
                    "pages": pages,
                    "page_count": pages.len(),
                }),
            );
            Err(HadReadError::NeedPages(pages))
        }
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
    let obstacle_layer = session.map_overlay_config.obstacle_layer.clone();
    let mut config = map_overlay_config_from_vector_manifest_json(&manifest_json)
        .map_err(|err| err.to_string())?;
    config.obstacle_layer = obstacle_layer;
    session.map_overlay_config = config;
    rebuild_metar_tile_cache(session);
    session.vector_manifest_loaded = true;
    Ok(())
}

#[derive(Default)]
struct VectorInputLoadStats {
    iterations: u32,
    manifest_ms: u64,
    request_ms: u64,
    key_ms: u64,
    missing_pages_ms: u64,
    read_tiles_ms: u64,
    read_features_ms: u64,
    insert_ms: u64,
    probe: NavKvPageProbeStats,
    total_needed_vector_tiles: usize,
    total_needed_airspace_features: usize,
    needed_pages: usize,
    loaded_vector_tiles: usize,
    loaded_airspace_features: usize,
}

fn merge_nav_kv_probe_stats(total: &mut NavKvPageProbeStats, next: NavKvPageProbeStats) {
    total.keys += next.keys;
    total.node_page_hits += next.node_page_hits;
    total.node_page_misses += next.node_page_misses;
    total.leaf_entries_scanned += next.leaf_entries_scanned;
    total.inline_values += next.inline_values;
    total.external_values += next.external_values;
    total.value_page_hits += next.value_page_hits;
    total.value_page_misses += next.value_page_misses;
}

fn ensure_vector_inputs_loaded(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
) -> Result<(), HadReadError> {
    let total_started_at = crate::core_clock_ms();
    let mut stats = VectorInputLoadStats::default();
    let result = ensure_vector_inputs_loaded_impl(session, metrics, &mut stats);
    let status = match &result {
        Ok(()) => "ok",
        Err(HadReadError::NeedPages(_)) => "need_pages",
        Err(HadReadError::Fatal(_)) => "fatal",
    };
    let error = match &result {
        Ok(()) => None,
        Err(HadReadError::NeedPages(_)) => None,
        Err(HadReadError::Fatal(message)) => Some(message.as_str()),
    };
    crate::core_debug_log(
        "map.overlay.vector_inputs",
        &serde_json::json!({
            "status": status,
            "error": error,
            "total_ms": elapsed_ms(total_started_at),
            "manifest_ms": stats.manifest_ms,
            "request_ms": stats.request_ms,
            "key_ms": stats.key_ms,
            "missing_pages_ms": stats.missing_pages_ms,
            "read_tiles_ms": stats.read_tiles_ms,
            "read_features_ms": stats.read_features_ms,
            "insert_ms": stats.insert_ms,
            "probe_keys": stats.probe.keys,
            "probe_node_page_hits": stats.probe.node_page_hits,
            "probe_node_page_misses": stats.probe.node_page_misses,
            "probe_leaf_entries_scanned": stats.probe.leaf_entries_scanned,
            "probe_inline_values": stats.probe.inline_values,
            "probe_external_values": stats.probe.external_values,
            "probe_value_page_hits": stats.probe.value_page_hits,
            "probe_value_page_misses": stats.probe.value_page_misses,
            "iterations": stats.iterations,
            "total_needed_vector_tiles": stats.total_needed_vector_tiles,
            "total_needed_airspace_features": stats.total_needed_airspace_features,
            "needed_pages": stats.needed_pages,
            "loaded_vector_tiles": stats.loaded_vector_tiles,
            "loaded_airspace_features": stats.loaded_airspace_features,
            "cached_vector_tiles": session.vector_tile_cache.len(),
            "cached_airspace_features": session.airspace_feature_cache.len(),
        }),
    );
    result
}

fn ensure_vector_inputs_loaded_impl(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
    stats: &mut VectorInputLoadStats,
) -> Result<(), HadReadError> {
    let manifest_started_at = crate::core_clock_ms();
    ensure_vector_manifest_loaded(session)?;
    stats.manifest_ms += elapsed_ms(manifest_started_at);
    for _ in 0..8 {
        stats.iterations += 1;
        let request_started_at = crate::core_clock_ms();
        let inputs = vector_overlay_input_requests(
            metrics,
            &session.map_overlay_config,
            &session.vector_tile_cache,
            &session.airspace_feature_cache,
        );
        stats.request_ms += elapsed_ms(request_started_at);
        let needed_vector_inputs =
            inputs.needed_vector_tiles.len() + inputs.needed_airspace_features.len();
        stats.total_needed_vector_tiles += inputs.needed_vector_tiles.len();
        stats.total_needed_airspace_features += inputs.needed_airspace_features.len();
        if needed_vector_inputs == 0 {
            return Ok(());
        }

        let mut loaded_any = false;
        let store = session.nav_kv_store.as_ref().ok_or_else(|| {
            HadReadError::Fatal("session missing nav kv store for vector overlay".to_string())
        })?;
        let keys_started_at = crate::core_clock_ms();
        let input_keys = vector_input_keys(&inputs);
        stats.key_ms += elapsed_ms(keys_started_at);
        let missing_pages_started_at = crate::core_clock_ms();
        let (needed_pages, probe_stats) = store
            .missing_pages_for_keys_with_stats(&input_keys)
            .map_err(HadReadError::Fatal)?;
        merge_nav_kv_probe_stats(&mut stats.probe, probe_stats);
        stats.missing_pages_ms += elapsed_ms(missing_pages_started_at);
        stats.needed_pages += needed_pages.len();
        if !needed_pages.is_empty() {
            return Err(HadReadError::NeedPages(needed_pages));
        }

        let mut vector_tiles = Vec::new();
        let read_tiles_started_at = crate::core_clock_ms();
        for tile in inputs.needed_vector_tiles {
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
                airspace_refs: Vec::new(),
                airspace_labels: Vec::new(),
            });
            vector_tiles.push(payload);
        }
        stats.read_tiles_ms += elapsed_ms(read_tiles_started_at);

        let mut features = Vec::new();
        let read_features_started_at = crate::core_clock_ms();
        for feature in inputs.needed_airspace_features {
            let payload = read_attached_json_required::<AirspaceFeaturePayload>(
                store,
                NavKvQuery::VectorAirspaceFeature { id: feature.id },
                "vector airspace feature",
            )?;
            features.push(payload);
        }
        stats.read_features_ms += elapsed_ms(read_features_started_at);

        let insert_started_at = crate::core_clock_ms();
        for tile in vector_tiles {
            session.vector_tile_cache.insert(
                crate::aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y),
                tile,
            );
            stats.loaded_vector_tiles += 1;
            loaded_any = true;
        }
        for feature in features {
            session
                .airspace_feature_cache
                .insert(feature.id.clone(), feature);
            stats.loaded_airspace_features += 1;
            loaded_any = true;
        }
        stats.insert_ms += elapsed_ms(insert_started_at);
        if !loaded_any {
            return Ok(());
        }
    }
    Err(HadReadError::Fatal(
        "vector overlay did not converge after loading HAD inputs".to_string(),
    ))
}

fn queue_nav_kv_pages_for_map_overlay(session: &mut UiSession, pages: Vec<u32>) {
    for resource in nav_kv_page_resources(pages) {
        enqueue_session_resource_effect(session, resource, [UiInvalidation::MapOverlay]);
    }
}

fn vector_inputs_status_record(
    value: impl Into<String>,
    severity: UiStatusSeverity,
    detail: impl Into<String>,
) -> DataStatusRecord {
    DataStatusRecord::new(
        VECTOR_INPUTS_STATUS_ID,
        "Map overlay",
        Some(value.into()),
        severity,
        false,
        detail.into(),
    )
}

fn ensure_vector_inputs_loaded_for_map_overlay(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
) -> Vec<DataStatusRecord> {
    match ensure_vector_inputs_loaded(session, metrics) {
        Ok(()) => Vec::new(),
        Err(HadReadError::NeedPages(pages)) => {
            queue_nav_kv_pages_for_map_overlay(session, pages);
            vec![vector_inputs_status_record(
                "Loading vectors",
                UiStatusSeverity::Info,
                "Visible vector data is waiting for nav-db pages. The map is rendering the resident overlay data and will redraw when the pages arrive.",
            )]
        }
        Err(HadReadError::Fatal(message)) => vec![vector_inputs_status_record(
            "Vector overlay failed",
            UiStatusSeverity::Caution,
            format!("Vector overlay data could not be loaded: {message}"),
        )],
    }
}

fn vector_input_keys(inputs: &VectorOverlayInputRequests) -> Vec<String> {
    inputs
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
            inputs
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

fn ensure_live_obstacle_inputs_loaded(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
) -> Vec<DataStatusRecord> {
    if session.obstacle_had.is_none() {
        if let HadOperationOutcome::NeedResources { resources } =
            session.live_feeds.sync_product_outcome("obstacles")
        {
            for resource in resources {
                enqueue_session_resource_effect(session, resource, [UiInvalidation::MapOverlay]);
            }
            return Vec::new();
        }
    }
    let Some(layer) = session.map_overlay_config.obstacle_layer.clone() else {
        return Vec::new();
    };
    let Some(had) = session.obstacle_had.clone() else {
        return Vec::new();
    };
    if had.store.is_none() {
        enqueue_session_resource_effect(
            session,
            live_obstacle_had_root_resource(&had),
            [UiInvalidation::MapOverlay],
        );
        return vec![live_feed_unavailable_status_record(
            "obstacles",
            "Obstacle live feed is waiting for the current HAD root".to_string(),
        )];
    }
    let obstacle_context = ownship_overlay_context(session);
    let tiles = visible_obstacle_tile_window(
        &layer,
        &metrics.viewport,
        metrics.width_px,
        metrics.height_px,
        obstacle_context.as_ref(),
        metrics.display_scale,
    );
    if tiles.is_empty() {
        return Vec::new();
    }
    let keys = tiles
        .iter()
        .filter_map(|tile| {
            nav_kv_key_for_query(&NavKvQuery::ObstacleTile {
                z: tile.z,
                x: tile.x,
                y: tile.y,
            })
        })
        .collect::<Vec<_>>();
    let Some(store) = had.store.as_ref() else {
        return Vec::new();
    };
    let missing_pages = match store.missing_pages_for_keys(&keys) {
        Ok(pages) => pages,
        Err(message) => {
            return vec![live_feed_unavailable_status_record(
                "obstacles",
                format!("Obstacle live feed unavailable: failed to query HAD pages: {message}"),
            )];
        }
    };
    if !missing_pages.is_empty() {
        for resource in live_obstacle_had_page_resources(&had, missing_pages) {
            enqueue_session_resource_effect(session, resource, [UiInvalidation::MapOverlay]);
        }
        return vec![live_feed_unavailable_status_record(
            "obstacles",
            "Obstacle live feed is waiting for visible HAD pages".to_string(),
        )];
    }

    let mut loaded_any = false;
    for tile in tiles {
        let cache_key = crate::tile_key("obstacle", tile.z, tile.x, tile.y);
        if session.obstacle_tile_cache.contains_key(&cache_key) {
            continue;
        }
        let Some(key) = nav_kv_key_for_query(&NavKvQuery::ObstacleTile {
            z: tile.z,
            x: tile.x,
            y: tile.y,
        }) else {
            continue;
        };
        match store.get_bytes(&key) {
            Ok(NavKvLookup::Hit(bytes)) => match serde_json::from_slice::<PointTilePayload>(&bytes)
            {
                Ok(payload) => {
                    session.obstacle_tile_cache.insert(cache_key, payload);
                    loaded_any = true;
                }
                Err(err) => {
                    return vec![live_feed_unavailable_status_record(
                        "obstacles",
                        format!("Obstacle live feed unavailable: failed to parse tile: {err}"),
                    )];
                }
            },
            Ok(NavKvLookup::MissingKey) => {}
            Ok(NavKvLookup::MissingPages(pages)) => {
                for resource in live_obstacle_had_page_resources(&had, pages) {
                    enqueue_session_resource_effect(
                        session,
                        resource,
                        [UiInvalidation::MapOverlay],
                    );
                }
            }
            Err(message) => {
                return vec![live_feed_unavailable_status_record(
                    "obstacles",
                    format!("Obstacle live feed unavailable: failed to read tile: {message}"),
                )];
            }
        }
    }
    if loaded_any {
        clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
    }
    Vec::new()
}

pub fn get_map_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    get_map_overlay_in_session_at_epoch_ms(handle, viewport, width_px, height_px, 0)
}

pub fn get_map_overlay_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    get_map_overlay_in_session_with_point_display_scale_at_epoch_ms(
        handle, viewport, width_px, height_px, 1.0, epoch_ms,
    )
}

pub fn get_map_overlay_in_session_with_point_display_scale(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> AppResult<HadOperationOutcome> {
    get_map_overlay_in_session_with_point_display_scale_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        point_display_scale,
        0,
    )
}

pub fn get_map_overlay_in_session_with_point_display_scale_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let total_started_at = crate::core_clock_ms();
    let lock_started_at = crate::core_clock_ms();
    let mut sessions = lock_sessions();
    let lock_ms = elapsed_ms(lock_started_at);
    let session = session_mut(&mut sessions, handle)?;
    let advance_started_at = crate::core_clock_ms();
    advance_session_wall_clock(session, epoch_ms);
    let advance_ms = elapsed_ms(advance_started_at);
    let freshness_ms = 0;
    let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, point_display_scale);
    if !session.map_layer_state.vectors.visible
        && !session.map_layer_state.metars.visible
        && !session.map_layer_state.offline_regions.visible
    {
        return Ok(HadOperationOutcome::complete(
            serde_json::to_value(empty_map_overlay_query()).map_err(internal_json_error)?,
        ));
    }
    let mut supplemental_status_records = Vec::new();
    let vector_inputs_started_at = crate::core_clock_ms();
    if session.map_layer_state.vectors.visible {
        supplemental_status_records.extend(ensure_vector_inputs_loaded_for_map_overlay(
            session, &metrics,
        ));
    }
    let vector_inputs_ms = elapsed_ms(vector_inputs_started_at);
    let display_vectors = session.map_layer_state.vectors.visible && session.vector_manifest_loaded;
    let obstacle_inputs_started_at = crate::core_clock_ms();
    if display_vectors {
        supplemental_status_records.extend(ensure_live_obstacle_inputs_loaded(session, &metrics));
    }
    let obstacle_inputs_ms = elapsed_ms(obstacle_inputs_started_at);
    let metar_importance_started_at = crate::core_clock_ms();
    if metar_importance_required_for_viewport(session, &viewport) {
        if let Some(record) = try_ensure_metar_station_importance_loaded(session) {
            supplemental_status_records.push(record);
        }
    }
    let metar_importance_ms = elapsed_ms(metar_importance_started_at);
    let offline_started_at = crate::core_clock_ms();
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
            Err(HadReadError::NeedPages(pages)) => {
                queue_nav_kv_pages_for_map_overlay(session, pages);
                Vec::new()
            }
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
    } else {
        Vec::new()
    };
    let offline_ms = elapsed_ms(offline_started_at);
    let overlay_started_at = crate::core_clock_ms();
    let mut overlay = query_map_overlay_for_surface(
        &metrics,
        &session.map_overlay_config,
        display_vectors,
        session.map_layer_state.metars.visible,
        &offline_region_records,
        ownship_overlay_context(session).as_ref(),
        &session.vector_tile_cache,
        &session.obstacle_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        &session.airspace_feature_cache,
        session.tfr_payload.as_ref(),
    );
    let overlay_ms = elapsed_ms(overlay_started_at);
    let flight_plan_started_at = crate::core_clock_ms();
    if display_vectors {
        overlay.flight_plan_features =
            match flight_plan_overlay_features(session, &viewport, width_px, height_px) {
                Ok(features) => features,
                Err(HadReadError::NeedPages(pages)) => {
                    queue_nav_kv_pages_for_map_overlay(session, pages);
                    Vec::new()
                }
                Err(err) => return had_read_error_to_overlay_outcome(err),
            };
    }
    let flight_plan_ms = elapsed_ms(flight_plan_started_at);
    let supplemental_started_at = crate::core_clock_ms();
    overlay
        .data_status_records
        .extend(supplemental_status_records);
    let supplemental_ms = elapsed_ms(supplemental_started_at);
    let resources_started_at = crate::core_clock_ms();
    let resources = weather_overlay_resources(session, &overlay);
    let resources_ms = elapsed_ms(resources_started_at);
    if !resources.is_empty() {
        return Ok(HadOperationOutcome::NeedResources { resources });
    }
    let status_ms = 0;
    let to_value_started_at = crate::core_clock_ms();
    let overlay_value = serde_json::to_value(overlay).map_err(internal_json_error)?;
    let to_value_ms = elapsed_ms(to_value_started_at);
    let total_ms = elapsed_ms(total_started_at);
    crate::core_debug_log(
        "map.overlay.session",
        &serde_json::json!({
            "total_ms": total_ms,
            "lock_ms": lock_ms,
            "advance_ms": advance_ms,
            "freshness_ms": freshness_ms,
            "vector_inputs_ms": vector_inputs_ms,
            "obstacle_inputs_ms": obstacle_inputs_ms,
            "metar_importance_ms": metar_importance_ms,
            "offline_ms": offline_ms,
            "overlay_ms": overlay_ms,
            "flight_plan_ms": flight_plan_ms,
            "supplemental_ms": supplemental_ms,
            "resources_ms": resources_ms,
            "status_ms": status_ms,
            "to_value_ms": to_value_ms,
            "invalidations": Vec::<UiInvalidation>::new(),
        }),
    );
    Ok(HadOperationOutcome::complete(overlay_value))
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
    get_map_selection_in_session_at_epoch_ms(handle, viewport, width_px, height_px, click, 0)
}

pub fn get_map_selection_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    click: LatLon,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    get_map_selection_in_session_with_point_display_scale_at_epoch_ms(
        handle, viewport, width_px, height_px, click, 1.0, epoch_ms,
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
    get_map_selection_in_session_with_point_display_scale_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        click,
        point_display_scale,
        0,
    )
}

pub fn get_map_selection_in_session_with_point_display_scale_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    click: LatLon,
    point_display_scale: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
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
    get_terrain_overlay_in_session_at_epoch_ms(handle, viewport, width_px, height_px, 0)
}

pub fn get_terrain_overlay_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    let freshness_invalidations = Vec::new();
    if !session.map_layer_state.terrain_warning.visible {
        let result = TerrainOverlayQueryResult {
            status: crate::TerrainOverlayStatus::Hidden,
            tile_requests: Vec::new(),
        };
        return complete_terrain_overlay_outcome_with_invalidations(
            session,
            result,
            freshness_invalidations,
        );
    }
    let kinematics = session.app_state.ownship.resolved.kinematics.as_ref();
    let has_position = kinematics.is_some_and(|kinematics| {
        kinematics.position.lat.is_finite() && kinematics.position.lon.is_finite()
    });
    let has_altitude = ownship_terrain_altitude_ft(session).is_some();
    let mut query =
        crate::query_terrain_overlay(&viewport, width_px, height_px, has_position, has_altitude);
    match resolve_terrain_overlay_source_resources(session, &mut query) {
        TerrainSourceResolution::NeedResources(resources) => {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
        TerrainSourceResolution::Unavailable { reason } => {
            return complete_terrain_overlay_outcome_with_invalidations(
                session,
                TerrainOverlayQueryResult {
                    status: crate::TerrainOverlayStatus::Unavailable { reason },
                    tile_requests: Vec::new(),
                },
                freshness_invalidations,
            );
        }
        TerrainSourceResolution::Resolved => {}
    }
    complete_terrain_overlay_outcome_with_invalidations(session, query, freshness_invalidations)
}

enum TerrainSourceResolution {
    Resolved,
    NeedResources(Vec<CoreResourceRequest>),
    Unavailable { reason: String },
}

fn resolve_terrain_overlay_source_resources(
    session: &UiSession,
    query: &mut TerrainOverlayQueryResult,
) -> TerrainSourceResolution {
    let mut metadata_resources = Vec::new();
    for request in &mut query.tile_requests {
        if request.source_tiles.is_empty() {
            request.source_tiles = terrain_source_tiles(request);
        }
        for source_tile in &mut request.source_tiles {
            let key = terrain_source_tile_cache_key(&source_tile.product_id, &source_tile.path);
            let target_resource_id = format!("terrain/source/{key}");
            let requested = match session.publication_resolver.package_resource_requests(
                &target_resource_id,
                &source_tile.product_id,
                &source_tile.path,
                false,
            ) {
                Ok(requested) => requested,
                Err(message) => {
                    return TerrainSourceResolution::Unavailable {
                        reason: format!("{target_resource_id}: {message}"),
                    };
                }
            };
            let mut resolved_source = false;
            let mut requested_metadata = false;
            for resource in requested {
                if resource.id == target_resource_id {
                    if let CoreResourceSource::Unavailable { message } = &resource.source {
                        return TerrainSourceResolution::Unavailable {
                            reason: format!("{}: {message}", resource.id),
                        };
                    }
                    source_tile.resource = Some(resource);
                    resolved_source = true;
                } else {
                    requested_metadata = true;
                    metadata_resources.push(resource);
                }
            }
            if !resolved_source && !requested_metadata {
                return TerrainSourceResolution::Unavailable {
                    reason: format!("{target_resource_id}: package resolver returned no source"),
                };
            }
        }
    }
    if !metadata_resources.is_empty() {
        TerrainSourceResolution::NeedResources(dedupe_resource_requests(metadata_resources))
    } else {
        TerrainSourceResolution::Resolved
    }
}

pub fn get_nexrad_overlay_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    get_nexrad_overlay_in_session_at_epoch_ms(handle, viewport, width_px, height_px, 0)
}

pub fn get_nexrad_overlay_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    let freshness_invalidations = Vec::new();
    if !session.map_layer_state.nexrad.visible {
        return complete_nexrad_overlay_outcome_with_invalidations(
            session,
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Hidden,
                tiles: Vec::new(),
                stats: NexradOverlayStats::default(),
            },
            freshness_invalidations,
        );
    }
    let manifest = if let Some(installed) = &session.nexrad_installed {
        installed.manifest.clone()
    } else {
        if let HadOperationOutcome::NeedResources { resources } = session.live_feeds.sync_outcome()
        {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
        let Some(manifest) = session.live_feeds.product_state_manifest("nexrad").cloned() else {
            return complete_nexrad_overlay_outcome_with_invalidations(
                session,
                NexradOverlayQueryResult {
                    status: NexradOverlayStatus::Unavailable {
                        reason: "NEXRAD product is missing from the live feed index".to_string(),
                    },
                    tiles: Vec::new(),
                    stats: NexradOverlayStats::default(),
                },
                freshness_invalidations,
            );
        };
        manifest
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
    complete_nexrad_overlay_outcome_with_invalidations(session, query, freshness_invalidations)
}

pub fn nexrad_tile_bytes_in_session(handle: u32, src: &str) -> AppResult<Vec<u8>> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let installed = session.nexrad_installed.as_ref().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: "no installed NEXRAD package is available in this session".to_string(),
    })?;
    let member_path = nexrad_installed_member_path(src).ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!(
            "NEXRAD tile URL {src} is not inside installed package {}",
            installed.version
        ),
    })?;
    installed
        .members
        .get(&member_path)
        .cloned()
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("installed NEXRAD package missing {member_path}"),
        })
}

fn nexrad_installed_member_path(src: &str) -> Option<String> {
    let src = src.trim_start_matches('/');
    let (_, rest) = src.split_once("/tiles/")?;
    // The installed package is rooted at the state directory, while web URLs include
    // live-feeds/states/nexrad/<state-id>/.
    Some(format!("tiles/{rest}"))
}

#[derive(Debug, Deserialize)]
struct NexradSourceGridManifest {
    state_id: String,
    observed_at_utc: Option<DateTime<Utc>>,
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

fn terrain_source_tiles(
    request: &crate::TerrainOverlayTileRequest,
) -> Vec<crate::TerrainOverlaySourceTile> {
    if request.source_tiles.is_empty() {
        vec![crate::TerrainOverlaySourceTile {
            product_id: request.product_id.clone(),
            path: request.path.clone(),
            resource: None,
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

fn commit_session_flight_plan_with_invalidations_outcome(
    session: &mut UiSession,
    plan: FlightPlan,
) -> AppResult<HadOperationOutcome> {
    let started = crate::CoreDebugTimer::start();
    let mut candidate = session.clone();
    replace_session_flight_plan(&mut candidate, plan)?;
    match sync_guidance_geometry_for_session(&mut candidate, &started) {
        Ok(()) => match try_snapshot_for_session(&candidate) {
            Ok(_) => {
                *session = candidate;
                Ok(HadOperationOutcome::complete_with_invalidations(
                    serde_json::Value::Null,
                    vec![
                        UiInvalidation::SessionSnapshot,
                        UiInvalidation::FlightPlanRoute,
                        UiInvalidation::MapOverlay,
                    ],
                ))
            }
            Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            }),
            Err(HadReadError::Fatal(message)) => Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message,
            }),
        },
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
    let total_started_at = crate::core_clock_ms();
    let app_ui_started_at = crate::core_clock_ms();
    let app_ui_state = project_session_app_ui_state(session)?;
    let app_ui_ms = elapsed_ms(app_ui_started_at);
    let debug_started_at = crate::core_clock_ms();
    let debug_state = debug_state_for_app_state(&session.debug_state, &session.app_state);
    let debug_ms = elapsed_ms(debug_started_at);
    let app_state_started_at = crate::core_clock_ms();
    let app_state = state::project_ui_snapshot_app_state(&session.app_state);
    let app_state_ms = elapsed_ms(app_state_started_at);
    let playback_started_at = crate::core_clock_ms();
    let playback_ui_state = session.playback.ui_state();
    let playback_ms = elapsed_ms(playback_started_at);
    let map_follow_started_at = crate::core_clock_ms();
    let map_follow_ui_state = session
        .map_follow
        .ui_state(&session.app_state.ownship.render);
    let map_follow_target_viewport = session
        .map_follow
        .target_viewport(&session.app_state.ownship.render);
    let map_follow_ms = elapsed_ms(map_follow_started_at);
    let clone_started_at = crate::core_clock_ms();
    let chart_page_state = session.chart_page_state.clone();
    let map_layer_state = session.map_layer_state.clone();
    let data_status_state = session.data_status_state.clone();
    let data_status_page_state = project_data_status_page_state(session);
    let clone_ms = elapsed_ms(clone_started_at);
    let raster_started_at = crate::core_clock_ms();
    let raster_map = session
        .raster_map_catalog
        .as_ref()
        .and_then(crate::raster_map_ui_state);
    let raster_ms = elapsed_ms(raster_started_at);
    let total_ms = elapsed_ms(total_started_at);
    crate::core_debug_log(
        "session.snapshot.core",
        &serde_json::json!({
            "total_ms": total_ms,
            "app_ui_ms": app_ui_ms,
            "debug_ms": debug_ms,
            "app_state_ms": app_state_ms,
            "playback_ms": playback_ms,
            "map_follow_ms": map_follow_ms,
            "clone_ms": clone_ms,
            "raster_ms": raster_ms,
            "status_boxes": data_status_state.boxes.len(),
            "status_page_rows": data_status_page_state.rows.len(),
            "map_families": raster_map.as_ref().map(|state| state.family_options.len()).unwrap_or(0),
        }),
    );
    Ok(UiSessionSnapshot {
        app_state,
        app_ui_state,
        playback_ui_state,
        map_follow_ui_state,
        map_follow_target_viewport,
        chart_page_state,
        map_layer_state,
        data_status_state,
        data_status_page_state,
        debug_state,
        raster_map,
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
    UiDataStatusState {
        boxes: Vec::new(),
        launcher_count: None,
        launcher_severity: UiStatusSeverity::Info,
    }
}

fn default_data_status_page_state() -> UiDataStatusPageState {
    UiDataStatusPageState {
        title: "Data status".to_string(),
        summary: "Status will appear after core session data loads.".to_string(),
        rows: Vec::new(),
    }
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
        PointVectorRecord, ProcedureLegProvenance, ProcedureSegmentRole, ResolvedLeg,
        ResolvedLegSource, RouteComponent, SequencingMode, Situation, SituationPosition,
        SituationSample,
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

    fn utc(value: &str) -> DateTime<Utc> {
        parse_utc_instant(value).expect("UTC timestamp")
    }

    fn create_current_test_session() -> UiSessionInitResult {
        create_ui_session_at_epoch_ms(
            FlightPlan::default(),
            &[],
            None,
            None,
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("create session")
    }

    fn data_status_box<'a>(
        snapshot: &'a UiSessionSnapshot,
        id: &str,
    ) -> &'a crate::data_status::UiDataStatusBox {
        snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == id)
            .unwrap_or_else(|| panic!("missing status box {id}"))
    }

    fn has_data_status_box(snapshot: &UiSessionSnapshot, id: &str) -> bool {
        snapshot
            .data_status_state
            .boxes
            .iter()
            .any(|box_| box_.id == id)
    }

    fn nav_db_open_result_for_test(
        package_id: &str,
        expiration_date: Option<&str>,
    ) -> NavDbOpenResult {
        NavDbOpenResult {
            selected_package_id: package_id.to_string(),
            selected_filename: format!("{package_id}.zip"),
            selected_contract_id: None,
            selected_cycle: None,
            selected_cycle_version: None,
            selected_effective_date: None,
            selected_expiration_date: expiration_date.map(str::to_string),
            selected_warning_text: None,
            statuses: Vec::new(),
        }
    }

    fn attach_nav_db_package_records_for_test(handle: u32, packages: Vec<serde_json::Value>) {
        let entries = packages
            .iter()
            .map(|package| {
                let package_id = package["id"].as_str().expect("package id");
                let key = nav_kv_key_for_query(&NavKvQuery::PackageById {
                    package_id: package_id.to_string(),
                })
                .expect("package key");
                let bytes = serde_json::to_vec(package).expect("package json");
                (key, bytes)
            })
            .collect::<Vec<_>>();
        let entry_refs = entries
            .iter()
            .map(|(key, bytes)| (key.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>();
        let store = crate::navkv::nav_kv_store_for_test(&entry_refs, 2048);
        attach_nav_kv_store_to_session(handle, 1, &store).expect("attach nav kv store");
    }

    fn ingest_bundle_packages_for_test(handle: u32, packages: Vec<serde_json::Value>) {
        let bundle = serde_json::json!({ "packages": packages });
        let bytes = serde_json::to_vec(&bundle).expect("bundle json");
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle).expect("session");
        session
            .publication_resolver
            .ingest_resource("publication/bundle/test.json", &bytes)
            .expect("ingest bundle");
    }

    fn package_record_json(
        id: &str,
        family_id: &str,
        effective_date: Option<&str>,
        expiration_date: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "family_id": family_id,
            "contract_id": product_contracts::contract_id_for_family(family_id).unwrap_or("test"),
            "filename": format!("{id}.zip"),
            "relative_path": format!("{id}.zip"),
            "cycle": "2605",
            "cycle_version": "01",
            "effective_date": effective_date,
            "expiration_date": expiration_date
        })
    }

    fn assert_no_session_snapshot_invalidation(invalidations: &[UiInvalidation]) {
        assert!(
            !invalidations.contains(&UiInvalidation::SessionSnapshot),
            "viewport overlay queries must not drive session snapshot invalidations"
        );
    }

    fn raster_map_option(
        id: &str,
        label: &str,
        effective_date: Option<&str>,
        expiration_date: Option<&str>,
    ) -> crate::RasterMapViewOption {
        crate::RasterMapViewOption {
            id: id.to_string(),
            label: label.to_string(),
            region_id: "nw".to_string(),
            map_view: crate::RasterMapView {
                chart_family: id.split(':').next().unwrap_or("sec").to_string(),
                chart_name: label.to_string(),
                chart_index: 0,
                tile_root: "tiles".to_string(),
                tile_url_root: "/package/tiles".to_string(),
                tile_path_template: "{z}/{x}/{y}.webp".to_string(),
                tile_size: 512,
                min_zoom: 0.0,
                max_zoom: 12.0,
                max_source_zoom: 12,
                max_display_zoom: 12.0,
                storage_kind: "sectional_package".to_string(),
                package_name: Some("chart_pkg".to_string()),
                package_relative_path: Some("chart_pkg.zip".to_string()),
                package_effective_date: effective_date.map(str::to_string),
                package_expiration_date: expiration_date.map(str::to_string),
                full_coverage_zoom: None,
                wide_angle: None,
                initial_viewport: crate::RasterInitialViewport {
                    lat: 47.0,
                    lon: -122.0,
                    zoom: 7.0,
                },
                levels: Vec::new(),
            },
        }
    }

    fn raster_catalog_with_displayed_maps(
        selected_map: crate::RasterMapViewOption,
        displayed_maps: Vec<crate::RasterMapViewOption>,
    ) -> RasterMapCatalog {
        RasterMapCatalog {
            selected_map_id: selected_map.id.clone(),
            selected_map: Some(selected_map.clone()),
            available_maps: displayed_maps.clone(),
            displayed_maps,
            geometry: crate::RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        }
    }

    fn expired_raster_catalog(expiration_date: &str) -> RasterMapCatalog {
        let option = raster_map_option("sec:nw", "NW Charts", None, Some(expiration_date));
        raster_catalog_with_displayed_maps(option.clone(), vec![option])
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
            generated_at_utc: None,
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
    fn prepared_live_feed_metars_install_postcard_tile_index() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let state = serde_json::json!({
            "schema_version": 3,
            "version_label": "v1",
            "metar_count": 1,
            "metars_by_station": {
                "KAAA": {
                    "raw_text": "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000",
                    "observed_at_utc": "2026-05-03T00:00:00.000Z",
                    "station_id": "KAAA",
                    "flight_category": "MVFR",
                    "clouds": { "symbol": "SCT" },
                    "longitude": 0.0,
                    "latitude": 0.0
                }
            },
            "pireps": []
        });
        let state_bytes = serde_json::to_vec(&state).expect("state bytes");
        let resource_id = "live_feeds/state/metars/v1";
        let (_raw_state, prepared_bytes) =
            crate::prepare_metar_live_feed_state_resource(resource_id, &state_bytes)
                .expect("prepared metars");
        let prepared_envelope = crate::decode_prepared_metar_live_feed(&prepared_bytes)
            .expect("decode prepared metars");

        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.map_overlay_config = map_overlay_config_from_vector_manifest_json(
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
            session.important_metar_station_ids = Some(HashSet::from(["KAAA".to_string()]));
            session
                .live_feeds
                .ingest_resource(
                    "live_feeds/current",
                    format!(
                        r#"{{
                            "products": {{
                                "metars": {{
                                    "current": "v1",
                                    "version_manifest_url": "versions/metars/v1.json",
                                    "state_url": "states/metars/v1.json",
                                    "state_sha256": "{}"
                                }}
                            }}
                        }}"#,
                        prepared_envelope.state_sha256
                    )
                    .as_bytes(),
                )
                .expect("current manifest");
            session
                .live_feeds
                .ingest_prepared_metar_live_feed(resource_id, &prepared_envelope)
                .expect("prepared live-feed state");
            install_prepared_metar_live_feed(session, prepared_envelope)
                .expect("install prepared metars");
            assert!(session.prepared_metar_feed.is_some());
            assert!(!session.metar_tile_cache.is_empty());
        }

        let outcome = get_map_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 8.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            240.0,
            240.0,
        )
        .expect("overlay");
        let crate::HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("overlay unexpectedly needed resources");
        };
        let overlay: MapOverlayQueryResult =
            serde_json::from_value(result).expect("overlay result");
        assert_eq!(overlay.visible_metars.len(), 1);
        assert_eq!(overlay.visible_metars[0].station_id, "KAAA");
        assert_eq!(overlay.visible_metars[0].flight_category, "mvfr");
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
            nav_db_artifact: None,
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
            current_artifacts_checked_epoch_ms: None,
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_feed: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
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
            generated_at_utc: None,
            metar_count: Some(1),
            metars_by_station,
            pireps: Vec::new(),
        });
        session.important_metar_station_ids = Some(HashSet::from(["KAAA".to_string()]));
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
            generated_at_utc: None,
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
    fn metar_station_importance_comes_from_dense_nav_db_record() {
        let store = crate::navkv::nav_kv_store_for_test(
            &[(
                "weather/metar-important-stations",
                br#"{"schema_version":1,"station_ids":["kaaa","KBBB",""]}"#,
            )],
            1024,
        );
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
            nav_kv_store_id: Some(1),
            nav_kv_store: Some(store),
            nav_db_artifact: None,
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
            current_artifacts_checked_epoch_ms: None,
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_feed: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
        };

        ensure_metar_station_importance_loaded(&mut session).expect("important station ids");

        assert_eq!(
            session.important_metar_station_ids,
            Some(HashSet::from(["KAAA".to_string(), "KBBB".to_string()]))
        );
    }

    #[test]
    fn unresolved_metar_station_importance_does_not_block_vector_overlay() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let store = crate::navkv::nav_kv_store_without_pages_for_test(
            &[(
                "weather/metar-important-stations",
                br#"{"schema_version":1,"station_ids":["KAAA"]}"#,
            )],
            256,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.map_overlay_config = map_overlay_config_from_vector_manifest_json(
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
            .expect("overlay config");
            session.vector_manifest_loaded = true;
            session.map_layer_state.vectors.visible = true;
            session.map_layer_state.metars.visible = true;
            session.vector_tile_cache.insert(
                crate::aggregate_vector_tile_cache_key(9, 256, 256),
                VectorAggregateTilePayload {
                    schema_version: 1,
                    z: 9,
                    x: 256,
                    y: 256,
                    airports: vec![PointVectorRecord {
                        id: "airport:KAAA".to_string(),
                        kind: "airport".to_string(),
                        lat: 0.0,
                        lon: 0.0,
                        label: "KAAA".to_string(),
                        style_class: "public".to_string(),
                        towered: Some(true),
                        fuel_available: None,
                        public_use: Some(true),
                        private_use: None,
                        has_paved_runway: None,
                        heliport: None,
                        has_water_runway: None,
                        longest_runway_length_ft: None,
                        longest_runway_heading_true_deg: None,
                        elevation_msl_ft: None,
                        obstacle: None,
                    }],
                    fixes: Vec::new(),
                    navaids: Vec::new(),
                    airspace_refs: Vec::new(),
                    airspace_labels: Vec::new(),
                },
            );
            for x in 255..=256 {
                for y in 255..=256 {
                    session
                        .vector_tile_cache
                        .entry(crate::aggregate_vector_tile_cache_key(9, x, y))
                        .or_insert_with(|| empty_vector_aggregate_tile(9, x, y));
                }
            }
            session.vector_tile_cache.insert(
                crate::aggregate_vector_tile_cache_key(0, 0, 0),
                empty_vector_aggregate_tile(0, 0, 0),
            );
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "v1".to_string(),
                generated_at_utc: None,
                metar_count: Some(1),
                metars_by_station: HashMap::from([(
                    "KAAA".to_string(),
                    crate::MetarRecord {
                        raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                        observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                        station_id: "KAAA".to_string(),
                        flight_category: Some("VFR".to_string()),
                        clouds: None,
                        longitude: 0.0,
                        latitude: 0.0,
                    },
                )]),
                pireps: Vec::new(),
            });
            rebuild_metar_tile_cache(session);
        }

        let outcome = get_map_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1.0,
            1.0,
        )
        .expect("overlay outcome");

        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("unresolved METAR importance must not block vectors: {outcome:?}");
        };
        let overlay: MapOverlayQueryResult =
            serde_json::from_value(result).expect("decode overlay result");
        assert_eq!(overlay.visible_features.len(), 1);
        assert_eq!(overlay.visible_features[0].label, "KAAA");
        assert_eq!(overlay.visible_metars.len(), 1);
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            assert!(
                session.important_metar_station_ids.is_none(),
                "high-zoom METAR rendering must not request the low-zoom importance table"
            );
            assert!(session.metar_station_importance_status.is_none());
        }
    }

    #[test]
    fn missing_vector_pages_are_background_effects_for_map_overlay() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let (store, pages) = crate::navkv::nav_kv_store_without_pages_and_pages_for_test(
            &[("vector/manifest", minimal_vector_manifest_json().as_bytes())],
            1024,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let outcome = get_map_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            240.0,
            240.0,
        )
        .expect("overlay outcome");

        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("missing vector pages should not block map overlay: {outcome:?}");
        };
        assert_no_session_snapshot_invalidation(&invalidations);
        let overlay: MapOverlayQueryResult =
            serde_json::from_value(result).expect("decode overlay result");
        assert!(overlay.visible_features.is_empty());
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            assert!(
                !session
                    .data_status_records
                    .contains_key(VECTOR_INPUTS_STATUS_ID),
                "viewport-local vector loading must not dirty session status"
            );
        }

        let effects = drain_session_resource_effects(init.handle).expect("drain effects");
        assert!(
            !effects.is_empty(),
            "missing vector pages should be queued as background effects"
        );
        for effect in effects {
            assert_eq!(
                effect.after_success_invalidations,
                vec![UiInvalidation::MapOverlay]
            );
            let page_index = crate::nav_kv_page_index_from_resource_id(&effect.resource.id)
                .expect("nav kv page resource id");
            insert_nav_kv_page_for_attached_sessions(1, page_index, &pages[page_index as usize]);
        }

        let retry_outcome = get_map_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            240.0,
            240.0,
        )
        .expect("retry overlay outcome");
        let HadOperationOutcome::Complete { invalidations, .. } = retry_outcome else {
            panic!("loaded vector pages should complete map overlay: {retry_outcome:?}");
        };
        assert_no_session_snapshot_invalidation(&invalidations);
    }

    #[test]
    fn low_zoom_metars_disappear_while_station_importance_is_unresolved() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let (store, pages) = crate::navkv::nav_kv_store_without_pages_and_pages_for_test(
            &[(
                "weather/metar-important-stations",
                br#"{"schema_version":1,"station_ids":["KAAA"]}"#,
            )],
            256,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.map_overlay_config = map_overlay_config_from_vector_manifest_json(
                r#"{
                    "point_layers": {
                        "airport": { "available_zooms": [5] },
                        "fix": { "available_zooms": [5] },
                        "nav": { "available_zooms": [5] },
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
            .expect("overlay config");
            session.vector_manifest_loaded = true;
            session.map_layer_state.vectors.visible = false;
            session.map_layer_state.metars.visible = true;
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "v1".to_string(),
                generated_at_utc: None,
                metar_count: Some(1),
                metars_by_station: HashMap::from([(
                    "KAAA".to_string(),
                    crate::MetarRecord {
                        raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                        observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                        station_id: "KAAA".to_string(),
                        flight_category: Some("VFR".to_string()),
                        clouds: None,
                        longitude: 0.0,
                        latitude: 0.0,
                    },
                )]),
                pireps: Vec::new(),
            });
            rebuild_metar_tile_cache(session);
        }

        let outcome = get_map_overlay_in_session(
            init.handle,
            MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 5.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            240.0,
            240.0,
        )
        .expect("overlay outcome");

        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("unresolved METAR importance must not block low-zoom overlay: {outcome:?}");
        };
        assert_no_session_snapshot_invalidation(&invalidations);
        let overlay: MapOverlayQueryResult =
            serde_json::from_value(result).expect("decode overlay result");
        assert!(overlay.visible_metars.is_empty());
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            assert!(session.important_metar_station_ids.is_none());
            assert!(
                !session
                    .data_status_records
                    .contains_key(METAR_STATION_IMPORTANCE_STATUS_ID),
                "viewport-local METAR importance loading must not dirty session status"
            );
        }

        let mut resolved_overlay = None;
        for _ in 0..=pages.len() {
            let effects = drain_session_resource_effects(init.handle).expect("drain effects");
            assert!(
                !effects.is_empty(),
                "low-zoom METAR importance should enqueue nav-kv page effects until it loads"
            );
            for effect in effects {
                assert_eq!(
                    effect.after_success_invalidations,
                    vec![UiInvalidation::MapOverlay]
                );
                let page_index = crate::nav_kv_page_index_from_resource_id(&effect.resource.id)
                    .expect("nav kv page resource id");
                insert_nav_kv_page_for_attached_sessions(
                    1,
                    page_index,
                    &pages[page_index as usize],
                );
            }

            let retry_outcome = get_map_overlay_in_session(
                init.handle,
                MapViewport {
                    center: LatLon { lat: 0.0, lon: 0.0 },
                    zoom: 5.0,
                    rotation_deg: 0.0,
                    pitch_deg: 0.0,
                },
                240.0,
                240.0,
            )
            .expect("retry overlay outcome");
            let HadOperationOutcome::Complete {
                result,
                invalidations,
            } = retry_outcome
            else {
                panic!(
                    "METAR importance page effects must not block low-zoom overlay: {retry_outcome:?}"
                );
            };
            assert_no_session_snapshot_invalidation(&invalidations);
            let overlay: MapOverlayQueryResult =
                serde_json::from_value(result).expect("decode retry overlay result");
            if !overlay.visible_metars.is_empty() {
                resolved_overlay = Some(overlay);
                break;
            }
        }

        let overlay = resolved_overlay.expect("METAR importance should load after queued pages");
        assert_eq!(overlay.visible_metars.len(), 1);
        assert_eq!(overlay.visible_metars[0].station_id, "KAAA");
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            assert_eq!(
                session.important_metar_station_ids,
                Some(HashSet::from(["KAAA".to_string()]))
            );
            assert!(!session
                .data_status_records
                .contains_key(METAR_STATION_IMPORTANCE_STATUS_ID));
        }
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
            nav_db_artifact: None,
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
            current_artifacts_checked_epoch_ms: None,
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_feed: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            taf_payload: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
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
    fn live_obstacle_had_pages_fault_into_map_overlay() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let obstacle_manifest_base = serde_json::json!({
            "schema_version": 1,
            "product_id": "obstacles",
            "version_label": "v1",
            "encoding": format!("had-nav-kv-v{}", had_nav_kv::VERSION),
            "root": "root",
            "page_path_template": "page_{page:04}",
            "point_layers": {
                "obstacle": {
                    "min_zoom": 8,
                    "max_zoom": 8,
                    "available_zooms": [8],
                    "zoom_levels": [{
                        "zoom": 8,
                        "filtered": false,
                        "min_agl_ft": 0
                    }]
                }
            }
        });
        let obstacle_layer =
            obstacle_layer_config_from_live_manifest_value(obstacle_manifest_base.clone())
                .expect("obstacle layer config");
        let obstacle_tile =
            visible_obstacle_tile_window(&obstacle_layer, &viewport, 240.0, 240.0, None, 1.0)
                .into_iter()
                .next()
                .expect("visible obstacle tile");
        let tile_payload = PointTilePayload {
            schema_version: 1,
            layer: "obstacle".to_string(),
            z: obstacle_tile.z,
            x: obstacle_tile.x,
            y: obstacle_tile.y,
            records: vec![PointVectorRecord {
                id: "obstacle:test".to_string(),
                kind: "obstacle".to_string(),
                lat: 0.0,
                lon: 0.0,
                label: "".to_string(),
                style_class: "obstacle".to_string(),
                towered: None,
                fuel_available: None,
                public_use: None,
                private_use: None,
                has_paved_runway: None,
                heliport: None,
                has_water_runway: None,
                longest_runway_length_ft: None,
                longest_runway_heading_true_deg: None,
                elevation_msl_ft: None,
                obstacle: Some(crate::map_overlay::ObstaclePointSemantics {
                    height_agl_ft: 300.0,
                    elevation_msl_ft: 500.0,
                    top_msl_ft: 800.0,
                    is_tall: false,
                }),
            }],
        };
        let obstacle_key = nav_kv_key_for_query(&NavKvQuery::ObstacleTile {
            z: obstacle_tile.z,
            x: obstacle_tile.x,
            y: obstacle_tile.y,
        })
        .expect("obstacle tile key");
        let pairs = vec![had_nav_kv::NavKvPair {
            key: obstacle_key,
            value: serde_json::to_vec(&tile_payload).expect("tile json"),
        }];
        let built =
            had_nav_kv::build_nav_kv_sorted(pairs.clone(), 1024).expect("build obstacle HAD");
        let state_sha256 = had_nav_kv::nav_kv_canonical_sha256_from_pairs(&pairs);
        let mut obstacle_manifest = obstacle_manifest_base;
        obstacle_manifest["page_count"] = serde_json::json!(built.pages.len());
        obstacle_manifest["state_sha256"] = serde_json::json!(state_sha256);

        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            install_vector_manifest_config(session, minimal_vector_manifest_json())
                .expect("vector manifest");
            session.map_layer_state.vectors.visible = true;
        }
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "products": {{
                        "obstacles": {{
                            "current": "v1",
                            "version_manifest_url": "versions/obstacles/v1.json",
                            "state_url": "states/obstacles/v1/manifest.json",
                            "state_sha256": "{state_sha256}"
                        }}
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("current manifest");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/version/obstacles/v1",
            format!(
                r#"{{
                    "product": "obstacles",
                    "version": "v1",
                    "state": {{
                        "kind": "nav_kv",
                        "url": "states/obstacles/v1/manifest.json",
                        "state_sha256": "{state_sha256}"
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("version manifest");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/state/obstacles/v1",
            &serde_json::to_vec(&obstacle_manifest).expect("manifest json"),
        )
        .expect("state manifest");

        let metrics = MapSurfaceMetrics::new(viewport, 240.0, 240.0, 1.0);
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, &metrics);
            assert_eq!(statuses.len(), 1);
        }
        let effects = drain_session_resource_effects(init.handle).expect("root effects");
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].resource.id, "live_obstacle_had/v1/root");
        assert_eq!(
            effects[0].resource.source,
            crate::CoreResourceSource::PublicUrl {
                url: "/live-feeds/states/obstacles/v1/root".to_string(),
            }
        );
        ingest_resource_in_session(init.handle, "live_obstacle_had/v1/root", &built.root_bytes)
            .expect("ingest root");

        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, &metrics);
            assert_eq!(statuses.len(), 1);
        }
        let page_effects = drain_session_resource_effects(init.handle).expect("page effects");
        assert!(!page_effects.is_empty());
        for effect in &page_effects {
            let page_text = effect
                .resource
                .id
                .strip_prefix("live_obstacle_had/v1/page/")
                .expect("page resource id");
            let page_index = page_text.parse::<usize>().expect("page index");
            ingest_resource_in_session(init.handle, &effect.resource.id, &built.pages[page_index])
                .expect("ingest page");
        }

        let (config, obstacle_cache) = {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, &metrics);
            assert!(statuses.is_empty());
            (
                session.map_overlay_config.clone(),
                session.obstacle_tile_cache.clone(),
            )
        };
        let overlay = query_map_overlay_for_surface(
            &metrics,
            &config,
            true,
            false,
            &[],
            None,
            &HashMap::new(),
            &obstacle_cache,
            &HashMap::new(),
            None,
            &HashMap::new(),
            None,
        );

        assert!(overlay
            .visible_features
            .iter()
            .any(|feature| feature.id == "obstacle:test"));
    }

    #[test]
    fn installed_live_obstacle_had_opens_without_page_faults() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let obstacle_manifest_base = serde_json::json!({
            "schema_version": 1,
            "product_id": "obstacles",
            "version_label": "installed-v1",
            "encoding": format!("had-nav-kv-v{}", had_nav_kv::VERSION),
            "root": "root",
            "page_path_template": "page_{page:04}",
            "point_layers": {
                "obstacle": {
                    "min_zoom": 8,
                    "max_zoom": 8,
                    "available_zooms": [8],
                    "zoom_levels": [{
                        "zoom": 8,
                        "filtered": false,
                        "min_agl_ft": 0
                    }]
                }
            }
        });
        let obstacle_layer =
            obstacle_layer_config_from_live_manifest_value(obstacle_manifest_base.clone())
                .expect("obstacle layer config");
        let obstacle_tile =
            visible_obstacle_tile_window(&obstacle_layer, &viewport, 240.0, 240.0, None, 1.0)
                .into_iter()
                .next()
                .expect("visible obstacle tile");
        let tile_payload = PointTilePayload {
            schema_version: 1,
            layer: "obstacle".to_string(),
            z: obstacle_tile.z,
            x: obstacle_tile.x,
            y: obstacle_tile.y,
            records: vec![PointVectorRecord {
                id: "obstacle:installed".to_string(),
                kind: "obstacle".to_string(),
                lat: 0.0,
                lon: 0.0,
                label: "".to_string(),
                style_class: "obstacle".to_string(),
                towered: None,
                fuel_available: None,
                public_use: None,
                private_use: None,
                has_paved_runway: None,
                heliport: None,
                has_water_runway: None,
                longest_runway_length_ft: None,
                longest_runway_heading_true_deg: None,
                elevation_msl_ft: None,
                obstacle: Some(crate::map_overlay::ObstaclePointSemantics {
                    height_agl_ft: 300.0,
                    elevation_msl_ft: 500.0,
                    top_msl_ft: 800.0,
                    is_tall: false,
                }),
            }],
        };
        let obstacle_key = nav_kv_key_for_query(&NavKvQuery::ObstacleTile {
            z: obstacle_tile.z,
            x: obstacle_tile.x,
            y: obstacle_tile.y,
        })
        .expect("obstacle tile key");
        let pairs = vec![had_nav_kv::NavKvPair {
            key: obstacle_key,
            value: serde_json::to_vec(&tile_payload).expect("tile json"),
        }];
        let built =
            had_nav_kv::build_nav_kv_sorted(pairs.clone(), 1024).expect("build obstacle HAD");
        let state_sha256 = had_nav_kv::nav_kv_canonical_sha256_from_pairs(&pairs);
        let mut obstacle_manifest = obstacle_manifest_base;
        obstacle_manifest["page_count"] = serde_json::json!(built.pages.len());
        obstacle_manifest["page_size"] = serde_json::json!(built.page_size);
        obstacle_manifest["state_sha256"] = serde_json::json!(state_sha256);

        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            install_vector_manifest_config(session, minimal_vector_manifest_json())
                .expect("vector manifest");
            session.map_layer_state.vectors.visible = true;
        }
        install_live_feed_installed_state_in_session(
            init.handle,
            &crate::LiveFeedInstalledState {
                product: "obstacles".to_string(),
                version: "installed-v1".to_string(),
                state_sha256: state_sha256.clone(),
                payload: crate::LiveFeedInstalledPayload::NavKv {
                    manifest: serde_json::to_vec(&obstacle_manifest).expect("manifest json"),
                    root: built.root_bytes,
                    pages: built.pages,
                },
            },
        )
        .expect("install live obstacle");

        let metrics = MapSurfaceMetrics::new(viewport, 240.0, 240.0, 1.0);
        let (statuses, effects, config, obstacle_cache) = {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, &metrics);
            (
                statuses,
                std::mem::take(&mut session.pending_resource_effects),
                session.map_overlay_config.clone(),
                session.obstacle_tile_cache.clone(),
            )
        };
        assert!(statuses.is_empty());
        assert!(effects.is_empty());
        let overlay = query_map_overlay_for_surface(
            &metrics,
            &config,
            true,
            false,
            &[],
            None,
            &HashMap::new(),
            &obstacle_cache,
            &HashMap::new(),
            None,
            &HashMap::new(),
            None,
        );

        assert!(overlay
            .visible_features
            .iter()
            .any(|feature| feature.id == "obstacle:installed"));
    }

    #[test]
    fn failed_live_feed_current_records_nexrad_caution_when_nexrad_layer_visible() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
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
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
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
    fn installed_nexrad_package_drives_overlay_and_tile_lookup() {
        let manifest = serde_json::json!({
            "state_id": "nexrad-installed-v1",
            "observed_at_utc": "2026-05-21T12:00:00Z",
            "source_grid": {
                "geo_transform": [-123.0, 0.01, 0.0, 48.0, 0.0, -0.01],
            },
            "levels": [{
                "res": 0,
                "width": 256,
                "height": 256,
                "tile_cols": 1,
                "tile_rows": 1,
            }],
            "tile_size": 256,
            "tile_path_template": "tiles/res{res}/{x}/{y}.png",
        });
        let state_sha256 = canonical_json_sha256_value(&manifest).expect("manifest hash");
        let package = nexrad_test_zip(
            &serde_json::to_vec(&manifest).expect("manifest json"),
            &[("tiles/res0/0/0.png", b"tile-png".as_slice())],
        );
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        install_live_feed_installed_state_in_session(
            init.handle,
            &crate::LiveFeedInstalledState {
                product: "nexrad".to_string(),
                version: "nexrad-installed-v1".to_string(),
                state_sha256,
                payload: crate::LiveFeedInstalledPayload::Opaque { bytes: package },
            },
        )
        .expect("install nexrad");

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
            512.0,
            512.0,
        )
        .expect("query nexrad");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected installed nexrad query to complete without page faults");
        };
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("nexrad result");
        assert!(matches!(
            query.status,
            NexradOverlayStatus::Ready { count } if count > 0
        ));
        assert_eq!(query.stats.res, Some(0));
        assert_eq!(
            query.stats.observed_at_utc,
            Some(utc("2026-05-21T12:00:00Z"))
        );
        let bytes =
            nexrad_tile_bytes_in_session(init.handle, &query.tiles[0].src).expect("tile bytes");
        assert_eq!(bytes, b"tile-png");
    }

    #[test]
    fn visible_nexrad_without_product_state_records_caution() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
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

    fn nexrad_test_zip(manifest: &[u8], members: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;

        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        writer
            .start_file("manifest.json", options)
            .expect("start manifest");
        writer.write_all(manifest).expect("write manifest");
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("start member");
            writer.write_all(bytes).expect("write member");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn visible_metars_without_product_state_records_caution() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        set_map_layer_visibility_in_session(init.handle, "vectors", false).expect("hide vectors");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.map_overlay_config = map_overlay_config_from_vector_manifest_json(
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
        }

        let outcome = get_map_overlay_in_session(
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
        .expect("query map overlay");
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete map overlay query");
        };
        assert_no_session_snapshot_invalidation(&invalidations);

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let metars = snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == LIVE_FEED_METARS_STATUS_ID)
            .expect("metars caution");
        assert_eq!(metars.label, "METARS");
        assert_eq!(metars.value.as_deref(), Some("UNAVAIL"));
        assert!(metars.drives_caution);
        assert!(metars.detail.contains("no current METAR product is loaded"));
    }

    #[test]
    fn visible_vectors_without_tfr_product_state_records_caution() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let store = crate::navkv::nav_kv_store_for_test(
            &[("vector/manifest", minimal_vector_manifest_json().as_bytes())],
            1024,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let outcome = get_map_overlay_in_session(
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
        .expect("query map overlay");
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete map overlay query");
        };
        assert_no_session_snapshot_invalidation(&invalidations);

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let tfrs = snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == LIVE_FEED_TFRS_STATUS_ID)
            .expect("tfrs caution");
        assert_eq!(tfrs.label, "TFRS");
        assert_eq!(tfrs.value.as_deref(), Some("UNAVAIL"));
        assert!(tfrs.drives_caution);
        assert!(tfrs.detail.contains("no current TFR product is loaded"));
    }

    #[test]
    fn loaded_metar_feed_older_than_policy_records_warning() {
        let init = create_current_test_session();
        set_map_layer_visibility_in_session(init.handle, "metars", true).expect("show metars");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "old-metars".to_string(),
                generated_at_utc: Some(utc("2020-01-01T00:00:00Z")),
                metar_count: Some(0),
                metars_by_station: HashMap::new(),
                pireps: Vec::new(),
            });
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let metars = data_status_box(&snapshot, LIVE_FEED_METARS_STATUS_ID);
        assert_eq!(metars.label, "METARS");
        assert_eq!(metars.value.as_deref(), Some("OLD"));
        assert_eq!(metars.severity, UiStatusSeverity::Warning);
        assert!(metars.detail.contains("METARS data is"));
    }

    #[test]
    fn metar_fetch_failure_does_not_override_loaded_payload_status() {
        let init = create_current_test_session();
        set_map_layer_visibility_in_session(init.handle, "metars", true).expect("show metars");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "loaded-metars".to_string(),
                generated_at_utc: Some(utc("2026-05-20T11:55:00Z")),
                metar_count: Some(0),
                metars_by_station: HashMap::new(),
                pireps: Vec::new(),
            });
        }

        let snapshot = report_session_resource_failure_in_session(
            init.handle,
            "live_feeds/state/metars/newer-metars",
            "diagnostic fetch failure",
        )
        .expect("report failure");

        assert!(
            !has_data_status_box(&snapshot, LIVE_FEED_METARS_STATUS_ID),
            "loaded fresh METAR data should not be replaced by an unavailable warning"
        );
        let data_status = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:metars")
            .expect("METAR data-status row");
        assert_eq!(data_status.value, "OK");
        assert_eq!(data_status.severity, UiStatusSeverity::Ok);
    }

    #[test]
    fn viewport_query_advances_session_clock_for_freshness_checks() {
        let init = create_ui_session_at_epoch_ms(
            FlightPlan::default(),
            &[],
            None,
            None,
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("create session");
        set_map_layer_visibility_in_session(init.handle, "metars", true).expect("show metars");
        set_map_layer_visibility_in_session(init.handle, "vectors", false).expect("hide vectors");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "fresh-then-old-metars".to_string(),
                generated_at_utc: Some(utc("2026-05-20T11:45:00Z")),
                metar_count: Some(0),
                metars_by_station: HashMap::new(),
                pireps: Vec::new(),
            });
        }

        let fresh_snapshot = get_session_snapshot(init.handle).expect("fresh snapshot");
        assert!(!fresh_snapshot
            .data_status_state
            .boxes
            .iter()
            .any(|box_| box_.id == LIVE_FEED_METARS_STATUS_ID));

        let outcome = get_map_overlay_in_session_at_epoch_ms(
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
            800.0,
            600.0,
            utc("2026-05-20T12:20:01Z").timestamp_millis(),
        )
        .expect("map overlay");
        match outcome {
            HadOperationOutcome::Complete { invalidations, .. } => {
                assert_no_session_snapshot_invalidation(&invalidations);
            }
            HadOperationOutcome::NeedResources { .. } => panic!("unexpected resource request"),
        }

        let stale_snapshot = get_session_snapshot(init.handle).expect("stale snapshot");
        let metars = data_status_box(&stale_snapshot, LIVE_FEED_METARS_STATUS_ID);
        assert_eq!(metars.value.as_deref(), Some("OLD"));
        assert_eq!(metars.severity, UiStatusSeverity::Warning);
    }

    #[test]
    fn loaded_tfr_feed_older_than_policy_records_warning() {
        let init = create_current_test_session();
        set_map_layer_visibility_in_session(init.handle, "vectors", true).expect("show vectors");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.tfr_payload = Some(TfrProductPayload {
                schema_version: 1,
                version_label: "old-tfrs".to_string(),
                generated_at_utc: Some(utc("2020-01-01T00:00:00Z")),
                notam_count: 0,
                area_group_count: 0,
                areas: Vec::new(),
            });
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let tfrs = data_status_box(&snapshot, LIVE_FEED_TFRS_STATUS_ID);
        assert_eq!(tfrs.label, "TFRS");
        assert_eq!(tfrs.value.as_deref(), Some("OLD"));
        assert_eq!(tfrs.severity, UiStatusSeverity::Warning);
        assert!(tfrs.detail.contains("TFRS data is"));
    }

    #[test]
    fn loaded_obstacle_feed_older_than_policy_records_warning() {
        let init = create_current_test_session();
        set_map_layer_visibility_in_session(init.handle, "vectors", true).expect("show vectors");
        let pairs = vec![had_nav_kv::NavKvPair {
            key: "obstacle/8/0/0".to_string(),
            value: b"{}".to_vec(),
        }];
        let built =
            had_nav_kv::build_nav_kv_sorted(pairs.clone(), 1024).expect("build empty obstacle HAD");
        let state_sha256 = had_nav_kv::nav_kv_canonical_sha256_from_pairs(&pairs);
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "products": {{
                        "obstacles": {{
                            "current": "v1",
                            "version_manifest_url": "versions/obstacles/v1.json",
                            "state_url": "states/obstacles/v1/manifest.json",
                            "state_sha256": "{state_sha256}"
                        }}
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("ingest current");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/version/obstacles/v1",
            format!(
                r#"{{
                    "product": "obstacles",
                    "version": "v1",
                    "state": {{
                        "kind": "nav_kv",
                        "url": "states/obstacles/v1/manifest.json",
                        "state_sha256": "{state_sha256}"
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("ingest version");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/state/obstacles/v1",
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "product_id": "obstacles",
                "version_label": "v1",
                "generated_at_utc": "2020-01-01T00:00:00Z",
                "encoding": format!("had-nav-kv-v{}", had_nav_kv::VERSION),
                "root": "root",
                "page_path_template": "page_{page:04}",
                "page_count": built.pages.len(),
                "state_sha256": state_sha256,
                "point_layers": {
                    "obstacle": {
                        "min_zoom": 8,
                        "max_zoom": 8,
                        "available_zooms": [8],
                        "zoom_levels": [{
                            "zoom": 8,
                            "filtered": false,
                            "min_agl_ft": 0
                        }]
                    }
                }
            }))
            .expect("obstacle manifest json")
            .as_slice(),
        )
        .expect("ingest obstacle state");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let obstacles = data_status_box(&snapshot, LIVE_FEED_OBSTACLES_STATUS_ID);
        assert_eq!(obstacles.label, "OBSTACLES");
        assert_eq!(obstacles.value.as_deref(), Some("OLD"));
        assert_eq!(obstacles.severity, UiStatusSeverity::Warning);
        assert!(obstacles.detail.contains("OBSTACLES data is"));
    }

    #[test]
    fn loaded_nexrad_feed_older_than_policy_records_warning() {
        let init = create_current_test_session();
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            let invalidations = sync_nexrad_status_record(
                session,
                &NexradOverlayQueryResult {
                    status: NexradOverlayStatus::Ready { count: 0 },
                    tiles: Vec::new(),
                    stats: NexradOverlayStats {
                        observed_at_utc: Some(utc("2020-01-01T00:00:00Z")),
                        ..NexradOverlayStats::default()
                    },
                },
            );
            assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let nexrad = data_status_box(&snapshot, LIVE_FEED_NEXRAD_STATUS_ID);
        assert_eq!(nexrad.label, "NEXRAD");
        assert_eq!(nexrad.value.as_deref(), Some("OLD"));
        assert_eq!(nexrad.severity, UiStatusSeverity::Warning);
        assert!(nexrad.detail.contains("NEXRAD data is"));
    }

    #[test]
    fn expired_displayed_chart_records_warning() {
        let init = create_current_test_session();
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.raster_map_catalog = Some(expired_raster_catalog("2020-01-01"));
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let chart = data_status_box(&snapshot, CYCLE_DISPLAYED_CHART_STATUS_ID);
        assert_eq!(chart.label, "CHART");
        assert_eq!(chart.value.as_deref(), Some("EXPIRED"));
        assert_eq!(chart.severity, UiStatusSeverity::Warning);
        assert_eq!(chart.detail, "Sectional charts expired.");
    }

    #[test]
    fn expired_background_chart_records_warning_when_selected_chart_is_current() {
        let init = create_current_test_session();
        let sectional = raster_map_option("sec:nw", "NW Sectional", None, Some("2020-01-01"));
        let tac = raster_map_option("tac:nw", "NW TAC", Some("2026-05-14"), Some("2026-07-09"));
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.raster_map_catalog = Some(raster_catalog_with_displayed_maps(
                tac.clone(),
                vec![sectional, tac],
            ));
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let chart = data_status_box(&snapshot, CYCLE_DISPLAYED_CHART_STATUS_ID);
        assert_eq!(chart.value.as_deref(), Some("EXPIRED"));
        assert_eq!(chart.detail, "Sectional charts expired.");
    }

    #[test]
    fn not_yet_effective_displayed_chart_records_warning() {
        let init = create_current_test_session();
        let option = raster_map_option(
            "sec:nw",
            "NW Charts",
            Some("2026-06-11"),
            Some("2026-07-09"),
        );
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.raster_map_catalog = Some(raster_catalog_with_displayed_maps(
                option.clone(),
                vec![option],
            ));
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let chart = data_status_box(&snapshot, CYCLE_DISPLAYED_CHART_STATUS_ID);
        assert_eq!(chart.label, "CHART");
        assert_eq!(chart.value.as_deref(), Some("EARLY"));
        assert_eq!(chart.severity, UiStatusSeverity::Warning);
        assert_eq!(chart.detail, "Sectional charts not valid yet.");
    }

    #[test]
    fn mixed_displayed_chart_validity_records_family_summary() {
        let init = create_current_test_session();
        let sectional = raster_map_option(
            "sec:nw",
            "NW Sectional",
            Some("2026-06-11"),
            Some("2026-07-09"),
        );
        let tac = raster_map_option("tac:nw", "NW TAC", None, Some("2020-01-01"));
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.raster_map_catalog = Some(raster_catalog_with_displayed_maps(
                tac.clone(),
                vec![sectional, tac],
            ));
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let chart = data_status_box(&snapshot, CYCLE_DISPLAYED_CHART_STATUS_ID);
        assert_eq!(chart.value.as_deref(), Some("INVALID"));
        assert_eq!(chart.detail, "TAC, Sectional charts not valid.");
    }

    #[test]
    fn data_status_page_reports_valid_chart_package_lifetime() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("SEC_NW_2605", "sec", Some("2026-05-14"), Some("2026-05-26")),
                package_record_json(
                    "TAC_NW_2605",
                    "tac",
                    Some("2026-05-14"),
                    Some("2026-05-24T12:00:00Z"),
                ),
            ],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:charts")
            .expect("charts row");
        assert_eq!(row.value, "OK");
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert!(row.detail.contains("valid until 2026-05-24 12:00 UTC"));
        assert!(row
            .facts
            .iter()
            .any(|fact| fact.label == "Products" && fact.value == "Sectional, TAC"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Expires"
                && fact.value == "2026-05-24 12:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-05-24T12:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
    }

    #[test]
    fn data_status_page_reports_invalid_chart_package_summary() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![package_record_json(
                "SEC_NW_2605",
                "sec",
                Some("2020-01-01"),
                Some("2020-02-01"),
            )],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:charts")
            .expect("charts row");
        assert_eq!(row.value, "EXPIRED");
        assert_eq!(row.severity, UiStatusSeverity::Warning);
        assert_eq!(row.detail, "Sectional charts expired.");
    }

    #[test]
    fn data_status_page_reports_docs_and_static_package_rows() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("TPP_2605", "tpp", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("CSUP_2605", "csup", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("TERRAIN_2025", "terrain", Some("2025-01-01"), None),
                package_record_json(
                    "SHADED_RELIEF_2025",
                    "shaded-relief",
                    Some("2025-02-01"),
                    None,
                ),
                package_record_json(
                    "WORLD_BASEMAP_2025",
                    "world-basemap",
                    Some("2025-03-01"),
                    None,
                ),
            ],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let docs = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:airport_docs")
            .expect("airport docs row");
        assert_eq!(docs.value, "OK");
        assert!(docs.detail.contains("TPP, CSup airport docs valid"));
        let static_data = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "static:base_data")
            .expect("static data row");
        assert_eq!(static_data.value, "OK");
        assert!(static_data
            .detail
            .contains("source data dates back to 2025-01-01 00:00 UTC"));
    }

    #[test]
    fn data_status_page_reports_docs_from_attached_nav_db_not_future_publication_bundle() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("TPP_2605", "tpp", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("CSUP_2605", "csup", Some("2026-05-14"), Some("2026-07-09")),
            ],
        );
        ingest_bundle_packages_for_test(
            init.handle,
            vec![package_record_json(
                "TPP_2606",
                "tpp",
                Some("2026-06-11"),
                Some("2026-07-09"),
            )],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let docs = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:airport_docs")
            .expect("airport docs row");
        assert_eq!(docs.value, "OK");
        assert!(docs.detail.contains("TPP, CSup airport docs valid"));
        assert!(!docs.detail.contains("not valid yet"));
    }

    #[test]
    fn data_status_page_delivers_absolute_times_for_web_relative_rendering() {
        let checked_at = utc("2026-05-20T12:00:00Z").timestamp_millis();
        let init =
            create_ui_session_at_epoch_ms(FlightPlan::default(), &[], None, None, checked_at)
                .expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session
                .publication_resolver
                .ingest_resource(
                    "publication/current_artifacts",
                    br#"{"schema_version":1,"as_of_utc":"2026-05-20T12:00:00Z","artifact_roots":{"packaged":"published_packaged","unpacked":"published_unpacked"},"bundles":[]}"#,
                )
                .expect("ingest current artifacts");
            session.current_artifacts_checked_epoch_ms = Some(checked_at);
        }

        let snapshot = get_session_snapshot_at_epoch_ms(
            init.handle,
            utc("2026-05-20T12:07:00Z").timestamp_millis(),
        )
        .expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "publication:current_artifacts")
            .expect("package library row");
        assert_eq!(
            row.detail,
            "current_artifacts.json checked at 2026-05-20 12:00 UTC."
        );
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Checked"
                && fact.value == "2026-05-20 12:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-05-20T12:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Ago)
        }));
    }

    #[test]
    fn data_status_page_reports_live_feed_connection_state() {
        let init = create_current_test_session();
        report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Connected,
                message: None,
            },
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("connected");
        let snapshot = report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Message,
                message: None,
            },
            utc("2026-05-20T12:03:00Z").timestamp_millis(),
        )
        .expect("message");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:connection")
            .expect("connection row");
        assert_eq!(row.value, "CONNECTED");
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert!(row
            .detail
            .contains("Last server event was at 2026-05-20 12:03 UTC"));
    }

    #[test]
    fn data_status_page_reports_attached_nav_db_without_self_package_row() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let open_result = nav_db_open_result_for_test("NAV_DB_TEST", Some("2026-06-18T00:00:00Z"));
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "nav_db")
            .expect("nav-db page row");
        assert_eq!(row.value, "OK");
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert!(row
            .detail
            .contains("NAV DB valid until 2026-06-18 00:00 UTC"));
        assert!(!row.detail.contains("contains no nav-db package rows"));
    }

    #[test]
    fn expired_nav_db_package_records_warning() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let open_result = nav_db_open_result_for_test("NAV_DB_TEST", Some("2020-01-01"));
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let nav_db = data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID);
        assert_eq!(nav_db.label, "NAV DB");
        assert_eq!(nav_db.value.as_deref(), Some("EXPIRED"));
        assert_eq!(nav_db.severity, UiStatusSeverity::Warning);
        assert!(nav_db.detail.contains("NAV_DB_TEST expired"));
    }

    #[test]
    fn package_ui_warning_records_warning() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(
            &[(
                "package/by-id/SEC_SUNSET",
                br#"{
                    "id": "SEC_SUNSET",
                    "family_id": "sec",
                    "ui_warning": {
                        "severity": "warning",
                        "label": "SECTIONAL",
                        "value": "SUNSET",
                        "detail": "This sectional package format is being sunsetted."
                    }
                }"#,
            )],
            1024,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:SEC_SUNSET");
        assert_eq!(warning.label, "SECTIONAL");
        assert_eq!(warning.value.as_deref(), Some("SUNSET"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert!(warning.drives_caution);
        assert!(warning.detail.contains("sunsetted"));
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:SEC_SUNSET"
                && row.value == "SUNSET"
                && row.detail.contains("sunsetted")
        }));
    }

    #[test]
    fn package_warning_text_records_warning_and_status_page_row() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![serde_json::json!({
                "id": "ENR_H_SAMPLE",
                "family_id": "enr-h",
                "warning_text": "This is a sample warning!"
            })],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:ENR_H_SAMPLE");
        assert_eq!(warning.label, "IFR-H");
        assert_eq!(warning.value.as_deref(), Some("WARNING"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert!(warning.drives_caution);
        assert_eq!(warning.detail, "This is a sample warning!");
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:ENR_H_SAMPLE"
                && row.label == "IFR-H"
                && row.value == "WARNING"
                && row.detail == "This is a sample warning!"
        }));
    }

    #[test]
    fn publication_bundle_warning_text_records_warning_and_status_page_row() {
        let init = create_current_test_session();
        let mut package =
            package_record_json("ENR_H_BUNDLE_SAMPLE", "enr-h", Some("2026-05-14"), None);
        package["warning_text"] = serde_json::json!("This is a sample warning!");
        ingest_bundle_packages_for_test(init.handle, vec![package]);

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:ENR_H_BUNDLE_SAMPLE");
        assert_eq!(warning.label, "IFR-H");
        assert_eq!(warning.value.as_deref(), Some("WARNING"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert_eq!(warning.detail, "This is a sample warning!");
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:ENR_H_BUNDLE_SAMPLE"
                && row.label == "IFR-H"
                && row.value == "WARNING"
        }));
    }

    #[test]
    fn selected_nav_db_warning_text_records_warning() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut open_result = nav_db_open_result_for_test("NAV_DB_SAMPLE", Some("2026-06-11"));
        open_result.selected_warning_text = Some("This is a sample warning!".to_string());
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:NAV_DB_SAMPLE");
        assert_eq!(warning.label, "NAV DB");
        assert_eq!(warning.value.as_deref(), Some("WARNING"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert_eq!(warning.detail, "This is a sample warning!");
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:NAV_DB_SAMPLE"
                && row.label == "NAV DB"
                && row.value == "WARNING"
        }));
    }

    #[test]
    fn cycle_product_freshness_snapshot_skips_clean_recompute() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let open_result = nav_db_open_result_for_test("NAV_DB_TEST", Some("2020-01-01"));
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            assert!(!session.cycle_product_freshness.dirty);
            assert!(session
                .data_status_records
                .remove(CYCLE_NAV_DB_STATUS_ID)
                .is_some());
            sync_data_status_projection(session);
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        assert!(!has_data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID));
    }

    #[test]
    fn cycle_product_freshness_ignores_clean_expiration_boundary() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let open_result = nav_db_open_result_for_test("NAV_DB_TEST", Some("2026-05-21T00:00:00Z"));
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");
        let snapshot = get_session_snapshot(init.handle).expect("fresh snapshot");
        assert!(!has_data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID));
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            assert!(!session.cycle_product_freshness.dirty);
            session.wall_clock_epoch_ms = utc("2026-05-21T00:00:01Z").timestamp_millis();
        }

        let snapshot = get_session_snapshot(init.handle).expect("expired snapshot");
        assert!(!has_data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID));
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            mark_cycle_product_freshness_dirty(session);
        }

        let snapshot = get_session_snapshot(init.handle).expect("dirty expired snapshot");
        let nav_db = data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID);
        assert_eq!(nav_db.label, "NAV DB");
        assert_eq!(nav_db.value.as_deref(), Some("EXPIRED"));
    }

    #[test]
    fn terrain_overlay_without_altitude_records_caution() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");

        let outcome = get_terrain_overlay_in_session(
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
        .expect("query terrain overlay");
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete terrain overlay query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let terrain = snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id == TERRAIN_STATUS_ID)
            .expect("terrain caution");
        assert_eq!(terrain.label, "TERRAIN");
        assert_eq!(terrain.value.as_deref(), Some("UNAVAIL"));
        assert!(terrain.drives_caution);
        assert!(terrain.detail.contains("ownship position is unavailable"));
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
        let init = create_ui_session(plan.clone(), &[], None, None).expect("create session");

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

    fn assert_session_snapshot_invalidated(outcome: HadOperationOutcome) {
        match outcome {
            HadOperationOutcome::Complete { invalidations, .. } => {
                assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
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
        assert_session_snapshot_invalidated(after_direct_to);
        let after_direct_to = get_session_snapshot(init.handle).expect("direct-to snapshot");
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

        let after_direct_to = activate_direct_to_nav_ref_in_session_outcome(
            init.handle,
            NavRef::Fix("VPDUB".to_string()),
        );
        assert_session_snapshot_invalidated(after_direct_to.expect("direct-to on-plan fix"));
        let after_direct_to = get_session_snapshot(init.handle).expect("direct-to snapshot");
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
        assert_session_snapshot_invalidated(after_activate_leg);
        let after_activate_leg = get_session_snapshot(init.handle).expect("activate-leg snapshot");
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
