// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  AppUiState,
  AirwayPresentationPlan,
  AirwaySuggestion,
  CifpTppMatch,
  ChartPageData,
  FlightPlanEntryPreview,
  FlightPlanRouteProjection,
  ChartFamilyId,
  FlightDataCell,
  LatLon,
  MapFollowUiState,
  NavRef,
  NavSymbolFeature,
  OwnshipSelectionCommand,
  OwnshipSourceRegistration,
  OwnshipSourceStatusUpdate,
  PlaybackUiState,
  ProcedureLoadOption,
  ProcedureKind,
  ProcedureOptions,
  ProcedureSummary,
  Situation,
  SituationControlInput,
  SituationRingCandidate,
  SituationSample,
  WaypointIdentifierSuggestion,
  WeatherDetailUiView,
} from "./types";
import type { NexradOverlayQueryResult } from "../generated/nexradOverlayWire";
import type {
  CloudAuthorizationRequest,
  CloudAuthorizationResponse,
  CloudHttpRequest,
  CloudHttpResponse,
  CloudUiActionId,
  CloudUiFieldValue,
  UiCloudPageState,
} from "../generated/cloudWire";
import type { UiHomePageState } from "../generated/homePageWire";
export type {
  CloudAuthorizationMode,
  CloudAuthorizationRequest,
  CloudAuthorizationResponse,
  CloudHttpHeader,
  CloudHttpMethod,
  CloudHttpRequest,
  CloudHttpResponse,
  CloudPlatformEffect,
  CloudProviderKind,
  CloudUiActionId,
  CloudUiFieldId,
  CloudUiFieldValue,
  UiCloudAction,
  UiCloudPageState,
  UiCloudPanel,
  UiCloudPanelControl,
  UiCloudPanelState,
  UiQrCode,
} from "../generated/cloudWire";
export type { UiHomeDestination, UiHomePageButton, UiHomePageState } from "../generated/homePageWire";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import { advanceSharedNavKvStore, attachNavKvStoreToSession, resolveChartAssetUrl, runCoreHadOperation, runCoreHadSessionOperation, type UiInvalidation, type UiInvalidationListener } from "./navKv";
import { debugLog, debugTiming, installRustDebugLogBridge, perfDebugLog } from "./debugLog";
import { ingestPreparedLiveFeedResource, resetLiveFeedPrep } from "./liveFeedPrep";
import { liveFeedSourceUrl } from "./liveFeedUrls";
export { resolveLiveFeedResourceUrl, resolveLiveFeedSourceUrl } from "./liveFeedUrls";

declare const __AEROBAG_CLIENT_BUILD_INFO__: ClientBuildInfo;

type LiveFeedE2eProbeState = {
  open_attempts: number;
  active_event_sources: number;
  reconnect_scheduled: number;
  online_events: number;
  errors: number;
  messages: number;
  last_events_url: string | null;
  last_ready_state: number | null;
  last_reconnect_delay_ms: number | null;
};

function liveFeedE2eProbeState(): LiveFeedE2eProbeState {
  const global = (typeof window !== "undefined" ? window : globalThis) as typeof globalThis & {
    __aerobagLiveFeedE2eState?: LiveFeedE2eProbeState;
  };
  global.__aerobagLiveFeedE2eState ??= {
    open_attempts: 0,
    active_event_sources: 0,
    reconnect_scheduled: 0,
    online_events: 0,
    errors: 0,
    messages: 0,
    last_events_url: null,
    last_ready_state: null,
    last_reconnect_delay_ms: null,
  };
  return global.__aerobagLiveFeedE2eState;
}

export type {
  NexradOverlayQueryResult,
  NexradOverlayStats,
  NexradOverlayStatus,
  NexradOverlayStatusState,
  NexradOverlayTile,
  NexradOverlayTileCorners,
} from "../generated/nexradOverlayWire";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  reference_families: ChartPageData["airports"];
  airport_menu_entries: Array<
    | { kind: "separator"; label: string }
    | { kind: "airport"; airport: ChartPageData["airports"][number] }
    | { kind: "reference"; reference: ChartPageData["airports"][number] }
    | { kind: "external_link"; label: string; url: string }
  >;
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_reference_family_id?: string | null;
  selected_chart_id: string;
  suggested_chart_ids: string[];
};

export type RasterMapUiState = {
  selected_map_id: string;
  selected_map_label: string;
  selected_family_id: ChartFamilyId;
  selected_family_label: string;
  selected_family_launcher_label: string;
  min_zoom: number;
  max_zoom: number;
  initial_viewport: {
    lat: number;
    lon: number;
    zoom: number;
  };
  family_options: Array<{
    id: ChartFamilyId;
    label: string;
    launcher_label: string;
    enabled: boolean;
    disabled_reason?: string | null;
    active: boolean;
    has_references: boolean;
  }>;
};

export type UiSessionSnapshot = {
  session_revision: number;
  flight_plan_route_revision: number;
  nav_data_epoch: number;
  active_nav_db: {
    package_id: string;
    filename: string;
    contract_id: string | null;
    cycle: string | null;
    cycle_version: string | null;
  } | null;
  next_nav_db_maintenance_epoch_ms: number | null;
  app_ui_state: AppUiState;
  playback_ui_state: PlaybackUiState;
  playback_panel_state: UiPlaybackPanelState;
  map_follow_ui_state: MapFollowUiState;
  map_follow_target_viewport: {
    center: LatLon;
    zoom: number;
    rotation_deg: number;
    pitch_deg: number;
  } | null;
  chart_page_state: UiChartPageState;
  map_layer_state: UiMapLayerState;
  data_status_state: UiDataStatusState;
  data_status_page_state: UiDataStatusPageState;
  settings_page_state: UiSettingsPageState;
  cloud_page_state: UiCloudPageState;
  offline_package_preferences_json: string;
  home_page_state: UiHomePageState;
  display_policy: UiDisplayPolicy | null;
  disclaimer_state: UiDisclaimerState;
  debug_state: UiDebugState;
  raster_map?: RasterMapUiState | null;
  next_cycle_product_freshness_check_epoch_ms?: number | null;
};

export type UiDisclaimerState = {
  agreement_id: string;
  required: boolean;
  html: string;
  text: string;
  accept_label: string;
};

export type DebugFlagId = "tile_labels" | "nexrad_tile_labels" | "fast_tiles" | "offline_simulated_clock_buttons" | "sequencing_finish_lines" | "plate_flight_plan" | "bad_autopilot" | "gps_capture" | "debug_log_to_developer_server";

export type UiDebugState = {
  tile_labels: boolean;
  nexrad_tile_labels: boolean;
  fast_tiles: boolean;
  offline_simulated_clock_buttons: boolean;
  sequencing_finish_lines: boolean;
  plate_flight_plan: boolean;
  bad_autopilot: boolean;
  gps_capture: boolean;
  debug_log_to_developer_server: boolean;
};

export type UiPlaybackPanelState = {
  visible: boolean;
};

export type UiStatusSeverity = "ok" | "info" | "caution" | "warning" | "unavailable";

export type UiStatusActionStyle = "normal" | "hush";

export type UiStatusAction = {
  id: string;
  label: string;
  enabled: boolean;
  style: UiStatusActionStyle;
};

export type UiDataStatusBox = {
  id: string;
  label: string;
  value: string | null;
  severity: UiStatusSeverity;
  drives_caution: boolean;
  detail: string;
  actions: UiStatusAction[];
  hushed: boolean;
};

export type UiDataStatusState = {
  boxes: UiDataStatusBox[];
  launcher_count: string | null;
  launcher_severity: UiStatusSeverity;
};

export type UiDataStatusPageFact = {
  label: string;
  value: string;
  link_url?: string | null;
  time_utc?: string | null;
  time_display?: "ago" | "old" | "until" | null;
};

export type UiDataStatusPageRow = {
  id: string;
  label: string;
  value: string;
  severity: UiStatusSeverity;
  detail: string;
  facts: UiDataStatusPageFact[];
};

export type UiDataStatusPageState = {
  title: string;
  summary: string;
  rows: UiDataStatusPageRow[];
};

export type UiSettingsSliderStop = {
  id: string;
  label: string;
};

export type UiSettingsGridItem = {
  cell: FlightDataCell;
  enabled: boolean;
};

export type UiSettingsPageRow = {
  kind: string;
  id: string;
  title: string;
  value_id: string;
  stops: UiSettingsSliderStop[];
  items: UiSettingsGridItem[];
  action_id: string;
};

export type UiSettingsPageState = {
  title: string;
  summary: string;
  rows: UiSettingsPageRow[];
};

export type UiDisplayPolicy = {
  keep_screen_on: boolean;
  dim_after_ms: number | null;
  dim_brightness: number;
};

export type ClientBuildInfo = {
  platform: string;
  version: string;
  built_at_utc?: string | null;
  commit?: string | null;
  dirty: boolean;
};

export type MapLayerId =
  | "world_basemap"
  | "vectors"
  | "metars"
  | "nexrad"
  | "terrain_warning"
  | "offline_regions";

export type UiMapLayerToggleState = {
  visible: boolean;
  enabled: boolean;
  disabled_reason?: string | null;
};

export type UiMapLayerState = {
  world_basemap: UiMapLayerToggleState;
  vectors: UiMapLayerToggleState;
  metars: UiMapLayerToggleState;
  nexrad: UiMapLayerToggleState;
  terrain_warning: UiMapLayerToggleState;
  offline_regions: UiMapLayerToggleState;
};

export type UiChartPageState = {
  ordered_airport_ids: string[];
  recent_airport_ids: string[];
  plate_target_airport_id?: string | null;
  selected_airport_id: string;
  selected_reference_family_id?: string | null;
  selected_chart_id: string;
  suggested_chart_ids?: string[];
};

export type PointTilePayload = {
  schema_version: number;
  layer: string;
  z: number;
  x: number;
  y: number;
  records: Array<{
    id: string;
    kind: string;
    lat: number;
    lon: number;
    label: string;
    style_class: string;
    towered?: boolean;
    fuel_available?: boolean;
    has_paved_runway?: boolean | null;
    heliport?: boolean | null;
    has_water_runway?: boolean | null;
    longest_runway_length_ft?: number | null;
    longest_runway_heading_true_deg?: number | null;
    elevation_msl_ft?: number | null;
    obstacle?: {
      height_agl_ft: number;
      elevation_msl_ft: number;
      top_msl_ft: number;
      is_tall: boolean;
    } | null;
  }>;
};

