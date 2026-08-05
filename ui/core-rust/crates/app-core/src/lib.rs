// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

mod adsb;
pub mod aircraft_profiles;
pub mod airport_info;
pub mod altitude_planner;
pub mod chart_page;
pub mod cloud;
mod cloud_acs;
#[cfg(test)]
mod cloud_acs_memory;
mod cloud_google_drive;
pub mod content;
pub mod data_status;
pub mod debug_log;
mod device_setup_code;
pub mod errors;
pub mod flight_data;
mod flight_plan_materialization;
mod forecast_atmosphere;
pub mod freshness;
pub mod geodesy;
pub mod geometry;
pub mod had_ops;
pub mod ids;
pub mod live_feed_cache;
pub mod live_feed_runtime;
pub mod live_feeds;
pub mod map_follow;
pub mod map_overlay;
pub mod navdb_types;
pub mod navkv;
pub mod ownship;
pub mod package_management;
pub mod planning;
pub mod playback;
pub mod publication;
pub mod raster_tiles;
mod sequencing;
pub mod session;
pub mod situation;
pub mod state;
pub mod terrain;
pub mod ui_work_scheduler;

pub use adsb::VisibleAdsbTraffic;
pub use aircraft_profiles::{
    pa46_310p_climb_points, pa46_310p_profile, Pa46CruiseConfiguration, PA46_310P_AIRCRAFT_LABEL,
    PA46_310P_AIRCRAFT_MODEL_ID, PA46_310P_PERFORMANCE_SOURCE,
};
pub use airport_info::{
    AirportCommunicationUiView, AirportInfoUiView, AirportRunwayUiView, AirportSolarEventUiView,
};
pub use altitude_planner::{
    format_trajectory_wind, project_altitude_planner_ui, AircraftPerformanceProfile,
    AltitudeComparisonPanelUiView, AltitudeComparisonUiView, AltitudePlannerControlId,
    AltitudePlannerControlUiView, AltitudePlannerUiInput, AltitudePlannerUiView,
    AltitudePlannerUnavailableReason, AltitudePlannerUnavailableReasonCode, AtmosphereModel,
    AtmosphereSample, CruisePerformancePoint, FlightEstimateKind, FlightPlanEstimateModeUiView,
    NoWindIsaAtmosphere, PerformanceAirspeedBasis, TrajectoryLegPrediction, TrajectoryPlanInput,
    TrajectoryPlanner, TrajectoryPlannerError, TrajectoryPrediction, TrajectoryRouteLeg,
    VerticalPerformancePoint,
};
pub use chart_page::{
    airport_ids_from_plan, chart_page_airport_ids_from_plan, derive_chart_page_state_from_airports,
    derive_chart_page_state_from_collections, route_airport_ids_from_plan, ChartAssetRecord,
    ChartReferenceFamilyRecord, ChartReferenceFamilySummary, DerivedChartAirport,
    DerivedChartAirportMenuEntry, DerivedChartAsset, DerivedChartCatalog, DerivedChartPage,
    DerivedChartPageState, DerivedChartReferenceFamily,
};
pub use cloud::{
    CloudAuthorizationMode, CloudAuthorizationRequest, CloudAuthorizationResponse, CloudCompletion,
    CloudEngine, CloudEventStreamEvent, CloudEventStreamEventKind, CloudEventStreamPlan,
    CloudHttpHeader, CloudHttpMethod, CloudHttpRequest, CloudHttpResponse, CloudPersistentState,
    CloudPlatformEffect, CloudProviderKind, CloudProviderPrincipal, CloudStatusFact,
    CloudStatusSummary, CloudUiActionId, CloudUiFieldId, CloudUiFieldValue,
    ProviderAuthorizationState, UiCloudAction, UiCloudPageState, UiCloudPanel, UiCloudPanelControl,
    UiCloudPanelState, UiCloudTimeFact, UiQrCode, CLOUD_STATUS_ID,
};
pub use content::{
    AvailabilityDetail, CachedPlate, CachedTileset, ContentAvailability, ContentInventory,
    ContentPolicy, ContentReport, ContentReportItem, ContentRequirement, InstalledPackage,
};
pub use data_status::{
    DataStatusRecord, UiDataStatusBox, UiDataStatusPageFact, UiDataStatusPageRow,
    UiDataStatusPageState, UiDataStatusPageTimeDisplay, UiDataStatusState, UiStatusAction,
    UiStatusActionStyle, UiStatusSeverity, RELOAD_APPLICATION_ACTION_ID,
};
pub use debug_log::{
    core_clock_ms, core_debug_log, core_debug_log_value, core_perf_debug_log, set_core_clock_ms,
    set_core_debug_logger, CoreClockMs, CoreDebugLogger, CoreDebugTimer, CORE_VERBOSE_PERF_LOGS,
};
pub use errors::{AppError, AppErrorKind, AppResult};
pub use flight_data::{
    altitude_comparison_cells, altitude_comparison_columns, FlightDataBannerInput,
    FlightDataBannerModel, FlightDataCell, FlightDataCellTone, FlightDataColumn,
    FlightDataComputer, FlightTimeFuelEstimate,
};
pub(crate) use forecast_atmosphere::InstalledForecastAtmosphere;
pub use geodesy::{
    cross_track_left_nm, great_circle_display_path, great_circle_distance_nm,
    great_circle_intermediate, initial_course_deg,
};
pub use geometry::{GeoBounds, GeometryBundle, LatLon, MapViewport, PolygonRecord};
pub use had_ops::{
    decode_nav_db_page_resource_bytes, nav_kv_page_index_from_resource_id, run_had_operation,
    select_preferred_nav_db_candidate, CoreResourceRequest, CoreResourceSource, HadOperation,
    HadOperationOutcome, NavDbArtifactCandidate, NavDbArtifactOpenStatus, NavDbOpenController,
    NavDbOpenResult, UiInvalidation,
};
pub use ids::{AirportId, ChartFamilyId, ChartId, PackageId, PlateId, RegionId};
pub use live_feed_cache::{
    live_feed_product_registry, LiveFeedCache, LiveFeedFetchedPayload, LiveFeedInstalledPayload,
    LiveFeedInstalledState, LiveFeedInstalledSummary, LiveFeedProductDriver,
    LiveFeedProductRegistry, LiveFeedResourceManifest, LiveFeedResourceRef,
};
pub use live_feed_runtime::{
    live_feed_runtime_decision, LiveFeedConnectionEvent, LiveFeedConnectionEventKind,
    LiveFeedNetworkStatus, LiveFeedRuntimeDecision, LiveFeedRuntimeEventKind, LiveFeedRuntimeInput,
    LiveFeedRuntimeState,
};
pub use live_feeds::{
    decode_prepared_live_feed, live_feed_events_url, live_feed_status_url,
    normalize_live_feed_source_root_url, prepare_live_feed_delta_resource,
    prepare_live_feed_state_resource, prepare_notam_live_feed_delta_resource_with_work,
    prepare_notam_live_feed_state_resource_with_work, should_prepare_live_feed_resource,
    supports_prepared_live_feed, BackgroundNotamWork, LiveFeedCacheRequest,
    LiveFeedCacheRequestKind, LiveFeedDeltaRef, LiveFeedDurableInstalledProduct,
    LiveFeedPayloadRef, LiveFeedSseEvent, LiveFeedsSnapshot, LiveFeedsState,
    NotamProjectionPreparer, PreparedLiveFeedEnvelope, PreparedLiveFeedPayload,
    PreparedMetarLiveFeed, PreparedMetarTile, PreparedNotamPayload,
};
pub use map_follow::MapFollowUiState;
pub(crate) use map_overlay::query_map_overlay_for_surface_at;
pub use map_overlay::{
    aggregate_vector_tile_cache_key, airspace_feature_path, airspace_label_tile_key,
    airspace_ref_tile_key, chart_ident_label_for_nav_ref_symbol,
    map_overlay_config_from_vector_manifest_json, overlay_surface_decision,
    point_vector_record_to_symbol_feature, project_nav_symbol_feature, query_map_overlay,
    query_map_overlay_for_surface, query_map_overlay_with_point_display_scale, query_map_selection,
    query_map_selection_for_surface, query_map_selection_for_surface_in_time_zone,
    query_map_selection_with_point_display_scale, selected_map_selection_item_id_for_nav_ref,
    tile_key, visible_point_tile_window, visible_point_tile_window_with_display_scale,
    AirportNotamIndex, AirportNotamProjectionCheckpoint, AirportNotamProjectionDelta,
    AirportNotamProjectionMutation, AirportNotamProjectionRecord, AirportNotamUiView,
    AirportPlateAvailability, AirspaceDisplayLabel, AirspaceDisplayPath, AirspaceDisplayStroke,
    AirspaceDisplayStyle, AirspaceDisplaySubpath, AirspaceFeaturePath, AirspaceFeaturePayload,
    AirspaceFeatureRequest, AirspaceLabelRecord, AirspaceLabelTilePayload,
    AirspaceReferenceTilePayload, AirspaceScreenPoint, MapOverlayConfig, MapOverlayQueryResult,
    MapSelectionAction, MapSelectionCategory, MapSelectionDetailStatus,
    MapSelectionForNavRefResult, MapSelectionHighlight, MapSelectionItem, MapSelectionQueryResult,
    MapSelectionSessionAction, MapSurfaceMetrics, MetarProductPayload, MetarRecord,
    MetarTilePayload, NavSymbolFeature, NotamRecord, ObstacleOverlayContext, OfflineRegionCatalog,
    OfflineRegionDisplay, OfflineRegionRecord, OverlaySurfaceDecision, PirepRecord,
    PointTilePayload, PointVectorRecord, TafProductPayload, TafRecord, TfrAltitudeLimit,
    TfrAreaPayload, TfrLatLonPoint, TfrNotamMetadata, TfrProductPayload, TfrScheduleFragment,
    VectorAggregateTilePayload, VectorIdentLabelStyle, VectorTileRequest, VisibleMapFeature,
    VisibleMetarFeature, VisiblePirepFeature, WeatherDetailUiView, WeatherStationAirportAliases,
    AIRSPACE_DISPLAY_FEATURE_LIMIT, VECTOR_DISPLAY_FEATURE_LIMIT,
};
pub use navdb_types::{
    AirwayPresentationPlan, AirwayPresentationPoint, AirwayPresentationSelection, AirwaySuggestion,
    CifpTppMatch, CifpTppMatchRow, MaterializedProcedure, ProcedureOptions, ProcedureSpecChoice,
    ProcedureSummary, WaypointIdentifierSuggestion,
};
#[cfg(debug_assertions)]
pub use navkv::nav_kv_store_for_smoke_test;
pub use navkv::{
    nav_kv_key_for_query, NavKvLookup, NavKvPageProbeStats, NavKvQuery, NavKvRoot, NavKvStore,
    NAV_DB_CONTRACT_KEY, REQUIRED_NAV_DB_CONTRACT_ID,
};
pub use notam_state::NotamApplyWork;
pub use ownship::{
    push_sample, register_source, set_policy, situation_ring_candidates, update_source_status,
    OwnshipBannerSeverity, OwnshipControlModel, OwnshipLauncherTextTone, OwnshipMode,
    OwnshipPolicy, OwnshipRenderState, OwnshipSelectionCommand, OwnshipSelectionPolicy,
    OwnshipSourceId, OwnshipSourceKind, OwnshipSourceMenuItem, OwnshipSourceRegistration,
    OwnshipSourceStatus, OwnshipSourceStatusUpdate, OwnshipState, OwnshipTextAction,
    OwnshipUiState, ResolvedOwnshipState, SituationControlInput, SituationControlMenuItem,
    SituationKinematics, SituationRingCandidate, SituationSample, SourceConnectionState,
};
pub use package_management::{
    current_artifacts_manifest_is_supported, decode_bundle_manifest,
    decode_current_artifacts_manifest, decode_current_artifacts_manifest_list,
    default_offline_package_preferences, initialize_offline_packages,
    plan_current_artifacts_discovery, plan_offline_packages, reduce_offline_packages,
    reduce_offline_packages_controller, reduce_offline_packages_controller_owned,
    select_supported_current_artifacts_manifests, BundleManifest, BundlePackageArtifact,
    BundlePackageMetadata, CurrentArtifactsBundleRef, CurrentArtifactsBundleRequest,
    CurrentArtifactsDiscoveryPlan, CurrentArtifactsManifest, InstalledArtifact,
    OfflinePackagePreferences, OfflinePackageSelection, OfflinePackagesControllerCommand,
    OfflinePackagesControllerEvent, OfflinePackagesControllerInput,
    OfflinePackagesControllerResult, OfflinePackagesControllerState,
    OfflinePackagesControllerUiState, OfflinePackagesEvent, OfflinePackagesInitInput,
    OfflinePackagesLibraryCache, OfflinePackagesReduceInput, OfflinePackagesReduceResult,
    OfflinePackagesState, OfflinePackagesStorageInfo, OfflinePackagesSyncProgress,
    OfflinePackagesSyncSummary, OfflinePackagesUiRow, OfflinePackagesUiState,
    OfflinePackagesWarning, PackageManagementInput, PackageManagementPlan,
};
pub use planning::{
    activate_direct_to, activate_direct_to_row, activate_leg, activate_leg_at_detail_index,
    activate_next_leg, active_guidance_leg, at_fix_requirement, attached_procedure_component_index,
    basic_terminal_state, change_airway_entry, change_airway_exit,
    change_procedure_enroute_transition, change_procedure_runway_transition,
    common_resume_candidate_decision, delete_component, delete_waypoint_component,
    direct_to_fix_with_course_continuation_requirement, enter_hold_requirement,
    established_on_course_requirement, first_guidance_detail_index_for_leg,
    flatten_component_to_waypoints, flight_plan_contains_nav_ref,
    flight_plan_has_direct_to_overlay, insert_airport_waypoint, insert_airway_after_airway,
    insert_airway_after_waypoint, insert_airway_between_waypoints, insert_departure_after_airport,
    insert_initial_procedure_before_airport, insert_procedure_between_waypoints,
    insert_terminal_procedure_before_airport, insert_waypoint, intercept_course_requirement,
    materialize_airway_exit_before_component, move_component, procedure_component_index_for_load,
    project_ui_state, reconcile_handoff, reentry_to_anchor_requirement,
    remove_airway_child_waypoint, remove_all_above, remove_all_above_airway_child_waypoint,
    replace_airway_component, replace_procedure_component, restore_direct_to,
    sequence_active_detail, sequence_active_leg, start_requirement_from_leg_characteristics,
    stop_navigation, suspend_sequencing, terminal_hold_start_detail_index_for_leg,
    terminal_hold_start_element_index_for_leg, terminal_state_with_leg_characteristics,
    top_level_waypoint_component_count, top_level_waypoint_component_index, unsuspend_sequencing,
    yieldable_course_to_fix_requirement, AirwaySegment, CodedFixSatisfaction,
    CommonSegmentTerminalState, ConcretizedNavItem, DirectToState, DirectToTargetRow,
    DirectToUiView, FlightPlan, FlightPlanControlId, FlightPlanControlUiView,
    FlightPlanDisplayRowKind, FlightPlanRowActionExecution, FlightPlanRowActionId, FlightPlanRowId,
    FlightPlanUiState, GuidanceState, GuidanceUiView, HandoffDecision, HoldTerminalState,
    LegDisplayElement, LegDisplayPath, LegDisplayPathStyle, NavRef, PathTermination, PlanLeg,
    ProcedureDiscontinuity, ProcedureKind, ProcedureLegProvenance, ProcedureSegment,
    ProcedureSegmentRole, ProcedureTurnTerminalState, ResolvedLeg, ResolvedLegSource,
    RouteComponent, RouteComponentViewKind, SequencingMode, StartRequirement, TerminalState,
};
pub use playback::{PlaybackGapSpan, PlaybackStatus, PlaybackUiState};
pub use product_contracts::{SseTransportPolicy, AEROBAG_SSE_TRANSPORT_POLICY};
pub use publication::{
    nav_db_artifact_candidates_from_installed_artifacts, serialize_publication_outcome,
    CoreResourcePolicy, PublicationResolvedResource,
};
pub use raster_tiles::{
    preferred_family_map, raster_map_ui_state, raster_tile_plan, raster_tile_plan_with_options,
    select_map_family_in_catalog, select_map_in_catalog, RasterChartReferenceAction,
    RasterChartReferenceAsset, RasterChartReferenceCoverage, RasterDetailMapView,
    RasterDisplayGeometry, RasterDisplayPolygonSet, RasterInitialViewport, RasterMapCatalog,
    RasterMapFamilyOption, RasterMapUiState, RasterMapView, RasterMapViewOption, RasterPolygon,
    RasterResourceMode, RasterTileBounds, RasterTileDraw, RasterTileLevel, RasterTilePlan,
    RasterTilePlanOptions, RasterTileResource, RasterTileSource,
};
pub use session::{
    accept_disclaimer_in_session, advance_nav_kv_store_in_session_with_open_result,
    apply_situation_control_input_in_session, attach_nav_kv_store_to_session,
    attach_nav_kv_store_to_session_with_open_result, cloud_event_stream_plan_in_session,
    complete_cloud_authorization_in_session, complete_cloud_provider_request_in_session,
    configure_live_feed_source_in_session, configure_platform_capabilities_in_session,
    create_ui_session, create_ui_session_at_epoch_ms, create_ui_session_profiled,
    create_ui_session_profiled_at_epoch_ms, debug_drop_nav_kv_pages_for_attached_sessions,
    destroy_session, disengage_map_follow_in_session, drain_session_resource_effects,
    engage_map_follow_in_session, get_map_overlay_in_session,
    get_map_overlay_in_session_at_epoch_ms, get_map_overlay_in_session_with_point_display_scale,
    get_map_overlay_in_session_with_point_display_scale_at_epoch_ms,
    get_map_selection_for_nav_ref_in_session_with_point_display_scale_at_epoch_ms,
    get_map_selection_in_session, get_map_selection_in_session_at_epoch_ms,
    get_map_selection_in_session_with_point_display_scale,
    get_map_selection_in_session_with_point_display_scale_at_epoch_ms,
    get_nexrad_overlay_in_session, get_nexrad_overlay_in_session_at_epoch_ms,
    get_raster_tile_plan_in_session, get_raster_tile_plan_in_session_at_epoch_ms,
    get_raster_tile_plan_in_session_with_display_scale,
    get_raster_tile_plan_in_session_with_display_scale_at_epoch_ms,
    get_raster_tile_plan_in_session_with_options,
    get_raster_tile_plan_in_session_with_options_at_epoch_ms,
    get_scheduled_terrain_overlay_in_session_at_epoch_ms, get_session_snapshot,
    get_session_snapshot_at_epoch_ms, get_terrain_overlay_in_session,
    get_terrain_overlay_in_session_at_epoch_ms, ingest_airspace_features_in_session,
    ingest_airspace_label_tiles_in_session, ingest_airspace_ref_tiles_in_session,
    ingest_live_feed_sse_event_in_session, ingest_live_feed_sse_event_in_session_at_epoch_ms,
    ingest_live_feed_sse_events_in_session, ingest_live_feed_sse_events_in_session_at_epoch_ms,
    ingest_point_tiles_in_session, ingest_prepared_live_feed_resource_in_session,
    ingest_resource_in_session, ingest_resource_in_session_at_epoch_ms, ingest_tafs_in_session,
    ingest_tfrs_in_session, insert_nav_kv_page_for_attached_sessions,
    install_live_feed_installed_state_in_session,
    install_prepared_live_feed_cache_product_in_session, live_feed_runtime_decision_in_session,
    load_offline_package_library_cache_in_session, load_playback_trace_in_session,
    load_raster_map_catalog_in_session, maintain_nav_db_in_session_at_epoch_ms,
    nexrad_tile_bytes_in_session, pause_playback_in_session, perform_cloud_ui_action_in_session,
    perform_flight_plan_command_in_session, perform_map_selection_action_in_session,
    perform_ownship_text_action_in_session, perform_settings_action_in_session,
    perform_status_action_in_session, play_playback_in_session, prepare_nexrad_tile_in_session,
    project_flight_plan_route_in_session, push_situation_sample_in_session,
    query_flight_plan_in_session, record_offline_package_preferences_in_session,
    refresh_live_feed_current_in_session, register_ownship_source_in_session,
    render_terrain_overlay_tile_by_key_in_session, render_terrain_overlay_tile_in_session,
    render_terrain_overlay_tiles_in_session, report_cloud_event_stream_event_in_session,
    report_live_feed_connection_event_in_session, report_session_resource_failure_in_session,
    report_session_resource_failure_in_session_at_epoch_ms,
    resolve_chart_asset_resource_in_session, resolve_metar_manifest_in_session,
    resolve_nav_db_artifact_candidates_in_session, restore_chart_page_state_in_session,
    seek_playback_in_session, select_airport_in_session, select_chart_in_session,
    select_chart_reference_in_session, select_map_family_in_session,
    select_ownship_source_in_session, select_raster_map_in_session, set_debug_flag_in_session,
    set_installed_package_ids_in_session, set_map_follow_offset_in_session,
    set_map_layer_enabled_in_session, set_map_layer_visibility_in_session,
    set_playback_rate_in_session, set_resource_policy_in_session, set_situation_in_session,
    sync_guidance_geometry_in_session, sync_live_feed_catalog_in_session,
    sync_live_feeds_in_session, sync_map_follow_in_session,
    take_cloud_authorization_request_in_session, take_cloud_provider_request_in_session,
    tick_bad_autopilot_in_session, tick_playback_in_session,
    update_ownship_source_status_in_session, ClientBuildInfo, DebugFlagId, DisplayDimTimeout,
    FlightPlanSessionCommand, FlightPlanSessionQuery, GuidanceLegGeometry,
    LiveFeedAcquisitionPolicy, MapLayerId, NavDbAdvanceDisposition, NavDbAdvanceResult,
    NavDbMaintenanceAction, NavDbMaintenanceResult, PlatformCapabilities, PlatformCloudCapability,
    PlatformDisplayPolicyCapability, PlatformLiveFeedsCapability,
    PlatformOfflinePackagesCapability, SettingsPreferences, SettingsStorage, SettingsStorageHandle,
    UiChartPageState, UiDebugState, UiDisclaimerState, UiDisplayPolicy, UiHomeDestination,
    UiHomePageButton, UiHomePageState, UiMapLayerState, UiMapLayerToggleState, UiNavDbIdentity,
    UiPlaybackPanelState, UiSessionInitResult, UiSessionResourceEffect, UiSessionSnapshot,
    UiSettingsAction, UiSettingsGridItem, UiSettingsPageRow, UiSettingsPageState,
    UiSettingsSliderStop,
};
pub use situation::{Situation, SituationPosition};
pub use state::{project_app_ui_state, AppEvent, AppState, AppUiState};
pub use terrain::{
    parse_abt2_tile, prepare_terrain_overlay_frame, query_terrain_overlay,
    query_terrain_overlay_with_available_packages, render_terrain_warning_raw_rgba_from_tiles,
    render_terrain_warning_rgba, schedule_terrain_overlay_frame, terrain_altitude_bucket_ft,
    TerrainOverlayQueryResult, TerrainOverlayScheduleDecision, TerrainOverlaySourceTile,
    TerrainOverlayStatus, TerrainOverlayTileRequest, TerrainTileInfo,
};
pub use ui_work_scheduler::{
    SessionSnapshotRefreshDecision, SessionSnapshotRefreshPriority,
    SessionSnapshotRefreshScheduler, SessionSnapshotRefreshSchedulerConfig,
    UiSessionWorkCompletionDecision, UiSessionWorkKind, UiSessionWorkRequest,
    UiSessionWorkRequestDecision, UiSessionWorkResultAction, UiSessionWorkScheduler,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlateProcedureLoadCandidateInput {
    pub airport_id: String,
    pub cifp_id: String,
    pub match_rows: Vec<CifpTppMatchRow>,
    pub options: ProcedureOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureLoadOption {
    pub load_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureLoadMenu {
    pub procedure_kind: Option<ProcedureKind>,
    pub launcher_label: String,
    pub header: String,
    pub header_tone: ProcedureLoadHeaderTone,
    pub options: Vec<ProcedureLoadOption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureLoadHeaderTone {
    Normal,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcedureLoadPlanTarget {
    ExistingOrigin { row_uid: String },
    PrependOrigin,
    ExistingDestination { row_uid: String },
    AppendDestination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureLoadCommand {
    pub target: ProcedureLoadPlanTarget,
    pub airport_id: String,
    pub procedure_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightPlanRouteSegmentStatus {
    Completed,
    Active,
    ActiveLegRemaining,
    Remaining,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanRouteSegment {
    pub id: String,
    pub leg_id: String,
    pub from: LatLon,
    pub to: LatLon,
    pub path: Vec<LatLon>,
    #[serde(default)]
    pub style: LegDisplayPathStyle,
    pub geometry: GuidanceRouteGeometry,
    pub distance_nm: f64,
    pub course_deg: f64,
    pub status: FlightPlanRouteSegmentStatus,
    #[serde(default)]
    pub finish_lines: Vec<FlightPlanRouteFinishLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanRouteDistanceAnnotation {
    pub id: String,
    pub segment_indexes: Vec<usize>,
    pub text: String,
    pub distance_nm: f64,
    pub status: FlightPlanRouteSegmentStatus,
    pub required_feature_ids: Vec<String>,
    pub minimum_path_to_pill_width_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanRouteProjection {
    pub flight_plan_route_revision: u64,
    pub segments: Vec<FlightPlanRouteSegment>,
    #[serde(default)]
    pub distance_annotations: Vec<FlightPlanRouteDistanceAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanRouteFinishLine {
    pub start: LatLon,
    pub end: LatLon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuidanceRouteGeometry {
    Segment {
        start: LatLon,
        end: LatLon,
    },
    Arc {
        center: LatLon,
        radius_nm: f64,
        start: LatLon,
        end: LatLon,
        clockwise: bool,
        sweep_degrees: f64,
    },
}

pub(crate) fn guidance_detail_count_for_leg(leg: &ResolvedLeg) -> usize {
    leg.procedure_provenance
        .as_ref()
        .and_then(|provenance| provenance.display_path.as_ref())
        .map(|path| path.elements.len().max(1))
        .unwrap_or(1)
}

pub(crate) fn guidance_detail_id_for_leg_element(
    leg_index: usize,
    leg: &ResolvedLeg,
    element_index: usize,
) -> String {
    format!("leg:{leg_index}:{}#{element_index}", leg.id)
}

fn guidance_route_geometry_from_display_element(
    element: &LegDisplayElement,
) -> GuidanceRouteGeometry {
    match element {
        LegDisplayElement::Segment { start, end } => GuidanceRouteGeometry::Segment {
            start: *start,
            end: *end,
        },
        LegDisplayElement::Arc {
            center,
            radius_nm,
            start,
            end,
            clockwise,
            sweep_degrees,
        } => GuidanceRouteGeometry::Arc {
            center: *center,
            radius_nm: *radius_nm,
            start: *start,
            end: *end,
            clockwise: *clockwise,
            sweep_degrees: *sweep_degrees,
        },
    }
}

fn guidance_route_path_from_geometry(geometry: &GuidanceRouteGeometry) -> Vec<LatLon> {
    match geometry {
        GuidanceRouteGeometry::Segment { start, end } => great_circle_display_path(*start, *end),
        GuidanceRouteGeometry::Arc {
            center,
            radius_nm,
            start,
            clockwise,
            sweep_degrees,
            ..
        } => {
            let start_bearing = route_bearing_from(*center, *start);
            let sweep = if *clockwise {
                sweep_degrees.abs()
            } else {
                -sweep_degrees.abs()
            };
            let steps = usize::max(8, (sweep.abs() / 15.0).ceil() as usize);
            let mut path = Vec::with_capacity(steps + 1);
            for index in 0..=steps {
                let fraction = index as f64 / steps as f64;
                let bearing = start_bearing + sweep * fraction;
                let point = route_destination_point(*center, bearing, *radius_nm);
                if path.last().copied() != Some(point) {
                    path.push(point);
                }
            }
            path
        }
    }
}

fn guidance_route_distance_nm(geometry: &GuidanceRouteGeometry) -> f64 {
    match geometry {
        GuidanceRouteGeometry::Segment { start, end } => flight_leg_distance_nm(*start, *end),
        GuidanceRouteGeometry::Arc {
            radius_nm,
            sweep_degrees,
            ..
        } => radius_nm * sweep_degrees.to_radians().abs(),
    }
}

fn guidance_route_course_deg(geometry: &GuidanceRouteGeometry) -> f64 {
    match geometry {
        GuidanceRouteGeometry::Segment { start, end } => flight_leg_course_deg(*start, *end),
        GuidanceRouteGeometry::Arc {
            center,
            start,
            clockwise,
            ..
        } => {
            let radial_deg = bearing_degrees(*center, *start);
            normalize_bearing_degrees(if *clockwise {
                radial_deg + 90.0
            } else {
                radial_deg - 90.0
            })
        }
    }
}

fn guidance_route_terminal_course_deg(geometry: &GuidanceRouteGeometry) -> f64 {
    match geometry {
        GuidanceRouteGeometry::Segment { start, end } => flight_leg_course_deg(*start, *end),
        GuidanceRouteGeometry::Arc {
            center,
            end,
            clockwise,
            ..
        } => {
            let radial_deg = bearing_degrees(*center, *end);
            normalize_bearing_degrees(if *clockwise {
                radial_deg + 90.0
            } else {
                radial_deg - 90.0
            })
        }
    }
}

fn sequencing_plane_finish_lines(
    current: &GuidanceRouteGeometry,
    next: Option<&GuidanceRouteGeometry>,
) -> Vec<FlightPlanRouteFinishLine> {
    let (_, intersection) = guidance_route_endpoints(current);
    let inbound_course = guidance_route_terminal_course_deg(current);
    let outbound_course = next
        .map(guidance_route_course_deg)
        .unwrap_or(inbound_course);
    sequencing::plane_finish_criterion(intersection, inbound_course, outbound_course).finish_lines()
}

fn sequencing_arc_finish_lines(
    current: &GuidanceRouteGeometry,
) -> Option<Vec<FlightPlanRouteFinishLine>> {
    let GuidanceRouteGeometry::Arc {
        center,
        end,
        clockwise,
        sweep_degrees,
        ..
    } = current
    else {
        return None;
    };
    sequencing::arc_finish_criterion(*center, *end, *clockwise, *sweep_degrees)
        .map(sequencing::SequencingFinishCriterion::finish_lines)
}

fn sequencing_finish_lines(
    current: &GuidanceRouteGeometry,
    next: Option<&GuidanceRouteGeometry>,
) -> Vec<FlightPlanRouteFinishLine> {
    sequencing_arc_finish_lines(current)
        .unwrap_or_else(|| sequencing_plane_finish_lines(current, next))
}

fn guidance_route_endpoints(geometry: &GuidanceRouteGeometry) -> (LatLon, LatLon) {
    match geometry {
        GuidanceRouteGeometry::Segment { start, end } => (*start, *end),
        GuidanceRouteGeometry::Arc { start, end, .. } => (*start, *end),
    }
}

fn route_bearing_from(from: LatLon, to: LatLon) -> f64 {
    let from_lat = from.lat.to_radians();
    let to_lat = to.lat.to_radians();
    let delta_lon = (to.lon - from.lon).to_radians();
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    normalize_bearing_degrees(y.atan2(x).to_degrees())
}

fn route_destination_point(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
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

pub(crate) fn project_flight_plan_route_with_resolver<E, F>(
    plan: &FlightPlan,
    mut resolve_position: F,
) -> Result<Vec<FlightPlanRouteSegment>, E>
where
    F: FnMut(&NavRef, Option<&str>) -> Result<LatLon, E>,
{
    let mut route = Vec::new();
    let route_statuses = flight_plan_materialization::route_statuses_for_plan(plan);
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        route.extend(project_flight_plan_leg_route_with_statuses(
            leg_index,
            leg,
            &route_statuses[leg_index],
            &mut resolve_position,
        )?);
    }
    let route_geometries = route
        .iter()
        .map(|segment| segment.geometry.clone())
        .collect::<Vec<_>>();
    for index in 0..route.len() {
        if route[index].style != LegDisplayPathStyle::Vectors {
            route[index].finish_lines =
                sequencing_finish_lines(&route_geometries[index], route_geometries.get(index + 1));
        }
    }
    if let Some(direct_to) = plan
        .guidance
        .as_ref()
        .filter(|guidance| guidance.sequencing_mode == SequencingMode::DirectTo)
        .and_then(|guidance| guidance.direct_to.as_ref())
    {
        let resume_geometry = direct_to
            .resume_row_id
            .as_ref()
            .and_then(|_| planning::direct_to_resume_leg_index(plan, direct_to))
            .and_then(|resume_leg_index| {
                let segment_index = plan.resolved_legs[..resume_leg_index]
                    .iter()
                    .map(guidance_detail_count_for_leg)
                    .sum::<usize>();
                route.get(segment_index)
            })
            .map(|segment| segment.geometry.clone());
        let from = resolve_position(&direct_to.start, None)?;
        let to = resolve_position(&direct_to.target, None)?;
        let geometry = GuidanceRouteGeometry::Segment {
            start: from,
            end: to,
        };
        route.push(FlightPlanRouteSegment {
            id: "direct-to".to_string(),
            leg_id: "direct-to".to_string(),
            from,
            to,
            path: guidance_route_path_from_geometry(&geometry),
            style: LegDisplayPathStyle::Solid,
            geometry: geometry.clone(),
            distance_nm: guidance_route_distance_nm(&geometry),
            course_deg: guidance_route_course_deg(&geometry),
            status: FlightPlanRouteSegmentStatus::Active,
            finish_lines: sequencing_finish_lines(&geometry, resume_geometry.as_ref()),
        });
    }
    Ok(route)
}

const MINIMUM_ROUTE_PATH_TO_DISTANCE_PILL_WIDTH_RATIO: f64 = 1.6;

pub(crate) fn project_flight_plan_route_distance_annotations(
    plan: &FlightPlan,
    route: &[FlightPlanRouteSegment],
) -> AppResult<Vec<FlightPlanRouteDistanceAnnotation>> {
    let mut annotations = Vec::new();
    let mut segment_index = 0;

    for leg in &plan.resolved_legs {
        let segment_count = guidance_detail_count_for_leg(leg);
        let end_index = segment_index + segment_count;
        let segments = route
            .get(segment_index..end_index)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::Internal,
                message: format!(
                    "route projection omitted segments for resolved leg {}",
                    leg.id
                ),
            })?;
        if segments.iter().any(|segment| segment.leg_id != leg.id) {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: format!(
                    "route projection segment order does not match resolved leg {}",
                    leg.id
                ),
            });
        }

        let eligible_procedure_leg = !planning::resolved_leg_ends_in_manual_sequence(leg)
            && leg.procedure_provenance.as_ref().is_none_or(|provenance| {
                !matches!(
                    provenance.path_termination,
                    PathTermination::HeadingToManual | PathTermination::HeadingToAltitude
                ) && !matches!(
                    &provenance.path_termination,
                    PathTermination::Other(code) if matches!(code.trim(), "HA" | "HF" | "HM")
                )
            });
        if eligible_procedure_leg {
            let from_feature_id = flight_plan_waypoint_feature_id(&leg.from);
            let to_feature_id = flight_plan_waypoint_feature_id(&leg.to);
            if let (Some(from_feature_id), Some(to_feature_id)) = (from_feature_id, to_feature_id) {
                if from_feature_id != to_feature_id {
                    push_route_distance_annotation(
                        &mut annotations,
                        leg.id.clone(),
                        (segment_index..end_index).collect(),
                        segments,
                        vec![from_feature_id, to_feature_id],
                    );
                }
            }
        }
        segment_index = end_index;
    }

    if let Some(direct_to) = plan
        .guidance
        .as_ref()
        .filter(|guidance| guidance.sequencing_mode == SequencingMode::DirectTo)
        .and_then(|guidance| guidance.direct_to.as_ref())
    {
        let segment = route.get(segment_index).ok_or_else(|| AppError {
            kind: AppErrorKind::Internal,
            message: "route projection omitted active direct-to segment".to_string(),
        })?;
        if segment.leg_id != "direct-to" || route.len() != segment_index + 1 {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: "route projection direct-to segment order is invalid".to_string(),
            });
        }
        if let Some(target_feature_id) = flight_plan_waypoint_feature_id(&direct_to.target) {
            push_route_distance_annotation(
                &mut annotations,
                "direct-to".to_string(),
                vec![segment_index],
                std::slice::from_ref(segment),
                vec![target_feature_id],
            );
        }
        segment_index += 1;
    }

    if segment_index != route.len() {
        return Err(AppError {
            kind: AppErrorKind::Internal,
            message: "route projection contains segments without a resolved flight-plan leg"
                .to_string(),
        });
    }
    Ok(annotations)
}

fn push_route_distance_annotation(
    annotations: &mut Vec<FlightPlanRouteDistanceAnnotation>,
    id: String,
    segment_indexes: Vec<usize>,
    segments: &[FlightPlanRouteSegment],
    required_feature_ids: Vec<String>,
) {
    let distance_nm = segments
        .iter()
        .map(|segment| segment.distance_nm)
        .sum::<f64>();
    if !distance_nm.is_finite() || distance_nm <= 0.0 {
        return;
    }
    let status = if segments.iter().any(|segment| {
        matches!(
            segment.status,
            FlightPlanRouteSegmentStatus::Active | FlightPlanRouteSegmentStatus::ActiveLegRemaining
        )
    }) {
        FlightPlanRouteSegmentStatus::Active
    } else if segments
        .iter()
        .all(|segment| segment.status == FlightPlanRouteSegmentStatus::Completed)
    {
        FlightPlanRouteSegmentStatus::Completed
    } else {
        FlightPlanRouteSegmentStatus::Remaining
    };
    annotations.push(FlightPlanRouteDistanceAnnotation {
        id,
        segment_indexes,
        text: format!("{}nm", flight_data::format_nm(distance_nm)),
        distance_nm,
        status,
        required_feature_ids,
        minimum_path_to_pill_width_ratio: MINIMUM_ROUTE_PATH_TO_DISTANCE_PILL_WIDTH_RATIO,
    });
}

pub(crate) fn flight_plan_waypoint_feature_id(nav_ref: &NavRef) -> Option<String> {
    if matches!(nav_ref, NavRef::LatLon(_) | NavRef::Spot(_)) {
        return None;
    }
    Some(format!(
        "flight-plan:{}",
        serde_json::to_string(nav_ref).expect("NavRef must always serialize as JSON")
    ))
}

pub(crate) fn project_flight_plan_leg_route_with_resolver<E, F>(
    plan: &FlightPlan,
    leg_index: usize,
    leg: &ResolvedLeg,
    resolve_position: &mut F,
) -> Result<Vec<FlightPlanRouteSegment>, E>
where
    F: FnMut(&NavRef, Option<&str>) -> Result<LatLon, E>,
{
    let route_statuses = flight_plan_materialization::route_statuses_for_plan(plan);
    project_flight_plan_leg_route_with_statuses(
        leg_index,
        leg,
        &route_statuses[leg_index],
        resolve_position,
    )
}

fn project_flight_plan_leg_route_with_statuses<E, F>(
    leg_index: usize,
    leg: &ResolvedLeg,
    statuses: &[FlightPlanRouteSegmentStatus],
    resolve_position: &mut F,
) -> Result<Vec<FlightPlanRouteSegment>, E>
where
    F: FnMut(&NavRef, Option<&str>) -> Result<LatLon, E>,
{
    let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
        (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
    });
    let mut route = Vec::new();
    if let Some(display_path) = leg
        .procedure_provenance
        .as_ref()
        .and_then(|provenance| provenance.display_path.as_ref())
    {
        let ends_in_vectors = planning::resolved_leg_ends_in_manual_sequence(leg);
        for (element_index, element) in display_path.elements.iter().enumerate() {
            let geometry = guidance_route_geometry_from_display_element(element);
            let (from, to) = guidance_route_endpoints(&geometry);
            route.push(FlightPlanRouteSegment {
                id: guidance_detail_id_for_leg_element(leg_index, leg, element_index),
                leg_id: leg.id.clone(),
                from,
                to,
                path: guidance_route_path_from_geometry(&geometry),
                style: if ends_in_vectors && element_index + 1 == display_path.elements.len() {
                    LegDisplayPathStyle::Vectors
                } else {
                    display_path.style.clone()
                },
                geometry: geometry.clone(),
                distance_nm: guidance_route_distance_nm(&geometry),
                course_deg: guidance_route_course_deg(&geometry),
                status: statuses[element_index].clone(),
                finish_lines: Vec::new(),
            });
        }
    } else {
        let from = resolve_position(&leg.from, procedure_airport_id)?;
        let to = resolve_position(&leg.to, procedure_airport_id)?;
        let geometry = GuidanceRouteGeometry::Segment {
            start: from,
            end: to,
        };
        route.push(FlightPlanRouteSegment {
            id: guidance_detail_id_for_leg_element(leg_index, leg, 0),
            leg_id: leg.id.clone(),
            from,
            to,
            path: guidance_route_path_from_geometry(&geometry),
            style: LegDisplayPathStyle::Solid,
            geometry: geometry.clone(),
            distance_nm: guidance_route_distance_nm(&geometry),
            course_deg: guidance_route_course_deg(&geometry),
            status: statuses[0].clone(),
            finish_lines: Vec::new(),
        });
    }
    Ok(route)
}

pub fn load_geometry(geometry_json: &str) -> AppResult<GeometryBundle> {
    serde_json::from_str(geometry_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse geometry json: {err}"),
    })
}

pub fn build_flight_plan(plan: FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.normalized();

    planning::validate_procedure_attachments(&plan.route_components)?;

    if plan.resolved_legs.is_empty() && plan.route_components.len() > 1 {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg".to_string(),
        });
    }

    planning::validate_final_procedure_geometry(&plan.resolved_legs)?;

    Ok(plan)
}

pub fn classify_procedure_identifier(
    identifier: &str,
    exists_as_airport: bool,
    exists_as_navaid: bool,
    exists_as_fix: bool,
) -> Option<NavRef> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("RW") {
        return Some(NavRef::Fix(trimmed.to_string()));
    }
    if exists_as_navaid {
        return Some(NavRef::Navaid(trimmed.to_string()));
    }
    if exists_as_airport {
        return Some(NavRef::Airport(trimmed.to_string()));
    }
    if exists_as_fix {
        return Some(NavRef::Fix(trimmed.to_string()));
    }
    None
}

pub(crate) fn prepare_airway_presentation(
    airway_name: &str,
    branches: Vec<navdb_types::AirwayBranch>,
    origin_position: LatLon,
    destination_position: Option<LatLon>,
) -> AppResult<AirwayPresentationPlan> {
    let mut best: Option<(f64, AirwayPresentationPlan)> = None;

    for branch in branches
        .into_iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
    {
        if branch.points.is_empty() {
            continue;
        }
        let mut entry_index = 0usize;
        let mut exit_index = branch.points.len().saturating_sub(1);
        let mut entry_distance = f64::MAX;
        let mut exit_distance = 0.0;

        for (index, point) in branch.points.iter().enumerate() {
            let origin_distance = distance_nm(origin_position, point.position);
            if origin_distance < entry_distance {
                entry_distance = origin_distance;
                entry_index = index;
            }
            if let Some(destination_position) = destination_position {
                let destination_distance = distance_nm(destination_position, point.position);
                if index == 0 || destination_distance < exit_distance {
                    exit_distance = destination_distance;
                    exit_index = index;
                }
            }
        }

        let mut points = branch
            .points
            .iter()
            .enumerate()
            .map(|(branch_point_index, point)| AirwayPresentationPoint {
                uid: airway_presentation_point_uid(
                    &branch.branch_key,
                    branch_point_index,
                    point.sequence,
                ),
                sequence: point.sequence,
                nav_ref: point.nav_ref.clone(),
            })
            .collect::<Vec<_>>();

        let mut suggested_entry_index = entry_index;
        let mut suggested_exit_index = destination_position.map(|_| exit_index);
        if destination_position.is_some() && entry_index > exit_index {
            points.reverse();
            suggested_entry_index = points.len() - 1 - entry_index;
            suggested_exit_index = Some(points.len() - 1 - exit_index);
        }
        let suggested_entry_uid = points[suggested_entry_index].uid.clone();
        let suggested_exit_uid = suggested_exit_index.map(|index| points[index].uid.clone());

        let score = entry_distance + exit_distance;
        let presentation = AirwayPresentationPlan {
            airway_name: branch.display_name,
            branch_key: branch.branch_key,
            points,
            suggested_entry_uid,
            suggested_exit_uid,
        };

        if best
            .as_ref()
            .is_none_or(|(current_score, _)| score < *current_score)
        {
            best = Some((score, presentation));
        }
    }

    best.map(|(_, presentation)| presentation)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("no airway branches found for {}", airway_name.trim()),
        })
}

