import type {
  AppState,
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
  CifpTppMatchRow,
  ChartPageData,
  ContentInventory,
  ContentPolicy,
  FlightPlan,
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
  ChartFamilyId,
  ContentAvailability,
  GuidanceState,
  LatLon,
  MapFollowUiState,
  MaterializedProcedure,
  NavRef,
  PlanLeg,
  PlaybackUiState,
  PlateProcedureLoadCandidateInput,
  ProcedureLoadOption,
  ProcedureOptions,
  ProcedureSummary,
  ResolvedLeg,
  ResolvedLegUiView,
  RouteComponentUiView,
  SequencingMode,
  Situation,
} from "./types";
import { deriveChartPage as deriveChartCatalog } from "./resourceIndexAdapters";
import {
  loadCifpTppMatchesForAirport,
  loadCifpTppMatchesForPlate,
  loadCifpTppMatchesForProcedure,
  listProceduresForAirport,
  loadProcedureDistinctRows,
  loadProcedureMaterializationRecords,
} from "./procedurePlanner";
import { sampleCatalog } from "./sampleData";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";
import {
  materializeAirwaySelection as materializeAirwaySelectionWithNavDb,
  prepareAirwayPresentationForAnchors as prepareAirwayPresentationForAnchorsWithNavDb,
  resolveNavRefPosition,
  suggestAirwaysNearAnchor as suggestAirwaysNearAnchorWithNavDb,
} from "./airwayPlanner";
import { getBrowserNavDb } from "./webNavDb";
import { debugLog } from "./debugLog";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
};