export type AirspaceReferenceTilePayload = {
  schema_version: number;
  layer: string;
  z: number;
  x: number;
  y: number;
  refs: string[];
};

export type AirspaceLabelTilePayload = {
  schema_version: number;
  layer: string;
  z: number;
  x: number;
  y: number;
  labels: Array<{
    feature_id: string;
    text: string;
    lon: number;
    lat: number;
    style_hint: string;
  }>;
};

export type AirspaceFeaturePayload = {
  schema_version: number;
  id: string;
  kind: string;
  name: string;
  ident?: string;
  airspace_class: string;
  style_hint: string;
  vertical: {
    upper: { display: string };
    lower: { display: string };
  };
  bbox: [number, number, number, number];
  paths: Array<{
    role: string;
    closed: boolean;
    points: Array<[number, number]>;
  }>;
};

export type VectorTileRequest = {
  layer: string;
  z: number;
  x: number;
  y: number;
};

export type VisibleMapFeature = {
  id: string;
  kind: string;
  label: string;
  symbol_kind: "airport" | "nav" | "obstacle" | "fix" | string;
  style_class: string;
  obstacle_variant?: "short" | "tall" | null;
  obstacle_tone?: "danger" | "caution" | "muted" | null;
  screen_x: number;
  screen_y: number;
  towered: boolean;
  fuel_available: boolean;
  has_paved_runway?: boolean | null;
  heliport?: boolean | null;
  has_water_runway?: boolean | null;
  runway_length_ratio: number;
  longest_runway_heading_true_deg: number | null;
  label_style?: "default" | "flight_plan" | "active_flight_plan";
};

export type VisibleMetarFeature = {
  station_id: string;
  screen_x: number;
  screen_y: number;
  flight_category: "vfr" | "mvfr" | "ifr" | "lifr" | "missing" | string;
  ceiling_amount: "skc" | "few" | "sct" | "bkn" | "ovc" | "missing" | string;
};

export type VisiblePirepFeature = {
  id: string;
  screen_x: number;
  screen_y: number;
  symbol:
    | "generic"
    | "light-icing"
    | "moderate-icing"
    | "severe-icing"
    | "light-turbulence"
    | "moderate-turbulence"
    | "severe-turbulence"
    | string;
  icing: "none" | "light" | "moderate" | "severe" | "unknown" | string;
  turbulence: "none" | "light" | "moderate" | "severe" | "unknown" | string;
};

export type AirspaceFeatureRequest = {
  id: string;
  path: string;
};

export type AirspaceDisplayPath = {
  id: string;
  name: string;
  style_key: string;
  style: {
    fill_color_key: string;
    fill_opacity: number;
    strokes: Array<{
      color_key: string;
      width_px: number;
      dash_px: number[];
      line_cap: "butt" | "round" | "square" | string;
    }>;
  };
  paths: Array<{
    closed: boolean;
    points: Array<{ x: number; y: number }>;
  }>;
  decorations: Array<{
    color_key: string;
    width_px: number;
    line_cap: "butt" | "round" | "square" | string;
    paths?: Array<{
      closed: boolean;
      points: Array<{ x: number; y: number }>;
    }>;
    segments?: Array<[number, number, number, number]>;
  }>;
};

export type AirspaceDisplayLabel = {
  feature_id: string;
  glyph: AirspaceLimitGlyph;
  screen_x: number;
  screen_y: number;
};

export type AirspaceLimitGlyph = {
  upper: string;
  lower: string;
  style_key: string;
  color_key: string;
};

export type MapOverlayQueryResult = {
  visible_features: VisibleMapFeature[];
  flight_plan_features?: VisibleMapFeature[];
  visible_metars: VisibleMetarFeature[];
  visible_pireps: VisiblePirepFeature[];
  airspace_paths: AirspaceDisplayPath[];
  tfr_paths: AirspaceDisplayPath[];
  airspace_labels: AirspaceDisplayLabel[];
  offline_regions: Array<{
    id: string;
    kind: string;
    region_id: string;
    label: string;
    color_key: string;
    summary: Array<{
      action: string;
      cycle: string;
      count: number;
    }>;
    points: Array<{
      x: number;
      y: number;
    }>;
    label_x: number;
    label_y: number;
  }>;
};

export type MapSelectionQueryResult = {
  click_lat: number;
  click_lon: number;
  initial_selected_item_id?: string | null;
  categories: MapSelectionCategory[];
};

export type MapSelectionForNavRefResult = {
  position: LatLon;
  target_zoom: number;
  selection: MapSelectionQueryResult;
  selected_item_id?: string | null;
};

export type MapSelectionCategory = {
  id: string;
  label: string;
  items: MapSelectionItem[];
};

export type MapSelectionItem = {
  id: string;
  label: string;
  sublabel: string;
  description?: string | null;
  secondary_description?: string | null;
  detail_text?: string | null;
  highlight: MapSelectionHighlight;
  nav_ref?: NavRef | null;
  symbol_feature?: NavSymbolFeature | null;
  metar_feature?: VisibleMetarFeature | null;
  pirep_feature?: VisiblePirepFeature | null;
  airspace_icon?: AirspaceDisplayPath | null;
  actions: MapSelectionAction[];
};

export type MapSelectionHighlight =
  | { kind: "feature_ref"; id: string }
  | { kind: "metar"; station_id: string }
  | { kind: "pirep"; id: string }
  | { kind: "offline_region"; id: string }
  | { kind: "spot"; lat: number; lon: number };

export type MapSelectionAction = {
  id: string;
  label: string;
  enabled: boolean;
  display_only: boolean;
  detail_text?: string | null;
  detail_title?: string | null;
  detail_status?: MapSelectionDetailStatus | null;
  disabled_reason?: string | null;
  weather_detail?: WeatherDetailUiView | null;
  airport_info_airport_id?: string | null;
  airspace_limit?: AirspaceLimitGlyph | null;
  session_action?: string | null;
  flight_plan_row_action?: {
    row_uid: string;
    action_uid: string;
  } | null;
  navigation?: MapSelectionNavigationAction | null;
};

export type AirportInfoUiView = {
  airport_id: string;
  name: string;
  elevation_label: string;
  traffic_pattern_altitude_label: string;
  traffic_pattern_altitude_source: "published" | "derived";
  local_time_label: string;
  utc_time_label: string;
  time_zone_label: string;
  sunrise?: AirportSolarEventUiView | null;
  sunset?: AirportSolarEventUiView | null;
  communications: AirportCommunicationUiView[];
  runway_diagram_complex: boolean;
  runways: AirportRunwayUiView[];
};

export type AirportSolarEventUiView = {
  local_time_label: string;
  utc_time_label: string;
  next_in_label?: string | null;
};

export type AirportCommunicationUiView = {
  label: string;
  value: string;
  kind: "frequency" | "phone";
};

export type AirportRunwayUiView = {
  end_a_label: string;
  end_b_label: string;
  dimensions_label: string;
  surface_label: string;
  surface_color_key: string;
  diagram_end_a_x: number;
  diagram_end_a_y: number;
  diagram_end_b_x: number;
  diagram_end_b_y: number;
  diagram_width_ratio: number;
};

export type MapSelectionDetailStatus = {
  text: string;
  color_key: string;
};

export type MapSelectionNavigationAction =
  | {
      kind: "open_plate_target";
      airport_id: string;
      target: "Folder" | "CSup";
      chart_id: string;
    };

export type TerrainOverlayStatus =
  | { state: "hidden" }
  | { state: "no_position" }
  | { state: "no_altitude" }
  | { state: "too_many_tiles"; count: number }
  | { state: "unavailable"; reason: string }
  | { state: "ready"; count: number };

export type CoreResourceSource =
  | { kind: "public_url"; url: string }
  | { kind: "package_member"; package_id: string; filename: string; member_path: string }
  | {
      kind: "live_feed_package_member";
      product: string;
      version: string;
      blob_sha256: string;
      member_path: string;
    }
  | { kind: "installed_artifact_member"; filename: string; member_path: string }
  | { kind: "nav_kv_member"; member_path: string }
  | { kind: "unavailable"; message: string };

export type CoreResourceRequest = {
  id: string;
  source: CoreResourceSource;
  optional?: boolean;
};

export type TerrainOverlaySourceTile = {
  product_id: string;
  path: string;
  resource?: CoreResourceRequest | null;
};

export type TerrainOverlayTileRequest = {
  key: string;
  cache_key: string;
  product_id: string;
  path: string;
  source_tiles: TerrainOverlaySourceTile[];
  z: number;
  x: number;
  y_tms: number;
  left: number;
  top: number;
  size: number;
};

export type TerrainOverlayQueryResult = {
  status: TerrainOverlayStatus;
  tile_requests: TerrainOverlayTileRequest[];
  altitude_bucket_ft: number | null;
  frame_key: string | null;
  schedule: TerrainOverlayScheduleDecision;
};

export type TerrainOverlayScheduleDecision = {
  cached_count: number;
  in_flight_count: number;
  missing_count: number;
  frame_complete: boolean;
  work_batch: TerrainOverlayTileRequest[];
};

export type RasterTileSource = {
  map_view_id: string;
  package_name?: string | null;
  storage_kind: string;
  relative_path: string;
  resource:
    | { kind: "public_unpacked"; package_name: string; member_path: string }
    | { kind: "installed_package"; package_name: string; member_path: string }
    | { kind: "resolved_public_url"; url: string };
};

export type RasterTileDraw = {
  draw_key: string;
  family: ChartFamilyId;
  source_zoom: number;
  x: number;
  y_tms: number;
  left_px: number;
  top_px: number;
  size_px: number;
  z_order: number;
  primary: RasterTileSource;
  fallbacks: RasterTileSource[];
};

export type RasterTilePlan = {
  selected_map_id: string;
  tiles: RasterTileDraw[];
  chart_reference_action?: {
    family_id: ChartFamilyId;
    suggested_chart_ids: string[];
  } | null;
  debug_timing?: {
    planner_total_ms: number;
    planner_group_ms: number;
    planner_render_ms: number;
    planner_dedupe_ms: number;
    planner_draw_ms: number;
    planner_sort_ms: number;
    planner_families: number;
    planner_displayed_maps: number;
    planner_planned_tiles: number;
    planner_deduped_tiles: number;
    planner_tiles: number;
    session_total_ms?: number;
    session_lock_ms?: number;
    session_advance_ms?: number;
    session_freshness_ms?: number;
    session_catalog_filter_ms?: number;
    session_source_displayed_maps?: number;
    session_source_available_maps?: number;
    session_displayed_maps?: number;
  };
};