pub(crate) fn airway_presentation_point_uid(
    branch_key: &str,
    branch_point_index: usize,
    sequence: i32,
) -> String {
    // Keep branch internals opaque at the UI boundary while making choices reproducible.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in format!("{branch_key}\0{branch_point_index}\0{sequence}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("airway-point-{hash:016x}")
}

pub fn sort_airway_suggestions_for_ui(
    mut suggestions: Vec<AirwaySuggestion>,
) -> Vec<AirwaySuggestion> {
    suggestions.sort_by(|left, right| {
        compare_airway_name_for_ui(&left.airway_name, &right.airway_name).then_with(|| {
            left.distance_from_anchor_nm
                .partial_cmp(&right.distance_from_anchor_nm)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    suggestions
}

pub fn select_preferred_cifp_tpp_match(rows: Vec<CifpTppMatchRow>) -> Option<CifpTppMatch> {
    rows.into_iter()
        .map(|row| CifpTppMatch {
            airport_id: row.airport_id,
            cifp_id: row.cifp_id,
            procedure_kind: row.procedure_kind,
            plate_id: row.plate_id,
            plate_label: row.plate_label,
            package_id: row.package_id,
            match_kind: row.match_kind,
            is_primary: row.is_primary != 0,
        })
        .min_by(|left, right| {
            right
                .is_primary
                .cmp(&left.is_primary)
                .then_with(|| left.match_kind.cmp(&right.match_kind))
                .then_with(|| left.plate_label.cmp(&right.plate_label))
        })
}

pub fn describe_show_plate_for_procedure(rows: Vec<CifpTppMatchRow>) -> Option<CifpTppMatch> {
    select_preferred_cifp_tpp_match(rows)
}

pub fn list_approach_procedures_from_match_rows(
    airport_id: &str,
    rows: Vec<CifpTppMatchRow>,
) -> AppResult<Vec<ProcedureSummary>> {
    let mut rows_by_procedure = std::collections::BTreeMap::<String, Vec<CifpTppMatchRow>>::new();
    for row in rows.into_iter().filter(|row| {
        row.airport_id.trim() == airport_id.trim() && row.procedure_kind == ProcedureKind::Approach
    }) {
        let procedure_id = row.cifp_id.trim();
        if !procedure_id.is_empty() {
            rows_by_procedure
                .entry(procedure_id.to_string())
                .or_default()
                .push(row);
        }
    }
    let mut summaries = Vec::new();
    for (procedure_id, rows) in rows_by_procedure {
        let matched = select_preferred_cifp_tpp_match(rows).ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("approach {airport_id} {procedure_id} has no preferred plate label"),
        })?;
        let display_label = matched.plate_label.trim();
        if display_label.is_empty() {
            return Err(AppError {
                kind: AppErrorKind::InvalidCatalog,
                message: format!("approach {airport_id} {procedure_id} has an empty plate label"),
            });
        }
        summaries.push(ProcedureSummary {
            airport_id: airport_id.trim().to_string(),
            procedure_id,
            display_label: display_label.to_string(),
            kind: ProcedureKind::Approach,
        });
    }
    disambiguate_duplicate_procedure_display_labels(&mut summaries);
    Ok(summaries)
}

pub(crate) fn disambiguate_duplicate_procedure_display_labels(procedures: &mut [ProcedureSummary]) {
    let mut label_counts = std::collections::BTreeMap::<(String, String), usize>::new();
    for procedure in procedures.iter() {
        *label_counts
            .entry((
                procedure.airport_id.trim().to_string(),
                procedure.display_label.trim().to_string(),
            ))
            .or_default() += 1;
    }
    for procedure in procedures {
        let key = (
            procedure.airport_id.trim().to_string(),
            procedure.display_label.trim().to_string(),
        );
        if label_counts.get(&key).copied().unwrap_or_default() > 1 {
            procedure.display_label = pilot_facing_duplicate_procedure_label(procedure);
        }
    }
}

fn pilot_facing_duplicate_procedure_label(procedure: &ProcedureSummary) -> String {
    let label = procedure.display_label.trim();
    let procedure_id = procedure.procedure_id.trim();
    let prefix = procedure_id.chars().next();
    if let Some((left, right)) = label.split_once(" or ") {
        match prefix {
            Some('I') => {
                if let Some(runway) = procedure_runway_designator(procedure_id) {
                    return format!("{left} {runway}");
                }
            }
            Some('L') => return right.to_string(),
            _ => {}
        }
    }
    if let Some(runway) = procedure_runway_designator(procedure_id) {
        if let Some((left, right_side)) = label.rsplit_once(" and ") {
            if matches!(right_side, "L" | "R" | "C") {
                if let Some((prefix, published_runway)) = left.rsplit_once(' ') {
                    if published_runway
                        .chars()
                        .last()
                        .is_some_and(|side| matches!(side, 'L' | 'R' | 'C'))
                    {
                        return format!("{prefix} {runway}");
                    }
                }
            }
        }
    }
    format!("{label} ({procedure_id})")
}

fn procedure_runway_designator(procedure_id: &str) -> Option<&str> {
    let base = procedure_id.split('-').next()?;
    let runway = base.get(1..)?;
    (!runway.is_empty()
        && runway
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, 'L' | 'R' | 'C')))
    .then_some(runway)
}

