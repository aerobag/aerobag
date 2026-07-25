// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Read,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, MutexGuard, OnceLock,
    },
};

use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::CoreResourcePolicy;
use crate::{
    chart_ident_label_for_nav_ref_symbol,
    chart_page::chart_page_airport_ids_from_plan,
    chart_page::ChartAssetRecord,
    data_status::{
        parse_status_action_id, project_data_status_state, DataStatusRecord, UiDataStatusPageFact,
        UiDataStatusPageRow, UiDataStatusPageState, UiDataStatusPageTimeDisplay, UiDataStatusState,
        UiStatusAction, UiStatusActionCommand, UiStatusActionStyle, UiStatusSeverity,
        RELOAD_APPLICATION_ACTION_ID,
    },
    first_guidance_detail_index_for_leg,
    freshness::{
        cycle_product_is_expired, evaluate_age, format_age, parse_utc_instant, FreshnessSeverity,
        FreshnessViolation, DATA_FRESHNESS_POLICIES,
    },
    guidance_detail_id_for_leg_element,
    had_ops::{
        flight_plan_ui_state, insert_waypoint_best_position,
        materialize_airway_presentation_selection, materialize_procedure, nav_kv_page_resources,
        nav_ref_position, nav_symbol_feature, suggest_waypoint_identifiers, CoreResourceRequest,
        CoreResourceSource, HadOperationOutcome, HadReadError, UiInvalidation,
    },
    live_feed_runtime::{
        LiveFeedConnectionEvent, LiveFeedConnectionEventKind, LiveFeedNetworkStatus,
        LiveFeedRuntimeDecision, LiveFeedRuntimeInput, LiveFeedRuntimeState,
    },
    live_feeds::{LiveFeedSseEvent, LiveFeedsState, LIVE_FEEDS_BASE_PATH},
    map_follow::{MapFollowSessionState, MapFollowUiState},
    map_overlay::{
        nearest_available_layer_zoom, obstacle_layer_config_from_live_manifest_value,
        vector_overlay_input_requests, visible_obstacle_tile_window, FlightPlanSelectionPoint,
        MetarTileRecord, PointTileLayerConfig, VectorOverlayInputRequests,
        MAP_SELECTION_NAV_REF_MIN_FOCUS_ZOOM,
    },
    map_overlay_config_from_vector_manifest_json, nav_kv_key_for_query,
    planning::NavElementUiView,
    playback::PlaybackSessionState,
    project_nav_symbol_feature,
    publication::{PublicationResolvedResource, PublicationResolver},
    query_map_overlay_for_surface_at, query_map_selection_for_surface_in_time_zone, state,
    AirportNotamIndex, AirportPlateAvailability, AirspaceFeaturePayload, AirspaceLabelTilePayload,
    AirspaceReferenceTilePayload, AirwayPresentationPlan, AppError, AppErrorKind, AppEvent,
    AppResult, AppState, AppUiState, FlightPlan, FlightPlanDisplayRowKind,
    FlightPlanRowActionExecution, FlightPlanRowActionId, FlightPlanUiState, GuidanceState, LatLon,
    LegDisplayElement, MapOverlayConfig, MapOverlayQueryResult, MapSelectionForNavRefResult,
    MapSelectionQueryResult, MapSelectionSessionAction, MapSurfaceMetrics, MapViewport,
    MetarProductPayload, MetarTilePayload, NavDbArtifactCandidate, NavDbOpenResult, NavKvLookup,
    NavKvPageProbeStats, NavKvQuery, NavKvRoot, NavKvStore, NavRef, OfflinePackagesLibraryCache,
    PlaybackUiState, PointTilePayload, ProcedureDiscontinuity, ProcedureKind, ProcedureLoadCommand,
    RasterMapCatalog, RasterResourceMode, RasterTilePlan, ResolvedLeg, ResolvedLegSource,
    RouteComponentViewKind, SequencingMode, SituationControlInput, SituationControlMenuItem,
    TafProductPayload, TerrainOverlayQueryResult, TfrProductPayload, UiSnapshotAppState,
    VectorAggregateTilePayload, VectorIdentLabelStyle,
};

const WORLD_MERCATOR_MAX_LATITUDE: f64 = 85.051_128_78;
const SETTINGS_PERSISTENCE_VERSION: u32 = 1;
const NO_WARRANTY_DISCLAIMER_HTML: &str = include_str!("../../../../shared/no-warranty.html");
const NO_WARRANTY_DISCLAIMER_AGREEMENT_ID: &str = "no-warranty-v1";
const DISPLAY_DIM_TIMEOUT_ROW_ID: &str = "display_dim_timeout";
const DISPLAY_DIM_TIMEOUT_ACTION_ID: &str = "display_dim_timeout";
const FLIGHT_DATA_VISIBILITY_ROW_ID: &str = "flight_data_visibility";
const FLIGHT_DATA_VISIBILITY_ACTION_ID: &str = "flight_data_visibility";
const DISPLAY_DIM_BRIGHTNESS: f32 = 0.05;

impl DisplayDimTimeout {
    fn id(self) -> &'static str {
        match self {
            Self::TenSeconds => "10s",
            Self::ThirtySeconds => "30s",
            Self::OneMinute => "1m",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::Never => "never",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TenSeconds => "10s",
            Self::ThirtySeconds => "30s",
            Self::OneMinute => "1m",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::Never => "Never",
        }
    }

    fn dim_after_ms(self) -> Option<u64> {
        match self {
            Self::TenSeconds => Some(10_000),
            Self::ThirtySeconds => Some(30_000),
            Self::OneMinute => Some(60_000),
            Self::TwoMinutes => Some(120_000),
            Self::FiveMinutes => Some(300_000),
            Self::Never => None,
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "10s" => Some(Self::TenSeconds),
            "30s" => Some(Self::ThirtySeconds),
            "1m" => Some(Self::OneMinute),
            "2m" => Some(Self::TwoMinutes),
            "5m" => Some(Self::FiveMinutes),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    fn all_stops() -> [Self; 6] {
        [
            Self::TenSeconds,
            Self::ThirtySeconds,
            Self::OneMinute,
            Self::TwoMinutes,
            Self::FiveMinutes,
            Self::Never,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiChartPageState {
    pub ordered_airport_ids: Vec<String>,
    pub recent_airport_ids: Vec<String>,
    #[serde(default)]
    pub plate_target_airport_id: Option<String>,
    pub selected_airport_id: String,
    #[serde(default)]
    pub selected_reference_family_id: Option<String>,
    pub selected_chart_id: String,
    #[serde(default)]
    pub suggested_chart_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiMapLayerToggleState {
    pub visible: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
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
    pub fast_tiles: bool,
    pub offline_simulated_clock_buttons: bool,
    #[serde(default)]
    pub sequencing_finish_lines: bool,
    #[serde(default)]
    pub bad_autopilot: bool,
    #[serde(default)]
    pub gps_capture: bool,
    #[serde(default)]
    pub debug_log_to_developer_server: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlatformDisplayPolicyCapability {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlatformOfflinePackagesCapability {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientBuildInfo {
    pub platform: String,
    pub version: String,
    #[serde(default)]
    pub built_at_utc: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlatformCapabilities {
    #[serde(default)]
    pub display_policy: Option<PlatformDisplayPolicyCapability>,
    #[serde(default)]
    pub offline_packages: Option<PlatformOfflinePackagesCapability>,
    #[serde(default)]
    pub client_build: Option<ClientBuildInfo>,
    #[serde(default)]
    pub local_time_zone: Option<String>,
}

pub trait SettingsStorage: Send + Sync {
    fn read_settings(&self) -> AppResult<Option<Vec<u8>>>;
    fn write_settings(&self, bytes: &[u8]) -> AppResult<()>;
}

pub type SettingsStorageHandle = Arc<dyn SettingsStorage>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDimTimeout {
    #[serde(rename = "10s")]
    TenSeconds,
    #[serde(rename = "30s")]
    ThirtySeconds,
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "2m")]
    TwoMinutes,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "never")]
    Never,
}

impl Default for DisplayDimTimeout {
    fn default() -> Self {
        Self::TwoMinutes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettingsPreferences {
    #[serde(default)]
    pub display_dim_timeout: DisplayDimTimeout,
    #[serde(default)]
    pub disabled_flight_data_cell_ids: BTreeSet<String>,
    #[serde(default)]
    pub accepted_disclaimer_agreement_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettingsSliderStop {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettingsGridItem {
    pub cell: crate::FlightDataCell,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettingsPageRow {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub value_id: String,
    pub stops: Vec<UiSettingsSliderStop>,
    #[serde(default)]
    pub items: Vec<UiSettingsGridItem>,
    pub action_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettingsPageState {
    pub title: String,
    pub summary: String,
    pub rows: Vec<UiSettingsPageRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiHomePageButton {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiHomePageState {
    pub buttons: Vec<UiHomePageButton>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDisplayPolicy {
    pub keep_screen_on: bool,
    pub dim_after_ms: Option<u64>,
    pub dim_brightness: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDisclaimerState {
    pub agreement_id: String,
    pub required: bool,
    pub html: String,
    pub text: String,
    pub accept_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSettingsAction {
    pub action_id: String,
    pub value_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SettingsPersistenceDocument {
    version: u32,
    preferences: SettingsPreferences,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiPlaybackPanelState {
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSessionSnapshot {
    pub session_revision: u64,
    pub nav_data_epoch: u64,
    pub active_nav_db: Option<UiNavDbIdentity>,
    pub next_nav_db_maintenance_epoch_ms: Option<i64>,
    pub app_state: UiSnapshotAppState,
    pub app_ui_state: AppUiState,
    pub playback_ui_state: PlaybackUiState,
    pub playback_panel_state: UiPlaybackPanelState,
    pub map_follow_ui_state: MapFollowUiState,
    pub map_follow_target_viewport: Option<MapViewport>,
    pub chart_page_state: UiChartPageState,
    pub map_layer_state: UiMapLayerState,
    pub data_status_state: UiDataStatusState,
    pub data_status_page_state: UiDataStatusPageState,
    pub settings_page_state: UiSettingsPageState,
    pub home_page_state: UiHomePageState,
    pub display_policy: Option<UiDisplayPolicy>,
    pub disclaimer_state: UiDisclaimerState,
    pub debug_state: UiDebugState,
    pub raster_map: Option<crate::RasterMapUiState>,
    pub next_cycle_product_freshness_check_epoch_ms: Option<i64>,
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
    session_revision: u64,
    nav_data_epoch: u64,
    nav_db_advance_blocked: bool,
    app_state: AppState,
    playback: PlaybackSessionState,
    plan_preview: PlanPreviewState,
    bad_autopilot: BadAutopilotState,
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
    platform_capabilities: PlatformCapabilities,
    settings_preferences: SettingsPreferences,
    settings_storage: Option<SettingsStorageHandle>,
    debug_state: UiDebugState,
    resource_policy: CoreResourcePolicy,
    installed_package_ids: BTreeSet<String>,
    publication_resolver: PublicationResolver,
    cycle_product_freshness: CycleProductFreshnessState,
    live_feeds: LiveFeedsState,
    live_feed_connection: LiveFeedConnectionSessionState,
    raster_map_catalog: Option<RasterMapCatalog>,
    vector_tile_cache: HashMap<String, VectorAggregateTilePayload>,
    metar_tile_cache: HashMap<String, MetarTilePayload>,
    metar_payload: Option<MetarProductPayload>,
    prepared_metar_tiles: Option<Vec<crate::PreparedMetarTile>>,
    important_metar_station_ids: Option<HashSet<String>>,
    metar_station_importance_status: Option<DataStatusRecord>,
    obstacle_had: Option<LiveObstacleHadState>,
    obstacle_tile_cache: HashMap<String, PointTilePayload>,
    taf_payload: Option<TafProductPayload>,
    airport_notam_index: Option<AirportNotamIndex>,
    airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    tfr_payload: Option<TfrProductPayload>,
    nexrad_installed: Option<LiveNexradInstalledState>,
    nexrad_tile_cache: HashMap<String, Vec<u8>>,
    terrain_source_tile_cache: HashMap<String, Vec<u8>>,
    pending_resource_effects: Vec<UiSessionResourceEffect>,
    wall_clock_epoch_ms: i64,
    live_feed_current_refresh: LiveFeedCurrentRefreshState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LiveFeedCurrentRefreshState {
    #[default]
    Idle,
    Requested,
    Ingested,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiNavDbIdentity {
    pub package_id: String,
    pub filename: String,
    pub contract_id: Option<String>,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
}

impl From<&AttachedNavDbArtifact> for UiNavDbIdentity {
    fn from(artifact: &AttachedNavDbArtifact) -> Self {
        Self {
            package_id: artifact.package_id.clone(),
            filename: artifact.filename.clone(),
            contract_id: artifact.contract_id.clone(),
            cycle: artifact.cycle.clone(),
            cycle_version: artifact.cycle_version.clone(),
        }
    }
}

const NAV_DB_ADVANCE_STATUS_ID: &str = "nav_db:advance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavDbAdvanceDisposition {
    Adopted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavDbAdvanceResult {
    pub disposition: NavDbAdvanceDisposition,
    pub snapshot: UiSessionSnapshot,
    pub active_artifact_filename: Option<String>,
    pub retained_artifact_filenames: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavDbMaintenanceAction {
    None,
    AttemptAdvance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavDbMaintenanceResult {
    pub action: NavDbMaintenanceAction,
    pub snapshot: UiSessionSnapshot,
}

const NAV_DB_PUBLICATION_POLL_INTERVAL_MS: i64 = 4 * 60 * 60 * 1000;

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
    runtime: LiveFeedRuntimeState,
    source_url: Option<String>,
    status_url: Option<String>,
    last_state_change_epoch_ms: Option<i64>,
    last_heard_epoch_ms: Option<i64>,
    last_error_epoch_ms: Option<i64>,
    last_error_message: Option<String>,
    last_resource_error_epoch_ms: Option<i64>,
    last_resource_error_message: Option<String>,
    network_status: Option<LiveFeedNetworkStatus>,
}

#[derive(Clone, Default)]
struct CycleProductFreshnessState {
    dirty: bool,
    missing_nav_kv_pages: BTreeSet<u32>,
    next_check_epoch_ms: Option<i64>,
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
    #[serde(default)]
    pub animation: NexradOverlayAnimation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexradOverlayAnimation {
    pub phase: NexradOverlayAnimationPhase,
    pub selected_frame_index: Option<usize>,
    pub frame_count: usize,
    pub age_labels: Vec<String>,
    pub age_summary: String,
    pub next_update_delay_ms: Option<u32>,
    pub next_update_epoch_ms: Option<i64>,
}

impl Default for NexradOverlayAnimation {
    fn default() -> Self {
        Self::idle()
    }
}

impl NexradOverlayAnimation {
    fn idle() -> Self {
        Self {
            phase: NexradOverlayAnimationPhase::Idle,
            selected_frame_index: None,
            frame_count: 0,
            age_labels: Vec::new(),
            age_summary: "---".to_string(),
            next_update_delay_ms: None,
            next_update_epoch_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexradOverlayAnimationPhase {
    Idle,
    Frame,
    Blank,
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
struct BadAutopilotState {
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
const BAD_AUTOPILOT_SOURCE_ID: &str = "__bad_autopilot__";
const CDI_NM_PER_DOT: f64 = 1.0;
const CDI_OFFSCALE_DOTS: f64 = 2.1;
const BAD_AUTOPILOT_NM_PER_SECOND: f64 = 0.36;
const BAD_AUTOPILOT_REPORTED_SPEED_SCALE: f64 = 0.1;
const BAD_AUTOPILOT_MAX_DT_SECONDS: f64 = 1.0;
const BAD_AUTOPILOT_WANDER_NM: f64 = 0.125;
const BAD_AUTOPILOT_OVERRUN_NM: f64 = 0.5;

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
const LIVE_FEED_TAFS_STATUS_ID: &str = "live_feed:tafs_unavailable";
const LIVE_FEED_NEXRAD_STATUS_ID: &str = "live_feed:nexrad_unavailable";
const NEXRAD_ANIMATION_MAX_FRAMES: usize = 7;
const NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS: i64 = 1_000;
const NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS: i64 = 2_500;
const NEXRAD_ANIMATION_BLANK_DWELL_MS: i64 = 500;
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
}

#[derive(Debug, Clone, Deserialize)]
struct NavDbFamilyRecord {
    id: String,
    display_name: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CycleWindow {
    effective: Option<DateTime<Utc>>,
    expiration: Option<DateTime<Utc>>,
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
    collected_utc: Option<DateTime<Utc>>,
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
            collected_utc,
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
        metars_status.collected_utc,
        DATA_FRESHNESS_POLICIES.live_feeds.metars,
    ));

    let tfrs_visible = session.map_layer_state.vectors.visible;
    let tfrs_collected_utc = live_feed_status_timestamp(session, "tfrs").or_else(|| {
        session
            .tfr_payload
            .as_ref()
            .and_then(|payload| payload.generated_at_utc)
    });
    let tfrs_loaded = session.tfr_payload.is_some();
    invalidations.extend(sync_live_feed_product_status_record(
        session,
        tfrs_visible,
        tfrs_loaded,
        "tfrs",
        "TFR live feed unavailable: no current TFR product is loaded",
        tfrs_collected_utc,
        DATA_FRESHNESS_POLICIES.live_feeds.tfrs,
    ));

    let obstacles_visible = session.map_layer_state.vectors.visible;
    let obstacles_collected_utc = live_feed_status_timestamp(session, "obstacles").or_else(|| {
        session
            .live_feeds
            .product_state_manifest("obstacles")
            .and_then(json_generated_at_utc)
    });
    let obstacles_loaded = session.obstacle_had.is_some();
    invalidations.extend(sync_live_feed_product_status_record(
        session,
        obstacles_visible,
        obstacles_loaded,
        "obstacles",
        "Obstacle live feed unavailable: no current obstacle product is loaded",
        obstacles_collected_utc,
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
    collected_utc: Option<DateTime<Utc>>,
    loaded_version: Option<String>,
}

fn metar_live_feed_status_source(session: &UiSession) -> LiveFeedProductStatusSource {
    if let Some(payload) = session.metar_payload.as_ref() {
        return LiveFeedProductStatusSource {
            loaded: true,
            collected_utc: live_feed_status_timestamp(session, "metars")
                .or(payload.generated_at_utc),
            loaded_version: Some(payload.version_label.clone()),
        };
    }
    LiveFeedProductStatusSource {
        loaded: false,
        collected_utc: None,
        loaded_version: session
            .live_feeds
            .product_loaded_version("metars")
            .map(str::to_string),
    }
}

fn taf_live_feed_status_source(session: &UiSession) -> LiveFeedProductStatusSource {
    if let Some(payload) = session.taf_payload.as_ref() {
        return LiveFeedProductStatusSource {
            loaded: true,
            collected_utc: live_feed_status_timestamp(session, "tafs").or(payload.generated_at_utc),
            loaded_version: Some(payload.version_label.clone()),
        };
    }
    LiveFeedProductStatusSource {
        loaded: false,
        collected_utc: None,
        loaded_version: session
            .live_feeds
            .product_loaded_version("tafs")
            .map(str::to_string),
    }
}

fn live_feed_status_timestamp(session: &UiSession, product: &str) -> Option<DateTime<Utc>> {
    session
        .live_feeds
        .product_published_at_utc(product)
        .or_else(|| session.live_feeds.product_collected_at_utc(product))
        .and_then(parse_utc_instant)
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct OwnshipTerrainRefreshKey {
    has_position: bool,
    altitude_bucket_ft: Option<f64>,
}

fn ownship_terrain_refresh_key(session: &UiSession) -> OwnshipTerrainRefreshKey {
    let has_position = session
        .app_state
        .ownship
        .resolved
        .kinematics
        .as_ref()
        .is_some_and(|kinematics| {
            kinematics.position.lat.is_finite() && kinematics.position.lon.is_finite()
        });
    OwnshipTerrainRefreshKey {
        has_position,
        altitude_bucket_ft: crate::terrain_altitude_bucket_ft(ownship_terrain_altitude_ft(session)),
    }
}

fn terrain_overlay_invalidations_for_ownship_change(
    before: OwnshipTerrainRefreshKey,
    after: OwnshipTerrainRefreshKey,
) -> Vec<UiInvalidation> {
    if before == after {
        Vec::new()
    } else {
        vec![UiInvalidation::TerrainOverlay]
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
        "tafs" => DataStatusRecord::new(
            LIVE_FEED_TAFS_STATUS_ID,
            "TAFS",
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

fn live_feed_resource_failure_detail(resource_id: &str, message: &str) -> String {
    if resource_id == "live_feeds/current" {
        return format!("Live feed index unavailable: {message}");
    }
    if let Some(product) = live_feed_product_from_resource_id(resource_id) {
        return format!(
            "{} live feed unavailable: {message}",
            product.to_ascii_uppercase()
        );
    }
    format!("Live feed resource {resource_id} unavailable: {message}")
}

fn record_live_feed_resource_error(session: &mut UiSession, message: String) {
    session.live_feed_connection.last_resource_error_epoch_ms = Some(session.wall_clock_epoch_ms);
    session.live_feed_connection.last_resource_error_message = Some(message);
}

fn clear_live_feed_resource_error(session: &mut UiSession) {
    session.live_feed_connection.last_resource_error_epoch_ms = None;
    session.live_feed_connection.last_resource_error_message = None;
}

fn record_live_feed_fetch_failure(
    session: &mut UiSession,
    resource_id: &str,
    message: &str,
) -> bool {
    if resource_id == "live_feeds/current" {
        let detail = live_feed_resource_failure_detail(resource_id, message);
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
    upsert_data_status_record(
        session,
        live_feed_unavailable_status_record(
            product,
            live_feed_resource_failure_detail(resource_id, message),
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
            nexrad_freshest_frame_observed_at_utc(session),
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

fn nav_db_candidates_for_session(
    session: &UiSession,
) -> Result<Result<Vec<NavDbArtifactCandidate>, Vec<CoreResourceRequest>>, String> {
    let candidates = match session.publication_resolver.nav_db_artifact_candidates()? {
        Ok(candidates) => candidates,
        Err(resources) => return Ok(Err(resources)),
    };
    if session.resource_policy == CoreResourcePolicy::PublicUnpacked {
        return Ok(Ok(candidates));
    }
    Ok(Ok(candidates
        .into_iter()
        .filter(|candidate| {
            session
                .installed_package_ids
                .contains(&candidate.package_id)
        })
        .collect()))
}

fn nav_db_candidate_effective_epoch_ms(candidate: &NavDbArtifactCandidate) -> Option<i64> {
    candidate
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant)
        .map(|instant| instant.timestamp_millis())
}

fn next_nav_db_maintenance_epoch_ms(session: &UiSession) -> Option<i64> {
    if session.nav_db_advance_blocked {
        return None;
    }
    let now_epoch_ms = session.wall_clock_epoch_ms;
    let mut next = if session.resource_policy == CoreResourcePolicy::PublicUnpacked {
        Some(
            session
                .publication_resolver
                .current_artifacts_checked_epoch_ms()
                .map(|checked| checked.saturating_add(NAV_DB_PUBLICATION_POLL_INTERVAL_MS))
                .unwrap_or(now_epoch_ms),
        )
    } else {
        None
    };
    let candidates = match nav_db_candidates_for_session(session) {
        Ok(Ok(candidates)) => candidates,
        Ok(Err(_)) | Err(_) => {
            return if session.resource_policy == CoreResourcePolicy::PublicUnpacked {
                min_optional_epoch_ms(next, Some(now_epoch_ms))
            } else {
                None
            };
        }
    };
    let preferred = crate::had_ops::select_preferred_nav_db_candidate(&candidates, now_epoch_ms);
    if preferred.is_some_and(|candidate| {
        session
            .nav_db_artifact
            .as_ref()
            .is_none_or(|active| active.filename != candidate.filename)
    }) {
        next = min_optional_epoch_ms(next, Some(now_epoch_ms));
    }
    let next_effective = candidates
        .iter()
        .filter_map(nav_db_candidate_effective_epoch_ms)
        .filter(|effective| *effective > now_epoch_ms)
        .min();
    min_optional_epoch_ms(next, next_effective)
}

fn min_epoch_ms(current: Option<i64>, candidate: i64) -> Option<i64> {
    Some(current.map_or(candidate, |current| current.min(candidate)))
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
    _collected_utc: DateTime<Utc>,
    violation: FreshnessViolation,
) -> DataStatusRecord {
    DataStatusRecord::new(
        status_id,
        label,
        Some("OLD".to_string()),
        freshness_status_severity(violation),
        true,
        format!(
            "{product_name} data is {} old.",
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
    collected_utc: Option<DateTime<Utc>>,
    policy: crate::freshness::AgeFreshnessPolicy,
) -> bool {
    if !visible {
        return clear_data_status_record(session, status_id);
    }
    let Some(collected_utc) = collected_utc else {
        return clear_data_status_record(session, status_id);
    };
    let now_utc = session_wall_clock_utc(session);
    if let Some(violation) = evaluate_age(policy, collected_utc, now_utc) {
        upsert_data_status_record(
            session,
            live_feed_stale_status_record(status_id, label, product_name, collected_utc, violation),
        )
    } else {
        clear_data_status_record(session, status_id)
    }
}

#[derive(Default)]
struct CycleProductFreshnessSync {
    changed: bool,
    missing_nav_kv_pages: BTreeSet<u32>,
    next_check_epoch_ms: Option<i64>,
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
        sync.next_check_epoch_ms = min_optional_epoch_ms(
            sync.next_check_epoch_ms,
            collect_chart_validity_violations(
                &mut violations,
                &mut seen,
                family_label,
                family_sort_key,
                option.map_view.package_effective_date.as_deref(),
                option.map_view.package_expiration_date.as_deref(),
                now_utc,
            ),
        );
        if let Some(wide_angle) = option.map_view.wide_angle.as_ref() {
            sync.next_check_epoch_ms = min_optional_epoch_ms(
                sync.next_check_epoch_ms,
                collect_chart_validity_violations(
                    &mut violations,
                    &mut seen,
                    family_label,
                    family_sort_key,
                    wide_angle.package_effective_date.as_deref(),
                    wide_angle.package_expiration_date.as_deref(),
                    now_utc,
                ),
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

fn min_optional_epoch_ms(current: Option<i64>, candidate: Option<i64>) -> Option<i64> {
    match candidate {
        Some(candidate) => min_epoch_ms(current, candidate),
        None => current,
    }
}

fn collect_chart_validity_violations(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    effective_date: Option<&str>,
    expiration_date: Option<&str>,
    now_utc: DateTime<Utc>,
) -> Option<i64> {
    let mut next_check_epoch_ms = None;
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
                next_check_epoch_ms =
                    min_epoch_ms(next_check_epoch_ms, effective_utc.timestamp_millis());
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
            } else {
                next_check_epoch_ms =
                    min_epoch_ms(next_check_epoch_ms, expiration_utc.timestamp_millis());
            }
        }
    }
    next_check_epoch_ms
}

fn push_chart_validity_violation(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    kind: ChartValidityViolationKind,
) {
    let key = (family_sort_key, kind);
    if !seen.insert(key) {
        return;
    }
    violations.push(ChartValidityViolation {
        family_label,
        family_sort_key,
        kind,
    });
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
    let family_list = violations
        .iter()
        .map(|violation| (violation.family_sort_key, violation.family_label))
        .collect::<BTreeSet<_>>()
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
        sync.next_check_epoch_ms = Some(expiration_utc.timestamp_millis());
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

fn mark_cycle_product_freshness_dirty_if_deadline_due(session: &mut UiSession) {
    let Some(next_check_epoch_ms) = session.cycle_product_freshness.next_check_epoch_ms else {
        return;
    };
    if session.wall_clock_epoch_ms >= next_check_epoch_ms {
        mark_cycle_product_freshness_dirty(session);
    }
}

fn sync_cycle_product_freshness_status_records_if_needed(
    session: &mut UiSession,
) -> Vec<UiInvalidation> {
    if !session.cycle_product_freshness.dirty {
        return Vec::new();
    }
    sync_cycle_product_freshness_status_records(session)
}

fn sync_cycle_product_freshness_status_records(session: &mut UiSession) -> Vec<UiInvalidation> {
    let selected = sync_displayed_chart_validity_freshness(session);
    let nav_db = sync_nav_db_expiration_freshness(session);
    session.cycle_product_freshness = CycleProductFreshnessState {
        dirty: false,
        missing_nav_kv_pages: nav_db.missing_nav_kv_pages,
        next_check_epoch_ms: min_optional_epoch_ms(
            selected.next_check_epoch_ms,
            nav_db.next_check_epoch_ms,
        ),
    };
    let changed =
        selected.changed | nav_db.changed | sync_package_ui_warning_status_records(session);
    if changed {
        vec![UiInvalidation::SessionSnapshot]
    } else {
        Vec::new()
    }
}

fn chart_family_status_label(chart_family: &str) -> (&'static str, u8) {
    match chart_family {
        "tac" => ("TAC", 10),
        "flyway" => ("Flyway", 15),
        "sec" => ("Sectional", 20),
        "enr-l" => ("IFR-Low", 30),
        "enr-h" => ("IFR-High", 40),
        "world-basemap" => ("World basemap", 50),
        "shaded-relief" => ("Shaded relief", 60),
        _ => ("Chart", 250),
    }
}

fn structured_package_warning_status_record(
    warning_id: &str,
    label: String,
    value: Option<String>,
    severity: UiStatusSeverity,
    detail: String,
) -> DataStatusRecord {
    DataStatusRecord::new(
        format!("{PACKAGE_UI_WARNING_STATUS_PREFIX}{warning_id}"),
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
    warning_id: &str,
    family_id: &str,
    warning_text: &str,
) -> DataStatusRecord {
    DataStatusRecord::new(
        format!("{PACKAGE_UI_WARNING_STATUS_PREFIX}{warning_id}"),
        package_warning_label(family_id),
        Some("WARNING".to_string()),
        UiStatusSeverity::Warning,
        true,
        warning_text.to_string(),
    )
}

fn nav_db_family_warning_status_record(family: &NavDbFamilyRecord) -> Option<DataStatusRecord> {
    if !family_warning_is_supported(&family.id) {
        return None;
    }
    let warning_id = format!("family:{}", family.id);
    if let Some(warning) = family.ui_warning.as_ref() {
        return Some(structured_package_warning_status_record(
            &warning_id,
            if warning.label.is_empty() {
                family.display_name.clone()
            } else {
                warning.label.clone()
            },
            warning.value.clone(),
            warning.severity,
            warning.detail.clone(),
        ));
    }
    family.warning_text.as_ref().map(|warning_text| {
        warning_text_package_status_record(&warning_id, &family.id, warning_text)
    })
}

fn family_warning_is_supported(family_id: &str) -> bool {
    matches!(
        family_id,
        "sec" | "tac" | "flyway" | "enr-l" | "enr-h" | "tpp" | "csup"
    )
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
        "flyway" => "Flyway".to_string(),
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

fn nav_db_family_records(session: &UiSession) -> Vec<NavDbFamilyRecord> {
    let Some(store) = session.nav_kv_store.as_ref() else {
        return Vec::new();
    };
    let Ok(NavKvLookup::Hit(bytes)) = store.get_bytes("resource/families") else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<NavDbFamilyRecord>>(&bytes).unwrap_or_default()
}

fn package_warning_status_records(session: &UiSession) -> Vec<DataStatusRecord> {
    let mut records = BTreeMap::new();
    for family in nav_db_family_records(session) {
        if let Some(record) = nav_db_family_warning_status_record(&family) {
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

fn project_data_status_page_state(session: &UiSession) -> UiDataStatusPageState {
    let metars_status = metar_live_feed_status_source(session);
    let tafs_status = taf_live_feed_status_source(session);
    let mut rows = vec![
        client_build_status_page_row(session),
        publication_status_page_row(session),
        expected_contract_versions_status_page_row(),
        nav_db_status_page_row(session),
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
        live_feed_connection_status_page_row(session),
        live_feed_product_status_page_row(
            session,
            "tfrs",
            "TFRs",
            session.tfr_payload.is_some(),
            live_feed_status_timestamp(session, "tfrs").or_else(|| {
                session
                    .tfr_payload
                    .as_ref()
                    .and_then(|payload| payload.generated_at_utc)
            }),
            DATA_FRESHNESS_POLICIES.live_feeds.tfrs,
            session
                .live_feeds
                .product_loaded_version("tfrs")
                .map(str::to_string),
        ),
        live_feed_product_status_page_row(
            session,
            "notams",
            "NOTAMs",
            session.airport_notam_index.is_some(),
            live_feed_status_timestamp(session, "notams"),
            DATA_FRESHNESS_POLICIES.live_feeds.notams,
            session
                .live_feeds
                .product_loaded_version("notams")
                .map(str::to_string),
        ),
        live_feed_product_status_page_row(
            session,
            "metars",
            "METARs",
            metars_status.loaded,
            metars_status.collected_utc,
            DATA_FRESHNESS_POLICIES.live_feeds.metars,
            metars_status.loaded_version,
        ),
        live_feed_product_status_page_row(
            session,
            "tafs",
            "TAFs",
            tafs_status.loaded,
            tafs_status.collected_utc,
            DATA_FRESHNESS_POLICIES.live_feeds.tafs,
            tafs_status.loaded_version,
        ),
        nexrad_live_feed_status_page_row(session),
        live_feed_product_status_page_row(
            session,
            "obstacles",
            "Obstacles",
            session.obstacle_had.is_some()
                || session
                    .live_feeds
                    .product_state_manifest("obstacles")
                    .is_some(),
            live_feed_status_timestamp(session, "obstacles").or_else(|| {
                session
                    .live_feeds
                    .product_state_manifest("obstacles")
                    .and_then(json_generated_at_utc)
            }),
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
    ];
    rows.extend(package_warning_status_page_rows(session));
    UiDataStatusPageState {
        title: "Status".to_string(),
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
        return "All tracked systems are usable.".to_string();
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

fn client_build_status_page_row(session: &UiSession) -> UiDataStatusPageRow {
    let Some(build) = session.platform_capabilities.client_build.as_ref() else {
        return status_page_row(
            "client",
            "Client",
            "UNKNOWN",
            UiStatusSeverity::Info,
            "Client build identity was not provided by this platform.",
            Vec::new(),
        );
    };

    let mut facts = vec![status_fact("Platform", build.platform.clone())];
    if let Some(built_at_utc) = build.built_at_utc.as_deref() {
        if let Some(instant) = parse_utc_instant(built_at_utc) {
            facts.push(status_time_fact(
                "Built",
                instant,
                UiDataStatusPageTimeDisplay::Ago,
            ));
        } else {
            facts.push(status_fact("Built", built_at_utc.to_string()));
        }
    }
    if let Some(commit) = build.commit.as_deref().filter(|commit| !commit.is_empty()) {
        facts.push(status_fact("Commit", commit.to_string()));
    }
    if build.dirty {
        facts.push(status_fact("Worktree", "dirty"));
    }

    let detail = if build.dirty {
        format!(
            "Running the {} client build {} from a dirty worktree.",
            build.platform, build.version
        )
    } else {
        format!(
            "Running the {} client build {}.",
            build.platform, build.version
        )
    };

    status_page_row(
        "client",
        "Client",
        build.version.clone(),
        UiStatusSeverity::Ok,
        detail,
        facts,
    )
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

    let mut packages_by_family: BTreeMap<u8, (&'static str, Vec<&NavDbPackageRecord>)> =
        BTreeMap::new();
    for package in &packages {
        let Some((_, family_label, family_sort_key)) = family_spec_for_package(families, package)
        else {
            continue;
        };
        family_set.insert((family_sort_key, family_label));
        packages_by_family
            .entry(family_sort_key)
            .or_insert_with(|| (family_label, Vec::new()))
            .1
            .push(package);
    }

    for (family_sort_key, (family_label, family_packages)) in &packages_by_family {
        let current_packages = family_packages
            .iter()
            .copied()
            .filter(|package| cycle_package_is_currently_valid(package, now_utc))
            .collect::<Vec<_>>();
        if current_packages.is_empty() {
            for package in family_packages {
                collect_chart_validity_violations(
                    &mut violations,
                    &mut seen_violations,
                    family_label,
                    *family_sort_key,
                    package.effective_date.as_deref(),
                    package.expiration_date.as_deref(),
                    now_utc,
                );
            }
            continue;
        }
        let mut family_has_expiration = false;
        for package in current_packages {
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
                family_has_expiration = true;
                earliest_expiration = Some(
                    earliest_expiration
                        .map(|current| current.min(expiration_utc))
                        .unwrap_or(expiration_utc),
                );
            }
        }
        if !family_has_expiration {
            missing_expiration_families.insert((*family_sort_key, *family_label));
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
    let next_cycle_window = next_published_cycle_window_for_families(session, families, now_utc)
        .or_else(|| next_cycle_window_for_package_groups(&packages_by_family, now_utc));

    if let Some(expiration) = earliest_expiration {
        facts.push(status_time_fact(
            "Expires",
            expiration,
            UiDataStatusPageTimeDisplay::Until,
        ));
        push_next_cycle_window_facts(&mut facts, next_cycle_window);
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
    push_next_cycle_window_facts(&mut facts, next_cycle_window);
    status_page_row(
        id,
        label,
        "UNKNOWN",
        UiStatusSeverity::Info,
        format!("{family_list} {noun} validity metadata is not available."),
        facts,
    )
}

fn next_published_cycle_window_for_families(
    session: &UiSession,
    families: &[(&'static str, &'static str, u8)],
    now_utc: DateTime<Utc>,
) -> Option<CycleWindow> {
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    for (family_id, _, _) in families {
        let Some((package, effective)) = session
            .publication_resolver
            .loaded_bundle_packages()
            .filter(|package| {
                package.family_id == *family_id
                    && crate::package_management::package_contract_is_supported(package)
            })
            .filter_map(|package| {
                let effective = package
                    .effective_date
                    .as_deref()
                    .and_then(parse_utc_instant)?;
                (now_utc < effective).then_some((package, effective))
            })
            .min_by(
                |(left_package, left_effective), (right_package, right_effective)| {
                    left_effective
                        .cmp(right_effective)
                        .then_with(|| left_package.id.cmp(&right_package.id))
                },
            )
        else {
            continue;
        };
        latest_effective = Some(
            latest_effective
                .map(|current| current.max(effective))
                .unwrap_or(effective),
        );
        if let Some(expiration) = package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            earliest_expiration = Some(
                earliest_expiration
                    .map(|current| current.min(expiration))
                    .unwrap_or(expiration),
            );
        }
    }
    if latest_effective.is_none() && earliest_expiration.is_none() {
        None
    } else {
        Some(CycleWindow {
            effective: latest_effective,
            expiration: earliest_expiration,
        })
    }
}

fn next_cycle_window_for_package_groups(
    packages_by_family: &BTreeMap<u8, (&'static str, Vec<&NavDbPackageRecord>)>,
    now_utc: DateTime<Utc>,
) -> Option<CycleWindow> {
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    for (_, family_packages) in packages_by_family.values() {
        let Some(package) = family_packages
            .iter()
            .filter_map(|package| {
                let effective = package
                    .effective_date
                    .as_deref()
                    .and_then(parse_utc_instant)?;
                (now_utc < effective).then_some((*package, effective))
            })
            .min_by(|(_, left), (_, right)| left.cmp(right))
        else {
            continue;
        };
        latest_effective = Some(
            latest_effective
                .map(|current| current.max(package.1))
                .unwrap_or(package.1),
        );
        if let Some(expiration) = package
            .0
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            earliest_expiration = Some(
                earliest_expiration
                    .map(|current| current.min(expiration))
                    .unwrap_or(expiration),
            );
        }
    }
    if latest_effective.is_none() && earliest_expiration.is_none() {
        None
    } else {
        Some(CycleWindow {
            effective: latest_effective,
            expiration: earliest_expiration,
        })
    }
}

fn push_next_cycle_window_facts(
    facts: &mut Vec<UiDataStatusPageFact>,
    window: Option<CycleWindow>,
) {
    let Some(window) = window else {
        return;
    };
    if let Some(effective) = window.effective {
        facts.push(status_time_fact(
            "Next effective",
            effective,
            UiDataStatusPageTimeDisplay::Until,
        ));
    }
    if let Some(expiration) = window.expiration {
        facts.push(status_time_fact(
            "Next expires",
            expiration,
            UiDataStatusPageTimeDisplay::Until,
        ));
    }
}

fn cycle_package_is_currently_valid(package: &NavDbPackageRecord, now_utc: DateTime<Utc>) -> bool {
    if package
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant)
        .is_some_and(|effective| now_utc < effective)
    {
        return false;
    }
    if package
        .expiration_date
        .as_deref()
        .and_then(parse_utc_instant)
        .is_some_and(|expiration| cycle_product_is_expired(expiration, now_utc))
    {
        return false;
    }
    true
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
    push_next_cycle_window_facts(&mut facts, next_nav_db_cycle_window(session, now_utc));
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

fn next_nav_db_cycle_window(session: &UiSession, now_utc: DateTime<Utc>) -> Option<CycleWindow> {
    let mut candidates = session
        .publication_resolver
        .loaded_bundle_packages()
        .filter(|package| {
            package.family_id == "nav-db"
                && crate::package_management::package_contract_is_supported(package)
        })
        .filter_map(|package| {
            let effective = package
                .effective_date
                .as_deref()
                .and_then(parse_utc_instant)?;
            (now_utc < effective).then_some((package, effective))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(
        |(left_package, left_effective), (right_package, right_effective)| {
            left_effective
                .cmp(right_effective)
                .then_with(|| left_package.id.cmp(&right_package.id))
        },
    );
    let (package, effective) = candidates.first().copied()?;
    Some(CycleWindow {
        effective: Some(effective),
        expiration: package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant),
    })
}

fn expected_contract_versions_status_page_row() -> UiDataStatusPageRow {
    let facts = product_contracts::PRODUCT_CONTRACTS
        .iter()
        .map(|contract| {
            status_fact(
                package_warning_label(contract.family_id),
                contract.contract_id,
            )
        })
        .collect::<Vec<_>>();
    status_page_row(
        "contracts:expected",
        "Contract versions",
        product_contracts::PRODUCT_CONTRACTS.len().to_string(),
        UiStatusSeverity::Ok,
        "Core will only accept packages that match these product contract ids.",
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
    if let Some(checked_at) = session
        .publication_resolver
        .current_artifacts_checked_epoch_ms()
    {
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
    if let Some(source_url) = connection.source_url.as_deref() {
        facts.push(status_link_fact(
            "Server",
            source_url,
            connection.status_url.as_deref().unwrap_or(source_url),
        ));
    }
    if let Some(last_heard) = connection.last_heard_epoch_ms {
        facts.push(status_time_fact(
            "Last server event",
            utc_from_epoch_ms(last_heard),
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(status) = connection.network_status {
        facts.push(status_fact(
            "Network",
            live_feed_network_status_label(status),
        ));
    }
    let last_error_epoch = connection
        .last_resource_error_epoch_ms
        .or(connection.last_error_epoch_ms);
    let network_issue = live_feed_network_status_issue(connection.network_status);
    let last_error_message = connection
        .last_resource_error_message
        .as_deref()
        .or_else(|| {
            if matches!(connection.mode, LiveFeedConnectionMode::Error) {
                network_issue
            } else {
                None
            }
        })
        .or(connection.last_error_message.as_deref());
    if let Some(last_error) = last_error_epoch {
        facts.push(status_time_fact(
            "Last error",
            utc_from_epoch_ms(last_error),
            UiDataStatusPageTimeDisplay::Ago,
        ));
    }
    if let Some(message) = last_error_message {
        facts.push(status_fact("Error", message.to_string()));
    }
    let resource_error_message = connection.last_resource_error_message.as_ref();
    let (value, severity, detail) = if let Some(message) = resource_error_message {
        (
            "ERROR",
            UiStatusSeverity::Unavailable,
            match connection.mode {
                LiveFeedConnectionMode::Connected => format!(
                    "The live-feed event stream is connected, but live-feed data is unavailable: {message}"
                ),
                LiveFeedConnectionMode::Closed => {
                    format!("The live-feed event stream is closed. Last live-feed error: {message}")
                }
                _ => format!("Live-feed data is unavailable: {message}"),
            },
        )
    } else if let Some(message) = network_issue {
        (
            match connection.network_status {
                Some(LiveFeedNetworkStatus::Metered) => "METERED",
                Some(LiveFeedNetworkStatus::NoActiveNetwork) => "NO NETWORK",
                _ => "NETWORK",
            },
            UiStatusSeverity::Unavailable,
            message.to_string(),
        )
    } else {
        match connection.mode {
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
                    .map(|message| {
                        format!("The live-feed event stream reported an error: {message}.")
                    })
                    .unwrap_or_else(|| "The live-feed event stream reported an error.".to_string()),
            ),
            LiveFeedConnectionMode::Closed => (
                "CLOSED",
                UiStatusSeverity::Unavailable,
                connection
                    .last_error_message
                    .as_ref()
                    .map(|message| {
                        format!(
                            "The live-feed event stream is closed. Last live-feed error: {message}"
                        )
                    })
                    .unwrap_or_else(|| "The live-feed event stream is closed.".to_string()),
            ),
        }
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

fn live_feed_network_status_label(status: LiveFeedNetworkStatus) -> &'static str {
    match status {
        LiveFeedNetworkStatus::Unmetered => "Unmetered",
        LiveFeedNetworkStatus::Metered => "Metered",
        LiveFeedNetworkStatus::NoActiveNetwork => "No active network",
        LiveFeedNetworkStatus::Unknown => "Unknown",
    }
}

fn live_feed_network_status_issue(status: Option<LiveFeedNetworkStatus>) -> Option<&'static str> {
    match status {
        Some(LiveFeedNetworkStatus::Metered) => Some(
            "The active network is metered. Live feeds are allowed, but this network condition can explain live-feed connectivity failures.",
        ),
        Some(LiveFeedNetworkStatus::NoActiveNetwork) => {
            Some("Android reports no active network for live feeds.")
        }
        _ => None,
    }
}

fn live_feed_product_status_page_row(
    session: &UiSession,
    product: &str,
    label: &str,
    loaded: bool,
    collected_utc: Option<DateTime<Utc>>,
    policy: crate::freshness::AgeFreshnessPolicy,
    loaded_version: Option<String>,
) -> UiDataStatusPageRow {
    let now_utc = session_wall_clock_utc(session);
    let mut facts = Vec::new();
    if let Some(version) = loaded_version {
        facts.push(status_fact("Version", version));
    }
    if let Some(collected) = collected_utc {
        facts.push(status_time_fact(
            "Collected At",
            collected,
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
    let Some(collected_utc) = collected_utc else {
        return status_page_row(
            format!("live_feed:{product}"),
            label,
            "CACHED",
            UiStatusSeverity::Info,
            format!("Cached {label} live-feed data is available, but source timestamp is unknown."),
            facts,
        );
    };
    if let Some(violation) = evaluate_age(policy, collected_utc, now_utc) {
        return status_page_row(
            format!("live_feed:{product}"),
            label,
            "OLD",
            freshness_status_severity(violation),
            format!("{label} data is {} old.", format_age(violation.age_ms)),
            facts,
        );
    }
    status_page_row(
        format!("live_feed:{product}"),
        label,
        "OK",
        UiStatusSeverity::Ok,
        format!("{label} is loaded."),
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

fn nexrad_live_feed_status_page_row(session: &UiSession) -> UiDataStatusPageRow {
    let mut row = live_feed_product_status_page_row(
        session,
        "nexrad",
        "NEXRAD",
        nexrad_status_manifest(session).is_some(),
        live_feed_status_timestamp(session, "nexrad")
            .or_else(|| nexrad_status_manifest(session).and_then(json_observed_at_utc)),
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
    );
    row.facts
        .push(status_fact("Frames", nexrad_frame_age_summary(session)));
    row
}

fn nexrad_frame_age_summary(session: &UiSession) -> String {
    if !session.map_layer_state.nexrad.visible {
        return "off".to_string();
    }
    let frames = nexrad_frame_candidates(session);
    let labels = nexrad_frame_age_labels(&frames, session.wall_clock_epoch_ms);
    if labels.is_empty() {
        "inop".to_string()
    } else {
        labels.join(", ")
    }
}

fn nexrad_frame_age_banner_value(session: &UiSession) -> String {
    if !session.map_layer_state.nexrad.visible {
        return "off".to_string();
    }
    let frames = nexrad_frame_candidates(session);
    if frames.is_empty() {
        return "inop".to_string();
    }
    let animation = nexrad_animation_for_frames(&frames, session.wall_clock_epoch_ms);
    let Some(index) = animation.selected_frame_index else {
        return "---".to_string();
    };
    nexrad_frame_age_values(&frames, session.wall_clock_epoch_ms)
        .get(index)
        .cloned()
        .unwrap_or_else(|| "inop".to_string())
}

fn nexrad_freshest_frame_observed_at_utc(session: &UiSession) -> Option<DateTime<Utc>> {
    nexrad_frame_candidates(session)
        .into_iter()
        .filter_map(|frame| frame.observed_at_utc)
        .max()
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
        link_url: None,
        time_utc: None,
        time_display: None,
    }
}

fn status_link_fact(
    label: impl Into<String>,
    value: impl Into<String>,
    link_url: impl Into<String>,
) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: value.into(),
        link_url: Some(link_url.into()),
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
        link_url: None,
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
    let app_state = register_bad_autopilot_source(app_state)?;
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
        None,
        selected_airport_id,
        selected_chart_id,
    );
    let map_layer_state = default_map_layer_state();
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_derive_chart_page_state");
    }
    let playback = PlaybackSessionState::default();
    let mut map_follow = MapFollowSessionState::default();
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_default_session_state");
    }
    let snapshot_app_state = state::project_ui_snapshot_app_state(&app_state);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_snapshot_app_state");
    }
    let debug_state = default_debug_state();
    let mut app_ui_state = state::project_app_ui_state(&app_state);
    project_bad_autopilot_availability_for_state(&debug_state, false, &mut app_ui_state);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_app_ui_state");
    }
    let playback_ui_state = playback.ui_state();
    let playback_panel_state = playback_panel_state_for_app_state(&app_state);
    let (map_follow_ui_state, map_follow_target_viewport) =
        map_follow.snapshot_projection(&app_state.ownship.render);
    if let Some(mark) = mark.as_deref_mut() {
        mark("core_project_other_ui_state");
    }
    let mut data_status_records = BTreeMap::new();
    for record in procedure_geometry_status_records_for_plan(&active_plan) {
        data_status_records.insert(record.id.clone(), record);
    }
    let guidance_leg_geometry = self_contained_guidance_leg_geometry_for_plan(&active_plan)?
        .unwrap_or_default()
        .into_iter()
        .map(|geometry| (geometry.leg_id.clone(), geometry))
        .collect();
    let hushed_status_ids = BTreeSet::new();
    let data_status_state = project_data_status_state(&data_status_records, &hushed_status_ids);
    let data_status_page_state = default_data_status_page_state();
    let settings_page_state = default_settings_page_state();
    let settings_preferences = SettingsPreferences::default();
    let platform_capabilities = PlatformCapabilities::default();
    let home_page_state = project_home_page_state(&platform_capabilities);
    let snapshot = UiSessionSnapshot {
        session_revision: 0,
        nav_data_epoch: 0,
        active_nav_db: None,
        next_nav_db_maintenance_epoch_ms: None,
        app_state: snapshot_app_state,
        app_ui_state,
        playback_ui_state,
        playback_panel_state,
        map_follow_ui_state,
        map_follow_target_viewport,
        chart_page_state: chart_page_state.clone(),
        map_layer_state: map_layer_state.clone(),
        data_status_state: data_status_state.clone(),
        data_status_page_state,
        settings_page_state,
        home_page_state,
        display_policy: None,
        disclaimer_state: project_disclaimer_state(&settings_preferences),
        debug_state: debug_state.clone(),
        raster_map: None,
        next_cycle_product_freshness_check_epoch_ms: None,
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    lock_sessions().insert(
        handle,
        UiSession {
            session_revision: 0,
            nav_data_epoch: 0,
            nav_db_advance_blocked: false,
            app_state,
            playback,
            plan_preview: PlanPreviewState::default(),
            bad_autopilot: BadAutopilotState::default(),
            map_follow,
            guidance_leg_geometry,
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
            platform_capabilities,
            settings_preferences,
            settings_storage: None,
            debug_state,
            resource_policy: CoreResourcePolicy::InstalledPackage,
            installed_package_ids: BTreeSet::new(),
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
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
            prepared_metar_tiles: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            nexrad_tile_cache: HashMap::new(),
            taf_payload: None,
            airport_notam_index: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms,
            live_feed_current_refresh: LiveFeedCurrentRefreshState::Idle,
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
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    if let Some(outcome) = preflight_session_snapshot_resources(session)? {
        return Ok(outcome);
    }
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
    changed_session_snapshot_outcome(session)
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
    changed_session_snapshot_outcome(session)
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

pub fn resolve_chart_asset_resource_in_session(
    handle: u32,
    chart_id: &str,
    asset_kind: &str,
) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    let chart = match read_chart_asset_by_id(session_nav_kv_store(session)?, chart_id) {
        Ok(chart) => chart,
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            });
        }
        Err(HadReadError::Fatal(message)) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidManifest,
                message,
            });
        }
    };
    let member_path = match asset_kind {
        "asset" => Some(chart.asset_path.as_str()),
        "thumbnail" => chart.thumbnail_path.as_deref(),
        _ => {
            return Err(AppError {
                kind: AppErrorKind::InvalidManifest,
                message: format!("unsupported chart asset kind: {asset_kind}"),
            });
        }
    };
    let member_path = member_path.ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("chart {chart_id} has no {asset_kind} asset"),
    })?;
    let target_resource_id = format!("chart_asset/{asset_kind}/{chart_id}");
    let mut package_ids = chart.package_ids.clone();
    if package_ids.is_empty() {
        package_ids.push(chart.package_id.clone());
    }
    if let Some(preferred_package_id) = session
        .raster_map_catalog
        .as_ref()
        .and_then(|catalog| {
            catalog
                .displayed_maps
                .iter()
                .find(|view| view.id == catalog.selected_map_id)
                .and_then(|view| view.map_view.package_name.as_ref())
        })
        .filter(|package_id| package_ids.iter().any(|candidate| candidate == *package_id))
    {
        package_ids.retain(|package_id| package_id != preferred_package_id);
        package_ids.insert(0, preferred_package_id.clone());
    }
    if let Some(installed_package_id) = package_ids
        .iter()
        .find(|package_id| session.installed_package_ids.contains(*package_id))
        .cloned()
    {
        package_ids.retain(|package_id| package_id != &installed_package_id);
        package_ids.insert(0, installed_package_id);
    }
    let package_id = package_ids.first().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("chart {chart_id} has no package source"),
    })?;
    let resources = session
        .publication_resolver
        .package_resource_requests(&target_resource_id, package_id, member_path, false)
        .map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message,
        })?;
    for resource in &resources {
        if resource.id == target_resource_id {
            return Ok(HadOperationOutcome::complete(
                serde_json::to_value(PublicationResolvedResource {
                    source: resource.source.clone(),
                })
                .map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
            ));
        }
    }
    Ok(HadOperationOutcome::NeedResources { resources })
}

pub fn resolve_metar_manifest_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    session
        .publication_resolver
        .resolve_family_resource("metars", "manifest.json")
        .map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message,
        })
}

pub fn set_resource_policy_in_session(
    handle: u32,
    policy: CoreResourcePolicy,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if session.resource_policy != policy {
        session.resource_policy = policy;
        session.publication_resolver.set_resource_policy(policy);
        session.raster_map_catalog = None;
    }
    changed_session_snapshot_outcome(session)
}

pub fn load_offline_package_library_cache_in_session(
    handle: u32,
    cache: OfflinePackagesLibraryCache,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, cache.fetched_at_epoch_ms);
    session
        .publication_resolver
        .load_offline_library_cache(&cache)
        .map_err(|message| AppError {
            kind: AppErrorKind::InvalidManifest,
            message,
        })?;
    mark_cycle_product_freshness_dirty(session);
    changed_session_snapshot_outcome(session)
}

pub fn configure_platform_capabilities_in_session(
    handle: u32,
    capabilities: PlatformCapabilities,
    settings_storage: Option<SettingsStorageHandle>,
) -> AppResult<HadOperationOutcome> {
    if let Some(local_time_zone) = capabilities.local_time_zone.as_deref() {
        local_time_zone.parse::<Tz>().map_err(|_| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("unsupported platform local time zone {local_time_zone:?}"),
        })?;
    }
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.platform_capabilities = capabilities;
    session.settings_storage = settings_storage;
    load_settings_preferences_from_storage(session)?;
    changed_session_snapshot_outcome(session)
}

pub fn perform_settings_action_in_session(
    handle: u32,
    action: UiSettingsAction,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    match action.action_id.as_str() {
        DISPLAY_DIM_TIMEOUT_ACTION_ID => {
            if session.platform_capabilities.display_policy.is_none() {
                return Err(invalid_settings_action(&action.action_id));
            }
            let timeout = DisplayDimTimeout::from_value_id(&action.value_id).ok_or_else(|| {
                invalid_settings_action_value(&action.action_id, &action.value_id)
            })?;
            session.settings_preferences.display_dim_timeout = timeout;
            write_settings_preferences_to_storage(session)?;
        }
        FLIGHT_DATA_VISIBILITY_ACTION_ID => {
            if !crate::flight_data::is_flight_data_banner_cell_id(&action.value_id) {
                return Err(invalid_settings_action_value(
                    &action.action_id,
                    &action.value_id,
                ));
            }
            if !session
                .settings_preferences
                .disabled_flight_data_cell_ids
                .remove(&action.value_id)
            {
                session
                    .settings_preferences
                    .disabled_flight_data_cell_ids
                    .insert(action.value_id);
            }
            write_settings_preferences_to_storage(session)?;
        }
        _ => return Err(invalid_settings_action(&action.action_id)),
    }
    changed_session_snapshot_outcome(session)
}

pub fn accept_disclaimer_in_session(
    handle: u32,
    agreement_id: &str,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if agreement_id != NO_WARRANTY_DISCLAIMER_AGREEMENT_ID {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("unsupported disclaimer agreement id: {agreement_id}"),
        });
    }
    session
        .settings_preferences
        .accepted_disclaimer_agreement_ids
        .insert(agreement_id.to_string());
    write_settings_preferences_to_storage(session)?;
    changed_session_snapshot_outcome(session)
}

pub fn set_installed_package_ids_in_session(
    handle: u32,
    package_ids: Vec<String>,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.installed_package_ids = package_ids.into_iter().collect();
    changed_session_snapshot_outcome(session)
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
        return changed_session_snapshot_outcome(session);
    };
    crate::select_map_family_in_catalog(catalog, family_id);
    sync_cycle_product_freshness_status_records(session);
    changed_session_snapshot_outcome(session)
}

pub fn select_raster_map_in_session(
    handle: u32,
    selected_map_id: &str,
) -> AppResult<HadOperationOutcome> {
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
    changed_session_snapshot_outcome(session)
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
    // TASK-25 raster exception: raster tiles intentionally do not flow through
    // generic CoreResourceRequest ingestion. Core still owns package/member
    // resolution here, but returns resolved browser URLs so the web renderer can
    // let the browser image cache stream many tiles cheaply. New non-raster
    // resources should use the normalized core-driven resource path instead;
    // see resolve_chart_asset_resource_in_session and terrain/NEXRAD queries.
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
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let layer = parse_map_layer_id(layer_id)?;
    if let Some(outcome) = preflight_session_snapshot_resources(session)? {
        return Ok(outcome);
    }
    let toggle = map_layer_toggle_mut(&mut session.map_layer_state, layer);
    toggle.enabled = enabled;
    toggle.disabled_reason = (!enabled).then(|| map_layer_disabled_reason(layer).to_string());
    if !enabled {
        toggle.visible = false;
    }
    changed_session_snapshot_outcome(session)
}

#[allow(dead_code)]
pub(crate) fn set_guidance_leg_geometry_in_session(
    handle: u32,
    geometries: Vec<GuidanceLegGeometry>,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    install_guidance_leg_geometry(session, geometries)?;
    changed_session_snapshot_outcome(session)
}

fn install_guidance_leg_geometry(
    session: &mut UiSession,
    geometries: Vec<GuidanceLegGeometry>,
) -> AppResult<()> {
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
    Ok(())
}

fn guidance_leg_geometry_from_route(
    route: Vec<crate::FlightPlanRouteSegment>,
) -> Vec<GuidanceLegGeometry> {
    route
        .into_iter()
        .map(|segment| GuidanceLegGeometry {
            leg_id: segment.id,
            from: segment.from,
            to: segment.to,
            path: segment.path,
        })
        .collect()
}

fn self_contained_guidance_leg_geometry_for_plan(
    plan: &FlightPlan,
) -> AppResult<Option<Vec<GuidanceLegGeometry>>> {
    let mut resolve_position = |nav_ref: &NavRef, _procedure_airport_id: Option<&str>| match nav_ref
    {
        NavRef::LatLon(position) | NavRef::Spot(position) => Ok(*position),
        _ => Err(()),
    };
    if let Ok(route) = crate::project_flight_plan_route_with_resolver(plan, &mut resolve_position) {
        return Ok(Some(guidance_leg_geometry_from_route(route)));
    }

    let mut geometries = Vec::new();
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        if let Ok(route) = crate::project_flight_plan_leg_route_with_resolver(
            plan,
            leg_index,
            leg,
            &mut resolve_position,
        ) {
            geometries.extend(guidance_leg_geometry_from_route(route));
        }
    }
    Ok((!geometries.is_empty()).then_some(geometries))
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
            changed_session_snapshot_outcome(session)
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
        if let Some(geometries) =
            self_contained_guidance_leg_geometry_for_plan(&plan).map_err(|err| {
                HadReadError::Fatal(format!(
                    "self-contained route projection failed: {}",
                    err.message
                ))
            })?
        {
            install_guidance_leg_geometry(session, geometries)
                .map_err(|err| HadReadError::Fatal(err.message))?;
        } else {
            session.guidance_leg_geometry.clear();
        }
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
    install_guidance_leg_geometry(session, guidance_leg_geometry_from_route(route))
        .map_err(|err| HadReadError::Fatal(err.message))?;
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

pub fn select_airport_in_session(handle: u32, airport_id: &str) -> AppResult<HadOperationOutcome> {
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
    session.chart_page_state = derive_compact_chart_page_state(
        &plan,
        &recent_airport_ids,
        session.chart_page_state.plate_target_airport_id.as_deref(),
        Some(airport_id),
        None,
    );
    changed_session_snapshot_outcome(session)
}

pub fn select_chart_in_session(handle: u32, chart_id: &str) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state_with_reference(
        &plan,
        &session.chart_page_state.recent_airport_ids,
        session.chart_page_state.plate_target_airport_id.as_deref(),
        Some(&session.chart_page_state.selected_airport_id),
        session
            .chart_page_state
            .selected_reference_family_id
            .as_deref(),
        Some(chart_id),
        &session.chart_page_state.suggested_chart_ids,
    );
    changed_session_snapshot_outcome(session)
}

pub fn select_chart_reference_in_session(
    handle: u32,
    family_id: &str,
    suggested_chart_ids: &[String],
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state_with_reference(
        &plan,
        &session.chart_page_state.recent_airport_ids,
        session.chart_page_state.plate_target_airport_id.as_deref(),
        Some(&session.chart_page_state.selected_airport_id),
        Some(family_id),
        None,
        suggested_chart_ids,
    );
    changed_session_snapshot_outcome(session)
}

pub fn register_ownship_source_in_session(
    handle: u32,
    registration: crate::OwnshipSourceRegistration,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::RegisterOwnshipSource(registration),
    )?;
    changed_session_snapshot_outcome(session)
}

pub fn update_ownship_source_status_in_session(
    handle: u32,
    update: crate::OwnshipSourceStatusUpdate,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    maybe_log_gps_capture_status(session, &update);
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::UpdateOwnshipSourceStatus(update),
    )?;
    changed_session_snapshot_outcome(session)
}

pub fn push_situation_sample_in_session(
    handle: u32,
    sample: crate::SituationSample,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = apply_ownship_sample(session, sample)?;
    changed_session_snapshot_outcome_for_ownship_motion(session, Some(motion))
}

fn maybe_log_gps_capture_status(session: &UiSession, update: &crate::OwnshipSourceStatusUpdate) {
    if !session.debug_state.gps_capture {
        return;
    }
    let source_kind = session
        .app_state
        .ownship
        .sources
        .iter()
        .find(|source| source.source_id == update.source_id)
        .map(|source| source.source_kind);
    if !source_kind.is_some_and(is_gps_capture_source_kind) {
        return;
    }
    crate::core_debug_log(
        "ownship.gps_capture.status",
        &serde_json::json!({
            "kind": "status",
            "source_id": update.source_id,
            "source_kind": source_kind,
            "update": update,
        }),
    );
}

fn maybe_log_gps_capture_sample(session: &UiSession, sample: &crate::SituationSample) {
    if !session.debug_state.gps_capture || !is_gps_capture_source_kind(sample.source_kind) {
        return;
    }
    if session
        .app_state
        .ownship
        .sources
        .iter()
        .find(|source| source.source_id == sample.source_id)
        .and_then(|source| source.latest_sample.as_ref())
        .is_some_and(|latest_sample| latest_sample == sample)
    {
        return;
    }
    crate::core_debug_log(
        "ownship.gps_capture.sample",
        &serde_json::json!({
            "kind": "sample",
            "source_id": sample.source_id,
            "source_kind": sample.source_kind,
            "sample": sample,
        }),
    );
}

fn is_gps_capture_source_kind(source_kind: crate::OwnshipSourceKind) -> bool {
    matches!(
        source_kind,
        crate::OwnshipSourceKind::DeviceGps | crate::OwnshipSourceKind::ExternalGps
    )
}

pub fn set_ownship_policy_in_session(
    handle: u32,
    policy: crate::OwnshipPolicy,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::SetOwnshipPolicy(policy))?;
    changed_session_snapshot_outcome(session)
}

pub fn select_ownship_source_in_session(
    handle: u32,
    selection: crate::OwnshipSelectionCommand,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let terrain_key_before = ownship_terrain_refresh_key(session);
    let mut sequenced_guidance = false;
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
    if selected_source_kind == Some(crate::OwnshipSourceKind::BadAutopilot)
        && !bad_autopilot_selectable(session)
    {
        return session_snapshot_outcome(session);
    }
    session.app_state =
        state::reduce(&session.app_state, AppEvent::SelectOwnshipSource(selection))?;
    match selected_source_kind {
        Some(crate::OwnshipSourceKind::FlightPlanSimulator) => {
            sync_plan_preview_to_active_leg(session)?;
        }
        Some(crate::OwnshipSourceKind::BadAutopilot) => {
            session.bad_autopilot = BadAutopilotState::default();
            sequenced_guidance =
                tick_bad_autopilot(session, 0.0)?.is_some_and(|motion| motion.sequenced_guidance);
        }
        _ => {}
    }
    changed_session_snapshot_outcome_with_invalidations(
        session,
        ownship_motion_invalidations_from(session, terrain_key_before, sequenced_guidance),
    )
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
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    situation_source_handler_for_session(session).apply_input(session, input, now_epoch_ms)?;
    changed_session_snapshot_outcome(session)
}

pub fn load_playback_trace_in_session(
    handle: u32,
    source_path: &str,
    trace_json: &str,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let playback_state = session
        .playback
        .load_trace_json(source_path.to_string(), trace_json)?;
    let motion = apply_playback_state_to_ownship(session, playback_state, 0)?;
    changed_session_snapshot_outcome_for_ownship_motion(session, Some(motion))
}

pub fn play_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = session
        .playback
        .play(now_epoch_ms)
        .map(|playback_state| {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)
        })
        .transpose()?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

pub fn pause_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = session
        .playback
        .pause(now_epoch_ms)
        .map(|playback_state| {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)
        })
        .transpose()?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

pub fn seek_playback_in_session(
    handle: u32,
    cursor_seconds: f64,
    now_epoch_ms: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = session
        .playback
        .seek(cursor_seconds, now_epoch_ms)
        .map(|playback_state| {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)
        })
        .transpose()?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

pub fn set_playback_rate_in_session(
    handle: u32,
    rate: f64,
    now_epoch_ms: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = session
        .playback
        .set_rate(rate, now_epoch_ms)
        .map(|playback_state| {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)
        })
        .transpose()?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

pub fn tick_playback_in_session(handle: u32, now_epoch_ms: f64) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = session
        .playback
        .tick(now_epoch_ms)
        .map(|playback_state| {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)
        })
        .transpose()?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

pub fn set_situation_in_session(
    handle: u32,
    situation: crate::Situation,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = apply_situation_to_ownship(
        session,
        DIRECT_SITUATION_SOURCE_ID,
        crate::OwnshipSourceKind::FlightPlanSimulator,
        "Plan Preview",
        situation,
        0,
    )?;
    changed_session_snapshot_outcome_for_ownship_motion(session, Some(motion))
}

pub fn tick_bad_autopilot_in_session(
    handle: u32,
    now_epoch_ms: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let motion = tick_bad_autopilot(session, now_epoch_ms)?;
    changed_session_snapshot_outcome_for_ownship_motion(session, motion)
}

#[allow(dead_code)]
fn replace_flight_plan_in_session(handle: u32, plan: FlightPlan) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    replace_session_flight_plan(session, plan)?;
    changed_session_snapshot_outcome(session)
}

pub fn activate_next_leg_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    mutate_session_flight_plan(handle, crate::activate_next_leg)
}

pub fn stop_navigation_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    mutate_session_flight_plan(handle, crate::stop_navigation)
}

pub fn suspend_sequencing_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    mutate_session_guidance(handle, crate::suspend_sequencing)
}

pub fn unsuspend_sequencing_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    mutate_session_guidance(handle, crate::unsuspend_sequencing)
}

pub fn sequence_active_leg_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    mutate_session_flight_plan(handle, crate::sequence_active_leg)
}

fn mutate_session_flight_plan(
    handle: u32,
    mutation: impl FnOnce(&FlightPlan) -> AppResult<FlightPlan>,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = mutation(&plan)?;
    replace_session_flight_plan(session, next_plan)?;
    changed_session_snapshot_outcome(session)
}

fn mutate_session_guidance(
    handle: u32,
    mutation: impl FnOnce(&FlightPlan) -> AppResult<FlightPlan>,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = mutation(&plan)?;
    session.app_state = state::reduce(&session.app_state, AppEvent::ReplaceFlightPlan(next_plan))?;
    changed_session_snapshot_outcome(session)
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
    let mut built = match materialize_procedure(
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
    if let Some(display_label) = command
        .display_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        built.procedure.display_label = Some(display_label.to_string());
    }
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
) -> AppResult<HadOperationOutcome> {
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
    changed_session_snapshot_outcome(session)
}

pub fn perform_status_action_in_session(
    handle: u32,
    action_id: String,
) -> AppResult<HadOperationOutcome> {
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
        UiStatusActionCommand::ReloadApplication => {
            if !session
                .data_status_records
                .values()
                .flat_map(|record| &record.actions)
                .any(|action| action.id == action_id && action.enabled)
            {
                return Err(invalid_status_action(&action_id));
            }
        }
    }
    changed_session_snapshot_outcome(session)
}

fn invalid_status_action(action_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown status action: {action_id}"),
    }
}

fn invalid_settings_action(action_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown settings action: {action_id}"),
    }
}

fn invalid_settings_action_value(action_id: &str, value_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown settings value for {action_id}: {value_id}"),
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

pub fn restore_direct_to_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    let next_plan = crate::restore_direct_to(&plan)?;
    commit_session_flight_plan_with_invalidations_outcome(session, next_plan)
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

pub fn advance_nav_kv_store_in_session_with_open_result(
    handle: u32,
    store_id: u32,
    store: &NavKvStore,
    open_result: &NavDbOpenResult,
    installed_package_ids: Vec<String>,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let live = session_mut(&mut sessions, handle)?;
    if live.nav_db_advance_blocked {
        let active_artifact_filename = live
            .nav_db_artifact
            .as_ref()
            .map(|artifact| artifact.filename.clone());
        let result = NavDbAdvanceResult {
            disposition: NavDbAdvanceDisposition::Rejected,
            snapshot: try_snapshot_for_session(live).map_err(|error| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "NAVDB advance is blocked until application reload; retained snapshot failed: {error:?}"
                ),
            })?,
            active_artifact_filename: active_artifact_filename.clone(),
            retained_artifact_filenames: active_artifact_filename.into_iter().collect(),
            rejection_reason: Some(
                "NAVDB advance is blocked until application reload".to_string(),
            ),
        };
        return Ok(HadOperationOutcome::complete(
            serde_json::to_value(result).map_err(|err| AppError {
                kind: AppErrorKind::Internal,
                message: err.to_string(),
            })?,
        ));
    }
    let previous_filename = live
        .nav_db_artifact
        .as_ref()
        .map(|artifact| artifact.filename.clone());
    let nav_db_changed = previous_filename.as_deref() != Some(&open_result.selected_filename);
    let selected_map_id = live
        .raster_map_catalog
        .as_ref()
        .map(|catalog| catalog.selected_map_id.clone());
    let selected_family_id = live.raster_map_catalog.as_ref().and_then(|catalog| {
        catalog
            .family_options
            .iter()
            .find(|family| family.active)
            .map(|family| family.id.clone())
    });

    let mut candidate = live.clone();
    candidate.nav_kv_store_id = Some(store_id);
    candidate.nav_kv_store = Some(store.clone());
    candidate.nav_db_artifact = Some(AttachedNavDbArtifact::from(open_result));
    candidate.installed_package_ids = installed_package_ids.into_iter().collect();
    candidate
        .installed_package_ids
        .insert(open_result.selected_package_id.clone());
    candidate.vector_manifest_loaded = false;
    candidate.vector_tile_cache.clear();
    candidate.airspace_feature_cache.clear();
    candidate.important_metar_station_ids = None;
    candidate.metar_station_importance_status = None;
    candidate.terrain_source_tile_cache.clear();
    rebuild_metar_tile_cache(&mut candidate);
    mark_cycle_product_freshness_dirty(&mut candidate);

    let rebuild = (|| -> Result<UiSessionSnapshot, HadReadError> {
        if let Some(plan) = candidate.app_state.active_plan.as_ref() {
            let rebuilt_plan = crate::had_ops::rebuild_flight_plan_from_nav_kv(store, plan)?;
            replace_session_flight_plan(&mut candidate, rebuilt_plan)
                .map_err(HadReadError::from)?;
        }
        candidate.raster_map_catalog = Some(crate::had_ops::raster_map_catalog_from_nav_kv(
            store,
            selected_map_id.as_deref(),
            selected_family_id.as_deref(),
        )?);
        ensure_vector_manifest_loaded(&mut candidate)?;
        if !candidate.chart_page_state.selected_chart_id.is_empty() {
            read_chart_asset_by_id(store, &candidate.chart_page_state.selected_chart_id)?;
        }
        for airport_id in [
            candidate.chart_page_state.selected_airport_id.as_str(),
            candidate
                .chart_page_state
                .plate_target_airport_id
                .as_deref()
                .unwrap_or_default(),
        ] {
            if !airport_id.is_empty() {
                let _ = airport_plate_availability(store, airport_id)?;
            }
        }
        sync_guidance_geometry_for_session(&mut candidate, &crate::CoreDebugTimer::start())?;
        sync_cycle_product_freshness_status_records(&mut candidate);
        clear_data_status_record(&mut candidate, NAV_DB_ADVANCE_STATUS_ID);
        if nav_db_changed {
            candidate.nav_data_epoch = candidate.nav_data_epoch.saturating_add(1);
        }
        advance_session_revision(&mut candidate);
        try_snapshot_for_session(&mut candidate)
    })();

    match rebuild {
        Ok(snapshot) => {
            let active_artifact_filename = Some(open_result.selected_filename.clone());
            *live = candidate;
            let result = NavDbAdvanceResult {
                disposition: NavDbAdvanceDisposition::Adopted,
                snapshot,
                active_artifact_filename: active_artifact_filename.clone(),
                retained_artifact_filenames: active_artifact_filename.into_iter().collect(),
                rejection_reason: None,
            };
            let mut invalidations = vec![
                UiInvalidation::SessionSnapshot,
                UiInvalidation::RasterTiles,
                UiInvalidation::MapOverlay,
                UiInvalidation::TerrainOverlay,
                UiInvalidation::FlightPlanRoute,
            ];
            if nav_db_changed {
                invalidations.insert(0, UiInvalidation::NavData);
            }
            Ok(HadOperationOutcome::complete_with_invalidations(
                serde_json::to_value(result).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
                invalidations,
            ))
        }
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => {
            let candidate_label = open_result
                .selected_cycle
                .as_deref()
                .unwrap_or(&open_result.selected_filename);
            let mut warning = DataStatusRecord::new(
                NAV_DB_ADVANCE_STATUS_ID,
                "NAV DB",
                Some("ADVANCE FAILED".to_string()),
                UiStatusSeverity::Warning,
                true,
                format!(
                    "Could not advance to new NAVDB {candidate_label}. Reload application when not flying. {message}"
                ),
            )
            .with_action(UiStatusAction {
                id: RELOAD_APPLICATION_ACTION_ID.to_string(),
                label: "Reload application".to_string(),
                enabled: true,
                style: UiStatusActionStyle::Normal,
            });
            warning.hushable = false;
            live.nav_db_advance_blocked = true;
            live.installed_package_ids = candidate.installed_package_ids.clone();
            if let Some(active_artifact) = live.nav_db_artifact.as_ref() {
                live.installed_package_ids
                    .insert(active_artifact.package_id.clone());
            }
            upsert_data_status_record(live, warning);
            advance_session_revision(live);
            let snapshot = try_snapshot_for_session(live).map_err(|error| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: match error {
                    HadReadError::NeedPages(pages) => format!(
                        "NAVDB advance failed and the retained session needs pages {pages:?}: {message}"
                    ),
                    HadReadError::Fatal(snapshot_message) => format!(
                        "NAVDB advance failed ({message}); retained snapshot failed: {snapshot_message}"
                    ),
                },
            })?;
            let active_artifact_filename = previous_filename;
            let result = NavDbAdvanceResult {
                disposition: NavDbAdvanceDisposition::Rejected,
                snapshot,
                active_artifact_filename: active_artifact_filename.clone(),
                retained_artifact_filenames: active_artifact_filename.into_iter().collect(),
                rejection_reason: Some(message),
            };
            Ok(HadOperationOutcome::complete_with_invalidations(
                serde_json::to_value(result).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
                vec![UiInvalidation::SessionSnapshot],
            ))
        }
    }
}

pub fn maintain_nav_db_in_session_at_epoch_ms(
    handle: u32,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    mark_cycle_product_freshness_dirty_if_deadline_due(session);
    sync_cycle_product_freshness_status_records_if_needed(session);

    if session.nav_db_advance_blocked {
        return nav_db_maintenance_outcome(session, NavDbMaintenanceAction::None);
    }

    if session.resource_policy == CoreResourcePolicy::PublicUnpacked {
        let refresh_due = session
            .publication_resolver
            .current_artifacts_checked_epoch_ms()
            .is_none_or(|checked| {
                checked.saturating_add(NAV_DB_PUBLICATION_POLL_INTERVAL_MS)
                    <= session.wall_clock_epoch_ms
            });
        if refresh_due {
            let resource = session
                .publication_resolver
                .current_artifacts_refresh_request()
                .map_err(|message| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message,
                })?;
            return Ok(HadOperationOutcome::NeedResources {
                resources: vec![resource],
            });
        }
    }

    let candidates = match nav_db_candidates_for_session(session).map_err(|message| AppError {
        kind: AppErrorKind::InvalidManifest,
        message,
    })? {
        Ok(candidates) => candidates,
        Err(resources) => {
            if session.resource_policy == CoreResourcePolicy::PublicUnpacked {
                return Ok(HadOperationOutcome::NeedResources { resources });
            }
            return nav_db_maintenance_outcome(session, NavDbMaintenanceAction::None);
        }
    };
    let preferred =
        crate::had_ops::select_preferred_nav_db_candidate(&candidates, session.wall_clock_epoch_ms);
    let action = if preferred.is_some_and(|candidate| {
        session
            .nav_db_artifact
            .as_ref()
            .is_none_or(|active| active.filename != candidate.filename)
    }) {
        NavDbMaintenanceAction::AttemptAdvance
    } else {
        NavDbMaintenanceAction::None
    };
    nav_db_maintenance_outcome(session, action)
}

fn nav_db_maintenance_outcome(
    session: &mut UiSession,
    action: NavDbMaintenanceAction,
) -> AppResult<HadOperationOutcome> {
    let result = NavDbMaintenanceResult {
        action,
        snapshot: try_snapshot_for_session(session).map_err(|error| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("NAVDB maintenance snapshot failed: {error:?}"),
        })?,
    };
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(result).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    ))
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

pub fn debug_drop_nav_kv_pages_for_attached_sessions(store_id: u32) {
    let mut sessions = lock_sessions();
    for session in sessions.values_mut() {
        if session.nav_kv_store_id == Some(store_id) {
            if let Some(store) = session.nav_kv_store.as_mut() {
                store.clear_pages();
            }
        }
    }
}

pub fn engage_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.engage(viewport);
    changed_session_snapshot_outcome(session)
}

pub fn disengage_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.disengage(viewport);
    changed_session_snapshot_outcome(session)
}

pub fn set_map_follow_offset_in_session(
    handle: u32,
    viewport: MapViewport,
    offset_x_px: f64,
    offset_y_px: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session
        .map_follow
        .set_anchor_offset(viewport, offset_x_px, offset_y_px);
    changed_session_snapshot_outcome(session)
}

pub fn sync_map_follow_in_session(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.map_follow.sync_for_viewport(
        &session.app_state.ownship.render,
        viewport,
        width_px,
        height_px,
    );
    changed_session_snapshot_outcome(session)
}

pub fn restore_chart_page_state_in_session(
    handle: u32,
    recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
    selected_airport_id: Option<&str>,
    selected_reference_family_id: Option<&str>,
    selected_chart_id: Option<&str>,
    suggested_chart_ids: &[String],
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let plan = session_plan(session)?;
    session.chart_page_state = derive_compact_chart_page_state_with_reference(
        &plan,
        recent_airport_ids,
        plate_target_airport_id,
        selected_airport_id,
        selected_reference_family_id,
        selected_chart_id,
        suggested_chart_ids,
    );
    changed_session_snapshot_outcome(session)
}

pub fn set_debug_flag_in_session(
    handle: u32,
    flag_id: &str,
    enabled: bool,
) -> AppResult<HadOperationOutcome> {
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
        "gps_capture" => session.debug_state.gps_capture = enabled,
        "debug_log_to_developer_server" => {
            session.debug_state.debug_log_to_developer_server = enabled
        }
        "bad_autopilot" => {
            session.debug_state.bad_autopilot = enabled;
            if !enabled {
                session.bad_autopilot = BadAutopilotState::default();
                if selected_ownship_source_kind(&session.app_state.ownship)
                    == Some(crate::OwnshipSourceKind::BadAutopilot)
                {
                    session.app_state = state::reduce(
                        &session.app_state,
                        AppEvent::SelectOwnshipSource(crate::OwnshipSelectionCommand::Auto),
                    )?;
                }
            }
        }
        _ => {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: format!("unknown debug flag id: {flag_id}"),
            });
        }
    }
    changed_session_snapshot_outcome(session)
}

pub fn get_session_snapshot(handle: u32) -> AppResult<HadOperationOutcome> {
    get_session_snapshot_at_epoch_ms(handle, 0)
}

pub fn get_session_snapshot_at_epoch_ms(
    handle: u32,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let total_started_at = crate::core_clock_ms();
    let lock_started_at = crate::core_clock_ms();
    let mut sessions = lock_sessions();
    let lock_ms = elapsed_ms(lock_started_at);
    let lookup_started_at = crate::core_clock_ms();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    mark_cycle_product_freshness_dirty_if_deadline_due(session);
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
    let result = session_snapshot_outcome(session);
    let snapshot_ms = elapsed_ms(snapshot_started_at);
    crate::core_perf_debug_log("session.snapshot.total", || {
        serde_json::json!({
            "total_ms": elapsed_ms(total_started_at),
            "lock_ms": lock_ms,
            "lookup_ms": lookup_ms,
            "cycle_freshness_ms": cycle_freshness_ms,
            "live_feed_status_ms": live_feed_status_ms,
            "snapshot_ms": snapshot_ms,
            "status_record_count": status_record_count,
            "pending_resource_effect_count": pending_resource_effect_count,
            "status": if result.is_ok() { "ok" } else { "error" },
        })
    });
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
    Ok(session
        .live_feeds
        .sync_outcome_with_invalidations_at_epoch_ms(session.wall_clock_epoch_ms))
}

pub fn refresh_live_feed_current_in_session(handle: u32) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    if session.live_feed_current_refresh == LiveFeedCurrentRefreshState::Ingested {
        session.live_feed_current_refresh = LiveFeedCurrentRefreshState::Idle;
        return Ok(session.live_feeds.complete_outcome_with_invalidations());
    }
    session.live_feed_current_refresh = LiveFeedCurrentRefreshState::Requested;
    Ok(session
        .live_feeds
        .refresh_current_outcome_with_invalidations_at_epoch_ms(session.wall_clock_epoch_ms))
}

pub fn configure_live_feed_source_in_session(handle: u32, source_root_url: &str) -> AppResult<()> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let normalized = session.live_feeds.set_source_root_url(source_root_url)?;
    session.live_feed_connection.source_url = Some(normalized.clone());
    session.live_feed_connection.status_url = Some(crate::live_feed_status_url(&normalized)?);
    Ok(())
}

pub fn live_feed_runtime_decision_in_session(
    handle: u32,
    input: LiveFeedRuntimeInput,
) -> AppResult<LiveFeedRuntimeDecision> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    Ok(crate::live_feed_runtime_decision(
        &mut session.live_feed_connection.runtime,
        input,
    ))
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
            source_url: None,
            status_url: None,
            network_status: None,
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
                source_url: None,
                status_url: None,
                network_status: None,
            },
            epoch_ms,
        );
    }
    let affected = match session.live_feeds.ingest_sse_events(events.iter().cloned()) {
        Ok(affected) => affected,
        Err(err) => {
            record_live_feed_resource_error(
                session,
                format!("Failed to ingest live-feed server event: {}", err.message),
            );
            return Ok(session
                .live_feeds
                .sync_outcome_with_invalidations_at_epoch_ms(session.wall_clock_epoch_ms));
        }
    };
    Ok(session
        .live_feeds
        .sync_products_outcome_with_invalidations(affected.iter().map(String::as_str)))
}

fn mark_live_feed_current_refresh_ingested(session: &mut UiSession, resource_id: &str) {
    if resource_id == "live_feeds/current"
        && session.live_feed_current_refresh == LiveFeedCurrentRefreshState::Requested
    {
        session.live_feed_current_refresh = LiveFeedCurrentRefreshState::Ingested;
    }
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
    if let Some(src) = nexrad_tile_src_from_resource_id(resource_id) {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        session.nexrad_tile_cache.insert(src, bytes.to_vec());
        return Ok(());
    }
    if LiveFeedsState::handles_resource(resource_id) {
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        if resource_id == "live_feeds/current" {
            record_live_feed_connection_event(
                session,
                LiveFeedConnectionEvent {
                    kind: LiveFeedConnectionEventKind::Message,
                    message: None,
                    source_url: None,
                    status_url: None,
                    network_status: None,
                },
                epoch_ms,
            );
        }
        if let Err(err) = session.live_feeds.ingest_resource(resource_id, bytes) {
            mark_live_feed_current_refresh_ingested(session, resource_id);
            session
                .live_feeds
                .record_resource_failure(resource_id, session.wall_clock_epoch_ms);
            let detail = live_feed_resource_failure_detail(resource_id, &err.message);
            record_live_feed_resource_error(session, detail);
            record_live_feed_fetch_failure(session, resource_id, &err.message);
            sync_live_feed_overlay_status_records(session);
            return Ok(());
        }
        if let Err(err) = install_live_feed_payloads(session) {
            mark_live_feed_current_refresh_ingested(session, resource_id);
            session
                .live_feeds
                .record_resource_failure(resource_id, session.wall_clock_epoch_ms);
            let detail = live_feed_resource_failure_detail(resource_id, &err.message);
            record_live_feed_resource_error(session, detail);
            record_live_feed_fetch_failure(session, resource_id, &err.message);
            sync_live_feed_overlay_status_records(session);
            return Ok(());
        }
        mark_live_feed_current_refresh_ingested(session, resource_id);
        clear_live_feed_resource_error(session);
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
        if resource_id == "publication/current_artifacts" {
            advance_session_wall_clock(session, epoch_ms);
            session
                .publication_resolver
                .ingest_resource_at_epoch_ms(resource_id, bytes, session.wall_clock_epoch_ms)
                .map_err(|message| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message,
                })?;
            mark_cycle_product_freshness_dirty(session);
            return Ok(());
        }
        session
            .publication_resolver
            .ingest_resource(resource_id, bytes)
            .map_err(|message| AppError {
                kind: AppErrorKind::InvalidManifest,
                message,
            })?;
        mark_cycle_product_freshness_dirty(session);
        return Ok(());
    }
    if let Some(rest) = resource_id.strip_prefix("terrain/source/") {
        let abt2_bytes =
            crate::terrain::terrain_source_payload_to_abt2_bytes(bytes).map_err(|message| {
                AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message,
                }
            })?;
        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, handle)?;
        session
            .terrain_source_tile_cache
            .insert(rest.to_string(), abt2_bytes);
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
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    record_live_feed_connection_event(session, event, epoch_ms);
    changed_session_snapshot_outcome(session)
}

fn record_live_feed_connection_event(
    session: &mut UiSession,
    event: LiveFeedConnectionEvent,
    epoch_ms: i64,
) {
    advance_session_wall_clock(session, epoch_ms);
    let at = session.wall_clock_epoch_ms;
    if let Some(source_url) = event.source_url {
        match session.live_feeds.set_source_root_url(&source_url) {
            Ok(normalized) => {
                session.live_feed_connection.source_url = Some(normalized);
            }
            Err(err) => {
                session.live_feed_connection.source_url = Some(source_url);
                session.live_feed_connection.last_resource_error_epoch_ms = Some(at);
                session.live_feed_connection.last_resource_error_message = Some(err.to_string());
            }
        }
    }
    if event.status_url.is_some() {
        session.live_feed_connection.status_url = event.status_url;
    }
    if event.network_status.is_some() {
        session.live_feed_connection.network_status = event.network_status;
    }
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
        LiveFeedConnectionEventKind::NetworkStatus => {}
    }
}

pub fn ingest_prepared_live_feed_resource_in_session(
    handle: u32,
    resource_id: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let envelope = crate::decode_prepared_live_feed(bytes)?;
    let envelope_version = envelope.version.clone();
    let envelope_product = envelope.product.clone();
    let notam_mutation_count = match &envelope.payload {
        crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::InstallCheckpoint(
            checkpoint,
        )) => Some(checkpoint.records.len()),
        crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::ApplyDelta(delta)) => {
            Some(delta.mutations.len())
        }
        _ => None,
    };
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    let mut next_live_feeds = session.live_feeds.clone();
    next_live_feeds.ingest_prepared_live_feed(resource_id, &envelope)?;
    if next_live_feeds.product_staged_version(&envelope_product) != Some(envelope_version.as_str())
    {
        return Ok(());
    }
    if let Err(error) = install_prepared_live_feed(session, envelope.payload) {
        if envelope_product == "notams" {
            crate::core_debug_log(
                "live_feed.notams.integrity_failure",
                &serde_json::json!({
                    "severity": "critical",
                    "live_feed_schema_version": product_contracts::LIVE_FEEDS_SCHEMA_VERSION,
                    "notam_contract_version": product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION,
                    "blob_sha256": envelope.delta_blob_sha256,
                    "expected_from_state_id": envelope.from_state_sha256,
                    "expected_to_state_id": envelope.state_sha256,
                    "actual_state_id": session
                        .airport_notam_index
                        .as_ref()
                        .map(AirportNotamIndex::state_id),
                    "mutation_count": notam_mutation_count,
                    "failure_class": notam_failure_class(&error),
                }),
            );
        }
        if envelope_product == "notams" && session.airport_notam_index.is_none() {
            session.live_feeds.mark_product_no_state("notams");
        }
        return Err(error);
    }
    session.live_feeds = next_live_feeds;
    clear_live_feed_resource_error(session);
    sync_live_feed_overlay_status_records(session);
    Ok(())
}

fn notam_failure_class(error: &AppError) -> &'static str {
    if error.message.contains("starts at") || error.message.contains("without installed state") {
        "base_state"
    } else if error.message.contains("target state") {
        "target_state"
    } else if error.message.contains("counter mismatch") {
        "counters"
    } else if error.message.contains("ordered") || error.message.contains("ordering") {
        "mutation_order"
    } else if error.message.contains("contract") {
        "contract"
    } else {
        "validation"
    }
}

pub fn install_live_feed_installed_state_in_session(
    handle: u32,
    installed: &crate::LiveFeedInstalledState,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    install_live_feed_installed_state(session, installed)?;
    changed_session_snapshot_outcome(session)
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
            session.prepared_metar_tiles = None;
            rebuild_metar_tile_cache(session);
            clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
        }
        ("tafs", crate::LiveFeedInstalledPayload::Json { bytes }) => {
            let payload: TafProductPayload =
                serde_json::from_slice(bytes).map_err(|err| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to parse installed TAF live feed: {err}"),
                })?;
            session.taf_payload = Some(payload);
            let status_id = live_feed_unavailable_status_record("tafs", String::new()).id;
            clear_data_status_record(session, &status_id);
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
        ("notams", crate::LiveFeedInstalledPayload::NotamResources { checkpoint, deltas }) => {
            install_notam_resource_chain(session, installed, checkpoint, deltas)?;
            let status_id = live_feed_unavailable_status_record("notams", String::new()).id;
            clear_data_status_record(session, &status_id);
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
    session.live_feeds.mark_durable_product_loaded(
        installed.product.clone(),
        installed.version.clone(),
        installed.state_sha256.clone(),
        installed_live_feed_state_manifest(installed),
    );
    sync_live_feed_overlay_status_records(session);
    Ok(())
}

pub fn sync_live_feed_catalog_in_session(
    handle: u32,
    live_feeds: &crate::LiveFeedsState,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    session.live_feeds.merge_catalog_from(live_feeds);
    sync_live_feed_overlay_status_records(session);
    changed_session_snapshot_outcome(session)
}

fn installed_live_feed_state_manifest(
    installed: &crate::LiveFeedInstalledState,
) -> Option<serde_json::Value> {
    match &installed.payload {
        crate::LiveFeedInstalledPayload::Json { bytes } => serde_json::from_slice(bytes).ok(),
        crate::LiveFeedInstalledPayload::NavKv { manifest, .. } => {
            serde_json::from_slice(manifest).ok()
        }
        crate::LiveFeedInstalledPayload::Opaque { .. } => None,
        crate::LiveFeedInstalledPayload::NotamResources { .. } => None,
    }
}

fn install_notam_resource_chain(
    session: &mut UiSession,
    installed: &crate::LiveFeedInstalledState,
    checkpoint_bytes: &[u8],
    delta_bytes: &[std::sync::Arc<Vec<u8>>],
) -> AppResult<()> {
    let deltas = delta_bytes
        .iter()
        .map(|bytes| {
            let decoded =
                nav_kv_package::decode_xz_if_needed(bytes).map_err(|message| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message,
                })?;
            serde_json::from_slice::<notam_state::NotamDelta>(decoded.as_ref()).map_err(|error| {
                AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to decode installed NOTAM delta: {error}"),
                }
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let start = session.airport_notam_index.as_ref().and_then(|index| {
        deltas
            .iter()
            .position(|delta| delta.from_state_id == index.state_id())
    });
    let mut work = notam_state::NotamApplyWork::default();
    if let Some(start) = start {
        let index = session
            .airport_notam_index
            .as_mut()
            .ok_or_else(|| AppError {
                kind: AppErrorKind::Internal,
                message: "NOTAM index disappeared during installed delta replay".to_string(),
            })?;
        for delta in deltas.into_iter().skip(start) {
            if let Err(error) = index.apply_delta(delta, &mut work) {
                session.airport_notam_index = None;
                return Err(AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to replay installed NOTAM delta: {error}"),
                });
            }
        }
    } else {
        let decoded =
            nav_kv_package::decode_xz_if_needed(checkpoint_bytes).map_err(|message| AppError {
                kind: AppErrorKind::InvalidManifest,
                message,
            })?;
        let checkpoint: notam_state::NotamCheckpoint = serde_json::from_slice(decoded.as_ref())
            .map_err(|error| AppError {
                kind: AppErrorKind::InvalidManifest,
                message: format!("failed to decode installed NOTAM checkpoint: {error}"),
            })?;
        let mut index =
            AirportNotamIndex::from_checkpoint(checkpoint, &mut work).map_err(|error| {
                AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to install NOTAM checkpoint: {error}"),
                }
            })?;
        for delta in deltas {
            index
                .apply_delta(delta, &mut work)
                .map_err(|error| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("failed to replay installed NOTAM delta: {error}"),
                })?;
        }
        session.airport_notam_index = Some(index);
    }
    let actual = session
        .airport_notam_index
        .as_ref()
        .map(AirportNotamIndex::state_id)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::Internal,
            message: "NOTAM resource replay produced no state".to_string(),
        })?
        .to_string();
    if actual != installed.state_sha256 || actual != installed.version {
        session.airport_notam_index = None;
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "installed NOTAM resources end at {actual}, expected {}/{}",
                installed.version, installed.state_sha256
            ),
        });
    }
    Ok(())
}

fn install_prepared_live_feed(
    session: &mut UiSession,
    payload: crate::PreparedLiveFeedPayload,
) -> AppResult<()> {
    match payload {
        crate::PreparedLiveFeedPayload::Metars(feed) => {
            if feed.schema_version != 1 {
                return Err(AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("unsupported prepared METAR schema {}", feed.schema_version),
                });
            }
            session.metar_payload = Some(feed.payload);
            session.prepared_metar_tiles = Some(feed.tiles);
            rebuild_metar_tile_cache(session);
            clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
        }
        crate::PreparedLiveFeedPayload::Tafs(payload) => {
            session.taf_payload = Some(payload);
            let status_id = live_feed_unavailable_status_record("tafs", String::new()).id;
            clear_data_status_record(session, &status_id);
        }
        crate::PreparedLiveFeedPayload::Tfrs(payload) => {
            session.tfr_payload = Some(payload);
            clear_data_status_record(session, LIVE_FEED_TFRS_STATUS_ID);
        }
        crate::PreparedLiveFeedPayload::Notams(message) => {
            let mut work = notam_state::NotamApplyWork::default();
            match message {
                crate::PreparedNotamPayload::InstallCheckpoint(checkpoint) => {
                    let index = AirportNotamIndex::from_checkpoint(checkpoint, &mut work).map_err(
                        |error| AppError {
                            kind: AppErrorKind::InvalidManifest,
                            message: format!("failed to install NOTAM checkpoint: {error}"),
                        },
                    )?;
                    session.airport_notam_index = Some(index);
                }
                crate::PreparedNotamPayload::ApplyDelta(delta) => {
                    let index = session
                        .airport_notam_index
                        .as_mut()
                        .ok_or_else(|| AppError {
                            kind: AppErrorKind::InvalidManifest,
                            message: "cannot apply NOTAM delta without installed state".to_string(),
                        })?;
                    if let Err(error) = index.apply_delta(delta, &mut work) {
                        let preserve = matches!(
                            error,
                            notam_state::NotamStateError::Contract(_)
                                | notam_state::NotamStateError::InvalidOrdering(_)
                                | notam_state::NotamStateError::BaseStateMismatch { .. }
                        );
                        if !preserve {
                            session.airport_notam_index = None;
                        }
                        return Err(AppError {
                            kind: AppErrorKind::InvalidManifest,
                            message: format!("failed to apply NOTAM delta: {error}"),
                        });
                    }
                }
            }
            let status_id = live_feed_unavailable_status_record("notams", String::new()).id;
            clear_data_status_record(session, &status_id);
        }
    }
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
) -> AppResult<HadOperationOutcome> {
    report_session_resource_failure_in_session_at_epoch_ms(handle, resource_id, message, 0)
}

pub fn report_session_resource_failure_in_session_at_epoch_ms(
    handle: u32,
    resource_id: &str,
    message: &str,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    if LiveFeedsState::handles_resource(resource_id) {
        session
            .live_feeds
            .record_resource_failure(resource_id, session.wall_clock_epoch_ms);
        record_live_feed_resource_error(
            session,
            live_feed_resource_failure_detail(resource_id, message),
        );
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
    changed_session_snapshot_outcome(session)
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
    let page_bytes = nav_kv_package::decode_xz_if_needed(bytes).map_err(|message| AppError {
        kind: AppErrorKind::InvalidManifest,
        message: format!("failed to decode obstacle HAD page {resource_id}: {message}"),
    })?;
    store.insert_page(page_index, page_bytes.into_owned());
    clear_data_status_record(session, LIVE_FEED_OBSTACLES_STATUS_ID);
    Ok(())
}

fn install_live_feed_payloads(session: &mut UiSession) -> AppResult<()> {
    if session.live_feeds.current_loaded() {
        if !session.live_feeds.has_product_current_version("metars") {
            session.metar_tile_cache.clear();
            session.metar_payload = None;
            session.prepared_metar_tiles = None;
            clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
        }
        if !session.live_feeds.has_product_current_version("tafs") {
            session.taf_payload = None;
            let status_id = live_feed_unavailable_status_record("tafs", String::new()).id;
            clear_data_status_record(session, &status_id);
        }
        if !session.live_feeds.has_product_current_version("notams") {
            session.airport_notam_index = None;
            let status_id = live_feed_unavailable_status_record("notams", String::new()).id;
            clear_data_status_record(session, &status_id);
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
                    session.prepared_metar_tiles = None;
                    rebuild_metar_tile_cache(session);
                    clear_data_status_record(session, LIVE_FEED_METARS_STATUS_ID);
                }
                Err(err) => {
                    session.metar_tile_cache.clear();
                    session.metar_payload = None;
                    session.prepared_metar_tiles = None;
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
    let loaded_tafs_version = session.live_feeds.product_loaded_version("tafs");
    let tafs_installed = session
        .taf_payload
        .as_ref()
        .and_then(|payload| loaded_tafs_version.map(|version| payload.version_label == version))
        .unwrap_or(false);
    if !tafs_installed {
        if let Some(tafs_value) = session.live_feeds.product_state_manifest("tafs").cloned() {
            match serde_json::from_value::<TafProductPayload>(tafs_value) {
                Ok(payload) => {
                    session.taf_payload = Some(payload);
                    let status_id = live_feed_unavailable_status_record("tafs", String::new()).id;
                    clear_data_status_record(session, &status_id);
                }
                Err(err) => {
                    session.taf_payload = None;
                    upsert_data_status_record(
                        session,
                        live_feed_unavailable_status_record(
                            "tafs",
                            format!("TAF live feed unavailable: failed to parse state: {err}"),
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
    if let Some(current_obstacles_version) = session.live_feeds.current_product_version("obstacles")
    {
        if session
            .obstacle_had
            .as_ref()
            .is_some_and(|had| had.version != current_obstacles_version)
        {
            session.obstacle_had = None;
            session.obstacle_tile_cache.clear();
            session.map_overlay_config.obstacle_layer = None;
        }
    }
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
    tiles: &[crate::PreparedMetarTile],
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
    let feed_zooms = tiles.iter().map(|tile| tile.z).collect::<HashSet<_>>();
    if !available_zooms.is_subset(&feed_zooms) {
        return None;
    }
    let mut cache = HashMap::new();
    for tile in tiles {
        if !available_zooms.contains(&tile.z) {
            continue;
        }
        let mut records = Vec::new();
        for station_id in &tile.station_ids {
            if tile.z == layer.min_zoom && !important_station_ids.contains(station_id) {
                continue;
            }
            records.push(MetarTileRecord {
                kind: "metar".to_string(),
                id: station_id.clone(),
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
    if let Some(tiles) = session.prepared_metar_tiles.as_ref() {
        let empty = HashSet::new();
        let important_station_ids = session
            .important_metar_station_ids
            .as_ref()
            .unwrap_or(&empty);
        if let Some(cache) = metar_tile_cache_for_prepared_live_feed(
            tiles,
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
        format!("{LIVE_FEEDS_BASE_PATH}/{member}")
    } else {
        format!("{LIVE_FEEDS_BASE_PATH}/{base}/{member}")
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
    crate::core_perf_debug_log("map.overlay.vector_inputs", || {
        serde_json::json!({
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
        })
    });
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
) -> Result<Vec<DataStatusRecord>, HadReadError> {
    match ensure_vector_inputs_loaded(session, metrics) {
        Ok(()) => Ok(Vec::new()),
        Err(HadReadError::NeedPages(pages)) => Err(HadReadError::NeedPages(pages)),
        Err(HadReadError::Fatal(message)) => Ok(vec![vector_inputs_status_record(
            "Vector overlay failed",
            UiStatusSeverity::Caution,
            format!("Vector overlay data could not be loaded: {message}"),
        )]),
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
        if let HadOperationOutcome::NeedResources { resources } = session
            .live_feeds
            .sync_product_outcome_at_epoch_ms("obstacles", session.wall_clock_epoch_ms)
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
        match ensure_vector_inputs_loaded_for_map_overlay(session, &metrics) {
            Ok(records) => supplemental_status_records.extend(records),
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
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
                return Ok(HadOperationOutcome::NeedResources {
                    resources: nav_kv_page_resources(pages),
                });
            }
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
    } else {
        Vec::new()
    };
    let offline_ms = elapsed_ms(offline_started_at);
    let flight_plan_started_at = crate::core_clock_ms();
    let flight_plan_features = if display_vectors {
        match flight_plan_overlay_features(session, &viewport, width_px, height_px) {
            Ok(features) => features,
            Err(HadReadError::NeedPages(pages)) => {
                return Ok(HadOperationOutcome::NeedResources {
                    resources: nav_kv_page_resources(pages),
                });
            }
            Err(err) => return had_read_error_to_overlay_outcome(err),
        }
    } else {
        Vec::new()
    };
    let flight_plan_ms = elapsed_ms(flight_plan_started_at);
    let overlay_started_at = crate::core_clock_ms();
    let mut overlay = query_map_overlay_for_surface_at(
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
        &flight_plan_features,
        Some(session_wall_clock_utc(session)),
    );
    let overlay_ms = elapsed_ms(overlay_started_at);
    overlay.flight_plan_features = flight_plan_features;
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
    let selection = match materialize_map_selection_in_session(session, &metrics, click)? {
        MapSelectionMaterialization::Complete(selection) => selection,
        MapSelectionMaterialization::NeedResources(resources) => {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
    };
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(selection).map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    ))
}

pub fn get_map_selection_for_nav_ref_in_session_with_point_display_scale_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    nav_ref: NavRef,
    point_display_scale: f64,
    epoch_ms: i64,
) -> AppResult<HadOperationOutcome> {
    let mut sessions = lock_sessions();
    let session = session_mut(&mut sessions, handle)?;
    advance_session_wall_clock(session, epoch_ms);
    let store = session_nav_kv_store(session)?;
    let position = match nav_ref_position(store, &nav_ref, None) {
        Ok(position) => position,
        Err(err) => return had_read_error_to_overlay_outcome(err),
    };
    let target_zoom = viewport.zoom.max(MAP_SELECTION_NAV_REF_MIN_FOCUS_ZOOM);
    let selection_viewport = MapViewport {
        center: position,
        zoom: target_zoom,
        ..viewport
    };
    let metrics =
        MapSurfaceMetrics::new(selection_viewport, width_px, height_px, point_display_scale);
    let selection = match materialize_map_selection_in_session(session, &metrics, position)? {
        MapSelectionMaterialization::Complete(selection) => selection,
        MapSelectionMaterialization::NeedResources(resources) => {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
    };
    let selected_item_id = crate::selected_map_selection_item_id_for_nav_ref(&selection, &nav_ref);
    Ok(HadOperationOutcome::complete(
        serde_json::to_value(MapSelectionForNavRefResult {
            position,
            target_zoom,
            selection,
            selected_item_id,
        })
        .map_err(|err| AppError {
            kind: AppErrorKind::Internal,
            message: err.to_string(),
        })?,
    ))
}

enum MapSelectionMaterialization {
    Complete(MapSelectionQueryResult),
    NeedResources(Vec<CoreResourceRequest>),
}

fn had_read_error_to_map_selection_materialization(
    err: HadReadError,
) -> AppResult<MapSelectionMaterialization> {
    match err {
        HadReadError::NeedPages(pages) => Ok(MapSelectionMaterialization::NeedResources(
            nav_kv_page_resources(pages),
        )),
        HadReadError::Fatal(message) => Err(AppError {
            kind: AppErrorKind::Internal,
            message,
        }),
    }
}

fn materialize_map_selection_in_session(
    session: &mut UiSession,
    metrics: &MapSurfaceMetrics,
    click: LatLon,
) -> AppResult<MapSelectionMaterialization> {
    if let Err(err) = ensure_vector_inputs_loaded(session, &metrics) {
        return had_read_error_to_map_selection_materialization(err);
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
            Err(err) => {
                return had_read_error_to_map_selection_materialization(err);
            }
        }
    } else {
        Vec::new()
    };
    let flight_plan_points = match flight_plan_selection_points(session) {
        Ok(points) => points,
        Err(err) => {
            return had_read_error_to_map_selection_materialization(err);
        }
    };
    let local_time_zone = session
        .platform_capabilities
        .local_time_zone
        .as_deref()
        .unwrap_or("UTC")
        .parse::<Tz>()
        .map_err(|_| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: "configured platform local time zone is invalid".to_string(),
        })?;
    let selection = query_map_selection_for_surface_in_time_zone(
        metrics,
        &session.map_overlay_config,
        plan,
        click,
        &session.vector_tile_cache,
        &session.metar_tile_cache,
        session.metar_payload.as_ref(),
        session.taf_payload.as_ref(),
        session.airport_notam_index.as_ref(),
        &offline_region_records,
        &session.airspace_feature_cache,
        session.tfr_payload.as_ref(),
        &flight_plan_points,
        &mut availability,
        Some(session_wall_clock_utc(session)),
        local_time_zone,
    );
    let ownship_position = session.app_state.ownship.render.position;
    let selection = map_selection_with_ownship_distances(selection, ownship_position);
    let selection =
        map_selection_with_session_action_availability(selection, ownship_position.is_some());
    if !missing_pages.is_empty() {
        return Ok(MapSelectionMaterialization::NeedResources(
            nav_kv_page_resources(missing_pages),
        ));
    }
    Ok(MapSelectionMaterialization::Complete(selection))
}

fn map_selection_with_ownship_distances(
    mut selection: MapSelectionQueryResult,
    ownship_position: Option<LatLon>,
) -> MapSelectionQueryResult {
    let Some(ownship_position) = ownship_position else {
        return selection;
    };

    for item in selection
        .categories
        .iter_mut()
        .flat_map(|category| category.items.iter_mut())
    {
        let Some(point_position) = item.position else {
            continue;
        };
        let distance_nm = crate::great_circle_distance_nm(ownship_position, point_position);
        if !distance_nm.is_finite() {
            continue;
        }
        let distance = format!("{}nm", crate::flight_data::format_nm(distance_nm));
        item.description = Some(
            item.description
                .take()
                .map(|description| description.trim().to_string())
                .filter(|description| !description.is_empty())
                .map(|description| format!("{description} · {distance}"))
                .unwrap_or(distance),
        );
    }

    selection
}

fn map_selection_with_session_action_availability(
    mut selection: MapSelectionQueryResult,
    has_ownship_position: bool,
) -> MapSelectionQueryResult {
    if has_ownship_position {
        return selection;
    }

    for action in selection
        .categories
        .iter_mut()
        .flat_map(|category| category.items.iter_mut())
        .flat_map(|item| item.actions.iter_mut())
        .filter(|action| action.id == "direct_to")
    {
        action.enabled = false;
        action.disabled_reason = Some("Direct-to requires ownship position.".to_string());
        action.session_action = None;
        action.flight_plan_row_action = None;
    }

    selection
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
    get_scheduled_terrain_overlay_in_session_at_epoch_ms(
        handle,
        viewport,
        width_px,
        height_px,
        &BTreeSet::new(),
        &BTreeSet::new(),
        epoch_ms,
    )
}

pub fn get_scheduled_terrain_overlay_in_session_at_epoch_ms(
    handle: u32,
    viewport: MapViewport,
    width_px: f64,
    height_px: f64,
    decoded_cache_keys: &BTreeSet<String>,
    in_flight_cache_keys: &BTreeSet<String>,
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
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: crate::TerrainOverlayScheduleDecision {
                cached_count: 0,
                in_flight_count: 0,
                missing_count: 0,
                frame_complete: false,
                work_batch: Vec::new(),
            },
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
    let ownship_position = kinematics.map(|kinematics| kinematics.position);
    let terrain_altitude_ft = ownship_terrain_altitude_ft(session);
    let mut query = match session.resource_policy {
        CoreResourcePolicy::InstalledPackage => {
            crate::query_terrain_overlay_with_available_packages(
                &viewport,
                width_px,
                height_px,
                has_position,
                has_altitude,
                &session.installed_package_ids,
            )
        }
        CoreResourcePolicy::PublicUnpacked => {
            crate::query_terrain_overlay(&viewport, width_px, height_px, has_position, has_altitude)
        }
    };
    crate::prepare_terrain_overlay_frame(
        &mut query,
        terrain_altitude_ft,
        ownship_position,
        &viewport,
        width_px,
        height_px,
    );
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
                    altitude_bucket_ft: None,
                    frame_key: None,
                    schedule: crate::TerrainOverlayScheduleDecision {
                        cached_count: 0,
                        in_flight_count: 0,
                        missing_count: 0,
                        frame_complete: false,
                        work_batch: Vec::new(),
                    },
                },
                freshness_invalidations,
            );
        }
        TerrainSourceResolution::Resolved => {}
    }
    crate::schedule_terrain_overlay_frame(&mut query, decoded_cache_keys, in_flight_cache_keys);
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
    let previous_nexrad_banner_value = nexrad_frame_age_banner_value(session);
    advance_session_wall_clock(session, epoch_ms);
    let mut freshness_invalidations = Vec::new();
    if previous_nexrad_banner_value != nexrad_frame_age_banner_value(session) {
        freshness_invalidations.push(UiInvalidation::SessionSnapshot);
    }
    if !session.map_layer_state.nexrad.visible {
        return complete_nexrad_overlay_outcome_with_invalidations(
            session,
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Hidden,
                tiles: Vec::new(),
                stats: NexradOverlayStats::default(),
                animation: NexradOverlayAnimation::idle(),
            },
            freshness_invalidations,
        );
    }
    if session.nexrad_installed.is_none() {
        if let HadOperationOutcome::NeedResources { resources } = session
            .live_feeds
            .sync_product_outcome_at_epoch_ms("nexrad", session.wall_clock_epoch_ms)
        {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
        let history_resources = session
            .live_feeds
            .missing_history_resources_for_product_at_epoch_ms(
                "nexrad",
                session.wall_clock_epoch_ms,
            );
        for resource in history_resources {
            enqueue_session_resource_effect(
                session,
                resource,
                [
                    UiInvalidation::NexradOverlay,
                    UiInvalidation::SessionSnapshot,
                    UiInvalidation::DebugPanel,
                ],
            );
        }
    }
    let frames = nexrad_frame_candidates(session);
    if frames.is_empty() {
        return complete_nexrad_overlay_outcome_with_invalidations(
            session,
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Unavailable {
                    reason: "NEXRAD product is missing from the live feed index".to_string(),
                },
                tiles: Vec::new(),
                stats: NexradOverlayStats::default(),
                animation: NexradOverlayAnimation::idle(),
            },
            freshness_invalidations,
        );
    }
    let animation = nexrad_animation_for_frames(&frames, session.wall_clock_epoch_ms);
    let selected_frame_index = animation.selected_frame_index;
    let query = match selected_frame_index {
        Some(index) => {
            match nexrad_overlay_query(&frames[index].manifest, &viewport, width_px, height_px) {
                Ok(mut query) => {
                    query.animation = animation;
                    query
                }
                Err(err) => NexradOverlayQueryResult {
                    status: NexradOverlayStatus::Unavailable {
                        reason: err.to_string(),
                    },
                    tiles: Vec::new(),
                    stats: NexradOverlayStats::default(),
                    animation,
                },
            }
        }
        None => {
            let mut stats = NexradOverlayStats::default();
            stats.observed_at_utc = frames.last().and_then(|frame| frame.observed_at_utc);
            NexradOverlayQueryResult {
                status: NexradOverlayStatus::Ready { count: 0 },
                tiles: Vec::new(),
                stats,
                animation,
            }
        }
    };
    complete_nexrad_overlay_outcome_with_invalidations(session, query, freshness_invalidations)
}

#[derive(Clone)]
struct NexradFrameCandidate {
    version: String,
    manifest: serde_json::Value,
    observed_at_utc: Option<DateTime<Utc>>,
}

fn nexrad_frame_candidates(session: &UiSession) -> Vec<NexradFrameCandidate> {
    let mut frames = Vec::new();
    let mut identities = HashSet::new();
    if let Some(installed) = &session.nexrad_installed {
        frames.push(NexradFrameCandidate {
            version: installed.version.clone(),
            observed_at_utc: json_observed_at_utc(&installed.manifest),
            manifest: installed.manifest.clone(),
        });
    } else {
        for loaded in session.live_feeds.product_loaded_state_manifests("nexrad") {
            let identity = nexrad_manifest_identity(loaded.version, loaded.manifest);
            if identities.insert(identity) {
                frames.push(NexradFrameCandidate {
                    version: loaded.version.to_string(),
                    observed_at_utc: json_observed_at_utc(loaded.manifest),
                    manifest: loaded.manifest.clone(),
                });
            }
        }
    }
    frames.sort_by(|left, right| {
        left.observed_at_utc
            .cmp(&right.observed_at_utc)
            .then_with(|| left.version.cmp(&right.version))
    });
    if frames.len() > NEXRAD_ANIMATION_MAX_FRAMES {
        frames.drain(0..frames.len() - NEXRAD_ANIMATION_MAX_FRAMES);
    }
    frames
}

fn nexrad_manifest_identity(version: &str, manifest: &serde_json::Value) -> String {
    manifest
        .get("state_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(version)
        .to_string()
}

fn nexrad_animation_for_frames(
    frames: &[NexradFrameCandidate],
    epoch_ms: i64,
) -> NexradOverlayAnimation {
    if frames.is_empty() {
        return NexradOverlayAnimation::idle();
    }
    let age_labels = nexrad_frame_age_labels(frames, epoch_ms);
    let age_summary = if age_labels.is_empty() {
        "---".to_string()
    } else {
        age_labels.join(", ")
    };
    if frames.len() == 1 {
        return NexradOverlayAnimation {
            phase: NexradOverlayAnimationPhase::Frame,
            selected_frame_index: Some(0),
            frame_count: 1,
            age_labels,
            age_summary,
            next_update_delay_ms: None,
            next_update_epoch_ms: None,
        };
    }
    let cycle_ms = nexrad_animation_cycle_ms(frames.len());
    let offset_ms = epoch_ms.rem_euclid(cycle_ms);
    let mut phase_start_ms = 0;
    for index in 0..frames.len() {
        let dwell_ms = nexrad_animation_frame_dwell_ms(index, frames.len());
        let phase_end_ms = phase_start_ms + dwell_ms;
        if offset_ms < phase_end_ms {
            return NexradOverlayAnimation {
                phase: NexradOverlayAnimationPhase::Frame,
                selected_frame_index: Some(index),
                frame_count: frames.len(),
                age_labels,
                age_summary,
                next_update_delay_ms: Some((phase_end_ms - offset_ms) as u32),
                next_update_epoch_ms: Some(epoch_ms + (phase_end_ms - offset_ms)),
            };
        }
        phase_start_ms = phase_end_ms;
    }
    NexradOverlayAnimation {
        phase: NexradOverlayAnimationPhase::Blank,
        selected_frame_index: None,
        frame_count: frames.len(),
        age_labels,
        age_summary,
        next_update_delay_ms: Some((cycle_ms - offset_ms) as u32),
        next_update_epoch_ms: Some(epoch_ms + (cycle_ms - offset_ms)),
    }
}

fn nexrad_animation_cycle_ms(frame_count: usize) -> i64 {
    if frame_count <= 1 {
        return 0;
    }
    (frame_count.saturating_sub(1) as i64 * NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS)
        + NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS
        + NEXRAD_ANIMATION_BLANK_DWELL_MS
}

fn nexrad_animation_frame_dwell_ms(index: usize, frame_count: usize) -> i64 {
    if index + 1 == frame_count {
        NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS
    } else {
        NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS
    }
}

fn nexrad_frame_age_labels(frames: &[NexradFrameCandidate], epoch_ms: i64) -> Vec<String> {
    nexrad_frame_age_values(frames, epoch_ms)
        .into_iter()
        .map(|value| {
            if value == "unknown" {
                value
            } else {
                format!("{value} ago")
            }
        })
        .collect()
}

fn nexrad_frame_age_values(frames: &[NexradFrameCandidate], epoch_ms: i64) -> Vec<String> {
    frames
        .iter()
        .map(|frame| match frame.observed_at_utc {
            Some(observed_at_utc) => {
                format_age(epoch_ms.saturating_sub(observed_at_utc.timestamp_millis()))
            }
            None => "unknown".to_string(),
        })
        .collect()
}

pub fn nexrad_tile_bytes_in_session(handle: u32, src: &str) -> AppResult<Vec<u8>> {
    let sessions = lock_sessions();
    let session = session_ref(&sessions, handle)?;
    if let Some(installed) = &session.nexrad_installed {
        let member_path = nexrad_installed_member_path(src).ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "NEXRAD tile URL {src} is not inside installed package {}",
                installed.version
            ),
        })?;
        if let Some(bytes) = installed.members.get(&member_path) {
            return Ok(bytes.clone());
        }
    }
    session
        .nexrad_tile_cache
        .get(src)
        .cloned()
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("NEXRAD tile bytes are not loaded for {src}"),
        })
}

fn nexrad_installed_member_path(src: &str) -> Option<String> {
    let src = src.trim_start_matches('/');
    let (_, rest) = src.split_once("/tiles/")?;
    // The installed package is rooted at the state directory, while web URLs include
    // live-feeds/v3/states/nexrad/<state-id>/.
    Some(format!("tiles/{rest}"))
}

const NEXRAD_TILE_RESOURCE_PREFIX: &str = "live_feeds/nexrad_tile/";

pub fn prepare_nexrad_tile_in_session(handle: u32, src: &str) -> AppResult<HadOperationOutcome> {
    {
        let sessions = lock_sessions();
        let session = session_ref(&sessions, handle)?;
        if nexrad_tile_bytes_loaded(session, src)? {
            return Ok(HadOperationOutcome::complete(serde_json::Value::Null));
        }
    }
    let resource = nexrad_tile_resource_request(src)?;
    Ok(HadOperationOutcome::NeedResources {
        resources: vec![resource],
    })
}

fn nexrad_tile_bytes_loaded(session: &UiSession, src: &str) -> AppResult<bool> {
    if let Some(installed) = &session.nexrad_installed {
        let member_path = nexrad_installed_member_path(src).ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "NEXRAD tile URL {src} is not inside installed package {}",
                installed.version
            ),
        })?;
        if installed.members.contains_key(&member_path) {
            return Ok(true);
        }
    }
    Ok(session.nexrad_tile_cache.contains_key(src))
}

fn nexrad_tile_resource_request(src: &str) -> AppResult<CoreResourceRequest> {
    let resource_id = nexrad_tile_resource_id(src)?;
    Ok(CoreResourceRequest::public_url(resource_id, src, false))
}

fn nexrad_tile_resource_id(src: &str) -> AppResult<String> {
    let src = normalize_nexrad_tile_src(src)?;
    let prefix = format!(
        "{}/states/nexrad/",
        LIVE_FEEDS_BASE_PATH.trim_start_matches('/')
    );
    let suffix = src
        .trim_start_matches('/')
        .strip_prefix(&prefix)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("NEXRAD tile URL {src} is not a live-feed NEXRAD state tile"),
        })?;
    Ok(format!("{NEXRAD_TILE_RESOURCE_PREFIX}{suffix}"))
}

fn nexrad_tile_src_from_resource_id(resource_id: &str) -> Option<String> {
    let suffix = resource_id.strip_prefix(NEXRAD_TILE_RESOURCE_PREFIX)?;
    let src = format!("{LIVE_FEEDS_BASE_PATH}/states/nexrad/{suffix}");
    normalize_nexrad_tile_src(&src).ok()
}

fn normalize_nexrad_tile_src(src: &str) -> AppResult<String> {
    let trimmed = src.trim_start_matches('/');
    let prefix = format!(
        "{}/states/nexrad/",
        LIVE_FEEDS_BASE_PATH.trim_start_matches('/')
    );
    let Some(rest) = trimmed.strip_prefix(&prefix) else {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("NEXRAD tile URL {src} is not a live-feed NEXRAD state tile"),
        });
    };
    if !rest.contains("/tiles/") {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("NEXRAD tile URL {src} is missing a tiles/ member path"),
        });
    }
    Ok(format!("/{trimmed}"))
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
            animation: NexradOverlayAnimation::idle(),
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
            animation: NexradOverlayAnimation::idle(),
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
            animation: NexradOverlayAnimation::idle(),
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
            animation: NexradOverlayAnimation::idle(),
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
            animation: NexradOverlayAnimation::idle(),
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
                "{LIVE_FEEDS_BASE_PATH}/states/nexrad/{}/tiles/res{}/{}/{}.png",
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
        animation: NexradOverlayAnimation::idle(),
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

fn read_chart_asset_by_id(
    store: &NavKvStore,
    chart_id: &str,
) -> Result<ChartAssetRecord, HadReadError> {
    let key = nav_kv_key_for_query(&NavKvQuery::PlateById {
        plate_id: chart_id.to_string(),
    })
    .ok_or_else(|| HadReadError::Fatal("invalid chart asset id query".to_string()))?;
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Err(HadReadError::Fatal(format!(
            "HAD missing required chart asset key: {key}"
        ))),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
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
    if let Some(geometries) = self_contained_guidance_leg_geometry_for_plan(&normalized_plan)? {
        install_guidance_leg_geometry(session, geometries)?;
    } else {
        session.guidance_leg_geometry.clear();
    }
    session.chart_page_state = derive_compact_chart_page_state_with_reference(
        &normalized_plan,
        &session.chart_page_state.recent_airport_ids,
        session.chart_page_state.plate_target_airport_id.as_deref(),
        Some(&session.chart_page_state.selected_airport_id),
        session
            .chart_page_state
            .selected_reference_family_id
            .as_deref(),
        Some(&session.chart_page_state.selected_chart_id),
        &session.chart_page_state.suggested_chart_ids,
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
        Ok(()) => {
            advance_session_revision(&mut candidate);
            match try_snapshot_for_session(&mut candidate) {
                Ok(snapshot) => {
                    *session = candidate;
                    let snapshot = serde_json::to_value(snapshot).map_err(|err| AppError {
                        kind: AppErrorKind::Internal,
                        message: err.to_string(),
                    })?;
                    Ok(HadOperationOutcome::complete_with_invalidations(
                        snapshot,
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
            }
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

fn advance_session_revision(session: &mut UiSession) {
    session.session_revision = session.session_revision.saturating_add(1);
}

fn session_snapshot_outcome(session: &mut UiSession) -> AppResult<HadOperationOutcome> {
    session_snapshot_outcome_with_invalidations(session, Vec::new())
}

// Layer commands do not change the snapshot's HAD read set. Fault every required page
// before mutating so NeedResources remains side-effect free across platform retries.
fn preflight_session_snapshot_resources(
    session: &mut UiSession,
) -> AppResult<Option<HadOperationOutcome>> {
    match try_snapshot_for_session(session) {
        Ok(_) => Ok(None),
        Err(HadReadError::NeedPages(pages)) => Ok(Some(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        })),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn changed_session_snapshot_outcome(session: &mut UiSession) -> AppResult<HadOperationOutcome> {
    changed_session_snapshot_outcome_with_invalidations(session, Vec::new())
}

fn changed_session_snapshot_outcome_with_invalidations(
    session: &mut UiSession,
    mut invalidations: Vec<UiInvalidation>,
) -> AppResult<HadOperationOutcome> {
    advance_session_revision(session);
    dedupe_invalidations(&mut invalidations);
    match try_snapshot_for_session(session) {
        Ok(snapshot) => serde_json::to_value(snapshot)
            .map(|snapshot| {
                if invalidations.is_empty() {
                    HadOperationOutcome::complete(snapshot)
                } else {
                    HadOperationOutcome::complete_with_invalidations(snapshot, invalidations)
                }
            })
            .map_err(|err| AppError {
                kind: AppErrorKind::Internal,
                message: err.to_string(),
            }),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedSnapshotResources {
            resources: nav_kv_page_resources(pages),
            invalidations,
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn session_snapshot_outcome_with_invalidations(
    session: &mut UiSession,
    mut invalidations: Vec<UiInvalidation>,
) -> AppResult<HadOperationOutcome> {
    dedupe_invalidations(&mut invalidations);
    match try_snapshot_for_session(session) {
        Ok(snapshot) => serde_json::to_value(snapshot)
            .map(|snapshot| {
                if invalidations.is_empty() {
                    HadOperationOutcome::complete(snapshot)
                } else {
                    HadOperationOutcome::complete_with_invalidations(snapshot, invalidations)
                }
            })
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

fn try_snapshot_for_session(session: &mut UiSession) -> Result<UiSessionSnapshot, HadReadError> {
    let total_started_at = crate::core_clock_ms();
    let app_ui_started_at = crate::core_clock_ms();
    let mut app_ui_state = project_session_app_ui_state(session)?;
    let app_ui_ms = elapsed_ms(app_ui_started_at);
    let debug_started_at = crate::core_clock_ms();
    let debug_state = session.debug_state.clone();
    let debug_ms = elapsed_ms(debug_started_at);
    let playback_panel_started_at = crate::core_clock_ms();
    let playback_panel_state = playback_panel_state_for_app_state(&session.app_state);
    let playback_panel_ms = elapsed_ms(playback_panel_started_at);
    let app_state_started_at = crate::core_clock_ms();
    let app_state = state::project_ui_snapshot_app_state(&session.app_state);
    let app_state_ms = elapsed_ms(app_state_started_at);
    let playback_started_at = crate::core_clock_ms();
    let playback_ui_state = session.playback.ui_state();
    let playback_ms = elapsed_ms(playback_started_at);
    let map_follow_started_at = crate::core_clock_ms();
    let (map_follow_ui_state, map_follow_target_viewport) = session
        .map_follow
        .snapshot_projection(&session.app_state.ownship.render);
    let map_follow_ms = elapsed_ms(map_follow_started_at);
    let clone_started_at = crate::core_clock_ms();
    let chart_page_state = session.chart_page_state.clone();
    let map_layer_state = session.map_layer_state.clone();
    let data_status_state = session.data_status_state.clone();
    let data_status_page_state = project_data_status_page_state(session);
    let settings_page_state =
        project_settings_page_state(session, &app_ui_state.flight_data_banner);
    let home_page_state = project_home_page_state(&session.platform_capabilities);
    app_ui_state.flight_data_banner.cells.retain(|cell| {
        !session
            .settings_preferences
            .disabled_flight_data_cell_ids
            .contains(&cell.id)
    });
    let display_policy = project_display_policy(session);
    let clone_ms = elapsed_ms(clone_started_at);
    let raster_started_at = crate::core_clock_ms();
    let raster_map = session
        .raster_map_catalog
        .as_ref()
        .and_then(crate::raster_map_ui_state);
    let raster_ms = elapsed_ms(raster_started_at);
    let total_ms = elapsed_ms(total_started_at);
    crate::core_perf_debug_log("session.snapshot.core", || {
        serde_json::json!({
            "total_ms": total_ms,
            "app_ui_ms": app_ui_ms,
            "debug_ms": debug_ms,
            "playback_panel_ms": playback_panel_ms,
            "app_state_ms": app_state_ms,
            "playback_ms": playback_ms,
            "map_follow_ms": map_follow_ms,
            "clone_ms": clone_ms,
            "raster_ms": raster_ms,
            "status_boxes": data_status_state.boxes.len(),
            "status_page_rows": data_status_page_state.rows.len(),
            "settings_page_rows": settings_page_state.rows.len(),
            "map_families": raster_map.as_ref().map(|state| state.family_options.len()).unwrap_or(0),
        })
    });
    Ok(UiSessionSnapshot {
        session_revision: session.session_revision,
        nav_data_epoch: session.nav_data_epoch,
        active_nav_db: session.nav_db_artifact.as_ref().map(UiNavDbIdentity::from),
        next_nav_db_maintenance_epoch_ms: next_nav_db_maintenance_epoch_ms(session),
        app_state,
        app_ui_state,
        playback_ui_state,
        playback_panel_state,
        map_follow_ui_state,
        map_follow_target_viewport,
        chart_page_state,
        map_layer_state,
        data_status_state,
        data_status_page_state,
        settings_page_state,
        home_page_state,
        display_policy,
        disclaimer_state: project_disclaimer_state(&session.settings_preferences),
        debug_state,
        raster_map,
        next_cycle_product_freshness_check_epoch_ms: session
            .cycle_product_freshness
            .next_check_epoch_ms,
    })
}

fn playback_panel_state_for_app_state(app_state: &AppState) -> UiPlaybackPanelState {
    UiPlaybackPanelState {
        visible: situation_source_handler_for_ownship(&app_state.ownship).is_replay(),
    }
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

fn register_bad_autopilot_source(app_state: AppState) -> AppResult<AppState> {
    let app_state = state::reduce(
        &app_state,
        AppEvent::RegisterOwnshipSource(crate::OwnshipSourceRegistration {
            source_id: crate::OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            source_kind: crate::OwnshipSourceKind::BadAutopilot,
            display_name: "Bad AP".to_string(),
            selectable: true,
            auto_eligible: true,
        }),
    )?;
    state::reduce(
        &app_state,
        AppEvent::UpdateOwnshipSourceStatus(crate::OwnshipSourceStatusUpdate {
            source_id: crate::OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
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
            disabled_reason: None,
        },
        vectors: UiMapLayerToggleState {
            visible: true,
            enabled: true,
            disabled_reason: None,
        },
        metars: UiMapLayerToggleState {
            visible: true,
            enabled: true,
            disabled_reason: None,
        },
        nexrad: UiMapLayerToggleState {
            visible: false,
            enabled: true,
            disabled_reason: None,
        },
        terrain_warning: UiMapLayerToggleState {
            visible: true,
            enabled: true,
            disabled_reason: None,
        },
        offline_regions: UiMapLayerToggleState {
            visible: false,
            enabled: true,
            disabled_reason: None,
        },
    }
}

fn map_layer_disabled_reason(layer_id: MapLayerId) -> &'static str {
    match layer_id {
        MapLayerId::WorldBasemap => "The world map layer is unavailable.",
        MapLayerId::Vectors => "The vector layer is unavailable.",
        MapLayerId::Metars => "Weather observations are unavailable.",
        MapLayerId::Nexrad => "NEXRAD is unavailable.",
        MapLayerId::TerrainWarning => "Terrain warning is unavailable.",
        MapLayerId::OfflineRegions => "Offline package regions are unavailable.",
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
        title: "Status".to_string(),
        summary: "Status will appear after core session data loads.".to_string(),
        rows: Vec::new(),
    }
}

fn default_settings_page_state() -> UiSettingsPageState {
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: String::new(),
        rows: Vec::new(),
    }
}

fn project_home_page_state(capabilities: &PlatformCapabilities) -> UiHomePageState {
    let offline_packages_enabled = capabilities.offline_packages.is_some();
    let button = |id: &str, label: &str| UiHomePageButton {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        disabled_reason: None,
    };
    UiHomePageState {
        buttons: vec![
            button("chart", "CHART"),
            button("plate", "PLATE"),
            button("flight-plan", "FLIGHT\nPLAN"),
            button("data-status", "STATUS"),
            button("settings", "SETTINGS"),
            UiHomePageButton {
                id: "offline-packages".to_string(),
                label: "OFFLINE\nPACKAGES".to_string(),
                enabled: offline_packages_enabled,
                disabled_reason: (!offline_packages_enabled).then(|| {
                    "This client fetches data as needed and does not support managing Offline Packages."
                        .to_string()
                }),
            },
            button("about", "ABOUT"),
        ],
    }
}

fn project_settings_page_state(
    session: &UiSession,
    flight_data_banner: &crate::FlightDataBannerModel,
) -> UiSettingsPageState {
    let mut rows = vec![UiSettingsPageRow {
        kind: "grid_choices".to_string(),
        id: FLIGHT_DATA_VISIBILITY_ROW_ID.to_string(),
        title: "Flight data grid".to_string(),
        value_id: String::new(),
        stops: Vec::new(),
        items: flight_data_banner
            .cells
            .iter()
            .map(|cell| UiSettingsGridItem {
                cell: cell.clone(),
                enabled: !session
                    .settings_preferences
                    .disabled_flight_data_cell_ids
                    .contains(&cell.id),
            })
            .collect(),
        action_id: FLIGHT_DATA_VISIBILITY_ACTION_ID.to_string(),
    }];
    if session.platform_capabilities.display_policy.is_some() {
        rows.push(UiSettingsPageRow {
            kind: "slider".to_string(),
            id: DISPLAY_DIM_TIMEOUT_ROW_ID.to_string(),
            title: "\u{1F50B} Display dims after...".to_string(),
            value_id: session
                .settings_preferences
                .display_dim_timeout
                .id()
                .to_string(),
            stops: DisplayDimTimeout::all_stops()
                .into_iter()
                .map(|timeout| UiSettingsSliderStop {
                    id: timeout.id().to_string(),
                    label: timeout.label().to_string(),
                })
                .collect(),
            items: Vec::new(),
            action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
        });
    }
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: if rows.is_empty() {
            "No platform settings are available.".to_string()
        } else {
            String::new()
        },
        rows,
    }
}

fn project_display_policy(session: &UiSession) -> Option<UiDisplayPolicy> {
    session
        .platform_capabilities
        .display_policy
        .as_ref()
        .map(|_| UiDisplayPolicy {
            keep_screen_on: true,
            dim_after_ms: session
                .settings_preferences
                .display_dim_timeout
                .dim_after_ms(),
            dim_brightness: DISPLAY_DIM_BRIGHTNESS,
        })
}

fn project_disclaimer_state(preferences: &SettingsPreferences) -> UiDisclaimerState {
    UiDisclaimerState {
        agreement_id: NO_WARRANTY_DISCLAIMER_AGREEMENT_ID.to_string(),
        required: !preferences
            .accepted_disclaimer_agreement_ids
            .contains(NO_WARRANTY_DISCLAIMER_AGREEMENT_ID),
        html: NO_WARRANTY_DISCLAIMER_HTML.to_string(),
        text: no_warranty_disclaimer_text(),
        accept_label: "I understand and agree".to_string(),
    }
}

fn no_warranty_disclaimer_text() -> String {
    let stripped = NO_WARRANTY_DISCLAIMER_HTML
        .replace("<p>", "")
        .replace("</p>", "")
        .replace("<strong>", "")
        .replace("</strong>", "");
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn load_settings_preferences_from_storage(session: &mut UiSession) -> AppResult<()> {
    let Some(storage) = session.settings_storage.as_ref() else {
        return Ok(());
    };
    let Some(bytes) = storage.read_settings()? else {
        return Ok(());
    };
    session.settings_preferences = decode_settings_preferences(&bytes);
    Ok(())
}

fn write_settings_preferences_to_storage(session: &UiSession) -> AppResult<()> {
    let Some(storage) = session.settings_storage.as_ref() else {
        return Ok(());
    };
    let bytes = encode_settings_preferences(&session.settings_preferences)?;
    storage.write_settings(&bytes)
}

fn decode_settings_preferences(bytes: &[u8]) -> SettingsPreferences {
    if bytes.is_empty() {
        return SettingsPreferences::default();
    }
    serde_json::from_slice::<SettingsPersistenceDocument>(bytes)
        .ok()
        .filter(|document| document.version == SETTINGS_PERSISTENCE_VERSION)
        .map(|document| document.preferences)
        .unwrap_or_default()
}

fn encode_settings_preferences(preferences: &SettingsPreferences) -> AppResult<Vec<u8>> {
    serde_json::to_vec(&SettingsPersistenceDocument {
        version: SETTINGS_PERSISTENCE_VERSION,
        preferences: preferences.clone(),
    })
    .map_err(|err| AppError {
        kind: AppErrorKind::Internal,
        message: err.to_string(),
    })
}

fn default_debug_state() -> UiDebugState {
    UiDebugState {
        tile_labels: false,
        nexrad_tile_labels: false,
        fast_tiles: false,
        offline_simulated_clock_buttons: false,
        sequencing_finish_lines: false,
        bad_autopilot: false,
        gps_capture: false,
        debug_log_to_developer_server: false,
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
    project_bad_autopilot_availability(session, &mut app_ui_state);
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
            crate::FlightDataComputer::with_clock(
                app_ui_state.ownship.render.speed_kt,
                Some(session.wall_clock_epoch_ms),
            ),
            crate::had_ops::FlightPlanLiveData {
                ownship_position: app_ui_state.ownship.render.position,
                now_epoch_ms: Some(session.wall_clock_epoch_ms),
            },
        )?);
    }
    if let Some(active_plan) = app_ui_state.active_plan.as_mut() {
        enrich_flight_plan_weather_actions(session, active_plan);
    }
    app_ui_state.flight_data_banner = project_flight_data_banner(session, &app_ui_state)?;
    Ok(app_ui_state)
}

fn enrich_flight_plan_weather_actions(session: &UiSession, active_plan: &mut FlightPlanUiState) {
    for row in &mut active_plan.display_rows {
        let station_id = row
            .chart_airport_id
            .as_deref()
            .map(crate::map_overlay::weather_station_id_for_airport_id)
            .or_else(|| match row.nav_ref.as_ref() {
                Some(NavRef::Airport(airport_id)) => Some(
                    crate::map_overlay::weather_station_id_for_airport_id(airport_id),
                ),
                _ => None,
            });
        let weather_detail = station_id.as_deref().and_then(|station_id| {
            crate::map_overlay::weather_detail_for_station(
                station_id,
                session.metar_payload.as_ref(),
                session.taf_payload.as_ref(),
                session.airport_notam_index.as_ref(),
                Some(session_wall_clock_utc(session)),
            )
        });
        for action in crate::planning::flight_plan_row_actions_mut(row) {
            if action.id == FlightPlanRowActionId::Weather {
                action.enabled = weather_detail.is_some();
                action.weather_detail = weather_detail.clone();
            }
        }
    }
}

fn project_flight_data_banner(
    session: &UiSession,
    app_ui_state: &AppUiState,
) -> Result<crate::FlightDataBannerModel, HadReadError> {
    let ownship = &app_ui_state.ownship.render;
    let position = ownship.position;
    let store = session.nav_kv_store.as_ref();
    let flight_data_computer =
        crate::FlightDataComputer::with_clock(ownship.speed_kt, Some(session.wall_clock_epoch_ms));

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

    if let Some(plan) = session.app_state.active_plan.as_ref() {
        if let Some(geometry) = active_guidance_projection(plan, &session.guidance_leg_geometry)
            .and_then(|projection| projection.geometry)
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

    let banner = flight_data_computer.banner(crate::FlightDataBannerInput {
        altitude_ft,
        vertical_speed_fpm: session
            .app_state
            .ownship
            .resolved
            .kinematics
            .as_ref()
            .and_then(|kinematics| kinematics.vertical_speed_fpm),
        track_magnetic_deg,
        desired_track_magnetic_deg,
        waypoint_distance_nm,
        final_distance_nm,
        nexrad_age: Some(nexrad_frame_age_banner_value(session)),
    });
    Ok(banner)
}

fn project_bad_autopilot_availability(session: &UiSession, app_ui_state: &mut AppUiState) {
    project_bad_autopilot_availability_for_state(
        &session.debug_state,
        bad_autopilot_available(session),
        app_ui_state,
    );
}

fn project_bad_autopilot_availability_for_state(
    debug_state: &UiDebugState,
    available: bool,
    app_ui_state: &mut AppUiState,
) {
    if !debug_state.bad_autopilot {
        app_ui_state
            .ownship
            .controls
            .sources
            .retain(|source| source.source_kind != crate::OwnshipSourceKind::BadAutopilot);
        return;
    }
    for source in &mut app_ui_state.ownship.controls.sources {
        if source.source_kind == crate::OwnshipSourceKind::BadAutopilot {
            source.enabled = available;
            if !available {
                source.tone = crate::ownship::OwnshipControlTone::Unavailable;
                source.status_label = "No active leg".to_string();
                source.disabled_reason = Some("Bad AP requires an active leg.".to_string());
            }
        }
    }
}

fn bad_autopilot_selectable(session: &UiSession) -> bool {
    session.debug_state.bad_autopilot && bad_autopilot_available(session)
}

fn bad_autopilot_available(session: &UiSession) -> bool {
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

    fn input_disabled_reason(
        &self,
        _session: &UiSession,
        _input: SituationControlInput,
    ) -> Option<String> {
        Some(
            "Replay and plan preview controls are not available for this ownship source."
                .to_string(),
        )
    }

    fn menu_items(&self, session: &UiSession) -> Vec<SituationControlMenuItem> {
        [
            (SituationControlInput::SkipBackward, "⏮"),
            (SituationControlInput::FastRewind, "⏪"),
            (SituationControlInput::FastForward, "⏩"),
            (SituationControlInput::SkipForward, "⏭"),
        ]
        .into_iter()
        .map(|(input, label)| {
            let enabled = self.input_enabled(session, input);
            SituationControlMenuItem {
                input,
                label: label.to_string(),
                enabled,
                disabled_reason: (!enabled)
                    .then(|| self.input_disabled_reason(session, input))
                    .flatten(),
            }
        })
        .collect()
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
        if let Some(playback_state) = session.playback.jog(delta_seconds, now_epoch_ms) {
            apply_playback_state_to_ownship(session, playback_state, now_epoch_ms as i64)?;
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

    fn input_disabled_reason(
        &self,
        session: &UiSession,
        input: SituationControlInput,
    ) -> Option<String> {
        let ui_state = session.playback.ui_state();
        if ui_state.duration_seconds <= 1e-6 {
            return Some("Load a trace before replaying.".to_string());
        }
        match input {
            SituationControlInput::SkipBackward | SituationControlInput::FastRewind => {
                Some("Already at the start of replay.".to_string())
            }
            SituationControlInput::FastForward | SituationControlInput::SkipForward => {
                Some("Already at the end of replay.".to_string())
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

    fn input_disabled_reason(
        &self,
        session: &UiSession,
        input: SituationControlInput,
    ) -> Option<String> {
        let records = session
            .app_state
            .active_plan
            .as_ref()
            .map(|plan| plan_preview_legs(plan, &session.guidance_leg_geometry))
            .unwrap_or_default();
        if records.is_empty() {
            return Some("No plan preview route is available.".to_string());
        }
        match input {
            SituationControlInput::SkipBackward | SituationControlInput::FastRewind => {
                Some("Already at the start of plan preview.".to_string())
            }
            SituationControlInput::FastForward | SituationControlInput::SkipForward => {
                Some("Already at the end of plan preview.".to_string())
            }
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
    let Some(projection) = active_guidance_projection(plan, &session.guidance_leg_geometry) else {
        return Ok(NavElementUiView::default());
    };
    projection.nav_element(session.app_state.ownship.render.position, store)
}

#[derive(Debug, Clone)]
struct ActiveGuidanceProjection {
    summary: String,
    geometry: Option<GuidanceLegGeometry>,
}

impl ActiveGuidanceProjection {
    fn nav_element(
        &self,
        position: Option<LatLon>,
        store: Option<&NavKvStore>,
    ) -> Result<NavElementUiView, HadReadError> {
        let Some(geometry) = self.geometry.as_ref() else {
            return Ok(NavElementUiView {
                active_leg_summary: self.summary.clone(),
                cdi_indicator_dots: None,
                cdi_offscale_readout: None,
            });
        };
        let course_deg = active_display_course_deg(geometry, position, store)?;
        let cdi_indicator_dots =
            position.map(|position| cdi_dots_for_guidance_geometry(geometry, position));
        let cdi_offscale_readout = cdi_indicator_dots.and_then(cdi_offscale_readout);
        let active_leg_summary = if let Some(course_deg) = course_deg {
            format!(
                "{} CRS {}",
                self.summary,
                crate::flight_data::format_course_degrees(course_deg)
            )
        } else {
            self.summary.clone()
        };

        Ok(NavElementUiView {
            active_leg_summary,
            cdi_indicator_dots,
            cdi_offscale_readout,
        })
    }
}

fn active_guidance_projection(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<ActiveGuidanceProjection> {
    let leg = crate::active_guidance_leg(plan)?;
    let geometry = active_guidance_projection_geometry(plan, geometry_by_leg_id);
    let summary = if active_guidance_detail_is_terminal_hold(plan) {
        "HOLD".to_string()
    } else {
        format!("{} -> {}", nav_ref_label(&leg.from), nav_ref_label(&leg.to))
    };
    Some(ActiveGuidanceProjection { summary, geometry })
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

fn active_guidance_projection_geometry(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        return geometry_by_leg_id.get("direct-to").cloned();
    }
    guidance
        .active_detail_index
        .and_then(|detail_index| {
            active_guidance_detail_geometry_for_index(plan, detail_index, geometry_by_leg_id)
        })
        .map(|(_, geometry)| geometry)
}

#[derive(Debug, Clone)]
struct PlanPreviewLeg {
    resolved_leg_index: usize,
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
        .into_iter()
        .find(|record| record.resolved_leg_index == leg_index)
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
    .map(|_| ())
}

fn tick_bad_autopilot(
    session: &mut UiSession,
    now_epoch_ms: f64,
) -> AppResult<Option<OwnshipMotionResult>> {
    if !session.debug_state.bad_autopilot {
        return Ok(None);
    }
    if selected_ownship_source_kind(&session.app_state.ownship)
        != Some(crate::OwnshipSourceKind::BadAutopilot)
    {
        return Ok(None);
    }
    session.bad_autopilot.running = true;

    let Some((detail_id, geometry)) = active_guidance_detail_geometry(session) else {
        session.bad_autopilot.last_tick_epoch_ms = Some(now_epoch_ms);
        return Ok(None);
    };
    let distance_nm = geometry_distance_nm(&geometry);
    if distance_nm <= f64::EPSILON {
        session.bad_autopilot.last_tick_epoch_ms = Some(now_epoch_ms);
        return Ok(None);
    }

    let dt_seconds = session
        .bad_autopilot
        .last_tick_epoch_ms
        .map(|last_tick| {
            ((now_epoch_ms - last_tick) / 1000.0).clamp(0.0, BAD_AUTOPILOT_MAX_DT_SECONDS)
        })
        .unwrap_or(0.0);
    session.bad_autopilot.last_tick_epoch_ms = Some(now_epoch_ms);

    if session.bad_autopilot.active_detail_id.as_deref() != Some(detail_id.as_str()) {
        session.bad_autopilot.active_detail_id = Some(detail_id);
        session.bad_autopilot.offset_nm = 0.0;
    }

    session.bad_autopilot.offset_nm = (session.bad_autopilot.offset_nm
        + dt_seconds * BAD_AUTOPILOT_NM_PER_SECOND)
        .min(distance_nm + BAD_AUTOPILOT_OVERRUN_NM);
    session.bad_autopilot.wander_phase_rad += dt_seconds * 0.7;

    let offset_nm = session.bad_autopilot.offset_nm;
    let heading = heading_along_geometry(&geometry, offset_nm)
        .unwrap_or_else(|| bearing_degrees(geometry.from, geometry.to));
    let base_position = position_along_geometry_with_overrun(&geometry, offset_nm);
    let wander_nm = BAD_AUTOPILOT_WANDER_NM * session.bad_autopilot.wander_phase_rad.sin();
    let position = project_nm_from(base_position, heading + 90.0, wander_nm);
    let motion_heading = session
        .bad_autopilot
        .last_position
        .filter(|last_position| crate::great_circle_distance_nm(*last_position, position) > 1e-4)
        .map(|last_position| bearing_degrees(last_position, position))
        .unwrap_or(heading);
    session.bad_autopilot.last_position = Some(position);

    apply_situation_to_ownship(
        session,
        BAD_AUTOPILOT_SOURCE_ID,
        crate::OwnshipSourceKind::BadAutopilot,
        "Bad AP",
        crate::Situation {
            position: crate::SituationPosition::LatLon {
                lat: position.lat,
                lon: position.lon,
            },
            orientation_deg: Some(motion_heading),
            speed_kt: Some(
                BAD_AUTOPILOT_NM_PER_SECOND * 3600.0 * BAD_AUTOPILOT_REPORTED_SPEED_SCALE,
            ),
            altitude_msl_ft: None,
        },
        now_epoch_ms as i64,
    )
    .map(Some)
}

fn active_guidance_detail_geometry(session: &UiSession) -> Option<(String, GuidanceLegGeometry)> {
    let plan = session.app_state.active_plan.as_ref()?;
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        let geometry = active_guidance_projection(plan, &session.guidance_leg_geometry)?.geometry?;
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
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        let detail_count = crate::guidance_detail_count_for_leg(leg);
        if active_detail_index < current_index + detail_count {
            let element_index = active_detail_index - current_index;
            let detail_id = guidance_detail_id_for_leg_element(leg_index, leg, element_index);
            let geometry = geometry_by_leg_id.get(&detail_id).cloned()?;
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
        }
    }
    plan.resolved_legs
        .iter()
        .enumerate()
        .filter_map(|(leg_index, leg)| {
            let pointer_key =
                pointer_key_for_preview_leg(plan, leg_index, leg, &component_leg_counts)?;
            let geometry = geometry_for_resolved_leg(leg_index, leg, geometry_by_leg_id)?;
            let distance_nm = geometry_distance_nm(&geometry);
            Some(PlanPreviewLeg {
                resolved_leg_index: leg_index,
                pointer_key,
                geometry,
                distance_nm,
            })
        })
        .collect()
}

fn pointer_key_for_preview_leg(
    plan: &FlightPlan,
    leg_index: usize,
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
                Some(format!(
                    "guidance-leg:{}:{}:{}",
                    preview_component_pointer_scope(plan, component_index),
                    leg_index,
                    leg.id
                ))
            } else {
                plan.route_component_uids.get(component_index).cloned()
            }
        }
    }
}

fn preview_component_pointer_scope(plan: &FlightPlan, component_index: usize) -> String {
    plan.route_component_uids
        .get(component_index)
        .map(|uid| format!("component:{uid}"))
        .unwrap_or_else(|| format!("component-index:{component_index}"))
}

fn geometry_for_resolved_leg(
    leg_index: usize,
    leg: &ResolvedLeg,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    let detail_count = crate::guidance_detail_count_for_leg(leg);
    if detail_count > 1 {
        let mut detail_geometries = Vec::with_capacity(detail_count);
        for element_index in 0..detail_count {
            detail_geometries.push(geometry_by_leg_id.get(&guidance_detail_id_for_leg_element(
                leg_index,
                leg,
                element_index,
            ))?);
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

    if let Some(geometry) =
        geometry_by_leg_id.get(&guidance_detail_id_for_leg_element(leg_index, leg, 0))
    {
        return Some(geometry.clone());
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
        NavRef::LatLon(_) | NavRef::Spot(_) => "SPOT".to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
struct OwnshipMotionResult {
    terrain_key_before: OwnshipTerrainRefreshKey,
    sequenced_guidance: bool,
}

fn register_manual_ownship_source(
    session: &mut UiSession,
    source_id: crate::OwnshipSourceId,
    source_kind: crate::OwnshipSourceKind,
    display_name: &str,
) -> AppResult<crate::OwnshipSourceId> {
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
    Ok(source_id)
}

fn apply_ownship_sample(
    session: &mut UiSession,
    sample: crate::SituationSample,
) -> AppResult<OwnshipMotionResult> {
    let terrain_key_before = ownship_terrain_refresh_key(session);
    maybe_log_gps_capture_sample(session, &sample);
    advance_session_wall_clock(session, sample.received_time_epoch_ms);
    session.app_state = state::reduce(&session.app_state, AppEvent::PushSituationSample(sample))?;
    let sequenced_guidance = sequence_guidance_by_ownship_position(session)?;
    Ok(OwnshipMotionResult {
        terrain_key_before,
        sequenced_guidance,
    })
}

fn ownship_motion_invalidations(
    session: &UiSession,
    motion: OwnshipMotionResult,
) -> Vec<UiInvalidation> {
    ownship_motion_invalidations_from(
        session,
        motion.terrain_key_before,
        motion.sequenced_guidance,
    )
}

fn ownship_motion_invalidations_from(
    session: &UiSession,
    terrain_key_before: OwnshipTerrainRefreshKey,
    sequenced_guidance: bool,
) -> Vec<UiInvalidation> {
    let mut invalidations = terrain_overlay_invalidations_for_ownship_change(
        terrain_key_before,
        ownship_terrain_refresh_key(session),
    );
    if sequenced_guidance {
        invalidations.extend([
            UiInvalidation::SessionSnapshot,
            UiInvalidation::FlightPlanRoute,
            UiInvalidation::MapOverlay,
        ]);
    }
    invalidations
}

fn changed_session_snapshot_outcome_for_ownship_motion(
    session: &mut UiSession,
    motion: Option<OwnshipMotionResult>,
) -> AppResult<HadOperationOutcome> {
    let invalidations = motion
        .map(|motion| ownship_motion_invalidations(session, motion))
        .unwrap_or_default();
    changed_session_snapshot_outcome_with_invalidations(session, invalidations)
}

fn apply_situation_to_ownship(
    session: &mut UiSession,
    source_id: &str,
    source_kind: crate::OwnshipSourceKind,
    display_name: &str,
    situation: crate::Situation,
    timestamp_epoch_ms: i64,
) -> AppResult<OwnshipMotionResult> {
    let source_id = register_manual_ownship_source(
        session,
        crate::OwnshipSourceId(source_id.to_string()),
        source_kind,
        display_name,
    )?;
    apply_ownship_sample(
        session,
        crate::SituationSample {
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
            vertical_speed_fpm: None,
        },
    )
}

fn apply_playback_state_to_ownship(
    session: &mut UiSession,
    playback_state: crate::playback::PlaybackOwnshipState,
    timestamp_epoch_ms: i64,
) -> AppResult<OwnshipMotionResult> {
    let source_id = register_manual_ownship_source(
        session,
        crate::OwnshipSourceId(PLAYBACK_SOURCE_ID.to_string()),
        playback_state.source_kind,
        &playback_state.display_name,
    )?;
    let (position, orientation_deg, speed_kt, altitude_msl_ft) = match playback_state.situation {
        Some(situation) => (
            situation.position.lat_lon(),
            situation.orientation_deg,
            situation.speed_kt,
            situation.altitude_msl_ft,
        ),
        None => (None, None, None, None),
    };
    let motion = apply_ownship_sample(
        session,
        crate::SituationSample {
            source_id: source_id.clone(),
            source_kind: playback_state.source_kind,
            event_time_epoch_ms: timestamp_epoch_ms,
            received_time_epoch_ms: timestamp_epoch_ms,
            position,
            horizontal_accuracy_m: None,
            vertical_accuracy_m: None,
            track_deg_true: orientation_deg,
            heading_deg_true: None,
            ground_speed_kt: speed_kt,
            altitude_msl_ft,
            pressure_altitude_ft: None,
            vertical_speed_fpm: None,
        },
    )?;
    session.app_state = state::reduce(
        &session.app_state,
        AppEvent::UpdateOwnshipSourceStatus(crate::OwnshipSourceStatusUpdate {
            source_id,
            connection_state: playback_state.connection_state,
            enabled: true,
            status_label: playback_state.status_label,
        }),
    )?;
    Ok(motion)
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

fn sequence_guidance_by_ownship_position(session: &mut UiSession) -> AppResult<bool> {
    let Some(position) = session.app_state.ownship.render.position else {
        return Ok(false);
    };
    let mut sequenced = false;
    for _ in 0..16 {
        let Some(plan) = session.app_state.active_plan.as_ref() else {
            return Ok(sequenced);
        };
        let Some(guidance) = plan.guidance.as_ref() else {
            return Ok(sequenced);
        };
        let (finish_criterion, suspended_hold) = if guidance.sequencing_mode
            == SequencingMode::DirectTo
        {
            let Some(finish_criterion) =
                direct_to_finish_criterion(plan, &session.guidance_leg_geometry)
            else {
                return Ok(sequenced);
            };
            (finish_criterion, false)
        } else {
            let Some(active_detail_index) = active_guidance_detail_index_for_motion(plan, guidance)
            else {
                return Ok(sequenced);
            };
            let suspended_hold = guidance.sequencing_mode == SequencingMode::Suspended;
            let Some(finish_criterion) = active_detail_finish_criterion(
                plan,
                active_detail_index,
                &session.guidance_leg_geometry,
                suspended_hold,
            ) else {
                return Ok(sequenced);
            };
            (finish_criterion, suspended_hold)
        };
        if !position_satisfies_finish_criterion(position, finish_criterion) {
            return Ok(sequenced);
        }
        let next_plan = if suspended_hold {
            sequence_suspended_terminal_hold_detail(plan)?
        } else {
            crate::sequence_active_detail(plan)?
        };
        session.app_state =
            state::reduce(&session.app_state, AppEvent::ReplaceFlightPlan(next_plan))?;
        sequenced = true;
    }
    Ok(sequenced)
}

fn direct_to_finish_criterion(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<SequencingFinishCriterion> {
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode != SequencingMode::DirectTo {
        return None;
    }
    let current = active_guidance_projection_geometry(plan, geometry_by_leg_id)?;
    let current_course = terminal_course_for_guidance_geometry(&current)?;
    let next_course = guidance
        .direct_to
        .as_ref()
        .and_then(|direct_to| direct_to.resume_leg_id.as_deref())
        .and_then(|resume_leg_id| {
            plan.resolved_legs
                .iter()
                .position(|leg| leg.id == resume_leg_id)
        })
        .and_then(|resume_leg_index| first_guidance_detail_index_for_leg(plan, resume_leg_index))
        .and_then(|detail_index| {
            active_guidance_detail_geometry_for_index(plan, detail_index, geometry_by_leg_id)
        })
        .and_then(|(_, geometry)| initial_course_for_guidance_geometry(&geometry))
        .unwrap_or(current_course);
    Some(SequencingFinishCriterion::Plane(SequencingFinishPlane {
        point: current.to,
        normal_course_deg: finish_line_normal_course_deg(current_course, next_course),
    }))
}

fn active_detail_finish_criterion(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
    wrap_terminal_hold: bool,
) -> Option<SequencingFinishCriterion> {
    let (_, current) =
        active_guidance_detail_geometry_for_index(plan, active_detail_index, geometry_by_leg_id)?;
    if let Some(arc_criterion) = active_detail_arc_finish_criterion(plan, active_detail_index) {
        return Some(arc_criterion);
    }
    let current_course = terminal_course_for_guidance_geometry(&current)?;
    let next_detail_index = if wrap_terminal_hold {
        next_terminal_hold_detail_index(plan, active_detail_index)
    } else {
        active_detail_index.checked_add(1)
    };
    let next_course = next_detail_index
        .and_then(|detail_index| {
            active_guidance_detail_geometry_for_index(plan, detail_index, geometry_by_leg_id)
        })
        .and_then(|(_, geometry)| initial_course_for_guidance_geometry(&geometry))
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
    plate_target_airport_id: Option<&str>,
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> UiChartPageState {
    derive_compact_chart_page_state_with_reference(
        plan,
        stored_recent_airport_ids,
        plate_target_airport_id,
        candidate_airport_id,
        None,
        candidate_chart_id,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn derive_compact_chart_page_state_with_reference(
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
    candidate_airport_id: Option<&str>,
    selected_reference_family_id: Option<&str>,
    candidate_chart_id: Option<&str>,
    suggested_chart_ids: &[String],
) -> UiChartPageState {
    let mut ordered_airport_ids = Vec::new();
    for airport_id in compact_chart_page_airport_candidates(
        plan,
        stored_recent_airport_ids,
        plate_target_airport_id,
        candidate_airport_id,
    ) {
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
    let selected_airport_id = normalize_compact_airport_id(candidate_airport_id)
        .filter(|airport_id| {
            ordered_airport_ids
                .iter()
                .any(|existing| existing == airport_id)
        })
        .or_else(|| recent_airport_ids.first().cloned())
        .or_else(|| ordered_airport_ids.first().cloned())
        .unwrap_or_default();
    let plate_target_airport_id =
        normalize_compact_airport_id(plate_target_airport_id).filter(|airport_id| {
            ordered_airport_ids
                .iter()
                .any(|existing| existing == airport_id)
        });
    UiChartPageState {
        ordered_airport_ids,
        recent_airport_ids,
        plate_target_airport_id,
        selected_airport_id,
        selected_reference_family_id: selected_reference_family_id.map(str::to_string),
        selected_chart_id: candidate_chart_id.unwrap_or_default().to_string(),
        suggested_chart_ids: suggested_chart_ids.to_vec(),
    }
}

fn compact_chart_page_airport_candidates(
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    plate_target_airport_id: Option<&str>,
    candidate_airport_id: Option<&str>,
) -> Vec<String> {
    let mut airport_ids = Vec::new();
    if let Some(plate_target_airport_id) = plate_target_airport_id
        .map(str::trim)
        .filter(|airport_id| !airport_id.is_empty())
    {
        airport_ids.push(plate_target_airport_id.to_ascii_uppercase());
    }
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
    airport_ids.extend(chart_page_airport_ids_from_plan(plan));

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

fn normalize_compact_airport_id(airport_id: Option<&str>) -> Option<String> {
    airport_id
        .map(str::trim)
        .filter(|airport_id| !airport_id.is_empty())
        .map(str::to_ascii_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        map_overlay::NotamProductPayload, AirportId, FlightPlan, GuidanceState, HadOperation,
        LegDisplayElement, LegDisplayPath, LegDisplayPathStyle, MapSelectionAction,
        MapSelectionCategory, MapSelectionHighlight, MapSelectionItem, MaterializedProcedure,
        NavRef, OwnshipSourceId, OwnshipSourceKind, PathTermination, PointVectorRecord,
        ProcedureLegProvenance, ProcedureSegmentRole, ResolvedLeg, ResolvedLegSource,
        RouteComponent, SequencingMode, Situation, SituationPosition, SituationSample,
        REQUIRED_NAV_DB_CONTRACT_ID,
    };

    #[test]
    fn restoring_chart_page_state_preserves_chart_reference_selection() {
        let init = create_ui_session(
            FlightPlan::default(),
            &["KPWT".to_string()],
            Some("KPWT"),
            None,
        )
        .expect("create session");
        let chart_id = "chart-reference:enr-l:legend:enr-l01";
        let suggested_chart_ids = vec!["chart-reference:enr-l:inset:test".to_string()];

        let snapshot = restore_chart_page_state_in_session(
            init.handle,
            &["KPWT".to_string()],
            None,
            Some("KPWT"),
            Some("enr-l"),
            Some(chart_id),
            &suggested_chart_ids,
        )
        .expect("restore chart reference");

        assert_eq!(
            snapshot
                .chart_page_state
                .selected_reference_family_id
                .as_deref(),
            Some("enr-l")
        );
        assert_eq!(snapshot.chart_page_state.selected_chart_id, chart_id);
        assert_eq!(
            snapshot.chart_page_state.suggested_chart_ids,
            suggested_chart_ids
        );
        destroy_session(init.handle);
    }

    #[test]
    fn spot_cdi_label_omits_coordinates() {
        assert_eq!(
            nav_ref_label(&NavRef::Spot(LatLon {
                lat: 47.626,
                lon: -122.194,
            })),
            "SPOT"
        );
        assert_eq!(
            nav_ref_label(&NavRef::LatLon(LatLon {
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

    fn snapshot_from_outcome(outcome: HadOperationOutcome) -> UiSessionSnapshot {
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("session operation unexpectedly needed resources: {outcome:?}");
        };
        serde_json::from_value(result).expect("session snapshot")
    }

    macro_rules! snapshot_wrapper {
        ($name:ident($($arg:ident: $arg_type:ty),* $(,)?)) => {
            #[allow(dead_code)]
            fn $name($($arg: $arg_type),*) -> AppResult<UiSessionSnapshot> {
                super::$name($($arg),*).map(snapshot_from_outcome)
            }
        };
    }

    snapshot_wrapper!(set_map_layer_visibility_in_session(
        handle: u32,
        layer_id: &str,
        visible: bool,
    ));
    snapshot_wrapper!(set_guidance_leg_geometry_in_session(
        handle: u32,
        geometries: Vec<GuidanceLegGeometry>,
    ));
    snapshot_wrapper!(load_raster_map_catalog_in_session(handle: u32));
    snapshot_wrapper!(set_resource_policy_in_session(
        handle: u32,
        policy: CoreResourcePolicy,
    ));
    snapshot_wrapper!(load_offline_package_library_cache_in_session(
        handle: u32,
        cache: OfflinePackagesLibraryCache,
    ));
    snapshot_wrapper!(configure_platform_capabilities_in_session(
        handle: u32,
        capabilities: PlatformCapabilities,
        settings_storage: Option<SettingsStorageHandle>,
    ));
    snapshot_wrapper!(perform_settings_action_in_session(
        handle: u32,
        action: UiSettingsAction,
    ));
    snapshot_wrapper!(accept_disclaimer_in_session(
        handle: u32,
        agreement_id: &str,
    ));
    snapshot_wrapper!(set_installed_package_ids_in_session(
        handle: u32,
        package_ids: Vec<String>,
    ));
    snapshot_wrapper!(select_map_family_in_session(handle: u32, family_id: &str));
    snapshot_wrapper!(select_raster_map_in_session(
        handle: u32,
        selected_map_id: &str,
    ));
    snapshot_wrapper!(select_airport_in_session(handle: u32, airport_id: &str));
    snapshot_wrapper!(select_chart_in_session(handle: u32, chart_id: &str));
    snapshot_wrapper!(select_chart_reference_in_session(
        handle: u32,
        family_id: &str,
        suggested_chart_ids: &[String],
    ));
    snapshot_wrapper!(register_ownship_source_in_session(
        handle: u32,
        registration: crate::OwnshipSourceRegistration,
    ));
    snapshot_wrapper!(update_ownship_source_status_in_session(
        handle: u32,
        update: crate::OwnshipSourceStatusUpdate,
    ));
    snapshot_wrapper!(push_situation_sample_in_session(
        handle: u32,
        sample: crate::SituationSample,
    ));
    snapshot_wrapper!(set_ownship_policy_in_session(
        handle: u32,
        policy: crate::OwnshipPolicy,
    ));
    snapshot_wrapper!(select_ownship_source_in_session(
        handle: u32,
        selection: crate::OwnshipSelectionCommand,
    ));
    snapshot_wrapper!(apply_situation_control_input_in_session(
        handle: u32,
        input: SituationControlInput,
        now_epoch_ms: f64,
    ));
    snapshot_wrapper!(load_playback_trace_in_session(
        handle: u32,
        source_path: &str,
        trace_json: &str,
    ));
    snapshot_wrapper!(play_playback_in_session(handle: u32, now_epoch_ms: f64));
    snapshot_wrapper!(pause_playback_in_session(handle: u32, now_epoch_ms: f64));
    snapshot_wrapper!(seek_playback_in_session(
        handle: u32,
        cursor_seconds: f64,
        now_epoch_ms: f64,
    ));
    snapshot_wrapper!(set_playback_rate_in_session(
        handle: u32,
        rate: f64,
        now_epoch_ms: f64,
    ));
    snapshot_wrapper!(tick_playback_in_session(handle: u32, now_epoch_ms: f64));
    snapshot_wrapper!(set_situation_in_session(handle: u32, situation: Situation));
    snapshot_wrapper!(tick_bad_autopilot_in_session(handle: u32, now_epoch_ms: f64));
    snapshot_wrapper!(activate_next_leg_in_session(handle: u32));
    snapshot_wrapper!(stop_navigation_in_session(handle: u32));
    snapshot_wrapper!(suspend_sequencing_in_session(handle: u32));
    snapshot_wrapper!(unsuspend_sequencing_in_session(handle: u32));
    snapshot_wrapper!(sequence_active_leg_in_session(handle: u32));
    snapshot_wrapper!(replace_flight_plan_in_session(
        handle: u32,
        plan: FlightPlan,
    ));
    snapshot_wrapper!(activate_direct_to_leg_in_session(
        handle: u32,
        target_leg_index: usize,
    ));
    snapshot_wrapper!(perform_status_action_in_session(
        handle: u32,
        action_id: String,
    ));
    snapshot_wrapper!(restore_direct_to_in_session(handle: u32));
    snapshot_wrapper!(engage_map_follow_in_session(
        handle: u32,
        viewport: MapViewport,
    ));
    snapshot_wrapper!(disengage_map_follow_in_session(
        handle: u32,
        viewport: MapViewport,
    ));
    snapshot_wrapper!(set_map_follow_offset_in_session(
        handle: u32,
        viewport: MapViewport,
        offset_x_px: f64,
        offset_y_px: f64,
    ));
    snapshot_wrapper!(sync_map_follow_in_session(
        handle: u32,
        viewport: MapViewport,
        width_px: f64,
        height_px: f64,
    ));
    snapshot_wrapper!(restore_chart_page_state_in_session(
        handle: u32,
        recent_airport_ids: &[String],
        plate_target_airport_id: Option<&str>,
        selected_airport_id: Option<&str>,
        selected_reference_family_id: Option<&str>,
        selected_chart_id: Option<&str>,
        suggested_chart_ids: &[String],
    ));
    snapshot_wrapper!(set_debug_flag_in_session(
        handle: u32,
        flag_id: &str,
        enabled: bool,
    ));
    snapshot_wrapper!(report_live_feed_connection_event_in_session(
        handle: u32,
        event: LiveFeedConnectionEvent,
        epoch_ms: i64,
    ));
    snapshot_wrapper!(install_live_feed_installed_state_in_session(
        handle: u32,
        installed: &crate::LiveFeedInstalledState,
    ));
    snapshot_wrapper!(sync_live_feed_catalog_in_session(
        handle: u32,
        live_feeds: &crate::LiveFeedsState,
    ));
    snapshot_wrapper!(report_session_resource_failure_in_session(
        handle: u32,
        resource_id: &str,
        message: &str,
    ));
    snapshot_wrapper!(report_session_resource_failure_in_session_at_epoch_ms(
        handle: u32,
        resource_id: &str,
        message: &str,
        epoch_ms: i64,
    ));

    fn get_session_snapshot(handle: u32) -> AppResult<UiSessionSnapshot> {
        super::get_session_snapshot(handle).map(snapshot_from_outcome)
    }

    fn get_session_snapshot_at_epoch_ms(
        handle: u32,
        epoch_ms: i64,
    ) -> AppResult<UiSessionSnapshot> {
        super::get_session_snapshot_at_epoch_ms(handle, epoch_ms).map(snapshot_from_outcome)
    }

    #[derive(Default)]
    struct MemorySettingsStorage {
        bytes: Mutex<Option<Vec<u8>>>,
    }

    impl SettingsStorage for MemorySettingsStorage {
        fn read_settings(&self) -> AppResult<Option<Vec<u8>>> {
            Ok(self.bytes.lock().expect("settings lock").clone())
        }

        fn write_settings(&self, bytes: &[u8]) -> AppResult<()> {
            *self.bytes.lock().expect("settings lock") = Some(bytes.to_vec());
            Ok(())
        }
    }

    #[test]
    fn home_page_buttons_are_stable_and_explain_unavailable_offline_packages() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let web_snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities::default(),
            None,
        )
        .expect("configure web-like platform capabilities");

        assert_eq!(
            web_snapshot
                .home_page_state
                .buttons
                .iter()
                .map(|button| (button.id.as_str(), button.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("chart", "CHART"),
                ("plate", "PLATE"),
                ("flight-plan", "FLIGHT\nPLAN"),
                ("data-status", "STATUS"),
                ("settings", "SETTINGS"),
                ("offline-packages", "OFFLINE\nPACKAGES"),
                ("about", "ABOUT"),
            ]
        );
        let web_offline = web_snapshot
            .home_page_state
            .buttons
            .iter()
            .find(|button| button.id == "offline-packages")
            .expect("offline packages button");
        assert!(!web_offline.enabled);
        assert_eq!(
            web_offline.disabled_reason.as_deref(),
            Some(
                "This client fetches data as needed and does not support managing Offline Packages."
            )
        );

        let android_snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities {
                offline_packages: Some(PlatformOfflinePackagesCapability::default()),
                ..PlatformCapabilities::default()
            },
            None,
        )
        .expect("configure Android-like platform capabilities");
        assert_eq!(
            android_snapshot
                .home_page_state
                .buttons
                .iter()
                .map(|button| (&button.id, &button.label))
                .collect::<Vec<_>>(),
            web_snapshot
                .home_page_state
                .buttons
                .iter()
                .map(|button| (&button.id, &button.label))
                .collect::<Vec<_>>()
        );
        let android_offline = android_snapshot
            .home_page_state
            .buttons
            .iter()
            .find(|button| button.id == "offline-packages")
            .expect("offline packages button");
        assert!(android_offline.enabled);
        assert_eq!(android_offline.disabled_reason, None);
    }

    #[test]
    fn settings_page_always_contains_flight_data_grid_choices() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities::default(),
            Some(Arc::new(MemorySettingsStorage::default())),
        )
        .expect("configure platform capabilities");

        assert_eq!(snapshot.settings_page_state.rows.len(), 1);
        let row = &snapshot.settings_page_state.rows[0];
        assert_eq!(row.id, "flight_data_visibility");
        assert_eq!(row.kind, "grid_choices");
        assert_eq!(row.items.len(), 12);
        assert!(row.items.iter().all(|item| item.enabled));
        assert!(snapshot.display_policy.is_none());
    }

    #[test]
    fn display_dim_setting_uses_core_owned_storage() {
        let storage: SettingsStorageHandle = Arc::new(MemorySettingsStorage::default());
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities {
                display_policy: Some(PlatformDisplayPolicyCapability::default()),
                ..PlatformCapabilities::default()
            },
            Some(storage.clone()),
        )
        .expect("configure platform capabilities");

        assert_eq!(snapshot.settings_page_state.rows.len(), 2);
        assert_eq!(snapshot.settings_page_state.rows[1].value_id, "2m");
        assert_eq!(
            snapshot
                .display_policy
                .as_ref()
                .and_then(|policy| policy.dim_after_ms),
            Some(120_000)
        );

        let snapshot = perform_settings_action_in_session(
            init.handle,
            UiSettingsAction {
                action_id: "display_dim_timeout".to_string(),
                value_id: "30s".to_string(),
            },
        )
        .expect("perform settings action");
        assert_eq!(snapshot.settings_page_state.rows[1].value_id, "30s");
        assert_eq!(
            snapshot
                .display_policy
                .as_ref()
                .and_then(|policy| policy.dim_after_ms),
            Some(30_000)
        );

        let persisted = storage
            .read_settings()
            .expect("read settings")
            .expect("persisted bytes");
        let persisted_json: serde_json::Value =
            serde_json::from_slice(&persisted).expect("persisted json");
        assert_eq!(persisted_json["version"], 1);
        assert_eq!(persisted_json["preferences"]["display_dim_timeout"], "30s");

        let next =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let next_snapshot = configure_platform_capabilities_in_session(
            next.handle,
            PlatformCapabilities {
                display_policy: Some(PlatformDisplayPolicyCapability::default()),
                ..PlatformCapabilities::default()
            },
            Some(storage),
        )
        .expect("configure platform capabilities");
        assert_eq!(next_snapshot.settings_page_state.rows[1].value_id, "30s");
        assert_eq!(
            next_snapshot
                .display_policy
                .as_ref()
                .and_then(|policy| policy.dim_after_ms),
            Some(30_000)
        );
    }

    #[test]
    fn flight_data_visibility_setting_filters_snapshot_and_persists() {
        let storage: SettingsStorageHandle = Arc::new(MemorySettingsStorage::default());
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities::default(),
            Some(storage.clone()),
        )
        .expect("configure platform capabilities");
        assert_eq!(snapshot.app_ui_state.flight_data_banner.cells.len(), 12);

        let snapshot = perform_settings_action_in_session(
            init.handle,
            UiSettingsAction {
                action_id: "flight_data_visibility".to_string(),
                value_id: "nexrad_age".to_string(),
            },
        )
        .expect("disable NEXRAD cell");
        assert!(!snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .any(|cell| cell.id == "nexrad_age"));
        let settings_item = snapshot.settings_page_state.rows[0]
            .items
            .iter()
            .find(|item| item.cell.id == "nexrad_age")
            .expect("NEXRAD settings item");
        assert!(!settings_item.enabled);

        let persisted = storage
            .read_settings()
            .expect("read settings")
            .expect("persisted bytes");
        let persisted_json: serde_json::Value =
            serde_json::from_slice(&persisted).expect("persisted json");
        assert_eq!(
            persisted_json["preferences"]["disabled_flight_data_cell_ids"][0],
            "nexrad_age"
        );

        let next =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let next_snapshot = configure_platform_capabilities_in_session(
            next.handle,
            PlatformCapabilities::default(),
            Some(storage),
        )
        .expect("load persisted preferences");
        assert!(!next_snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .any(|cell| cell.id == "nexrad_age"));

        let restored_snapshot = perform_settings_action_in_session(
            next.handle,
            UiSettingsAction {
                action_id: "flight_data_visibility".to_string(),
                value_id: "nexrad_age".to_string(),
            },
        )
        .expect("re-enable NEXRAD cell");
        assert!(restored_snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .any(|cell| cell.id == "nexrad_age"));
    }

    #[test]
    fn disclaimer_agreement_uses_core_owned_storage() {
        let storage: SettingsStorageHandle = Arc::new(MemorySettingsStorage::default());
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities::default(),
            Some(storage.clone()),
        )
        .expect("configure platform capabilities");

        assert!(snapshot.disclaimer_state.required);
        assert_eq!(snapshot.disclaimer_state.agreement_id, "no-warranty-v1");
        assert!(snapshot.disclaimer_state.text.contains("NO WARRANTY"));

        let snapshot =
            accept_disclaimer_in_session(init.handle, "no-warranty-v1").expect("accept disclaimer");
        assert!(!snapshot.disclaimer_state.required);

        let persisted = storage
            .read_settings()
            .expect("read settings")
            .expect("persisted bytes");
        let persisted_json: serde_json::Value =
            serde_json::from_slice(&persisted).expect("persisted json");
        assert_eq!(
            persisted_json["preferences"]["accepted_disclaimer_agreement_ids"][0],
            "no-warranty-v1"
        );

        let next =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let next_snapshot = configure_platform_capabilities_in_session(
            next.handle,
            PlatformCapabilities::default(),
            Some(storage),
        )
        .expect("configure platform capabilities");
        assert!(!next_snapshot.disclaimer_state.required);
    }

    struct TestObstacleHad {
        manifest: serde_json::Value,
        root_bytes: Vec<u8>,
        pages: Vec<Vec<u8>>,
        state_sha256: String,
    }

    fn build_test_obstacle_had(
        viewport: &MapViewport,
        version: &str,
        record_id: &str,
    ) -> TestObstacleHad {
        let obstacle_manifest_base = serde_json::json!({
            "schema_version": 1,
            "product_id": "obstacles",
            "version_label": version,
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
            visible_obstacle_tile_window(&obstacle_layer, viewport, 240.0, 240.0, None, 1.0)
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
                id: record_id.to_string(),
                kind: "obstacle".to_string(),
                lat: viewport.center.lat,
                lon: viewport.center.lon,
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
        let mut manifest = obstacle_manifest_base;
        manifest["page_count"] = serde_json::json!(built.pages.len());
        manifest["page_size"] = serde_json::json!(built.page_size);
        manifest["state_sha256"] = serde_json::json!(state_sha256);

        TestObstacleHad {
            manifest,
            root_bytes: built.root_bytes,
            pages: built.pages,
            state_sha256,
        }
    }

    fn ingest_test_live_obstacle_state(handle: u32, version: &str, obstacle_had: &TestObstacleHad) {
        ingest_resource_in_session(
            handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "schema_version": 3,
                    "products": {{
                        "obstacles": {{
                            "current": "{version}",
                            "version_manifest_url": "versions/obstacles/{version}.json",
                            "state_url": "states/obstacles/{version}/manifest.json",
                            "state_sha256": "{}"
                        }}
                    }}
                }}"#,
                obstacle_had.state_sha256
            )
            .as_bytes(),
        )
        .expect("current manifest");
        ingest_resource_in_session(
            handle,
            &format!("live_feeds/version/obstacles/{version}"),
            format!(
                r#"{{
                    "schema_version": 3,
                    "product": "obstacles",
                    "version": "{version}",
                    "state": {{
                        "kind": "nav_kv",
                        "url": "states/obstacles/{version}/manifest.json",
                        "state_sha256": "{}"
                    }}
                }}"#,
                obstacle_had.state_sha256
            )
            .as_bytes(),
        )
        .expect("version manifest");
        ingest_resource_in_session(
            handle,
            &format!("live_feeds/state/obstacles/{version}"),
            &serde_json::to_vec(&obstacle_had.manifest).expect("manifest json"),
        )
        .expect("state manifest");
    }

    fn load_test_live_obstacle_had_pages(
        handle: u32,
        version: &str,
        obstacle_had: &TestObstacleHad,
        metrics: &MapSurfaceMetrics,
    ) {
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, metrics);
            assert_eq!(statuses.len(), 1);
        }
        let effects = drain_session_resource_effects(handle).expect("root effects");
        let effects = effects
            .into_iter()
            .filter(|effect| {
                effect
                    .resource
                    .id
                    .starts_with(&format!("live_obstacle_had/{version}/"))
            })
            .collect::<Vec<_>>();
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0].resource.id,
            format!("live_obstacle_had/{version}/root")
        );
        ingest_resource_in_session(
            handle,
            &format!("live_obstacle_had/{version}/root"),
            &obstacle_had.root_bytes,
        )
        .expect("ingest root");

        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, metrics);
            assert_eq!(statuses.len(), 1);
        }
        let page_effects = drain_session_resource_effects(handle)
            .expect("page effects")
            .into_iter()
            .filter(|effect| {
                effect
                    .resource
                    .id
                    .starts_with(&format!("live_obstacle_had/{version}/"))
            })
            .collect::<Vec<_>>();
        assert!(!page_effects.is_empty());
        for effect in &page_effects {
            let page_text = effect
                .resource
                .id
                .strip_prefix(&format!("live_obstacle_had/{version}/page/"))
                .expect("page resource id");
            let page_index = page_text.parse::<usize>().expect("page index");
            ingest_resource_in_session(
                handle,
                &effect.resource.id,
                &obstacle_had.pages[page_index],
            )
            .expect("ingest page");
        }
    }

    fn query_obstacle_feature_ids(handle: u32, metrics: &MapSurfaceMetrics) -> Vec<String> {
        let (config, obstacle_cache) = {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, handle).expect("session");
            let statuses = ensure_live_obstacle_inputs_loaded(session, metrics);
            assert!(statuses.is_empty());
            (
                session.map_overlay_config.clone(),
                session.obstacle_tile_cache.clone(),
            )
        };
        let overlay = crate::query_map_overlay_for_surface(
            metrics,
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
            &[],
        );
        overlay
            .visible_features
            .into_iter()
            .map(|feature| feature.id)
            .collect()
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

    fn nav_db_advance_store_for_test() -> NavKvStore {
        crate::navkv::nav_kv_store_for_test(
            &[
                ("chart/catalog", br#"[]"#),
                ("vector/manifest", minimal_vector_manifest_json().as_bytes()),
            ],
            1024,
        )
    }

    fn nav_db_advance_two_airport_plan() -> FlightPlan {
        crate::build_flight_plan(FlightPlan {
            id: "nav-db-advance-two-airport-plan".to_string(),
            name: "KAAA KBBB".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build NAVDB advance test plan")
    }

    fn nav_db_advance_store_with_airports(include_kbbb: bool) -> NavKvStore {
        let mut entries = vec![
            ("chart/catalog", br#"[]"#.as_slice()),
            ("vector/manifest", minimal_vector_manifest_json().as_bytes()),
            (
                "navref/position/airport/KAAA",
                br#"{"lat":47.0,"lon":-122.0}"#.as_slice(),
            ),
        ];
        if include_kbbb {
            entries.push((
                "navref/position/airport/KBBB",
                br#"{"lat":48.0,"lon":-123.0}"#.as_slice(),
            ));
        }
        crate::navkv::nav_kv_store_for_test(&entries, 1024)
    }

    fn nav_db_advance_result(outcome: HadOperationOutcome) -> NavDbAdvanceResult {
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("NAVDB advance unexpectedly needed resources: {outcome:?}");
        };
        serde_json::from_value(result).expect("NAVDB advance result")
    }

    fn nav_db_maintenance_result(outcome: HadOperationOutcome) -> NavDbMaintenanceResult {
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("NAVDB maintenance unexpectedly needed resources: {outcome:?}");
        };
        serde_json::from_value(result).expect("NAVDB maintenance result")
    }

    fn install_nav_db_maintenance_catalog(
        handle: u32,
        fetched_at_epoch_ms: i64,
        installed_package_ids: &[&str],
    ) {
        let bundle_filename = "bundle-cycle-test.json";
        let discovery =
            serde_json::from_value::<crate::CurrentArtifactsManifest>(serde_json::json!({
                "schema_version": 1,
                "contracts": { "nav-db": crate::REQUIRED_NAV_DB_CONTRACT_ID },
                "artifact_roots": {
                    "packaged": "published_packaged",
                    "unpacked": "published_unpacked"
                },
                "as_of_utc": "2026-07-01T00:00:00Z",
                "bundles": [{
                    "filename": bundle_filename,
                    "relative_path": bundle_filename,
                    "id": "bundle-cycle-test",
                    "bundle_type": "cycle"
                }]
            }))
            .expect("maintenance discovery");
        let packages = [
            (
                "NAV_DB_2607",
                "2607",
                "2026-07-01T00:00:00Z",
                "2026-07-16T00:00:00Z",
            ),
            (
                "NAV_DB_2608",
                "2608",
                "2026-07-16T00:00:00Z",
                "2026-08-13T00:00:00Z",
            ),
        ]
        .into_iter()
        .map(|(id, cycle, effective, expiration)| {
            serde_json::from_value::<crate::BundlePackageArtifact>(serde_json::json!({
                "id": id,
                "family_id": "nav-db",
                "contract_id": crate::REQUIRED_NAV_DB_CONTRACT_ID,
                "filename": format!("{id}.zip"),
                "relative_path": format!("{id}.zip"),
                "cycle": cycle,
                "cycle_version": "01",
                "effective_date": effective,
                "expiration_date": expiration
            }))
            .expect("maintenance package")
        })
        .collect();
        load_offline_package_library_cache_in_session(
            handle,
            OfflinePackagesLibraryCache {
                package_source_base_url: "https://example.test/packages".to_string(),
                fetched_at_epoch_ms,
                discovery_manifests: vec![discovery],
                bundle_manifests_by_filename: BTreeMap::from([(
                    bundle_filename.to_string(),
                    crate::BundleManifest { packages },
                )]),
            },
        )
        .expect("load maintenance package catalog");
        set_installed_package_ids_in_session(
            handle,
            installed_package_ids
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
        )
        .expect("set installed maintenance packages");
    }

    #[test]
    fn nav_db_maintenance_requests_advance_when_installed_next_cycle_becomes_effective() {
        let before_rollover = utc("2026-07-15T23:59:00Z").timestamp_millis();
        let rollover = utc("2026-07-16T00:00:00Z").timestamp_millis();
        let init =
            create_ui_session_at_epoch_ms(FlightPlan::default(), &[], None, None, before_rollover)
                .expect("create maintenance session");
        let old_store = nav_db_advance_store_for_test();
        let mut old_open = nav_db_open_result_for_test("NAV_DB_2607", Some("2026-07-16T00:00:00Z"));
        old_open.selected_effective_date = Some("2026-07-01T00:00:00Z".to_string());
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            1,
            &old_store,
            Some(&old_open),
        )
        .expect("attach current NAVDB");
        install_nav_db_maintenance_catalog(
            init.handle,
            before_rollover,
            &["NAV_DB_2607", "NAV_DB_2608"],
        );

        let before = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(init.handle, before_rollover)
                .expect("maintenance before rollover"),
        );
        assert_eq!(before.action, NavDbMaintenanceAction::None);
        assert_eq!(
            before.snapshot.next_nav_db_maintenance_epoch_ms,
            Some(rollover)
        );

        let after = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(init.handle, rollover)
                .expect("maintenance at rollover"),
        );
        assert_eq!(after.action, NavDbMaintenanceAction::AttemptAdvance);
        assert_eq!(
            after
                .snapshot
                .active_nav_db
                .as_ref()
                .map(|identity| identity.package_id.as_str()),
            Some("NAV_DB_2607")
        );
    }

    #[test]
    fn web_nav_db_maintenance_periodically_refreshes_current_artifacts() {
        let checked_at = utc("2026-07-15T12:00:00Z").timestamp_millis();
        let init =
            create_ui_session_at_epoch_ms(FlightPlan::default(), &[], None, None, checked_at)
                .expect("create web maintenance session");
        set_resource_policy_in_session(init.handle, CoreResourcePolicy::PublicUnpacked)
            .expect("set web resource policy");
        let current_artifacts = format!(
            r#"[{{"schema_version":1,"contracts":{{"nav-db":"{}"}},"as_of_utc":"2026-07-15T12:00:00Z","artifact_roots":{{"packaged":"packaged","unpacked":"unpacked"}},"bundles":[]}}]"#,
            crate::REQUIRED_NAV_DB_CONTRACT_ID
        );
        ingest_resource_in_session_at_epoch_ms(
            init.handle,
            "publication/current_artifacts",
            current_artifacts.as_bytes(),
            checked_at,
        )
        .expect("ingest current artifacts");

        let before = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(
                init.handle,
                checked_at + NAV_DB_PUBLICATION_POLL_INTERVAL_MS - 1,
            )
            .expect("maintenance before poll"),
        );
        assert_eq!(before.action, NavDbMaintenanceAction::None);

        let due = maintain_nav_db_in_session_at_epoch_ms(
            init.handle,
            checked_at + NAV_DB_PUBLICATION_POLL_INTERVAL_MS,
        )
        .expect("maintenance at poll");
        let HadOperationOutcome::NeedResources { resources } = due else {
            panic!("web maintenance did not request publication refresh");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "publication/current_artifacts");
    }

    #[test]
    fn web_nav_db_maintenance_waits_for_newly_published_candidate_to_become_effective() {
        let checked_at = utc("2026-07-15T12:00:00Z").timestamp_millis();
        let poll_at = checked_at + NAV_DB_PUBLICATION_POLL_INTERVAL_MS;
        let effective_at = utc("2026-07-15T16:01:00Z").timestamp_millis();
        let init =
            create_ui_session_at_epoch_ms(FlightPlan::default(), &[], None, None, checked_at)
                .expect("create web discovery session");
        set_resource_policy_in_session(init.handle, CoreResourcePolicy::PublicUnpacked)
            .expect("set web policy");
        let old_store = nav_db_advance_store_for_test();
        let old_open = nav_db_open_result_for_test("NAV_DB_2607", None);
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            1,
            &old_store,
            Some(&old_open),
        )
        .expect("attach old NAVDB");
        let initial = format!(
            r#"[{{"schema_version":1,"contracts":{{"nav-db":"{}"}},"as_of_utc":"2026-07-15T12:00:00Z","artifact_roots":{{"packaged":"packaged","unpacked":"unpacked"}},"bundles":[]}}]"#,
            crate::REQUIRED_NAV_DB_CONTRACT_ID
        );
        ingest_resource_in_session_at_epoch_ms(
            init.handle,
            "publication/current_artifacts",
            initial.as_bytes(),
            checked_at,
        )
        .expect("ingest initial publication");
        assert!(matches!(
            maintain_nav_db_in_session_at_epoch_ms(init.handle, poll_at)
                .expect("request publication refresh"),
            HadOperationOutcome::NeedResources { .. }
        ));

        let refreshed = format!(
            r#"[{{"schema_version":1,"contracts":{{"nav-db":"{}"}},"as_of_utc":"2026-07-15T16:00:00Z","artifact_roots":{{"packaged":"packaged","unpacked":"unpacked"}},"bundles":[{{"filename":"bundle-2608.json","relative_path":"bundle-2608.json","id":"bundle-2608","bundle_type":"cycle"}}]}}]"#,
            crate::REQUIRED_NAV_DB_CONTRACT_ID
        );
        ingest_resource_in_session_at_epoch_ms(
            init.handle,
            "publication/current_artifacts",
            refreshed.as_bytes(),
            poll_at,
        )
        .expect("ingest refreshed publication");
        let missing_bundle =
            maintain_nav_db_in_session_at_epoch_ms(init.handle, poll_at).expect("request bundle");
        let HadOperationOutcome::NeedResources { resources } = missing_bundle else {
            panic!("new publication did not request its bundle");
        };
        assert_eq!(resources[0].id, "publication/bundle/bundle-2608.json");

        let bundle = serde_json::json!({
            "packages": [{
                "id": "NAV_DB_2608",
                "family_id": "nav-db",
                "contract_id": crate::REQUIRED_NAV_DB_CONTRACT_ID,
                "filename": "NAV_DB_2608.zip",
                "relative_path": "NAV_DB_2608.zip",
                "cycle": "2608",
                "cycle_version": "01",
                "effective_date": "2026-07-15T16:01:00Z",
                "expiration_date": "2026-08-13T00:00:00Z"
            }]
        });
        ingest_resource_in_session(
            init.handle,
            "publication/bundle/bundle-2608.json",
            &serde_json::to_vec(&bundle).expect("bundle json"),
        )
        .expect("ingest new bundle");

        let discovered = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(init.handle, poll_at)
                .expect("discover new candidate"),
        );
        assert_eq!(discovered.action, NavDbMaintenanceAction::None);
        assert_eq!(
            discovered.snapshot.next_nav_db_maintenance_epoch_ms,
            Some(effective_at)
        );

        let effective = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(init.handle, effective_at)
                .expect("candidate becomes effective"),
        );
        assert_eq!(effective.action, NavDbMaintenanceAction::AttemptAdvance);
    }

    #[test]
    fn nav_db_advance_commits_candidate_atomically_and_advances_epoch_once() {
        let init = create_current_test_session();
        let old_store = nav_db_advance_store_for_test();
        let old_open = nav_db_open_result_for_test("NAV_DB_2607", None);
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            1,
            &old_store,
            Some(&old_open),
        )
        .expect("attach old NAVDB");
        let next_store = nav_db_advance_store_for_test();
        let next_open = nav_db_open_result_for_test("NAV_DB_2608", None);

        let outcome = advance_nav_kv_store_in_session_with_open_result(
            init.handle,
            2,
            &next_store,
            &next_open,
            vec!["NAV_DB_2608".to_string()],
        )
        .expect("advance NAVDB");
        let invalidations = match &outcome {
            HadOperationOutcome::Complete { invalidations, .. } => invalidations.clone(),
            _ => unreachable!(),
        };
        let result = nav_db_advance_result(outcome);

        assert_eq!(result.disposition, NavDbAdvanceDisposition::Adopted);
        assert_eq!(result.snapshot.nav_data_epoch, 1);
        assert_eq!(result.snapshot.session_revision, 1);
        assert_eq!(
            result
                .snapshot
                .active_nav_db
                .as_ref()
                .map(|identity| identity.filename.as_str()),
            Some("NAV_DB_2608.zip")
        );
        assert_eq!(
            result.active_artifact_filename.as_deref(),
            Some("NAV_DB_2608.zip")
        );
        assert_eq!(result.retained_artifact_filenames, ["NAV_DB_2608.zip"]);
        assert!(invalidations.contains(&UiInvalidation::NavData));
        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).expect("live session");
        assert_eq!(session.nav_data_epoch, 1);
        assert_eq!(session.nav_kv_store_id, Some(2));
        assert_eq!(
            session
                .nav_db_artifact
                .as_ref()
                .map(|artifact| artifact.filename.as_str()),
            Some("NAV_DB_2608.zip")
        );
    }

    #[test]
    fn nav_db_advance_page_fault_does_not_mutate_live_generation() {
        let init = create_current_test_session();
        let old_store = nav_db_advance_store_for_test();
        let old_open = nav_db_open_result_for_test("NAV_DB_2607", None);
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            11,
            &old_store,
            Some(&old_open),
        )
        .expect("attach old NAVDB");
        let entries = [
            ("chart/catalog", br#"[]"#.as_slice()),
            ("vector/manifest", minimal_vector_manifest_json().as_bytes()),
        ];
        let (mut next_store, pages) =
            crate::navkv::nav_kv_store_without_pages_and_pages_for_test(&entries, 1024);
        let next_open = nav_db_open_result_for_test("NAV_DB_2608", None);

        let first = advance_nav_kv_store_in_session_with_open_result(
            init.handle,
            12,
            &next_store,
            &next_open,
            vec!["NAV_DB_2608".to_string()],
        )
        .expect("fault candidate pages");
        assert!(matches!(first, HadOperationOutcome::NeedResources { .. }));
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("live session");
            assert_eq!(session.nav_data_epoch, 0);
            assert_eq!(session.nav_kv_store_id, Some(11));
            assert_eq!(
                session
                    .nav_db_artifact
                    .as_ref()
                    .map(|artifact| artifact.filename.as_str()),
                Some("NAV_DB_2607.zip")
            );
        }

        for (page_index, page) in pages.into_iter().enumerate() {
            next_store.insert_page(page_index as u32, page);
        }
        let result = nav_db_advance_result(
            advance_nav_kv_store_in_session_with_open_result(
                init.handle,
                12,
                &next_store,
                &next_open,
                vec!["NAV_DB_2608".to_string()],
            )
            .expect("retry NAVDB advance"),
        );
        assert_eq!(result.disposition, NavDbAdvanceDisposition::Adopted);
        assert_eq!(result.snapshot.nav_data_epoch, 1);
    }

    #[test]
    fn nav_db_advance_rejects_bad_candidate_and_pins_old_artifact() {
        let init = create_current_test_session();
        let old_store = nav_db_advance_store_for_test();
        let old_open = nav_db_open_result_for_test("NAV_DB_2607", None);
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            21,
            &old_store,
            Some(&old_open),
        )
        .expect("attach old NAVDB");
        let broken_store = crate::navkv::nav_kv_store_for_test(
            &[("vector/manifest", minimal_vector_manifest_json().as_bytes())],
            1024,
        );
        let next_open = nav_db_open_result_for_test("NAV_DB_2608", None);

        let result = nav_db_advance_result(
            advance_nav_kv_store_in_session_with_open_result(
                init.handle,
                22,
                &broken_store,
                &next_open,
                vec!["NAV_DB_2608".to_string()],
            )
            .expect("reject candidate"),
        );

        assert_eq!(result.disposition, NavDbAdvanceDisposition::Rejected);
        assert_eq!(result.snapshot.nav_data_epoch, 0);
        assert_eq!(result.retained_artifact_filenames, ["NAV_DB_2607.zip"]);
        assert!(result.rejection_reason.is_some());
        let warning = data_status_box(&result.snapshot, NAV_DB_ADVANCE_STATUS_ID);
        assert!(!warning.hushed);
        assert!(warning
            .detail
            .contains("Reload application when not flying"));
        assert!(warning
            .actions
            .iter()
            .any(|action| action.id == RELOAD_APPLICATION_ACTION_ID));
        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).expect("live session");
        assert_eq!(session.nav_kv_store_id, Some(21));
        assert!(session.nav_db_advance_blocked);
        drop(sessions);

        let maintenance = nav_db_maintenance_result(
            maintain_nav_db_in_session_at_epoch_ms(
                init.handle,
                utc("2026-07-16T00:00:00Z").timestamp_millis(),
            )
            .expect("blocked maintenance"),
        );
        assert_eq!(maintenance.action, NavDbMaintenanceAction::None);
        assert_eq!(maintenance.snapshot.next_nav_db_maintenance_epoch_ms, None);

        let second = nav_db_advance_result(
            advance_nav_kv_store_in_session_with_open_result(
                init.handle,
                23,
                &broken_store,
                &next_open,
                vec!["NAV_DB_2608".to_string()],
            )
            .expect("blocked second advance"),
        );
        assert_eq!(second.disposition, NavDbAdvanceDisposition::Rejected);
        assert_eq!(
            second.rejection_reason.as_deref(),
            Some("NAVDB advance is blocked until application reload")
        );
    }

    #[test]
    fn nav_db_advance_rejects_candidate_missing_required_plan_waypoint() {
        let plan = nav_db_advance_two_airport_plan();
        let init = create_ui_session(plan.clone(), &[], None, None).expect("create session");
        let old_store = nav_db_advance_store_with_airports(true);
        let old_open = nav_db_open_result_for_test("NAV_DB_2607", None);
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            31,
            &old_store,
            Some(&old_open),
        )
        .expect("attach old NAVDB");
        let missing_waypoint_store = nav_db_advance_store_with_airports(false);
        let next_open = nav_db_open_result_for_test("NAV_DB_2608", None);

        let result = nav_db_advance_result(
            advance_nav_kv_store_in_session_with_open_result(
                init.handle,
                32,
                &missing_waypoint_store,
                &next_open,
                vec!["NAV_DB_2608".to_string()],
            )
            .expect("reject candidate missing flight-plan waypoint"),
        );

        assert_eq!(result.disposition, NavDbAdvanceDisposition::Rejected);
        assert_eq!(result.snapshot.nav_data_epoch, 0);
        assert_eq!(result.snapshot.app_state.active_plan.as_ref(), Some(&plan));
        assert!(result
            .rejection_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("KBBB")));
        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).expect("retained session");
        assert_eq!(session.nav_kv_store_id, Some(31));
        assert_eq!(session.app_state.active_plan.as_ref(), Some(&plan));
    }

    fn load_nav_db_fixture_zip(path: &std::path::Path) -> NavKvStore {
        let file = std::fs::File::open(path)
            .unwrap_or_else(|error| panic!("open NAVDB fixture {}: {error}", path.display()));
        let mut archive = zip::ZipArchive::new(file)
            .unwrap_or_else(|error| panic!("decode NAVDB fixture {}: {error}", path.display()));
        let mut root_bytes = Vec::new();
        archive
            .by_name("root")
            .expect("NAVDB root member")
            .read_to_end(&mut root_bytes)
            .expect("read NAVDB root");
        let mut store = NavKvStore::new(NavKvRoot::parse(&root_bytes).expect("parse NAVDB root"));
        for index in 0..archive.len() {
            let mut member = archive.by_index(index).expect("NAVDB member");
            let name = member.name().to_string();
            let Some(page_index) = name
                .strip_prefix("page_")
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            let mut bytes = Vec::new();
            member.read_to_end(&mut bytes).expect("read NAVDB page");
            let resource_id = format!("nav_kv/page/{page_index:04}");
            let decoded = crate::decode_nav_db_page_resource_bytes(&resource_id, &bytes)
                .expect("decode NAVDB page");
            store.insert_page(page_index, decoded.into_owned());
        }
        store
    }

    fn nav_db_advance_fixture_root() -> std::path::PathBuf {
        std::env::var_os("AEROBAG_TEST_ARTIFACTS_ROOT")
            .map(std::path::PathBuf::from)
            .expect("set AEROBAG_TEST_ARTIFACTS_ROOT to run external fixture tests")
    }

    #[test]
    #[ignore = "requires the external NAVDB transition fixture"]
    fn real_nav_db_2607_to_2608_advance_preserves_rich_session() {
        let root = nav_db_advance_fixture_root();
        let fixture = root.join("nav-db/advance-2607-to-2608/source/packaged");
        let old_path = fixture.join(
            "nav_db_NAV12_2607_01_bcf5bb62d186a9f214a6fa027dde333441ae2676000116fadd30a21758d1022c.zip",
        );
        let next_path = fixture.join(
            "nav_db_NAV12_2608_01_193319fdd18ba981ebab22c25139e7ba0c3da3c080bdc12b63d20052c7572f5f.zip",
        );
        assert!(old_path.is_file(), "missing {}", old_path.display());
        assert!(next_path.is_file(), "missing {}", next_path.display());
        let old_store = load_nav_db_fixture_zip(&old_path);
        let next_store = load_nav_db_fixture_zip(&next_path);
        let base_plan = FlightPlan {
            id: "nav-db-advance-rich-plan".to_string(),
            name: "KRNT SEA KPAE VOR-A ECEPO".to_string(),
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
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(6_000),
            notes: Some("NAVDB advance regression".to_string()),
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let materialized = match crate::run_had_operation(
            &old_store,
            HadOperation::MaterializeProcedure {
                airport_id: "KPAE".to_string(),
                procedure_id: "VOR-A".to_string(),
                procedure_kind: ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition: Some("ECEPO".to_string()),
                component_index: 2,
            },
        )
        .expect("materialize 2607 procedure")
        {
            HadOperationOutcome::Complete { result, .. } => {
                serde_json::from_value::<MaterializedProcedure>(result)
                    .expect("decode materialized procedure")
            }
            outcome => panic!("fully loaded fixture unexpectedly faulted: {outcome:?}"),
        };
        let mutation = crate::insert_procedure_materialized_ui(&base_plan, 1, 2, materialized)
            .expect("insert KPAE VOR-A ECEPO");
        let plan = crate::activate_leg(&mutation.mutation.plan, 5)
            .expect("activate procedure hold inbound leg");
        let expected_plan = plan.clone();
        let airport_key = nav_kv_key_for_query(&NavKvQuery::PlateAirport {
            airport_id: "KPAE".to_string(),
        })
        .expect("KPAE plate key");
        let NavKvLookup::Hit(airport_bytes) = old_store
            .get_bytes(&airport_key)
            .expect("read KPAE plate folder")
        else {
            panic!("KPAE plate folder missing from 2607 fixture");
        };
        let airport: crate::chart_page::PlateAirportRecord =
            serde_json::from_slice(&airport_bytes).expect("decode KPAE plate folder");
        let selected_chart_id = airport
            .chart_ids
            .iter()
            .find(|chart_id| {
                read_chart_asset_by_id(&old_store, chart_id)
                    .is_ok_and(|chart| chart.label == "VOR-A")
            })
            .cloned()
            .expect("KPAE VOR-A chart");
        let init = create_ui_session(plan, &[], Some("KPAE"), Some(&selected_chart_id))
            .expect("create rich session");
        let mut old_open = nav_db_open_result_for_test("NAV_DB_NAV12_2607_01", None);
        old_open.selected_filename = old_path
            .file_name()
            .expect("2607 filename")
            .to_string_lossy()
            .to_string();
        old_open.selected_cycle = Some("2607".to_string());
        old_open.selected_contract_id = Some(REQUIRED_NAV_DB_CONTRACT_ID.to_string());
        attach_nav_kv_store_to_session_with_open_result(
            init.handle,
            2607,
            &old_store,
            Some(&old_open),
        )
        .expect("attach 2607 NAVDB");
        let catalog_before = snapshot_from_outcome(
            super::load_raster_map_catalog_in_session(init.handle).expect("load 2607 catalog"),
        )
        .raster_map
        .expect("2607 raster map");
        sync_guidance_geometry_in_session(init.handle).expect("build 2607 guidance");

        let mut next_open = nav_db_open_result_for_test("NAV_DB_NAV12_2608_01", None);
        next_open.selected_filename = next_path
            .file_name()
            .expect("2608 filename")
            .to_string_lossy()
            .to_string();
        next_open.selected_cycle = Some("2608".to_string());
        next_open.selected_contract_id = Some(REQUIRED_NAV_DB_CONTRACT_ID.to_string());
        let result = nav_db_advance_result(
            advance_nav_kv_store_in_session_with_open_result(
                init.handle,
                2608,
                &next_store,
                &next_open,
                vec!["NAV_DB_NAV12_2608_01".to_string()],
            )
            .expect("advance rich session to 2608"),
        );

        assert_eq!(result.disposition, NavDbAdvanceDisposition::Adopted);
        assert_eq!(result.snapshot.nav_data_epoch, 1);
        assert_eq!(
            result
                .snapshot
                .raster_map
                .as_ref()
                .map(|map| map.selected_family_id.as_str()),
            Some(catalog_before.selected_family_id.as_str())
        );
        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).expect("committed 2608 session");
        assert_eq!(session.app_state.active_plan.as_ref(), Some(&expected_plan));
        assert_eq!(session.nav_kv_store_id, Some(2608));
        assert!(!session.guidance_leg_geometry.is_empty());
        let route = crate::had_ops::project_flight_plan_route(
            session.nav_kv_store.as_ref().expect("committed NAVDB"),
            session.app_state.active_plan.as_ref().expect("active plan"),
        )
        .expect("project committed 2608 route");
        assert!(route
            .iter()
            .any(|segment| matches!(segment.geometry, crate::GuidanceRouteGeometry::Arc { .. })));
        assert!(route
            .iter()
            .any(|segment| { segment.status == crate::FlightPlanRouteSegmentStatus::Active }));
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

    fn complete_invalidations(outcome: HadOperationOutcome) -> Vec<UiInvalidation> {
        match outcome {
            HadOperationOutcome::Complete { invalidations, .. } => invalidations,
            HadOperationOutcome::NeedResources { resources } => {
                panic!("unexpected resource request: {resources:?}")
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic outcome unexpectedly requested snapshot continuation")
            }
        }
    }

    fn assert_only_terrain_overlay_invalidated(invalidations: &[UiInvalidation]) {
        assert!(invalidations.contains(&UiInvalidation::TerrainOverlay));
        assert!(!invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(!invalidations.contains(&UiInvalidation::SessionSnapshot));
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
            reference_assets: Vec::new(),
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
        let current_artifacts = format!(
            r#"[{{
                "schema_version": 1,
                "contracts": {{"nav-db": "{}"}},
                "artifact_roots": {{
                    "packaged": "published_packaged",
                    "unpacked": "published_unpacked"
                }},
                "bundles": []
            }}]"#,
            crate::REQUIRED_NAV_DB_CONTRACT_ID
        );
        ingest_resource_in_session(
            init.handle,
            "publication/current_artifacts",
            current_artifacts.as_bytes(),
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
                        "schema_version": 3,
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
    fn live_feed_current_refresh_completes_after_current_resource_ingest() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        configure_live_feed_source_in_session(init.handle, "http://feeds.example.test")
            .expect("configure live-feed source");

        let HadOperationOutcome::NeedResources { resources } =
            refresh_live_feed_current_in_session(init.handle).expect("refresh current")
        else {
            panic!("expected current manifest request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/current");

        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            br#"{
                "schema_version": 3,
                "products": {
                    "metars": {
                        "current": "v1",
                        "version_manifest_url": "versions/metars/v1.json",
                        "state_url": "states/metars/v1.json",
                        "state_sha256": "unused"
                    }
                }
            }"#,
        )
        .expect("ingest current manifest");

        let HadOperationOutcome::Complete { invalidations, .. } =
            refresh_live_feed_current_in_session(init.handle).expect("finish refresh current")
        else {
            panic!("current refresh should complete after current manifest ingest");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));

        let HadOperationOutcome::NeedResources { resources } =
            sync_live_feeds_in_session(init.handle).expect("sync live feeds")
        else {
            panic!("expected version manifest request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/version/metars/v1");
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
            observed_at_utc: None,
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
            crate::prepare_live_feed_state_resource(resource_id, &state_bytes)
                .expect("prepared metars");
        let prepared_envelope =
            crate::decode_prepared_live_feed(&prepared_bytes).expect("decode prepared metars");

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
                            "schema_version": 3,
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
        }
        ingest_prepared_live_feed_resource_in_session(init.handle, resource_id, &prepared_bytes)
            .expect("install prepared metars");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            assert!(session.prepared_metar_tiles.is_some());
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
    fn prepared_live_feed_products_swap_final_typed_state_into_session() {
        let cases = [
            (
                "tafs",
                serde_json::json!({
                    "schema_version": 1,
                    "version_label": "v1",
                    "taf_count": 1,
                    "tafs_by_station": {
                        "KSEA": {
                            "raw_text": "TAF KSEA",
                            "station_id": "KSEA",
                            "longitude": -122.3,
                            "latitude": 47.4
                        }
                    }
                }),
            ),
            (
                "tfrs",
                serde_json::json!({
                    "schema_version": 1,
                    "version_label": "v1",
                    "notam_count": 0,
                    "area_group_count": 0,
                    "areas": []
                }),
            ),
        ];

        for (product, state) in cases {
            let init =
                create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
            let resource_id = format!("live_feeds/state/{product}/v1");
            let encoded =
                nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&state).unwrap())
                    .unwrap();
            let (_, prepared_bytes) =
                crate::prepare_live_feed_state_resource(&resource_id, &encoded).unwrap();
            let envelope = crate::decode_prepared_live_feed(&prepared_bytes).unwrap();
            ingest_resource_in_session(
                init.handle,
                "live_feeds/current",
                format!(
                    r#"{{
                        "schema_version": 3,
                        "products": {{
                            "{product}": {{
                                "current": "v1",
                                "version_manifest_url": "versions/{product}/v1.json",
                                "state_url": "states/{product}/v1.json.xz",
                                "state_sha256": "{}"
                            }}
                        }}
                    }}"#,
                    envelope.state_sha256
                )
                .as_bytes(),
            )
            .unwrap();
            ingest_prepared_live_feed_resource_in_session(
                init.handle,
                &resource_id,
                &prepared_bytes,
            )
            .unwrap();

            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).expect("session");
            match product {
                "tafs" => assert_eq!(
                    session
                        .taf_payload
                        .as_ref()
                        .and_then(|payload| payload.tafs_by_station.get("KSEA"))
                        .map(|taf| taf.raw_text.as_str()),
                    Some("TAF KSEA")
                ),
                "tfrs" => assert_eq!(
                    session
                        .tfr_payload
                        .as_ref()
                        .map(|payload| payload.areas.len()),
                    Some(0)
                ),
                "notams" => assert_eq!(
                    session
                        .airport_notam_index
                        .as_ref()
                        .map(|index| index.version_label.as_str()),
                    Some("v1")
                ),
                _ => unreachable!(),
            }
            drop(sessions);
            destroy_session(init.handle);
        }
    }

    #[test]
    fn live_feed_tafs_install_weather_inspector_payload() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let state = serde_json::json!({
            "schema_version": 1,
            "version_label": "v1",
            "taf_count": 1,
            "tafs_by_station": {
                "KAAA": {
                    "raw_text": "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020",
                    "issued_at_utc": "2026-05-03T00:00:00.000Z",
                    "station_id": "KAAA",
                    "longitude": 0.0,
                    "latitude": 0.0
                }
            }
        });
        let state_bytes = serde_json::to_vec(&state).expect("state bytes");
        let state_sha256 = format!("{:x}", Sha256::digest(&state_bytes));

        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "schema_version": 3,
                    "products": {{
                        "tafs": {{
                            "current": "v1",
                            "version_manifest_url": "versions/tafs/v1.json",
                            "state_url": "states/tafs/v1.json",
                            "state_sha256": "{state_sha256}",
                            "collected_at_utc": "2026-05-03T00:05:00Z"
                        }}
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("current manifest");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/version/tafs/v1",
            format!(
                r#"{{
                    "schema_version": 3,
                    "product": "tafs",
                    "version": "v1",
                    "state": {{
                        "kind": "json",
                        "url": "states/tafs/v1.json",
                        "bytes": {},
                        "blob_sha256": "{state_sha256}",
                        "state_sha256": "{state_sha256}"
                    }}
                }}"#,
                state_bytes.len()
            )
            .as_bytes(),
        )
        .expect("version manifest");
        ingest_resource_in_session(init.handle, "live_feeds/state/tafs/v1", &state_bytes)
            .expect("tafs state");

        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).expect("session");
        let taf_payload = session.taf_payload.as_ref().expect("TAF payload");
        assert_eq!(taf_payload.version_label, "v1");
        assert_eq!(
            taf_payload.tafs_by_station["KAAA"].raw_text,
            "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020"
        );
        drop(sessions);

        let snapshot = get_session_snapshot_at_epoch_ms(
            init.handle,
            utc("2026-05-03T00:10:00Z").timestamp_millis(),
        )
        .expect("snapshot");
        let data_status = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:tafs")
            .expect("TAF data-status row");
        assert_eq!(data_status.label, "TAFs");
        assert_eq!(data_status.value, "OK");
        assert_eq!(data_status.severity, UiStatusSeverity::Ok);
        assert_eq!(data_status.detail, "TAFs is loaded.");
        assert_eq!(
            data_status
                .facts
                .iter()
                .find(|fact| fact.label == "Version")
                .map(|fact| fact.value.as_str()),
            Some("v1")
        );
        assert_eq!(
            data_status
                .facts
                .iter()
                .find(|fact| fact.label == "Collected At")
                .map(|fact| fact.value.as_str()),
            Some("2026-05-03 00:05 UTC")
        );
    }

    #[test]
    fn notam_postcondition_failure_discards_state_but_stale_base_preserves_it() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let record = |id: &str, text: &str| notam_state::NotamRecord {
            id: id.to_string(),
            airport_id: Some("KSEA".to_string()),
            airport_effects: [product_contracts::AirportNotamEffect::RoutineAdvisory]
                .into_iter()
                .collect(),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: None,
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        };
        let mut source = notam_state::NotamState::empty();
        source
            .apply_mutation(
                notam_state::NotamMutation::Upsert {
                    record: record("A", "initial"),
                },
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();
        let checkpoint = source.checkpoint();
        let base_id = source.state_id().to_string();
        let mutation = notam_state::NotamMutation::Upsert {
            record: record("B", "changed"),
        };
        source
            .apply_mutation(
                mutation.clone(),
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();

        let mut sessions = lock_sessions();
        let session = session_mut(&mut sessions, init.handle).expect("session");
        install_prepared_live_feed(
            session,
            crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::InstallCheckpoint(
                checkpoint.clone(),
            )),
        )
        .unwrap();
        let bad_target = notam_state::NotamDelta::new(
            base_id.clone(),
            "f".repeat(64),
            source.counters(),
            vec![mutation],
        );
        assert!(install_prepared_live_feed(
            session,
            crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::ApplyDelta(
                bad_target
            )),
        )
        .is_err());
        assert!(session.airport_notam_index.is_none());

        install_prepared_live_feed(
            session,
            crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::InstallCheckpoint(
                checkpoint,
            )),
        )
        .unwrap();
        let stale = notam_state::NotamDelta::new(
            "e".repeat(64),
            "d".repeat(64),
            notam_state::NotamCounters::default(),
            Vec::new(),
        );
        assert!(install_prepared_live_feed(
            session,
            crate::PreparedLiveFeedPayload::Notams(crate::PreparedNotamPayload::ApplyDelta(stale)),
        )
        .is_err());
        assert_eq!(
            session
                .airport_notam_index
                .as_ref()
                .map(AirportNotamIndex::state_id),
            Some(base_id.as_str())
        );
    }

    #[test]
    fn prepared_notam_checkpoint_behind_head_installs_before_delta_replay() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let record = |id: &str, text: &str| notam_state::NotamRecord {
            id: id.to_string(),
            airport_id: Some("KSEA".to_string()),
            airport_effects: [product_contracts::AirportNotamEffect::RoutineAdvisory]
                .into_iter()
                .collect(),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: None,
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        };
        let mut producer = notam_state::NotamState::empty();
        producer
            .apply_mutation(
                notam_state::NotamMutation::Upsert {
                    record: record("A", "checkpoint"),
                },
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();
        let checkpoint = producer.checkpoint();
        let checkpoint_id = checkpoint.state_id.clone();
        let mutation = notam_state::NotamMutation::Upsert {
            record: record("B", "delta"),
        };
        producer
            .apply_mutation(
                mutation.clone(),
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();
        let head_id = producer.state_id().to_string();
        let delta = notam_state::NotamDelta::new(
            checkpoint_id.clone(),
            head_id.clone(),
            producer.counters(),
            vec![mutation],
        );
        let checkpoint_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&checkpoint).unwrap())
                .unwrap();
        let delta_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();
        let checkpoint_resource = format!("live_feeds/state/notams/{checkpoint_id}");
        let delta_resource = format!("live_feeds/delta/notams/{checkpoint_id}/{head_id}");
        let (_, prepared_checkpoint) =
            crate::prepare_live_feed_state_resource(&checkpoint_resource, &checkpoint_bytes)
                .unwrap();
        let (_, prepared_delta) = crate::prepare_live_feed_delta_resource(
            &delta_resource,
            &serde_json::Value::Null,
            &delta_bytes,
        )
        .unwrap();
        let delta_ref = serde_json::json!({
            "kind": "notam_ordered_delta_xz",
            "from_version": checkpoint_id,
            "from_state_sha256": checkpoint_id,
            "to_version": head_id,
            "to_state_sha256": head_id,
            "url": format!("deltas/notams/{checkpoint_id}__{head_id}.json.xz"),
            "mutation_count": 1
        });

        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).unwrap();
            session
                .live_feeds
                .ingest_resource(
                    "live_feeds/current",
                    &serde_json::to_vec(&serde_json::json!({
                        "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
                        "products": {
                            "notams": {
                                "current": head_id,
                                "version_manifest_url": format!(
                                    "versions/notams/{head_id}.json"
                                ),
                                "state_url": format!(
                                    "states/notams/{checkpoint_id}.json.xz"
                                ),
                                "state_sha256": head_id
                            }
                        }
                    }))
                    .unwrap(),
                )
                .unwrap();
            session
                .live_feeds
                .ingest_resource(
                    &format!("live_feeds/version/notams/{head_id}"),
                    &serde_json::to_vec(&serde_json::json!({
                        "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
                        "product": "notams",
                        "version": head_id,
                        "state": {
                            "kind": "notam_checkpoint_xz",
                            "url": format!("states/notams/{checkpoint_id}.json.xz"),
                            "state_sha256": checkpoint_id
                        },
                        "delta_from_previous": delta_ref,
                        "recent_deltas": [delta_ref]
                    }))
                    .unwrap(),
                )
                .unwrap();
        }

        ingest_prepared_live_feed_resource_in_session(
            init.handle,
            &checkpoint_resource,
            &prepared_checkpoint,
        )
        .unwrap();
        {
            let sessions = lock_sessions();
            let session = sessions.get(&init.handle).unwrap();
            assert_eq!(
                session
                    .airport_notam_index
                    .as_ref()
                    .map(AirportNotamIndex::state_id),
                Some(checkpoint_id.as_str())
            );
            assert_eq!(
                session.live_feeds.product_staged_version("notams"),
                Some(checkpoint_id.as_str())
            );
            assert_eq!(session.live_feeds.product_loaded_version("notams"), None);
            let crate::HadOperationOutcome::NeedResources { resources } =
                session.live_feeds.sync_outcome()
            else {
                panic!("checkpoint should be followed by retained delta");
            };
            assert_eq!(resources[0].id, delta_resource);
        }

        ingest_prepared_live_feed_resource_in_session(
            init.handle,
            &delta_resource,
            &prepared_delta,
        )
        .unwrap();
        let sessions = lock_sessions();
        let session = sessions.get(&init.handle).unwrap();
        assert_eq!(
            session
                .airport_notam_index
                .as_ref()
                .map(AirportNotamIndex::state_id),
            Some(head_id.as_str())
        );
        assert_eq!(
            session.live_feeds.product_loaded_version("notams"),
            Some(head_id.as_str())
        );
    }

    #[test]
    fn live_feed_notams_install_airport_records() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let state = serde_json::json!({
            "schema_version": product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION,
            "version_label": "v1",
            "notam_count": 5,
            "notams_by_id": {
                "D:AAA:2026:N:1": {
                    "id": "D:AAA:2026:N:1",
                    "airport_id": "KAAA",
                    "airport_effects": ["surface_condition"],
                    "notam_keyword": "RWY",
                    "text": "RWY 10L FICON 5/5/5 100 PCT WET"
                },
                "D:AAA:2026:N:2": {
                    "id": "D:AAA:2026:N:2",
                    "airport_id": "KAAA",
                    "airport_effects": ["runway_closed"],
                    "notam_keyword": "RWY",
                    "text": "RWY 10L/28R CLSD"
                },
                "D:AAA:2026:N:3": {
                    "id": "D:AAA:2026:N:3",
                    "airport_id": "KAAA",
                    "airport_effects": ["surface_condition"],
                    "notam_keyword": "RWY",
                    "text": "RWY 10R FICON 5/5/5 100 PCT WET"
                },
                "D:AAA:2026:N:4": {
                    "id": "D:AAA:2026:N:4",
                    "airport_id": "KAAA",
                    "airport_effects": ["surface_condition"],
                    "notam_keyword": "RWY",
                    "text": "RWY 14 FICON 5/5/5 100 PCT WET"
                },
                "D:AAA:2026:N:5": {
                    "id": "D:AAA:2026:N:5",
                    "airport_id": null,
                    "airport_effects": [],
                    "notam_keyword": "NAV",
                    "text": "VOR U/S"
                }
            }
        });
        let mut records = state["notams_by_id"]
            .as_object()
            .expect("NOTAM records")
            .values()
            .map(|record| serde_json::from_value::<notam_state::NotamRecord>(record.clone()))
            .collect::<Result<Vec<_>, _>>()
            .expect("decode NOTAM records");
        records.sort_by(|left, right| left.id.cmp(&right.id));
        let mut notam_state = notam_state::NotamState::empty();
        let mut work = notam_state::NotamApplyWork::default();
        for record in records {
            notam_state
                .apply_mutation(notam_state::NotamMutation::Upsert { record }, &mut work)
                .expect("build NOTAM checkpoint");
        }
        let checkpoint = notam_state.checkpoint();
        let state_id = checkpoint.state_id.clone();
        let checkpoint_bytes = nav_kv_package::xz_frame_uncompressed_bytes(
            &serde_json::to_vec(&checkpoint).expect("checkpoint JSON"),
        )
        .expect("checkpoint XZ");
        let resource_id = format!("live_feeds/state/notams/{state_id}");
        let (_, prepared_bytes) =
            crate::prepare_live_feed_state_resource(&resource_id, &checkpoint_bytes)
                .expect("prepare NOTAM checkpoint");
        {
            let mut sessions = lock_sessions();
            let session = sessions.get_mut(&init.handle).expect("session");
            session
                .live_feeds
                .ingest_resource(
                    "live_feeds/current",
                    serde_json::to_string(&serde_json::json!({
                        "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
                        "products": {
                            "notams": {
                                "current": state_id,
                                "version_manifest_url": format!("versions/notams/{state_id}.json"),
                                "state_url": format!("states/notams/{state_id}.json.xz"),
                                "state_sha256": state_id,
                            }
                        }
                    }))
                    .expect("current JSON")
                    .as_bytes(),
                )
                .expect("install NOTAM current");
            session
                .live_feeds
                .ingest_resource(
                    &format!("live_feeds/version/notams/{state_id}"),
                    serde_json::to_string(&serde_json::json!({
                        "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
                        "product": "notams",
                        "version": state_id,
                        "state": {
                            "kind": "notam_checkpoint_xz",
                            "url": format!("states/notams/{state_id}.json.xz"),
                            "state_sha256": state_id,
                        }
                    }))
                    .expect("version JSON")
                    .as_bytes(),
                )
                .expect("install NOTAM version");
        }
        ingest_prepared_live_feed_resource_in_session(init.handle, &resource_id, &prepared_bytes)
            .expect("install NOTAM checkpoint");
        {
            let mut sessions = lock_sessions();
            let session = sessions.get_mut(&init.handle).expect("session");
            let index = session.airport_notam_index.as_ref().unwrap_or_else(|| {
                panic!("airport NOTAM index: {:?}", session.data_status_records)
            });
            assert_eq!(index.version_label, state_id);
            let detail = crate::map_overlay::weather_detail_for_station(
                "KAAA",
                None,
                None,
                Some(index),
                None,
            )
            .expect("airport NOTAM detail");
            assert_eq!(detail.notams.len(), 4);
            assert_eq!(detail.notams[0].text, "RWY 10L/28R CLSD");
        }
        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let status = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:notams")
            .expect("NOTAM status row");
        assert_eq!(status.label, "NOTAMs");
        assert_eq!(status.value, "CACHED");
        assert_eq!(status.severity, UiStatusSeverity::Info);
        assert!(status
            .facts
            .iter()
            .any(|fact| fact.label == "Version" && fact.value == state_id));
    }

    #[test]
    fn airport_flight_plan_rows_project_weather_action_from_session_payloads() {
        let plan = FlightPlan {
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KAAA".to_string()),
            }],
            route_component_uids: vec!["component-1".to_string()],
            route_component_uid_counter: 1,
            ..empty_test_plan()
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = sessions.get_mut(&init.handle).expect("session");
            let mut metars_by_station = HashMap::new();
            metars_by_station.insert(
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
            );
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "v1".to_string(),
                generated_at_utc: None,
                observed_at_utc: None,
                metar_count: Some(1),
                metars_by_station,
                pireps: Vec::new(),
            });
            let mut tafs_by_station = HashMap::new();
            tafs_by_station.insert(
                "KAAA".to_string(),
                crate::TafRecord {
                    raw_text: "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020".to_string(),
                    issued_at_utc: Some("2026-05-03T00:58:00.000Z".to_string()),
                    station_id: "KAAA".to_string(),
                    longitude: 0.0,
                    latitude: 0.0,
                },
            );
            session.taf_payload = Some(TafProductPayload {
                schema_version: 1,
                version_label: "v1".to_string(),
                generated_at_utc: None,
                taf_count: Some(1),
                tafs_by_station,
            });
            session.airport_notam_index = Some(
                AirportNotamIndex::from_payload(NotamProductPayload {
                    schema_version: product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION,
                    version_label: "v1".to_string(),
                    notam_count: Some(1),
                    notams_by_id: HashMap::from([(
                        "D:AAA:2026:N:1".to_string(),
                        crate::NotamRecord {
                            id: "D:AAA:2026:N:1".to_string(),
                            airport_id: Some("KAAA".to_string()),
                            airport_effects: BTreeSet::from([
                                product_contracts::AirportNotamEffect::RunwayClosed,
                            ]),
                            notam_keyword: Some("RWY".to_string()),
                            effective_start_utc: None,
                            effective_end_utc: None,
                            text: Some("RWY 18 CLSD".to_string()),
                            local_text: None,
                            icao_text: None,
                        },
                    )]),
                })
                .expect("supported NOTAM fixture"),
            );
        }

        let snapshot = get_session_snapshot_at_epoch_ms(
            init.handle,
            parse_utc_instant("2026-05-03T01:12:00Z")
                .expect("test time")
                .timestamp_millis(),
        )
        .expect("snapshot");
        let active_plan = snapshot.app_ui_state.active_plan.expect("active plan");
        let row = active_plan
            .display_rows
            .iter()
            .find(|row| row.nav_ref == Some(NavRef::Airport("KAAA".to_string())))
            .expect("KAAA row");
        let wx = crate::planning::flight_plan_row_actions(row)
            .find(|action| action.id == FlightPlanRowActionId::Weather)
            .expect("WX action");

        assert!(wx.enabled);
        assert_eq!(wx.label, "WX");
        let detail = wx.weather_detail.as_ref().expect("weather detail");
        assert_eq!(detail.station_id, "KAAA");
        assert_eq!(
            detail.metar_text.as_deref(),
            Some("METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000")
        );
        assert_eq!(detail.metar_age_label.as_deref(), Some("1.2h old"));
        assert!(detail.metar_age_warning);
        assert_eq!(
            detail.taf_text.as_deref(),
            Some("TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020")
        );
        assert_eq!(detail.taf_age_label.as_deref(), Some("14m old"));
        assert!(!detail.taf_age_warning);
        assert_eq!(detail.notams.len(), 1);
        assert_eq!(detail.notams[0].text, "RWY 18 CLSD");
    }

    #[test]
    fn durable_live_feed_tafs_install_uses_same_data_status_metadata() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let state = serde_json::json!({
            "schema_version": 1,
            "version_label": "v1",
            "taf_count": 1,
            "tafs_by_station": {
                "KAAA": {
                    "raw_text": "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020",
                    "issued_at_utc": "2026-05-03T00:00:00.000Z",
                    "station_id": "KAAA",
                    "longitude": 0.0,
                    "latitude": 0.0
                }
            }
        });
        let state_bytes = serde_json::to_vec(&state).expect("state bytes");
        let state_sha256 = format!("{:x}", Sha256::digest(&state_bytes));

        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "schema_version": 3,
                    "products": {{
                        "tafs": {{
                            "current": "v1",
                            "version_manifest_url": "versions/tafs/v1.json",
                            "state_url": "states/tafs/v1.json",
                            "state_sha256": "{state_sha256}",
                            "collected_at_utc": "2026-05-03T00:05:00Z"
                        }}
                    }}
                }}"#
            )
            .as_bytes(),
        )
        .expect("current manifest");

        install_live_feed_installed_state_in_session(
            init.handle,
            &crate::LiveFeedInstalledState {
                product: "tafs".to_string(),
                version: "v1".to_string(),
                state_sha256,
                payload: crate::LiveFeedInstalledPayload::Json { bytes: state_bytes },
            },
        )
        .expect("install durable tafs");

        let snapshot = get_session_snapshot_at_epoch_ms(
            init.handle,
            utc("2026-05-03T00:10:00Z").timestamp_millis(),
        )
        .expect("snapshot");
        let data_status = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:tafs")
            .expect("TAF data-status row");
        assert_eq!(data_status.label, "TAFs");
        assert_eq!(data_status.value, "OK");
        assert_eq!(data_status.severity, UiStatusSeverity::Ok);
        assert_eq!(data_status.detail, "TAFs is loaded.");
        assert_eq!(
            data_status
                .facts
                .iter()
                .find(|fact| fact.label == "Version")
                .map(|fact| fact.value.as_str()),
            Some("v1")
        );
        assert_eq!(
            data_status
                .facts
                .iter()
                .find(|fact| fact.label == "Collected At")
                .map(|fact| fact.value.as_str()),
            Some("2026-05-03 00:05 UTC")
        );
    }

    #[test]
    fn live_feed_cache_catalog_sync_adds_metadata_after_durable_startup_install() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let taf_state = serde_json::json!({
            "schema_version": 1,
            "version_label": "taf-v1",
            "taf_count": 1,
            "tafs_by_station": {
                "KAAA": {
                    "raw_text": "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020",
                    "issued_at_utc": "2026-05-03T00:00:00.000Z",
                    "station_id": "KAAA",
                    "longitude": 0.0,
                    "latitude": 0.0
                }
            }
        });
        let taf_bytes = serde_json::to_vec(&taf_state).expect("TAF state bytes");
        let taf_sha256 = format!("{:x}", Sha256::digest(&taf_bytes));
        let tfr_state = serde_json::json!({
            "schema_version": 1,
            "version_label": "tfr-v1",
            "notam_count": 0,
            "area_group_count": 0,
            "areas": []
        });
        let tfr_bytes = serde_json::to_vec(&tfr_state).expect("TFR state bytes");
        let tfr_sha256 = format!("{:x}", Sha256::digest(&tfr_bytes));

        install_live_feed_installed_state_in_session(
            init.handle,
            &crate::LiveFeedInstalledState {
                product: "tafs".to_string(),
                version: "taf-v1".to_string(),
                state_sha256: taf_sha256.clone(),
                payload: crate::LiveFeedInstalledPayload::Json { bytes: taf_bytes },
            },
        )
        .expect("install durable TAFs before current catalog");
        install_live_feed_installed_state_in_session(
            init.handle,
            &crate::LiveFeedInstalledState {
                product: "tfrs".to_string(),
                version: "tfr-v1".to_string(),
                state_sha256: tfr_sha256.clone(),
                payload: crate::LiveFeedInstalledPayload::Json { bytes: tfr_bytes },
            },
        )
        .expect("install durable TFRs before current catalog");

        let cached_snapshot = get_session_snapshot_at_epoch_ms(
            init.handle,
            utc("2026-05-03T00:04:00Z").timestamp_millis(),
        )
        .expect("cached snapshot before catalog sync");
        let cached_rows = &cached_snapshot.data_status_page_state.rows;
        let cached_tafs = cached_rows
            .iter()
            .find(|row| row.id == "live_feed:tafs")
            .expect("cached TAF data-status row");
        assert_eq!(cached_tafs.value, "CACHED");
        assert_eq!(cached_tafs.severity, UiStatusSeverity::Info);
        assert_eq!(
            cached_tafs.detail,
            "Cached TAFs live-feed data is available, but source timestamp is unknown."
        );
        assert!(cached_tafs
            .facts
            .iter()
            .all(|fact| fact.label != "Collected At"));
        let cached_tfrs = cached_rows
            .iter()
            .find(|row| row.id == "live_feed:tfrs")
            .expect("cached TFR data-status row");
        assert_eq!(cached_tfrs.value, "CACHED");
        assert_eq!(cached_tfrs.severity, UiStatusSeverity::Info);
        assert_eq!(
            cached_tfrs.detail,
            "Cached TFRs live-feed data is available, but source timestamp is unknown."
        );
        assert!(cached_tfrs
            .facts
            .iter()
            .all(|fact| fact.label != "Collected At"));

        let mut cache_catalog = crate::LiveFeedsState::default();
        let live_feeds_schema_version = crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION;
        cache_catalog
            .ingest_resource(
                "live_feeds/current",
                format!(
                    r#"{{
                        "schema_version": {live_feeds_schema_version},
                        "products": {{
                            "tafs": {{
                                "current": "taf-v1",
                                "version_manifest_url": "versions/tafs/taf-v1.json",
                                "state_url": "states/tafs/taf-v1.json",
                                "state_sha256": "{taf_sha256}",
                                "collected_at_utc": "2026-05-03T00:05:00Z"
                            }},
                            "tfrs": {{
                                "current": "tfr-v1",
                                "version_manifest_url": "versions/tfrs/tfr-v1.json",
                                "state_url": "states/tfrs/tfr-v1.json",
                                "state_sha256": "{tfr_sha256}",
                                "collected_at_utc": "2026-05-03T00:06:00Z"
                            }}
                        }}
                    }}"#
                )
                .as_bytes(),
            )
            .expect("cache current catalog");

        let snapshot = sync_live_feed_catalog_in_session(init.handle, &cache_catalog)
            .expect("sync cache catalog into session");
        let rows = &snapshot.data_status_page_state.rows;
        let tafs = rows
            .iter()
            .find(|row| row.id == "live_feed:tafs")
            .expect("TAF data-status row");
        assert_eq!(tafs.value, "OK");
        assert_eq!(
            tafs.facts
                .iter()
                .find(|fact| fact.label == "Collected At")
                .map(|fact| fact.value.as_str()),
            Some("2026-05-03 00:05 UTC")
        );
        let tfrs = rows
            .iter()
            .find(|row| row.id == "live_feed:tfrs")
            .expect("TFR data-status row");
        assert_eq!(tfrs.value, "OK");
        assert_eq!(
            tfrs.facts
                .iter()
                .find(|fact| fact.label == "Collected At")
                .map(|fact| fact.value.as_str()),
            Some("2026-05-03 00:06 UTC")
        );
    }

    #[test]
    fn live_metar_layer_survives_vector_manifest_without_weather_layers() {
        let mut session = UiSession {
            session_revision: 0,
            nav_data_epoch: 0,
            nav_db_advance_blocked: false,
            app_state: register_default_situation_sources(AppState::default()).expect("app state"),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            bad_autopilot: BadAutopilotState::default(),
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
                None,
            ),
            nav_kv_store_id: None,
            nav_kv_store: None,
            nav_db_artifact: None,
            map_layer_state: default_map_layer_state(),
            data_status_records: BTreeMap::new(),
            hushed_status_ids: BTreeSet::new(),
            data_status_state: default_data_status_state(),
            platform_capabilities: PlatformCapabilities::default(),
            settings_preferences: SettingsPreferences::default(),
            settings_storage: None,
            debug_state: default_debug_state(),
            resource_policy: CoreResourcePolicy::InstalledPackage,
            installed_package_ids: BTreeSet::new(),
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_tiles: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            nexrad_tile_cache: HashMap::new(),
            taf_payload: None,
            airport_notam_index: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
            live_feed_current_refresh: LiveFeedCurrentRefreshState::Idle,
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
            observed_at_utc: None,
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
            observed_at_utc: None,
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
            session_revision: 0,
            nav_data_epoch: 0,
            nav_db_advance_blocked: false,
            app_state: register_default_situation_sources(AppState::default()).expect("app state"),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            bad_autopilot: BadAutopilotState::default(),
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
                None,
            ),
            nav_kv_store_id: Some(1),
            nav_kv_store: Some(store),
            nav_db_artifact: None,
            map_layer_state: default_map_layer_state(),
            data_status_records: BTreeMap::new(),
            hushed_status_ids: BTreeSet::new(),
            data_status_state: default_data_status_state(),
            platform_capabilities: PlatformCapabilities::default(),
            settings_preferences: SettingsPreferences::default(),
            settings_storage: None,
            debug_state: default_debug_state(),
            resource_policy: CoreResourcePolicy::InstalledPackage,
            installed_package_ids: BTreeSet::new(),
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_tiles: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            nexrad_tile_cache: HashMap::new(),
            taf_payload: None,
            airport_notam_index: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
            live_feed_current_refresh: LiveFeedCurrentRefreshState::Idle,
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
                observed_at_utc: None,
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
    fn missing_vector_pages_block_map_overlay_until_loaded() {
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

        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("missing vector pages should block map overlay: {outcome:?}");
        };
        assert!(
            !resources.is_empty(),
            "missing vector pages should be returned through the paged operation contract"
        );
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

        for resource in resources {
            let page_index = crate::nav_kv_page_index_from_resource_id(&resource.id)
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
                observed_at_utc: None,
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
            session_revision: 0,
            nav_data_epoch: 0,
            nav_db_advance_blocked: false,
            app_state: register_default_situation_sources(AppState::default()).expect("app state"),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            bad_autopilot: BadAutopilotState::default(),
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
                None,
            ),
            nav_kv_store_id: None,
            nav_kv_store: None,
            nav_db_artifact: None,
            map_layer_state: default_map_layer_state(),
            data_status_records: BTreeMap::new(),
            hushed_status_ids: BTreeSet::new(),
            data_status_state: default_data_status_state(),
            platform_capabilities: PlatformCapabilities::default(),
            settings_preferences: SettingsPreferences::default(),
            settings_storage: None,
            debug_state: default_debug_state(),
            resource_policy: CoreResourcePolicy::InstalledPackage,
            installed_package_ids: BTreeSet::new(),
            publication_resolver: PublicationResolver::with_resource_policy(
                "/packages",
                CoreResourcePolicy::InstalledPackage,
            ),
            cycle_product_freshness: CycleProductFreshnessState::default(),
            live_feeds: LiveFeedsState::default(),
            live_feed_connection: LiveFeedConnectionSessionState::default(),
            raster_map_catalog: None,
            vector_tile_cache: HashMap::new(),
            metar_tile_cache: HashMap::new(),
            metar_payload: None,
            prepared_metar_tiles: None,
            important_metar_station_ids: None,
            metar_station_importance_status: None,
            obstacle_had: None,
            obstacle_tile_cache: HashMap::new(),
            nexrad_installed: None,
            nexrad_tile_cache: HashMap::new(),
            taf_payload: None,
            airport_notam_index: None,
            airspace_feature_cache: HashMap::new(),
            tfr_payload: None,
            terrain_source_tile_cache: HashMap::new(),
            pending_resource_effects: Vec::new(),
            wall_clock_epoch_ms: 0,
            live_feed_current_refresh: LiveFeedCurrentRefreshState::Idle,
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
                    "schema_version": 3,
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
                    "schema_version": 3,
                    "product": "tfrs",
                    "version": "bad",
                    "state": {{
                        "kind": "json",
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
                    "schema_version": 3,
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
                    "schema_version": 3,
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
                url: format!("{LIVE_FEEDS_BASE_PATH}/states/obstacles/v1/root"),
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
        let overlay = crate::query_map_overlay_for_surface(
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
            &[],
        );

        assert!(overlay
            .visible_features
            .iter()
            .any(|feature| feature.id == "obstacle:test"));
    }

    #[test]
    fn live_obstacle_current_change_drops_stale_had_before_new_pages_arrive() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let v1 = build_test_obstacle_had(&viewport, "v1", "obstacle:old");
        let v2 = build_test_obstacle_had(&viewport, "v2", "obstacle:new");
        let metrics = MapSurfaceMetrics::new(viewport, 240.0, 240.0, 1.0);

        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            install_vector_manifest_config(session, minimal_vector_manifest_json())
                .expect("vector manifest");
            session.map_layer_state.vectors.visible = true;
        }

        ingest_test_live_obstacle_state(init.handle, "v1", &v1);
        load_test_live_obstacle_had_pages(init.handle, "v1", &v1, &metrics);
        let feature_ids = query_obstacle_feature_ids(init.handle, &metrics);
        assert!(feature_ids.iter().any(|id| id == "obstacle:old"));

        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            format!(
                r#"{{
                    "schema_version": 3,
                    "products": {{
                        "obstacles": {{
                            "current": "v2",
                            "version_manifest_url": "versions/obstacles/v2.json",
                            "state_url": "states/obstacles/v2/manifest.json",
                            "state_sha256": "{}"
                        }}
                    }}
                }}"#,
                v2.state_sha256
            )
            .as_bytes(),
        )
        .expect("v2 current manifest");

        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            assert!(session.obstacle_had.is_none());
            assert!(session.obstacle_tile_cache.is_empty());
            assert!(session.map_overlay_config.obstacle_layer.is_none());
        }
        let feature_ids = query_obstacle_feature_ids(init.handle, &metrics);
        assert!(!feature_ids.iter().any(|id| id == "obstacle:old"));

        ingest_test_live_obstacle_state(init.handle, "v2", &v2);
        load_test_live_obstacle_had_pages(init.handle, "v2", &v2, &metrics);
        let feature_ids = query_obstacle_feature_ids(init.handle, &metrics);
        assert!(!feature_ids.iter().any(|id| id == "obstacle:old"));
        assert!(feature_ids.iter().any(|id| id == "obstacle:new"));
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
        let overlay = crate::query_map_overlay_for_surface(
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
            &[],
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
            "failed to fetch /live-feeds/v3/current.json: 404",
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
    fn map_layer_mutation_waits_for_snapshot_pages_before_committing() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        set_situation_in_session(
            init.handle,
            Situation {
                position: SituationPosition::LatLon {
                    lat: 48.54,
                    lon: -109.76,
                },
                orientation_deg: None,
                speed_kt: None,
                altitude_msl_ft: None,
            },
        )
        .expect("set ownship position");
        let (store, pages) = crate::navkv::nav_kv_store_without_pages_and_pages_for_test(
            &[
                ("magvar/48/-110", b"14"),
                ("magvar/48/-109", b"14"),
                ("magvar/49/-110", b"14"),
                ("magvar/49/-109", b"14"),
            ],
            256,
        );
        let store_id = 91_001;
        attach_nav_kv_store_to_session(init.handle, store_id, &store).expect("attach nav kv");
        let revision_before = {
            let sessions = lock_sessions();
            let session = session_ref(&sessions, init.handle).expect("session");
            assert!(!session.map_layer_state.nexrad.visible);
            session.session_revision
        };

        let outcome = super::set_map_layer_visibility_in_session(init.handle, "nexrad", true)
            .expect("request layer visibility");
        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("missing snapshot pages must suspend the layer command: {outcome:?}");
        };
        assert!(!resources.is_empty());
        {
            let sessions = lock_sessions();
            let session = session_ref(&sessions, init.handle).expect("session");
            assert!(!session.map_layer_state.nexrad.visible);
            assert_eq!(session.session_revision, revision_before);
        }

        for (page_index, page) in pages.iter().enumerate() {
            insert_nav_kv_page_for_attached_sessions(store_id, page_index as u32, page);
        }
        let snapshot = set_map_layer_visibility_in_session(init.handle, "nexrad", true)
            .expect("retry layer visibility");
        assert!(snapshot.map_layer_state.nexrad.visible);
        assert_eq!(snapshot.session_revision, revision_before + 1);
        destroy_session(init.handle);
    }

    #[test]
    fn committed_mutation_page_fault_resumes_snapshot_without_repeating_mutation() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let (store, pages) = crate::navkv::nav_kv_store_without_pages_and_pages_for_test(
            &[
                ("magvar/48/-110", b"14"),
                ("magvar/48/-109", b"14"),
                ("magvar/49/-110", b"14"),
                ("magvar/49/-109", b"14"),
            ],
            256,
        );
        let store_id = 91_002;
        attach_nav_kv_store_to_session(init.handle, store_id, &store).expect("attach nav kv");
        let position = LatLon {
            lat: 48.54,
            lon: -109.76,
        };

        let outcome = super::set_situation_in_session(
            init.handle,
            Situation {
                position: SituationPosition::LatLon {
                    lat: position.lat,
                    lon: position.lon,
                },
                orientation_deg: None,
                speed_kt: None,
                altitude_msl_ft: Some(3_000.0),
            },
        )
        .expect("set ownship position");
        let HadOperationOutcome::NeedSnapshotResources {
            resources,
            invalidations,
        } = outcome
        else {
            panic!("committed snapshot fault must request snapshot resources: {outcome:?}");
        };
        assert!(!resources.is_empty());
        assert!(invalidations.contains(&UiInvalidation::TerrainOverlay));
        {
            let sessions = lock_sessions();
            let session = session_ref(&sessions, init.handle).expect("session");
            assert_eq!(session.session_revision, 1);
            assert_eq!(session.app_state.ownship.render.position, Some(position));
        }

        for (page_index, page) in pages.iter().enumerate() {
            insert_nav_kv_page_for_attached_sessions(store_id, page_index as u32, page);
        }
        let snapshot = snapshot_from_outcome(
            super::get_session_snapshot(init.handle).expect("resume snapshot projection"),
        );
        assert_eq!(snapshot.session_revision, 1);
        assert_eq!(
            snapshot.app_ui_state.ownship.render.position,
            Some(position)
        );
        destroy_session(init.handle);
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
    fn nexrad_overlay_wire_status_is_state_tagged() {
        let value = serde_json::to_value(NexradOverlayQueryResult {
            status: NexradOverlayStatus::Ready { count: 96 },
            tiles: Vec::new(),
            stats: NexradOverlayStats::default(),
            animation: NexradOverlayAnimation::idle(),
        })
        .expect("serialize nexrad overlay");

        assert_eq!(
            value["status"],
            serde_json::json!({ "state": "ready", "count": 96 })
        );
        assert!(value["tiles"].as_array().expect("tiles array").is_empty());
        assert_eq!(value["stats"]["source_tile_count"], serde_json::json!(0));
    }

    fn nexrad_live_test_manifest(version: &str, observed_at_utc: &str) -> serde_json::Value {
        serde_json::json!({
            "product": "nexrad",
            "state_id": version,
            "observed_at_utc": observed_at_utc,
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
        })
    }

    fn ingest_nexrad_live_test_state(handle: u32, version: &str, manifest: &serde_json::Value) {
        let state_sha256 = canonical_json_sha256_value(manifest).expect("state hash");
        let version_manifest = serde_json::json!({
            "schema_version": 3,
            "product": "nexrad",
            "version": version,
            "state": {
                "kind": "json",
                "url": format!("states/nexrad/{version}/manifest.json"),
                "state_sha256": state_sha256,
            }
        });
        ingest_resource_in_session(
            handle,
            &format!("live_feeds/version/nexrad/{version}"),
            &serde_json::to_vec(&version_manifest).expect("version manifest json"),
        )
        .expect("ingest nexrad version");
        ingest_resource_in_session(
            handle,
            &format!("live_feeds/state/nexrad/{version}"),
            &serde_json::to_vec(manifest).expect("state manifest json"),
        )
        .expect("ingest nexrad state");
    }

    #[test]
    fn live_nexrad_history_drives_animation_and_frame_age_status() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let snapshot = get_session_snapshot(init.handle).expect("initial snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("initial nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("off"));

        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        let snapshot = get_session_snapshot(init.handle).expect("empty nexrad snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("empty nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("inop"));

        let versions = [
            ("nexrad-v1", "2026-05-20T12:00:00Z"),
            ("nexrad-v2", "2026-05-20T12:05:00Z"),
            ("nexrad-v3", "2026-05-20T12:10:00Z"),
            ("nexrad-v4", "2026-05-20T12:15:00Z"),
            ("nexrad-v5", "2026-05-20T12:20:00Z"),
            ("nexrad-v6", "2026-05-20T12:25:00Z"),
            ("nexrad-v7", "2026-05-20T12:30:00Z"),
        ];
        let manifests = versions
            .iter()
            .map(|(version, observed)| (*version, nexrad_live_test_manifest(version, observed)))
            .collect::<Vec<_>>();
        let history = manifests[..6]
            .iter()
            .map(|(version, manifest)| {
                let state_sha256 = canonical_json_sha256_value(manifest).expect("state hash");
                serde_json::json!({
                    "version": version,
                    "version_manifest_url": format!("versions/nexrad/{version}.json"),
                    "state_url": format!("states/nexrad/{version}/manifest.json"),
                    "state_sha256": state_sha256,
                })
            })
            .collect::<Vec<_>>();
        let current = serde_json::json!({
            "schema_version": 3,
            "products": {
                "nexrad": {
                    "current": "nexrad-v7",
                    "version_manifest_url": "versions/nexrad/nexrad-v7.json",
                    "history": history,
                }
            }
        });
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            &serde_json::to_vec(&current).expect("current json"),
        )
        .expect("ingest current");
        for (version, manifest) in &manifests {
            ingest_nexrad_live_test_state(init.handle, version, manifest);
        }

        let nominal_now = utc("2026-05-20T12:30:00Z").timestamp_millis();
        let cycle_ms = nexrad_animation_cycle_ms(versions.len());
        let now = nominal_now + (cycle_ms - nominal_now.rem_euclid(cycle_ms)).rem_euclid(cycle_ms);
        let outcome = get_nexrad_overlay_in_session_at_epoch_ms(
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
            now,
        )
        .expect("query nexrad");
        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("expected complete nexrad query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("nexrad result");
        assert_eq!(query.animation.phase, NexradOverlayAnimationPhase::Frame);
        assert_eq!(query.animation.selected_frame_index, Some(0));
        assert_eq!(query.animation.frame_count, 7);
        assert_eq!(
            query.animation.next_update_delay_ms,
            Some(NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS as u32)
        );
        assert_eq!(
            query.animation.next_update_epoch_ms,
            Some(now + NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS)
        );
        assert_eq!(
            query.animation.age_labels,
            vec!["30m ago", "25m ago", "20m ago", "15m ago", "10m ago", "5m ago", "0m ago"]
        );
        assert!(query
            .tiles
            .iter()
            .any(|tile| tile.src.contains("/states/nexrad/nexrad-v1/")));

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let nexrad_row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:nexrad")
            .expect("nexrad row");
        let frames_fact = nexrad_row
            .facts
            .iter()
            .find(|fact| fact.label == "Frames")
            .expect("frames fact");
        assert_eq!(
            frames_fact.value,
            "30m ago, 25m ago, 20m ago, 15m ago, 10m ago, 5m ago, 0m ago"
        );
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("nexrad age cell");
        assert_eq!(nexrad_age_cell.label, "NEXRAD");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("30m"));

        let outcome = get_nexrad_overlay_in_session_at_epoch_ms(
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
            now + NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS,
        )
        .expect("query second nexrad frame");
        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("expected complete second nexrad query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("second nexrad result");
        assert_eq!(query.animation.phase, NexradOverlayAnimationPhase::Frame);
        assert_eq!(query.animation.selected_frame_index, Some(1));
        assert_eq!(
            query.animation.next_update_delay_ms,
            Some(NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS as u32)
        );
        assert_eq!(
            query.animation.next_update_epoch_ms,
            Some(now + (2 * NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS))
        );
        let snapshot = get_session_snapshot(init.handle).expect("second-frame snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("second-frame nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("25m"));

        let latest_now =
            now + (versions.len() - 1) as i64 * NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS;
        let outcome = get_nexrad_overlay_in_session_at_epoch_ms(
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
            latest_now,
        )
        .expect("query latest nexrad frame");
        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("expected complete latest nexrad query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("latest nexrad result");
        assert_eq!(query.animation.phase, NexradOverlayAnimationPhase::Frame);
        assert_eq!(query.animation.selected_frame_index, Some(6));
        assert_eq!(
            query.animation.next_update_delay_ms,
            Some(NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS as u32)
        );
        assert_eq!(
            query.animation.next_update_epoch_ms,
            Some(latest_now + NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS)
        );
        let snapshot = get_session_snapshot(init.handle).expect("latest-frame snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("latest-frame nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("0m"));

        let blank_now = latest_now + NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS;
        let outcome = get_nexrad_overlay_in_session_at_epoch_ms(
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
            blank_now,
        )
        .expect("query blank nexrad frame");
        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("expected complete blank nexrad query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("blank nexrad result");
        assert_eq!(query.animation.phase, NexradOverlayAnimationPhase::Blank);
        assert_eq!(query.animation.selected_frame_index, None);
        assert_eq!(
            query.animation.next_update_delay_ms,
            Some(NEXRAD_ANIMATION_BLANK_DWELL_MS as u32)
        );
        assert_eq!(
            query.animation.next_update_epoch_ms,
            Some(blank_now + NEXRAD_ANIMATION_BLANK_DWELL_MS)
        );
        let snapshot = get_session_snapshot(init.handle).expect("blank snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("blank nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("---"));

        set_map_layer_visibility_in_session(init.handle, "nexrad", false).expect("hide nexrad");
        let snapshot = get_session_snapshot(init.handle).expect("hidden snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("hidden nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("off"));
    }

    #[test]
    fn live_nexrad_warning_uses_freshest_animation_frame_age() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");

        let frame_ages_min = [43_i64, 38, 33, 28, 23, 18, 13];
        let cycle_ms = nexrad_animation_cycle_ms(frame_ages_min.len());
        let now = 2_000_000 * cycle_ms;
        let manifests = frame_ages_min
            .iter()
            .enumerate()
            .map(|(index, age_min)| {
                let version = format!("nexrad-v{}", index + 1);
                let observed = utc_from_epoch_ms(now - age_min * 60_000)
                    .to_rfc3339_opts(SecondsFormat::Secs, true);
                let manifest = nexrad_live_test_manifest(&version, &observed);
                (version, manifest)
            })
            .collect::<Vec<_>>();
        let history = manifests[..manifests.len() - 1]
            .iter()
            .map(|(version, manifest)| {
                let state_sha256 = canonical_json_sha256_value(manifest).expect("state hash");
                serde_json::json!({
                    "version": version,
                    "version_manifest_url": format!("versions/nexrad/{version}.json"),
                    "state_url": format!("states/nexrad/{version}/manifest.json"),
                    "state_sha256": state_sha256,
                })
            })
            .collect::<Vec<_>>();
        let current_version = manifests
            .last()
            .map(|(version, _)| version.as_str())
            .expect("current version");
        let current = serde_json::json!({
            "schema_version": 3,
            "products": {
                "nexrad": {
                    "current": current_version,
                    "version_manifest_url": format!("versions/nexrad/{current_version}.json"),
                    "history": history,
                }
            }
        });
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            &serde_json::to_vec(&current).expect("current json"),
        )
        .expect("ingest current");
        for (version, manifest) in &manifests {
            ingest_nexrad_live_test_state(init.handle, version, manifest);
        }

        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let outcome =
            get_nexrad_overlay_in_session_at_epoch_ms(init.handle, viewport, 512.0, 512.0, now)
                .expect("query first stale frame");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete first stale frame query");
        };
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("nexrad result");
        assert_eq!(query.animation.selected_frame_index, Some(0));
        let snapshot = get_session_snapshot(init.handle).expect("first stale frame snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("43m"));
        let nexrad = data_status_box(&snapshot, LIVE_FEED_NEXRAD_STATUS_ID);
        assert_eq!(nexrad.value.as_deref(), Some("OLD"));
        assert_eq!(nexrad.detail, "NEXRAD data is 13m old.");

        let outcome = get_nexrad_overlay_in_session_at_epoch_ms(
            init.handle,
            viewport,
            512.0,
            512.0,
            now + NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS,
        )
        .expect("query second stale frame");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete second stale frame query");
        };
        let query: NexradOverlayQueryResult =
            serde_json::from_value(result).expect("second nexrad result");
        assert_eq!(query.animation.selected_frame_index, Some(1));
        let snapshot = get_session_snapshot(init.handle).expect("second stale frame snapshot");
        let nexrad_age_cell = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "nexrad_age")
            .expect("second nexrad age cell");
        assert_eq!(nexrad_age_cell.value.as_deref(), Some("38m"));
        let nexrad = data_status_box(&snapshot, LIVE_FEED_NEXRAD_STATUS_ID);
        assert_eq!(nexrad.detail, "NEXRAD data is 13m old.");
    }

    #[test]
    fn nexrad_tile_prepare_faults_and_caches_live_feed_tile_resource() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let src = "/live-feeds/v3/states/nexrad/state-v1/tiles/res3/0/0.png";

        let outcome = prepare_nexrad_tile_in_session(init.handle, src).expect("prepare tile");
        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("expected missing NEXRAD tile to fault a resource");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources[0].id,
            "live_feeds/nexrad_tile/state-v1/tiles/res3/0/0.png"
        );
        assert_eq!(resources[0].optional, false);
        assert_eq!(
            resources[0].source,
            CoreResourceSource::PublicUrl {
                url: src.to_string()
            }
        );

        ingest_resource_in_session(init.handle, &resources[0].id, b"png-bytes")
            .expect("ingest tile bytes");

        let outcome =
            prepare_nexrad_tile_in_session(init.handle, src).expect("prepare cached tile");
        assert!(matches!(outcome, HadOperationOutcome::Complete { .. }));
        let bytes = nexrad_tile_bytes_in_session(init.handle, src).expect("tile bytes");
        assert_eq!(bytes, b"png-bytes");
    }

    #[test]
    fn visible_nexrad_without_product_state_records_caution() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        set_map_layer_visibility_in_session(init.handle, "nexrad", true).expect("show nexrad");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            br#"{"schema_version":3,"products":{}}"#,
        )
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
                observed_at_utc: Some(utc("2020-01-01T00:00:00Z")),
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
                observed_at_utc: Some(utc("2026-05-20T11:55:00Z")),
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
    fn loaded_metar_feed_uses_top_level_timestamp_for_freshness_status() {
        let init = create_ui_session_at_epoch_ms(
            FlightPlan::default(),
            &[],
            None,
            None,
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("create session");
        set_map_layer_visibility_in_session(init.handle, "metars", true).expect("show metars");
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            session.metar_payload = Some(MetarProductPayload {
                schema_version: 3,
                version_label: "fresh-metars".to_string(),
                generated_at_utc: Some(utc("2026-05-20T11:55:00Z")),
                observed_at_utc: None,
                metar_count: Some(0),
                metars_by_station: HashMap::new(),
                pireps: Vec::new(),
            });
        }

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");

        assert!(
            !has_data_status_box(&snapshot, LIVE_FEED_METARS_STATUS_ID),
            "fresh METAR data with a top-level timestamp should not raise a chart warning"
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
                observed_at_utc: Some(utc("2026-05-20T11:45:00Z")),
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
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("overlay query unexpectedly requested snapshot continuation")
            }
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
                    "schema_version": 3,
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
                    "schema_version": 3,
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
        let version = "nexrad-old";
        let manifest = nexrad_live_test_manifest(version, "2020-01-01T00:00:00Z");
        ingest_resource_in_session(
            init.handle,
            "live_feeds/current",
            br#"{"schema_version":3,"products":{"nexrad":{"current":"nexrad-old","version_manifest_url":"versions/nexrad/nexrad-old.json"}}}"#,
        )
        .expect("ingest current");
        ingest_nexrad_live_test_state(init.handle, version, &manifest);

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
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete nexrad query");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));

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
    fn data_status_page_uses_current_chart_cycle_when_future_cycle_is_installed() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("SEC_NW_2606", "sec", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("TAC_NW_2606", "tac", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("SEC_NW_2607", "sec", Some("2026-06-11"), Some("2026-07-09")),
                package_record_json("TAC_NW_2607", "tac", Some("2026-06-11"), Some("2026-07-09")),
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
        assert!(row.detail.contains("valid until 2026-06-11 00:00 UTC"));
        assert!(!row.detail.contains("not valid yet"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next effective"
                && fact.value == "2026-06-11 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-06-11T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next expires"
                && fact.value == "2026-07-09 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-07-09T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
    }

    #[test]
    fn data_status_page_reports_next_chart_cycle_from_publication() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("SEC_NW_2606", "sec", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("TAC_NW_2606", "tac", Some("2026-05-14"), Some("2026-06-11")),
            ],
        );
        ingest_bundle_packages_for_test(
            init.handle,
            vec![
                package_record_json("SEC_NW_2607", "sec", Some("2026-06-11"), Some("2026-07-09")),
                package_record_json("TAC_NW_2607", "tac", Some("2026-06-11"), Some("2026-07-09")),
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
        assert!(row.detail.contains("valid until 2026-06-11 00:00 UTC"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next effective"
                && fact.value == "2026-06-11 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-06-11T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next expires"
                && fact.value == "2026-07-09 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-07-09T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
    }

    #[test]
    fn data_status_page_uses_current_docs_cycle_when_future_cycle_is_installed() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("TPP_2606", "tpp", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("CSUP_2606", "csup", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("TPP_2607", "tpp", Some("2026-06-11"), Some("2026-07-09")),
                package_record_json("CSUP_2607", "csup", Some("2026-06-11"), Some("2026-07-09")),
            ],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:airport_docs")
            .expect("airport docs row");
        assert_eq!(row.value, "OK");
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert!(row.detail.contains("valid until 2026-06-11 00:00 UTC"));
        assert!(!row.detail.contains("not valid yet"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next effective"
                && fact.value == "2026-06-11 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-06-11T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next expires"
                && fact.value == "2026-07-09 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-07-09T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
    }

    #[test]
    fn data_status_page_reports_next_docs_cycle_from_publication() {
        let init = create_current_test_session();
        attach_nav_db_package_records_for_test(
            init.handle,
            vec![
                package_record_json("TPP_2606", "tpp", Some("2026-05-14"), Some("2026-06-11")),
                package_record_json("CSUP_2606", "csup", Some("2026-05-14"), Some("2026-06-11")),
            ],
        );
        ingest_bundle_packages_for_test(
            init.handle,
            vec![
                package_record_json("TPP_2607", "tpp", Some("2026-06-11"), Some("2026-07-09")),
                package_record_json("CSUP_2607", "csup", Some("2026-06-11"), Some("2026-07-09")),
            ],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "cycle:airport_docs")
            .expect("airport docs row");
        assert_eq!(row.value, "OK");
        assert!(row.detail.contains("valid until 2026-06-11 00:00 UTC"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next effective"
                && fact.value == "2026-06-11 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-06-11T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next expires"
                && fact.value == "2026-07-09 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-07-09T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
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
                .ingest_resource_at_epoch_ms(
                    "publication/current_artifacts",
                    format!(
                        r#"[{{"schema_version":1,"contracts":{{"nav-db":"{}"}},"as_of_utc":"2026-05-20T12:00:00Z","artifact_roots":{{"packaged":"published_packaged","unpacked":"published_unpacked"}},"bundles":[]}}]"#,
                        crate::REQUIRED_NAV_DB_CONTRACT_ID
                    )
                    .as_bytes(),
                    checked_at,
                )
                .expect("ingest current artifacts");
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
    fn data_status_package_library_uses_offline_library_cache() {
        let checked_at = utc("2026-05-20T12:00:00Z").timestamp_millis();
        let init =
            create_ui_session_at_epoch_ms(FlightPlan::default(), &[], None, None, checked_at)
                .expect("create session");
        let discovery = serde_json::from_str::<crate::CurrentArtifactsManifest>(&format!(
            r#"{{"schema_version":1,"contracts":{{"nav-db":"{}"}},"as_of_utc":"2026-05-20T12:00:00Z","artifact_roots":{{"packaged":"published_packaged","unpacked":"published_unpacked"}},"bundles":[]}}"#,
            crate::REQUIRED_NAV_DB_CONTRACT_ID
        ))
        .expect("current artifacts");

        let snapshot = load_offline_package_library_cache_in_session(
            init.handle,
            OfflinePackagesLibraryCache {
                package_source_base_url: "https://aerobag.org/packages".to_string(),
                fetched_at_epoch_ms: checked_at,
                discovery_manifests: vec![discovery],
                bundle_manifests_by_filename: BTreeMap::new(),
            },
        )
        .expect("load package library cache");

        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "publication:current_artifacts")
            .expect("package library row");
        assert_eq!(row.value, "OK");
        assert_eq!(
            row.detail,
            "current_artifacts.json checked at 2026-05-20 12:00 UTC."
        );
    }

    #[test]
    fn data_status_page_reports_live_feed_connection_state() {
        let init = create_current_test_session();
        report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Connected,
                message: None,
                source_url: Some("http://aerobag-dev.iac.jonh.net:9085".to_string()),
                status_url: Some(
                    "http://aerobag-dev.iac.jonh.net:9085/live-feeds/status.html".to_string(),
                ),
                network_status: Some(LiveFeedNetworkStatus::Unmetered),
            },
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("connected");
        let snapshot = report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Message,
                message: None,
                source_url: None,
                status_url: None,
                network_status: None,
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
        let server = row
            .facts
            .iter()
            .find(|fact| fact.label == "Server")
            .expect("server URL fact");
        assert_eq!(server.value, "http://aerobag-dev.iac.jonh.net:9085");
        assert_eq!(
            server.link_url.as_deref(),
            Some("http://aerobag-dev.iac.jonh.net:9085/live-feeds/status.html")
        );
        assert!(row
            .facts
            .iter()
            .any(|fact| fact.label == "Network" && fact.value == "Unmetered"));
    }

    #[test]
    fn live_feed_runtime_backoff_is_session_owned() {
        let init = create_current_test_session();
        let first = live_feed_runtime_decision_in_session(
            init.handle,
            LiveFeedRuntimeInput {
                kind: crate::LiveFeedRuntimeEventKind::Error,
                message: Some("boom".to_string()),
                source_url: None,
                status_url: None,
                network_status: None,
            },
        )
        .expect("first decision");
        let second = live_feed_runtime_decision_in_session(
            init.handle,
            LiveFeedRuntimeInput {
                kind: crate::LiveFeedRuntimeEventKind::Error,
                message: Some("boom".to_string()),
                source_url: None,
                status_url: None,
                network_status: None,
            },
        )
        .expect("second decision");

        assert_eq!(first.reconnect_delay_ms, Some(5_000));
        assert_eq!(second.reconnect_delay_ms, Some(10_000));
    }

    #[test]
    fn data_status_page_prefers_metered_network_context_over_dns_error() {
        let init = create_current_test_session();
        let snapshot = report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Error,
                message: Some(
                    "Unable to resolve host \"aerobag.org\": No address associated with hostname"
                        .to_string(),
                ),
                source_url: Some("https://aerobag.org".to_string()),
                status_url: Some("https://aerobag.org/live-feeds/status.html".to_string()),
                network_status: Some(LiveFeedNetworkStatus::Metered),
            },
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("metered error");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:connection")
            .expect("connection row");

        assert_eq!(row.value, "METERED");
        assert_eq!(row.severity, UiStatusSeverity::Unavailable);
        assert!(row.detail.contains("active network is metered"));
        assert!(!row.detail.contains("Unable to resolve host"));
        assert!(row
            .facts
            .iter()
            .any(|fact| fact.label == "Network" && fact.value == "Metered"));
        assert!(row
            .facts
            .iter()
            .any(|fact| fact.label == "Error" && fact.value.contains("metered")));
    }

    #[test]
    fn data_status_page_reports_metered_network_status_event() {
        let init = create_current_test_session();
        report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Connected,
                message: None,
                source_url: Some("https://aerobag.org".to_string()),
                status_url: Some("https://aerobag.org/live-feeds/status.html".to_string()),
                network_status: Some(LiveFeedNetworkStatus::Unmetered),
            },
            utc("2026-05-20T12:00:00Z").timestamp_millis(),
        )
        .expect("connected");
        let snapshot = report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::NetworkStatus,
                message: None,
                source_url: Some("https://aerobag.org".to_string()),
                status_url: Some("https://aerobag.org/live-feeds/status.html".to_string()),
                network_status: Some(LiveFeedNetworkStatus::Metered),
            },
            utc("2026-05-20T12:01:00Z").timestamp_millis(),
        )
        .expect("metered status");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:connection")
            .expect("connection row");

        assert_eq!(row.value, "METERED");
        assert_eq!(row.severity, UiStatusSeverity::Unavailable);
        assert!(row.detail.contains("active network is metered"));
        assert!(row
            .facts
            .iter()
            .any(|fact| fact.label == "Network" && fact.value == "Metered"));
    }

    #[test]
    fn data_status_page_open_live_feed_connection_reports_resource_error() {
        let init = create_current_test_session();
        ingest_resource_in_session_at_epoch_ms(
            init.handle,
            "live_feeds/current",
            br#"{
                "schema_version": 1,
                "products": {}
            }"#,
            utc("2026-05-20T12:01:00Z").timestamp_millis(),
        )
        .expect("unsupported live-feed resource should be captured as status");

        let snapshot = report_live_feed_connection_event_in_session(
            init.handle,
            LiveFeedConnectionEvent {
                kind: LiveFeedConnectionEventKind::Connected,
                message: None,
                source_url: Some("http://aerobag-dev.iac.jonh.net:9085".to_string()),
                status_url: Some(
                    "http://aerobag-dev.iac.jonh.net:9085/live-feeds/status.html".to_string(),
                ),
                network_status: Some(LiveFeedNetworkStatus::Unmetered),
            },
            utc("2026-05-20T12:02:00Z").timestamp_millis(),
        )
        .expect("connected");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "live_feed:connection")
            .expect("connection row");

        assert_eq!(row.value, "ERROR");
        assert_eq!(row.severity, UiStatusSeverity::Unavailable);
        assert!(row.detail.contains(
            "The live-feed event stream is connected, but live-feed data is unavailable"
        ));
        assert!(row
            .detail
            .contains("current manifest has schema_version 1; client requires schema_version 3"));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Error"
                && fact.value.contains(
                    "current manifest has schema_version 1; client requires schema_version 3",
                )
        }));
    }

    #[test]
    fn data_status_page_reports_expected_contract_versions() {
        let init = create_current_test_session();

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "contracts:expected")
            .expect("expected contract versions row");

        assert_eq!(row.label, "Contract versions");
        assert_eq!(
            row.value,
            product_contracts::PRODUCT_CONTRACTS.len().to_string()
        );
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert_eq!(
            row.facts
                .iter()
                .find(|fact| fact.label == "NAV DB")
                .map(|fact| fact.value.as_str()),
            Some(crate::REQUIRED_NAV_DB_CONTRACT_ID)
        );
    }

    #[test]
    fn data_status_page_reports_client_build_identity() {
        let init = create_current_test_session();
        let snapshot = configure_platform_capabilities_in_session(
            init.handle,
            PlatformCapabilities {
                client_build: Some(ClientBuildInfo {
                    platform: "Web".to_string(),
                    version: "0.1.202606201031+a04240eb.dirty".to_string(),
                    built_at_utc: Some("2026-06-20T10:31:42Z".to_string()),
                    commit: Some("a04240ebcafefeed".to_string()),
                    dirty: true,
                }),
                ..PlatformCapabilities::default()
            },
            None,
        )
        .expect("configure platform capabilities");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "client")
            .expect("client row");

        assert_eq!(snapshot.data_status_page_state.title, "Status");
        assert_eq!(row.label, "Client");
        assert_eq!(row.value, "0.1.202606201031+a04240eb.dirty");
        assert_eq!(row.severity, UiStatusSeverity::Ok);
        assert!(row.detail.contains("dirty worktree"));
        assert_eq!(
            row.facts
                .iter()
                .find(|fact| fact.label == "Platform")
                .map(|fact| fact.value.as_str()),
            Some("Web")
        );
        assert_eq!(
            row.facts
                .iter()
                .find(|fact| fact.label == "Built")
                .map(|fact| fact.value.as_str()),
            Some("2026-06-20 10:31 UTC")
        );
        assert_eq!(
            row.facts
                .iter()
                .find(|fact| fact.label == "Commit")
                .map(|fact| fact.value.as_str()),
            Some("a04240ebcafefeed")
        );
        assert_eq!(
            row.facts
                .iter()
                .find(|fact| fact.label == "Worktree")
                .map(|fact| fact.value.as_str()),
            Some("dirty")
        );
    }

    #[test]
    fn data_status_page_orders_core_tiles_for_scanability() {
        let init = create_current_test_session();

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row_ids = snapshot
            .data_status_page_state
            .rows
            .iter()
            .take(14)
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            row_ids,
            vec![
                "client",
                "publication:current_artifacts",
                "contracts:expected",
                "nav_db",
                "cycle:charts",
                "cycle:airport_docs",
                "static:base_data",
                "live_feed:connection",
                "live_feed:tfrs",
                "live_feed:notams",
                "live_feed:metars",
                "live_feed:tafs",
                "live_feed:nexrad",
                "live_feed:obstacles",
            ]
        );
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
    fn data_status_page_reports_next_nav_db_cycle_from_publication() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut open_result =
            nav_db_open_result_for_test("NAV_DB_2606", Some("2026-06-11T00:00:00Z"));
        open_result.selected_effective_date = Some("2026-05-14T00:00:00Z".to_string());
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");
        ingest_bundle_packages_for_test(
            init.handle,
            vec![
                package_record_json(
                    "NAV_DB_2606",
                    "nav-db",
                    Some("2026-05-14T00:00:00Z"),
                    Some("2026-06-11T00:00:00Z"),
                ),
                package_record_json(
                    "NAV_DB_2607",
                    "nav-db",
                    Some("2026-06-11T00:00:00Z"),
                    Some("2026-07-09T00:00:00Z"),
                ),
            ],
        );

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let row = snapshot
            .data_status_page_state
            .rows
            .iter()
            .find(|row| row.id == "nav_db")
            .expect("nav-db page row");
        assert_eq!(row.value, "OK");
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next effective"
                && fact.value == "2026-06-11 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-06-11T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
        assert!(row.facts.iter().any(|fact| {
            fact.label == "Next expires"
                && fact.value == "2026-07-09 00:00 UTC"
                && fact.time_utc.as_deref() == Some("2026-07-09T00:00:00Z")
                && fact.time_display == Some(UiDataStatusPageTimeDisplay::Until)
        }));
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
    fn nav_db_family_ui_warning_records_warning() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(
            &[(
                "resource/families",
                br#"[
                    {
                        "id": "sec",
                        "display_name": "Sectional",
                        "kind": "tiled_raster",
                        "ui_warning": {
                            "severity": "warning",
                            "label": "SECTIONAL",
                            "value": "SUNSET",
                            "detail": "This sectional family format is being sunsetted."
                        }
                    }
                ]"#,
            )],
            1024,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:family:sec");
        assert_eq!(warning.label, "SECTIONAL");
        assert_eq!(warning.value.as_deref(), Some("SUNSET"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert!(warning.drives_caution);
        assert!(warning.detail.contains("sunsetted"));
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:family:sec"
                && row.value == "SUNSET"
                && row.detail.contains("sunsetted")
        }));
    }

    #[test]
    fn nav_db_family_warning_text_records_warning_and_status_page_row() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(
            &[(
                "resource/families",
                br#"[
                    {
                        "id": "enr-h",
                        "display_name": "IFR High",
                        "kind": "tiled_raster",
                        "warning_text": "This IFR-high chart has a sample warning."
                    }
                ]"#,
            )],
            1024,
        );
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:family:enr-h");
        assert_eq!(warning.label, "IFR-H");
        assert_eq!(warning.value.as_deref(), Some("WARNING"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert!(warning.drives_caution);
        assert_eq!(warning.detail, "This IFR-high chart has a sample warning.");
        assert!(snapshot.data_status_page_state.rows.iter().any(|row| {
            row.id == "package_ui_warning:family:enr-h"
                && row.label == "IFR-H"
                && row.value == "WARNING"
                && row.detail == "This IFR-high chart has a sample warning."
        }));
    }

    #[test]
    fn nav_db_family_warning_text_dedupes_regional_packages() {
        let init = create_current_test_session();
        let packages = vec![
            serde_json::json!({
                "id": "AK_ENR_H_ENH1_2605",
                "family_id": "enr-h"
            }),
            serde_json::json!({
                "id": "NW_ENR_H_ENH1_2605",
                "family_id": "enr-h"
            }),
        ];
        let family_bytes = br#"[
            {
                "id": "enr-h",
                "display_name": "IFR High",
                "kind": "tiled_raster",
                "warning_text": "This IFR-high chart has a sample warning."
            }
        ]"#;
        let mut entries = packages
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
        entries.push(("resource/families".to_string(), family_bytes.to_vec()));
        let entry_refs = entries
            .iter()
            .map(|(key, bytes)| (key.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>();
        let store = crate::navkv::nav_kv_store_for_test(&entry_refs, 2048);
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warnings = snapshot
            .data_status_page_state
            .rows
            .iter()
            .filter(|row| row.id == "package_ui_warning:family:enr-h")
            .count();
        assert_eq!(warnings, 1);
    }

    #[test]
    fn selected_nav_db_warning_text_records_warning() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut open_result = nav_db_open_result_for_test("NAV_DB_SAMPLE", Some("2026-06-11"));
        open_result.selected_warning_text = Some("This NAV-DB is getting moldy.".to_string());
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let warning = data_status_box(&snapshot, "package_ui_warning:NAV_DB_SAMPLE");
        assert_eq!(warning.label, "NAV DB");
        assert_eq!(warning.value.as_deref(), Some("WARNING"));
        assert_eq!(warning.severity, UiStatusSeverity::Warning);
        assert_eq!(warning.detail, "This NAV-DB is getting moldy.");
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
    fn cycle_product_freshness_recomputes_when_expiration_deadline_passes() {
        let init = create_current_test_session();
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let expiration = utc("2026-05-21T00:00:00Z");
        let open_result = nav_db_open_result_for_test("NAV_DB_TEST", Some("2026-05-21T00:00:00Z"));
        attach_nav_kv_store_to_session_with_open_result(init.handle, 1, &store, Some(&open_result))
            .expect("attach nav kv");
        let snapshot = get_session_snapshot(init.handle).expect("fresh snapshot");
        assert!(!has_data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID));
        assert_eq!(
            snapshot.next_cycle_product_freshness_check_epoch_ms,
            Some(expiration.timestamp_millis())
        );
        {
            let mut sessions = lock_sessions();
            let session = session_mut(&mut sessions, init.handle).expect("session");
            assert!(!session.cycle_product_freshness.dirty);
            session.wall_clock_epoch_ms = utc("2026-05-21T00:00:01Z").timestamp_millis();
        }

        let snapshot = get_session_snapshot(init.handle).expect("expired snapshot");
        let nav_db = data_status_box(&snapshot, CYCLE_NAV_DB_STATUS_ID);
        assert_eq!(nav_db.label, "NAV DB");
        assert_eq!(nav_db.value.as_deref(), Some("EXPIRED"));
        assert_eq!(snapshot.next_cycle_product_freshness_check_epoch_ms, None);
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
    fn playback_altitude_bucket_change_invalidates_only_terrain_overlay() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");

        let load_invalidations = complete_invalidations(
            super::load_playback_trace_in_session(
                init.handle,
                "test-trace.json",
                r#"{"trace":[[0.0,47.0,-122.0,0,100.0,90.0],[10.0,47.1,-122.1,1000,100.0,90.0]]}"#,
            )
            .expect("load replay trace"),
        );
        assert_only_terrain_overlay_invalidated(&load_invalidations);

        let play_invalidations = complete_invalidations(
            super::play_playback_in_session(init.handle, 0.0).expect("play replay"),
        );
        assert!(play_invalidations.is_empty());

        let tick_invalidations = complete_invalidations(
            super::tick_playback_in_session(init.handle, 10_000.0).expect("tick replay"),
        );
        assert_only_terrain_overlay_invalidated(&tick_invalidations);
    }

    #[test]
    fn terrain_source_resource_ingest_decodes_gzip_package_member_payload() {
        use std::io::Write;

        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        let samples = [40_i16; 4];
        let mut raw_abt2 = Vec::new();
        raw_abt2.extend_from_slice(b"ABT2");
        raw_abt2.extend_from_slice(&2_u16.to_le_bytes());
        raw_abt2.extend_from_slice(&2_u16.to_le_bytes());
        raw_abt2.extend_from_slice(&(-32768_i16).to_le_bytes());
        raw_abt2.extend_from_slice(&0_i16.to_le_bytes());
        raw_abt2.extend_from_slice(
            &(product_contracts::TERRAIN_TER2_HEIGHT_QUANTIZATION_FT as f32).to_le_bytes(),
        );
        raw_abt2.extend_from_slice(&0.0_f32.to_le_bytes());
        for (index, sample) in samples.iter().copied().enumerate() {
            let x = index % 2;
            let y = index / 2;
            let prediction = match (x, y) {
                (0, 0) => 0_u16,
                (_, 0) => samples[index - 1] as u16,
                (0, _) => samples[index - 2] as u16,
                _ => (samples[index - 1] as u16)
                    .wrapping_add(samples[index - 2] as u16)
                    .wrapping_sub(samples[index - 3] as u16),
            };
            raw_abt2.extend_from_slice(&(sample as u16).wrapping_sub(prediction).to_le_bytes());
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&raw_abt2)
            .expect("write gzip terrain tile");
        let gzip_abt2 = encoder.finish().expect("finish gzip terrain tile");

        ingest_resource_in_session(
            init.handle,
            "terrain/source/terrain-sw/tiles/9/1/2.terrain",
            &gzip_abt2,
        )
        .expect("ingest gzip terrain tile");
        let raw_rgba = render_terrain_overlay_tile_by_key_in_session(
            init.handle,
            "terrain/tiles/9/1/2.terrain",
            Some(2000.0),
        )
        .expect("render terrain tile by key");

        assert_eq!(u16::from_le_bytes([raw_rgba[0], raw_rgba[1]]), 2);
        assert_eq!(u16::from_le_bytes([raw_rgba[2], raw_rgba[3]]), 2);
        assert_eq!(&raw_rgba[4..8], &[185, 0, 45, 190]);
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
                    display_label: None,
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
            HadOperationOutcome::NeedSnapshotResources { resources, .. } => {
                panic!("unexpected session snapshot continuation request: {resources:?}")
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

    fn modda_zgood_normy_plan() -> FlightPlan {
        FlightPlan {
            id: "modda-zgood-normy".to_string(),
            name: "MODDA ZGOOD NORMY".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("MODDA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ZGOOD".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("NORMY".to_string()),
                },
            ],
            route_component_uids: vec![
                "row-modda".to_string(),
                "row-zgood".to_string(),
                "row-normy".to_string(),
            ],
            route_component_uid_counter: 3,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "leg-modda-zgood".to_string(),
                    from: NavRef::Fix("MODDA".to_string()),
                    to: NavRef::Fix("ZGOOD".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-zgood-normy".to_string(),
                    from: NavRef::Fix("ZGOOD".to_string()),
                    to: NavRef::Fix("NORMY".to_string()),
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

    fn modda_zgood_normy_nav_kv_store() -> NavKvStore {
        crate::navkv::nav_kv_store_for_test(
            &[
                (
                    "navref/position/fix/MODDA",
                    br#"{"lat":40.0,"lon":-120.0}"# as &[u8],
                ),
                (
                    "navref/position/fix/ZGOOD",
                    br#"{"lat":40.0,"lon":-119.95}"# as &[u8],
                ),
                (
                    "navref/position/fix/NORMY",
                    br#"{"lat":40.05,"lon":-119.95}"# as &[u8],
                ),
            ],
            256,
        )
    }

    fn create_synced_modda_zgood_normy_session() -> UiSessionInitResult {
        let store = modda_zgood_normy_nav_kv_store();
        let init =
            create_ui_session(modda_zgood_normy_plan(), &[], None, None).expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let sync = sync_guidance_geometry_in_session(init.handle).expect("sync guidance geometry");
        assert!(
            matches!(sync, HadOperationOutcome::Complete { .. }),
            "named-fix guidance geometry should sync from the test nav store"
        );
        init
    }

    fn twf_v4_ykm_chins_kpae_plan(pdt_nav_ref: NavRef) -> FlightPlan {
        FlightPlan {
            id: "twf-v4-ykm-chins-kpae".to_string(),
            name: "TWF ALKAL V4 YKM CHINS KPAE".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("TWF".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ALKAL".to_string()),
                },
                RouteComponent::Airway {
                    airway: crate::AirwaySegment {
                        name: "V4".to_string(),
                        branch_key: Some("V4-TWF-YKM".to_string()),
                        entry: NavRef::Fix("ALKAL".to_string()),
                        exit: NavRef::Navaid("YKM".to_string()),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("CHINS".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            route_component_uids: vec![
                "row-twf".to_string(),
                "row-alkal".to_string(),
                "row-v4".to_string(),
                "row-chins".to_string(),
                "row-kpae".to_string(),
            ],
            route_component_uid_counter: 5,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "leg-twf-alkal".to_string(),
                    from: NavRef::Navaid("TWF".to_string()),
                    to: NavRef::Fix("ALKAL".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v4-alkal-spuud".to_string(),
                    from: NavRef::Fix("ALKAL".to_string()),
                    to: NavRef::Fix("SPUUD".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v4-spuud-pdt".to_string(),
                    from: NavRef::Fix("SPUUD".to_string()),
                    to: pdt_nav_ref.clone(),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v4-pdt-cordo".to_string(),
                    from: pdt_nav_ref,
                    to: NavRef::Fix("CORDO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v4-cordo-ykm".to_string(),
                    from: NavRef::Fix("CORDO".to_string()),
                    to: NavRef::Navaid("YKM".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-ykm-chins".to_string(),
                    from: NavRef::Navaid("YKM".to_string()),
                    to: NavRef::Fix("CHINS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-chins-kpae".to_string(),
                    from: NavRef::Fix("CHINS".to_string()),
                    to: NavRef::Airport("KPAE".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 3 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 2,
                active_detail_index: Some(2),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(8000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn twf_v4_ykm_chins_kpae_nav_kv_store() -> NavKvStore {
        crate::navkv::nav_kv_store_for_test(
            &[
                (
                    "navref/position/navaid/TWF",
                    br#"{"lat":40.0,"lon":-120.4}"# as &[u8],
                ),
                (
                    "navref/position/fix/ALKAL",
                    br#"{"lat":40.0,"lon":-120.3}"# as &[u8],
                ),
                (
                    "navref/position/fix/SPUUD",
                    br#"{"lat":40.0,"lon":-120.2}"# as &[u8],
                ),
                (
                    "navref/position/fix/PDT",
                    br#"{"lat":40.0,"lon":-120.1}"# as &[u8],
                ),
                (
                    "navref/position/navaid/PDT",
                    br#"{"lat":40.0,"lon":-120.1}"# as &[u8],
                ),
                (
                    "navref/position/fix/CORDO",
                    br#"{"lat":40.0,"lon":-120.0}"# as &[u8],
                ),
                (
                    "navref/position/navaid/YKM",
                    br#"{"lat":40.0,"lon":-119.9}"# as &[u8],
                ),
                (
                    "navref/position/fix/CHINS",
                    br#"{"lat":40.0,"lon":-119.8}"# as &[u8],
                ),
                (
                    "navref/position/airport/KPAE",
                    br#"{"lat":40.0,"lon":-119.7}"# as &[u8],
                ),
            ],
            256,
        )
    }

    fn create_synced_twf_v4_ykm_chins_kpae_session(pdt_nav_ref: NavRef) -> UiSessionInitResult {
        let store = twf_v4_ykm_chins_kpae_nav_kv_store();
        let init = create_ui_session(twf_v4_ykm_chins_kpae_plan(pdt_nav_ref), &[], None, None)
            .expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let sync = sync_guidance_geometry_in_session(init.handle).expect("sync guidance geometry");
        assert!(
            matches!(sync, HadOperationOutcome::Complete { .. }),
            "TWF V4 guidance geometry should sync from the test nav store"
        );
        init
    }

    fn activate_direct_to_modda_row(handle: u32) {
        let before_direct_to = get_session_snapshot(handle).expect("snapshot before direct-to");
        let modda_row = before_direct_to
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("plan ui")
            .display_rows
            .iter()
            .find(|row| row.nav_ref == Some(NavRef::Fix("MODDA".to_string())))
            .expect("MODDA row")
            .clone();
        let direct_to_action = crate::planning::flight_plan_row_actions(&modda_row)
            .find(|action| action.id == FlightPlanRowActionId::DirectTo)
            .expect("direct-to action")
            .clone();
        let direct_to =
            perform_flight_plan_row_action_in_session(handle, modda_row.uid, direct_to_action.uid)
                .expect("direct-to MODDA");
        assert_session_snapshot_invalidated(direct_to);
    }

    fn session_route_segments(handle: u32) -> Vec<crate::FlightPlanRouteSegment> {
        let route = project_flight_plan_route_in_session(handle).expect("project route");
        let HadOperationOutcome::Complete { result, .. } = route else {
            panic!("route unexpectedly needed resources");
        };
        serde_json::from_value(result).expect("route segments")
    }

    fn session_route_statuses(handle: u32) -> Vec<crate::FlightPlanRouteSegmentStatus> {
        session_route_segments(handle)
            .iter()
            .map(|segment| segment.status.clone())
            .collect()
    }

    fn session_route_status_by_leg_id(
        handle: u32,
        leg_id: &str,
    ) -> crate::FlightPlanRouteSegmentStatus {
        session_route_segments(handle)
            .into_iter()
            .find(|segment| segment.leg_id == leg_id)
            .unwrap_or_else(|| panic!("missing route segment for leg {leg_id}"))
            .status
    }

    fn assert_ui_guidance_tracks_leg(
        snapshot: &UiSessionSnapshot,
        active_leg_index: usize,
        from_label: &str,
        to_label: &str,
    ) {
        let ui_guidance = snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan_ui| plan_ui.guidance.as_ref())
            .expect("ui guidance");
        assert_eq!(ui_guidance.active_leg_index, Some(active_leg_index));
        assert!(
            ui_guidance
                .nav_element
                .active_leg_summary
                .contains(from_label)
                && ui_guidance
                    .nav_element
                    .active_leg_summary
                    .contains(to_label),
            "CDI label should describe the active leg, got {}",
            ui_guidance.nav_element.active_leg_summary
        );
        assert!(
            ui_guidance.nav_element.cdi_indicator_dots.is_some(),
            "CDI should not be blank while the route has an active leg"
        );
    }

    fn short_procedure_display_path_plan() -> FlightPlan {
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
            id: "short-procedure-display-path".to_string(),
            name: "Synthetic Procedure".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: crate::AirportId("KAAA".to_string()),
                    procedure_id: "TEST".to_string(),
                    display_label: None,
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-procedure".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![ResolvedLeg {
                id: "procedure-leg".to_string(),
                from: NavRef::Fix("FIXA".to_string()),
                to: NavRef::Fix("FIXC".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: Some(crate::ProcedureLegProvenance {
                    airport_id: "KAAA".to_string(),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    role: crate::ProcedureSegmentRole::Common,
                    path_termination: crate::PathTermination::TrackToFix,
                    leg_sequence: 10,
                    display_path: Some(crate::LegDisplayPath {
                        style: crate::LegDisplayPathStyle::Solid,
                        elements: vec![
                            LegDisplayElement::Segment { start: a, end: b },
                            LegDisplayElement::Segment { start: b, end: c },
                        ],
                        effective_terminal_course_deg: None,
                        debug_element_sources: Vec::new(),
                        debug_element_roles: Vec::new(),
                    }),
                }),
            }],
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

    fn short_procedure_nav_kv_store() -> NavKvStore {
        crate::navkv::nav_kv_store_for_test(
            &[
                (
                    "navref/position/fix/FIXA",
                    br#"{"lat":40.0,"lon":-120.0}"# as &[u8],
                ),
                (
                    "navref/position/fix/FIXC",
                    br#"{"lat":40.05,"lon":-119.95}"# as &[u8],
                ),
            ],
            256,
        )
    }

    fn self_contained_route_statuses(
        plan: &FlightPlan,
    ) -> Vec<crate::FlightPlanRouteSegmentStatus> {
        crate::project_flight_plan_route_with_resolver(
            plan,
            |nav_ref, _procedure_airport_id| -> Result<LatLon, String> {
                match nav_ref {
                    NavRef::LatLon(position) | NavRef::Spot(position) => Ok(*position),
                    _ => Err(format!("missing test position for {nav_ref:?}")),
                }
            },
        )
        .expect("project self-contained route")
        .into_iter()
        .map(|segment| segment.status)
        .collect()
    }

    fn push_test_ownship_position(
        handle: u32,
        position: LatLon,
        epoch_ms: i64,
    ) -> UiSessionSnapshot {
        push_situation_sample_in_session(
            handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: epoch_ms,
                received_time_epoch_ms: epoch_ms,
                position: Some(position),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push ownship position")
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

    fn enable_bad_autopilot(handle: u32) -> UiSessionSnapshot {
        set_debug_flag_in_session(handle, "bad_autopilot", true).expect("enable Bad Autopilot")
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

    fn self_contained_geometry_map_for_test(
        plan: &FlightPlan,
    ) -> HashMap<String, GuidanceLegGeometry> {
        self_contained_guidance_leg_geometry_for_plan(plan)
            .expect("derive self-contained geometry")
            .unwrap_or_default()
            .into_iter()
            .map(|geometry| (geometry.leg_id.clone(), geometry))
            .collect()
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

    #[test]
    fn eta_cells_use_ground_speed_and_session_clock() {
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let noon_utc = utc("2026-06-14T12:00:00Z").timestamp_millis();
        let init = create_ui_session_at_epoch_ms(lat_lon_preview_plan(), &[], None, None, noon_utc)
            .expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");

        let snapshot = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: noon_utc,
                received_time_epoch_ms: noon_utc,
                position: Some(LatLon {
                    lat: 40.0,
                    lon: -119.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(0.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push sample");

        let banner_eta = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());
        let row_eta = snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("active plan")
            .display_rows
            .iter()
            .find(|row| row.leg_index == Some(1))
            .and_then(|row| {
                row.data_cells
                    .iter()
                    .find(|cell| cell.id == "final_eta")
                    .and_then(|cell| cell.value.as_deref())
            });

        assert_eq!(banner_eta, Some("12:30"));
        assert_eq!(row_eta, Some("12:30"));
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
                vertical_speed_fpm: None,
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
                vertical_speed_fpm: None,
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
                vertical_speed_fpm: None,
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
            !init.snapshot.playback_panel_state.visible,
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
        assert!(replay.playback_panel_state.visible);
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
                vertical_speed_fpm: None,
            },
        )
        .expect("push gps sample");
        assert!(
            gps.playback_panel_state.visible,
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
            !gps.playback_panel_state.visible,
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
    fn gps_capture_replay_stays_in_replay_source_slot() {
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
                    lat: 47.0,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(30.0),
                altitude_msl_ft: Some(100.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push gps sample");
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId(PLAYBACK_SOURCE_ID.to_string()),
            },
        )
        .expect("select Replay");

        let replay = load_playback_trace_in_session(
            init.handle,
            "gps.jsonl",
            r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":0,"received_time_epoch_ms":0,"source_kind":"device_gps","position":{"lat":48.0,"lon":-123.0},"altitude_msl_ft":200.0,"ground_speed_kt":40.0,"track_deg_true":100.0}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.sample"}"#,
        )
        .expect("load GPS capture replay");

        assert!(
            replay.playback_panel_state.visible,
            "loading a GPS capture through Replay must not hide the playback panel",
        );
        let sources = &replay.app_ui_state.ownship.controls.sources;
        assert_eq!(
            sources
                .iter()
                .filter(|source| source.label == "GPS")
                .count(),
            1,
            "live GPS should remain the only GPS menu item",
        );
        assert_eq!(
            sources
                .iter()
                .filter(|source| source.label == "Replay")
                .count(),
            1,
            "GPS capture playback should occupy the Replay menu item",
        );
        let active = sources
            .iter()
            .find(|source| source.active)
            .expect("active ownship source");
        assert_eq!(active.label, "Replay");
        assert_eq!(active.source_id.0, PLAYBACK_SOURCE_ID);
        assert_eq!(active.source_kind, OwnshipSourceKind::GpxPlayback);
    }

    #[test]
    fn gps_capture_replay_tick_projects_ownship_render() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
        let loaded = load_playback_trace_in_session(
            init.handle,
            "gps.jsonl",
            r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"searching","status_label":"Searching"}},"logged_at_epoch_ms":1000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":2000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":2000,"received_time_epoch_ms":2000,"source_kind":"device_gps","position":{"lat":47.0,"lon":-122.0},"altitude_msl_ft":300.0,"ground_speed_kt":42.0,"track_deg_true":90.0}},"logged_at_epoch_ms":2000,"tag":"ownship.gps_capture.sample"}"#,
        )
        .expect("load GPS capture replay");
        assert_eq!(
            loaded.app_ui_state.ownship.controls.launcher_label,
            "Replay: No GPS",
        );
        assert!(loaded.app_ui_state.ownship.render.position.is_none());
        assert!(!loaded.app_ui_state.ownship.render.draw_aircraft);

        play_playback_in_session(init.handle, 10_000.0).expect("play replay");
        let ticked =
            tick_playback_in_session(init.handle, 11_100.0).expect("tick past first GPS sample");
        let ownship = &ticked.app_ui_state.ownship.render;
        assert_eq!(ownship.mode, crate::OwnshipMode::Replay);
        assert_eq!(
            ownship.position,
            Some(LatLon {
                lat: 47.0,
                lon: -122.0,
            }),
        );
        assert_eq!(ownship.speed_kt, Some(42.0));
        assert_eq!(ownship.altitude_msl_ft, Some(300.0));
        assert_eq!(
            ticked.app_ui_state.ownship.controls.launcher_label,
            "Replay: GPS",
        );
    }

    #[test]
    fn gps_capture_replay_follow_does_not_snap_to_stale_viewport_when_gps_is_lost() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
        let seattle_viewport = MapViewport {
            center: LatLon {
                lat: 47.500,
                lon: -122.300,
            },
            zoom: 9.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let pass_position = LatLon {
            lat: 47.339,
            lon: -121.390,
        };
        load_playback_trace_in_session(
            init.handle,
            "gps.jsonl",
            r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":0,"received_time_epoch_ms":0,"source_kind":"device_gps","position":{"lat":47.339,"lon":-121.390},"altitude_msl_ft":3000.0,"ground_speed_kt":42.0,"track_deg_true":90.0}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.sample"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"searching","status_label":"Searching"}},"logged_at_epoch_ms":10000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":300000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":300000,"received_time_epoch_ms":300000,"source_kind":"device_gps","position":{"lat":47.350,"lon":-121.410},"altitude_msl_ft":3100.0,"ground_speed_kt":40.0,"track_deg_true":95.0}},"logged_at_epoch_ms":300000,"tag":"ownship.gps_capture.sample"}"#,
        )
        .expect("load GPS capture replay");

        let centered = engage_map_follow_in_session(init.handle, seattle_viewport)
            .expect("engage map follow with GPS fix");
        let centered_viewport = centered
            .map_follow_target_viewport
            .expect("follow target while GPS is connected");
        assert!(
            (centered_viewport.center.lat - pass_position.lat).abs() < 1e-6,
            "follow should target the replayed GPS latitude"
        );
        assert!(
            (centered_viewport.center.lon - pass_position.lon).abs() < 1e-6,
            "follow should target the replayed GPS longitude"
        );

        let lost =
            seek_playback_in_session(init.handle, 10.0, 10_000.0).expect("seek into GPS gap");
        assert!(lost.app_ui_state.ownship.render.position.is_none());
        assert!(
            !lost.map_follow_ui_state.following,
            "follow should disengage while the GPS source is unavailable"
        );
        let lost_viewport = lost
            .map_follow_target_viewport
            .expect("viewport target should preserve the last useful map position");
        assert_eq!(
            lost_viewport, centered_viewport,
            "losing GPS must not snap back to the stale viewport from CTR engagement"
        );
        assert_ne!(
            lost_viewport, seattle_viewport,
            "losing GPS must not send the map back to the engage-time viewport"
        );
    }

    #[test]
    fn restore_direct_to_in_session_returns_to_follow_plan_guidance() {
        let direct_to_plan = crate::activate_direct_to(
            &sample_guided_plan(),
            LatLon {
                lat: 37.44,
                lon: -122.15,
            },
            NavRef::Fix("OFFPL".to_string()),
        )
        .expect("activate off-plan direct-to");
        let init = create_ui_session(direct_to_plan, &[], None, None).expect("create session");
        let direct_to_guidance = init
            .snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("direct-to guidance");
        assert_eq!(direct_to_guidance.sequencing_mode, SequencingMode::DirectTo);
        assert!(direct_to_guidance.direct_to.is_some());
        assert!(init
            .snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("direct-to plan UI")
            .controls
            .iter()
            .any(
                |control| matches!(&control.id, crate::FlightPlanControlId::RestoreDirectTo)
                    && control.enabled
            ));

        let restored = restore_direct_to_in_session(init.handle).expect("restore direct-to");
        let guidance = restored
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("restored guidance");

        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert!(guidance.direct_to.is_none());
        let restored_restore_control = restored
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("restored plan UI")
            .controls
            .iter()
            .find(|control| matches!(&control.id, crate::FlightPlanControlId::RestoreDirectTo))
            .expect("restore direct-to control");
        assert!(!restored_restore_control.enabled);
    }

    #[test]
    fn empty_plan_direct_to_projects_route_through_session_api() {
        let init = create_ui_session(FlightPlan::empty(), &[], None, None).expect("create session");
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let start = LatLon {
            lat: 47.600,
            lon: -122.300,
        };
        let target = LatLon {
            lat: 47.700,
            lon: -122.100,
        };
        push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(start),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push ownship");
        let direct_to =
            activate_direct_to_nav_ref_in_session_outcome(init.handle, NavRef::LatLon(target))
                .expect("activate direct-to");
        assert_session_snapshot_invalidated(direct_to);

        let route = project_flight_plan_route_in_session(init.handle).expect("project route");
        let HadOperationOutcome::Complete { result, .. } = route else {
            panic!("empty-plan direct-to route unexpectedly needed resources");
        };
        let segments: Vec<crate::FlightPlanRouteSegment> =
            serde_json::from_value(result).expect("route segments");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].id, "direct-to");
        assert_eq!(
            segments[0].status,
            crate::FlightPlanRouteSegmentStatus::Active
        );
        assert_eq!(segments[0].from, start);
        assert_eq!(segments[0].to, target);
    }

    #[test]
    fn restore_direct_to_outcome_invalidates_flight_plan_route() {
        let direct_to_plan = crate::activate_direct_to(
            &sample_guided_plan(),
            LatLon {
                lat: 37.44,
                lon: -122.15,
            },
            NavRef::Fix("OFFPL".to_string()),
        )
        .expect("activate off-plan direct-to");
        let init = create_ui_session(direct_to_plan, &[], None, None).expect("create session");

        let outcome =
            super::restore_direct_to_in_session(init.handle).expect("restore direct-to outcome");
        let HadOperationOutcome::Complete {
            result,
            invalidations,
        } = outcome
        else {
            panic!("restore direct-to unexpectedly needed resources");
        };
        let snapshot: UiSessionSnapshot = serde_json::from_value(result).expect("snapshot result");

        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::FlightPlanRoute));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert_eq!(snapshot.session_revision, 1);
        assert_eq!(
            snapshot
                .app_state
                .active_plan
                .as_ref()
                .and_then(|plan| plan.guidance.as_ref())
                .expect("restored guidance")
                .sequencing_mode,
            SequencingMode::FollowPlan
        );
    }

    #[test]
    fn session_revision_advances_on_mutation_not_read() {
        let init =
            create_ui_session(FlightPlan::default(), &[], None, None).expect("create session");
        assert_eq!(init.snapshot.session_revision, 0);

        let initial_read = get_session_snapshot(init.handle).expect("read snapshot");
        assert_eq!(initial_read.session_revision, 0);

        let first_mutation =
            set_debug_flag_in_session(init.handle, "tile_labels", true).expect("set debug flag");
        assert_eq!(first_mutation.session_revision, 1);

        let post_mutation_read = get_session_snapshot(init.handle).expect("read snapshot again");
        assert_eq!(
            post_mutation_read.session_revision,
            first_mutation.session_revision
        );

        let second_mutation =
            set_debug_flag_in_session(init.handle, "tile_labels", false).expect("clear debug flag");
        assert_eq!(second_mutation.session_revision, 2);
    }

    #[test]
    fn map_selection_direct_to_requires_ownship_position() {
        let selection = MapSelectionQueryResult {
            click_lat: 47.49,
            click_lon: -122.76,
            categories: vec![MapSelectionCategory {
                id: "points".to_string(),
                label: "Points".to_string(),
                items: vec![MapSelectionItem {
                    id: "airport:KPWT".to_string(),
                    label: "KPWT".to_string(),
                    sublabel: "Airport".to_string(),
                    description: None,
                    secondary_description: None,
                    position: None,
                    detail_text: None,
                    highlight: MapSelectionHighlight::Spot {
                        lat: 47.49,
                        lon: -122.76,
                    },
                    nav_ref: Some(NavRef::Airport("KPWT".to_string())),
                    symbol_feature: None,
                    metar_feature: None,
                    pirep_feature: None,
                    airspace_icon: None,
                    actions: vec![MapSelectionAction {
                        id: "direct_to".to_string(),
                        label: "Direct-to".to_string(),
                        enabled: true,
                        display_only: false,
                        detail_text: None,
                        detail_title: None,
                        detail_status: None,
                        disabled_reason: None,
                        weather_detail: None,
                        airspace_limit: None,
                        session_action: Some("direct-to-action".to_string()),
                        flight_plan_row_action: None,
                        navigation: None,
                    }],
                }],
            }],
        };

        let unavailable = map_selection_with_session_action_availability(selection.clone(), false);
        let action = &unavailable.categories[0].items[0].actions[0];
        assert!(!action.enabled);
        assert_eq!(
            action.disabled_reason.as_deref(),
            Some("Direct-to requires ownship position.")
        );
        assert_eq!(action.detail_text, None);
        assert!(action.session_action.is_none());
        assert!(action.flight_plan_row_action.is_none());

        let available = map_selection_with_session_action_availability(selection, true);
        let action = &available.categories[0].items[0].actions[0];
        assert!(action.enabled);
        assert_eq!(action.session_action.as_deref(), Some("direct-to-action"));
        assert!(action.flight_plan_row_action.is_none());
    }

    fn test_map_selection_item(
        id: &str,
        description: Option<&str>,
        position: Option<LatLon>,
    ) -> MapSelectionItem {
        MapSelectionItem {
            id: id.to_string(),
            label: id.to_string(),
            sublabel: String::new(),
            description: description.map(str::to_string),
            secondary_description: None,
            position,
            detail_text: None,
            highlight: MapSelectionHighlight::Spot { lat: 0.0, lon: 0.0 },
            nav_ref: None,
            symbol_feature: None,
            metar_feature: None,
            pirep_feature: None,
            airspace_icon: None,
            actions: Vec::new(),
        }
    }

    fn test_map_selection_with_items(items: Vec<MapSelectionItem>) -> MapSelectionQueryResult {
        MapSelectionQueryResult {
            click_lat: 0.0,
            click_lon: 0.0,
            categories: vec![MapSelectionCategory {
                id: "test".to_string(),
                label: "Test".to_string(),
                items,
            }],
        }
    }

    #[test]
    fn map_selection_appends_ownship_distance_to_point_description() {
        let ownship = LatLon { lat: 0.0, lon: 0.0 };
        let point = LatLon { lat: 0.0, lon: 0.3 };
        let selection = test_map_selection_with_items(vec![test_map_selection_item(
            "KAPC",
            Some("Elev 36"),
            Some(point),
        )]);

        let selection = map_selection_with_ownship_distances(selection, Some(ownship));
        let expected_distance =
            crate::flight_data::format_nm(crate::great_circle_distance_nm(ownship, point));
        assert_eq!(
            selection.categories[0].items[0].description,
            Some(format!("Elev 36 · {expected_distance}nm"))
        );
    }

    #[test]
    fn map_selection_uses_common_tenths_format_for_near_point_distance() {
        let ownship = LatLon { lat: 0.0, lon: 0.0 };
        let point = LatLon { lat: 0.0, lon: 0.1 };
        let mut spot = test_map_selection_item("SPOT", None, Some(point));
        spot.secondary_description = Some("0.0000, 0.1000".to_string());
        let selection = test_map_selection_with_items(vec![spot]);

        let selection = map_selection_with_ownship_distances(selection, Some(ownship));
        assert_eq!(
            selection.categories[0].items[0].description,
            Some(format!(
                "{}nm",
                crate::flight_data::format_nm(crate::great_circle_distance_nm(ownship, point))
            ))
        );
        assert_eq!(
            selection.categories[0].items[0]
                .secondary_description
                .as_deref(),
            Some("0.0000, 0.1000")
        );
    }

    #[test]
    fn map_selection_does_not_add_distance_to_boundary_description() {
        let selection = test_map_selection_with_items(vec![test_map_selection_item(
            "class-b",
            Some("SFC-100"),
            None,
        )]);

        let selection =
            map_selection_with_ownship_distances(selection, Some(LatLon { lat: 0.0, lon: 0.0 }));
        assert_eq!(
            selection.categories[0].items[0].description.as_deref(),
            Some("SFC-100")
        );
    }

    #[test]
    fn debug_log_to_developer_server_is_core_owned_and_default_off() {
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        assert!(!init.snapshot.debug_state.debug_log_to_developer_server);

        let enabled = set_debug_flag_in_session(init.handle, "debug_log_to_developer_server", true)
            .expect("enable developer-server debug log");
        assert!(enabled.debug_state.debug_log_to_developer_server);

        let disabled =
            set_debug_flag_in_session(init.handle, "debug_log_to_developer_server", false)
                .expect("disable developer-server debug log");
        assert!(!disabled.debug_state.debug_log_to_developer_server);
    }

    #[test]
    fn bad_autopilot_source_is_hidden_until_debug_flag_is_enabled() {
        let init =
            create_ui_session(lat_lon_preview_plan(), &[], None, None).expect("create session");
        assert!(
            !init
                .snapshot
                .app_ui_state
                .ownship
                .controls
                .sources
                .iter()
                .any(|source| source.source_kind == OwnshipSourceKind::BadAutopilot),
            "Bad Autopilot must not appear in production/default source menu",
        );

        let ignored_select = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            },
        )
        .expect("select Bad Autopilot while hidden");
        assert!(
            ignored_select
                .app_ui_state
                .ownship
                .controls
                .sources
                .iter()
                .all(|source| source.source_kind != OwnshipSourceKind::BadAutopilot),
            "stale platform calls must not surface Bad Autopilot while the flag is off",
        );

        let enabled = enable_bad_autopilot(init.handle);
        assert!(
            enabled
                .app_ui_state
                .ownship
                .controls
                .sources
                .iter()
                .any(|source| {
                    source.source_kind == OwnshipSourceKind::BadAutopilot && source.enabled
                }),
            "Bad Autopilot should be selectable once the flag is enabled and active leg geometry is available",
        );

        let selected = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            },
        )
        .expect("select Bad Autopilot");
        assert!(matches!(
            selected.app_ui_state.ownship.controls.selection,
            crate::OwnshipSelectionCommand::Source { ref source_id }
                if source_id.0 == BAD_AUTOPILOT_SOURCE_ID
        ));

        let disabled =
            set_debug_flag_in_session(init.handle, "bad_autopilot", false).expect("disable flag");
        assert!(!disabled.debug_state.bad_autopilot);
        assert!(matches!(
            disabled.app_ui_state.ownship.controls.selection,
            crate::OwnshipSelectionCommand::Auto
        ));
        assert!(
            disabled
                .app_ui_state
                .ownship
                .controls
                .sources
                .iter()
                .all(|source| source.source_kind != OwnshipSourceKind::BadAutopilot),
            "disabling the flag removes Bad Autopilot from the source menu",
        );
    }

    #[test]
    fn session_projects_cdi_from_injected_guidance_geometry() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
        let after_geometry = set_guidance_leg_geometry_in_session(
            init.handle,
            vec![GuidanceLegGeometry {
                leg_id: "leg:0:component-0-1#0".to_string(),
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
                vertical_speed_fpm: None,
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
    fn on_plan_direct_to_cdi_uses_direct_segment_not_underlying_leg_path() {
        let init =
            create_ui_session(sample_guided_plan(), &[], None, None).expect("create session");
        let direct_start = LatLon {
            lat: 47.0,
            lon: -122.0,
        };
        let target = LatLon {
            lat: 48.0,
            lon: -122.0,
        };
        push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(direct_start),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push direct-to start");
        let direct_to = activate_direct_to_nav_ref_in_session_outcome(
            init.handle,
            NavRef::Fix("VPDUB".to_string()),
        )
        .expect("activate direct-to");
        assert_session_snapshot_invalidated(direct_to);
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: "leg:0:component-0-1#0".to_string(),
                    from: LatLon {
                        lat: 48.0,
                        lon: -123.0,
                    },
                    to: target,
                    path: vec![
                        LatLon {
                            lat: 48.0,
                            lon: -123.0,
                        },
                        target,
                    ],
                },
                GuidanceLegGeometry {
                    leg_id: "direct-to".to_string(),
                    from: direct_start,
                    to: target,
                    path: vec![direct_start, target],
                },
            ],
        )
        .expect("set guidance geometry");
        let sample_position = LatLon {
            lat: 47.5,
            lon: -121.985,
        };
        let after_position = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 2_000,
                received_time_epoch_ms: 2_000,
                position: Some(sample_position),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push sample position");
        let dots = after_position
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .and_then(|guidance| guidance.nav_element.cdi_indicator_dots)
            .expect("direct-to CDI dots");
        let active_leg_summary = after_position
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .map(|guidance| guidance.nav_element.active_leg_summary.as_str())
            .expect("direct-to CDI label");
        let expected = cdi_dots_for_leg(direct_start, target, sample_position);
        let stale = cdi_dots_for_leg(
            LatLon {
                lat: 48.0,
                lon: -123.0,
            },
            target,
            sample_position,
        );

        assert!(
            active_leg_summary.starts_with("SPOT -> VPDUB"),
            "direct-to CDI label must describe the same direct leg used for deviation: {active_leg_summary}"
        );
        assert!(
            (dots - expected).abs() < 1e-4,
            "direct-to CDI must use direct segment: got {dots}, expected {expected}"
        );
        assert!(
            (dots - stale).abs() > 0.25,
            "test must distinguish direct-to CDI from stale underlying leg CDI"
        );
    }

    #[test]
    fn synced_guidance_geometry_supports_replay_sequencing() {
        let store = short_procedure_nav_kv_store();
        let init = create_ui_session(short_procedure_display_path_plan(), &[], None, None)
            .expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let outcome =
            sync_guidance_geometry_in_session(init.handle).expect("sync guidance geometry");
        assert!(
            matches!(outcome, HadOperationOutcome::Complete { .. }),
            "procedure display-path geometry should sync without HAD page faults"
        );

        load_playback_trace_in_session(
            init.handle,
            "cross-active-leg.json",
            r#"{"trace":[[0.0,40.0,-120.0,3000,120.0,90.0],[10.0,40.002,-119.948,3000,120.0,45.0]]}"#,
        )
        .expect("load replay trace");
        play_playback_in_session(init.handle, 0.0).expect("play replay");
        let snapshot = tick_playback_in_session(init.handle, 10_000.0).expect("tick replay");
        let guidance = snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");

        assert_eq!(guidance.active_leg_index, Some(0));
        let sessions = lock_sessions();
        let core_guidance = session_ref(&sessions, init.handle)
            .expect("session")
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("core guidance");
        assert_eq!(core_guidance.active_detail_index, Some(1));
    }

    #[test]
    fn raw_ownship_sample_sequences_across_active_leg_finish_line() {
        let plan = short_lat_lon_preview_plan();
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
        let init = create_ui_session(plan.clone(), &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &plan.resolved_legs[0], 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(1, &plan.resolved_legs[1], 0),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ],
        )
        .expect("install guidance geometry");

        let outcome = super::push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 40.002,
                    lon: -119.948,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(45.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push sample");
        let invalidations = complete_invalidations(outcome);
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::FlightPlanRoute));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));

        let snapshot = get_session_snapshot(init.handle).expect("snapshot");
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.active_detail_index, Some(1));
    }

    #[test]
    fn playback_sequences_across_active_leg_finish_line() {
        let plan = short_lat_lon_preview_plan();
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
        let init = create_ui_session(plan.clone(), &[], None, None).expect("create session");
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &plan.resolved_legs[0], 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(1, &plan.resolved_legs[1], 0),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ],
        )
        .expect("install guidance geometry");
        load_playback_trace_in_session(
            init.handle,
            "cross-simple-leg.json",
            r#"{"trace":[[0.0,40.0,-120.0,3000,120.0,90.0],[10.0,40.002,-119.948,3000,120.0,45.0]]}"#,
        )
        .expect("load replay trace");
        play_playback_in_session(init.handle, 0.0).expect("play replay");

        let snapshot = tick_playback_in_session(init.handle, 10_000.0).expect("tick replay");
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.active_detail_index, Some(1));
    }

    #[test]
    fn playback_sequences_direct_to_route_start_into_first_leg() {
        let plan = short_lat_lon_preview_plan();
        let route_start = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let direct_start = LatLon {
            lat: 40.0,
            lon: -120.02,
        };
        let after_route_start = LatLon {
            lat: 40.0,
            lon: -119.998,
        };
        let direct_to_plan =
            crate::activate_direct_to(&plan, direct_start, NavRef::LatLon(route_start))
                .expect("activate direct-to route start");
        let init = create_ui_session(direct_to_plan, &[], None, None).expect("create session");
        load_playback_trace_in_session(
            init.handle,
            "cross-direct-to-route-start.json",
            &format!(
                r#"{{"trace":[[0.0,{},{},3000,120.0,90.0],[10.0,{},{},3000,120.0,90.0]]}}"#,
                direct_start.lat, direct_start.lon, after_route_start.lat, after_route_start.lon
            ),
        )
        .expect("load replay trace");
        play_playback_in_session(init.handle, 0.0).expect("play replay");

        let snapshot = tick_playback_in_session(init.handle, 10_000.0).expect("tick replay");
        let guidance = snapshot
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn route_status_tracks_active_guidance_through_direct_to_and_route_end() {
        let plan = short_lat_lon_preview_plan();
        let route_start = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let direct_start = LatLon {
            lat: 40.0,
            lon: -120.02,
        };
        let direct_to_plan =
            crate::activate_direct_to(&plan, direct_start, NavRef::LatLon(route_start))
                .expect("activate direct-to route start");
        let init = create_ui_session(direct_to_plan, &[], None, None).expect("create session");

        let after_direct = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -119.998,
            },
            1_000,
        );
        let after_direct_plan = after_direct
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after direct-to");
        assert_eq!(
            after_direct_plan
                .guidance
                .as_ref()
                .expect("guidance after direct-to")
                .active_leg_index,
            0
        );
        assert_eq!(
            self_contained_route_statuses(after_direct_plan),
            vec![
                crate::FlightPlanRouteSegmentStatus::Active,
                crate::FlightPlanRouteSegmentStatus::Remaining,
            ]
        );

        let after_first_leg = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.002,
                lon: -119.948,
            },
            2_000,
        );
        let after_first_leg_plan = after_first_leg
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after first leg");
        assert_eq!(
            after_first_leg_plan
                .guidance
                .as_ref()
                .expect("guidance after first leg")
                .active_leg_index,
            1
        );
        assert_eq!(
            self_contained_route_statuses(after_first_leg_plan),
            vec![
                crate::FlightPlanRouteSegmentStatus::Completed,
                crate::FlightPlanRouteSegmentStatus::Active,
            ]
        );

        let after_route_end = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.052,
                lon: -119.948,
            },
            3_000,
        );
        let after_route_end_plan = after_route_end
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after route end");
        assert!(crate::active_guidance_leg(after_route_end_plan).is_none());
        assert_eq!(
            self_contained_route_statuses(after_route_end_plan),
            vec![
                crate::FlightPlanRouteSegmentStatus::Completed,
                crate::FlightPlanRouteSegmentStatus::Completed,
            ]
        );
    }

    #[test]
    fn named_route_origin_direct_to_sequences_through_resumed_leg_with_cdi_and_active_route() {
        let init = create_synced_modda_zgood_normy_session();
        push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.02,
            },
            1_000,
        );
        activate_direct_to_modda_row(init.handle);

        let after_direct_to = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -119.998,
            },
            2_000,
        );
        let plan = after_direct_to
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after completing direct-to");
        let guidance = plan.guidance.as_ref().expect("guidance after direct-to");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert!(guidance.direct_to.is_none());
        assert_ui_guidance_tracks_leg(&after_direct_to, 0, "MODDA", "ZGOOD");
        assert_eq!(
            session_route_statuses(init.handle),
            vec![
                crate::FlightPlanRouteSegmentStatus::Active,
                crate::FlightPlanRouteSegmentStatus::Remaining,
            ],
            "route projection must render exactly the CDI-active leg as active"
        );

        let after_resumed_leg = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.002,
                lon: -119.948,
            },
            3_000,
        );
        let plan = after_resumed_leg
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after completing resumed leg");
        let guidance = plan
            .guidance
            .as_ref()
            .expect("guidance after completing resumed leg");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.active_detail_index, Some(1));
        assert!(guidance.direct_to.is_none());
        assert_ui_guidance_tracks_leg(&after_resumed_leg, 1, "ZGOOD", "NORMY");
        assert_eq!(
            session_route_statuses(init.handle),
            vec![
                crate::FlightPlanRouteSegmentStatus::Completed,
                crate::FlightPlanRouteSegmentStatus::Active,
            ],
            "route projection must advance active paint with guidance"
        );
    }

    #[test]
    fn named_route_origin_direct_to_can_sequence_multiple_finish_lines_in_one_sample() {
        let init = create_synced_modda_zgood_normy_session();
        push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.02,
            },
            1_000,
        );
        activate_direct_to_modda_row(init.handle);

        let after_jump = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.002,
                lon: -119.948,
            },
            2_000,
        );
        let plan = after_jump
            .app_state
            .active_plan
            .as_ref()
            .expect("plan after jump");
        let guidance = plan.guidance.as_ref().expect("guidance after jump");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.active_detail_index, Some(1));
        assert_ui_guidance_tracks_leg(&after_jump, 1, "ZGOOD", "NORMY");
    }

    #[test]
    fn map_direct_to_named_route_origin_preserves_resume_leg() {
        let init = create_synced_modda_zgood_normy_session();
        push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.02,
            },
            1_000,
        );
        let direct_to =
            activate_direct_to_nav_ref_in_session_outcome(init.handle, NavRef::Fix("MODDA".into()))
                .expect("map direct-to MODDA");
        assert_session_snapshot_invalidated(direct_to);
        let after_direct_to = get_session_snapshot(init.handle).expect("snapshot after direct-to");
        let direct_to_state = after_direct_to
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");
        assert_eq!(
            direct_to_state.resume_leg_id.as_deref(),
            Some("leg-modda-zgood"),
            "direct-to route origin must resume the first route leg"
        );

        let after_jump = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.002,
                lon: -119.948,
            },
            2_000,
        );
        let guidance = after_jump
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance after jump");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.active_detail_index, Some(1));
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn map_direct_to_airway_midpoint_sequences_into_resume_leg() {
        let init = create_synced_twf_v4_ykm_chins_kpae_session(NavRef::Navaid("PDT".to_string()));
        push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.18,
            },
            1_000,
        );
        let direct_to = activate_direct_to_nav_ref_in_session_outcome(
            init.handle,
            NavRef::Navaid("PDT".into()),
        )
        .expect("map direct-to PDT");
        assert_session_snapshot_invalidated(direct_to);
        let after_direct_to = get_session_snapshot(init.handle).expect("snapshot after direct-to");
        let direct_to_state = after_direct_to
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");
        assert_eq!(
            direct_to_state.resume_leg_id.as_deref(),
            Some("v4-pdt-cordo"),
            "direct-to an airway midpoint must remember the downstream airway leg"
        );

        let after_pdt = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.08,
            },
            2_000,
        );
        let guidance = after_pdt
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance after crossing PDT");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 3);
        assert_eq!(guidance.active_detail_index, Some(3));
        assert!(guidance.direct_to.is_none());
        assert_ui_guidance_tracks_leg(&after_pdt, 3, "PDT", "CORDO");
    }

    #[test]
    fn map_direct_to_airway_midpoint_marks_reachable_downstream_legs_remaining() {
        let init = create_synced_twf_v4_ykm_chins_kpae_session(NavRef::Navaid("PDT".to_string()));
        push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.18,
            },
            1_000,
        );
        let direct_to = activate_direct_to_nav_ref_in_session_outcome(
            init.handle,
            NavRef::Navaid("PDT".into()),
        )
        .expect("map direct-to PDT");
        assert_session_snapshot_invalidated(direct_to);
        let after_direct_to = get_session_snapshot(init.handle).expect("snapshot after direct-to");
        let direct_to_state = after_direct_to
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");
        assert_eq!(
            direct_to_state.resume_leg_id.as_deref(),
            Some("v4-pdt-cordo"),
            "direct-to should resume through the downstream airway leg"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "leg-twf-alkal"),
            crate::FlightPlanRouteSegmentStatus::Completed,
            "legs before the direct-to target are no longer reachable by sequencing"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "v4-alkal-spuud"),
            crate::FlightPlanRouteSegmentStatus::Completed,
            "airway legs before the direct-to target are no longer reachable by sequencing"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "v4-spuud-pdt"),
            crate::FlightPlanRouteSegmentStatus::Completed,
            "the normal inbound leg to the direct-to target is replaced by the direct-to leg"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "direct-to"),
            crate::FlightPlanRouteSegmentStatus::Active,
            "the direct-to overlay is the active leg"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "v4-pdt-cordo"),
            crate::FlightPlanRouteSegmentStatus::Remaining,
            "the downstream airway leg is reachable after crossing the direct-to target"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "v4-cordo-ykm"),
            crate::FlightPlanRouteSegmentStatus::Remaining,
            "later airway legs remain reachable after crossing the direct-to target"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "leg-ykm-chins"),
            crate::FlightPlanRouteSegmentStatus::Remaining,
            "post-airway legs remain reachable after crossing the direct-to target"
        );
        assert_eq!(
            session_route_status_by_leg_id(init.handle, "leg-chins-kpae"),
            crate::FlightPlanRouteSegmentStatus::Remaining,
            "the route destination remains reachable after crossing the direct-to target"
        );

        let after_pdt = push_test_ownship_position(
            init.handle,
            LatLon {
                lat: 40.0,
                lon: -120.08,
            },
            2_000,
        );
        let guidance = after_pdt
            .app_state
            .active_plan
            .as_ref()
            .and_then(|plan| plan.guidance.as_ref())
            .expect("guidance after crossing PDT");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 3);
        assert_eq!(guidance.active_detail_index, Some(3));
        assert!(guidance.direct_to.is_none());
        assert_ui_guidance_tracks_leg(&after_pdt, 3, "PDT", "CORDO");
    }

    #[test]
    fn synced_guidance_geometry_drives_flight_data_banner_eta() {
        let store = short_procedure_nav_kv_store();
        let noon_utc = utc("2026-06-14T12:00:00Z").timestamp_millis();
        let init = create_ui_session_at_epoch_ms(
            short_procedure_display_path_plan(),
            &[],
            None,
            None,
            noon_utc,
        )
        .expect("create session");
        attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let outcome =
            sync_guidance_geometry_in_session(init.handle).expect("sync guidance geometry");
        assert!(
            matches!(outcome, HadOperationOutcome::Complete { .. }),
            "procedure display-path geometry should sync without HAD page faults"
        );

        let snapshot = push_situation_sample_in_session(
            init.handle,
            SituationSample {
                source_id: OwnshipSourceId("test-gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: noon_utc,
                received_time_epoch_ms: noon_utc,
                position: Some(LatLon {
                    lat: 40.0,
                    lon: -120.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(3000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        )
        .expect("push sample");

        let banner_eta = snapshot
            .app_ui_state
            .flight_data_banner
            .cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());

        assert!(
            banner_eta.is_some(),
            "synced active geometry should produce final ETA"
        );
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
                vertical_speed_fpm: None,
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
                vertical_speed_fpm: None,
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
    fn plan_preview_syncs_active_leg_by_resolved_leg_index() {
        let b = LatLon {
            lat: 40.0,
            lon: -119.0,
        };
        let c = LatLon {
            lat: 41.0,
            lon: -119.0,
        };
        let d = LatLon {
            lat: 42.0,
            lon: -119.0,
        };
        let plan = FlightPlan {
            id: "preview-unpreviewable-prefix".to_string(),
            name: "unpreviewable prefix".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("NO_GEOM_A".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("NO_GEOM_B".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(b),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(c),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(d),
                },
            ],
            route_component_uids: vec![
                "row-no-geom-a".to_string(),
                "row-no-geom-b".to_string(),
                "row-b".to_string(),
                "row-c".to_string(),
                "row-d".to_string(),
            ],
            route_component_uid_counter: 5,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "missing-geometry".to_string(),
                    from: NavRef::Fix("NO_GEOM_A".to_string()),
                    to: NavRef::Fix("NO_GEOM_B".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-b-c".to_string(),
                    from: NavRef::LatLon(b),
                    to: NavRef::LatLon(c),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-c-d".to_string(),
                    from: NavRef::LatLon(c),
                    to: NavRef::LatLon(d),
                    source: ResolvedLegSource::RouteComponent { component_index: 3 },
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
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        let snapshot = select_plan_preview(init.handle);
        let position = ownship_position(&snapshot);

        assert_near(position.lat, b.lat);
        assert_near(position.lon, b.lon);
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
        let plan = lat_lon_preview_plan();
        let active_detail_id = guidance_detail_id_for_leg_element(1, &plan.resolved_legs[1], 0);
        let active_from = LatLon {
            lat: 40.0,
            lon: -119.0,
        };
        let active_to = LatLon {
            lat: 41.0,
            lon: -119.0,
        };
        let init = create_ui_session(plan, &[], None, None).expect("create session");
        select_plan_preview(init.handle);
        set_guidance_leg_geometry_in_session(
            init.handle,
            vec![GuidanceLegGeometry {
                leg_id: active_detail_id,
                from: active_from,
                to: active_to,
                path: vec![active_from, active_to],
            }],
        )
        .expect("install guidance geometry");
        enable_bad_autopilot(init.handle);
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");
        tick_bad_autopilot_in_session(init.handle, 10_000.0).expect("tick bad autopilot");

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
                        display_label: None,
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

        let geometry_by_leg_id = self_contained_geometry_map_for_test(&plan);
        let records = plan_preview_legs(&plan, &geometry_by_leg_id);

        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].pointer_key,
            "guidance-leg:component:row-proc:0:proc-1"
        );
        assert_eq!(
            records[1].pointer_key,
            "guidance-leg:component:row-proc:1:proc-2"
        );
    }

    #[test]
    fn plan_preview_distinguishes_duplicate_guidance_leg_ids_across_components() {
        let a = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let b = LatLon {
            lat: 40.0,
            lon: -119.9,
        };
        let c = LatLon {
            lat: 40.0,
            lon: -119.8,
        };
        let d = LatLon {
            lat: 40.0,
            lon: -119.7,
        };
        let e = LatLon {
            lat: 40.0,
            lon: -119.6,
        };
        let plan = FlightPlan {
            id: "duplicate-airway-leg-id-preview".to_string(),
            name: "duplicate airway ids".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Airway {
                    airway: crate::AirwaySegment {
                        name: "V1".to_string(),
                        branch_key: None,
                        entry: NavRef::LatLon(a),
                        exit: NavRef::LatLon(c),
                    },
                },
                RouteComponent::Airway {
                    airway: crate::AirwaySegment {
                        name: "V2".to_string(),
                        branch_key: None,
                        entry: NavRef::LatLon(c),
                        exit: NavRef::LatLon(e),
                    },
                },
            ],
            route_component_uids: vec!["row-v1".to_string(), "row-v2".to_string()],
            route_component_uid_counter: 2,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "airway--0".to_string(),
                    from: NavRef::LatLon(a),
                    to: NavRef::LatLon(b),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--1".to_string(),
                    from: NavRef::LatLon(b),
                    to: NavRef::LatLon(c),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--0".to_string(),
                    from: NavRef::LatLon(c),
                    to: NavRef::LatLon(d),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--1".to_string(),
                    from: NavRef::LatLon(d),
                    to: NavRef::LatLon(e),
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

        let geometry_by_leg_id = self_contained_geometry_map_for_test(&plan);
        let records = plan_preview_legs(&plan, &geometry_by_leg_id);
        let keys = records
            .iter()
            .map(|record| record.pointer_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys.len(), 4);
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len());
        assert_eq!(
            keys,
            vec![
                "guidance-leg:component:row-v1:0:airway--0",
                "guidance-leg:component:row-v1:1:airway--1",
                "guidance-leg:component:row-v2:2:airway--0",
                "guidance-leg:component:row-v2:3:airway--1",
            ]
        );
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
                    display_label: None,
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
                guidance_detail_id_for_leg_element(0, &leg, 0),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
            ),
            (
                guidance_detail_id_for_leg_element(0, &leg, 1),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ),
            (
                guidance_detail_id_for_leg_element(0, &leg, 2),
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 2),
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
                    display_label: None,
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
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 2),
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
    fn bad_autopilot_advances_guidance_detail_in_core() {
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
                    leg_id: "leg:0:driver-leg-a-b#0".to_string(),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: "leg:1:driver-leg-b-c#0".to_string(),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
            ],
        )
        .expect("install guidance geometry");
        enable_bad_autopilot(init.handle);
        let mut snapshot = select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");
        for now_epoch_ms in [
            1_000.0, 2_000.0, 3_000.0, 4_000.0, 5_000.0, 6_000.0, 7_000.0, 8_000.0, 9_000.0,
            10_000.0,
        ] {
            snapshot =
                tick_bad_autopilot_in_session(init.handle, now_epoch_ms).expect("tick driver");
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
                    display_label: None,
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
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 0),
                    from: outbound_start,
                    to: arc_start,
                    path: vec![outbound_start, arc_start],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 1),
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
                    leg_id: guidance_detail_id_for_leg_element(0, &leg, 2),
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
    fn bad_autopilot_flies_terminal_hold_until_unsuspended() {
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
                        display_label: None,
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
                    leg_id: guidance_detail_id_for_leg_element(0, &hold_leg, 0),
                    from: a,
                    to: b,
                    path: vec![a, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &hold_leg, 1),
                    from: b,
                    to: c,
                    path: vec![b, c],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &hold_leg, 2),
                    from: c,
                    to: d,
                    path: vec![c, d],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &hold_leg, 3),
                    from: d,
                    to: e,
                    path: vec![d, e],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(0, &hold_leg, 4),
                    from: e,
                    to: b,
                    path: vec![e, b],
                },
                GuidanceLegGeometry {
                    leg_id: guidance_detail_id_for_leg_element(1, &exit_leg, 0),
                    from: b,
                    to: f,
                    path: vec![b, f],
                },
            ],
        )
        .expect("install guidance geometry");
        enable_bad_autopilot(init.handle);
        select_ownship_source_in_session(
            init.handle,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(BAD_AUTOPILOT_SOURCE_ID.to_string()),
            },
        )
        .expect("select bad autopilot");

        let mut snapshot =
            tick_bad_autopilot_in_session(init.handle, 1_000.0).expect("tick driver");
        for second in 2..=4 {
            let now_epoch_ms = f64::from(second) * 1000.0;
            snapshot =
                tick_bad_autopilot_in_session(init.handle, now_epoch_ms).expect("tick driver");
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
            snapshot = tick_bad_autopilot_in_session(init.handle, now_epoch_ms)
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
            snapshot = tick_bad_autopilot_in_session(init.handle, now_epoch_ms)
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