export type SessionSnapshotRefreshPriority = "timely" | "low_priority";

export type SessionSnapshotRefreshDecision =
  | { kind: "idle" }
  | { kind: "schedule"; delay_ms: number; reason: string }
  | { kind: "start"; reason: string };

export interface UiSession {
  setInvalidationListener(listener: UiInvalidationListener | null): void;
  initialSnapshot(): UiSessionSnapshot;
  snapshot(): Promise<UiSessionSnapshot>;
  maintainNavDb(nowEpochMs: number): Promise<UiSessionSnapshot>;
  requestSessionSnapshotRefresh(priority: SessionSnapshotRefreshPriority, reason: string): Promise<SessionSnapshotRefreshDecision>;
  sessionSnapshotViewportGestureActiveChanged(active: boolean): Promise<SessionSnapshotRefreshDecision>;
  sessionSnapshotViewportActivity(): Promise<SessionSnapshotRefreshDecision>;
  sessionSnapshotRefreshCompleted(): Promise<SessionSnapshotRefreshDecision>;
  pollSessionSnapshotRefresh(): Promise<SessionSnapshotRefreshDecision>;
  deriveChartPageState(): Promise<DerivedChartPageState>;
  airportInfo(airportId: string, nowEpochMs?: number): Promise<AirportInfoUiView>;
  insertWaypointAtFlightPlanRow(rowUid: string, before: boolean, waypoint: NavRef): Promise<UiSessionSnapshot>;
  suggestWaypointIdentifiersAtFlightPlanRow(rowUid: string, before: boolean, query: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  previewFlightPlanEntry(input: string): Promise<FlightPlanEntryPreview>;
  appendFlightPlanEntry(input: string): Promise<UiSessionSnapshot>;
  prepareAirwayPresentationAtFlightPlanRow(rowUid: string, airwayName: string): Promise<AirwayPresentationPlan>;
  insertAirwayAtFlightPlanRow(rowUid: string, presentation: AirwayPresentationPlan, entryPointUid: string, exitPointUid: string): Promise<UiSessionSnapshot>;
  selectProcedureAtFlightPlanRow(rowUid: string, airportId: string, procedureId: string, kind: ProcedureKind, runwayTransition: string | null, enrouteTransition: string | null): Promise<UiSessionSnapshot>;
  describePlateProcedureLoads(plateId: string): Promise<ProcedureLoadOption[]>;
  loadPlateProcedure(loadId: string): Promise<UiSessionSnapshot>;
  restoreDirectTo(): Promise<UiSessionSnapshot>;
  performFlightPlanRowAction(rowUid: string, actionUid: string): Promise<UiSessionSnapshot>;
  performStatusAction(actionId: string): Promise<UiSessionSnapshot>;
  performMapSelectionAction(action: string): Promise<UiSessionSnapshot>;
  activateNextLeg(): Promise<UiSessionSnapshot>;
  stopNavigation(): Promise<UiSessionSnapshot>;
  suspendSequencing(): Promise<UiSessionSnapshot>;
  unsuspendSequencing(): Promise<UiSessionSnapshot>;
  sequenceActiveLeg(): Promise<UiSessionSnapshot>;
  setSituation(situation: Situation): Promise<UiSessionSnapshot>;
  tickBadAutopilot(nowEpochMs: number): Promise<UiSessionSnapshot>;
  loadPlaybackTrace(sourcePath: string, traceJson: string): Promise<UiSessionSnapshot>;
  playPlayback(nowEpochMs: number): Promise<UiSessionSnapshot>;
  pausePlayback(nowEpochMs: number): Promise<UiSessionSnapshot>;
  seekPlayback(cursorSeconds: number, nowEpochMs: number): Promise<UiSessionSnapshot>;
  setPlaybackRate(rate: number, nowEpochMs: number): Promise<UiSessionSnapshot>;
  tickPlayback(nowEpochMs: number): Promise<UiSessionSnapshot>;
  engageMapFollow(viewport: MapViewportState): Promise<UiSessionSnapshot>;
  disengageMapFollow(viewport: MapViewportState): Promise<UiSessionSnapshot>;
  setMapFollowOffset(viewport: MapViewportState, offsetXPx: number, offsetYPx: number): Promise<UiSessionSnapshot>;
  syncMapFollow(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<UiSessionSnapshot>;
  registerOwnshipSource(registration: OwnshipSourceRegistration): Promise<UiSessionSnapshot>;
  updateOwnshipSourceStatus(update: OwnshipSourceStatusUpdate): Promise<UiSessionSnapshot>;
  pushSituationSample(sample: SituationSample): Promise<UiSessionSnapshot>;
  selectOwnshipSource(selection: OwnshipSelectionCommand): Promise<UiSessionSnapshot>;
  applySituationControlInput(input: SituationControlInput, nowEpochMs: number): Promise<UiSessionSnapshot>;
  setMapLayerVisibility(layerId: MapLayerId, visible: boolean): Promise<UiSessionSnapshot>;
  setMapLayerEnabled(layerId: MapLayerId, enabled: boolean): Promise<UiSessionSnapshot>;
  setDebugFlag(flagId: DebugFlagId, enabled: boolean): Promise<UiSessionSnapshot>;
  performSettingsAction(actionId: string, valueId: string): Promise<UiSessionSnapshot>;
  takeCloudAuthorizationRequest(nowEpochMs: number): Promise<CloudAuthorizationRequest | null>;
  completeCloudAuthorization(requestId: number, response: CloudAuthorizationResponse, nowEpochMs: number): Promise<UiSessionSnapshot>;
  performCloudUiAction(actionId: CloudUiActionId, fields: CloudUiFieldValue[], nowEpochMs: number): Promise<UiSessionSnapshot>;
  takeCloudProviderRequest(nowEpochMs: number): Promise<CloudHttpRequest | null>;
  completeCloudProviderRequest(requestId: number, response: CloudHttpResponse, nowEpochMs: number): Promise<UiSessionSnapshot>;
  acceptDisclaimer(agreementId: string): Promise<UiSessionSnapshot>;
  loadRasterMapCatalog(): Promise<UiSessionSnapshot>;
  resolveChartAssetUrl(chartId: string, assetKind: "asset" | "thumbnail"): Promise<string>;
  selectMapFamily(familyId: ChartFamilyId): Promise<UiSessionSnapshot>;
  selectRasterMap(selectedMapId: string): Promise<UiSessionSnapshot>;
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  selectChartReference(familyId: ChartFamilyId, suggestedChartIds: string[]): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  ingestAirspaceRefTiles(tiles: AirspaceReferenceTilePayload[]): Promise<void>;
  ingestAirspaceFeatures(features: AirspaceFeaturePayload[]): Promise<void>;
  ingestAirspaceLabelTiles(tiles: AirspaceLabelTilePayload[]): Promise<void>;
  queryMapOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<MapOverlayQueryResult>;
  queryMapSelection(viewport: MapViewportState, widthPx: number, heightPx: number, click: LatLon): Promise<MapSelectionQueryResult>;
  queryMapSelectionForNavRef(viewport: MapViewportState, widthPx: number, heightPx: number, navRef: NavRef): Promise<MapSelectionForNavRefResult>;
  queryTerrainOverlay(
    viewport: MapViewportState,
    widthPx: number,
    heightPx: number,
    decodedCacheKeys: string[],
    inFlightCacheKeys: string[],
  ): Promise<TerrainOverlayQueryResult>;
  queryNexradOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<NexradOverlayQueryResult>;
  queryRasterTilePlan(viewport: MapViewportState, widthPx: number, heightPx: number, devicePixelRatio?: number): Promise<RasterTilePlan>;
  renderTerrainOverlayTileByKey(tileKey: string, aircraftAltitudeFt: number): Promise<Uint8Array>;
  projectFlightPlanRoute(): Promise<FlightPlanRouteProjection>;
  syncLiveFeeds(): Promise<void>;
  startLiveFeedSubscription(): Promise<void>;
  notifyLiveFeedOnline(): void;
  stopLiveFeedSubscription(): Promise<void>;
  ingestLiveFeedSseEvent(event: LiveFeedSseEvent): Promise<void>;
  ingestLiveFeedSseEvents(events: LiveFeedSseEvent[]): Promise<void>;
  restoreChartPageState(
    recentAirportIds: string[],
    plateTargetAirportId?: string | null,
    selectedAirportId?: string,
    selectedReferenceFamilyId?: string | null,
    selectedChartId?: string,
    suggestedChartIds?: string[],
  ): Promise<UiSessionSnapshot>;
  destroy(): Promise<void>;
}

export type LiveFeedSseEvent = {
  id?: string | null;
  event?: string | null;
  data: string;
};

export type { UiInvalidation, UiInvalidationListener };

export interface AppCoreAdapter {
  prewarm(): Promise<void>;
  situationRingCandidates(): SituationRingCandidate[];
  loadSituationRingCandidates(): Promise<SituationRingCandidate[]>;
  createUiSession(
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession>;
  resolveWaypointIdentifier(identifier: string): Promise<NavRef | null>;
  resolveNavRefPosition(navRef: NavRef): Promise<LatLon>;
  suggestWaypointIdentifiersNear(anchor: LatLon, query: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  suggestAirwaysNearAnchor(anchor: NavRef, limit?: number): Promise<AirwaySuggestion[]>;
  listProcedures(airportId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]>;
  describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions>;
  findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null>;
}

export type AdapterBackendKind = "wasm" | "wasm-worker";

export type LoadedAdapter = {
  adapter: AppCoreAdapter;
  backend: AdapterBackendKind;
  detail: string;
};

function ownshipSelectionToCore(selection: OwnshipSelectionCommand): "auto" | { source: { source_id: string } } {
  if (selection.kind === "auto") {
    return "auto";
  }
  return {
    source: {
      source_id: sourceIdString(selection.source_id),
    },
  };
}

function sourceIdString(sourceId: { 0: string } | string): string {
  return typeof sourceId === "string" ? sourceId : sourceId[0];
}

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  resolve_metar_manifest_in_session(handle: number): Promise<string> | string;
  resolve_nav_db_artifact_candidates_in_session(handle: number): Promise<string> | string;
  resolve_chart_asset_resource_in_session(handle: number, chartId: string, assetKind: string): Promise<string> | string;
  situation_ring_candidates_json(): Promise<string> | string;
  create_ui_session(recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string, nowEpochMs: number): Promise<string> | string;
  create_ui_session_profiled?: (recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string, nowEpochMs: number) => Promise<string> | string;
  maintain_nav_db_in_session_at_epoch_ms(handle: number, nowEpochMs: bigint): Promise<string> | string;
  set_resource_policy_in_session(handle: number, policyJson: string): Promise<string> | string;
  configure_platform_capabilities_in_session(handle: number, capabilitiesJson: string): Promise<string> | string;
  should_prepare_live_feed_resource(resourceId: string): boolean;
  set_situation_in_session_paged(handle: number, situationJson: string): Promise<string> | string;
  tick_bad_autopilot_in_session_paged(handle: number, nowEpochMs: number): Promise<string> | string;
  engage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  disengage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  set_map_follow_offset_in_session(handle: number, viewportJson: string, offsetXPx: number, offsetYPx: number): Promise<string> | string;
  sync_map_follow_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  load_playback_trace_in_session_paged(handle: number, sourcePathJson: string, traceJson: string): Promise<string> | string;
  play_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<string> | string;
  pause_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<string> | string;
  seek_playback_in_session_paged(handle: number, cursorSeconds: number, nowEpochMs: number): Promise<string> | string;
  set_playback_rate_in_session_paged(handle: number, rate: number, nowEpochMs: number): Promise<string> | string;
  tick_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<string> | string;
  register_ownship_source_in_session_paged(handle: number, registrationJson: string): Promise<string> | string;
  update_ownship_source_status_in_session_paged(handle: number, updateJson: string): Promise<string> | string;
  push_situation_sample_in_session_paged(handle: number, sampleJson: string): Promise<string> | string;
  select_ownship_source_in_session_paged(handle: number, selectionJson: string): Promise<string> | string;
  apply_situation_control_input_in_session(handle: number, inputJson: string, nowEpochMs: number): Promise<string> | string;
  set_map_layer_visibility_in_session_paged(handle: number, layerIdJson: string, visible: boolean): Promise<string> | string;
  set_map_layer_enabled_in_session_paged(handle: number, layerIdJson: string, enabled: boolean): Promise<string> | string;
  set_debug_flag_in_session(handle: number, flagIdJson: string, enabled: boolean): Promise<string> | string;
  perform_settings_action_in_session(handle: number, actionJson: string): Promise<string> | string;
  take_cloud_authorization_request_in_session(handle: number, nowEpochMs: bigint): Promise<string> | string;
  complete_cloud_authorization_in_session(handle: number, requestId: bigint, responseJson: string, nowEpochMs: bigint): Promise<string> | string;
  perform_cloud_ui_action_in_session(handle: number, actionIdJson: string, fieldsJson: string, nowEpochMs: bigint): Promise<string> | string;
  take_cloud_provider_request_in_session(handle: number, nowEpochMs: bigint): Promise<string> | string;
  complete_cloud_provider_request_in_session(handle: number, requestId: bigint, responseJson: string, nowEpochMs: bigint): Promise<string> | string;
  accept_disclaimer_in_session(handle: number, agreementId: string): Promise<string> | string;
  load_raster_map_catalog_in_session(handle: number): Promise<string> | string;
  sync_guidance_geometry_in_session(handle: number): Promise<string> | string;
  project_flight_plan_route_in_session(handle: number): Promise<string> | string;
  select_map_family_in_session(handle: number, familyIdJson: string): Promise<string> | string;
  select_raster_map_in_session(handle: number, selectedMapIdJson: string): Promise<string> | string;
  perform_map_selection_action_in_session(
    sessionHandle: number,
    actionJson: string,
    nowEpochMs: bigint,
  ): Promise<string> | string;
  perform_flight_plan_command_in_session(
    sessionHandle: number,
    commandJson: string,
    nowEpochMs: bigint,
  ): Promise<string> | string;
  query_flight_plan_in_session(sessionHandle: number, queryJson: string): Promise<string> | string;
  perform_status_action_in_session(sessionHandle: number, actionId: string): Promise<string> | string;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<string> | string;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<string> | string;
  select_chart_reference_in_session(handle: number, familyIdJson: string, suggestedChartIdsJson: string): Promise<string> | string;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_ref_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_features_in_session(handle: number, featuresJson: string): Promise<void> | void;
  ingest_airspace_label_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_prepared_live_feed_resource_in_session(handle: number, resourceId: string, preparedResourceBytes: Uint8Array): Promise<void> | void;
  ingest_resource_in_session(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  report_session_resource_failure_in_session(handle: number, resourceId: string, message: string): Promise<string> | string;
  report_session_resource_failure_in_session_at_epoch_ms(handle: number, resourceId: string, message: string, nowEpochMs: number): Promise<string> | string;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<string> | string;
  get_map_selection_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, clickJson: string, nowEpochMs: number): Promise<string> | string;
  get_map_selection_for_nav_ref_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, navRefJson: string, nowEpochMs: number): Promise<string> | string;
  get_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<string> | string;
  get_scheduled_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, decodedCacheKeysJson: string, inFlightCacheKeysJson: string, nowEpochMs: number): Promise<string> | string;
  get_nexrad_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<string> | string;
  get_raster_tile_plan_in_session_with_display_scale(handle: number, viewportJson: string, widthPx: number, heightPx: number, devicePixelRatio: number, nowEpochMs: number): Promise<string> | string;
  render_terrain_overlay_tile_by_key_in_session(handle: number, terrainTileKey: string, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  get_session_snapshot_paged(handle: number): Promise<string> | string;
  get_session_snapshot_at_epoch_ms_paged(handle: number, nowEpochMs: bigint): Promise<string> | string;
  create_session_snapshot_refresh_scheduler(): Promise<number> | number;
  destroy_session_snapshot_refresh_scheduler(handle: number): Promise<void> | void;
  session_snapshot_refresh_scheduler_request(handle: number, priorityJson: string, reason: string): Promise<string> | string;
  session_snapshot_refresh_scheduler_viewport_gesture_active_changed(handle: number, active: boolean): Promise<string> | string;
  session_snapshot_refresh_scheduler_viewport_activity(handle: number): Promise<string> | string;
  session_snapshot_refresh_scheduler_refresh_completed(handle: number): Promise<string> | string;
  session_snapshot_refresh_scheduler_poll(handle: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, plateTargetAirportIdJson: string, selectedAirportIdJson: string, selectedReferenceFamilyIdJson: string, selectedChartIdJson: string, suggestedChartIdsJson: string): Promise<string> | string;
  destroy_session(handle: number): void;
  install_rust_debug_logger(): Promise<void> | void;
  nav_db_open_controller_create(candidatesJson: string, nowEpochMs: bigint): Promise<number> | number;
  nav_db_open_controller_destroy(handle: number): Promise<void> | void;
  nav_db_open_controller_finish(handle: number): Promise<string> | string;
  nav_db_open_controller_ingest_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  nav_db_open_controller_step(handle: number): Promise<string> | string;
  nav_kv_insert_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  nav_kv_prefetch_pages(handle: number): Promise<string> | string;
  nav_kv_destroy(handle: number): Promise<void> | void;
  attach_nav_kv_store_to_session(navKvHandle: number, sessionHandle: number): Promise<void> | void;
  core_had_operation(handle: number, operationJson: string): Promise<string> | string;
  drain_session_resource_effects(handle: number): Promise<string> | string;
  sync_live_feeds_in_session(handle: number): Promise<string> | string;
  configure_live_feed_source_in_session(handle: number, sourceRootUrl: string): Promise<void> | void;
  live_feed_events_url(sourceRootUrl: string): Promise<string> | string;
  live_feed_status_url(sourceRootUrl: string): Promise<string> | string;
  live_feed_runtime_decision_in_session(handle: number, inputJson: string): Promise<string> | string;
  refresh_live_feed_current_in_session(handle: number): Promise<string> | string;
  ingest_live_feed_sse_event_in_session(handle: number, eventJson: string): Promise<string> | string;
  ingest_live_feed_sse_events_in_session(handle: number, eventsJson: string): Promise<string> | string;
  report_live_feed_connection_event_in_session(handle: number, eventJson: string): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(private readonly module: WasmModule) {}

  async prewarm(): Promise<void> {}

  situationRingCandidates(): SituationRingCandidate[] {
    const candidatesJson = this.module.situation_ring_candidates_json();
    if (typeof candidatesJson !== "string") {
      throw new Error("situation_ring_candidates_json must be synchronous");
    }
    return JSON.parse(candidatesJson) as SituationRingCandidate[];
  }

  async loadSituationRingCandidates(): Promise<SituationRingCandidate[]> {
    return this.situationRingCandidates();
  }

  async createUiSession(
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession> {
    const module = this.module;
    let invalidationListener: UiInvalidationListener | null = null;
    const publishInvalidations: UiInvalidationListener = (invalidations) => {
      if (invalidations.length === 0) {
        return;
      }
      debugLog("core.ui.invalidations", { invalidations });
      invalidationListener?.(invalidations);
    };
    let reportSessionResourceFailure: ((resourceId: string, message: string) => Promise<void>) | null = null;
    const ingestResourceForHandle = async (
      sessionHandle: number,
      resourceId: string,
      resourceBytes: Uint8Array,
    ) => {
      if (await ingestPreparedLiveFeedResource(
        sessionHandle,
        resourceId,
        resourceBytes,
        (preparedSessionHandle, preparedResourceId, preparedBytes) =>
          debugTiming("live_feed.prepared.core_ingest", () =>
            this.module.ingest_prepared_live_feed_resource_in_session(
              preparedSessionHandle,
              preparedResourceId,
              preparedBytes,
            ), {
              resource_id: preparedResourceId,
              prepared_bytes: preparedBytes.byteLength,
            }),
        (candidateResourceId) =>
          this.module.should_prepare_live_feed_resource(candidateResourceId),
      )) {
        return;
      }
      await this.module.ingest_resource_in_session(sessionHandle, resourceId, resourceBytes);
    };
    const runSessionOperationForHandle = <T>(
      sessionHandle: number,
      operation: (navKvHandle: number) => Promise<string> | string,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => runCoreHadSessionOperation<T>(
      sessionHandle,
      operation,
      ingestSessionResource ?? ((resourceId, resourceBytes) =>
        ingestResourceForHandle(sessionHandle, resourceId, resourceBytes)),
      publishInvalidations,
      async (resourceId, message) => {
        await reportSessionResourceFailure?.(resourceId, message);
      },
      () => this.module.drain_session_resource_effects(sessionHandle),
      operationLabel,
      () => this.module.get_session_snapshot_paged(sessionHandle),
    );
    const createSession = async (
      nextRecentAirportIds: string[],
      nextSelectedAirportId?: string,
      nextSelectedChartId?: string,
    ) => {
      const recentAirportIdsJson = debugTiming("startup.session.stringify.recent_airports", () => JSON.stringify(nextRecentAirportIds));
      const selectedAirportIdJson = JSON.stringify(nextSelectedAirportId ?? null);
      const selectedChartIdJson = JSON.stringify(nextSelectedChartId ?? null);
      const createUiSession = module.create_ui_session_profiled ?? module.create_ui_session;
      const nowEpochMs = Date.now();
      const createdJson = await debugTiming("startup.session.wasm_call", () => createUiSession(
        recentAirportIdsJson,
        selectedAirportIdJson,
        selectedChartIdJson,
        nowEpochMs,
      ), { profiled: Boolean(module.create_ui_session_profiled) });
      const createdEnvelope = debugTiming("startup.session.parse_result", () => JSON.parse(createdJson)) as { result?: { handle: number; snapshot: UiSessionSnapshot }; timings?: Array<{ label: string; elapsed_ms: number }>; handle: number; snapshot: UiSessionSnapshot };
      if (createdEnvelope.timings) {
        debugLog("startup.session.core_profile", { timings: createdEnvelope.timings });
      }
      const created = createdEnvelope.result ?? createdEnvelope;
      await debugTiming("startup.session.reset_live_feed_prep", () => resetLiveFeedPrep());
      await debugTiming("startup.session.set_resource_policy", () =>
        module.set_resource_policy_in_session(created.handle, JSON.stringify("public_unpacked")),
      );
      await debugTiming("startup.session.configure_platform", () =>
        module.configure_platform_capabilities_in_session(
          created.handle,
          JSON.stringify({
            display_policy: null,
            offline_packages: null,
            cloud: { qr_scan: false },
            live_feeds: { acquisition_policy: "jit_public_resources" },
            client_build: __AEROBAG_CLIENT_BUILD_INFO__,
            local_time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
          }),
        ),
      );
      await debugTiming("startup.session.attach_nav_kv", () => attachNavKvStoreToSession(created.handle));
      const catalogedSnapshot = await debugTiming("startup.session.load_raster_catalog", () =>
        runSessionOperationForHandle<UiSessionSnapshot>(created.handle, () =>
          module.load_raster_map_catalog_in_session(created.handle),
        ),
      );
      return {
        ...created,
        snapshot: catalogedSnapshot,
      };
    };
    const init = await createSession(recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    let snapshot = init.snapshot;
    const snapshotRefreshSchedulerHandle = await debugTiming("startup.session.create_snapshot_scheduler", () =>
      this.module.create_session_snapshot_refresh_scheduler(),
    );
    const runSessionOperation = <T>(
      operation: (navKvHandle: number) => Promise<string> | string,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => runSessionOperationForHandle<T>(handle, operation, ingestSessionResource, operationLabel);
    const parseSessionSnapshotRefreshDecision = async (json: Promise<string> | string) =>
      JSON.parse(await json) as SessionSnapshotRefreshDecision;
    let liveFeedSubscription: LiveFeedSubscription | null = null;
    let configuredLiveFeedSourceUrl: string | null = null;
    reportSessionResourceFailure = async (resourceId, message) => {
      snapshot = await runSessionOperation<UiSessionSnapshot>(
        () => this.module.report_session_resource_failure_in_session_at_epoch_ms(
          handle,
          resourceId,
          message,
          Date.now(),
        ),
      );
      debugLog("core.ui.invalidations.source", {
        source: "resource_failure",
        resource_id: resourceId,
        message,
        invalidations: ["session_snapshot"],
      });
      publishInvalidations(["session_snapshot"]);
    };
    const configureLiveFeedSource = async () => {
      const sourceUrl = liveFeedSourceUrl();
      if (configuredLiveFeedSourceUrl !== sourceUrl) {
        await this.module.configure_live_feed_source_in_session(handle, sourceUrl);
        configuredLiveFeedSourceUrl = sourceUrl;
      }
      return sourceUrl;
    };
    const syncLiveFeeds = async () => {
      await runSessionOperation<unknown>(
        () => this.module.sync_live_feeds_in_session(handle),
        (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        "live_feeds.sync",
      );
    };
    const refreshLiveFeedCurrent = async () => {
      await runSessionOperation<unknown>(
        () => this.module.refresh_live_feed_current_in_session(handle),
        (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        "live_feeds.current_refresh",
      );
    };
    const refreshLiveFeedCurrentAndSync = async () => {
      await refreshLiveFeedCurrent();
      await syncLiveFeeds();
    };
    const handleLiveFeedRuntimeEvent = async (input: LiveFeedRuntimeInput): Promise<LiveFeedRuntimeDecision> => {
      const sourceUrl = await configureLiveFeedSource();
      const statusUrl = await this.module.live_feed_status_url(sourceUrl);
      const decision = JSON.parse(await this.module.live_feed_runtime_decision_in_session(handle, JSON.stringify({
        source_url: sourceUrl,
        status_url: statusUrl,
        ...input,
      }))) as LiveFeedRuntimeDecision;
      if (decision.connection_event) {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.report_live_feed_connection_event_in_session(
            handle,
            JSON.stringify(decision.connection_event),
          ),
        );
        publishInvalidations(["session_snapshot"]);
      }
      if (decision.refresh_current) {
        await refreshLiveFeedCurrentAndSync();
      }
      return decision;
    };
    const syncGuidanceGeometry = async (reason = "unspecified") => {
      snapshot = await debugTiming("plan.guidance.sync", () =>
        runSessionOperation<UiSessionSnapshot>(() =>
          this.module.sync_guidance_geometry_in_session(handle),
        ),
        { reason });
      return snapshot;
    };
    const runFlightPlanMutation = async (operation: () => Promise<string> | string) => {
      snapshot = await runSessionOperation<UiSessionSnapshot>(operation);
      return snapshot;
    };
    const performFlightPlanCommand = (command: Record<string, unknown>) =>
      runFlightPlanMutation(
        () =>
          this.module.perform_flight_plan_command_in_session(
            handle,
            JSON.stringify(command),
            BigInt(Date.now()),
          ),
      );
    const queryFlightPlan = <T,>(query: Record<string, unknown>) =>
      runSessionOperation<T>(
        () => this.module.query_flight_plan_in_session(handle, JSON.stringify(query)),
      );
    await debugTiming("startup.session.sync_guidance_geometry.initial", () => syncGuidanceGeometry());
    return {
      setInvalidationListener: (listener) => {
        invalidationListener = listener;
      },
      initialSnapshot: () => snapshot,
      snapshot: async () => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.get_session_snapshot_at_epoch_ms_paged(handle, BigInt(Date.now())));
        return snapshot;
      },
      maintainNavDb: async (nowEpochMs) => {
        const maintenance = await runSessionOperation<{
          action: "none" | "attempt_advance";
          snapshot: UiSessionSnapshot;
        }>(
          () => this.module.maintain_nav_db_in_session_at_epoch_ms(
            handle,
            BigInt(Math.trunc(nowEpochMs)),
          ),
          (resourceId, resourceBytes) => ingestResourceForHandle(
            handle,
            resourceId,
            resourceBytes,
          ),
          "nav_db.maintenance",
        );
        snapshot = maintenance.snapshot;
        if (maintenance.action === "attempt_advance") {
          const advanced = await advanceSharedNavKvStore(
            handle,
            (resourceId, resourceBytes) => ingestResourceForHandle(
              handle,
              resourceId,
              resourceBytes,
            ),
            publishInvalidations,
            reportSessionResourceFailure,
            () => this.module.drain_session_resource_effects(handle),
            () => this.module.get_session_snapshot_paged(handle),
          );
          snapshot = advanced.snapshot;
        }
        return snapshot;
      },
      requestSessionSnapshotRefresh: (priority, reason) =>
        parseSessionSnapshotRefreshDecision(
          this.module.session_snapshot_refresh_scheduler_request(
            snapshotRefreshSchedulerHandle,
            JSON.stringify(priority),
            reason,
          ),
        ),
      sessionSnapshotViewportGestureActiveChanged: (active) =>
        parseSessionSnapshotRefreshDecision(
          this.module.session_snapshot_refresh_scheduler_viewport_gesture_active_changed(
            snapshotRefreshSchedulerHandle,
            active,
          ),
        ),
      sessionSnapshotViewportActivity: () =>
        parseSessionSnapshotRefreshDecision(
          this.module.session_snapshot_refresh_scheduler_viewport_activity(
            snapshotRefreshSchedulerHandle,
          ),
        ),
      sessionSnapshotRefreshCompleted: () =>
        parseSessionSnapshotRefreshDecision(
          this.module.session_snapshot_refresh_scheduler_refresh_completed(
            snapshotRefreshSchedulerHandle,
          ),
        ),
      pollSessionSnapshotRefresh: () =>
        parseSessionSnapshotRefreshDecision(
          this.module.session_snapshot_refresh_scheduler_poll(snapshotRefreshSchedulerHandle),
        ),
      deriveChartPageState: async () => {
        return queryFlightPlan<DerivedChartPageState>({ kind: "chart_page_state" });
      },
      airportInfo: async (airportId, nowEpochMs = Date.now()) => {
        return queryFlightPlan<AirportInfoUiView>({
          kind: "airport_info",
          airport_id: airportId,
          now_epoch_ms: Math.trunc(nowEpochMs),
        });
      },
      performMapSelectionAction: async (action) => {
        return runFlightPlanMutation(
          () =>
            this.module.perform_map_selection_action_in_session(
              handle,
              action,
              BigInt(Date.now()),
            ),
        );
      },
      insertWaypointAtFlightPlanRow: async (rowUid, before, waypoint) => {
        return performFlightPlanCommand({
          kind: "insert_waypoint_at_row",
          row_uid: rowUid,
          before,
          waypoint,
        });
      },
      suggestWaypointIdentifiersAtFlightPlanRow: async (rowUid, before, query, limit = 8) => {
        return queryFlightPlan<WaypointIdentifierSuggestion[]>({
          kind: "suggest_waypoint_identifiers_at_row",
          row_uid: rowUid,
          before,
          query,
          limit,
        });
      },
      previewFlightPlanEntry: async (input) => {
        return queryFlightPlan<FlightPlanEntryPreview>({ kind: "preview_entry", input });
      },
      appendFlightPlanEntry: async (input) => {
        return performFlightPlanCommand({ kind: "append_entry", input });
      },
      prepareAirwayPresentationAtFlightPlanRow: async (rowUid, airwayName) => {
        return queryFlightPlan<AirwayPresentationPlan>({
          kind: "prepare_airway_presentation_at_row",
          row_uid: rowUid,
          airway_name: airwayName,
        });
      },
      insertAirwayAtFlightPlanRow: async (rowUid, presentation, entryPointUid, exitPointUid) => {
        return performFlightPlanCommand({
          kind: "insert_airway_at_row",
          row_uid: rowUid,
          selection: {
            airway_name: presentation.airway_name,
            branch_key: presentation.branch_key,
            entry_point_uid: entryPointUid,
            exit_point_uid: exitPointUid,
          },
        });
      },
      selectProcedureAtFlightPlanRow: async (rowUid, airportId, procedureId, kind, runwayTransition, enrouteTransition) => {
        const trace = { row_uid: rowUid, airport_id: airportId, procedure_id: procedureId, kind, runway_transition: runwayTransition, enroute_transition: enrouteTransition };
        return debugTiming("plan.procedure.select.session_mutation", () =>
          performFlightPlanCommand({
            kind: "select_procedure_at_row",
            row_uid: rowUid,
            airport_id: airportId,
            procedure_id: procedureId,
            procedure_kind: kind,
            runway_transition: runwayTransition,
            enroute_transition: enrouteTransition,
          }),
          trace);
      },
      describePlateProcedureLoads: async (plateId) => {
        return queryFlightPlan<ProcedureLoadOption[]>({
          kind: "describe_plate_procedure_loads",
          plate_id: plateId,
        });
      },
      loadPlateProcedure: async (loadId) => {
        return performFlightPlanCommand({ kind: "load_plate_procedure", load_id: loadId });
      },
      restoreDirectTo: async () => {
        return performFlightPlanCommand({ kind: "restore_direct_to" });
      },
      performFlightPlanRowAction: async (rowUid, actionUid) => {
        return performFlightPlanCommand({
          kind: "perform_row_action",
          row_uid: rowUid,
          action_uid: actionUid,
        });
      },
      performStatusAction: async (actionId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.perform_status_action_in_session(handle, actionId),
        );
        return snapshot;
      },
      activateNextLeg: async () => {
        return performFlightPlanCommand({ kind: "activate_next_leg" });
      },
      stopNavigation: async () => {
        return performFlightPlanCommand({ kind: "stop_navigation" });
      },
      suspendSequencing: async () => {
        return performFlightPlanCommand({ kind: "suspend_sequencing" });
      },
      unsuspendSequencing: async () => {
        return performFlightPlanCommand({ kind: "unsuspend_sequencing" });
      },
      sequenceActiveLeg: async () => {
        return performFlightPlanCommand({ kind: "sequence_active_leg" });
      },
      setSituation: async (situation) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.set_situation_in_session_paged(handle, JSON.stringify(situation)),
        );
        return snapshot;
      },
      tickBadAutopilot: async (nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.tick_bad_autopilot_in_session_paged(handle, nowEpochMs),
        );
        return syncGuidanceGeometry("tick_bad_autopilot");
      },
      registerOwnshipSource: async (registration) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.register_ownship_source_in_session_paged(handle, JSON.stringify(registration)),
        );
        return snapshot;
      },
      updateOwnshipSourceStatus: async (update) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.update_ownship_source_status_in_session_paged(handle, JSON.stringify(update)),
        );
        return snapshot;
      },
      pushSituationSample: async (sample) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.push_situation_sample_in_session_paged(handle, JSON.stringify(sample)),
        );
        return snapshot;
      },
      selectOwnshipSource: async (selection) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.select_ownship_source_in_session_paged(handle, JSON.stringify(ownshipSelectionToCore(selection))),
        );
        return snapshot;
      },
      applySituationControlInput: async (input, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.apply_situation_control_input_in_session(
            handle,
            JSON.stringify(input),
            nowEpochMs,
          ),
        );
        return snapshot;
      },
      setMapLayerVisibility: async (layerId, visible) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.set_map_layer_visibility_in_session_paged(handle, JSON.stringify(layerId), visible),
        );
        return snapshot;
      },
      setMapLayerEnabled: async (layerId, enabled) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(
          () => this.module.set_map_layer_enabled_in_session_paged(handle, JSON.stringify(layerId), enabled),
        );
        return snapshot;
      },
      setDebugFlag: async (flagId, enabled) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.set_debug_flag_in_session(handle, JSON.stringify(flagId), enabled),
        );
        return snapshot;
      },
      performSettingsAction: async (actionId, valueId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.perform_settings_action_in_session(
            handle,
            JSON.stringify({ action_id: actionId, value_id: valueId }),
          ),
        );
        return snapshot;
      },
      takeCloudAuthorizationRequest: async (nowEpochMs) => {
        return JSON.parse(
          await this.module.take_cloud_authorization_request_in_session(
            handle,
            BigInt(Math.trunc(nowEpochMs)),
          ),
        ) as CloudAuthorizationRequest | null;
      },
      completeCloudAuthorization: async (requestId, response, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.complete_cloud_authorization_in_session(
            handle,
            BigInt(requestId),
            JSON.stringify(response),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      performCloudUiAction: async (actionId, fields, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.perform_cloud_ui_action_in_session(
            handle,
            JSON.stringify(actionId),
            JSON.stringify(fields),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      takeCloudProviderRequest: async (nowEpochMs) => {
        return JSON.parse(
          await this.module.take_cloud_provider_request_in_session(
            handle,
            BigInt(Math.trunc(nowEpochMs)),
          ),
        ) as CloudHttpRequest | null;
      },
      completeCloudProviderRequest: async (requestId, response, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.complete_cloud_provider_request_in_session(
            handle,
            BigInt(requestId),
            JSON.stringify(response),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      acceptDisclaimer: async (agreementId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.accept_disclaimer_in_session(handle, agreementId),
        );
        return snapshot;
      },
      loadRasterMapCatalog: async () => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.load_raster_map_catalog_in_session(handle),
        );
        return snapshot;
      },
      resolveChartAssetUrl: (chartId, assetKind) =>
        resolveChartAssetUrl(handle, chartId, assetKind),
      selectMapFamily: async (familyId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.select_map_family_in_session(handle, JSON.stringify(familyId)),
        );
        return snapshot;
      },
      selectRasterMap: async (selectedMapId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.select_raster_map_in_session(handle, JSON.stringify(selectedMapId)),
        );
        return snapshot;
      },
      engageMapFollow: async (viewport) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.engage_map_follow_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
          ),
        );
        return snapshot;
      },
      disengageMapFollow: async (viewport) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.disengage_map_follow_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
          ),
        );
        return snapshot;
      },
      setMapFollowOffset: async (viewport, offsetXPx, offsetYPx) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.set_map_follow_offset_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
            offsetXPx,
            offsetYPx,
          ),
        );
        return snapshot;
      },
      syncMapFollow: async (viewport, widthPx, heightPx) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.sync_map_follow_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
            widthPx,
            heightPx,
          ),
        );
        return snapshot;
      },
      loadPlaybackTrace: async (sourcePath, traceJson) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.load_playback_trace_in_session_paged(handle, JSON.stringify(sourcePath), traceJson),
        );
        return snapshot;
      },
      playPlayback: async (nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.play_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      pausePlayback: async (nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.pause_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      seekPlayback: async (cursorSeconds, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.seek_playback_in_session_paged(handle, cursorSeconds, nowEpochMs),
        );
        return snapshot;
      },
      setPlaybackRate: async (rate, nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.set_playback_rate_in_session_paged(handle, rate, nowEpochMs),
        );
        return snapshot;
      },
      tickPlayback: async (nowEpochMs) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.tick_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      selectAirport: async (airportId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.select_airport_in_session(handle, JSON.stringify(airportId)),
        );
        return snapshot;
      },
      selectChart: async (chartId) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.select_chart_in_session(handle, JSON.stringify(chartId)),
        );
        return snapshot;
      },
      selectChartReference: async (familyId, suggestedChartIds) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.select_chart_reference_in_session(
            handle,
            JSON.stringify(familyId),
            JSON.stringify(suggestedChartIds),
          ),
        );
        return snapshot;
      },
      ingestPointTiles: async (tiles) => {
        await this.module.ingest_point_tiles_in_session(handle, JSON.stringify(tiles));
      },
      ingestAirspaceRefTiles: async (tiles) => {
        await this.module.ingest_airspace_ref_tiles_in_session(handle, JSON.stringify(tiles));
      },
      ingestAirspaceFeatures: async (features) => {
        await this.module.ingest_airspace_features_in_session(handle, JSON.stringify(features));
      },
      ingestAirspaceLabelTiles: async (tiles) => {
        await this.module.ingest_airspace_label_tiles_in_session(handle, JSON.stringify(tiles));
      },
      queryMapOverlay: async (viewport, widthPx, heightPx) =>
        runSessionOperation<MapOverlayQueryResult>(
          () =>
            this.module.get_map_overlay_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              Date.now(),
            ),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
          "map.overlay",
        ),
      queryMapSelection: async (viewport, widthPx, heightPx, click) =>
        runSessionOperation<MapSelectionQueryResult>(() =>
          this.module.get_map_selection_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
            widthPx,
            heightPx,
            JSON.stringify(click),
            Date.now(),
          ),
        ),
      queryMapSelectionForNavRef: async (viewport, widthPx, heightPx, navRef) =>
        runSessionOperation<MapSelectionForNavRefResult>(() =>
          this.module.get_map_selection_for_nav_ref_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
            widthPx,
            heightPx,
            JSON.stringify(navRef),
            Date.now(),
          ),
        ),
      queryTerrainOverlay: async (viewport, widthPx, heightPx, decodedCacheKeys, inFlightCacheKeys) =>
        runSessionOperation<TerrainOverlayQueryResult>(
          () =>
            this.module.get_scheduled_terrain_overlay_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              JSON.stringify(decodedCacheKeys),
              JSON.stringify(inFlightCacheKeys),
              Date.now(),
            ),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        ),
      queryNexradOverlay: async (viewport, widthPx, heightPx) =>
        runSessionOperation<NexradOverlayQueryResult>(
          () =>
            this.module.get_nexrad_overlay_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              Date.now(),
            ),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        ),
      queryRasterTilePlan: async (viewport, widthPx, heightPx, devicePixelRatio = 1) => {
        const totalStartedAt = performance.now();
        const stringifyStartedAt = performance.now();
        const viewportJson = JSON.stringify(coreViewportForMap(viewport));
        const stringifyMs = performance.now() - stringifyStartedAt;
        const wasmStartedAt = performance.now();
        const planJson = await this.module.get_raster_tile_plan_in_session_with_display_scale(
          handle,
          viewportJson,
          widthPx,
          heightPx,
          devicePixelRatio,
          Date.now(),
        );
        const wasmMs = performance.now() - wasmStartedAt;
        const parseStartedAt = performance.now();
        const plan = JSON.parse(planJson) as RasterTilePlan;
        const parseMs = performance.now() - parseStartedAt;
        const totalMs = performance.now() - totalStartedAt;
        perfDebugLog("raster.tile_plan.adapter_timing", () => ({
          total_ms: Math.round(totalMs),
          stringify_ms: Math.round(stringifyMs),
          wasm_ms: Math.round(wasmMs),
          parse_ms: Math.round(parseMs),
          json_bytes: planJson.length,
          tiles: plan.tiles.length,
          core_timing: plan.debug_timing ?? null,
          width_px: widthPx,
          height_px: heightPx,
          device_pixel_ratio: devicePixelRatio,
          zoom: viewport.zoom,
        }));
        return plan;
      },
      renderTerrainOverlayTileByKey: async (tileKey, aircraftAltitudeFt) =>
        new Uint8Array(await this.module.render_terrain_overlay_tile_by_key_in_session(handle, tileKey, aircraftAltitudeFt)),
      projectFlightPlanRoute: async () =>
        runSessionOperation<FlightPlanRouteProjection>(() =>
          this.module.project_flight_plan_route_in_session(handle),
        ),
      syncLiveFeeds,
      startLiveFeedSubscription: async () => {
        await handleLiveFeedRuntimeEvent({ kind: "start" });
        if (liveFeedSubscription) {
          return;
        }
        liveFeedSubscription = createLiveFeedSubscription(
          () => this.module.live_feed_events_url(liveFeedSourceUrl()),
          handleLiveFeedRuntimeEvent,
          async (events) => {
            await runSessionOperation<unknown>(
              () => this.module.ingest_live_feed_sse_events_in_session(handle, JSON.stringify(events)),
              (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
              "live_feeds.sse_ingest",
            );
          },
          (tag, data) => debugLog(tag, data),
        );
      },
      notifyLiveFeedOnline: () => {
        liveFeedSubscription?.notifyOnline();
      },
      stopLiveFeedSubscription: async () => {
        if (liveFeedSubscription) {
          const closing = liveFeedSubscription;
          liveFeedSubscription = null;
          closing.close();
          await handleLiveFeedRuntimeEvent({ kind: "closed" });
          return;
        }
        liveFeedSubscription = null;
      },
      ingestLiveFeedSseEvent: async (event) => {
        await runSessionOperation<unknown>(
          () => this.module.ingest_live_feed_sse_event_in_session(handle, JSON.stringify(event)),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
          "live_feeds.sse_ingest",
        );
      },
      ingestLiveFeedSseEvents: async (events) => {
        await runSessionOperation<unknown>(
          () => this.module.ingest_live_feed_sse_events_in_session(handle, JSON.stringify(events)),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        );
      },
      restoreChartPageState: async (
        nextRecentAirportIds,
        nextPlateTargetAirportId,
        nextSelectedAirportId,
        nextSelectedReferenceFamilyId,
        nextSelectedChartId,
        nextSuggestedChartIds = [],
      ) => {
        snapshot = await runSessionOperation<UiSessionSnapshot>(() =>
          this.module.restore_chart_page_state_in_session(
            handle,
            JSON.stringify(nextRecentAirportIds),
            JSON.stringify(nextPlateTargetAirportId ?? null),
            JSON.stringify(nextSelectedAirportId ?? null),
            JSON.stringify(nextSelectedReferenceFamilyId ?? null),
            JSON.stringify(nextSelectedChartId ?? null),
            JSON.stringify(nextSuggestedChartIds),
          ),
        );
        return snapshot;
      },
      destroy: async () => {
        liveFeedSubscription?.close();
        liveFeedSubscription = null;
        this.module.destroy_session(handle);
        await this.module.destroy_session_snapshot_refresh_scheduler(snapshotRefreshSchedulerHandle);
      },
    };
  }

  async resolveWaypointIdentifier(identifier: string): Promise<NavRef | null> {
    return runCoreHadOperation<NavRef | null>({ kind: "resolve_waypoint_identifier", identifier });
  }

  async resolveNavRefPosition(navRef: NavRef): Promise<LatLon> {
    return runCoreHadOperation<LatLon>({ kind: "resolve_nav_ref_position", nav_ref: navRef });
  }

  async suggestWaypointIdentifiersNear(anchor: LatLon, query: string, limit = 8): Promise<WaypointIdentifierSuggestion[]> {
    return runCoreHadOperation<WaypointIdentifierSuggestion[]>({
      kind: "suggest_waypoint_identifiers_near",
      anchor,
      query,
      limit,
    });
  }

  async suggestAirwaysNearAnchor(anchor: NavRef, limit = 30): Promise<AirwaySuggestion[]> {
    return runCoreHadOperation<AirwaySuggestion[]>({ kind: "suggest_airways_near_anchor", anchor, limit });
  }

  async listProcedures(airportId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]> {
    return runCoreHadOperation<ProcedureSummary[]>({
      kind: "list_procedures",
      airport_id: airportId,
      procedure_kind: kind,
    });
  }

  async describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions> {
    return runCoreHadOperation<ProcedureOptions>({
      kind: "describe_procedure_options",
      airport_id: airportId,
      procedure_id: procedureId,
      procedure_kind: kind,
    });
  }

  async findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null> {
    return runCoreHadOperation<CifpTppMatch | null>({
      kind: "find_procedure_plate_match",
      airport_id: airportId,
      cifp_id: cifpId,
    });
  }

}

