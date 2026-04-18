import type {
  AppState,
  UiSnapshotAppState,
  AppUiState,
  AirwayAutoSelection,
  AirwayBranch,
  AirwayEntryCandidate,
  AirwayExitCandidate,
  AirwayPresentationPlan,
  AirwaySuggestion,
  AirwaySegment,
  CatalogJson,
  CifpTppMatch,
  ChartPageData,
  ContentInventory,
  ContentPolicy,
  FlightPlan,
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
  ChartFamilyId,
  ContentAvailability,
  GuidanceLegGeometry,
  GuidanceState,
  LatLon,
  MapFollowUiState,
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
  SituationSample,
  WaypointIdentifierSuggestion,
} from "./types";
import { sampleCatalog } from "./sampleData";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import { installBrowserNavDbQueryHost } from "./webNavDb";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
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
};

export type UiChartPageState = {
  ordered_airport_ids: string[];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
};

type FlightPlanDisplayRowWithShowPlateTarget = FlightPlanUiState["display_rows"][number] & {
  show_plate_target_id?: string | null;
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
    longest_runway_length_ft?: number | null;
    longest_runway_heading_true_deg?: number | null;
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
  screen_x: number;
  screen_y: number;
  towered: boolean;
  fuel_available: boolean;
  runway_length_ratio: number;
  longest_runway_heading_true_deg: number | null;
};

export type MapOverlayQueryResult = {
  needed_point_tiles: VectorTileRequest[];
  visible_features: VisibleMapFeature[];
  warnings: Array<{
    code: string;
    message: string;
  }>;
};

