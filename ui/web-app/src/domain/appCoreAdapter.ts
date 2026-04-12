import type {
  AppState,
  AirwayAutoSelection,
  AirwayBranch,
  AirwayPresentationPlan,
  AirwaySuggestion,
  AirwaySegment,
  CatalogJson,
  ChartPageData,
  ContentInventory,
  ContentPolicy,
  FlightPlan,
  FlightPlanUiMutation,
  FlightPlanUiState,
  ChartFamilyId,
  ContentAvailability,
  GuidanceState,
  LatLon,
  NavRef,
  PlanLeg,
  ResolvedLeg,
  ResolvedLegUiView,
  RouteComponentUiView,
  SequencingMode,
  Situation,
} from "./types";
import { deriveChartPage as deriveChartCatalog } from "./resourceIndexAdapters";
import { sampleCatalog } from "./sampleData";
import { viewportCenterLatLon, type MapViewportState } from "./mapViewport";

export type DerivedChartPageState = {
  airports: ChartPageData["airports"];
  recent_airport_ids: string[];
  selected_airport_id: string;
  selected_chart_id: string;
};

export type UiSessionSnapshot = {
  app_state: AppState;
  chart_page_state: UiChartPageState;
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
  removeLeg(index: number): Promise<UiSessionSnapshot>;
  moveWaypoint(index: number, delta: number): Promise<UiSessionSnapshot>;
  setSituation(situation: Situation): Promise<UiSessionSnapshot>;
  selectAirport(airportId: string): Promise<UiSessionSnapshot>;
  selectChart(chartId: string): Promise<UiSessionSnapshot>;
  ingestPointTiles(tiles: PointTilePayload[]): Promise<void>;
  queryMapOverlay(viewport: MapViewportState, widthPx: number, heightPx: number): Promise<MapOverlayQueryResult>;
  restoreChartPageState(recentAirportIds: string[], selectedAirportId?: string, selectedChartId?: string): Promise<UiSessionSnapshot>;
  destroy(): Promise<void>;
}

export interface AppCoreAdapter {
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
  activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation>;
  activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation>;
  prepareAirwayPresentation(
    airwayName: string,
    branches: AirwayBranch[],
    originPosition: LatLon,
    destinationPosition: LatLon | null,
  ): Promise<AirwayPresentationPlan>;
  sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]>;
  insertAirwayMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation>;
  chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null>;
}