const defaultWasmImporter = () => import("@generated/app_wasm.js");
let defaultAdapterLoadPromise: Promise<LoadedAdapter> | null = null;

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = defaultWasmImporter,
): Promise<LoadedAdapter> {
  if (importer === defaultWasmImporter) {
    defaultAdapterLoadPromise ??= loadDefaultAdapter().catch((error) => {
      defaultAdapterLoadPromise = null;
      throw error;
    });
    return defaultAdapterLoadPromise;
  }
  return loadBestAvailableAdapterUncached(importer);
}

async function loadDefaultAdapter(): Promise<LoadedAdapter> {
  if (shouldUseWorkerAppCore()) {
    const { loadWorkerBackedAdapter } = await import("./workerAppCoreAdapter");
    return await loadWorkerBackedAdapter();
  }
  return loadWasmAdapterOnThisThread();
}

function shouldUseWorkerAppCore(): boolean {
  if (typeof Worker === "undefined" || typeof URL === "undefined") {
    return false;
  }
  if (typeof window !== "undefined") {
    const setting = new URLSearchParams(window.location.search).get("appCoreWorker");
    if (setting === "0" || setting === "false") {
      return false;
    }
  }
  return true;
}

export async function loadWasmAdapterOnThisThread(
  importer: () => Promise<unknown> = defaultWasmImporter,
): Promise<LoadedAdapter> {
  return loadBestAvailableAdapterUncached(importer);
}

