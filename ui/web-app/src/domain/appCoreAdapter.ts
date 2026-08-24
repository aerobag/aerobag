// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  AltitudeComparisonPanelUiView,
  AppUiState,
  AirwayPresentationPlan,
  AirwaySuggestion,
  CifpTppMatch,
  ChartPageData,
  FlightPlanEntryPreview,
  FlightPlanRouteProjection,
  ChartFamilyId,
  LatLon,
  MapFollowUiState,
  NavRef,
  NavSymbolFeature,
  OwnshipSelectionCommand,
  OwnshipSourceRegistration,
  OwnshipSourceStatusUpdate,
  PlaybackUiState,
  ProcedureLoadMenu,
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
import {
  UI_SESSION_UPDATE_GROUPS,
  type UiSessionUpdateGroup,
} from "../generated/sessionUpdateWire";
import type {
  CloudEventStreamEvent,
  CloudEventStreamPlan,
  CloudHttpRequest,
  CloudHttpResponse,
  CloudUiActionId,
  CloudUiFieldValue,
  UiCloudPageState,
} from "../generated/cloudWire";
import type { UiHomePageState } from "../generated/homePageWire";
import type {
  ClientBuildInfo,
  MapLayerId,
  UiChartPageState,
  UiDataStatusPageState,
  UiDataStatusState,
  UiDebugState,
  UiDisclaimerState,
  UiDisplayPolicy,
  UiMapLayerState,
  UiNavigationPageState,
  UiPlaybackPanelState,
  UiSettingsPageState,
  UiStatusActionDecision,
  UiSurfaceStatusState,
} from "../generated/sessionPageWire";
import { UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION } from "../generated/sessionPageWire";
export { UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION } from "../generated/sessionPageWire";
export type {
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
export type {
  ClientBuildInfo,
  DebugFlagId,
  MapLayerId,
  UiChartPageState,
  UiDataStatusBox,
  UiDataStatusPageFact,
  UiDataStatusPageRow,
  UiDataStatusPageState,
  UiDataStatusState,
  UiDebugState,
  UiDisclaimerState,
  UiDisplayPolicy,
  UiMapLayerState,
  UiMapLayerToggleState,
  UiNavigationPageId,
  UiNavigationPageOption,
  UiNavigationPageState,
  UiPlaybackPanelState,
  UiSettingsGridItem,
  UiSettingsPageRow,
  UiSettingsPageState,
  UiSettingsSliderStop,
  UiStatusAction,
  UiStatusActionStyle,
  UiStatusSeverity,
  UiStatusActionDecision,
  UiStatusPlatformEffect,
  UiSurfaceStatusControl,
  UiSurfaceStatusControlId,
  UiSurfaceStatusState,
} from "../generated/sessionPageWire";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import { packageSourceBaseUrl } from "./packageSourceUrl";
import {
  advanceSharedNavKvStore,
  attachNavKvStoreToSession,
  completeResourceFreeSessionMutation,
  resolveChartAssetUrl,
  runCoreHadOperation,
  runCoreHadSessionMutationOperation,
  runCoreHadSessionResultOperation,
  runCoreHadSessionSnapshotOperation,
  type SessionMutationOperation,
  type SessionMutationOperationJson,
  type SessionResultOperation,
  type SessionResultOperationJson,
  type SessionSnapshotOperation,
  type SessionSnapshotOperationJson,
  type UiInvalidation,
  type UiInvalidationListener,
} from "./navKv";
import {
  DebugLogDeveloperServerPath,
  debugLog,
  debugTiming,
  installRustDebugLogBridge,
  isDebugLogEnabled,
  perfDebugLog,
} from "./debugLog";
import { ingestPreparedLiveFeedResource, resetLiveFeedPrep } from "./liveFeedPrep";
import { liveFeedSourceUrl } from "./liveFeedUrls";
import { SessionUpdateAccumulator } from "./sessionUpdateAccumulator";
import { WebUiSessionWorkRunner } from "./uiSessionWorkRunner";
export { resolveLiveFeedResourceUrl, resolveLiveFeedSourceUrl } from "./liveFeedUrls";

export function sessionUpdateGroupNames(value: unknown): UiSessionUpdateGroup[] {
  if (!value || typeof value !== "object") {
    return [];
  }
  const update = value as Record<string, unknown>;
  return UI_SESSION_UPDATE_GROUPS.filter((group) => update[group] != null);
}

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

declare const __AEROBAG_CLIENT_BUILD_INFO__: ClientBuildInfo;
declare const __AEROBAG_CLOUD_SERVER_BASE_URL__: string | null;

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
  collection_control: ChartSelectorControlUiView;
  chart_control: ChartSelectorControlUiView;
  procedure_load_menu: ProcedureLoadMenu;
  procedure_geometry_status: UiDataStatusState;
  status_controls: UiSurfaceStatusState;
};

export type ChartSelectorControlUiView = {
  launcher_label: string;
  enabled: boolean;
  disabled_reason?: string | null;
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
  ui_contract_version: number;
  session_revision: number;
  flight_plan_route_revision: number;
  notam_display_state_id?: string | null;
  nav_data_epoch: number;
  active_nav_db: {
    package_id: string;
    filename: string;
    contract_id: string | null;
    cycle: string | null;
    cycle_version: string | null;
  } | null;
  next_nav_db_maintenance_epoch_ms: number | null;
  next_session_snapshot_refresh_epoch_ms: number;
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
  map_status_controls: UiSurfaceStatusState;
  data_status_page_state: UiDataStatusPageState;
  settings_page_state: UiSettingsPageState;
  cloud_page_state: UiCloudPageState;
  offline_package_preferences_json: string;
  home_page_state: UiHomePageState;
  navigation_page_state: UiNavigationPageState;
  display_policy: UiDisplayPolicy | null;
  disclaimer_state: UiDisclaimerState;
  debug_state: UiDebugState;
  raster_map?: RasterMapUiState | null;
  next_cycle_product_freshness_check_epoch_ms?: number | null;
};

function assertUiContractVersion(snapshot: UiSessionSnapshot): UiSessionSnapshot {
  if (snapshot.ui_contract_version !== UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION) {
    throw new Error(
      `UI wire contract ${snapshot.ui_contract_version} is unsupported; client requires ${UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION}`,
    );
  }
  return snapshot;
}


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
  visible_traffic: Array<{
    id: string;
    screen_x: number;
    screen_y: number;
    track_deg_true?: number | null;
    label: string;
    detail_label: string;
  }>;
  traffic_next_refresh_epoch_ms?: number | null;
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
  distance?: string | null;
  distance_target?: LatLon | null;
  secondary_description?: string | null;
  detail_text?: string | null;
  highlight: MapSelectionHighlight;
  nav_ref?: NavRef | null;
  symbol_feature?: NavSymbolFeature | null;
  metar_feature?: VisibleMetarFeature | null;
  weather_detail?: WeatherDetailUiView | null;
  automatic_action_uid?: string | null;
  pirep_feature?: VisiblePirepFeature | null;
  airspace_icon?: AirspaceDisplayPath | null;
  actions: MapSelectionAction[];
};

export type MapSelectionHighlight =
  | { kind: "feature_ref"; id: string }
  | { kind: "metar"; station_id: string }
  | { kind: "pirep"; id: string }
  | { kind: "adsb_traffic"; id: string }
  | { kind: "offline_region"; id: string }
  | { kind: "spot"; lat: number; lon: number };

export type MapSelectionAction = {
  id: string;
  label: string;
  enabled: boolean;
  display_only: boolean;
  action_uid?: string | null;
  placeholder: boolean;
  disabled_reason?: string | null;
  airspace_limit?: AirspaceLimitGlyph | null;
};