pub fn describe_plate_procedure_load_menu(
    plan: &FlightPlan,
    candidates: Vec<PlateProcedureLoadCandidateInput>,
) -> AppResult<ProcedureLoadMenu> {
    let plan = plan.clone().normalized();
    let airport_id = candidates
        .first()
        .map(|candidate| candidate.airport_id.trim().to_string())
        .unwrap_or_default();
    let procedure_kind = candidates
        .iter()
        .flat_map(|candidate| candidate.match_rows.iter())
        .map(|row| row.procedure_kind.clone())
        .next();
    let Some(procedure_kind) = procedure_kind else {
        return Ok(empty_procedure_load_menu());
    };
    if candidates.iter().any(|candidate| {
        candidate
            .match_rows
            .iter()
            .any(|row| row.procedure_kind != procedure_kind)
            || candidate.options.kind != procedure_kind
    }) {
        return Err(AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("plate procedure candidates mix procedure kinds for {airport_id}"),
        });
    }
    let (header, header_tone, target) =
        plate_procedure_load_context(&plan, &airport_id, &procedure_kind)?;
    let mut choices = std::collections::BTreeMap::<
        (Option<String>, Option<String>),
        (CifpTppMatch, ProcedureSpecChoice),
    >::new();
    for candidate in candidates {
        let Some(preferred) = select_preferred_cifp_tpp_match(candidate.match_rows) else {
            continue;
        };
        if preferred.airport_id.trim() != airport_id {
            return Err(AppError {
                kind: AppErrorKind::InvalidCatalog,
                message: format!(
                    "plate procedure candidates mix airports {airport_id} and {}",
                    preferred.airport_id.trim()
                ),
            });
        }
        for choice in candidate.options.valid_choices {
            let key = (
                choice.runway_transition.clone(),
                choice.enroute_transition.clone(),
            );
            match choices.entry(key) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((preferred.clone(), choice));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if plate_procedure_preference(&preferred)
                        < plate_procedure_preference(&entry.get().0)
                    {
                        entry.insert((preferred.clone(), choice));
                    }
                }
            }
        }
    }
    let options = choices
        .into_values()
        .map(|(preferred, choice)| {
            let command = ProcedureLoadCommand {
                target: target.clone(),
                airport_id: preferred.airport_id.trim().to_string(),
                procedure_id: preferred.cifp_id.trim().to_string(),
                display_label: Some(preferred.plate_label.trim().to_string()),
                kind: procedure_kind.clone(),
                runway_transition: choice.runway_transition.clone(),
                enroute_transition: choice.enroute_transition.clone(),
            };
            Ok(ProcedureLoadOption {
                load_id: serde_json::to_string(&command).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
                label: procedure_load_entry_label(&procedure_kind, &choice),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ProcedureLoadMenu {
        procedure_kind: Some(procedure_kind.clone()),
        launcher_label: procedure_load_launcher_label(&procedure_kind).to_string(),
        header,
        header_tone,
        options,
    })
}

fn plate_procedure_load_context(
    plan: &FlightPlan,
    airport_id: &str,
    procedure_kind: &ProcedureKind,
) -> AppResult<(String, ProcedureLoadHeaderTone, ProcedureLoadPlanTarget)> {
    if *procedure_kind == ProcedureKind::Sid {
        return plate_departure_load_context(plan, airport_id);
    }
    let destination_index = plan.route_components.len().checked_sub(1).filter(|index| {
        matches!(
            plan.route_components.get(*index),
            Some(RouteComponent::Waypoint { waypoint: NavRef::Airport(code) })
                if code.trim() == airport_id
        )
    });
    let Some(destination_index) = destination_index else {
        return Ok((
            format!(
                "Append {airport_id} to plan and load {}",
                procedure_kind_noun(procedure_kind)
            ),
            ProcedureLoadHeaderTone::Normal,
            ProcedureLoadPlanTarget::AppendDestination,
        ));
    };
    let destination_row_uid = project_ui_state(plan)
        .display_rows
        .into_iter()
        .find(|row| {
            row.depth == 0
                && row.component_index == Some(destination_index)
                && row.nav_ref
                    .as_ref()
                    .is_some_and(|nav_ref| matches!(nav_ref, NavRef::Airport(code) if code.trim() == airport_id.trim()))
        })
        .map(|row| row.uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("procedure load target row missing for airport {airport_id}"),
        })?;
    let has_current_procedure =
        attached_procedure_component_index(plan, destination_index, procedure_kind.clone())
            .is_some();
    Ok((
        if has_current_procedure {
            format!("Replace current {}", procedure_kind_noun(procedure_kind))
        } else {
            format!("Load {}", procedure_kind_noun(procedure_kind))
        },
        if has_current_procedure {
            ProcedureLoadHeaderTone::Destructive
        } else {
            ProcedureLoadHeaderTone::Normal
        },
        ProcedureLoadPlanTarget::ExistingDestination {
            row_uid: destination_row_uid,
        },
    ))
}

fn plate_departure_load_context(
    plan: &FlightPlan,
    airport_id: &str,
) -> AppResult<(String, ProcedureLoadHeaderTone, ProcedureLoadPlanTarget)> {
    let origin_index = (!plan.route_components.is_empty()
        && matches!(
            plan.route_components.first(),
            Some(RouteComponent::Waypoint { waypoint: NavRef::Airport(code) })
                if code.trim() == airport_id.trim()
        ))
    .then_some(0usize);
    let Some(origin_index) = origin_index else {
        return Ok((
            format!("Prepend {airport_id} to plan and load departure"),
            ProcedureLoadHeaderTone::Normal,
            ProcedureLoadPlanTarget::PrependOrigin,
        ));
    };
    let origin_row_uid = project_ui_state(plan)
        .display_rows
        .into_iter()
        .find(|row| row.depth == 0 && row.component_index == Some(origin_index))
        .map(|row| row.uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("departure load target row missing for airport {airport_id}"),
        })?;
    let has_current_departure = matches!(
        plan.route_components.get(origin_index + 1),
        Some(RouteComponent::Procedure { procedure })
            if procedure.kind == ProcedureKind::Sid
                && procedure.airport_id.0.trim() == airport_id.trim()
    );
    Ok((
        if has_current_departure {
            "Replace current departure".to_string()
        } else {
            "Load departure".to_string()
        },
        if has_current_departure {
            ProcedureLoadHeaderTone::Destructive
        } else {
            ProcedureLoadHeaderTone::Normal
        },
        ProcedureLoadPlanTarget::ExistingOrigin {
            row_uid: origin_row_uid,
        },
    ))
}

