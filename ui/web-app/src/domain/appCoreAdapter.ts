import type {
  UiSnapshotAppState,
  AppUiState,
  AirwayBranch,
  AirwayExitCandidate,
  AirwayPresentationPlan,
  AirwaySuggestion,
  CifpTppMatch,
  ChartPageData,
  FlightPlan,
  FlightPlanEntryPreview,
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
  ChartFamilyId,
  ContentAvailability,
  GuidanceState,
  LatLon,
  MapFollowUiState,
  NavRef,
  NavSymbolFeature,
  OwnshipSelectionCommand,
  OwnshipSourceRegistration,
  OwnshipSourceStatusUpdate,
  PlanLeg,
  PlaybackUiState,
  ProcedureLoadOption,
  ProcedureKind,
  ProcedureOptions,
  ProcedureSummary,
  ResolvedLegUiView,
  RouteComponentUiView,
  SequencingMode,
  Situation,
  SituationControlInput,
  SituationRingCandidate,
  SituationSample,
  WaypointIdentifierSuggestion,
} from "./types";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import { attachNavKvStoreToSession, PublicationResolver, runCoreHadOperation, runCoreHadSessionOperation } from "./navKv";
import { debugLog, debugTiming, installRustDebugLogBridge } from "./debugLog";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
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
    active: boolean;
  }>;
};

export type UiSessionSnapshot = {
  app_state: UiSnapshotAppState;
  app_ui_state: AppUiState;
  playback_ui_state: PlaybackUiState;
  map_follow_ui_state: MapFollowUiState;
  map_follow_target_viewport: {
    center: LatLon;
    zoom: number;
    rotation_deg: number;
    pitch_deg: number;
  } | null;
  chart_page_state: UiChartPageState;
  map_layer_state: UiMapLayerState;
  caution_state: UiCautionState;
  debug_state: UiDebugState;
  raster_map?: RasterMapUiState | null;
};

export type DebugFlagId = "tile_labels" | "fast_tiles" | "offline_simulated_clock_buttons" | "sequencing_finish_lines";

export type UiDebugState = {
  tile_labels: boolean;
  playback_visible: boolean;
  fast_tiles: boolean;
  offline_simulated_clock_buttons: boolean;
  sequencing_finish_lines: boolean;
};

export type UiCautionState = {
  obstacle_display_limited: boolean;
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
  selected_airport_id: string;
  selected_chart_id: string;
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
  ident: string;
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
  style_class: string;
  obstacle_variant?: "short" | "tall" | null;
  screen_x: number;
  screen_y: number;
  towered: boolean;
  fuel_available: boolean;
  has_paved_runway?: boolean | null;
  heliport?: boolean | null;
  has_water_runway?: boolean | null;
  runway_length_ratio: number;
  longest_runway_heading_true_deg: number | null;
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
    paths: Array<{
      closed: boolean;
      points: Array<{ x: number; y: number }>;
    }>;
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
  needed_vector_tiles: VectorTileRequest[];
  needed_metar_tiles: VectorTileRequest[];
  needed_airspace_features: AirspaceFeatureRequest[];
  needed_metars: boolean;
  needed_tfrs: boolean;
  visible_features: VisibleMapFeature[];
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
  warnings: Array<{
    code: string;
    message: string;
  }>;
};

export type MapSelectionQueryResult = {
  click_lat: number;
  click_lon: number;
  categories: MapSelectionCategory[];
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
  airspace_limit?: AirspaceLimitGlyph | null;
  session_action?: string | null;
  flight_plan_row_action?: {
    row_uid: string;
    action_uid: string;
  } | null;
};

export type TerrainOverlayStatus =
  | { state: "hidden" }
  | { state: "no_position" }
  | { state: "no_altitude" }
  | { state: "too_many_tiles"; count: number }
  | { state: "ready"; count: number };

export type TerrainOverlayTileRequest = {
  key: string;
  product_id: string;
  path: string;
  source_tiles: Array<{
    product_id: string;
    path: string;
  }>;
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
};

export type RasterTileSource = {
  map_view_id: string;
  package_name?: string | null;
  storage_kind: string;
  relative_path: string;
  resource:
    | { kind: "public_unpacked"; path: string }
    | { kind: "installed_package"; package_name: string; member_path: string };
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
};