export type MapSelectionActionDecision = {
  perform_session_mutation: boolean;
  dismiss_selection: boolean;
  effect?: MapSelectionActionEffect | null;
};

export type FlightPlanRowActionDecision = {
  perform_session_mutation: boolean;
  dismiss_tray: boolean;
  effect?: FlightPlanRowActionEffect | null;
};

export type FlightPlanRowActionEffect =
  | { kind: "show_weather"; detail: WeatherDetailUiView }
  | { kind: "load_airport_info"; airport_id: string }
  | { kind: "open_airport_charts"; airport_id: string }
  | { kind: "open_plate_target"; airport_id: string; target: string }
  | { kind: "open_waypoint_insert"; row_uid: string; before: boolean }
  | {
      kind: "open_airway_picker";
      row_uid: string;
      header: string;
      origin_anchor: NavRef;
      destination_anchor?: NavRef | null;
    }
  | {
      kind: "open_procedure_picker";
      row_uid: string;
      airport_id: string;
      procedure_kind: ProcedureKind;
      title: string;
      empty_message: string;
    };

export type MapSelectionActionEffect =
  | { kind: "show_weather"; detail: WeatherDetailUiView }
  | { kind: "load_airport_info"; airport_id: string; loading_text: string; failure_prefix: string }
  | { kind: "show_detail"; title: string; text: string; status?: MapSelectionDetailStatus | null }
  | { kind: "open_plate_target"; airport_id: string; target: "Folder" | "CSup"; chart_id: string }
  | { kind: "open_external_url"; url: string };

export type AirportInfoUiView = {
  airport_id: string;
  name: string;
  location_label?: string | null;
  elevation_label: string;
  traffic_pattern_altitude_label: string;
  traffic_pattern_altitude_source: "published" | "derived";
  time_label: string;
  time_display_action_id: string;
  time_zone_label: string;
  sunrise?: AirportSolarEventUiView | null;
  sunset?: AirportSolarEventUiView | null;
  communications: AirportCommunicationUiView[];
  fact_sections: AirportInfoFactSectionUiView[];
  runways_section_title: string;
  runway_diagram_complex: boolean;
  runways: AirportRunwayUiView[];
};

export type AirportInfoFactSectionUiView = {
  title?: string | null;
  facts: AirportInfoFactUiView[];
};

export type AirportInfoFactUiView = {
  label: string;
  value: string;
  next_in_label?: string | null;
  action_id?: string | null;
  link_url?: string | null;
};

export type AirportSolarEventUiView = {
  time_label: string;
  time_display_action_id: string;
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
  diagram_end_a_pattern?: AirportRunwayPatternUiView | null;
  diagram_end_b_pattern?: AirportRunwayPatternUiView | null;
};

export type AirportRunwayPatternUiView = {
  base_x: number;
  base_y: number;
  corner_x: number;
  corner_y: number;
  final_x: number;
  final_y: number;
};

