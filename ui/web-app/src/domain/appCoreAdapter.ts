import type {
  UiSnapshotAppState,
  AppUiState,
  AirwayAutoSelection,
  AirwayBranch,
  AirwayEntryCandidate,
  AirwayExitCandidate,
  AirwayPresentationPlan,
  AirwaySuggestion,
  AirwaySegment,
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
  GeometryJson,
  MapViewOptionJson,
  MaterializedProcedure,
  NavRef,
  NavSymbolFeature,
  OwnshipSelectionCommand,
  OwnshipSourceRegistration,
  OwnshipSourceStatusUpdate,
  PlanLeg,
  PlaybackUiState,
  ProcedureLoadOption,
  ProcedureOptions,
  ProcedureSummary,
  ResolvedLeg,
  ResolvedLegUiView,
  RouteComponentUiView,
  SequencingMode,
  Situation,
  SituationRingCandidate,
  SituationSample,
  WaypointIdentifierSuggestion,
} from "./types";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import { runCoreHadOperation, runCoreHadSessionOperation } from "./navKv";
import { debugLog, debugTiming } from "./debugLog";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
};

export type DerivedMapSelectorState = {
  selected_map_id: string;
  selected_map: MapViewOptionJson | null;
  displayed_maps: MapViewOptionJson[];
  geometry: GeometryJson;
  family_options: Array<{
    id: ChartFamilyId;
    label: string;
    launcher_label: string;
    enabled: boolean;
    active: boolean;
    next_map_id: string | null;
  }>;
};

const EMPTY_GEOMETRY: GeometryJson = {
  schema_version: 1,
  polygons: [],
  polygon_sets: [],
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
};

export type UiCautionState = {
  obstacle_display_limited: boolean;
};

export type MapLayerId = "vectors" | "nexrad" | "terrain_warning";

export type UiMapLayerToggleState = {
  visible: boolean;
  enabled: boolean;
};