export interface UiSession {
  chartCatalog: ChartPageData;
  snapshot(): Promise<UiSessionSnapshot>;
  replaceFlightPlan(plan: FlightPlan): Promise<UiSessionSnapshot>;
  removeLeg(index: number): Promise<UiSessionSnapshot>;
  moveWaypoint(index: number, delta: number): Promise<UiSessionSnapshot>;
  setGuidanceLegGeometry(geometries: GuidanceLegGeometry[]): Promise<UiSessionSnapshot>;
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
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  queryMapOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<MapOverlayQueryResult>;
  restoreChartPageState(recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<UiSessionSnapshot>;
  destroy(): Promise<void>;
}

export interface AppCoreAdapter {
  prewarm(): Promise<void>;
  createUiSession(
    resourceIndex: unknown,
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession>;
  replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState>;
  removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan>;
  deriveChartCatalog(resourceIndex: unknown): Promise<ChartPageData>;
  deriveChartPage(resourceIndex: unknown, plan: FlightPlan): Promise<ChartPageData>;
  deriveChartPageState(resourceIndex: unknown, plan: FlightPlan, recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<DerivedChartPageState>;
  setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState>;
  refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState>;
  projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]>;
  activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation>;
  activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  deleteComponentUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation>;
  moveComponentUi(plan: FlightPlan, componentIndex: number, delta: number): Promise<FlightPlanUiMutation>;
  resolveWaypointIdentifier(identifier: string): Promise<NavRef | null>;
  suggestWaypointIdentifiers(plan: FlightPlan, componentIndex: number, before: boolean, prefix: string, limit?: number): Promise<WaypointIdentifierSuggestion[]>;
  insertWaypointUi(plan: FlightPlan, componentIndex: number, before: boolean, waypoint: NavRef): Promise<FlightPlanUiMutation>;
  suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  prepareAirwayPresentation(
    airwayName: string,
    branches: AirwayBranch[],
    originPosition: LatLon,
    destinationPosition: LatLon | null,
  ): Promise<AirwayPresentationPlan>;
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
  sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]>;
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

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  create_ui_session(catalogJson: string, chartCatalogJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  remove_leg_in_session(handle: number, index: number): Promise<string> | string;
  move_waypoint_in_session(handle: number, waypointIndex: number, delta: number): Promise<string> | string;
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
  replace_flight_plan_in_session(handle: number, planJson: string): Promise<string> | string;
  set_guidance_leg_geometry_in_session(handle: number, geometriesJson: string): Promise<string> | string;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<string> | string;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<string> | string;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_session_snapshot(handle: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  destroy_session(handle: number): void;
  remove_flight_plan_leg(planJson: string, index: number): Promise<string> | string;
  derive_chart_catalog(resourceIndexJson: string): Promise<string> | string;
  derive_chart_page(resourceIndexJson: string, planJson: string): Promise<string> | string;
  derive_chart_page_state(resourceIndexJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  replace_flight_plan_state(stateJson: string, catalogJson: string, planJson: string): Promise<string> | string;
  set_content_policy_state(stateJson: string, catalogJson: string, policyJson: string): Promise<string> | string;
  refresh_content_state(stateJson: string, catalogJson: string, inventoryJson: string): Promise<string> | string;
  activate_leg_ui(planJson: string, legIndex: number): Promise<string> | string;
  activate_next_leg_ui(planJson: string): Promise<string> | string;
  delete_component_ui(planJson: string, componentIndex: number): Promise<string> | string;
  move_component_ui(planJson: string, componentIndex: number, delta: number): Promise<string> | string;
  insert_waypoint_ui(planJson: string, componentIndex: number, before: boolean, waypointJson: string): Promise<string> | string;
  suspend_sequencing_ui(planJson: string): Promise<string> | string;
  unsuspend_sequencing_ui(planJson: string): Promise<string> | string;
  sequence_active_leg_ui(planJson: string): Promise<string> | string;
  prepare_airway_presentation(
    airwayName: string,
    branchesJson: string,
    originPositionJson: string,
    destinationPositionJson: string,
  ): Promise<string> | string;
  sort_airway_suggestions_for_ui(suggestionsJson: string): Promise<string> | string;
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
  describe_procedure_options_from_rows(
    airportId: string,
    procedureId: string,
    kindJson: string,
    rowsJson: string,
  ): Promise<string> | string;
  list_approach_procedures_from_match_rows(
    airportId: string,
    rowsJson: string,
  ): Promise<string> | string;
  materialize_procedure_from_records(
    airportId: string,
    procedureId: string,
    kindJson: string,
    runwayTransitionJson: string,
    enrouteTransitionJson: string,
    componentIndex: number,
    rowsJson: string,
    legsJson: string,
  ): Promise<string> | string;
  select_preferred_cifp_tpp_match(rowsJson: string): Promise<string> | string;
  describe_show_plate_for_procedure(rowsJson: string): Promise<string> | string;
  describe_load_procedure_from_plate(
    planJson: string,
    airportId: string,
    procedureId: string,
    kindJson: string,
    optionsJson: string,
  ): Promise<string> | string;
  describe_plate_procedure_load_options(
    planJson: string,
    candidatesJson: string,
  ): Promise<string> | string;
  web_project_flight_plan_route(planJson: string): Promise<string> | string;
  web_suggest_airways_near(anchorJson: string, limit: number): Promise<string> | string;
  web_prepare_airway_presentation_for_anchors(
    airwayName: string,
    originAnchorJson: string,
    destinationAnchorJson: string,
  ): Promise<string> | string;
  web_materialize_airway_selection(
    startComponentIndex: number,
    entryJson: string,
    exitJson: string,
    originAnchorJson: string,
    destinationAnchorJson: string,
  ): Promise<string> | string;
  web_resolve_waypoint_identifier(identifier: string): Promise<string> | string;
  web_suggest_waypoint_identifiers(
    planJson: string,
    componentIndex: number,
    before: boolean,
    prefix: string,
    limit: number,
  ): Promise<string> | string;
  web_resolve_nav_symbol_feature(navRefJson: string): Promise<string> | string;
  web_list_procedures(airportId: string, kindJson: string): Promise<string> | string;
  web_describe_procedure_options(
    airportId: string,
    procedureId: string,
    kindJson: string,
  ): Promise<string> | string;
  web_materialize_procedure(
    airportId: string,
    procedureId: string,
    kindJson: string,
    runwayTransitionJson: string,
    enrouteTransitionJson: string,
    componentIndex: number,
  ): Promise<string> | string;
  web_find_procedure_plate_match(airportId: string, cifpId: string): Promise<string> | string;
  web_describe_plate_procedure_loads(planJson: string, plateId: string): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  private navDbHostPromise: Promise<void> | null = null;

  constructor(private readonly module: WasmModule) {}

  async prewarm(): Promise<void> {
    await this.ensureNavDbHost();
  }

  private ensureNavDbHost(): Promise<void> {
    if (this.navDbHostPromise === null) {
      this.navDbHostPromise = installBrowserNavDbQueryHost();
    }
    return this.navDbHostPromise;
  }

  private async enrichFlightPlanUiState(plan: FlightPlan, uiState: FlightPlanUiState): Promise<FlightPlanUiState> {
    await this.ensureNavDbHost();
    const routeSegments = JSON.parse(
      await this.module.web_project_flight_plan_route(JSON.stringify(plan)),
    ) as FlightPlanRouteSegment[];
    const display_rows = await Promise.all(uiState.display_rows.map(async (row) => {
      const showPlateAction = row.actions.find((action) => action.id === "show_plate");
      const legMetrics = row.leg_index !== null ? routeSegments[row.leg_index] ?? null : null;
      const symbol_feature = row.nav_ref
        ? JSON.parse(
          await this.module.web_resolve_nav_symbol_feature(JSON.stringify(row.nav_ref)),
        ) as NavSymbolFeature | null
        : null;
      if (!showPlateAction || !row.chart_airport_id || !row.procedure_id) {
        return {
          ...row,
          symbol_feature,
          distance_nm: legMetrics?.distance_nm ?? null,
          course_deg: legMetrics?.course_deg ?? null,
        } as FlightPlanDisplayRowWithShowPlateTarget;
      }
      const match = JSON.parse(
        await this.module.web_find_procedure_plate_match(row.chart_airport_id, row.procedure_id),
      ) as CifpTppMatch | null;
      return {
        ...row,
        symbol_feature,
        distance_nm: legMetrics?.distance_nm ?? null,
        course_deg: legMetrics?.course_deg ?? null,
        show_plate_target_id: match?.plate_id ?? null,
        actions: row.actions.map((action) =>
          action.id === "show_plate"
            ? { ...action, enabled: match !== null }
            : action,
        ),
      } satisfies FlightPlanDisplayRowWithShowPlateTarget;
    }));
    return {
      ...uiState,
      display_rows,
    };
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
    return {
      ...mutation,
      ui_state: await this.enrichFlightPlanUiState(mutation.plan, mutation.ui_state),
    };
  }

  async createUiSession(
    resourceIndex: unknown,
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession> {
    const catalogJson = JSON.stringify(sampleCatalogLike(resourceIndex));
    const chartCatalog = await this.deriveChartCatalog(resourceIndex);
    const chartCatalogJson = JSON.stringify(chartCatalog);
    const module = this.module;
    const createSession = async (
      nextPlan: FlightPlan,
      nextRecentAirportIds: string[],
      nextSelectedAirportId?: string,
      nextSelectedChartId?: string,
    ) => {
      const created = JSON.parse(
        await module.create_ui_session(
          catalogJson,
          chartCatalogJson,
          JSON.stringify(nextPlan),
          JSON.stringify(nextRecentAirportIds),
          JSON.stringify(nextSelectedAirportId ?? null),
          JSON.stringify(nextSelectedChartId ?? null),
        ),
      ) as { handle: number; chart_catalog: ChartPageData; snapshot: UiSessionSnapshot };
      return {
        ...created,
        snapshot: await this.enrichUiSessionSnapshot(created.snapshot),
      };
    };
    const init = await createSession(plan, recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    let snapshot = init.snapshot;
    const parseSessionSnapshot = async (json: Promise<string> | string) =>
      this.enrichUiSessionSnapshot(JSON.parse(await json) as UiSessionSnapshot);
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
    return {
      chartCatalog: init.chart_catalog,
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
        return snapshot;
      },
      removeLeg: async (index) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.remove_leg_in_session(handle, index)),
        );
        return snapshot;
      },
      moveWaypoint: async (index, delta) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.move_waypoint_in_session(handle, index, delta)),
        );
        return snapshot;
      },
      setGuidanceLegGeometry: async (geometries) => {
        snapshot = await withSessionRetry(async () =>
          parseSessionSnapshot(this.module.set_guidance_leg_geometry_in_session(handle, JSON.stringify(geometries))),
        );
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

  async removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan> {
    return JSON.parse(
      await this.module.remove_flight_plan_leg(
        JSON.stringify(plan),
        index,
      ),
    ) as FlightPlan;
  }

  async deriveChartPage(resourceIndex: unknown, plan: FlightPlan): Promise<ChartPageData> {
    return JSON.parse(
      await this.module.derive_chart_page(
        JSON.stringify(resourceIndex),
        JSON.stringify(plan),
      ),
    ) as ChartPageData;
  }

  async deriveChartCatalog(resourceIndex: unknown): Promise<ChartPageData> {
    return JSON.parse(
      await this.module.derive_chart_catalog(
        JSON.stringify(resourceIndex),
      ),
    ) as ChartPageData;
  }

  async deriveChartPageState(resourceIndex: unknown, plan: FlightPlan, recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<DerivedChartPageState> {
    return JSON.parse(
      await this.module.derive_chart_page_state(
        JSON.stringify(resourceIndex),
        JSON.stringify(plan),
        JSON.stringify(recentAirportIds),
        JSON.stringify(selectedAirportId ?? null),
        JSON.stringify(selectedChartId ?? null),
      ),
    ) as DerivedChartPageState;
  }

  async replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState> {
    return JSON.parse(
      await this.module.replace_flight_plan_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(plan),
      ),
    ) as AppState;
  }

  async setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState> {
    return JSON.parse(
      await this.module.set_content_policy_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(policy),
      ),
    ) as AppState;
  }