export type MapSelectionDetailStatus = {
  text: string;
  color_key: string;
  action_id?: string | null;
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

export type UiSessionProjectionLanding =
  | { kind: "update"; value: unknown }
  | { kind: "full_snapshot"; value: UiSessionSnapshot };

export type UiSessionProjectionPublication = {
  landing: UiSessionProjectionLanding;
  snapshot: UiSessionSnapshot;
  changedGroups: readonly UiSessionUpdateGroup[];
  fullSnapshot: boolean;
};

export type UiSessionProjectionListener = (publication: UiSessionProjectionPublication) => void;

export interface UiSession {
  setInvalidationListener(listener: UiInvalidationListener | null): void;
  setProjectionListener(listener: UiSessionProjectionListener | null): void;
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
  describePlateProcedureLoads(plateId: string): Promise<ProcedureLoadMenu>;
  loadPlateProcedure(loadId: string): Promise<UiSessionSnapshot>;
  performFlightPlanControl(controlId: import("./types").FlightPlanControlId): Promise<UiSessionSnapshot>;
  flightPlanRowActionDecision(rowUid: string, actionUid: string): Promise<FlightPlanRowActionDecision>;
  performFlightPlanRowAction(rowUid: string, actionUid: string): Promise<UiSessionSnapshot>;
  altitudeComparisons(): Promise<AltitudeComparisonPanelUiView>;
  performAltitudePlannerAction(actionUid: string): Promise<UiSessionSnapshot>;
  setAltitudePlannerDepartureInput(field: "time" | "when", input: string): Promise<UiSessionSnapshot>;
  performFlightPlanColumnAction(actionId: string): Promise<UiSessionSnapshot>;
  performTimeDisplayAction(actionId: string): Promise<UiSessionSnapshot>;
  statusActionDecision(actionId: string): Promise<UiStatusActionDecision>;
  performStatusAction(actionId: string): Promise<UiSessionSnapshot>;
  mapSelectionActionDecision(actionUid: string): Promise<MapSelectionActionDecision>;
  performMapSelectionUiAction(actionUid: string): Promise<UiSessionSnapshot>;
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
  performOwnshipTextAction(actionId: string, value: string, nowEpochMs: number): Promise<UiSessionSnapshot>;
  applySituationControlInput(input: SituationControlInput, nowEpochMs: number): Promise<UiSessionSnapshot>;
  setMapLayerVisibility(layerId: MapLayerId, visible: boolean): Promise<UiSessionSnapshot>;
  setMapLayerEnabled(layerId: MapLayerId, enabled: boolean): Promise<UiSessionSnapshot>;
  performSettingsAction(actionId: string, valueId: string): Promise<UiSessionSnapshot>;
  performAircraftLibraryAction(actionId: string, sourceJson?: string): Promise<UiSessionSnapshot>;
  performCloudUiAction(actionId: CloudUiActionId, fields: CloudUiFieldValue[], nowEpochMs: number): Promise<UiSessionSnapshot>;
  recordOfflinePackagePreferences(preferencesJson: string, nowEpochMs: number): Promise<UiSessionSnapshot>;
  takeCloudProviderRequest(nowEpochMs: number): Promise<CloudHttpRequest | null>;
  completeCloudProviderRequest(requestId: number, response: CloudHttpResponse, nowEpochMs: number): Promise<UiSessionSnapshot>;
  cloudEventStreamPlan(): Promise<CloudEventStreamPlan | null>;
  reportCloudEventStreamEvent(event: CloudEventStreamEvent, nowEpochMs: number): Promise<UiSessionSnapshot>;
  acceptDisclaimer(agreementId: string): Promise<UiSessionSnapshot>;
  loadRasterMapCatalog(): Promise<UiSessionSnapshot>;
  resolveChartAssetUrl(chartId: string, assetKind: "asset" | "thumbnail"): Promise<string>;
  selectMapFamily(familyId: ChartFamilyId): Promise<UiSessionSnapshot>;
  selectRasterMap(selectedMapId: string): Promise<UiSessionSnapshot>;
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  openChartAirport(airportId: string, chartId?: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  selectChartReference(familyId: ChartFamilyId, suggestedChartIds: string[]): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  ingestAirspaceRefTiles(tiles: AirspaceReferenceTilePayload[]): Promise<void>;
  ingestAirspaceFeatures(features: AirspaceFeaturePayload[]): Promise<void>;
  ingestAirspaceLabelTiles(tiles: AirspaceLabelTilePayload[]): Promise<void>;
  queryMapOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<MapOverlayQueryResult>;
  queryMapSelection(viewport: MapViewportState, widthPx: number, heightPx: number, click: LatLon): Promise<MapSelectionQueryResult>;
  queryMapSelectionDistance(target: LatLon): Promise<string | null>;
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
    nowEpochMs?: number,
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
  maintain_nav_db_in_session_at_epoch_ms(handle: number, nowEpochMs: bigint): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  set_resource_policy_in_session(handle: number, policyJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  configure_platform_capabilities_in_session(handle: number, capabilitiesJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  configure_data_sources_in_session(handle: number, cycleDataBaseUrl: string, liveFeedsBaseUrl: string, debugLogSinkUrl?: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  should_prepare_live_feed_resource(resourceId: string): boolean;
  set_situation_in_session_paged(handle: number, situationJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  tick_bad_autopilot_in_session_paged(handle: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  engage_map_follow_in_session(handle: number, viewportJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  disengage_map_follow_in_session(handle: number, viewportJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  set_map_follow_offset_in_session(handle: number, viewportJson: string, offsetXPx: number, offsetYPx: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  sync_map_follow_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  load_playback_trace_in_session_paged(handle: number, sourcePathJson: string, traceJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  play_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  pause_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  seek_playback_in_session_paged(handle: number, cursorSeconds: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  set_playback_rate_in_session_paged(handle: number, rate: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  tick_playback_in_session_paged(handle: number, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  register_ownship_source_in_session_paged(handle: number, registrationJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  update_ownship_source_status_in_session_paged(handle: number, updateJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  push_situation_sample_in_session_paged(handle: number, sampleJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  select_ownship_source_in_session_paged(handle: number, selectionJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_ownship_text_action_in_session(handle: number, actionId: string, value: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  apply_situation_control_input_in_session(handle: number, inputJson: string, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  set_map_layer_visibility_in_session_paged(handle: number, layerIdJson: string, visible: boolean): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  set_map_layer_enabled_in_session_paged(handle: number, layerIdJson: string, enabled: boolean): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_settings_action_in_session(handle: number, actionJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_aircraft_library_action_in_session(handle: number, actionId: string, sourceJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_cloud_ui_action_in_session(handle: number, actionIdJson: string, fieldsJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  record_offline_package_preferences_in_session(handle: number, preferencesJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  take_cloud_provider_request_in_session(handle: number, nowEpochMs: bigint): Promise<string> | string;
  complete_cloud_provider_request_in_session(handle: number, requestId: bigint, responseJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  cloud_event_stream_plan_in_session(handle: number): Promise<string> | string;
  report_cloud_event_stream_event_in_session(handle: number, eventJson: string, nowEpochMs: bigint): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  accept_disclaimer_in_session(handle: number, agreementId: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  load_raster_map_catalog_in_session(handle: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  sync_guidance_geometry_in_session(handle: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  project_flight_plan_route_in_session(handle: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  select_map_family_in_session(handle: number, familyIdJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  select_raster_map_in_session(handle: number, selectedMapIdJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  map_selection_action_decision_in_session(
    sessionHandle: number,
    actionUid: string,
  ): Promise<string> | string;
  perform_map_selection_ui_action_in_session(
    sessionHandle: number,
    actionUid: string,
    nowEpochMs: bigint,
  ): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  flight_plan_row_action_decision_in_session(
    sessionHandle: number,
    rowUid: string,
    actionUid: string,
  ): Promise<string> | string;
  perform_flight_plan_command_in_session(
    sessionHandle: number,
    commandJson: string,
    nowEpochMs: bigint,
  ): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_time_display_action_in_session(
    sessionHandle: number,
    actionId: string,
  ): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  perform_flight_plan_column_action_in_session(
    sessionHandle: number,
    actionId: string,
  ): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  query_flight_plan_in_session(sessionHandle: number, queryJson: string): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  status_action_decision_in_session(sessionHandle: number, actionId: string): Promise<string> | string;
  perform_status_action_in_session(sessionHandle: number, actionId: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  open_chart_airport_in_session(handle: number, airportIdJson: string, chartIdJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  select_chart_reference_in_session(handle: number, familyIdJson: string, suggestedChartIdsJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_ref_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_features_in_session(handle: number, featuresJson: string): Promise<void> | void;
  ingest_airspace_label_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_prepared_live_feed_resource_in_session(handle: number, resourceId: string, preparedResourceBytes: Uint8Array): Promise<void> | void;
  ingest_resource_in_session(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  report_session_resource_failure_in_session(handle: number, resourceId: string, message: string): Promise<string> | string;
  report_session_resource_failure_in_session_at_epoch_ms(handle: number, resourceId: string, message: string, nowEpochMs: number): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  get_map_selection_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, clickJson: string, nowEpochMs: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  get_map_selection_distance_in_session(handle: number, targetJson: string): Promise<string> | string;
  get_map_selection_for_nav_ref_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, navRefJson: string, nowEpochMs: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  get_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<string> | string;
  get_scheduled_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, decodedCacheKeysJson: string, inFlightCacheKeysJson: string, nowEpochMs: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  get_nexrad_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, nowEpochMs: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  get_raster_tile_plan_in_session_with_display_scale(handle: number, viewportJson: string, widthPx: number, heightPx: number, devicePixelRatio: number, nowEpochMs: number): Promise<string> | string;
  render_terrain_overlay_tile_by_key_in_session(handle: number, terrainTileKey: string, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  get_session_snapshot_paged(handle: number): Promise<SessionSnapshotOperationJson> | SessionSnapshotOperationJson;
  get_session_snapshot_at_epoch_ms_paged(handle: number, nowEpochMs: bigint): Promise<SessionSnapshotOperationJson> | SessionSnapshotOperationJson;
  create_session_snapshot_refresh_scheduler(): Promise<number> | number;
  destroy_session_snapshot_refresh_scheduler(handle: number): Promise<void> | void;
  session_snapshot_refresh_scheduler_request(handle: number, priorityJson: string, reason: string): Promise<string> | string;
  session_snapshot_refresh_scheduler_viewport_gesture_active_changed(handle: number, active: boolean): Promise<string> | string;
  session_snapshot_refresh_scheduler_viewport_activity(handle: number): Promise<string> | string;
  session_snapshot_refresh_scheduler_refresh_completed(handle: number): Promise<string> | string;
  session_snapshot_refresh_scheduler_poll(handle: number): Promise<string> | string;
  create_ui_session_work_scheduler(): Promise<number> | number;
  destroy_ui_session_work_scheduler(handle: number): Promise<void> | void;
  ui_session_work_scheduler_request(handle: number, requestJson: string): Promise<string> | string;
  ui_session_work_scheduler_complete(handle: number, requestId: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, plateTargetAirportIdJson: string, selectedAirportIdJson: string, selectedReferenceFamilyIdJson: string, selectedChartIdJson: string, suggestedChartIdsJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
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
  sync_live_feeds_in_session(handle: number): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  configure_live_feed_source_in_session(handle: number, sourceRootUrl: string): Promise<void> | void;
  live_feed_events_url(sourceRootUrl: string): Promise<string> | string;
  live_feed_status_url(sourceRootUrl: string): Promise<string> | string;
  live_feed_runtime_decision_in_session(handle: number, inputJson: string): Promise<string> | string;
  ingest_live_feed_sse_event_in_session(handle: number, eventJson: string): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  ingest_live_feed_sse_events_in_session(handle: number, eventsJson: string): Promise<SessionResultOperationJson> | SessionResultOperationJson;
  report_live_feed_connection_event_in_session(handle: number, eventJson: string): Promise<SessionMutationOperationJson> | SessionMutationOperationJson;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(
    private readonly module: WasmModule,
    private readonly clockEpochMs: () => number = Date.now,
  ) {}

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
    sessionNowEpochMs?: number,
  ): Promise<UiSession> {
    const module = this.module;
    let invalidationListener: UiInvalidationListener | null = null;
    let projectionListener: UiSessionProjectionListener | null = null;
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
    const sessionOperationOptionsForHandle = (
      sessionHandle: number,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => ({
      ingestSessionResource: ingestSessionResource ?? ((resourceId, resourceBytes) =>
        ingestResourceForHandle(sessionHandle, resourceId, resourceBytes)),
      onInvalidations: publishInvalidations,
      reportResourceFailure: async (resourceId: string, message: string) => {
        await reportSessionResourceFailure?.(resourceId, message);
      },
      drainSessionEffects: () => this.module.drain_session_resource_effects(sessionHandle),
      operationLabel,
    });
    const runSessionResultForHandle = <T>(
      sessionHandle: number,
      operation: SessionResultOperation,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => runCoreHadSessionResultOperation<T>(
      sessionHandle,
      operation,
      sessionOperationOptionsForHandle(sessionHandle, ingestSessionResource, operationLabel),
    );
    const runSessionSnapshotForHandle = <T>(
      sessionHandle: number,
      operation: SessionSnapshotOperation,
    ) => runCoreHadSessionSnapshotOperation<T>(
      sessionHandle,
      operation,
      sessionOperationOptionsForHandle(sessionHandle),
    );
    const runSessionMutationForHandle = <TUpdate, TSnapshot>(
      sessionHandle: number,
      operation: SessionMutationOperation,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => runCoreHadSessionMutationOperation<TUpdate, TSnapshot>(
      sessionHandle,
      operation,
      {
        ...sessionOperationOptionsForHandle(sessionHandle, ingestSessionResource, operationLabel),
        resumeSnapshot: () => this.module.get_session_snapshot_paged(sessionHandle),
      },
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
      const nowEpochMs = sessionNowEpochMs ?? this.clockEpochMs();
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
      try {
        assertUiContractVersion(created.snapshot);
        const snapshotAccumulator = new SessionUpdateAccumulator(
          created.snapshot,
          UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
        );
        const loadFullSnapshot = () => runSessionSnapshotForHandle<unknown>(created.handle, () =>
          this.module.get_session_snapshot_paged(created.handle));
        const applyResourceFreeBootstrapMutation = async (
          responseJson: SessionMutationOperationJson | Promise<SessionMutationOperationJson>,
          operationLabel: string,
        ) => {
          const update = await completeResourceFreeSessionMutation<unknown>(responseJson, operationLabel);
          const disposition = snapshotAccumulator.apply(update);
          if (disposition === "resync_required") {
            throw new Error(`${operationLabel} produced a bootstrap session revision gap`);
          }
        };
        const applyPagedBootstrapMutation = async (operation: SessionMutationOperation) => {
          const completion = await runSessionMutationForHandle<unknown, unknown>(
            created.handle,
            operation,
          );
          if (completion.kind === "session_snapshot") {
            snapshotAccumulator.replaceFullSnapshot(completion.snapshot);
          } else {
            await snapshotAccumulator.applyOrResync(completion.update, loadFullSnapshot);
          }
        };
        await debugTiming("startup.session.reset_live_feed_prep", () => resetLiveFeedPrep());
        await debugTiming("startup.session.set_resource_policy", async () =>
          applyResourceFreeBootstrapMutation(module.set_resource_policy_in_session(
            created.handle,
            JSON.stringify("public_unpacked"),
          ), "startup.session.set_resource_policy"),
        );
        await debugTiming("startup.session.configure_platform", async () =>
          applyResourceFreeBootstrapMutation(module.configure_platform_capabilities_in_session(
            created.handle,
            JSON.stringify({
              display_policy: null,
              offline_packages: null,
              cloud: {
                qr_scan: false,
                aerobag_cloud_base_url: new URL(
                  __AEROBAG_CLOUD_SERVER_BASE_URL__?.trim() || "/cloud/",
                  globalThis.location.href,
                ).toString(),
              },
              live_feeds: { acquisition_policy: "jit_public_resources" },
              client_build: __AEROBAG_CLIENT_BUILD_INFO__,
              local_time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
            }),
          ), "startup.session.configure_platform"),
        );
        const origin = globalThis.location?.origin?.replace(/\/+$/, "") ?? "";
        const liveFeedRoot = liveFeedSourceUrl().replace(/\/+$/, "");
        await debugTiming("startup.session.configure_data_sources", async () =>
          applyResourceFreeBootstrapMutation(module.configure_data_sources_in_session(
            created.handle,
            packageSourceBaseUrl(),
            liveFeedRoot ? `${liveFeedRoot}/live-feeds` : "/live-feeds",
            origin ? `${origin}${DebugLogDeveloperServerPath}` : DebugLogDeveloperServerPath,
          ), "startup.session.configure_data_sources"),
        );
        await debugTiming("startup.session.attach_nav_kv", () => attachNavKvStoreToSession(created.handle));
        await debugTiming("startup.session.load_raster_catalog", () =>
          applyPagedBootstrapMutation(() => module.load_raster_map_catalog_in_session(created.handle)));
        return {
          ...created,
          snapshot: assertUiContractVersion(snapshotAccumulator.snapshot as UiSessionSnapshot),
          snapshotAccumulator,
        };
      } catch (error) {
        module.destroy_session(created.handle);
        throw error;
      }
    };
    const init = await createSession(recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    const snapshotAccumulator = init.snapshotAccumulator;
    let snapshot = init.snapshot;
    let snapshotRefreshSchedulerHandle: number;
    try {
      snapshotRefreshSchedulerHandle = await debugTiming("startup.session.create_snapshot_scheduler", () =>
        this.module.create_session_snapshot_refresh_scheduler(),
      );
    } catch (error) {
      this.module.destroy_session(handle);
      throw error;
    }
    let destroyPromise: Promise<void> | null = null;
    const runSessionResult = <T>(
      operation: SessionResultOperation,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => runSessionResultForHandle<T>(
      handle,
      operation,
      ingestSessionResource,
      operationLabel,
    );
    const runSessionSnapshot = <T>(operation: SessionSnapshotOperation) =>
      runSessionSnapshotForHandle<T>(handle, operation);
    const decodeAccumulatedSnapshot = () =>
      assertUiContractVersion(snapshotAccumulator.snapshot as UiSessionSnapshot);
    const installFullSnapshot = (value: unknown) => {
      snapshotAccumulator.replaceFullSnapshot(value);
      snapshot = decodeAccumulatedSnapshot();
      const landing = { kind: "full_snapshot", value: snapshot } as const;
      projectionListener?.({
        landing,
        snapshot,
        changedGroups: UI_SESSION_UPDATE_GROUPS,
        fullSnapshot: true,
      });
      return snapshot;
    };
    const applySessionUpdate = async (value: unknown) => {
      const measureLanding = isDebugLogEnabled();
      const landingStartedAt = measureLanding ? performance.now() : 0;
      const updateJson = measureLanding ? JSON.stringify(value) : null;
      const changedGroups = measureLanding ? sessionUpdateGroupNames(value) : [];
      const disposition = await snapshotAccumulator.applyOrResync(
        value,
        async () => runSessionSnapshot<unknown>(() =>
          this.module.get_session_snapshot_paged(handle)),
      );
      if (disposition === "resync_required") {
        debugLog("session.update.revision_gap_resync", {
          session_revision: snapshotAccumulator.snapshot.session_revision,
        });
      }
      snapshot = decodeAccumulatedSnapshot();
      if (disposition === "resync_required") {
        const landing = { kind: "full_snapshot", value: snapshot } as const;
        projectionListener?.({
          landing,
          snapshot,
          changedGroups: UI_SESSION_UPDATE_GROUPS,
          fullSnapshot: true,
        });
      } else if (disposition === "applied") {
        const landing = { kind: "update", value } as const;
        projectionListener?.({
          landing,
          snapshot,
          changedGroups: sessionUpdateGroupNames(value),
          fullSnapshot: false,
        });
      }
      if (measureLanding) {
        debugLog("session.update.landed", {
          disposition,
          groups: changedGroups,
          update_json_bytes: utf8ByteLength(updateJson ?? ""),
          accumulated_snapshot_json_bytes: utf8ByteLength(JSON.stringify(snapshot)),
          landing_ms: performance.now() - landingStartedAt,
        });
      }
      return snapshot;
    };
    const applyOptionalSessionUpdate = (update: unknown) =>
      update === null || update === undefined ? Promise.resolve(snapshot) : applySessionUpdate(update);
    const runSessionMutation = async (
      operation: SessionMutationOperation,
      ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
      operationLabel?: string,
    ) => {
      const completion = await runSessionMutationForHandle<unknown, unknown>(
        handle,
        operation,
        ingestSessionResource,
        operationLabel,
      );
      return completion.kind === "session_snapshot"
        ? installFullSnapshot(completion.snapshot)
        : applySessionUpdate(completion.update);
    };
    const parseSessionSnapshotRefreshDecision = async (json: Promise<string> | string) =>
      JSON.parse(await json) as SessionSnapshotRefreshDecision;
    let liveFeedSubscriptionOwner: SerializedSubscriptionOwner<LiveFeedSubscription> | null = null;
    let liveFeedResourceRetryTimer: number | null = null;
    let liveFeedResourceRetryDueMs: number | null = null;
    let configuredLiveFeedSourceUrl: string | null = null;
    const scheduleLiveFeedResourceRetry = (decision: LiveFeedRuntimeDecision) => {
      const retry = decision.commands.find((command) => command.kind === "retry_resources");
      if (!retry) {
        return;
      }
      const dueMs = this.clockEpochMs() + retry.delay_ms;
      if (liveFeedResourceRetryDueMs !== null && liveFeedResourceRetryDueMs <= dueMs) {
        return;
      }
      if (liveFeedResourceRetryTimer !== null) {
        globalThis.clearTimeout(liveFeedResourceRetryTimer);
      }
      liveFeedResourceRetryDueMs = dueMs;
      liveFeedResourceRetryTimer = globalThis.setTimeout(() => {
        liveFeedResourceRetryTimer = null;
        liveFeedResourceRetryDueMs = null;
        void syncLiveFeeds().catch((error: unknown) => {
          debugLog("live_feeds.resource_retry.failed", {
            message: error instanceof Error ? error.message : String(error),
          });
        });
      }, retry.delay_ms) as unknown as number;
    };
    reportSessionResourceFailure = async (resourceId, message) => {
      snapshot = await runSessionMutation(
        () => this.module.report_session_resource_failure_in_session_at_epoch_ms(
          handle,
          resourceId,
          message,
          this.clockEpochMs(),
        ),
      );
      debugLog("core.ui.invalidations.source", {
        source: "resource_failure",
        resource_id: resourceId,
        message,
        invalidations: ["session_snapshot"],
      });
      publishInvalidations(["session_snapshot"]);
      if (resourceId.startsWith("live_feeds/")) {
        await handleLiveFeedRuntimeEvent({ kind: "start" });
      }
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
      await runSessionResult<unknown>(
        () => this.module.sync_live_feeds_in_session(handle),
        (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
        "live_feeds.sync",
      );
    };
    const handleLiveFeedRuntimeEvent = async (input: LiveFeedRuntimeInput): Promise<LiveFeedRuntimeDecision> => {
      const sourceUrl = await configureLiveFeedSource();
      const statusUrl = await this.module.live_feed_status_url(sourceUrl);
      const decision = JSON.parse(await this.module.live_feed_runtime_decision_in_session(handle, JSON.stringify({
        source_url: sourceUrl,
        status_url: statusUrl,
        now_ms: this.clockEpochMs(),
        ...input,
      }))) as LiveFeedRuntimeDecision;
      if (decision.connection_event) {
        snapshot = await runSessionMutation(
          () => this.module.report_live_feed_connection_event_in_session(
            handle,
            JSON.stringify(decision.connection_event),
          ),
        );
        publishInvalidations(["session_snapshot"]);
      }
      scheduleLiveFeedResourceRetry(decision);
      return decision;
    };
    liveFeedSubscriptionOwner = new SerializedSubscriptionOwner(
      async () => {
        await handleLiveFeedRuntimeEvent({ kind: "start" });
        return createLiveFeedSubscription(
          () => this.module.live_feed_events_url(liveFeedSourceUrl()),
          handleLiveFeedRuntimeEvent,
          async (events) => {
            await runSessionResult<unknown>(
              () => this.module.ingest_live_feed_sse_events_in_session(handle, JSON.stringify(events)),
              (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
              "live_feeds.sse_ingest",
            );
          },
          (tag, data) => debugLog(tag, data),
        );
      },
      async () => {
        await handleLiveFeedRuntimeEvent({ kind: "closed" });
      },
    );
    const syncGuidanceGeometry = async (reason = "unspecified") => {
      snapshot = await debugTiming("plan.guidance.sync", () =>
        runSessionMutation(() =>
          this.module.sync_guidance_geometry_in_session(handle),
        ),
        { reason });
      return snapshot;
    };
    const runFlightPlanMutation = async (operation: SessionMutationOperation) => {
      snapshot = await runSessionMutation(operation);
      return snapshot;
    };
    const performFlightPlanCommand = (command: Record<string, unknown>) =>
      runFlightPlanMutation(
        () =>
          this.module.perform_flight_plan_command_in_session(
            handle,
            JSON.stringify(command),
            BigInt(this.clockEpochMs()),
          ),
      );
    const queryFlightPlan = <T,>(query: Record<string, unknown>) =>
      runSessionResult<T>(
        () => this.module.query_flight_plan_in_session(handle, JSON.stringify(query)),
      );
    let uiSessionWorkRunner: WebUiSessionWorkRunner;
    try {
      await debugTiming("startup.session.sync_guidance_geometry.initial", () => syncGuidanceGeometry());
      uiSessionWorkRunner = await debugTiming(
        "startup.session.create_work_scheduler",
        () => WebUiSessionWorkRunner.create({
          create: () => this.module.create_ui_session_work_scheduler(),
          request: (schedulerHandle, requestJson) =>
            this.module.ui_session_work_scheduler_request(schedulerHandle, requestJson),
          complete: (schedulerHandle, requestId) =>
            this.module.ui_session_work_scheduler_complete(schedulerHandle, requestId),
          destroy: (schedulerHandle) =>
            this.module.destroy_ui_session_work_scheduler(schedulerHandle),
        }),
      );
    } catch (error) {
      try {
        await this.module.destroy_session_snapshot_refresh_scheduler(snapshotRefreshSchedulerHandle);
      } finally {
        this.module.destroy_session(handle);
      }
      throw error;
    }
    return {
      setInvalidationListener: (listener) => {
        invalidationListener = listener;
      },
      setProjectionListener: (listener) => {
        projectionListener = listener;
      },
      initialSnapshot: () => snapshot,
      snapshot: async () => {
        const fullSnapshot = await runSessionSnapshot<unknown>(() =>
          this.module.get_session_snapshot_at_epoch_ms_paged(handle, BigInt(this.clockEpochMs())));
        return installFullSnapshot(fullSnapshot);
      },
      maintainNavDb: async (nowEpochMs) => {
        const maintenance = await runSessionResult<{
          action: "none" | "attempt_advance";
          session_update?: unknown;
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
        await applyOptionalSessionUpdate(maintenance.session_update);
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
            nowEpochMs,
          );
          await applyOptionalSessionUpdate(advanced.session_update);
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
      airportInfo: async (airportId, nowEpochMs = this.clockEpochMs()) => {
        return queryFlightPlan<AirportInfoUiView>({
          kind: "airport_info",
          airport_id: airportId,
          now_epoch_ms: Math.trunc(nowEpochMs),
        });
      },
      mapSelectionActionDecision: async (actionUid) =>
        JSON.parse(
          await this.module.map_selection_action_decision_in_session(handle, actionUid),
        ) as MapSelectionActionDecision,
      performMapSelectionUiAction: async (actionUid) => {
        return runFlightPlanMutation(
          () => this.module.perform_map_selection_ui_action_in_session(
            handle,
            actionUid,
            BigInt(this.clockEpochMs()),
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
        return queryFlightPlan<ProcedureLoadMenu>({
          kind: "describe_plate_procedure_loads",
          plate_id: plateId,
        });
      },
      loadPlateProcedure: async (loadId) => {
        return performFlightPlanCommand({ kind: "load_plate_procedure", load_id: loadId });
      },
      performFlightPlanControl: async (controlId) => {
        return performFlightPlanCommand({
          kind: "perform_control",
          control_id: controlId,
        });
      },
      flightPlanRowActionDecision: async (rowUid, actionUid) =>
        JSON.parse(
          await this.module.flight_plan_row_action_decision_in_session(
            handle,
            rowUid,
            actionUid,
          ),
        ) as FlightPlanRowActionDecision,
      performFlightPlanRowAction: async (rowUid, actionUid) => {
        return performFlightPlanCommand({
          kind: "perform_row_action",
          row_uid: rowUid,
          action_uid: actionUid,
        });
      },
      altitudeComparisons: async () => {
        return queryFlightPlan<AltitudeComparisonPanelUiView>({ kind: "altitude_comparisons" });
      },
      performAltitudePlannerAction: async (actionUid) => {
        return performFlightPlanCommand({
          kind: "perform_altitude_planner_action",
          action_uid: actionUid,
        });
      },
      setAltitudePlannerDepartureInput: async (field, input) => {
        return performFlightPlanCommand({
          kind: "set_altitude_planner_departure_input",
          field,
          input,
        });
      },
      performTimeDisplayAction: async (actionId) => {
        return runSessionMutation(() =>
          this.module.perform_time_display_action_in_session(handle, actionId),
        );
      },
      performFlightPlanColumnAction: async (actionId) => {
        return runSessionMutation(() =>
          this.module.perform_flight_plan_column_action_in_session(handle, actionId),
        );
      },
      statusActionDecision: async (actionId) =>
        JSON.parse(await this.module.status_action_decision_in_session(handle, actionId)) as UiStatusActionDecision,
      performStatusAction: async (actionId) => {
        snapshot = await runSessionMutation(() =>
          this.module.perform_status_action_in_session(handle, actionId),
        );
        return snapshot;
      },
      setSituation: async (situation) => {
        snapshot = await runSessionMutation(() =>
          this.module.set_situation_in_session_paged(handle, JSON.stringify(situation)),
        );
        return snapshot;
      },
      tickBadAutopilot: async (nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.tick_bad_autopilot_in_session_paged(handle, nowEpochMs),
        );
        return syncGuidanceGeometry("tick_bad_autopilot");
      },
      registerOwnshipSource: async (registration) => {
        snapshot = await runSessionMutation(
          () => this.module.register_ownship_source_in_session_paged(handle, JSON.stringify(registration)),
        );
        return snapshot;
      },
      updateOwnshipSourceStatus: async (update) => {
        snapshot = await runSessionMutation(
          () => this.module.update_ownship_source_status_in_session_paged(handle, JSON.stringify(update)),
        );
        return snapshot;
      },
      pushSituationSample: async (sample) => {
        snapshot = await runSessionMutation(
          () => this.module.push_situation_sample_in_session_paged(handle, JSON.stringify(sample)),
        );
        return snapshot;
      },
      selectOwnshipSource: async (selection) => {
        snapshot = await runSessionMutation(
          () => this.module.select_ownship_source_in_session_paged(handle, JSON.stringify(ownshipSelectionToCore(selection))),
        );
        return snapshot;
      },
      performOwnshipTextAction: async (actionId, value, nowEpochMs) => {
        snapshot = await runSessionMutation(
          () => this.module.perform_ownship_text_action_in_session(
            handle,
            actionId,
            value,
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      applySituationControlInput: async (input, nowEpochMs) => {
        snapshot = await runSessionMutation(
          () => this.module.apply_situation_control_input_in_session(
            handle,
            JSON.stringify(input),
            nowEpochMs,
          ),
        );
        return snapshot;
      },
      setMapLayerVisibility: async (layerId, visible) => {
        snapshot = await runSessionMutation(
          () => this.module.set_map_layer_visibility_in_session_paged(handle, JSON.stringify(layerId), visible),
        );
        return snapshot;
      },
      setMapLayerEnabled: async (layerId, enabled) => {
        snapshot = await runSessionMutation(
          () => this.module.set_map_layer_enabled_in_session_paged(handle, JSON.stringify(layerId), enabled),
        );
        return snapshot;
      },
      performSettingsAction: async (actionId, valueId) => {
        snapshot = await runSessionMutation(() =>
          this.module.perform_settings_action_in_session(
            handle,
            JSON.stringify({ action_id: actionId, value_id: valueId }),
            BigInt(this.clockEpochMs()),
          ),
        );
        return snapshot;
      },
      performAircraftLibraryAction: async (actionId, sourceJson = "") => {
        snapshot = await runSessionMutation(() =>
          this.module.perform_aircraft_library_action_in_session(
            handle,
            actionId,
            sourceJson,
            BigInt(Date.now()),
          ),
        );
        return snapshot;
      },
      performCloudUiAction: async (actionId, fields, nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.perform_cloud_ui_action_in_session(
            handle,
            JSON.stringify(actionId),
            JSON.stringify(fields),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      recordOfflinePackagePreferences: async (preferencesJson, nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.record_offline_package_preferences_in_session(
            handle,
            preferencesJson,
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
        snapshot = await runSessionMutation(() =>
          this.module.complete_cloud_provider_request_in_session(
            handle,
            BigInt(requestId),
            JSON.stringify(response),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      cloudEventStreamPlan: async () => {
        return JSON.parse(
          await this.module.cloud_event_stream_plan_in_session(handle),
        ) as CloudEventStreamPlan | null;
      },
      reportCloudEventStreamEvent: async (event, nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.report_cloud_event_stream_event_in_session(
            handle,
            JSON.stringify(event),
            BigInt(Math.trunc(nowEpochMs)),
          ),
        );
        return snapshot;
      },
      acceptDisclaimer: async (agreementId) => {
        snapshot = await runSessionMutation(() =>
          this.module.accept_disclaimer_in_session(handle, agreementId),
        );
        return snapshot;
      },
      loadRasterMapCatalog: async () => {
        snapshot = await runSessionMutation(() =>
          this.module.load_raster_map_catalog_in_session(handle),
        );
        return snapshot;
      },
      resolveChartAssetUrl: (chartId, assetKind) =>
        uiSessionWorkRunner.run(
          "chart_asset",
          `chart_asset:${assetKind}:${chartId}`,
          () => resolveChartAssetUrl(handle, chartId, assetKind),
        ),
      selectMapFamily: async (familyId) => {
        snapshot = await runSessionMutation(() =>
          this.module.select_map_family_in_session(handle, JSON.stringify(familyId)),
        );
        return snapshot;
      },
      selectRasterMap: async (selectedMapId) => {
        snapshot = await runSessionMutation(() =>
          this.module.select_raster_map_in_session(handle, JSON.stringify(selectedMapId)),
        );
        return snapshot;
      },
      engageMapFollow: async (viewport) => {
        snapshot = await runSessionMutation(() =>
          this.module.engage_map_follow_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
          ),
        );
        return snapshot;
      },
      disengageMapFollow: async (viewport) => {
        snapshot = await runSessionMutation(() =>
          this.module.disengage_map_follow_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
          ),
        );
        return snapshot;
      },
      setMapFollowOffset: async (viewport, offsetXPx, offsetYPx) => {
        snapshot = await runSessionMutation(() =>
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
        snapshot = await runSessionMutation(() =>
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
        snapshot = await runSessionMutation(() =>
          this.module.load_playback_trace_in_session_paged(handle, JSON.stringify(sourcePath), traceJson),
        );
        return snapshot;
      },
      playPlayback: async (nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.play_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      pausePlayback: async (nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.pause_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      seekPlayback: async (cursorSeconds, nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.seek_playback_in_session_paged(handle, cursorSeconds, nowEpochMs),
        );
        return snapshot;
      },
      setPlaybackRate: async (rate, nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.set_playback_rate_in_session_paged(handle, rate, nowEpochMs),
        );
        return snapshot;
      },
      tickPlayback: async (nowEpochMs) => {
        snapshot = await runSessionMutation(() =>
          this.module.tick_playback_in_session_paged(handle, nowEpochMs),
        );
        return snapshot;
      },
      selectAirport: async (airportId) => {
        snapshot = await runSessionMutation(() =>
          this.module.select_airport_in_session(handle, JSON.stringify(airportId)),
        );
        return snapshot;
      },
      openChartAirport: async (airportId, chartId) => {
        snapshot = await runSessionMutation(() =>
          this.module.open_chart_airport_in_session(
            handle,
            JSON.stringify(airportId),
            JSON.stringify(chartId ?? null),
          ),
        );
        return snapshot;
      },
      selectChart: async (chartId) => {
        snapshot = await runSessionMutation(() =>
          this.module.select_chart_in_session(handle, JSON.stringify(chartId)),
        );
        return snapshot;
      },
      selectChartReference: async (familyId, suggestedChartIds) => {
        snapshot = await runSessionMutation(() =>
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
      queryMapOverlay: (viewport, widthPx, heightPx) =>
        uiSessionWorkRunner.run("map_overlay", "map_overlay", () =>
          runSessionResult<MapOverlayQueryResult>(
            () =>
              this.module.get_map_overlay_in_session(
                handle,
                JSON.stringify(coreViewportForMap(viewport)),
                widthPx,
                heightPx,
                this.clockEpochMs(),
              ),
            (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
            "map.overlay",
          ),
        ),
      queryMapSelection: (viewport, widthPx, heightPx, click) =>
        uiSessionWorkRunner.run("map_selection", "map_selection", () =>
          runSessionResult<MapSelectionQueryResult>(() =>
            this.module.get_map_selection_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              JSON.stringify(click),
              this.clockEpochMs(),
            ),
          ),
        ),
      queryMapSelectionDistance: async (target) =>
        JSON.parse(
          await this.module.get_map_selection_distance_in_session(
            handle,
            JSON.stringify(target),
          ),
        ) as string | null,
      queryMapSelectionForNavRef: (viewport, widthPx, heightPx, navRef) =>
        uiSessionWorkRunner.run("map_selection_for_nav_ref", "map_selection_for_nav_ref", () =>
          runSessionResult<MapSelectionForNavRefResult>(() =>
            this.module.get_map_selection_for_nav_ref_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              JSON.stringify(navRef),
              this.clockEpochMs(),
            ),
          ),
        ),
      queryTerrainOverlay: (viewport, widthPx, heightPx, decodedCacheKeys, inFlightCacheKeys) =>
        uiSessionWorkRunner.run("terrain_overlay", "terrain_overlay", () =>
          runSessionResult<TerrainOverlayQueryResult>(
            () =>
              this.module.get_scheduled_terrain_overlay_in_session(
                handle,
                JSON.stringify(coreViewportForMap(viewport)),
                widthPx,
                heightPx,
                JSON.stringify(decodedCacheKeys),
                JSON.stringify(inFlightCacheKeys),
                this.clockEpochMs(),
              ),
            (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
          ),
        ),
      queryNexradOverlay: (viewport, widthPx, heightPx) =>
        uiSessionWorkRunner.run("nexrad_overlay", "nexrad_overlay", () =>
          runSessionResult<NexradOverlayQueryResult>(
            () =>
              this.module.get_nexrad_overlay_in_session(
                handle,
                JSON.stringify(coreViewportForMap(viewport)),
                widthPx,
                heightPx,
                this.clockEpochMs(),
              ),
            (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
          ),
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
          this.clockEpochMs(),
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
      renderTerrainOverlayTileByKey: (tileKey, aircraftAltitudeFt) =>
        uiSessionWorkRunner.run("terrain_tile", `terrain_tile:${tileKey}`, async () =>
          new Uint8Array(await this.module.render_terrain_overlay_tile_by_key_in_session(
            handle,
            tileKey,
            aircraftAltitudeFt,
          )),
        ),
      projectFlightPlanRoute: async () =>
        runSessionResult<FlightPlanRouteProjection>(() =>
          this.module.project_flight_plan_route_in_session(handle),
        ),
      syncLiveFeeds,
      startLiveFeedSubscription: () => liveFeedSubscriptionOwner!.start(),
      notifyLiveFeedOnline: () => {
        liveFeedSubscriptionOwner?.current()?.notifyOnline();
      },
      stopLiveFeedSubscription: () => liveFeedSubscriptionOwner!.stop(),
      ingestLiveFeedSseEvent: async (event) => {
        await runSessionResult<unknown>(
          () => this.module.ingest_live_feed_sse_event_in_session(handle, JSON.stringify(event)),
          (resourceId, resourceBytes) => ingestResourceForHandle(handle, resourceId, resourceBytes),
          "live_feeds.sse_ingest",
        );
      },
      ingestLiveFeedSseEvents: async (events) => {
        await runSessionResult<unknown>(
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
        snapshot = await runSessionMutation(() =>
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
      destroy: () => {
        if (destroyPromise) {
          return destroyPromise;
        }
        destroyPromise = (async () => {
          try {
            await liveFeedSubscriptionOwner?.stop();
          } catch (error) {
            debugLog("live_feeds.subscription_stop.failed", {
              message: error instanceof Error ? error.message : String(error),
            });
          }
          liveFeedSubscriptionOwner = null;
          if (liveFeedResourceRetryTimer !== null) {
            globalThis.clearTimeout(liveFeedResourceRetryTimer);
            liveFeedResourceRetryTimer = null;
          }
          await uiSessionWorkRunner.close();
          await this.module.destroy_session_snapshot_refresh_scheduler(snapshotRefreshSchedulerHandle);
          this.module.destroy_session(handle);
        })();
        return destroyPromise;
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
  clockEpochMs: () => number = Date.now,
): Promise<LoadedAdapter> {
  return loadBestAvailableAdapterUncached(importer, clockEpochMs);
}

async function loadBestAvailableAdapterUncached(
  importer: () => Promise<unknown>,
  clockEpochMs: () => number = Date.now,
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
    "flight_plan_row_action_decision_in_session",
    "perform_flight_plan_column_action_in_session",
    "perform_time_display_action_in_session",
    "query_flight_plan_in_session",
    "status_action_decision_in_session",
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
    "perform_ownship_text_action_in_session",
    "apply_situation_control_input_in_session",
    "set_map_layer_visibility_in_session_paged",
    "set_map_layer_enabled_in_session_paged",
    "perform_settings_action_in_session",
    "perform_aircraft_library_action_in_session",
    "perform_cloud_ui_action_in_session",
    "record_offline_package_preferences_in_session",
    "take_cloud_provider_request_in_session",
    "complete_cloud_provider_request_in_session",
    "cloud_event_stream_plan_in_session",
    "report_cloud_event_stream_event_in_session",
    "accept_disclaimer_in_session",
    "load_raster_map_catalog_in_session",
    "resolve_chart_asset_resource_in_session",
    "sync_guidance_geometry_in_session",
    "project_flight_plan_route_in_session",
    "select_map_family_in_session",
    "select_raster_map_in_session",
    "map_selection_action_decision_in_session",
    "perform_map_selection_ui_action_in_session",
    "select_airport_in_session",
    "select_chart_in_session",
    "select_chart_reference_in_session",
    "ingest_point_tiles_in_session",
    "ingest_airspace_ref_tiles_in_session",
    "ingest_airspace_features_in_session",
    "ingest_airspace_label_tiles_in_session",
    "get_map_overlay_in_session",
    "get_map_selection_in_session",
    "get_map_selection_distance_in_session",
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
    "create_ui_session_work_scheduler",
    "destroy_ui_session_work_scheduler",
    "ui_session_work_scheduler_request",
    "ui_session_work_scheduler_complete",
    "configure_data_sources_in_session",
    "open_chart_airport_in_session",
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
    adapter: new WasmAppCoreAdapter(mod as WasmModule, clockEpochMs),
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

type LiveFeedRuntimeCommand =
  | { kind: "reconnect"; delay_ms: number }
  | { kind: "retry_resources"; delay_ms: number };

type LiveFeedRuntimeDecision = {
  transport_policy: {
    heartbeat_interval_ms: number;
    connect_timeout_ms: number;
    idle_timeout_ms: number;
    reconnect_initial_delay_ms: number;
    reconnect_max_delay_ms: number;
  };
  connection_event?: {
    kind: "connecting" | "connected" | "message" | "error" | "closed" | "network_status";
    message?: string | null;
    source_url?: string | null;
    status_url?: string | null;
    network_status?: "unmetered" | "metered" | "no_active_network" | "unknown" | null;
  } | null;
  commands: LiveFeedRuntimeCommand[];
};

type CloseableSubscription = {
  close(): void;
};

export class SerializedSubscriptionOwner<T extends CloseableSubscription> {
  private desiredRunning = false;
  private subscription: T | null = null;
  private lifecycle: Promise<void> = Promise.resolve();

  constructor(
    private readonly create: () => Promise<T>,
    private readonly reportStopped: () => Promise<void>,
  ) {}

  start(): Promise<void> {
    this.desiredRunning = true;
    return this.serialize(async () => {
      if (!this.desiredRunning || this.subscription) {
        return;
      }
      const candidate = await this.create();
      if (!this.desiredRunning) {
        candidate.close();
        await this.reportStopped();
        return;
      }
      this.subscription = candidate;
    });
  }

  stop(): Promise<void> {
    this.desiredRunning = false;
    return this.serialize(async () => {
      const closing = this.subscription;
      this.subscription = null;
      if (!closing) {
        return;
      }
      closing.close();
      await this.reportStopped();
    });
  }

  current(): T | null {
    return this.subscription;
  }

  hasSubscription(): boolean {
    return this.subscription !== null;
  }

  private serialize(operation: () => Promise<void>): Promise<void> {
    const result = this.lifecycle.then(operation);
    this.lifecycle = result.catch(() => undefined);
    return result;
  }
}

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
  let connectTimer: number | null = null;
  let idleTimer: number | null = null;
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
  const clearConnectionTimers = () => {
    if (connectTimer !== null) {
      globalThis.clearTimeout(connectTimer);
      connectTimer = null;
    }
    if (idleTimer !== null) {
      globalThis.clearTimeout(idleTimer);
      idleTimer = null;
    }
  };
  const reconnectDelay = (decision: LiveFeedRuntimeDecision): number | null =>
    decision.commands.find((command) => command.kind === "reconnect")?.delay_ms ?? null;
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
      if (!closed) {
        queuedEvents.unshift(...batch);
      }
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
    clearConnectionTimers();
    void runtimeEvent({ kind: "connecting" }).then(async (decision) => {
      const transportPolicy = decision.transport_policy;
      const eventsUrl = await liveFeedEventsUrl();
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
      const handleTimeout = (kind: "error" | "idle_timeout", message: string) => {
        if (!isCurrent()) {
          return;
        }
        events = null;
        closeEventSource(nextEvents);
        clearConnectionTimers();
        void runtimeEvent({ kind, message })
          .then((timeoutDecision) => scheduleReconnect(reconnectDelay(timeoutDecision) ?? 0))
          .catch(() => scheduleReconnect(0));
      };
      const armIdleTimer = () => {
        if (idleTimer !== null) {
          globalThis.clearTimeout(idleTimer);
        }
        idleTimer = globalThis.setTimeout(
          () => handleTimeout("idle_timeout", "EventSource idle timeout"),
          transportPolicy.idle_timeout_ms,
        ) as unknown as number;
      };
      connectTimer = globalThis.setTimeout(
        () => handleTimeout("error", "EventSource connect timeout"),
        transportPolicy.connect_timeout_ms,
      ) as unknown as number;
      const queueLiveFeedEvent = (eventName: "live-feed-catalog" | "live-feed-current") => (event: Event) => {
        if (!isCurrent()) {
          return;
        }
        clearReconnectTimer();
        armIdleTimer();
        const message = event as MessageEvent<string>;
        probeState.messages += 1;
        probeState.last_ready_state = nextEvents.readyState;
        reportEvent({ kind: "message" });
        queuedEvents.push({
          id: message.lastEventId || null,
          event: eventName,
          data: message.data,
        });
        scheduleFlush();
      };
      nextEvents.addEventListener("live-feed-catalog", queueLiveFeedEvent("live-feed-catalog"));
      nextEvents.addEventListener("live-feed-current", queueLiveFeedEvent("live-feed-current"));
      nextEvents.onopen = () => {
        if (!isCurrent()) {
          return;
        }
        clearReconnectTimer();
        if (connectTimer !== null) {
          globalThis.clearTimeout(connectTimer);
          connectTimer = null;
        }
        armIdleTimer();
        log("live_feeds.sse.open", { events_url: eventsUrl });
        probeState.last_ready_state = nextEvents.readyState;
        reportEvent({ kind: "connected" });
      };
      nextEvents.onerror = () => {
        if (!isCurrent()) {
          return;
        }
        log("live_feeds.sse.error", { ready_state: nextEvents.readyState });
        clearConnectionTimers();
        probeState.errors += 1;
        probeState.last_ready_state = nextEvents.readyState;
        void runtimeEvent({ kind: "error", message: "EventSource error" })
          .then((errorDecision) => scheduleReconnect(reconnectDelay(errorDecision) ?? 0))
          .catch(() => scheduleReconnect(0));
      };
    }).catch((error: unknown) => {
      log("live_feeds.sse.url.failed", { message: error instanceof Error ? error.message : String(error) });
      void runtimeEvent({ kind: "error", message: error instanceof Error ? error.message : String(error) })
        .then((errorDecision) => scheduleReconnect(reconnectDelay(errorDecision) ?? 0))
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
      .then((decision) => scheduleReconnect(reconnectDelay(decision) ?? 0))
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
      clearConnectionTimers();
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