export type AdapterBackendKind = "mock" | "wasm";

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
          : family === "ifr_area"
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

    return {
      chartCatalog,
      snapshot: async () => ({ app_state: appState, chart_page_state: chartPageState }),
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
        return { app_state: appState, chart_page_state: chartPageState };
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
        return { app_state: appState, chart_page_state: chartPageState };
      },
      setSituation: async (situation) => {
        appState = { ...appState, situation };
        return { app_state: appState, chart_page_state: chartPageState };
      },
      selectAirport: async (airportId) => {
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          appState.active_plan ?? plan,
          moveAirportToFront(chartPageState.recent_airport_ids, airportId, chartCatalog.airports),
          airportId,
          undefined,
        ));
        return { app_state: appState, chart_page_state: chartPageState };
      },
      selectChart: async (chartId) => {
        chartPageState = compactChartPageState(await adapter.deriveChartPageState(
          resourceIndex,
          appState.active_plan ?? plan,
          chartPageState.recent_airport_ids,
          chartPageState.selected_airport_id,
          chartId,
        ));
        return { app_state: appState, chart_page_state: chartPageState };
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
        return { app_state: appState, chart_page_state: chartPageState };
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
    return buildMockFlightPlanUiState(plan);
  }

  async activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation> {
    const uiState = buildMockFlightPlanUiState(plan, {
      active_leg_index: legIndex,
      sequencing_mode: "follow_plan",
      direct_to: null,
    });
    return { plan, ui_state: uiState };
  }

  async activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    const current = buildMockFlightPlanUiState(plan).guidance?.active_leg_index ?? 0;
    const nextIndex = Math.min(current + 1, Math.max(plan.legs.length - 1, 0));
    return this.activateLegUi(plan, nextIndex);
  }

  async suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    const current = buildMockFlightPlanUiState(plan).guidance?.active_leg_index ?? 0;
    const uiState = buildMockFlightPlanUiState(plan, {
      active_leg_index: current,
      sequencing_mode: "suspended",
      direct_to: null,
    });
    return { plan, ui_state: uiState };
  }

  async unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    const current = buildMockFlightPlanUiState(plan).guidance?.active_leg_index ?? 0;
    const uiState = buildMockFlightPlanUiState(plan, {
      active_leg_index: current,
      sequencing_mode: "follow_plan",
      direct_to: null,
    });
    return { plan, ui_state: uiState };
  }

  async sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return this.activateNextLegUi(plan);
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

  async sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]> {
    return [...suggestions].sort((left, right) => left.airway_name.localeCompare(right.airway_name));
  }

  async insertAirwayMaterializedUi(): Promise<FlightPlanUiMutation> {
    throw new Error("airway insertion requires wasm adapter");
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
    endComponentIndex: number,
    selectionJson: string,
    airwayJson: string,
    resolvedLegsJson: string,
  ): Promise<string> | string;
  chart_for_position(
    catalogJson: string,
    geometryJson: string,
    familyJson: string,
    lat: number,
    lon: number,
  ): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(private readonly module: WasmModule) {}

  async createUiSession(
    resourceIndex: unknown,
    plan: FlightPlan,
    recentAirportIds: string[],
    selectedAirportId?: string,
    selectedChartId?: string,
  ): Promise<UiSession> {
    const init = JSON.parse(
      await this.module.create_ui_session(
        JSON.stringify(sampleCatalogLike(resourceIndex)),
        JSON.stringify(deriveChartCatalog(resourceIndex as Parameters<typeof deriveChartCatalog>[0], plan)),
        JSON.stringify(plan),
        JSON.stringify(recentAirportIds),
        JSON.stringify(selectedAirportId ?? null),
        JSON.stringify(selectedChartId ?? null),
      ),
    ) as { handle: number; chart_catalog: ChartPageData; snapshot: UiSessionSnapshot };
    const { handle } = init;
    let snapshot = init.snapshot;
    return {
      chartCatalog: init.chart_catalog,
      snapshot: async () => {
        snapshot = JSON.parse(await this.module.get_session_snapshot(handle)) as UiSessionSnapshot;
        return snapshot;
      },
      removeLeg: async (index) => {
        snapshot = JSON.parse(await this.module.remove_leg_in_session(handle, index)) as UiSessionSnapshot;
        return snapshot;
      },
      moveWaypoint: async (index, delta) => {
        snapshot = JSON.parse(await this.module.move_waypoint_in_session(handle, index, delta)) as UiSessionSnapshot;
        return snapshot;
      },
      setSituation: async (situation) => {
        snapshot = JSON.parse(await this.module.set_situation_in_session(handle, JSON.stringify(situation))) as UiSessionSnapshot;
        return snapshot;
      },
      selectAirport: async (airportId) => {
        snapshot = JSON.parse(await this.module.select_airport_in_session(handle, JSON.stringify(airportId))) as UiSessionSnapshot;
        return snapshot;
      },
      selectChart: async (chartId) => {
        snapshot = JSON.parse(await this.module.select_chart_in_session(handle, JSON.stringify(chartId))) as UiSessionSnapshot;
        return snapshot;
      },
      ingestPointTiles: async (tiles) => {
        await this.module.ingest_point_tiles_in_session(handle, JSON.stringify(tiles));
      },
      queryMapOverlay: async (viewport, widthPx, heightPx) =>
        JSON.parse(
          await this.module.get_map_overlay_in_session(
            handle,
            JSON.stringify(coreViewportForMap(viewport)),
            widthPx,
            heightPx,
          ),
        ) as MapOverlayQueryResult,
      restoreChartPageState: async (nextRecentAirportIds, nextSelectedAirportId, nextSelectedChartId) => {
        snapshot = JSON.parse(
          await this.module.restore_chart_page_state_in_session(
            handle,
            JSON.stringify(nextRecentAirportIds),
            JSON.stringify(nextSelectedAirportId ?? null),
            JSON.stringify(nextSelectedChartId ?? null),
          ),
        ) as UiSessionSnapshot;
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
    return JSON.parse(
      await this.module.build_flight_plan_ui(JSON.stringify(plan)),
    ) as FlightPlanUiState;
  }

  async activateLegUi(plan: FlightPlan, legIndex: number): Promise<FlightPlanUiMutation> {
    return JSON.parse(
      await this.module.activate_leg_ui(JSON.stringify(plan), legIndex),
    ) as FlightPlanUiMutation;
  }

  async activateNextLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return JSON.parse(
      await this.module.activate_next_leg_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation;
  }

  async suspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return JSON.parse(
      await this.module.suspend_sequencing_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation;
  }

  async unsuspendSequencingUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return JSON.parse(
      await this.module.unsuspend_sequencing_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation;
  }

  async sequenceActiveLegUi(plan: FlightPlan): Promise<FlightPlanUiMutation> {
    return JSON.parse(
      await this.module.sequence_active_leg_ui(JSON.stringify(plan)),
    ) as FlightPlanUiMutation;
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

  async sortAirwaySuggestionsForUi(suggestions: AirwaySuggestion[]): Promise<AirwaySuggestion[]> {
    return JSON.parse(
      await this.module.sort_airway_suggestions_for_ui(JSON.stringify(suggestions)),
    ) as AirwaySuggestion[];
  }

  async insertAirwayMaterializedUi(
    plan: FlightPlan,
    startComponentIndex: number,
    endComponentIndex: number,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolvedLegs: ResolvedLeg[],
  ): Promise<FlightPlanUiMutation> {
    const result = JSON.parse(
      await this.module.insert_airway_materialized_ui(
        JSON.stringify(plan),
        startComponentIndex,
        endComponentIndex,
        JSON.stringify(selection),
        JSON.stringify(airway),
        JSON.stringify(resolvedLegs),
      ),
    ) as { mutation: { plan: FlightPlan }; ui_state: FlightPlanUiState };
    return {
      plan: result.mutation.plan,
      ui_state: result.ui_state,
    };
  }

  async chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null> {
    return JSON.parse(
      await this.module.chart_for_position(
        JSON.stringify(catalog),
        JSON.stringify(geometry),
        JSON.stringify(family),
        lat,
        lon,
      ),
    ) as CatalogJson["charts"][number] | null;
  }
}

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = () => import("@generated/app_wasm.js"),
): Promise<LoadedAdapter> {
  try {
    const mod = (await importer()) as Partial<WasmModule>;
    if (typeof mod.default === "function") {
      await mod.default();
    }
    if (
      typeof mod.create_ui_session !== "function" ||
      typeof mod.remove_leg_in_session !== "function" ||
      typeof mod.move_waypoint_in_session !== "function" ||
      typeof mod.set_situation_in_session !== "function" ||
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
      typeof mod.suspend_sequencing_ui !== "function" ||
      typeof mod.unsuspend_sequencing_ui !== "function" ||
      typeof mod.sequence_active_leg_ui !== "function" ||
      typeof mod.prepare_airway_presentation !== "function" ||
      typeof mod.sort_airway_suggestions_for_ui !== "function" ||
      typeof mod.insert_airway_materialized_ui !== "function" ||
      typeof mod.derive_chart_page !== "function" ||
      typeof mod.derive_chart_page_state !== "function" ||
      typeof mod.set_content_policy_state !== "function" ||
      typeof mod.refresh_content_state !== "function" ||
      typeof mod.chart_for_position !== "function"
    ) {
      throw new Error("generated wasm module is missing required exports");
    }

    return {
      adapter: new WasmAppCoreAdapter(mod as WasmModule),
      backend: "wasm",
      detail: "Using generated Rust WASM bindings.",
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      adapter: new MockAppCoreAdapter(),
      backend: "mock",
      detail: `Falling back to mock adapter: ${message}`,
    };
  }
}

function navRefLabel(ref: NavRef): string {
  if ("Airport" in ref) return ref.Airport;
  if ("Navaid" in ref) return ref.Navaid;
  if ("Fix" in ref) return ref.Fix;
  return `${ref.LatLon.lat.toFixed(3)}, ${ref.LatLon.lon.toFixed(3)}`;
}

function buildMockFlightPlanUiState(
  plan: FlightPlan,
  guidance?: GuidanceState | null,
): FlightPlanUiState {
  const legs = plan.legs.map<ResolvedLegUiView>((leg, index) => ({
    leg_index: index,
    leg_id: `leg-${index}`,
    component_index: index,
    from: leg.from,
    to: leg.to,
    active: guidance?.active_leg_index === index,
    suspend_boundary_after: false,
  }));

  const components = plan.legs.map<RouteComponentUiView>((leg, index) => ({
    component_index: index,
    kind: "waypoint",
    summary: `${navRefLabel(leg.from)} -> ${navRefLabel(leg.to)}`,
    items: [
      { kind: "waypoint", nav_ref: leg.from },
      { kind: "waypoint", nav_ref: leg.to },
    ],
    active: legs[index]?.active ?? false,
  }));

  const activeLeg = guidance && guidance.active_leg_index >= 0
    ? plan.legs[guidance.active_leg_index] ?? null
    : null;

  return {
    components,
    resolved_legs: legs,
    guidance: guidance
      ? {
          sequencing_mode: guidance.sequencing_mode as SequencingMode,
          active_leg_index: guidance.active_leg_index,
          active_component_index: guidance.active_leg_index,
          active_leg: activeLeg as PlanLeg | null,
          direct_to: null,
          can_sequence_active_leg: guidance.sequencing_mode === "direct_to" || guidance.active_leg_index < Math.max(plan.legs.length - 1, 0),
          can_activate_next_leg: guidance.active_leg_index < Math.max(plan.legs.length - 1, 0),
          can_suspend: guidance.sequencing_mode !== "suspended",
          can_unsuspend: guidance.sequencing_mode === "suspended",
          suspend_boundary_after_active_leg: false,
        }
      : null,
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