  async refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState> {
    return JSON.parse(
      await this.module.refresh_content_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(inventory),
      ),
    ) as AppState;
  }

  async projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]> {
    void planUiState;
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_project_flight_plan_route(JSON.stringify(plan)),
    ) as FlightPlanRouteSegment[];
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

  async resolveWaypointIdentifier(identifier: string): Promise<NavRef | null> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_resolve_waypoint_identifier(identifier),
    ) as NavRef | null;
  }

  async suggestWaypointIdentifiers(
    plan: FlightPlan,
    componentIndex: number,
    before: boolean,
    prefix: string,
    limit = 8,
  ): Promise<WaypointIdentifierSuggestion[]> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_suggest_waypoint_identifiers(
        JSON.stringify(plan),
        componentIndex,
        before,
        prefix,
        limit,
      ),
    ) as WaypointIdentifierSuggestion[];
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

  async prepareAirwayPresentation(
    airwayName: string,
    branches: AirwayBranch[],
    originPosition: LatLon,
    destinationPosition: LatLon | null,
  ): Promise<AirwayPresentationPlan> {
    return JSON.parse(
      await this.module.prepare_airway_presentation(
        airwayName,
        JSON.stringify(branches),
        JSON.stringify(originPosition),
        JSON.stringify(destinationPosition),
      ),
    ) as AirwayPresentationPlan;
  }

  async suggestAirwaysNearAnchor(anchor: NavRef, limit = 5): Promise<AirwaySuggestion[]> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_suggest_airways_near(JSON.stringify(anchor), limit),
    ) as AirwaySuggestion[];
  }

  async prepareAirwayPresentationForAnchors(
    airwayName: string,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<AirwayPresentationPlan> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_prepare_airway_presentation_for_anchors(
        airwayName,
        JSON.stringify(originAnchor),
        JSON.stringify(destinationAnchor),
      ),
    ) as AirwayPresentationPlan;
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
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_materialize_airway_selection(
        startComponentIndex,
        JSON.stringify(entry),
        JSON.stringify(exit),
        JSON.stringify(originAnchor),
        JSON.stringify(destinationAnchor),
      ),
    ) as {
      selection: AirwayAutoSelection;
      airway: AirwaySegment;
      resolvedLegs: ResolvedLeg[];
    };
  }

  async sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]> {
    return JSON.parse(
      await this.module.sort_airway_suggestions_for_ui(JSON.stringify(suggestions)),
    ) as AirwaySuggestion[];
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
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_list_procedures(airportId, JSON.stringify(kind)),
    ) as ProcedureSummary[];
  }

  async describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_describe_procedure_options(
        airportId,
        procedureId,
        JSON.stringify(kind),
      ),
    ) as ProcedureOptions;
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
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_materialize_procedure(
        airportId,
        procedureId,
        JSON.stringify(kind),
        JSON.stringify(runwayTransition),
        JSON.stringify(enrouteTransition),
        componentIndex,
      ),
    ) as MaterializedProcedure;
  }

  async findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_find_procedure_plate_match(airportId, cifpId),
    ) as CifpTppMatch | null;
  }

  async describePlateProcedureLoads(plan: FlightPlan, plateId: string): Promise<ProcedureLoadOption[]> {
    await this.ensureNavDbHost();
    return JSON.parse(
      await this.module.web_describe_plate_procedure_loads(
        JSON.stringify(plan),
        plateId,
      ),
    ) as ProcedureLoadOption[];
  }
}

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = () => import("@generated/app_wasm.js"),
): Promise<LoadedAdapter> {
  const mod = (await importer()) as Partial<WasmModule>;
  if (typeof mod.default === "function") {
    await mod.default();
  }
  if (
    typeof mod.create_ui_session !== "function" ||
    typeof mod.remove_leg_in_session !== "function" ||
    typeof mod.move_waypoint_in_session !== "function" ||
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
    typeof mod.replace_flight_plan_in_session !== "function" ||
    typeof mod.set_guidance_leg_geometry_in_session !== "function" ||
    typeof mod.select_airport_in_session !== "function" ||
    typeof mod.select_chart_in_session !== "function" ||
    typeof mod.ingest_point_tiles_in_session !== "function" ||
    typeof mod.get_map_overlay_in_session !== "function" ||
    typeof mod.get_session_snapshot !== "function" ||
    typeof mod.restore_chart_page_state_in_session !== "function" ||
    typeof mod.destroy_session !== "function" ||
    typeof mod.replace_flight_plan_state !== "function" ||
    typeof mod.remove_flight_plan_leg !== "function" ||
    typeof mod.activate_leg_ui !== "function" ||
    typeof mod.activate_next_leg_ui !== "function" ||
    typeof mod.delete_component_ui !== "function" ||
    typeof mod.move_component_ui !== "function" ||
    typeof mod.insert_waypoint_ui !== "function" ||
    typeof mod.suspend_sequencing_ui !== "function" ||
    typeof mod.unsuspend_sequencing_ui !== "function" ||
    typeof mod.sequence_active_leg_ui !== "function" ||
    typeof mod.prepare_airway_presentation !== "function" ||
    typeof mod.sort_airway_suggestions_for_ui !== "function" ||
    typeof mod.insert_airway_materialized_ui !== "function" ||
    typeof mod.replace_airway_materialized_ui !== "function" ||
    typeof mod.insert_procedure_materialized_ui !== "function" ||
    typeof mod.replace_procedure_materialized_ui !== "function" ||
    typeof mod.describe_procedure_options_from_rows !== "function" ||
    typeof mod.list_approach_procedures_from_match_rows !== "function" ||
    typeof mod.materialize_procedure_from_records !== "function" ||
    typeof mod.select_preferred_cifp_tpp_match !== "function" ||
    typeof mod.describe_show_plate_for_procedure !== "function" ||
    typeof mod.describe_load_procedure_from_plate !== "function" ||
    typeof mod.describe_plate_procedure_load_options !== "function" ||
    typeof mod.web_project_flight_plan_route !== "function" ||
    typeof mod.web_suggest_airways_near !== "function" ||
    typeof mod.web_prepare_airway_presentation_for_anchors !== "function" ||
    typeof mod.web_materialize_airway_selection !== "function" ||
    typeof mod.web_resolve_waypoint_identifier !== "function" ||
    typeof mod.web_suggest_waypoint_identifiers !== "function" ||
    typeof mod.web_resolve_nav_symbol_feature !== "function" ||
    typeof mod.web_list_procedures !== "function" ||
    typeof mod.web_describe_procedure_options !== "function" ||
    typeof mod.web_materialize_procedure !== "function" ||
    typeof mod.web_find_procedure_plate_match !== "function" ||
    typeof mod.web_describe_plate_procedure_loads !== "function" ||
    typeof mod.derive_chart_catalog !== "function" ||
    typeof mod.derive_chart_page !== "function" ||
    typeof mod.derive_chart_page_state !== "function" ||
    typeof mod.set_content_policy_state !== "function" ||
    typeof mod.refresh_content_state !== "function"
  ) {
    throw new Error("generated wasm module is missing required exports");
  }

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

function sampleCatalogLike(_resourceIndex: unknown): CatalogJson {
  return {
    ...({
      schema_version: sampleCatalog.schema_version,
      cycle: sampleCatalog.cycle,
      catalog_revision: sampleCatalog.catalog_revision,
      families: sampleCatalog.families,
      regions: sampleCatalog.regions,
      packages: sampleCatalog.packages,
      charts: sampleCatalog.charts,
      plates: sampleCatalog.plates,
      supplements: sampleCatalog.supplements,
    } as CatalogJson),
  };
}

function moveAirportToFront(
  recentAirportIds: string[],
  airportId: string,
  airports: ChartPageData["airports"],
): string[] {
  const validIds = new Set(airports.map((airport) => airport.id));
  const next = [airportId, ...recentAirportIds.filter((id) => id !== airportId && validIds.has(id))];
  for (const airport of airports) {
    if (!next.includes(airport.id)) {
      next.push(airport.id);
    }
  }
  return next;
}

function compactChartPageState(state: DerivedChartPageState): UiChartPageState {
  return {
    ordered_airport_ids: state.airports.map((airport) => airport.id),
    recent_airport_ids: state.recent_airport_ids,
    selected_airport_id: state.selected_airport_id,
    selected_chart_id: state.selected_chart_id,
  };
}