fn empty_procedure_load_menu() -> ProcedureLoadMenu {
    ProcedureLoadMenu {
        procedure_kind: None,
        launcher_label: "LOAD\nPROC".to_string(),
        header: "No loadable procedure".to_string(),
        header_tone: ProcedureLoadHeaderTone::Normal,
        options: Vec::new(),
    }
}

fn procedure_kind_noun(kind: &ProcedureKind) -> &'static str {
    match kind {
        ProcedureKind::Sid => "departure",
        ProcedureKind::Star => "arrival",
        ProcedureKind::Approach => "approach",
    }
}

fn procedure_load_launcher_label(kind: &ProcedureKind) -> &'static str {
    match kind {
        ProcedureKind::Sid => "LOAD\nDEP",
        ProcedureKind::Star => "LOAD\nARR",
        ProcedureKind::Approach => "LOAD\nAPPCH",
    }
}

fn plate_procedure_preference(procedure: &CifpTppMatch) -> (u8, &str) {
    let rank = match procedure.cifp_id.trim().chars().next() {
        Some('I') => 0,
        Some('L') => 1,
        _ => 2,
    };
    (rank, procedure.cifp_id.trim())
}

fn procedure_load_entry_label(kind: &ProcedureKind, choice: &ProcedureSpecChoice) -> String {
    let runway = choice.runway_transition.as_deref().map(str::trim);
    let enroute = choice.enroute_transition.as_deref().map(str::trim);
    match (kind, runway, enroute) {
        (ProcedureKind::Sid, Some(runway), Some(enroute)) => {
            format!("via {runway} to {enroute}")
        }
        (ProcedureKind::Sid, Some(runway), None) => format!("via {runway}"),
        (ProcedureKind::Sid, None, Some(enroute)) => format!("to {enroute}"),
        (ProcedureKind::Star, Some(runway), Some(enroute)) => {
            format!("from {enroute} to {runway}")
        }
        (ProcedureKind::Star, Some(runway), None) => format!("to {runway}"),
        (_, _, Some(enroute)) => format!("from {enroute}"),
        (_, Some(runway), None) => format!("from {runway}"),
        _ => "published route".to_string(),
    }
}