async function loadBestAvailableAdapterUncached(
  importer: () => Promise<unknown>,
): Promise<LoadedAdapter> {
  const mod = (await debugTiming("wasm.import", importer)) as Partial<WasmModule>;
  installRustDebugLogBridge();
  if (typeof mod.default === "function") {
    await debugTiming("wasm.init", () => mod.default?.());
  }
  mod.install_rust_debug_logger?.();
  debugLog("wasm.exports.check.start");
  const requiredExports = [
    "situation_ring_candidates_json",
    "create_ui_session",
    "maintain_nav_db_in_session_at_epoch_ms",
    "set_resource_policy_in_session",
    "configure_platform_capabilities_in_session",
    "should_prepare_live_feed_resource",
    "perform_flight_plan_command_in_session",
    "query_flight_plan_in_session",
    "perform_status_action_in_session",
    "set_situation_in_session_paged",
    "tick_bad_autopilot_in_session_paged",
    "engage_map_follow_in_session",
    "disengage_map_follow_in_session",
    "set_map_follow_offset_in_session",
    "sync_map_follow_in_session",
    "load_playback_trace_in_session_paged",
    "play_playback_in_session_paged",
    "pause_playback_in_session_paged",
    "seek_playback_in_session_paged",
    "set_playback_rate_in_session_paged",
    "tick_playback_in_session_paged",
    "register_ownship_source_in_session_paged",
    "update_ownship_source_status_in_session_paged",
    "push_situation_sample_in_session_paged",
    "select_ownship_source_in_session_paged",
    "apply_situation_control_input_in_session",
    "set_map_layer_visibility_in_session_paged",
    "set_map_layer_enabled_in_session_paged",
    "set_debug_flag_in_session",
    "perform_settings_action_in_session",
    "take_cloud_authorization_request_in_session",
    "complete_cloud_authorization_in_session",
    "perform_cloud_ui_action_in_session",
    "take_cloud_provider_request_in_session",
    "complete_cloud_provider_request_in_session",
    "accept_disclaimer_in_session",
    "load_raster_map_catalog_in_session",
    "resolve_chart_asset_resource_in_session",
    "sync_guidance_geometry_in_session",
    "project_flight_plan_route_in_session",
    "select_map_family_in_session",
    "select_raster_map_in_session",
    "perform_map_selection_action_in_session",
    "select_airport_in_session",
    "select_chart_in_session",
    "select_chart_reference_in_session",
    "ingest_point_tiles_in_session",
    "ingest_airspace_ref_tiles_in_session",
    "ingest_airspace_features_in_session",
    "ingest_airspace_label_tiles_in_session",
    "get_map_overlay_in_session",
    "get_map_selection_in_session",
    "get_map_selection_for_nav_ref_in_session",
    "get_terrain_overlay_in_session",
    "get_scheduled_terrain_overlay_in_session",
    "get_nexrad_overlay_in_session",
    "get_raster_tile_plan_in_session_with_display_scale",
    "render_terrain_overlay_tile_by_key_in_session",
    "get_session_snapshot_paged",
    "get_session_snapshot_at_epoch_ms_paged",
    "create_session_snapshot_refresh_scheduler",
    "destroy_session_snapshot_refresh_scheduler",
    "session_snapshot_refresh_scheduler_request",
    "session_snapshot_refresh_scheduler_viewport_gesture_active_changed",
    "session_snapshot_refresh_scheduler_viewport_activity",
    "session_snapshot_refresh_scheduler_refresh_completed",
    "session_snapshot_refresh_scheduler_poll",
    "restore_chart_page_state_in_session",
    "destroy_session",
    "install_rust_debug_logger",
    "ingest_resource_in_session",
    "report_session_resource_failure_in_session",
    "report_session_resource_failure_in_session_at_epoch_ms",
    "nav_db_open_controller_create",
    "nav_db_open_controller_destroy",
    "nav_db_open_controller_finish",
    "nav_db_open_controller_ingest_resource",
    "nav_db_open_controller_step",
    "nav_kv_insert_resource",
    "nav_kv_prefetch_pages",
    "nav_kv_destroy",
    "attach_nav_kv_store_to_session",
    "core_had_operation",
    "sync_live_feeds_in_session",
    "configure_live_feed_source_in_session",
    "live_feed_events_url",
    "live_feed_status_url",
    "live_feed_runtime_decision_in_session",
    "refresh_live_feed_current_in_session",
    "ingest_live_feed_sse_event_in_session",
    "ingest_live_feed_sse_events_in_session",
    "report_live_feed_connection_event_in_session",
  ] as const;
  const missingExports = requiredExports.filter((name) => typeof mod[name] !== "function");
  if (missingExports.length > 0) {
    debugLog("wasm.exports.check.missing", {
      missing: missingExports,
      available: Object.keys(mod).sort(),
    });
    throw new Error(`generated wasm module is missing required exports: ${missingExports.join(", ")}`);
  }
  const createUiSessionExport = mod.create_ui_session;
  if (typeof createUiSessionExport !== "function" || createUiSessionExport.length < 4) {
    throw new Error("generated wasm create_ui_session export is missing the explicit now_epoch_ms argument");
  }
  const createUiSessionProfiledExport = mod.create_ui_session_profiled;
  if (createUiSessionProfiledExport && createUiSessionProfiledExport.length < 4) {
    throw new Error("generated wasm create_ui_session_profiled export is missing the explicit now_epoch_ms argument");
  }
  debugLog("wasm.exports.check.done");

  return {
    adapter: new WasmAppCoreAdapter(mod as WasmModule),
    backend: "wasm",
    detail: "Using generated Rust WASM bindings.",
  };
}