export type UiSessionSnapshot = {
  app_state: AppState;
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
  deriveChartPage(resourceIndex: unknown, plan: FlightPlan): Promise<ChartPageData>;
  deriveChartPageState(resourceIndex: unknown, plan: FlightPlan, recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<DerivedChartPageState>;
  setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState>;
  refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState>;
  buildFlightPlanUi(plan: FlightPlan): Promise<FlightPlanUiState>;
  projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]>;
  activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation>;
  activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  deleteComponentUi(plan: FlightPlan, componentIndex: number): Promise<FlightPlanUiMutation>;
  moveComponentUi(plan: FlightPlan, componentIndex: number, delta: number): Promise<FlightPlanUiMutation>;
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

function packageName(region: string, family: string): string {
  const regionCode = region.toUpperCase();
  const familyCode =
    family === "sec"
      ? "SEC"
      : family === "enr-l"
        ? "ENR_L"
        : family === "enr-h"
          ? "ENR_H"
          : family === "enr-a"
            ? "ENR_A"
            : family.toUpperCase();
  return `${regionCode}_${familyCode}`;
}

function airportCode(ref: FlightPlan["legs"][number]["from"] | null | undefined): string | null {
  if (ref && "Airport" in ref) {
    return ref.Airport;
  }
  return null;
}

export class MockAppCoreAdapter implements AppCoreAdapter {
  async prewarm(): Promise<void> {}

  async createUiSession(
    resourceIndex: unknown,
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession> {
    const adapter = this;
    let appState = await this.replaceFlightPlanState(
      {
        active_plan: null,
        situation: { position: { kind: "unknown" }, orientation_deg: null, speed_kt: null },
        content_policy: "PreferLocal",
        last_content_requirements: [],
        last_content_report: null,
      },
      sampleCatalogLike(resourceIndex),
      plan,
    );
    const chartCatalog = await this.deriveChartPage(
      resourceIndex,
      plan,
    );
    let chartPageState = compactChartPageState(await this.deriveChartPageState(
      resourceIndex,
      plan,
      recentAirportIds,
      selectedAirportId,
      selectedChartId,
    ));
    let playbackUiState: PlaybackUiState = {
      status: "empty",
      source_path: null,
      registration: null,
      icao: null,
      aircraft_type: null,
      point_count: 0,
      duration_seconds: 0,
      cursor_seconds: 0,
      rate: 1,
      speed_profile_norm: [],
      altitude_profile_norm: [],
    };
    let mapFollowUiState: MapFollowUiState = {
      can_center_here: appState.situation.position.kind !== "unknown",
      following: false,
    };
    const snapshotWithUi = async (): Promise<UiSessionSnapshot> => ({
      app_state: appState,
      app_ui_state: {
        active_plan: appState.active_plan ? await adapter.buildFlightPlanUi(appState.active_plan) : null,
        content_policy: appState.content_policy,
        last_content_requirements: appState.last_content_requirements,
        last_content_report: appState.last_content_report,
      },
      playback_ui_state: playbackUiState,
      map_follow_ui_state: mapFollowUiState,
      map_follow_target_viewport: null,
      chart_page_state: chartPageState,
    });

    return {
      chartCatalog,
      snapshot: async () => snapshotWithUi(),
      replaceFlightPlan: async (nextPlan) => {
        appState = await adapter.replaceFlightPlanState(appState, sampleCatalogLike(resourceIndex), nextPlan);
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          nextPlan,
          chartPageState.recent_airport_ids,
          chartPageState.selected_airport_id,
          chartPageState.selected_chart_id,
        ));
        return snapshotWithUi();
      },
      removeLeg: async (index) => {
        const nextPlan = await adapter.removeFlightPlanLeg(appState.active_plan ?? plan, index);
        appState = await adapter.replaceFlightPlanState(appState, sampleCatalogLike(resourceIndex), nextPlan);
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          nextPlan,
          chartPageState.recent_airport_ids,
          chartPageState.selected_airport_id,
          chartPageState.selected_chart_id,
        ));
        return snapshotWithUi();
      },
      moveWaypoint: async (index, delta) => {
        const nextPlan = moveWaypointInPlan(appState.active_plan ?? plan, index, delta);
        appState = await adapter.replaceFlightPlanState(appState, sampleCatalogLike(resourceIndex), nextPlan);
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          nextPlan,
          chartPageState.recent_airport_ids,
          chartPageState.selected_airport_id,
          chartPageState.selected_chart_id,
        ));
        return snapshotWithUi();
      },
      setSituation: async (situation) => {
        appState = { ...appState, situation };
        return snapshotWithUi();
      },
      loadPlaybackTrace: async () => {
        throw new Error("playback trace loading requires wasm adapter");
      },
      playPlayback: async () => snapshotWithUi(),
      pausePlayback: async () => snapshotWithUi(),
      seekPlayback: async () => snapshotWithUi(),
      setPlaybackRate: async (rate) => {
        playbackUiState = { ...playbackUiState, rate };
        return snapshotWithUi();
      },
      tickPlayback: async () => snapshotWithUi(),
      engageMapFollow: async () => {
        mapFollowUiState = { ...mapFollowUiState, following: true };
        return snapshotWithUi();
      },
      disengageMapFollow: async () => {
        mapFollowUiState = { ...mapFollowUiState, following: false };
        return snapshotWithUi();
      },
      setMapFollowOffset: async () => snapshotWithUi(),
      selectAirport: async (airportId) => {
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          appState.active_plan ?? plan,
          moveAirportToFront(chartPageState.recent_airport_ids, airportId, chartCatalog.airports),
          airportId,
          undefined,
        ));
        return snapshotWithUi();
      },
      selectChart: async (chartId) => {
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          appState.active_plan ?? plan,
          chartPageState.recent_airport_ids,
          chartPageState.selected_airport_id,
          chartId,
        ));
        return snapshotWithUi();
      },
      ingestPointTiles: async () => {},
      queryMapOverlay: async () => ({
        needed_point_tiles: [],
        visible_features: [],
        warnings: [],
      }),
      restoreChartPageState: async (nextRecentAirportIds, nextSelectedAirportId, nextSelectedChartId) => {
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          appState.active_plan ?? plan,
          nextRecentAirportIds,
          nextSelectedAirportId,
          nextSelectedChartId,
        ));
        return snapshotWithUi();
      },
      destroy: async () => {},
    };
  }

  async replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState> {
    if (plan.legs.length === 0) {
      throw new Error("InvalidFlightPlan: flight plan must contain at least one leg");
    }

    const packageMap = new Map<
      string,
      CatalogJson["packages"][number]["id"]
    >();

    for (const leg of plan.legs) {
      for (const ref of [leg.from, leg.to]) {
        const code = airportCode(ref);
        if (!code) continue;
        for (const plate of catalog.plates) {
          if (plate.airport_id.toUpperCase() !== code.toUpperCase()) continue;
          const pkg = catalog.packages.find((entry) => entry.region_id === plate.region_id);
          if (pkg) {
            packageMap.set(JSON.stringify(pkg.id), pkg.id);
          }
        }
      }
    }

    return {
      ...state,
      active_plan: plan,
      last_content_requirements: [
        {
          package_ids: [...packageMap.values()],
          chart_ids: [],
          plate_ids: [],
        },
      ],
      last_content_report: null,
    };
  }

  async removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan> {
    if (index < 0 || index >= plan.legs.length) {
      throw new Error(`InvalidFlightPlan: flight plan leg index out of range: ${index}`);
    }
    const legs = plan.legs.filter((_, legIndex) => legIndex !== index);
    if (legs.length === 0) {
      throw new Error("InvalidFlightPlan: flight plan must contain at least one leg");
    }
    return {
      ...plan,
      legs,
      departure: airportCode(legs[0]?.from ?? null),
      destination: airportCode(legs[legs.length - 1]?.to ?? null),
      updated_at_epoch_ms: plan.updated_at_epoch_ms + 1,
      version: plan.version + 1,
    };
  }

  async deriveChartPage(resourceIndex: unknown, plan: FlightPlan): Promise<ChartPageData> {
    const { deriveChartPage } = await import("./resourceIndexAdapters");
    return deriveChartPage(resourceIndex as Parameters<typeof deriveChartPage>[0], plan);
  }

  async deriveChartPageState(resourceIndex: unknown, plan: FlightPlan, recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<DerivedChartPageState> {
    const chartPage = await this.deriveChartPage(resourceIndex, plan);
    const airports = chartPage.airports;
    const validIds = new Set(airports.map((airport) => airport.id));
    const mergedRecentIds = recentAirportIds.filter((id, index) => validIds.has(id) && recentAirportIds.indexOf(id) === index);
    for (const airport of airports) {
        if (!mergedRecentIds.includes(airport.id)) {
          mergedRecentIds.push(airport.id);
        }
    }
    const resolvedAirportId =
      selectedAirportId && airports.some((airport) => airport.id === selectedAirportId)
        ? selectedAirportId
        : mergedRecentIds[0] ?? airports[0]?.id ?? "";
    const resolvedChartId =
      selectedChartId && airports.find((airport) => airport.id === resolvedAirportId)?.charts.some((chart) => chart.id === selectedChartId)
        ? selectedChartId
        : airports.find((airport) => airport.id === resolvedAirportId)?.charts[0]?.id ?? "";
    const airportById = new Map(airports.map((airport) => [airport.id, airport]));
    return {
      airports: mergedRecentIds.map((airportId) => airportById.get(airportId)).filter((airport): airport is ChartPageData["airports"][number] => airport !== undefined),
      recent_airport_ids: mergedRecentIds,
      selected_airport_id: resolvedAirportId,
      selected_chart_id: resolvedChartId,
    };
  }

  async setContentPolicyState(state: AppState, _catalog: CatalogJson, policy: ContentPolicy): Promise<AppState> {
    return {
      ...state,
      content_policy: policy,
    };
  }

  async refreshContentState(state: AppState, _catalog: CatalogJson, inventory: ContentInventory): Promise<AppState> {
    const items = state.last_content_requirements.flatMap((requirement) =>
      requirement.package_ids.map((pkg) => {
        const installed = inventory.installed_packages.some(
          (entry) =>
            entry.integrity_ok &&
            entry.package_id.region === pkg.region &&
            entry.package_id.family === pkg.family &&
            entry.package_id.cycle === pkg.cycle,
        );

        const availability: ContentAvailability =
          installed
            ? state.content_policy === "StreamAllowed"
              ? "LocalAndRemote"
              : "LocalOnly"
            : state.content_policy === "StreamAllowed"
              ? "RemoteOnly"
              : "Unavailable";

        return {
          label: packageName(pkg.region, pkg.family),
          availability: {
            availability,
            cycle_current: true,
            integrity_ok: installed,
            cached: installed,
            offline_usable: installed,
          },
        };
      }),
    );

    const fullySatisfied = items.every((item) =>
      state.content_policy === "StreamAllowed"
        ? item.availability.availability !== "Unavailable"
        : item.availability.availability === "LocalOnly" || item.availability.availability === "LocalAndRemote",
    );

    return {
      ...state,
      last_content_report: {
        fully_satisfied: fullySatisfied,
        items,
      },
    };
  }

  async buildFlightPlanUi(plan: FlightPlan): Promise<FlightPlanUiState> {
    void plan;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan UI; use the wasm adapter");
  }

  async projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]> {
    void plan;
    void planUiState;
    throw new Error("MockAppCoreAdapter no longer supports route projection; use the wasm adapter");
  }

  async activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation> {
    void plan;
    void legIndex;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan mutations; use the wasm adapter");
  }

  async activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    void plan;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan mutations; use the wasm adapter");
  }

  async deleteComponentUi(): Promise<FlightPlanUiMutation> {
    throw new Error("delete component requires wasm adapter");
  }

  async moveComponentUi(): Promise<FlightPlanUiMutation> {
    throw new Error("move component requires wasm adapter");
  }

  async suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    void plan;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan mutations; use the wasm adapter");
  }

  async unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    void plan;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan mutations; use the wasm adapter");
  }

  async sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    void plan;
    throw new Error("MockAppCoreAdapter no longer supports flight-plan mutations; use the wasm adapter");
  }

  async prepareAirwayPresentation(
    airwayName: string,
    branches: AirwayBranch[],
    _originPosition: LatLon,
    destinationPosition: LatLon | null,
  ): Promise<AirwayPresentationPlan> {
    const branch = branches.find((candidate) => candidate.display_name === airwayName) ?? branches[0];
    if (!branch) {
      throw new Error(`no airway branches found for ${airwayName}`);
    }
    return {
      airway_name: branch.display_name,
      branch_key: branch.branch_key,
      points: branch.points.map((point, index) => ({
        branch_point_index: index,
        sequence: point.sequence,
        nav_ref: point.nav_ref,
      })),
      suggested_entry_index: 0,
      suggested_exit_index: destinationPosition ? Math.max(branch.points.length - 1, 0) : null,
    };
  }

  async suggestAirwaysNearAnchor(): Promise<AirwaySuggestion[]> {
    throw new Error("MockAppCoreAdapter no longer supports airway planning; use the wasm adapter");
  }

  async prepareAirwayPresentationForAnchors(): Promise<AirwayPresentationPlan> {
    throw new Error("MockAppCoreAdapter no longer supports airway planning; use the wasm adapter");
  }

  async materializeAirwaySelection(): Promise<{
    selection: AirwayAutoSelection;
    airway: AirwaySegment;
    resolvedLegs: ResolvedLeg[];
  }> {
    throw new Error("MockAppCoreAdapter no longer supports airway planning; use the wasm adapter");
  }

  async sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]> {
    return [...suggestions].sort((left, right) => left.airway_name.localeCompare(right.airway_name));
  }

  async insertAirwayMaterializedUi(): Promise<FlightPlanUiMutation> {
    throw new Error("airway insertion requires wasm adapter");
  }

  async replaceAirwayMaterializedUi(): Promise<FlightPlanUiMutation> {
    throw new Error("airway replacement requires wasm adapter");
  }

  async listProcedures(_airportId: string, _kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]> {
    return [];
  }

  async describeProcedureOptions(_airportId: string, _procedureId: string, _kind: "sid" | "star" | "approach"): Promise<ProcedureOptions> {
    throw new Error("mock adapter procedure options are unavailable");
  }

  async insertProcedureMaterializedUi(): Promise<FlightPlanUiMutation> {
    throw new Error("procedure insertion requires wasm adapter");
  }

  async replaceProcedureMaterializedUi(): Promise<FlightPlanUiMutation> {
    throw new Error("procedure replacement requires wasm adapter");
  }

  async materializeProcedure(): Promise<MaterializedProcedure> {
    throw new Error("procedure materialization requires wasm adapter");
  }

  async findProcedurePlateMatch(): Promise<CifpTppMatch | null> {
    throw new Error("procedure plate matching requires wasm adapter");
  }

  async describePlateProcedureLoads(): Promise<ProcedureLoadOption[]> {
    throw new Error("plate procedure loading requires wasm adapter");
  }

  async chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null> {
    for (const chart of catalog.charts) {
      if (chart.family_id !== family) {
        continue;
      }

      const coverage = chart.coverage as { kind?: string; value?: { polygon_id?: string } };
      const polygonId = coverage.value?.polygon_id;
      if (coverage.kind !== "polygon_ref" || !polygonId) {
        continue;
      }

      const polygon = geometry.polygons.find((entry) => entry.id === polygonId);
      if (polygon && pointInPolygon(lat, lon, polygon.points)) {
        return chart;
      }
    }

    return null;
  }
}

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  create_ui_session(catalogJson: string, chartCatalogJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  remove_leg_in_session(handle: number, index: number): Promise<string> | string;
  move_waypoint_in_session(handle: number, waypointIndex: number, delta: number): Promise<string> | string;
  set_situation_in_session(handle: number, situationJson: string): Promise<string> | string;
  engage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  disengage_map_follow_in_session(handle: number, viewportJson: string): Promise<string> | string;
  set_map_follow_offset_in_session(handle: number, viewportJson: string, offsetXPx: number, offsetYPx: number): Promise<string> | string;
  load_playback_trace_in_session(handle: number, sourcePathJson: string, traceJson: string): Promise<string> | string;
  play_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  pause_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  seek_playback_in_session(handle: number, cursorSeconds: number, nowEpochMs: number): Promise<string> | string;
  set_playback_rate_in_session(handle: number, rate: number, nowEpochMs: number): Promise<string> | string;
  tick_playback_in_session(handle: number, nowEpochMs: number): Promise<string> | string;
  replace_flight_plan_in_session(handle: number, planJson: string): Promise<string> | string;
  select_airport_in_session(handle: number, airportIdJson: string): Promise<string> | string;
  select_chart_in_session(handle: number, chartIdJson: string): Promise<string> | string;
  ingest_point_tiles_in_session(handle: number, tilesJson: string): Promise<void> | void;
  get_map_overlay_in_session(handle: number, viewportJson: string, widthPx: number, heightPx: number): Promise<string> | string;
  get_session_snapshot(handle: number): Promise<string> | string;
  restore_chart_page_state_in_session(handle: number, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  destroy_session(handle: number): void;
  remove_flight_plan_leg(planJson: string, index: number): Promise<string> | string;
  derive_chart_page(resourceIndexJson: string, planJson: string): Promise<string> | string;
  derive_chart_page_state(resourceIndexJson: string, planJson: string, recentAirportIdsJson: string, selectedAirportIdJson: string, selectedChartIdJson: string): Promise<string> | string;
  replace_flight_plan_state(stateJson: string, catalogJson: string, planJson: string): Promise<string> | string;
  set_content_policy_state(stateJson: string, catalogJson: string, policyJson: string): Promise<string> | string;
  refresh_content_state(stateJson: string, catalogJson: string, inventoryJson: string): Promise<string> | string;
  build_flight_plan_ui(planJson: string): Promise<string> | string;
  activate_leg_ui(planJson: string, legIndex: number): Promise<string> | string;
  activate_next_leg_ui(planJson: string): Promise<string> | string;
  delete_component_ui(planJson: string, componentIndex: number): Promise<string> | string;
  move_component_ui(planJson: string, componentIndex: number, delta: number): Promise<string> | string;
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
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(private readonly module: WasmModule) {}

  async prewarm(): Promise<void> {
    await getBrowserNavDb();
  }

  private async enrichFlightPlanUiState(uiState: FlightPlanUiState): Promise<FlightPlanUiState> {
    const display_rows = await Promise.all(uiState.display_rows.map(async (row) => {
      const showPlateAction = row.actions.find((action) => action.id === "show_plate");
      if (!showPlateAction || !row.chart_airport_id || !row.procedure_id) {
        return row as FlightPlanDisplayRowWithShowPlateTarget;
      }
      const rows = await loadCifpTppMatchesForProcedure(row.chart_airport_id, row.procedure_id);
      const match = JSON.parse(
        await this.module.describe_show_plate_for_procedure(JSON.stringify(rows)),
      ) as CifpTppMatch | null;
      return {
        ...row,
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

  private async enrichFlightPlanUiMutation(mutation: FlightPlanUiMutation): Promise<FlightPlanUiMutation> {
    return {
      ...mutation,
      ui_state: await this.enrichFlightPlanUiState(mutation.ui_state),
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
    const chartCatalog = deriveChartCatalog(resourceIndex as Parameters<typeof deriveChartCatalog>[0], plan);
    const chartCatalogJson = JSON.stringify(chartCatalog);
    const module = this.module;
    const createSession = async (
      nextPlan: FlightPlan,
      nextRecentAirportIds: string[],
      nextSelectedAirportId?: string,
      nextSelectedChartId?: string,
    ) => {
      return JSON.parse(
        await module.create_ui_session(
          catalogJson,
          chartCatalogJson,
          JSON.stringify(nextPlan),
          JSON.stringify(nextRecentAirportIds),
          JSON.stringify(nextSelectedAirportId ?? null),
          JSON.stringify(nextSelectedChartId ?? null),
        ),
      ) as { handle: number; chart_catalog: ChartPageData; snapshot: UiSessionSnapshot };
    };
    const init = await createSession(plan, recentAirportIds, selectedAirportId, selectedChartId);
    let handle = init.handle;
    let snapshot = init.snapshot;
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
      snapshot = JSON.parse(
        await this.module.set_situation_in_session(handle, JSON.stringify(snapshot.app_state.situation)),
      ) as UiSessionSnapshot;
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
          JSON.parse(await this.module.get_session_snapshot(handle)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      replaceFlightPlan: async (plan) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.replace_flight_plan_in_session(handle, JSON.stringify(plan))) as UiSessionSnapshot,
        );
        return snapshot;
      },
      removeLeg: async (index) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.remove_leg_in_session(handle, index)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      moveWaypoint: async (index, delta) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.move_waypoint_in_session(handle, index, delta)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      setSituation: async (situation) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.set_situation_in_session(handle, JSON.stringify(situation))) as UiSessionSnapshot,
        );
        return snapshot;
      },
      engageMapFollow: async (viewport) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(
            await this.module.engage_map_follow_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
            ),
          ) as UiSessionSnapshot,
        );
        return snapshot;
      },
      disengageMapFollow: async (viewport) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(
            await this.module.disengage_map_follow_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
            ),
          ) as UiSessionSnapshot,
        );
        return snapshot;
      },
      setMapFollowOffset: async (viewport, offsetXPx, offsetYPx) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(
            await this.module.set_map_follow_offset_in_session(
              handle,
              JSON.stringify(coreViewportForMap(viewport)),
              offsetXPx,
              offsetYPx,
            ),
          ) as UiSessionSnapshot,
        );
        return snapshot;
      },
      loadPlaybackTrace: async (sourcePath, traceJson) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.load_playback_trace_in_session(handle, JSON.stringify(sourcePath), traceJson)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      playPlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.play_playback_in_session(handle, nowEpochMs)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      pausePlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.pause_playback_in_session(handle, nowEpochMs)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      seekPlayback: async (cursorSeconds, nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.seek_playback_in_session(handle, cursorSeconds, nowEpochMs)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      setPlaybackRate: async (rate, nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.set_playback_rate_in_session(handle, rate, nowEpochMs)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      tickPlayback: async (nowEpochMs) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.tick_playback_in_session(handle, nowEpochMs)) as UiSessionSnapshot,
        );
        return snapshot;
      },
      selectAirport: async (airportId) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.select_airport_in_session(handle, JSON.stringify(airportId))) as UiSessionSnapshot,
        );
        return snapshot;
      },
      selectChart: async (chartId) => {
        snapshot = await withSessionRetry(async () =>
          JSON.parse(await this.module.select_chart_in_session(handle, JSON.stringify(chartId))) as UiSessionSnapshot,
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
          JSON.parse(
            await this.module.restore_chart_page_state_in_session(
              handle,
              JSON.stringify(nextRecentAirportIds),
              JSON.stringify(nextSelectedAirportId ?? null),
              JSON.stringify(nextSelectedChartId ?? null),
            ),
          ) as UiSessionSnapshot,
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

  async buildFlightPlanUi(plan: FlightPlan): Promise<FlightPlanUiState> {
    const uiState = JSON.parse(
      await this.module.build_flight_plan_ui(JSON.stringify(plan)),
    ) as FlightPlanUiState;
    return this.enrichFlightPlanUiState(uiState);
  }

  async projectFlightPlanRoute(plan: FlightPlan, planUiState: FlightPlanUiState | null): Promise<FlightPlanRouteSegment[]> {
    const rawLegs = plan.resolved_legs ?? [];
    const uiLegs = planUiState?.resolved_legs ?? [];
    if (rawLegs.length === 0 || uiLegs.length === 0) {
      return [];
    }
    const resolvedPositions = new Map<string, Promise<LatLon>>();
    const resolveCachedPosition = (navRef: NavRef, procedureAirportId?: string | null) => {
      const key = `${JSON.stringify(navRef)}:${procedureAirportId ?? ""}`;
      let promise = resolvedPositions.get(key);
      if (!promise) {
        promise = resolveNavRefPosition(navRef, procedureAirportId);
        resolvedPositions.set(key, promise);
      }
      return promise;
    };
    return Promise.all(
      rawLegs.map(async (leg, index) => ({
        id: leg.id,
        from: await resolveCachedPosition(leg.from, leg.procedure_airport_id ?? null),
        to: await resolveCachedPosition(leg.to, leg.procedure_airport_id ?? null),
        status: routeStatusForLeg(planUiState, uiLegs[index]?.leg_index ?? index),
      })),
    );
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
    return suggestAirwaysNearAnchorWithNavDb(this, anchor, limit);
  }

  async prepareAirwayPresentationForAnchors(
    airwayName: string,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ): Promise<AirwayPresentationPlan> {
    return prepareAirwayPresentationForAnchorsWithNavDb(this, airwayName, originAnchor, destinationAnchor);
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
    return materializeAirwaySelectionWithNavDb(
      startComponentIndex,
      entry,
      exit,
      originAnchor,
      destinationAnchor,
    );
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
      ui_state: await this.enrichFlightPlanUiState(result.ui_state),
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
      ui_state: await this.enrichFlightPlanUiState(result.ui_state),
    };
  }

  async listProcedures(airportId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureSummary[]> {
    if (kind === "approach") {
      const rows = await loadCifpTppMatchesForAirport(airportId);
      return JSON.parse(
        await this.module.list_approach_procedures_from_match_rows(
          airportId,
          JSON.stringify(rows),
        ),
      ) as ProcedureSummary[];
    }
    return listProceduresForAirport(airportId, kind);
  }

  async describeProcedureOptions(airportId: string, procedureId: string, kind: "sid" | "star" | "approach"): Promise<ProcedureOptions> {
    const rows = await loadProcedureDistinctRows(airportId, procedureId);
    return JSON.parse(
      await this.module.describe_procedure_options_from_rows(
        airportId,
        procedureId,
        JSON.stringify(kind),
        JSON.stringify(rows),
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
      ui_state: await this.enrichFlightPlanUiState(result.ui_state),
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
      ui_state: await this.enrichFlightPlanUiState(result.ui_state),
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
    const rows = await loadProcedureDistinctRows(airportId, procedureId);
    const legs = await loadProcedureMaterializationRecords(airportId, procedureId);
    return JSON.parse(
      await this.module.materialize_procedure_from_records(
        airportId,
        procedureId,
        JSON.stringify(kind),
        JSON.stringify(runwayTransition),
        JSON.stringify(enrouteTransition),
        componentIndex,
        JSON.stringify(rows),
        JSON.stringify(legs),
      ),
    ) as MaterializedProcedure;
  }

  async findProcedurePlateMatch(airportId: string, cifpId: string): Promise<CifpTppMatch | null> {
    const rows = await loadCifpTppMatchesForProcedure(airportId, cifpId);
    debugLog("adapter.cifp_tpp.by_procedure", { airport_id: airportId, cifp_id: cifpId, rows });
    return JSON.parse(
      await this.module.describe_show_plate_for_procedure(JSON.stringify(rows)),
    ) as CifpTppMatch | null;
  }

  async describePlateProcedureLoads(plan: FlightPlan, plateId: string): Promise<ProcedureLoadOption[]> {
    const rows = await loadCifpTppMatchesForPlate(plateId);
    debugLog("adapter.cifp_tpp.by_plate", { plate_id: plateId, rows });
    const grouped = new Map<string, CifpTppMatchRow[]>();
    for (const row of rows) {
      const key = `${row.airport_id}:${row.cifp_id}`;
      grouped.set(key, [...(grouped.get(key) ?? []), row]);
    }
    const candidates: PlateProcedureLoadCandidateInput[] = [];
    for (const groupedRows of grouped.values()) {
      const preferred = JSON.parse(
        await this.module.select_preferred_cifp_tpp_match(JSON.stringify(groupedRows)),
      ) as CifpTppMatch | null;
      if (!preferred) {
        continue;
      }
      const procedureRows = await loadProcedureDistinctRows(preferred.airport_id, preferred.cifp_id);
      if (procedureRows.length === 0) {
        continue;
      }
      candidates.push({
        airport_id: preferred.airport_id,
        cifp_id: preferred.cifp_id,
        match_rows: groupedRows,
        distinct_rows: procedureRows,
      });
    }
    return JSON.parse(
      await this.module.describe_plate_procedure_load_options(
        JSON.stringify(plan),
        JSON.stringify(candidates),
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
    typeof mod.load_playback_trace_in_session !== "function" ||
    typeof mod.play_playback_in_session !== "function" ||
    typeof mod.pause_playback_in_session !== "function" ||
    typeof mod.seek_playback_in_session !== "function" ||
    typeof mod.set_playback_rate_in_session !== "function" ||
    typeof mod.tick_playback_in_session !== "function" ||
    typeof mod.replace_flight_plan_in_session !== "function" ||
    typeof mod.select_airport_in_session !== "function" ||
    typeof mod.select_chart_in_session !== "function" ||
    typeof mod.ingest_point_tiles_in_session !== "function" ||
    typeof mod.get_map_overlay_in_session !== "function" ||
    typeof mod.get_session_snapshot !== "function" ||
    typeof mod.restore_chart_page_state_in_session !== "function" ||
    typeof mod.destroy_session !== "function" ||
    typeof mod.replace_flight_plan_state !== "function" ||
    typeof mod.remove_flight_plan_leg !== "function" ||
    typeof mod.build_flight_plan_ui !== "function" ||
    typeof mod.activate_leg_ui !== "function" ||
    typeof mod.activate_next_leg_ui !== "function" ||
    typeof mod.delete_component_ui !== "function" ||
    typeof mod.move_component_ui !== "function" ||
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

function routeStatusForLeg(planUiState: FlightPlanUiState | null, legIndex: number): FlightPlanRouteSegment["status"] {
  const guidance = planUiState?.guidance ?? null;
  const activeLegIndex = guidance?.active_leg != null ? guidance.active_leg_index : null;
  if (activeLegIndex != null) {
    if (legIndex < activeLegIndex) {
      return "completed";
    }
    if (legIndex === activeLegIndex) {
      return "active";
    }
    return "remaining";
  }
  const splitIndex = guidance?.display_split_leg_index ?? 0;
  return legIndex < splitIndex ? "completed" : "remaining";
}

function pointInPolygon(lat: number, lon: number, points: number[][]): boolean {
  let inside = false;
  let previousIndex = points.length - 1;

  for (let currentIndex = 0; currentIndex < points.length; currentIndex += 1) {
    const [currentLon, currentLat] = points[currentIndex];
    const [previousLon, previousLat] = points[previousIndex];
    const crossesLatitude = (currentLat > lat) !== (previousLat > lat);

    if (crossesLatitude) {
      const interpolatedLon =
        previousLon + ((currentLon - previousLon) * (lat - previousLat)) / (currentLat - previousLat);
      if (lon < interpolatedLon) {
        inside = !inside;
      }
    }

    previousIndex = currentIndex;
  }

  return inside;
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

function moveWaypointInPlan(plan: FlightPlan, waypointIndex: number, delta: number): FlightPlan {
  const waypoints = [
    plan.legs[0]?.from,
    ...plan.legs.map((leg) => leg.to),
  ].filter((waypoint): waypoint is FlightPlan["legs"][number]["from"] => waypoint !== undefined);
  if (waypointIndex < 0 || waypointIndex >= waypoints.length) {
    throw new Error(`InvalidFlightPlan: flight plan waypoint index out of range: ${waypointIndex}`);
  }
  const nextIndex = waypointIndex + delta;
  if (nextIndex < 0 || nextIndex >= waypoints.length) {
    throw new Error(`InvalidFlightPlan: flight plan waypoint move out of range: ${waypointIndex} -> ${nextIndex}`);
  }
  const nextWaypoints = [...waypoints];
  [nextWaypoints[waypointIndex], nextWaypoints[nextIndex]] = [nextWaypoints[nextIndex], nextWaypoints[waypointIndex]];
  const nextLegs = nextWaypoints.slice(0, -1).map((from, index) => ({
    from,
    to: nextWaypoints[index + 1],
    airway: null,
  }));
  return {
    ...plan,
    legs: nextLegs,
    departure: airportCode(nextWaypoints[0]) ?? null,
    destination: airportCode(nextWaypoints[nextWaypoints.length - 1]) ?? null,
    updated_at_epoch_ms: plan.updated_at_epoch_ms + 1,
    version: plan.version + 1,
  };
}
