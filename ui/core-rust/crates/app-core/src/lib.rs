// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

pub mod airport_info;
pub mod chart_page;
pub mod cloud;
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

pub use airport_info::{
    AirportCommunicationUiView, AirportInfoUiView, AirportRunwayUiView, AirportSolarEventUiView,
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
    CloudEngine, CloudHttpHeader, CloudHttpMethod, CloudHttpRequest, CloudHttpResponse,
    CloudPersistentState, CloudPlatformEffect, CloudProviderKind, CloudProviderPrincipal,
    CloudStatusFact, CloudStatusSummary, CloudUiActionId, CloudUiFieldId, CloudUiFieldValue,
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
    FlightDataBannerInput, FlightDataBannerModel, FlightDataCell, FlightDataCellTone,
    FlightDataColumn, FlightDataComputer,
};
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
    OwnshipSourceStatus, OwnshipSourceStatusUpdate, OwnshipState, OwnshipUiState,
    ResolvedOwnshipState, SituationControlInput, SituationControlMenuItem, SituationKinematics,
    SituationRingCandidate, SituationSample, SourceConnectionState,
};
pub use package_management::{
    current_artifacts_manifest_is_supported, decode_current_artifacts_manifest_list,
    default_offline_package_preferences, initialize_offline_packages,
    plan_current_artifacts_discovery, plan_offline_packages, reduce_offline_packages,
    reduce_offline_packages_controller, select_supported_current_artifacts_manifests,
    BundleManifest, BundlePackageArtifact, BundlePackageMetadata, CurrentArtifactsBundleRef,
    CurrentArtifactsBundleRequest, CurrentArtifactsDiscoveryPlan, CurrentArtifactsManifest,
    InstalledArtifact, OfflinePackagePreferences, OfflinePackageSelection,
    OfflinePackagesControllerCommand, OfflinePackagesControllerEvent,
    OfflinePackagesControllerInput, OfflinePackagesControllerResult,
    OfflinePackagesControllerState, OfflinePackagesControllerUiState, OfflinePackagesEvent,
    OfflinePackagesInitInput, OfflinePackagesLibraryCache, OfflinePackagesReduceInput,
    OfflinePackagesReduceResult, OfflinePackagesState, OfflinePackagesStorageInfo,
    OfflinePackagesSyncProgress, OfflinePackagesSyncSummary, OfflinePackagesUiRow,
    OfflinePackagesUiState, OfflinePackagesWarning, PackageManagementInput, PackageManagementPlan,
};
pub use planning::{
    activate_direct_to, activate_direct_to_row, activate_leg, activate_leg_at_detail_index,
    activate_next_leg, active_guidance_leg, at_fix_requirement, basic_terminal_state,
    change_airway_entry, change_airway_exit, change_procedure_enroute_transition,
    change_procedure_runway_transition, common_resume_candidate_decision, delete_component,
    delete_waypoint_component, direct_to_fix_with_course_continuation_requirement,
    enter_hold_requirement, established_on_course_requirement, first_guidance_detail_index_for_leg,
    flatten_component_to_waypoints, flight_plan_contains_nav_ref,
    flight_plan_has_direct_to_overlay, insert_airport_waypoint, insert_airway_after_airway,
    insert_airway_after_waypoint, insert_airway_between_waypoints,
    insert_initial_procedure_before_airport, insert_procedure_between_waypoints, insert_waypoint,
    intercept_course_requirement, materialize_airway_exit_before_component, move_component,
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
    attach_nav_kv_store_to_session_with_open_result, complete_cloud_authorization_in_session,
    complete_cloud_provider_request_in_session, configure_live_feed_source_in_session,
    configure_platform_capabilities_in_session, create_ui_session, create_ui_session_at_epoch_ms,
    create_ui_session_profiled, create_ui_session_profiled_at_epoch_ms,
    debug_drop_nav_kv_pages_for_attached_sessions, destroy_session,
    disengage_map_follow_in_session, drain_session_resource_effects, engage_map_follow_in_session,
    get_map_overlay_in_session, get_map_overlay_in_session_at_epoch_ms,
    get_map_overlay_in_session_with_point_display_scale,
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
    perform_settings_action_in_session, perform_status_action_in_session, play_playback_in_session,
    prepare_nexrad_tile_in_session, project_flight_plan_route_in_session,
    push_situation_sample_in_session, query_flight_plan_in_session,
    record_offline_package_preferences_in_session, refresh_live_feed_current_in_session,
    register_ownship_source_in_session, render_terrain_overlay_tile_by_key_in_session,
    render_terrain_overlay_tile_in_session, render_terrain_overlay_tiles_in_session,
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
    update_ownship_source_status_in_session, ClientBuildInfo, DisplayDimTimeout,
    FlightPlanSessionCommand, FlightPlanSessionQuery, GuidanceLegGeometry,
    LiveFeedAcquisitionPolicy, NavDbAdvanceDisposition, NavDbAdvanceResult, NavDbMaintenanceAction,
    NavDbMaintenanceResult, PlatformCapabilities, PlatformCloudCapability,
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
pub struct ProcedureLoadTarget {
    pub row_uid: String,
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub replace_component_index: Option<usize>,
    pub start_component_index: usize,
    pub end_component_index: usize,
    pub preferred_choice: Option<ProcedureSpecChoice>,
    pub valid_choices: Vec<ProcedureSpecChoice>,
}

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
pub struct ProcedureLoadCommand {
    pub row_uid: String,
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
pub struct FlightPlanRouteProjection {
    pub flight_plan_route_revision: u64,
    pub segments: Vec<FlightPlanRouteSegment>,
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
        route[index].finish_lines =
            sequencing_finish_lines(&route_geometries[index], route_geometries.get(index + 1));
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
        for (element_index, element) in display_path.elements.iter().enumerate() {
            let geometry = guidance_route_geometry_from_display_element(element);
            let (from, to) = guidance_route_endpoints(&geometry);
            route.push(FlightPlanRouteSegment {
                id: guidance_detail_id_for_leg_element(leg_index, leg, element_index),
                leg_id: leg.id.clone(),
                from,
                to,
                path: guidance_route_path_from_geometry(&geometry),
                style: display_path.style.clone(),
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
    for row in rows
        .into_iter()
        .filter(|row| row.airport_id.trim() == airport_id.trim())
    {
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

pub fn describe_plate_procedure_load_options(
    plan: &FlightPlan,
    candidates: Vec<PlateProcedureLoadCandidateInput>,
) -> AppResult<Vec<ProcedureLoadOption>> {
    let mut loads = Vec::new();
    for candidate in candidates {
        let Some(preferred) = select_preferred_cifp_tpp_match(candidate.match_rows) else {
            continue;
        };
        let Some(target) = describe_load_procedure_from_plate(
            plan,
            &preferred.airport_id,
            &preferred.cifp_id,
            ProcedureKind::Approach,
            candidate.options,
        )?
        else {
            continue;
        };
        let choices = target
            .preferred_choice
            .clone()
            .map(|choice| vec![choice])
            .unwrap_or_else(|| target.valid_choices.clone());
        let include_procedure_id = choices.len() > 1 || target.valid_choices.len() > 1;
        for choice in choices {
            let display_label = preferred.plate_label.trim().to_string();
            let label =
                format_procedure_load_option_label(&display_label, &choice, include_procedure_id);
            let command = ProcedureLoadCommand {
                row_uid: target.row_uid.clone(),
                airport_id: target.airport_id.clone(),
                procedure_id: target.procedure_id.clone(),
                display_label: Some(display_label),
                kind: target.kind.clone(),
                runway_transition: choice.runway_transition,
                enroute_transition: choice.enroute_transition,
            };
            loads.push(ProcedureLoadOption {
                load_id: serde_json::to_string(&command).map_err(|err| AppError {
                    kind: AppErrorKind::Internal,
                    message: err.to_string(),
                })?,
                label,
            });
        }
    }
    Ok(loads)
}

pub fn describe_load_procedure_from_plate(
    plan: &FlightPlan,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    options: ProcedureOptions,
) -> AppResult<Option<ProcedureLoadTarget>> {
    let plan = plan.clone().normalized();
    let Some(terminal_airport_index) = plan.route_components.iter().enumerate().rev().find_map(
        |(index, component)| match component {
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport(code),
            } if code.trim() == airport_id.trim() => Some(index),
            _ => None,
        },
    ) else {
        return Ok(None);
    };

    if terminal_airport_index == 0 {
        return Ok(None);
    }
    let terminal_airport_row_uid = project_ui_state(&plan)
        .display_rows
        .into_iter()
        .find(|row| {
            row.depth == 0
                && row.component_index == Some(terminal_airport_index)
                && row.nav_ref
                    .as_ref()
                    .is_some_and(|nav_ref| matches!(nav_ref, NavRef::Airport(code) if code.trim() == airport_id.trim()))
        })
        .map(|row| row.uid)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("procedure load target row missing for airport {airport_id}"),
        })?;

    let replace_component_index = match plan.route_components.get(terminal_airport_index - 1) {
        Some(RouteComponent::Procedure { procedure })
            if procedure.kind == ProcedureKind::Approach
                && procedure.airport_id.0.trim() == airport_id.trim() =>
        {
            Some(terminal_airport_index - 1)
        }
        _ => None,
    };

    let preferred_choice = choose_obvious_procedure_choice(&plan, terminal_airport_index, &options);

    Ok(Some(ProcedureLoadTarget {
        row_uid: terminal_airport_row_uid,
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind,
        replace_component_index,
        start_component_index: terminal_airport_index - 1,
        end_component_index: terminal_airport_index,
        preferred_choice,
        valid_choices: options.valid_choices,
    }))
}

fn format_procedure_load_option_label(
    procedure_id: &str,
    choice: &ProcedureSpecChoice,
    include_procedure_id: bool,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if include_procedure_id {
        parts.push(procedure_id.trim().to_string());
    } else {
        parts.push("Load Procedure".to_string());
    }
    if let Some(enroute_transition) = choice.enroute_transition.as_deref() {
        if !enroute_transition.trim().is_empty() {
            parts.push(enroute_transition.trim().to_string());
        }
    }
    if let Some(runway_transition) = choice.runway_transition.as_deref() {
        if !runway_transition.trim().is_empty() {
            parts.push(runway_transition.trim().to_string());
        }
    }
    parts.join(" ")
}

fn choose_obvious_procedure_choice(
    plan: &FlightPlan,
    terminal_airport_index: usize,
    options: &ProcedureOptions,
) -> Option<ProcedureSpecChoice> {
    if options.valid_choices.len() == 1 {
        return options.valid_choices.first().cloned();
    }

    let expected_enroute = plan
        .route_components
        .get(terminal_airport_index.checked_sub(1)?)
        .and_then(|component| match component {
            RouteComponent::Procedure { .. } => plan
                .route_components
                .get(terminal_airport_index.checked_sub(2)?),
            _ => Some(component),
        })
        .and_then(component_terminal_nav_ref)
        .and_then(nav_ref_identifier);

    if let Some(expected_enroute) = expected_enroute {
        let matching = options
            .valid_choices
            .iter()
            .filter(|choice| choice.enroute_transition.as_deref() == Some(expected_enroute))
            .cloned()
            .collect::<Vec<_>>();
        if matching.len() == 1 {
            return matching.into_iter().next();
        }
    }

    options
        .valid_choices
        .iter()
        .filter(|choice| choice.enroute_transition.is_none() && choice.runway_transition.is_none())
        .cloned()
        .next()
}

fn component_terminal_nav_ref(component: &RouteComponent) -> Option<&NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint),
        RouteComponent::Airway { airway } => Some(&airway.exit),
        RouteComponent::Procedure { .. } => None,
    }
}

fn nav_ref_identifier(nav_ref: &NavRef) -> Option<&str> {
    match nav_ref {
        NavRef::Airport(code) | NavRef::Navaid(code) | NavRef::Fix(code) => Some(code.as_str()),
        NavRef::ArincNavaid { identifier, .. } => Some(identifier.as_str()),
        NavRef::TerminalNavaid { identifier, .. } => Some(identifier.as_str()),
        NavRef::LatLon(_) | NavRef::Spot(_) => None,
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
    start_component_index: usize,
    end_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<FlightPlan> {
    insert_procedure_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        built.procedure,
        built.resolved_legs,
    )
}

pub(crate) fn insert_initial_procedure_materialized(
    plan: &FlightPlan,
    airport_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<FlightPlan> {
    insert_initial_procedure_before_airport(
        plan,
        airport_component_index,
        built.procedure,
        built.resolved_legs,
    )
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