export function coreViewportForMap(viewport: MapViewportState) {
  const center = viewportCenterLatLon(viewport);
  return {
    center,
    zoom: viewport.zoom,
    rotation_deg: viewport.rotationDeg ?? 0,
    pitch_deg: 0,
  };
}

type LiveFeedSubscription = {
  notifyOnline(): void;
  close(): void;
};

type LiveFeedRuntimeInput = {
  kind: "start" | "network_status" | "connecting" | "connected" | "message" | "error" | "closed" | "idle_timeout" | "online";
  message?: string | null;
  source_url?: string | null;
  status_url?: string | null;
  network_status?: "unmetered" | "metered" | "no_active_network" | "unknown" | null;
};

type LiveFeedRuntimeDecision = {
  connection_event?: {
    kind: "connecting" | "connected" | "message" | "error" | "closed" | "network_status";
    message?: string | null;
    source_url?: string | null;
    status_url?: string | null;
    network_status?: "unmetered" | "metered" | "no_active_network" | "unknown" | null;
  } | null;
  refresh_current?: boolean;
  reconnect_delay_ms?: number | null;
};

export function createLiveFeedSubscription(
  liveFeedEventsUrl: () => Promise<string> | string,
  handleRuntimeEvent: (input: LiveFeedRuntimeInput) => Promise<LiveFeedRuntimeDecision>,
  ingestEvents: (events: LiveFeedSseEvent[]) => Promise<void>,
  log: (tag: string, data?: unknown) => void,
): LiveFeedSubscription {
  if (typeof EventSource === "undefined") {
    throw new Error("EventSource is unavailable in this app-core runtime");
  }
  const queuedEvents: LiveFeedSseEvent[] = [];
  let closed = false;
  let events: EventSource | null = null;
  let flushTimer: number | null = null;
  let flushInFlight = false;
  let flushAgain = false;
  let reconnectTimer: number | null = null;
  const probeState = liveFeedE2eProbeState();
  const trackedEventSources = new WeakSet<EventSource>();
  const trackEventSource = (source: EventSource) => {
    trackedEventSources.add(source);
    probeState.active_event_sources += 1;
    probeState.last_ready_state = source.readyState;
  };
  const closeEventSource = (source: EventSource | null) => {
    if (!source) {
      return;
    }
    if (trackedEventSources.delete(source)) {
      probeState.active_event_sources = Math.max(0, probeState.active_event_sources - 1);
    }
    source.close();
    probeState.last_ready_state = source.readyState;
  };
  const runtimeEvent = (input: LiveFeedRuntimeInput): Promise<LiveFeedRuntimeDecision> =>
    handleRuntimeEvent(input).catch((error: unknown) => {
      log("live_feeds.connection_event.failed", { message: error instanceof Error ? error.message : String(error) });
      throw error;
    });
  const reportEvent = (input: LiveFeedRuntimeInput) => {
    void runtimeEvent(input).catch(() => {});
  };
  const clearReconnectTimer = () => {
    if (reconnectTimer === null) {
      return;
    }
    globalThis.clearTimeout(reconnectTimer);
    reconnectTimer = null;
  };
  const scheduleFlush = () => {
    if (flushTimer !== null || closed) {
      return;
    }
    flushTimer = globalThis.setTimeout(flushQueuedEvents, 100) as unknown as number;
  };
  const flushQueuedEvents = () => {
    flushTimer = null;
    if (closed || queuedEvents.length === 0) {
      return;
    }
    if (flushInFlight) {
      flushAgain = true;
      return;
    }
    const batch = queuedEvents.splice(0, queuedEvents.length);
    flushInFlight = true;
    void ingestEvents(batch).catch((error: unknown) => {
      log("live_feeds.sse_events.failed", { message: error instanceof Error ? error.message : String(error) });
    }).finally(() => {
      flushInFlight = false;
      if (closed) {
        return;
      }
      if (flushAgain || queuedEvents.length > 0) {
        flushAgain = false;
        scheduleFlush();
      }
    });
  };
  const openConnection = () => {
    if (closed) {
      return;
    }
    clearReconnectTimer();
    reportEvent({ kind: "connecting" });
    void Promise.resolve(liveFeedEventsUrl()).then((eventsUrl) => {
      if (closed) {
        return;
      }
      log("live_feeds.sse.opening", { events_url: eventsUrl });
      const nextEvents = new EventSource(eventsUrl);
      probeState.open_attempts += 1;
      probeState.last_events_url = eventsUrl;
      trackEventSource(nextEvents);
      events = nextEvents;
      const isCurrent = () => !closed && events === nextEvents;
      nextEvents.addEventListener("live-feed-current", (event) => {
        if (!isCurrent()) {
          return;
        }
        clearReconnectTimer();
        const message = event as MessageEvent<string>;
        probeState.messages += 1;
        probeState.last_ready_state = nextEvents.readyState;
        reportEvent({ kind: "message" });
        queuedEvents.push({
          id: message.lastEventId || null,
          event: "live-feed-current",
          data: message.data,
        });
        scheduleFlush();
      });
      nextEvents.onopen = () => {
        if (!isCurrent()) {
          return;
        }
        clearReconnectTimer();
        log("live_feeds.sse.open", { events_url: eventsUrl });
        probeState.last_ready_state = nextEvents.readyState;
        reportEvent({ kind: "connected" });
      };
      nextEvents.onerror = () => {
        if (!isCurrent()) {
          return;
        }
        log("live_feeds.sse.error", { ready_state: nextEvents.readyState });
        probeState.errors += 1;
        probeState.last_ready_state = nextEvents.readyState;
        void runtimeEvent({ kind: "error", message: "EventSource error" })
          .then((decision) => scheduleReconnect(decision.reconnect_delay_ms ?? 0))
          .catch(() => scheduleReconnect(0));
      };
    }).catch((error: unknown) => {
      log("live_feeds.sse.url.failed", { message: error instanceof Error ? error.message : String(error) });
      void runtimeEvent({ kind: "error", message: error instanceof Error ? error.message : String(error) })
        .then((decision) => scheduleReconnect(decision.reconnect_delay_ms ?? 0))
        .catch(() => scheduleReconnect(0));
    });
  };
  const scheduleReconnect = (delayMs: number) => {
    if (closed) {
      return;
    }
    if (reconnectTimer !== null) {
      return;
    }
    probeState.reconnect_scheduled += 1;
    probeState.last_reconnect_delay_ms = delayMs;
    reconnectTimer = globalThis.setTimeout(() => {
      reconnectTimer = null;
      if (closed) {
        return;
      }
      const previous = events;
      events = null;
      closeEventSource(previous);
      openConnection();
    }, delayMs) as unknown as number;
  };
  const handleOnline = () => {
    if (closed) {
      return;
    }
    probeState.online_events += 1;
    clearReconnectTimer();
    void runtimeEvent({ kind: "online" })
      .then((decision) => scheduleReconnect(decision.reconnect_delay_ms ?? 0))
      .catch(() => scheduleReconnect(0));
  };
  if (typeof window !== "undefined") {
    window.addEventListener("online", handleOnline);
  }
  openConnection();
  return {
    notifyOnline: handleOnline,
    close: () => {
      closed = true;
      clearReconnectTimer();
      if (flushTimer !== null) {
        globalThis.clearTimeout(flushTimer);
        flushTimer = null;
      }
      if (typeof window !== "undefined") {
        window.removeEventListener("online", handleOnline);
      }
      closeEventSource(events);
      events = null;
    },
  };
}