export interface UiSession {
  snapshot(): Promise<UiSessionSnapshot>;
  replaceFlightPlan(plan: FlightPlan): Promise<UiSessionSnapshot>;
  insertWaypointAtFlightPlanRow(rowUid: string, before: boolean, waypoint: NavRef): Promise<UiSessionSnapshot>;
  suggestWaypointIdentifiersAtFlightPlanRow(rowUid: string, before: boolean, prefix: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  insertAirwayAtFlightPlanRow(rowUid: string, presentation: AirwayPresentationPlan, entryIndex: number, exitIndex: number): Promise<UiSessionSnapshot>;
  selectProcedureAtFlightPlanRow(rowUid: string, airportId: string, procedureId: string, kind: ProcedureKind, runwayTransition: string | null, enrouteTransition: string | null): Promise<UiSessionSnapshot>;
  loadPlateProcedure(loadId: string): Promise<UiSessionSnapshot>;
  restoreDirectTo(): Promise<UiSessionSnapshot>;
  performFlightPlanRowAction(rowUid: string, actionUid: string): Promise<UiSessionSnapshot>;
  performMapSelectionAction(action: string): Promise<UiSessionSnapshot>;
  activateNextLeg(): Promise<UiSessionSnapshot>;
  suspendSequencing(): Promise<UiSessionSnapshot>;
  unsuspendSequencing(): Promise<UiSessionSnapshot>;
  sequenceActiveLeg(): Promise<UiSessionSnapshot>;
  setSituation(situation: Situation): Promise<UiSessionSnapshot>;
  tickDebugOwnshipDriver(nowEpochMs: number): Promise<UiSessionSnapshot>;
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
  loadRasterMapCatalog(): Promise<UiSessionSnapshot>;
  selectMapFamily(familyId: ChartFamilyId): Promise<UiSessionSnapshot>;
  selectRasterMap(selectedMapId: string): Promise<UiSessionSnapshot>;
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  ingestAirspaceRefTiles(tiles: AirspaceReferenceTilePayload[]): Promise<void>;
  ingestAirspaceFeatures(features: AirspaceFeaturePayload[]): Promise<void>;
  ingestAirspaceLabelTiles(tiles: AirspaceLabelTilePayload[]): Promise<void>;
  queryMapOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<MapOverlayQueryResult>;
  queryMapSelection(viewport: MapViewportState, widthPx: number, heightPx: number, click: LatLon, hitRadiusPx: number): Promise<MapSelectionQueryResult>;
  queryTerrainOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<TerrainOverlayQueryResult>;
  queryRasterTilePlan(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<RasterTilePlan>;
  renderTerrainOverlayTile(tileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array>;
  renderTerrainOverlayTiles(packedTileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array>;
  restoreChartPageState(recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<UiSessionSnapshot>;
  destroy(): Promise<void>;
}

export interface AppCoreAdapter {
  prewarm(): Promise<void>;
  situationRingCandidates(): SituationRingCandidate[];
  emptyFlightPlan(): Promise<FlightPlan>;
  createUiSession(
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession>;
  deriveChartPageState(
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<DerivedChartPageState>;
  projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]>;
  previewFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanEntryPreview>;
  appendFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanUiMutation>;
  resolveWaypointIdentifier(identifier: string): Promise<NavRef | null>;
  resolveNavRefPosition(navRef: NavRef): Promise<LatLon>;
  suggestWaypointIdentifiersNear(anchor: LatLon, prefix: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  suggestAirwaysNearAnchor(anchor: NavRef, limit?: number): Promise<AirwaySuggestion[]>;
  prepareAirwayPresentationForAnchors(
    airwayName: string,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<AirwayPresentationPlan>;
  listProcedures(airportId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]>;
  describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions>;
  findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null>;
  describePlateProcedureLoads(plan: FlightPlan, plateId: string): Promise<ProcedureLoadOption[]>;
}

export type AdapterBackendKind = "wasm";

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

async function fetchVectorManifestJson(module: WasmModule): Promise<string> {
  const publicationResolver = new PublicationResolver(module, module.publication_resolver_open("/packages"));
  const baseManifest: Record<string, unknown> = {
    airspace: {
      reference_tile_min_zoom: 0,
      reference_tile_max_zoom: 12,
      label_tile_min_zoom: 0,
      label_tile_max_zoom: 12,
    },
    point_layers: {},
  };
  try {
    const obstacleResponse = await fetch(await publicationResolver.resolveObstacleManifest(), { cache: "no-cache" });
    if (obstacleResponse.ok) {
      const obstacleManifest = JSON.parse(await obstacleResponse.text()) as {
        point_layers?: Record<string, unknown>;
        files?: Record<string, unknown>;
      };
      if (obstacleManifest.point_layers?.obstacle) {
        baseManifest.point_layers = {
          ...(typeof baseManifest.point_layers === "object" && baseManifest.point_layers !== null
            ? baseManifest.point_layers as Record<string, unknown>
            : {}),
          obstacle: obstacleManifest.point_layers.obstacle,
        };
      }
      if (obstacleManifest.files?.point_tiles_obstacle || obstacleManifest.files?.stats) {
        baseManifest.files = {
          ...(typeof baseManifest.files === "object" && baseManifest.files !== null
            ? baseManifest.files as Record<string, unknown>
            : {}),
          ...(obstacleManifest.files ?? {}),
        };
      }
    }
  } catch {
    // Obstacle overlay is optional; keep the base vector manifest usable if the fast product is absent.
  }
  try {
    const metarResponse = await fetch(await publicationResolver.resolveMetarManifest(), { cache: "no-cache" });
    if (metarResponse.ok) {
      const metarManifest = JSON.parse(await metarResponse.text()) as {
        map_view?: {
          min_zoom?: number;
          max_zoom?: number;
          levels?: Array<{ zoom?: number }>;
          tile_path_template?: string;
        };
      };
      const mapView = metarManifest.map_view;
      const availableZooms = Array.from(new Set(
        mapView?.levels
          ?.map((level) => level.zoom)
          .filter((zoom): zoom is number => typeof zoom === "number" && Number.isInteger(zoom))
          ?? [],
      )).sort((a, b) => a - b);
      if (mapView && availableZooms && availableZooms.length > 0) {
        baseManifest.point_layers = {
          ...(typeof baseManifest.point_layers === "object" && baseManifest.point_layers !== null
            ? baseManifest.point_layers as Record<string, unknown>
            : {}),
          metars: {
            min_zoom: typeof mapView.min_zoom === "number" ? mapView.min_zoom : availableZooms[0],
            max_zoom: typeof mapView.max_zoom === "number" ? mapView.max_zoom : availableZooms[availableZooms.length - 1],
            available_zooms: availableZooms,
            tile_path_template: mapView.tile_path_template ?? "points/metars/{z}/{x}/{y}.json",
          },
        };
      }
      if (mapView?.tile_path_template) {
        baseManifest.files = {
          ...(typeof baseManifest.files === "object" && baseManifest.files !== null
            ? baseManifest.files as Record<string, unknown>
            : {}),
          point_tiles_metars: mapView.tile_path_template,
          metars: "metars.json",
        };
      }
    }
  } catch {
    // METAR overlay is optional; keep the base vector manifest usable if the fast product is absent.
  }
  publicationResolver.destroy();
  return JSON.stringify(baseManifest);
}

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  publication_resolver_destroy(handle: number): void;
  publication_resolver_ingest_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  publication_resolver_open(publicBaseUrl: string): number;
  publication_resolver_resolve_metar_manifest(handle: number): Promise<string> | string;
  publication_resolver_resolve_nav_kv_resource(handle: number, memberPath: string): Promise<string> | string;
  publication_resolver_resolve_obstacle_manifest(handle: number): Promise<string> | string;
  publication_resolver_resolve_package_member(handle: number, packageId: string, memberPath: string): Promise<string> | string;
  situation_ring_candidates_json(): Promise<string> | string;
  empty_flight_plan_json(): Promise<string> | string;
  create_ui_session(vectorManifestJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  create_ui_session_profiled?: (vectorManifestJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string) => Promise<string> | string;
  set_raster_resource_mode_in_session(handle: number, modeJson: string): Promise<string> | string;
  set_situation_in_session(handle: number, situationJson: string): Promise<string> | string;
  tick_debug_ownship_driver_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  engage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  disengage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  set_map_follow_offset_in_session(handle: number, viewportJson: string, offsetXPx: number, offsetYPx: number): Promise<string> | string;
  sync_map_follow_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  load_playback_trace_in_session(handle: number, sourcePathJson: string, traceJson: string): Promise<string> | string;
  play_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  pause_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  seek_playback_in_session(handle: number, cursorSeconds: number, nowEpochMs: number): Promise<string> | string;
  set_playback_rate_in_session(handle: number, rate: number, nowEpochMs: number): Promise<string> | string;
  tick_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  register_ownship_source_in_session(handle: number, registrationJson: string): Promise<string> | string;
  update_ownship_source_status_in_session(handle: number, updateJson: string): Promise<string> | string;
  push_situation_sample_in_session(handle: number, sampleJson: string): Promise<string> | string;
  select_ownship_source_in_session(handle: number, selectionJson: string): Promise<string> | string;
  apply_situation_control_input_in_session(handle: number, inputJson: string, nowEpochMs: number): Promise<string> | string;
  set_map_layer_visibility_in_session(handle: number, layerIdJson: string, visible: boolean): Promise<string> | string;
  set_map_layer_enabled_in_session(handle: number, layerIdJson: string, enabled: boolean): Promise<string> | string;
  set_debug_flag_in_session(handle: number, flagIdJson: string, enabled: boolean): Promise<string> | string;
  load_raster_map_catalog_in_session(handle: number): Promise<string> | string;
  sync_guidance_geometry_in_session(handle: number): Promise<string> | string;
  select_map_family_in_session(handle: number, familyIdJson: string): Promise<string> | string;
  select_raster_map_in_session(handle: number, selectedMapIdJson: string): Promise<string> | string;
  replace_flight_plan_in_session(handle: number, planJson: string): Promise<string> | string;
  perform_map_selection_action_in_session(sessionHandle: number, actionJson: string): Promise<string> | string;
  insert_waypoint_at_flight_plan_row_in_session(sessionHandle: number, rowUid: string, before: boolean, waypointJson: string): Promise<string> | string;
  suggest_waypoint_identifiers_at_flight_plan_row_in_session(sessionHandle: number, rowUid: string, before: boolean, prefix: string, limit: number): Promise<string> | string;
  insert_airway_at_flight_plan_row_in_session(sessionHandle: number, rowUid: string, presentationJson: string, entryIndex: number, exitIndex: number): Promise<string> | string;
  select_procedure_at_flight_plan_row_in_session(
    sessionHandle: number,
    rowUid: string,
    airportId: string,
    procedureId: string,
    procedureKindJson: string,
    runwayTransitionJson: string,
    enrouteTransitionJson: string,
  ): Promise<string> | string;
  load_plate_procedure_in_session(sessionHandle: number, loadId: string): Promise<string> | string;
  restore_direct_to_in_session(sessionHandle: number): Promise<string> | string;
  perform_flight_plan_row_action_in_session(sessionHandle: number, rowUid: string, actionUid: string): Promise<string> | string;
  activate_next_leg_in_session(sessionHandle: number): Promise<string> | string;
  suspend_sequencing_in_session(sessionHandle: number): Promise<string> | string;
  unsuspend_sequencing_in_session(sessionHandle: number): Promise<string> | string;
  sequence_active_leg_in_session(sessionHandle: number): Promise<string> | string;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<string> | string;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<string> | string;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_ref_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_features_in_session(handle: number, featuresJson: string): Promise<void> | void;
  ingest_airspace_label_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_resource_in_session(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_map_selection_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, clickJson: string, hitRadiusPx: number): Promise<string> | string;
  get_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_raster_tile_plan_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  render_terrain_overlay_tile_in_session(handle: number, terrainTileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  render_terrain_overlay_tiles_in_session(handle: number, packedTerrainTileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  get_session_snapshot(handle: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  destroy_session(handle: number): void;
  install_rust_debug_logger(): Promise<void> | void;
  nav_kv_open(rootBytes: Uint8Array): Promise<number> | number;
  nav_kv_insert_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  nav_kv_prefetch_pages(handle: number): Promise<string> | string;
  nav_kv_destroy(handle: number): Promise<void> | void;
  attach_nav_kv_store_to_session(navKvHandle: number, sessionHandle: number): Promise<void> | void;
  core_had_operation(handle: number, operationJson: string): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  private vectorManifestJsonPromise: Promise<string> | null = null;

  constructor(private readonly module: WasmModule) {}

  private async vectorManifestJson(): Promise<string> {
    this.vectorManifestJsonPromise ??= fetchVectorManifestJson(this.module);
    return this.vectorManifestJsonPromise;
  }

  async prewarm(): Promise<void> {}

  situationRingCandidates(): SituationRingCandidate[] {
    const candidatesJson = this.module.situation_ring_candidates_json();
    if (typeof candidatesJson !== "string") {
      throw new Error("situation_ring_candidates_json must be synchronous");
    }
    return JSON.parse(candidatesJson) as SituationRingCandidate[];
  }

  async emptyFlightPlan(): Promise<FlightPlan> {
    return JSON.parse(await this.module.empty_flight_plan_json()) as FlightPlan;
  }

  private async enrichFlightPlanUiMutation(mutation: FlightPlanUiMutation): Promise<FlightPlanUiMutation> {
    return runCoreHadOperation<FlightPlanUiMutation>({ kind: "flight_plan_ui_mutation", mutation });
  }

  async createUiSession(
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession> {
    const vectorManifestJson = await debugTiming("startup.vector_manifest.fetch", () => this.vectorManifestJson());
    const module = this.module;
    const createSession = async (
      nextPlan: FlightPlan,
      nextRecentAirportIds: string[],
      nextSelectedAirportId?: string,
      nextSelectedChartId?: string,
    ) => {
      const planJson = debugTiming("startup.session.stringify.plan", () => JSON.stringify(nextPlan));
      const recentAirportIdsJson = debugTiming("startup.session.stringify.recent_airports", () => JSON.stringify(nextRecentAirportIds));
      const selectedAirportIdJson = JSON.stringify(nextSelectedAirportId ?? null);
      const selectedChartIdJson = JSON.stringify(nextSelectedChartId ?? null);
      const createUiSession = module.create_ui_session_profiled ?? module.create_ui_session;
      const createdJson = await debugTiming("startup.session.wasm_call", () => createUiSession(
        vectorManifestJson,
        planJson,
        recentAirportIdsJson,
        selectedAirportIdJson,
        selectedChartIdJson,
      ), { profiled: Boolean(module.create_ui_session_profiled) });
      const createdEnvelope = debugTiming("startup.session.parse_result", () => JSON.parse(createdJson)) as { result?: { handle: number; snapshot: UiSessionSnapshot }; timings?: Array<{ label: string; elapsed_ms: number }>; handle: number; snapshot: UiSessionSnapshot };
      if (createdEnvelope.timings) {
        debugLog("startup.session.core_profile", { timings: createdEnvelope.timings });
      }
      const created = createdEnvelope.result ?? createdEnvelope;
      await attachNavKvStoreToSession(created.handle);
      await module.set_raster_resource_mode_in_session(created.handle, JSON.stringify("public_unpacked"));
      const catalogedSnapshot = await debugTiming("startup.session.load_raster_catalog", () =>
        runCoreHadSessionOperation<UiSessionSnapshot>(() =>
          module.load_raster_map_catalog_in_session(created.handle),
        ),
      );
      return {
        ...created,
        snapshot: catalogedSnapshot,
      };
    };
    const init = await createSession(plan, recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    let snapshot = init.snapshot;
    const parseSessionSnapshot = async (json: Promise<string> | string) =>
      JSON.parse(await json) as UiSessionSnapshot;
    const syncGuidanceGeometry = async () => {
      snapshot = await runCoreHadSessionOperation<UiSessionSnapshot>(() =>
        this.module.sync_guidance_geometry_in_session(handle),
      );
      return snapshot;
    };
    const isInvalidSessionHandleError = (error: unknown) =>
      error instanceof Error && error.message.includes("invalid ui session handle");
    const ensureSession = async () => {
      const desiredPlan = snapshot.app_state.active_plan ?? plan;
      const desiredRecentAirportIds = snapshot.chart_page_state.recent_airport_ids;
      const desiredSelectedAirportId = snapshot.chart_page_state.selected_airport_id || undefined;
      const desiredSelectedChartId = snapshot.chart_page_state.selected_chart_id || undefined;
      const restored = await createSession(
        desiredPlan,
        desiredRecentAirportIds,
        desiredSelectedAirportId,
        desiredSelectedChartId,
      );
      handle = restored.handle;
      snapshot = restored.snapshot;
      await syncGuidanceGeometry();
    };
    const withSessionRetry = async <T>(operation: () => Promise<T>) => {
      try {
        return await operation();
      } catch (error) {
        if (!isInvalidSessionHandleError(error)) {
          throw error;
        }
        await ensureSession();
        return operation();
      }
    };
    await syncGuidanceGeometry();
    return {
      snapshot: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.get_session_snapshot(handle)),
        );
        return snapshot;
      },
      replaceFlightPlan: async (plan) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.replace_flight_plan_in_session(handle, JSON.stringify(plan))),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      performMapSelectionAction: async (action) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>(() =>
            this.module.perform_map_selection_action_in_session(handle, action),
          ),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      insertWaypointAtFlightPlanRow: async (rowUid, before, waypoint) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.insert_waypoint_at_flight_plan_row_in_session(
            handle,
            rowUid,
            before,
            JSON.stringify(waypoint),
          )),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      suggestWaypointIdentifiersAtFlightPlanRow: async (rowUid, before, prefix, limit = 8) => {
        return withSessionRetry(async () =>
          runCoreHadSessionOperation<WaypointIdentifierSuggestion[]>(() =>
            this.module.suggest_waypoint_identifiers_at_flight_plan_row_in_session(
              handle,
              rowUid,
              before,
              prefix,
              limit,
            ),
          ),
        );
      },
      insertAirwayAtFlightPlanRow: async (rowUid, presentation, entryIndex, exitIndex) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>(() =>
            this.module.insert_airway_at_flight_plan_row_in_session(
              handle,
              rowUid,
              JSON.stringify(presentation),
              entryIndex,
              exitIndex,
            ),
          ),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      selectProcedureAtFlightPlanRow: async (rowUid, airportId, procedureId, kind, runwayTransition, enrouteTransition) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>(() =>
            this.module.select_procedure_at_flight_plan_row_in_session(
              handle,
              rowUid,
              airportId,
              procedureId,
              JSON.stringify(kind),
              JSON.stringify(runwayTransition),
              JSON.stringify(enrouteTransition),
            ),
          ),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      loadPlateProcedure: async (loadId) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>(() =>
            this.module.load_plate_procedure_in_session(handle, loadId),
          ),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      restoreDirectTo: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.restore_direct_to_in_session(handle)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      performFlightPlanRowAction: async (rowUid, actionUid) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.perform_flight_plan_row_action_in_session(handle, rowUid, actionUid)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      activateNextLeg: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.activate_next_leg_in_session(handle)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      suspendSequencing: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.suspend_sequencing_in_session(handle)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      unsuspendSequencing: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.unsuspend_sequencing_in_session(handle)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      sequenceActiveLeg: async () => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.sequence_active_leg_in_session(handle)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      setSituation: async (situation) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.set_situation_in_session(handle, JSON.stringify(situation))),
        );
        return snapshot;
      },
      tickDebugOwnshipDriver: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.tick_debug_ownship_driver_in_session(handle, nowEpochMs)),
        );
        await syncGuidanceGeometry();
        return snapshot;
      },
      registerOwnshipSource: async (registration) => {
        snapshot = await parseSessionSnapshot(
          this.module.register_ownship_source_in_session(handle, JSON.stringify(registration)),
        );
        return snapshot;
      },
      updateOwnshipSourceStatus: async (update) => {
        snapshot = await parseSessionSnapshot(
          this.module.update_ownship_source_status_in_session(handle, JSON.stringify(update)),
        );
        return snapshot;
      },
      pushSituationSample: async (sample) => {
        snapshot = await parseSessionSnapshot(
          this.module.push_situation_sample_in_session(handle, JSON.stringify(sample)),
        );
        return snapshot;
      },
      selectOwnshipSource: async (selection) => {
        snapshot = await parseSessionSnapshot(
          this.module.select_ownship_source_in_session(handle, JSON.stringify(ownshipSelectionToCore(selection))),
        );
        return snapshot;
      },
      applySituationControlInput: async (input, nowEpochMs) => {
        snapshot = await parseSessionSnapshot(
          this.module.apply_situation_control_input_in_session(handle, JSON.stringify(input), nowEpochMs),
        );
        return snapshot;
      },
      setMapLayerVisibility: async (layerId, visible) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.set_map_layer_visibility_in_session(handle, JSON.stringify(layerId), visible),
          ),
        );
        return snapshot;
      },
      setMapLayerEnabled: async (layerId, enabled) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.set_map_layer_enabled_in_session(handle, JSON.stringify(layerId), enabled),
          ),
        );
        return snapshot;
      },
      setDebugFlag: async (flagId, enabled) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.set_debug_flag_in_session(handle, JSON.stringify(flagId), enabled),
          ),
        );
        return snapshot;
      },
      loadRasterMapCatalog: async () => {
        snapshot = await runCoreHadSessionOperation<UiSessionSnapshot>(() =>
          this.module.load_raster_map_catalog_in_session(handle),
        );
        return snapshot;
      },
      selectMapFamily: async (familyId) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>(() =>
            this.module.select_map_family_in_session(handle, JSON.stringify(familyId)),
          ),
        );
        return snapshot;
      },
      selectRasterMap: async (selectedMapId) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.select_raster_map_in_session(handle, JSON.stringify(selectedMapId))),
        );
        return snapshot;
      },
      engageMapFollow: async (viewport) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.engage_map_follow_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
            ),
          ),
        );
        return snapshot;
      },
      disengageMapFollow: async (viewport) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.disengage_map_follow_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
            ),
          ),
        );
        return snapshot;
      },
      setMapFollowOffset: async (viewport, offsetXPx, offsetYPx) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.set_map_follow_offset_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              offsetXPx,
              offsetYPx,
            ),
          ),
        );
        return snapshot;
      },
      syncMapFollow: async (viewport, widthPx, heightPx) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.sync_map_follow_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
            ),
          ),
        );
        return snapshot;
      },
      loadPlaybackTrace: async (sourcePath, traceJson) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.load_playback_trace_in_session(handle, JSON.stringify(sourcePath), traceJson)),
        );
        return snapshot;
      },
      playPlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.play_playback_in_session(handle, nowEpochMs)),
        );
        return snapshot;
      },
      pausePlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.pause_playback_in_session(handle, nowEpochMs)),
        );
        return snapshot;
      },
      seekPlayback: async (cursorSeconds, nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.seek_playback_in_session(handle, cursorSeconds, nowEpochMs)),
        );
        return snapshot;
      },
      setPlaybackRate: async (rate, nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.set_playback_rate_in_session(handle, rate, nowEpochMs)),
        );
        return snapshot;
      },
      tickPlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.tick_playback_in_session(handle, nowEpochMs)),
        );
        return snapshot;
      },
      selectAirport: async (airportId) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.select_airport_in_session(handle, JSON.stringify(airportId))),
        );
        return snapshot;
      },
      selectChart: async (chartId) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.select_chart_in_session(handle, JSON.stringify(chartId))),
        );
        return snapshot;
      },
      ingestPointTiles: async (tiles) => {
        await withSessionRetry(async () => {
          await this.module.ingest_point_tiles_in_session(handle, JSON.stringify(tiles));
        });
      },
      ingestAirspaceRefTiles: async (tiles) => {
        await withSessionRetry(async () => {
          await this.module.ingest_airspace_ref_tiles_in_session(handle, JSON.stringify(tiles));
        });
      },
      ingestAirspaceFeatures: async (features) => {
        await withSessionRetry(async () => {
          await this.module.ingest_airspace_features_in_session(handle, JSON.stringify(features));
        });
      },
      ingestAirspaceLabelTiles: async (tiles) => {
        await withSessionRetry(async () => {
          await this.module.ingest_airspace_label_tiles_in_session(handle, JSON.stringify(tiles));
        });
      },
      queryMapOverlay: async (viewport, widthPx, heightPx) =>
        withSessionRetry(async () =>
          runCoreHadSessionOperation<MapOverlayQueryResult>(
            () =>
              this.module.get_map_overlay_in_session(
                handle,
                JSON.stringify(coreViewportForMap(viewport)),
                widthPx,
                heightPx,
              ),
            (resourceId, resourceBytes) => this.module.ingest_resource_in_session(handle, resourceId, resourceBytes),
          ),
        ),
      queryMapSelection: async (viewport, widthPx, heightPx, click, hitRadiusPx) =>
        withSessionRetry(async () =>
          runCoreHadSessionOperation<MapSelectionQueryResult>(() =>
            this.module.get_map_selection_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              JSON.stringify(click),
              hitRadiusPx,
            ),
          ),
        ),
      queryTerrainOverlay: async (viewport, widthPx, heightPx) =>
        withSessionRetry(async () =>
          JSON.parse(
            await this.module.get_terrain_overlay_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
            ),
          ) as TerrainOverlayQueryResult,
        ),
      queryRasterTilePlan: async (viewport, widthPx, heightPx) =>
        withSessionRetry(async () =>
          JSON.parse(
            await this.module.get_raster_tile_plan_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
            ),
          ) as RasterTilePlan,
        ),
      renderTerrainOverlayTile: async (tileBytes, aircraftAltitudeFt) =>
        withSessionRetry(async () =>
          new Uint8Array(await this.module.render_terrain_overlay_tile_in_session(handle, tileBytes, aircraftAltitudeFt)),
        ),
      renderTerrainOverlayTiles: async (packedTileBytes, aircraftAltitudeFt) =>
        withSessionRetry(async () =>
          new Uint8Array(await this.module.render_terrain_overlay_tiles_in_session(handle, packedTileBytes, aircraftAltitudeFt)),
        ),
      restoreChartPageState: async (nextRecentAirportIds, nextSelectedAirportId, nextSelectedChartId) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.restore_chart_page_state_in_session(
              handle,
              JSON.stringify(nextRecentAirportIds),
              JSON.stringify(nextSelectedAirportId ?? null),
              JSON.stringify(nextSelectedChartId ?? null),
            ),
          ),
        );
        return snapshot;
      },
      destroy: async () => {
        this.module.destroy_session(handle);
      },
    };
  }

  async deriveChartPageState(
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<DerivedChartPageState> {
    return runCoreHadOperation<DerivedChartPageState>({
      kind: "chart_page_state",
      plan,
      recent_airport_ids: recentAirportIds,
      selected_airport_id: selectedAirportId ?? null,
      selected_chart_id: selectedChartId ?? null,
    });
  }

  async projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]> {
    void planUiState;
    return runCoreHadOperation<FlightPlanRouteSegment[]>({ kind: "project_flight_plan_route", plan });
  }

  async previewFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanEntryPreview> {
    return runCoreHadOperation<FlightPlanEntryPreview>({
      kind: "preview_flight_plan_entry",
      plan,
      input,
    });
  }

  async appendFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanUiMutation> {
    return runCoreHadOperation<FlightPlanUiMutation>({
      kind: "append_flight_plan_entry",
      plan,
      input,
    });
  }

  async resolveWaypointIdentifier(identifier: string): Promise<NavRef | null> {
    return runCoreHadOperation<NavRef | null>({ kind: "resolve_waypoint_identifier", identifier });
  }

  async resolveNavRefPosition(navRef: NavRef): Promise<LatLon> {
    return runCoreHadOperation<LatLon>({ kind: "resolve_nav_ref_position", nav_ref: navRef });
  }

  async suggestWaypointIdentifiersNear(anchor: LatLon, prefix: string, limit = 8): Promise<WaypointIdentifierSuggestion[]> {
    return runCoreHadOperation<WaypointIdentifierSuggestion[]>({
      kind: "suggest_waypoint_identifiers_near",
      anchor,
      prefix,
      limit,
    });
  }

  async suggestAirwaysNearAnchor(anchor: NavRef, limit = 30): Promise<AirwaySuggestion[]> {
    return runCoreHadOperation<AirwaySuggestion[]>({ kind: "suggest_airways_near_anchor", anchor, limit });
  }

  async airwayBranches(airwayName: string): Promise<AirwayBranch[]> {
    return runCoreHadOperation<AirwayBranch[]>({ kind: "airway_branches", airway_name: airwayName });
  }

  async prepareAirwayPresentationForAnchors(
    airwayName: string,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<AirwayPresentationPlan> {
    return runCoreHadOperation<AirwayPresentationPlan>({
      kind: "prepare_airway_presentation_for_anchors",
      airway_name: airwayName,
      origin_anchor: originAnchor,
      destination_anchor: destinationAnchor,
    });
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

  async describePlateProcedureLoads(plan: FlightPlan, plateId: string): Promise<ProcedureLoadOption[]> {
    return runCoreHadOperation<ProcedureLoadOption[]>({
      kind: "describe_plate_procedure_loads",
      plan,
      plate_id: plateId,
    });
  }
}

const defaultWasmImporter = () => import("@generated/app_wasm.js");
let defaultAdapterLoadPromise: Promise<LoadedAdapter> | null = null;

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = defaultWasmImporter,
): Promise<LoadedAdapter> {
  if (importer === defaultWasmImporter) {
    defaultAdapterLoadPromise ??= loadBestAvailableAdapterUncached(importer).catch((error) => {
      defaultAdapterLoadPromise = null;
      throw error;
    });
    return defaultAdapterLoadPromise;
  }
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
  if (
    typeof mod.situation_ring_candidates_json !== "function" ||
    typeof mod.empty_flight_plan_json !== "function" ||
    typeof mod.create_ui_session !== "function" ||
    typeof mod.set_raster_resource_mode_in_session !== "function" ||
    typeof mod.perform_flight_plan_row_action_in_session !== "function" ||
    typeof mod.set_situation_in_session !== "function" ||
    typeof mod.tick_debug_ownship_driver_in_session !== "function" ||
    typeof mod.engage_map_follow_in_session !== "function" ||
    typeof mod.disengage_map_follow_in_session !== "function" ||
    typeof mod.set_map_follow_offset_in_session !== "function" ||
    typeof mod.sync_map_follow_in_session !== "function" ||
    typeof mod.load_playback_trace_in_session !== "function" ||
    typeof mod.play_playback_in_session !== "function" ||
    typeof mod.pause_playback_in_session !== "function" ||
    typeof mod.seek_playback_in_session !== "function" ||
    typeof mod.set_playback_rate_in_session !== "function" ||
    typeof mod.tick_playback_in_session !== "function" ||
    typeof mod.register_ownship_source_in_session !== "function" ||
    typeof mod.update_ownship_source_status_in_session !== "function" ||
    typeof mod.push_situation_sample_in_session !== "function" ||
    typeof mod.select_ownship_source_in_session !== "function" ||
    typeof mod.apply_situation_control_input_in_session !== "function" ||
    typeof mod.set_map_layer_visibility_in_session !== "function" ||
    typeof mod.set_map_layer_enabled_in_session !== "function" ||
    typeof mod.set_debug_flag_in_session !== "function" ||
    typeof mod.load_raster_map_catalog_in_session !== "function" ||
    typeof mod.sync_guidance_geometry_in_session !== "function" ||
    typeof mod.select_map_family_in_session !== "function" ||
    typeof mod.select_raster_map_in_session !== "function" ||
    typeof mod.replace_flight_plan_in_session !== "function" ||
    typeof mod.perform_map_selection_action_in_session !== "function" ||
    typeof mod.select_airport_in_session !== "function" ||
    typeof mod.select_chart_in_session !== "function" ||
    typeof mod.ingest_point_tiles_in_session !== "function" ||
    typeof mod.ingest_airspace_ref_tiles_in_session !== "function" ||
    typeof mod.ingest_airspace_features_in_session !== "function" ||
    typeof mod.ingest_airspace_label_tiles_in_session !== "function" ||
    typeof mod.get_map_overlay_in_session !== "function" ||
    typeof mod.get_map_selection_in_session !== "function" ||
    typeof mod.get_terrain_overlay_in_session !== "function" ||
    typeof mod.get_raster_tile_plan_in_session !== "function" ||
    typeof mod.render_terrain_overlay_tile_in_session !== "function" ||
    typeof mod.render_terrain_overlay_tiles_in_session !== "function" ||
    typeof mod.get_session_snapshot !== "function" ||
    typeof mod.restore_chart_page_state_in_session !== "function" ||
    typeof mod.destroy_session !== "function" ||
    typeof mod.install_rust_debug_logger !== "function" ||
    typeof mod.insert_waypoint_at_flight_plan_row_in_session !== "function" ||
    typeof mod.suggest_waypoint_identifiers_at_flight_plan_row_in_session !== "function" ||
    typeof mod.insert_airway_at_flight_plan_row_in_session !== "function" ||
    typeof mod.select_procedure_at_flight_plan_row_in_session !== "function" ||
    typeof mod.load_plate_procedure_in_session !== "function" ||
    typeof mod.activate_next_leg_in_session !== "function" ||
    typeof mod.suspend_sequencing_in_session !== "function" ||
    typeof mod.unsuspend_sequencing_in_session !== "function" ||
    typeof mod.sequence_active_leg_in_session !== "function" ||
    typeof mod.ingest_resource_in_session !== "function" ||
    typeof mod.nav_kv_open !== "function" ||
    typeof mod.nav_kv_insert_resource !== "function" ||
    typeof mod.nav_kv_prefetch_pages !== "function" ||
    typeof mod.nav_kv_destroy !== "function" ||
    typeof mod.attach_nav_kv_store_to_session !== "function" ||
    typeof mod.core_had_operation !== "function"
  ) {
    throw new Error("generated wasm module is missing required exports");
  }
  debugLog("wasm.exports.check.done");

  return {
    adapter: new WasmAppCoreAdapter(mod as WasmModule),
    backend: "wasm",
    detail: "Using generated Rust WASM bindings.",
  };
}

function coreViewportForMap(viewport: MapViewportState) {
  const center = viewportCenterLatLon(viewport);
  return {
    center,
    zoom: viewport.zoom,
    rotation_deg: 0,
    pitch_deg: 0,
  };
}