export type UiMapLayerState = {
  vectors: UiMapLayerToggleState;
  nexrad: UiMapLayerToggleState;
  terrain_warning: UiMapLayerToggleState;
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
  vertical_label: string;
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

export type AirspaceFeatureRequest = {
  id: string;
  path: string;
};

export type AirspaceDisplayPath = {
  id: string;
  name: string;
  label: string;
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
  text: string;
  style_key: string;
  screen_x: number;
  screen_y: number;
};

export type TfrProductPayload = {
  schema_version: number;
  version_label: string;
  notam_count: number;
  area_group_count: number;
  areas: Array<{
    notam_id: string;
    area_index: number;
    schedule_fragments: Array<{
      kind: string;
      value_utc: string;
    }>;
    upper_limit: {
      value_text: string;
      unit: string;
    };
    lower_limit: {
      value_text: string;
      unit: string;
    };
    polygon: Array<{
      lat: number;
      lon: number;
    }>;
    avare_text: string;
  }>;
};

export type MapOverlayQueryResult = {
  needed_point_tiles: VectorTileRequest[];
  needed_airspace_ref_tiles: VectorTileRequest[];
  needed_airspace_features: AirspaceFeatureRequest[];
  needed_airspace_label_tiles: VectorTileRequest[];
  needed_tfrs: boolean;
  visible_features: VisibleMapFeature[];
  airspace_paths: AirspaceDisplayPath[];
  tfr_paths: AirspaceDisplayPath[];
  airspace_labels: AirspaceDisplayLabel[];
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
  highlight: MapSelectionHighlight;
  nav_ref?: NavRef | null;
  symbol_feature?: NavSymbolFeature | null;
  airspace_icon?: AirspaceDisplayPath | null;
  actions: MapSelectionAction[];
};

export type MapSelectionHighlight =
  | { kind: "feature_ref"; id: string }
  | { kind: "spot"; lat: number; lon: number };

export type MapSelectionAction = {
  id: string;
  label: string;
  enabled: boolean;
  display_only: boolean;
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
  url: string;
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
  removeLeg(index: number): Promise<UiSessionSnapshot>;
  moveWaypoint(index: number, delta: number): Promise<UiSessionSnapshot>;
  insertWaypointBestPosition(waypoint: NavRef): Promise<UiSessionSnapshot>;
  removeTopLevelWaypointByNavRef(navRef: NavRef): Promise<UiSessionSnapshot>;
  setSituation(situation: Situation): Promise<UiSessionSnapshot>;
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
  setMapLayerVisibility(layerId: MapLayerId, visible: boolean): Promise<UiSessionSnapshot>;
  setMapLayerEnabled(layerId: MapLayerId, enabled: boolean): Promise<UiSessionSnapshot>;
  installRasterMapCatalog(catalog: DerivedMapSelectorState): Promise<UiSessionSnapshot>;
  selectMap(mapId: string): Promise<UiSessionSnapshot>;
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  ingestAirspaceRefTiles(tiles: AirspaceReferenceTilePayload[]): Promise<void>;
  ingestAirspaceFeatures(features: AirspaceFeaturePayload[]): Promise<void>;
  ingestAirspaceLabelTiles(tiles: AirspaceLabelTilePayload[]): Promise<void>;
  ingestTfrs(payload: TfrProductPayload): Promise<void>;
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
  deriveMapSelectorState(selectedMapId?: string): Promise<DerivedMapSelectorState>;
  projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]>;
  activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation>;
  activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  deleteComponentUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation>;
  moveComponentUi(plan: FlightPlan, componentIndex: number, delta: number): Promise<FlightPlanUiMutation>;
  removeAllAboveUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation>;
  previewFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanEntryPreview>;
  appendFlightPlanEntry(plan: FlightPlan, input: string): Promise<FlightPlanUiMutation>;
  resolveWaypointIdentifier(identifier: string): Promise<NavRef | null>;
  resolveNavRefPosition(navRef: NavRef): Promise<LatLon>;
  suggestWaypointIdentifiers(plan: FlightPlan, componentIndex: number, before: boolean, prefix: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  suggestWaypointIdentifiersNear(anchor: LatLon, prefix: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  insertWaypointUi(plan: FlightPlan, componentIndex: number, before: boolean, waypoint: NavRef): Promise<FlightPlanUiMutation>;
  suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  suggestAirwaysNearAnchor(anchor: NavRef, limit?: number): Promise<AirwaySuggestion[]>;
  prepareAirwayPresentationForAnchors(
    airwayName: string,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<AirwayPresentationPlan>;
  materializeAirwaySelection(
    startComponentIndex: number,
    entry: AirwayEntryCandidate,
    exit: AirwayExitCandidate,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<{
    selection: AirwayAutoSelection;
    airway: AirwaySegment;
    resolvedLegs: ResolvedLeg[];
  }>;
  insertAirwayMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number | null,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation>;
  replaceAirwayMaterializedUi(
    plan: FlightPlan,
    componentIndex: number,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation>;
  listProcedures(airportId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]>;
  describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions>;
  insertProcedureMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number,
    built: MaterializedProcedure,
  ): Promise<FlightPlanUiMutation>;
  replaceProcedureMaterializedUi(
    plan: FlightPlan,
    componentIndex: number,
    built: MaterializedProcedure,
  ): Promise<FlightPlanUiMutation>;
  materializeProcedure(
    airportId: string,
    procedureId: string,
    kind: "sid" | "star" | "approach",
    runwayTransition: string | null,
    enrouteTransition: string | null,
    componentIndex: number,
  ): Promise<MaterializedProcedure>;
  findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null>;
  describePlateProcedureLoads(plan: FlightPlan, plateId: string): Promise<ProcedureLoadOption[]>;
}

export type AdapterBackendKind = "wasm";

export type LoadedAdapter = {
  adapter: AppCoreAdapter;
  backend: AdapterBackendKind;
  detail: string;
};

async function fetchVectorManifestJson(): Promise<string> {
  const response = await fetch("/vectors/vectors", { cache: "no-cache" });
  if (!response.ok) {
    throw new Error(`failed to load vector manifest: ${response.status}`);
  }
  const baseManifest = JSON.parse(await response.text()) as Record<string, unknown>;
  try {
    const obstacleResponse = await fetch("/fast-products/obstacles/obstacles", { cache: "no-cache" });
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
  return JSON.stringify(baseManifest);
}

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  situation_ring_candidates_json(): Promise<string> | string;
  create_ui_session(vectorManifestJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  create_ui_session_profiled?: (vectorManifestJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string) => Promise<string> | string;
  remove_leg_in_session(handle: number, index: number): Promise<string> | string;
  move_waypoint_in_session(handle: number, waypointIndex: number, delta: number): Promise<string> | string;
  remove_top_level_waypoint_by_nav_ref_in_session(handle: number, navRefJson: string): Promise<string> | string;
  set_situation_in_session(handle: number, situationJson: string): Promise<string> | string;
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
  set_map_layer_visibility_in_session(handle: number, layerIdJson: string, visible: boolean): Promise<string> | string;
  set_map_layer_enabled_in_session(handle: number, layerIdJson: string, enabled: boolean): Promise<string> | string;
  set_raster_map_catalog_in_session(handle: number, catalogJson: string): Promise<string> | string;
  select_map_in_session(handle: number, selectedMapIdJson: string): Promise<string> | string;
  replace_flight_plan_in_session(handle: number, planJson: string): Promise<string> | string;
  insert_waypoint_best_position_in_session(navKvHandle: number, sessionHandle: number, waypointJson: string): Promise<string> | string;
  set_guidance_leg_geometry_in_session(handle: number, geometriesJson: string): Promise<string> | string;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<string> | string;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<string> | string;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_ref_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_airspace_features_in_session(handle: number, featuresJson: string): Promise<void> | void;
  ingest_airspace_label_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  ingest_tfrs_in_session(handle: number, payloadJson: string): Promise<void> | void;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_map_selection_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number, clickJson: string, hitRadiusPx: number): Promise<string> | string;
  get_terrain_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_raster_tile_plan_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  render_terrain_overlay_tile_in_session(handle: number, terrainTileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  render_terrain_overlay_tiles_in_session(handle: number, packedTerrainTileBytes: Uint8Array, aircraftAltitudeFt: number): Promise<Uint8Array> | Uint8Array;
  get_session_snapshot(handle: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  destroy_session(handle: number): void;
  nav_kv_open(rootBytes: Uint8Array): Promise<number> | number;
  nav_kv_insert_page(handle: number, pageIndex: number, pageBytes: Uint8Array): Promise<void> | void;
  nav_kv_destroy(handle: number): Promise<void> | void;
  core_had_operation(handle: number, operationJson: string): Promise<string> | string;
  activate_leg_ui(planJson: string, legIndex: number): Promise<string> | string;
  activate_next_leg_ui(planJson: string): Promise<string> | string;
  delete_component_ui(planJson: string, componentIndex: number): Promise<string> | string;
  move_component_ui(planJson: string, componentIndex: number, delta: number): Promise<string> | string;
  remove_all_above_ui(planJson: string, componentIndex: number): Promise<string> | string;
  insert_waypoint_ui(planJson: string, componentIndex: number, before: boolean, waypointJson: string): Promise<string> | string;
  suspend_sequencing_ui(planJson: string): Promise<string> | string;
  unsuspend_sequencing_ui(planJson: string): Promise<string> | string;
  sequence_active_leg_ui(planJson: string): Promise<string> | string;
  insert_airway_materialized_ui(
    planJson: string,
    startComponentIndex: number,
    endComponentIndexJson: string,
    selectionJson: string,
    airwayJson: string,
    resolvedLegsJson: string,
  ): Promise<string> | string;
  replace_airway_materialized_ui(
    planJson: string,
    componentIndex: number,
    selectionJson: string,
    airwayJson: string,
    resolvedLegsJson: string,
  ): Promise<string> | string;
  insert_procedure_materialized_ui(
    planJson: string,
    startComponentIndex: number,
    endComponentIndex: number,
    builtJson: string,
  ): Promise<string> | string;
  replace_procedure_materialized_ui(
    planJson: string,
    componentIndex: number,
    builtJson: string,
  ): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  private vectorManifestJsonPromise: Promise<string> | null = null;

  constructor(private readonly module: WasmModule) {}

  private async vectorManifestJson(): Promise<string> {
    this.vectorManifestJsonPromise ??= fetchVectorManifestJson();
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

  private async enrichFlightPlanUiState(plan: FlightPlan, uiState: FlightPlanUiState): Promise<FlightPlanUiState> {
    return runCoreHadOperation<FlightPlanUiState>({
      kind: "flight_plan_ui_state",
      plan,
      current_ui_state: uiState,
    });
  }

  private async enrichUiSessionSnapshot(snapshot: UiSessionSnapshot): Promise<UiSessionSnapshot> {
    const plan = snapshot.app_state.active_plan;
    return {
      ...snapshot,
      app_ui_state: {
        ...snapshot.app_ui_state,
        active_plan: plan && snapshot.app_ui_state.active_plan
          ? await this.enrichFlightPlanUiState(plan, snapshot.app_ui_state.active_plan)
          : null,
      },
    };
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
      const rasterMapCatalog = await debugTiming("startup.session.raster_catalog", () =>
        this.deriveMapSelectorState(undefined),
      );
      const catalogedSnapshot = await debugTiming("startup.session.install_raster_catalog", async () =>
        JSON.parse(await module.set_raster_map_catalog_in_session(created.handle, JSON.stringify(rasterMapCatalog))) as UiSessionSnapshot,
      );
      return {
        ...created,
        snapshot: await debugTiming("startup.session.enrich_snapshot", () => this.enrichUiSessionSnapshot(catalogedSnapshot)),
      };
    };
    const init = await createSession(plan, recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    let snapshot = init.snapshot;
    const parseSessionSnapshot = async (json: Promise<string> | string) =>
      this.enrichUiSessionSnapshot(JSON.parse(await json) as UiSessionSnapshot);
    const syncGuidanceGeometry = async (nextPlan: FlightPlan | null) => {
      try {
        const geometries =
          nextPlan && (nextPlan.resolved_legs ?? []).length > 0
            ? (await this.projectFlightPlanRoute(nextPlan, null)).map((segment) => ({
                leg_id: segment.id,
                from: segment.from,
                to: segment.to,
                path: segment.path,
              }))
            : [];
        snapshot = await parseSessionSnapshot(
          this.module.set_guidance_leg_geometry_in_session(handle, JSON.stringify(geometries)),
        );
      } catch (error) {
        console.error("failed to sync session guidance geometry", error);
      }
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
      await syncGuidanceGeometry(snapshot.app_state.active_plan);
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
    await syncGuidanceGeometry(snapshot.app_state.active_plan);
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
        await syncGuidanceGeometry(snapshot.app_state.active_plan);
        return snapshot;
      },
      removeLeg: async (index) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.remove_leg_in_session(handle, index)),
        );
        await syncGuidanceGeometry(snapshot.app_state.active_plan);
        return snapshot;
      },
      moveWaypoint: async (index, delta) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.move_waypoint_in_session(handle, index, delta)),
        );
        await syncGuidanceGeometry(snapshot.app_state.active_plan);
        return snapshot;
      },
      insertWaypointBestPosition: async (waypoint) => {
        snapshot = await withSessionRetry(async () =>
          runCoreHadSessionOperation<UiSessionSnapshot>((navKvHandle) =>
            this.module.insert_waypoint_best_position_in_session(navKvHandle, handle, JSON.stringify(waypoint)),
          ),
        );
        await syncGuidanceGeometry(snapshot.app_state.active_plan);
        return snapshot;
      },
      removeTopLevelWaypointByNavRef: async (navRef) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.remove_top_level_waypoint_by_nav_ref_in_session(handle, JSON.stringify(navRef))),
        );
        await syncGuidanceGeometry(snapshot.app_state.active_plan);
        return snapshot;
      },
      setSituation: async (situation) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.set_situation_in_session(handle, JSON.stringify(situation))),
        );
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
          this.module.select_ownship_source_in_session(handle, JSON.stringify(selection)),
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
      installRasterMapCatalog: async (catalog) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(
            this.module.set_raster_map_catalog_in_session(handle, JSON.stringify(catalog)),
          ),
        );
        return snapshot;
      },
      selectMap: async (mapId) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.select_map_in_session(handle, JSON.stringify(mapId))),
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
      ingestTfrs: async (payload) => {
        await withSessionRetry(async () => {
          await this.module.ingest_tfrs_in_session(handle, JSON.stringify(payload));
        });
      },
      queryMapOverlay: async (viewport, widthPx, heightPx) =>
        withSessionRetry(async () =>
          JSON.parse(
            await this.module.get_map_overlay_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
            ),
          ) as MapOverlayQueryResult,
        ),
      queryMapSelection: async (viewport, widthPx, heightPx, click, hitRadiusPx) =>
        withSessionRetry(async () =>
          JSON.parse(
            await this.module.get_map_selection_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              widthPx,
              heightPx,
              JSON.stringify(click),
              hitRadiusPx,
            ),
          ) as MapSelectionQueryResult,
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

  async deriveMapSelectorState(selectedMapId?: string): Promise<DerivedMapSelectorState> {
    const state = await runCoreHadOperation<Partial<DerivedMapSelectorState>>({
      kind: "map_selector_state",
      selected_map_id: selectedMapId ?? null,
    });
    return {
      selected_map_id: state.selected_map_id ?? "",
      selected_map: state.selected_map ?? null,
      displayed_maps: state.displayed_maps ?? [],
      geometry: state.geometry ?? EMPTY_GEOMETRY,
      family_options: state.family_options ?? [],
    };
  }

  async projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]> {
    void planUiState;
    return runCoreHadOperation<FlightPlanRouteSegment[]>({ kind: "project_flight_plan_route", plan });
  }

  async activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.activate_leg_ui(JSON.stringify(plan), legIndex),
    ) as FlightPlanUiMutation);
  }

  async activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.activate_next_leg_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation);
  }

  async deleteComponentUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.delete_component_ui(JSON.stringify(plan), componentIndex),
    ) as FlightPlanUiMutation);
  }

  async moveComponentUi(plan: FlightPlan, componentIndex: number, delta: number): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.move_component_ui(JSON.stringify(plan), componentIndex, delta),
    ) as FlightPlanUiMutation);
  }

  async removeAllAboveUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.remove_all_above_ui(JSON.stringify(plan), componentIndex),
    ) as FlightPlanUiMutation);
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

  async suggestWaypointIdentifiers(
    plan: FlightPlan,
    componentIndex: number,
    before: boolean,
    prefix: string,
    limit = 8,
  ): Promise<WaypointIdentifierSuggestion[]> {
    return runCoreHadOperation<WaypointIdentifierSuggestion[]>({
      kind: "suggest_waypoint_identifiers",
      plan,
      component_index: componentIndex,
      before,
      prefix,
      limit,
    });
  }

  async suggestWaypointIdentifiersNear(anchor: LatLon, prefix: string, limit = 8): Promise<WaypointIdentifierSuggestion[]> {
    return runCoreHadOperation<WaypointIdentifierSuggestion[]>({
      kind: "suggest_waypoint_identifiers_near",
      anchor,
      prefix,
      limit,
    });
  }

  async insertWaypointUi(plan: FlightPlan, componentIndex: number, before: boolean, waypoint: NavRef): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.insert_waypoint_ui(JSON.stringify(plan), componentIndex, before, JSON.stringify(waypoint)),
    ) as FlightPlanUiMutation);
  }

  async suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.suspend_sequencing_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation);
  }

  async unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.unsuspend_sequencing_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation);
  }

  async sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return this.enrichFlightPlanUiMutation(JSON.parse(
      await this.module.sequence_active_leg_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation);
  }

  async suggestAirwaysNearAnchor(anchor: NavRef, limit = 5): Promise<AirwaySuggestion[]> {
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

  async materializeAirwaySelection(
    startComponentIndex: number,
    entry: AirwayEntryCandidate,
    exit: AirwayExitCandidate,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<{
    selection: AirwayAutoSelection;
    airway: AirwaySegment;
    resolvedLegs: ResolvedLeg[];
  }> {
    return runCoreHadOperation<{
      selection: AirwayAutoSelection;
      airway: AirwaySegment;
      resolvedLegs: ResolvedLeg[];
    }>({
      kind: "materialize_airway_selection",
      start_component_index: startComponentIndex,
      entry,
      exit,
      origin_anchor: originAnchor,
      destination_anchor: destinationAnchor,
    });
  }

  async insertAirwayMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number | null,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation> {
    const result = JSON.parse(
      await this.module.insert_airway_materialized_ui(
        JSON.stringify(plan),
        startComponentIndex,
        JSON.stringify(endComponentIndex),
        JSON.stringify(selection),
        JSON.stringify(airway),
        JSON.stringify(resolvedLegs),
      ),
    ) as { mutation: { plan: FlightPlan }; ui_state: FlightPlanUiState };
    return {
      plan: result.mutation.plan,
      ui_state: await this.enrichFlightPlanUiState(result.mutation.plan, result.ui_state),
    };
  }

  async replaceAirwayMaterializedUi(
    plan: FlightPlan,
    componentIndex: number,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation> {
    const result = JSON.parse(
      await this.module.replace_airway_materialized_ui(
        JSON.stringify(plan),
        componentIndex,
        JSON.stringify(selection),
        JSON.stringify(airway),
        JSON.stringify(resolvedLegs),
      ),
    ) as { mutation: { plan: FlightPlan }; ui_state: FlightPlanUiState };
    return {
      plan: result.mutation.plan,
      ui_state: await this.enrichFlightPlanUiState(result.mutation.plan, result.ui_state),
    };
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

  async insertProcedureMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number,
    built: MaterializedProcedure,
  ): Promise<FlightPlanUiMutation> {
    const result = JSON.parse(
      await this.module.insert_procedure_materialized_ui(
        JSON.stringify(plan),
        startComponentIndex,
        endComponentIndex,
        JSON.stringify(built),
      ),
    ) as { mutation: { plan: FlightPlan }; ui_state: FlightPlanUiState };
    return {
      plan: result.mutation.plan,
      ui_state: await this.enrichFlightPlanUiState(result.mutation.plan, result.ui_state),
    };
  }

  async replaceProcedureMaterializedUi(
    plan: FlightPlan,
    componentIndex: number,
    built: MaterializedProcedure,
  ): Promise<FlightPlanUiMutation> {
    const result = JSON.parse(
      await this.module.replace_procedure_materialized_ui(
        JSON.stringify(plan),
        componentIndex,
        JSON.stringify(built),
      ),
    ) as { mutation: { plan: FlightPlan }; ui_state: FlightPlanUiState };
    return {
      plan: result.mutation.plan,
      ui_state: await this.enrichFlightPlanUiState(result.mutation.plan, result.ui_state),
    };
  }

  async materializeProcedure(
    airportId: string,
    procedureId: string,
    kind: "sid" | "star" | "approach",
    runwayTransition: string | null,
    enrouteTransition: string | null,
    componentIndex: number,
  ): Promise<MaterializedProcedure> {
    return runCoreHadOperation<MaterializedProcedure>({
      kind: "materialize_procedure",
      airport_id: airportId,
      procedure_id: procedureId,
      procedure_kind: kind,
      runway_transition: runwayTransition,
      enroute_transition: enrouteTransition,
      component_index: componentIndex,
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

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = () => import("@generated/app_wasm.js"),
): Promise<LoadedAdapter> {
  const mod = (await debugTiming("wasm.import", importer)) as Partial<WasmModule>;
  if (typeof mod.default === "function") {
    await debugTiming("wasm.init", () => mod.default?.());
  }
  debugLog("wasm.exports.check.start");
  if (
    typeof mod.situation_ring_candidates_json !== "function" ||
    typeof mod.create_ui_session !== "function" ||
    typeof mod.remove_leg_in_session !== "function" ||
    typeof mod.move_waypoint_in_session !== "function" ||
    typeof mod.remove_top_level_waypoint_by_nav_ref_in_session !== "function" ||
    typeof mod.set_situation_in_session !== "function" ||
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
    typeof mod.set_map_layer_visibility_in_session !== "function" ||
    typeof mod.set_map_layer_enabled_in_session !== "function" ||
    typeof mod.set_raster_map_catalog_in_session !== "function" ||
    typeof mod.select_map_in_session !== "function" ||
    typeof mod.replace_flight_plan_in_session !== "function" ||
    typeof mod.set_guidance_leg_geometry_in_session !== "function" ||
    typeof mod.select_airport_in_session !== "function" ||
    typeof mod.select_chart_in_session !== "function" ||
    typeof mod.ingest_point_tiles_in_session !== "function" ||
    typeof mod.ingest_airspace_ref_tiles_in_session !== "function" ||
    typeof mod.ingest_airspace_features_in_session !== "function" ||
    typeof mod.ingest_airspace_label_tiles_in_session !== "function" ||
    typeof mod.ingest_tfrs_in_session !== "function" ||
    typeof mod.get_map_overlay_in_session !== "function" ||
    typeof mod.get_map_selection_in_session !== "function" ||
    typeof mod.get_terrain_overlay_in_session !== "function" ||
    typeof mod.get_raster_tile_plan_in_session !== "function" ||
    typeof mod.render_terrain_overlay_tile_in_session !== "function" ||
    typeof mod.render_terrain_overlay_tiles_in_session !== "function" ||
    typeof mod.get_session_snapshot !== "function" ||
    typeof mod.restore_chart_page_state_in_session !== "function" ||
    typeof mod.destroy_session !== "function" ||
    typeof mod.activate_leg_ui !== "function" ||
    typeof mod.activate_next_leg_ui !== "function" ||
    typeof mod.delete_component_ui !== "function" ||
    typeof mod.move_component_ui !== "function" ||
    typeof mod.insert_waypoint_ui !== "function" ||
    typeof mod.suspend_sequencing_ui !== "function" ||
    typeof mod.unsuspend_sequencing_ui !== "function" ||
    typeof mod.sequence_active_leg_ui !== "function" ||
    typeof mod.insert_airway_materialized_ui !== "function" ||
    typeof mod.replace_airway_materialized_ui !== "function" ||
    typeof mod.insert_procedure_materialized_ui !== "function" ||
    typeof mod.replace_procedure_materialized_ui !== "function" ||
    typeof mod.nav_kv_open !== "function" ||
    typeof mod.nav_kv_insert_page !== "function" ||
    typeof mod.nav_kv_destroy !== "function" ||
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