pub fn flight_leg_distance_nm(first: LatLon, second: LatLon) -> f64 {
    great_circle_distance_nm(first, second)
}

pub fn flight_leg_course_deg(from: LatLon, to: LatLon) -> f64 {
    initial_course_deg(from, to)
}

fn distance_nm(first: LatLon, second: LatLon) -> f64 {
    flight_leg_distance_nm(first, second)
}

fn bearing_degrees(from: LatLon, to: LatLon) -> f64 {
    flight_leg_course_deg(from, to)
}

fn normalize_bearing_degrees(bearing_deg: f64) -> f64 {
    bearing_deg.rem_euclid(360.0)
}

fn compare_airway_name_for_ui(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parsed = parse_airway_name_for_ui(left);
    let right_parsed = parse_airway_name_for_ui(right);
    left_parsed
        .0
        .cmp(&right_parsed.0)
        .then_with(|| left_parsed.1.cmp(&right_parsed.1))
        .then_with(|| left.cmp(right))
}

fn parse_airway_name_for_ui(name: &str) -> (String, i32) {
    let trimmed = name.trim();
    let split_at = trimmed
        .find(|ch: char| ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let prefix = trimmed[..split_at].to_ascii_uppercase();
    let number = trimmed[split_at..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<i32>()
        .unwrap_or(i32::MAX);
    (prefix, number)
}

pub(crate) fn insert_airway_materialized(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: Option<usize>,
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    match end_component_index {
        Some(end_component_index) => insert_airway_between_waypoints(
            plan,
            start_component_index,
            end_component_index,
            airway,
            resolved_legs,
        ),
        None => insert_airway_after_waypoint(plan, start_component_index, airway, resolved_legs),
    }
}

pub(crate) fn insert_procedure_materialized(
    plan: &FlightPlan,
    _start_component_index: usize,
    end_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<FlightPlan> {
    match built.procedure.kind.clone() {
        ProcedureKind::Sid => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "departures must be inserted through the origin attachment path".to_string(),
        }),
        ProcedureKind::Star | ProcedureKind::Approach => insert_terminal_procedure_before_airport(
            plan,
            end_component_index,
            built.procedure,
            built.resolved_legs,
        ),
    }
}

pub(crate) fn insert_departure_materialized(
    plan: &FlightPlan,
    airport_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<FlightPlan> {
    insert_departure_after_airport(
        plan,
        airport_component_index,
        built.procedure,
        built.resolved_legs,
    )
}

pub(crate) fn append_plate_destination(
    plan: &FlightPlan,
    airport_id: &str,
) -> AppResult<FlightPlan> {
    let airport_id = airport_id.trim();
    let mut appended = if plan.route_components.is_empty() {
        let mut appended = plan.clone();
        appended.route_components.push(RouteComponent::Waypoint {
            waypoint: NavRef::Airport(airport_id.to_string()),
        });
        build_flight_plan(appended)?
    } else {
        planning::insert_waypoint(
            plan,
            plan.route_components.len() - 1,
            false,
            NavRef::Airport(airport_id.to_string()),
        )?
    };
    appended.destination = Some(AirportId(airport_id.to_string()));
    Ok(appended)
}

pub(crate) fn prepend_plate_origin(plan: &FlightPlan, airport_id: &str) -> AppResult<FlightPlan> {
    let airport_id = airport_id.trim();
    let prepended = if plan.route_components.is_empty() {
        let mut prepended = plan.clone();
        prepended.route_components.push(RouteComponent::Waypoint {
            waypoint: NavRef::Airport(airport_id.to_string()),
        });
        build_flight_plan(prepended)?
    } else {
        planning::insert_waypoint(plan, 0, true, NavRef::Airport(airport_id.to_string()))?
    };
    Ok(prepended)
}

pub(crate) fn replace_procedure_materialized(
    plan: &FlightPlan,
    component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<FlightPlan> {
    replace_procedure_component(plan, component_index, built.procedure, built.resolved_legs)
}

pub fn resolve_content_status(
    requirements: &[ContentRequirement],
    inventory: &ContentInventory,
    policy: ContentPolicy,
) -> AppResult<ContentReport> {
    let mut items = Vec::new();

    for requirement in requirements {
        for package_id in &requirement.package_ids {
            let installed = inventory
                .installed_packages
                .iter()
                .any(|pkg| &pkg.package_id == package_id && pkg.integrity_ok);

            let availability = match (installed, policy) {
                (true, ContentPolicy::StreamAllowed) => ContentAvailability::LocalAndRemote,
                (true, _) => ContentAvailability::LocalOnly,
                (false, ContentPolicy::StreamAllowed) => ContentAvailability::RemoteOnly,
                (false, _) => ContentAvailability::Unavailable,
            };

            items.push(ContentReportItem {
                label: package_id.package_name(),
                availability: AvailabilityDetail {
                    availability,
                    cycle_current: true,
                    integrity_ok: installed,
                    cached: installed,
                    offline_usable: installed,
                },
            });
        }
    }

    let fully_satisfied = items.iter().all(|item| match policy {
        ContentPolicy::StreamAllowed => !matches!(
            item.availability.availability,
            ContentAvailability::Unavailable
        ),
        _ => matches!(
            item.availability.availability,
            ContentAvailability::LocalOnly | ContentAvailability::LocalAndRemote
        ),
    });

    Ok(ContentReport {
        fully_satisfied,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn koma_ils_or_loc_candidate(procedure_id: &str) -> PlateProcedureLoadCandidateInput {
        PlateProcedureLoadCandidateInput {
            airport_id: "KOMA".to_string(),
            cifp_id: procedure_id.to_string(),
            match_rows: vec![CifpTppMatchRow {
                airport_id: "KOMA".to_string(),
                cifp_id: procedure_id.to_string(),
                procedure_kind: ProcedureKind::Approach,
                plate_id: "plate:KOMA:ILS OR LOC 32R".to_string(),
                plate_label: "ILS or LOC 32R".to_string(),
                package_id: "tpp".to_string(),
                public: 1,
                priority: 1,
                match_kind: "exact".to_string(),
                is_primary: 1,
            }],
            options: ProcedureOptions {
                airport_id: "KOMA".to_string(),
                procedure_id: procedure_id.to_string(),
                kind: ProcedureKind::Approach,
                runway_transitions: Vec::new(),
                enroute_transitions: vec!["OVR".to_string()],
                has_common_segment: true,
                valid_choices: vec![ProcedureSpecChoice {
                    runway_transition: None,
                    enroute_transition: Some("OVR".to_string()),
                }],
            },
        }
    }

    fn ksea_bangr_sid_candidate() -> PlateProcedureLoadCandidateInput {
        PlateProcedureLoadCandidateInput {
            airport_id: "KSEA".to_string(),
            cifp_id: "BANGR9".to_string(),
            match_rows: vec![CifpTppMatchRow {
                airport_id: "KSEA".to_string(),
                cifp_id: "BANGR9".to_string(),
                procedure_kind: ProcedureKind::Sid,
                plate_id: "plate:KSEA:DP-WA-BANGR NINE (RNAV).png".to_string(),
                plate_label: "BANGR NINE (RNAV)".to_string(),
                package_id: "tpp".to_string(),
                public: 1,
                priority: 1,
                match_kind: "terminal-name".to_string(),
                is_primary: 1,
            }],
            options: ProcedureOptions {
                airport_id: "KSEA".to_string(),
                procedure_id: "BANGR9".to_string(),
                kind: ProcedureKind::Sid,
                runway_transitions: vec!["RW16L".to_string()],
                enroute_transitions: vec!["BANGR".to_string()],
                has_common_segment: true,
                valid_choices: vec![ProcedureSpecChoice {
                    runway_transition: Some("RW16L".to_string()),
                    enroute_transition: Some("BANGR".to_string()),
                }],
            },
        }
    }

    fn koma_sayin_star_candidate() -> PlateProcedureLoadCandidateInput {
        PlateProcedureLoadCandidateInput {
            airport_id: "KOMA".to_string(),
            cifp_id: "SAYIN3".to_string(),
            match_rows: vec![CifpTppMatchRow {
                airport_id: "KOMA".to_string(),
                cifp_id: "SAYIN3".to_string(),
                procedure_kind: ProcedureKind::Star,
                plate_id: "plate:KOMA:STAR-NE-SAYIN THREE.png".to_string(),
                plate_label: "SAYIN THREE".to_string(),
                package_id: "tpp".to_string(),
                public: 1,
                priority: 1,
                match_kind: "terminal-name".to_string(),
                is_primary: 1,
            }],
            options: ProcedureOptions {
                airport_id: "KOMA".to_string(),
                procedure_id: "SAYIN3".to_string(),
                kind: ProcedureKind::Star,
                runway_transitions: vec!["RW14R".to_string()],
                enroute_transitions: vec!["SAYIN".to_string()],
                has_common_segment: true,
                valid_choices: vec![ProcedureSpecChoice {
                    runway_transition: Some("RW14R".to_string()),
                    enroute_transition: Some("SAYIN".to_string()),
                }],
            },
        }
    }

    #[test]
    fn sid_plate_loads_target_the_origin_and_use_departure_language() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            destination: Some(AirportId("KPAE".to_string())),
            ..FlightPlan::default()
        };

        let menu = describe_plate_procedure_load_menu(&plan, vec![ksea_bangr_sid_candidate()])
            .expect("describe SID plate loads");

        assert_eq!(menu.procedure_kind, Some(ProcedureKind::Sid));
        assert_eq!(menu.launcher_label, "LOAD\nDEP");
        assert_eq!(menu.header, "Load departure");
        assert_eq!(menu.header_tone, ProcedureLoadHeaderTone::Normal);
        assert_eq!(menu.options.len(), 1);
        assert_eq!(menu.options[0].label, "via RW16L to BANGR");
        let command: ProcedureLoadCommand =
            serde_json::from_str(&menu.options[0].load_id).expect("decode SID load command");
        assert_eq!(command.kind, ProcedureKind::Sid);
        assert!(matches!(
            command.target,
            ProcedureLoadPlanTarget::ExistingOrigin { .. }
        ));
    }

    #[test]
    fn plate_loads_deduplicate_ils_and_loc_by_entry_transition() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KOMA".to_string()),
                },
            ],
            destination: Some(AirportId("KOMA".to_string())),
            ..FlightPlan::default()
        };

        let menu = describe_plate_procedure_load_menu(
            &plan,
            vec![
                koma_ils_or_loc_candidate("L32R"),
                koma_ils_or_loc_candidate("I32R"),
            ],
        )
        .expect("describe plate loads");

        assert_eq!(menu.header, "Load approach");
        assert_eq!(menu.header_tone, ProcedureLoadHeaderTone::Normal);
        assert_eq!(menu.options.len(), 1);
        assert_eq!(menu.options[0].label, "from OVR");
        let command: ProcedureLoadCommand =
            serde_json::from_str(&menu.options[0].load_id).expect("decode load command");
        assert_eq!(command.procedure_id, "I32R");
        assert!(matches!(
            command.target,
            ProcedureLoadPlanTarget::ExistingDestination { .. }
        ));
    }

    #[test]
    fn plate_load_menu_marks_replacing_the_destination_approach_as_destructive() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KOMA".to_string()),
                        procedure_id: "V32R".to_string(),
                        display_label: Some("VOR 32R".to_string()),
                        kind: ProcedureKind::Approach,
                        runway_transition: None,
                        enroute_transition: None,
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KOMA".to_string()),
                },
            ],
            destination: Some(AirportId("KOMA".to_string())),
            ..FlightPlan::default()
        };

        let menu =
            describe_plate_procedure_load_menu(&plan, vec![koma_ils_or_loc_candidate("I32R")])
                .expect("describe replacement menu");

        assert_eq!(menu.header, "Replace current approach");
        assert_eq!(menu.header_tone, ProcedureLoadHeaderTone::Destructive);
        assert_eq!(menu.options.len(), 1);
    }

    #[test]
    fn star_plate_marks_replacing_arrival_before_attached_approach_as_destructive() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KOMA".to_string()),
                        procedure_id: "OMAHA3".to_string(),
                        display_label: Some("OMAHA THREE".to_string()),
                        kind: ProcedureKind::Star,
                        runway_transition: Some("RW14R".to_string()),
                        enroute_transition: Some("OMAHA".to_string()),
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KOMA".to_string()),
                        procedure_id: "I14R".to_string(),
                        display_label: Some("ILS or LOC 14R".to_string()),
                        kind: ProcedureKind::Approach,
                        runway_transition: None,
                        enroute_transition: None,
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KOMA".to_string()),
                },
            ],
            destination: Some(AirportId("KOMA".to_string())),
            ..FlightPlan::default()
        };

        let menu = describe_plate_procedure_load_menu(&plan, vec![koma_sayin_star_candidate()])
            .expect("describe STAR replacement menu");

        assert_eq!(menu.procedure_kind, Some(ProcedureKind::Star));
        assert_eq!(menu.launcher_label, "LOAD\nARR");
        assert_eq!(menu.header, "Replace current arrival");
        assert_eq!(menu.header_tone, ProcedureLoadHeaderTone::Destructive);
        assert_eq!(menu.options[0].label, "from SAYIN to RW14R");
    }

    #[test]
    fn plate_load_menu_can_append_a_new_destination() {
        let plan = FlightPlan {
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KSEA".to_string()),
            }],
            destination: Some(AirportId("KSEA".to_string())),
            ..FlightPlan::default()
        };

        let menu =
            describe_plate_procedure_load_menu(&plan, vec![koma_ils_or_loc_candidate("I32R")])
                .expect("describe append menu");

        assert_eq!(menu.header, "Append KOMA to plan and load approach");
        assert_eq!(menu.header_tone, ProcedureLoadHeaderTone::Normal);
        assert_eq!(menu.options.len(), 1);
        let command: ProcedureLoadCommand =
            serde_json::from_str(&menu.options[0].load_id).expect("decode append command");
        assert_eq!(command.target, ProcedureLoadPlanTarget::AppendDestination);

        let appended =
            append_plate_destination(&plan, &command.airport_id).expect("append plate destination");
        assert_eq!(appended.destination, Some(AirportId("KOMA".to_string())));
        assert!(matches!(
            appended.route_components.last(),
            Some(RouteComponent::Waypoint { waypoint: NavRef::Airport(id) }) if id == "KOMA"
        ));
    }

    #[test]
    fn duplicate_approach_labels_use_pilot_facing_procedure_names() {
        let mut procedures = vec![
            ProcedureSummary {
                airport_id: "KAMA".to_string(),
                procedure_id: "I04".to_string(),
                display_label: "ILS or LOC 04".to_string(),
                kind: ProcedureKind::Approach,
            },
            ProcedureSummary {
                airport_id: "KAMA".to_string(),
                procedure_id: "L04".to_string(),
                display_label: "ILS or LOC 04".to_string(),
                kind: ProcedureKind::Approach,
            },
            ProcedureSummary {
                airport_id: "KBJC".to_string(),
                procedure_id: "D30L".to_string(),
                display_label: "VOR and DME 30L and R".to_string(),
                kind: ProcedureKind::Approach,
            },
            ProcedureSummary {
                airport_id: "KBJC".to_string(),
                procedure_id: "D30R".to_string(),
                display_label: "VOR and DME 30L and R".to_string(),
                kind: ProcedureKind::Approach,
            },
        ];

        disambiguate_duplicate_procedure_display_labels(&mut procedures);

        assert_eq!(
            procedures
                .iter()
                .map(|procedure| procedure.display_label.as_str())
                .collect::<Vec<_>>(),
            vec!["ILS 04", "LOC 04", "VOR and DME 30L", "VOR and DME 30R"]
        );
    }

    #[test]
    fn route_projection_uses_procedure_display_path_without_navref_lookup() {
        let start = LatLon {
            lat: 48.0,
            lon: -109.0,
        };
        let end = LatLon {
            lat: 48.1,
            lon: -109.2,
        };
        let plan = FlightPlan {
            resolved_legs: vec![ResolvedLeg {
                id: "procedure-leg".to_string(),
                from: NavRef::Fix("ISITE".to_string()),
                to: NavRef::Fix("JEPAL".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: Some(ProcedureLegProvenance {
                    airport_id: "KHVR".to_string(),
                    procedure_id: "R26".to_string(),
                    kind: ProcedureKind::Approach,
                    role: ProcedureSegmentRole::EnrouteTransition,
                    path_termination: PathTermination::TrackToFix,
                    leg_sequence: 20,
                    discontinuity_after: None,
                    display_path: Some(LegDisplayPath {
                        style: LegDisplayPathStyle::Solid,
                        elements: vec![LegDisplayElement::Segment { start, end }],
                        effective_terminal_course_deg: None,
                        debug_element_sources: Vec::new(),
                        debug_element_roles: Vec::new(),
                    }),
                }),
            }],
            ..FlightPlan::default()
        };
        let mut resolver_calls = 0;

        let route = project_flight_plan_route_with_resolver(&plan, |_, _| {
            resolver_calls += 1;
            Err("procedure display path should not need navref resolution")
        })
        .expect("project route from procedure display path");

        assert_eq!(resolver_calls, 0);
        assert_eq!(route.len(), 1);
        assert_eq!(route[0].from, start);
        assert_eq!(route[0].to, end);

        let annotations = project_flight_plan_route_distance_annotations(&plan, &route)
            .expect("project route distance annotations");
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].id, "procedure-leg");
        assert_eq!(annotations[0].segment_indexes, vec![0]);
        assert_eq!(
            annotations[0].required_feature_ids,
            vec![
                flight_plan_waypoint_feature_id(&NavRef::Fix("ISITE".to_string())).unwrap(),
                flight_plan_waypoint_feature_id(&NavRef::Fix("JEPAL".to_string())).unwrap(),
            ]
        );
        assert!(annotations[0].text.ends_with("nm"));
        assert_eq!(annotations[0].minimum_path_to_pill_width_ratio, 1.6);
    }

    #[test]
    fn route_distance_annotation_aggregates_procedure_geometry_and_omits_holds() {
        let start = LatLon {
            lat: 47.0,
            lon: -122.0,
        };
        let middle = LatLon {
            lat: 47.1,
            lon: -122.0,
        };
        let end = LatLon {
            lat: 47.1,
            lon: -121.9,
        };
        let mut plan = FlightPlan {
            resolved_legs: vec![ResolvedLeg {
                id: "procedure-leg".to_string(),
                from: NavRef::Fix("START".to_string()),
                to: NavRef::Fix("END".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: Some(ProcedureLegProvenance {
                    airport_id: "KSEA".to_string(),
                    procedure_id: "TEST".to_string(),
                    kind: ProcedureKind::Approach,
                    role: ProcedureSegmentRole::Common,
                    path_termination: PathTermination::Other("RF".to_string()),
                    leg_sequence: 10,
                    discontinuity_after: None,
                    display_path: Some(LegDisplayPath {
                        style: LegDisplayPathStyle::Solid,
                        elements: vec![
                            LegDisplayElement::Segment { start, end: middle },
                            LegDisplayElement::Segment { start: middle, end },
                        ],
                        effective_terminal_course_deg: None,
                        debug_element_sources: Vec::new(),
                        debug_element_roles: Vec::new(),
                    }),
                }),
            }],
            ..FlightPlan::default()
        };
        let route = project_flight_plan_route_with_resolver(&plan, |_, _| {
            Err::<LatLon, _>("display geometry must be self-contained")
        })
        .expect("project route");

        let annotations = project_flight_plan_route_distance_annotations(&plan, &route)
            .expect("project annotations");
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].segment_indexes, vec![0, 1]);
        assert!(
            (annotations[0].distance_nm - route.iter().map(|item| item.distance_nm).sum::<f64>())
                .abs()
                < 1e-9
        );

        plan.resolved_legs[0]
            .procedure_provenance
            .as_mut()
            .unwrap()
            .path_termination = PathTermination::Other("HF".to_string());
        let hold_route = project_flight_plan_route_with_resolver(&plan, |_, _| {
            Err::<LatLon, _>("display geometry must be self-contained")
        })
        .expect("project hold route");
        assert!(
            project_flight_plan_route_distance_annotations(&plan, &hold_route)
                .expect("project hold annotations")
                .is_empty()
        );

        plan.resolved_legs[0]
            .procedure_provenance
            .as_mut()
            .unwrap()
            .path_termination = PathTermination::HeadingToManual;
        let vector_route = project_flight_plan_route_with_resolver(&plan, |_, _| {
            Err::<LatLon, _>("display geometry must be self-contained")
        })
        .expect("project vector route");
        assert!(
            project_flight_plan_route_distance_annotations(&plan, &vector_route)
                .expect("project vector annotations")
                .is_empty()
        );

        let provenance = plan.resolved_legs[0]
            .procedure_provenance
            .as_mut()
            .expect("procedure provenance");
        provenance.path_termination = PathTermination::Other("VM".to_string());
        provenance.discontinuity_after = Some(ProcedureDiscontinuity::Vectors);
        let vm_route = project_flight_plan_route_with_resolver(&plan, |_, _| {
            Err::<LatLon, _>("display geometry must be self-contained")
        })
        .expect("project VM vectors route");
        assert!(
            project_flight_plan_route_distance_annotations(&plan, &vm_route)
                .expect("project VM vector annotations")
                .is_empty(),
            "finite vectors display geometry must not receive a distance pill"
        );
    }

    #[test]
    fn route_distance_annotations_keep_airway_hops_distinct() {
        let positions = [
            LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            LatLon {
                lat: 47.1,
                lon: -121.9,
            },
            LatLon {
                lat: 47.2,
                lon: -121.8,
            },
        ];
        let plan = FlightPlan {
            resolved_legs: vec![
                ResolvedLeg {
                    id: "airway-hop-a-b".to_string(),
                    from: NavRef::Fix("A".to_string()),
                    to: NavRef::Fix("B".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-hop-b-c".to_string(),
                    from: NavRef::Fix("B".to_string()),
                    to: NavRef::Fix("C".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
            ],
            ..FlightPlan::default()
        };
        let route = project_flight_plan_route_with_resolver(&plan, |nav_ref, _| match nav_ref {
            NavRef::Fix(ident) => match ident.as_str() {
                "A" => Ok(positions[0]),
                "B" => Ok(positions[1]),
                "C" => Ok(positions[2]),
                _ => Err("unexpected fix"),
            },
            _ => Err("unexpected nav ref"),
        })
        .expect("project airway route");

        let annotations = project_flight_plan_route_distance_annotations(&plan, &route)
            .expect("project airway annotations");
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].segment_indexes, vec![0]);
        assert_eq!(annotations[1].segment_indexes, vec![1]);
        assert_eq!(annotations[0].required_feature_ids.len(), 2);
        assert_eq!(annotations[1].required_feature_ids.len(), 2);
    }

    #[test]
    fn direct_to_distance_annotation_requires_only_its_labeled_target() {
        let start_position = LatLon {
            lat: 47.0,
            lon: -122.0,
        };
        let target_position = LatLon {
            lat: 47.2,
            lon: -121.8,
        };
        let target = NavRef::Fix("TARGET".to_string());
        let plan = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: None,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::DirectTo,
                direct_to: Some(DirectToState {
                    start: NavRef::Spot(start_position),
                    target: target.clone(),
                    target_row: DirectToTargetRow::Temporary {
                        row_id: FlightPlanRowId("direct-target".to_string()),
                    },
                    resume_row_id: None,
                }),
                suspend_reason: None,
            }),
            ..FlightPlan::default()
        };
        let route = project_flight_plan_route_with_resolver(&plan, |nav_ref, _| match nav_ref {
            NavRef::Spot(position) => Ok(*position),
            NavRef::Fix(ident) if ident == "TARGET" => Ok(target_position),
            _ => Err("unexpected nav ref"),
        })
        .expect("project direct-to route");

        let annotations = project_flight_plan_route_distance_annotations(&plan, &route)
            .expect("project direct-to annotation");
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].id, "direct-to");
        assert_eq!(annotations[0].segment_indexes, vec![0]);
        assert_eq!(
            annotations[0].required_feature_ids,
            vec![flight_plan_waypoint_feature_id(&target).unwrap()]
        );
    }

    #[test]
    fn route_projection_marks_only_the_manual_heading_before_vectors() {
        fn fail_resolver(_: &NavRef, _: Option<&str>) -> Result<LatLon, &'static str> {
            Err("display path should not require navref resolution")
        }

        let points = [
            LatLon {
                lat: 47.49,
                lon: -122.22,
            },
            LatLon {
                lat: 47.48,
                lon: -122.21,
            },
            LatLon {
                lat: 47.47,
                lon: -122.22,
            },
            LatLon {
                lat: 47.46,
                lon: -122.24,
            },
        ];
        let leg = ResolvedLeg {
            id: "rentn3-rw16-vectors".to_string(),
            from: NavRef::LatLon(points[0]),
            to: NavRef::LatLon(points[3]),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KRNT".to_string(),
                procedure_id: "RENTN3".to_string(),
                kind: ProcedureKind::Sid,
                role: ProcedureSegmentRole::RunwayTransition,
                path_termination: PathTermination::HeadingToAltitude,
                leg_sequence: 20,
                discontinuity_after: Some(ProcedureDiscontinuity::Vectors),
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: points
                        .windows(2)
                        .map(|pair| LegDisplayElement::Segment {
                            start: pair[0],
                            end: pair[1],
                        })
                        .collect(),
                    effective_terminal_course_deg: Some(130.0),
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            resolved_legs: vec![leg],
            ..FlightPlan::default()
        };
        let route = project_flight_plan_route_with_resolver(&plan, fail_resolver)
            .expect("project vectors route");
        let styles = route
            .iter()
            .map(|segment| serde_json::to_value(&segment.style).expect("serialize route style"))
            .collect::<Vec<_>>();

        assert_eq!(styles, vec!["solid", "solid", "vectors"]);
        assert!(
            route[2].finish_lines.is_empty(),
            "display-only vectors geometry must not expose a sequencing finish line"
        );
    }

    #[test]
    fn direct_to_route_projection_finish_line_uses_resume_leg_geometry() {
        let route_start = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let next = LatLon {
            lat: 40.0,
            lon: -119.95,
        };
        let final_waypoint = LatLon {
            lat: 40.05,
            lon: -119.95,
        };
        let direct_start = LatLon {
            lat: 39.95,
            lon: -120.0,
        };
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(route_start),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(next),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::LatLon(final_waypoint),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "route-start-to-next".to_string(),
                    from: NavRef::LatLon(route_start),
                    to: NavRef::LatLon(next),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "next-to-final".to_string(),
                    from: NavRef::LatLon(next),
                    to: NavRef::LatLon(final_waypoint),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            ..FlightPlan::default()
        };
        let plan = activate_direct_to(&plan, direct_start, NavRef::LatLon(route_start))
            .expect("activate direct-to route start");

        let route = project_flight_plan_route_with_resolver(&plan, |nav_ref, _| match nav_ref {
            NavRef::LatLon(position) => Ok(*position),
            _ => Err("unexpected nav ref"),
        })
        .expect("project route");
        let direct_to = route
            .iter()
            .find(|segment| segment.id == "direct-to")
            .expect("direct-to route segment");
        let finish_line = direct_to
            .finish_lines
            .first()
            .expect("direct-to finish line");
        let finish_line_course = route_bearing_from(finish_line.start, finish_line.end);

        assert!(
            (finish_line_course - 135.0).abs() < 1.0,
            "direct-to finish line should bisect the turn into the resumed route leg; got {finish_line_course:.1}"
        );
    }
}
