import { Fragment, useEffect, useMemo, useRef, useState, type CSSProperties, type Dispatch, type SetStateAction } from "react";
import { chartPage, mapViews, resourceIndex, sampleCatalog, samplePlan } from "./domain/sampleData";
import type {
  AirwayPresentationPlan,
  AirwaySuggestion,
  AppState,
  ChartPageData,
  FlightPlanUiMutation,
  FlightPlanUiState,
  LatLon,
  MaterializedProcedure,
  NavRef,
  ProcedureOptions,
  ProcedureSummary,
  Situation,
} from "./domain/types";
import uiTheme from "@shared-ui-theme";
import planViewIcon from "./assets/plan-view-icon.svg";
import {
  loadBestAvailableAdapter,
  type AdapterBackendKind,
  type AppCoreAdapter,
  type DerivedChartPageState,
  type UiSession,
  type UiSessionSnapshot,
} from "./domain/appCoreAdapter";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  dragViewport,
  latLonToWorld,
  preserveViewportForMap,
  renderTiles,
  scaleForZoom,
  viewportCenterLatLon,
  worldToScreen,
  zoomAroundPoint,
  type MapViewportState,
  type ScreenPoint,
} from "./domain/mapViewport";
import {
  clampImageViewport,
  clampImageZoom,
  createInitialImageViewport,
  dragImageViewport,
  imageDisplaySize,
  zoomImageAroundPoint,
  type ImageViewportState,
} from "./domain/imageViewport";
import { pointTileUrl, type PointTilePayload } from "./domain/vectorTiles";
import type { MapOverlayQueryResult } from "./domain/appCoreAdapter";
import {
  airwayEntryCandidateFromPresentation,
  airwayExitCandidatesFromPresentation,
  materializeAirwaySelection,
  prepareAirwayPresentationForAnchors,
  resolveNavRefPosition,
  suggestAirwaysNearAnchor,
} from "./domain/airwayPlanner";
import { getBrowserNavDb } from "./domain/webNavDb";
import { debugLog } from "./domain/debugLog";

type SurfaceSize = {
  width: number;
  height: number;
};

type FlightPlanRouteSegment = {
  id: string;
  from: LatLon;
  to: LatLon;
  status: "completed" | "active" | "remaining";
};

type AppPage = "map" | "plan" | "charts";

type ChartFamilyId = "sec" | "tac" | "enr-l" | "enr-h";

type ChartAsset = NonNullable<ChartPageData["airports"][number]>["charts"][number];
type TrayOption = {
  id: string;
  label: string;
  active?: boolean;
  disabled?: boolean;
  accentColor?: string;
  onSelect: () => void;
};

type UiThemeJson = {
  controls: {
    button_bg: string;
    header_button: string;
    button_fg: string;
    panel_bg: string;
    panel_border: string;
    panel_fg: string;
    panel_muted: string;
    chart_surface_bg: string;
  };
  plate_folder: {
    thumbnail_bg: string;
    label_colors: Record<string, string>;
  };
};

type TrayDockStyle = "compact" | "plate_narrow" | "plate_wide";
type PlateFolderCategory = ChartAsset["folder_category"];

const chartFamilies: Array<{ id: ChartFamilyId; label: string; launcherLabel: string }> = [
  { id: "sec", label: "SECTIONAL", launcherLabel: "SEC" },
  { id: "tac", label: "TAC", launcherLabel: "TAC" },
  { id: "enr-l", label: "IFR-LOW", launcherLabel: "IFR L" },
  { id: "enr-h", label: "IFR-HIGH", launcherLabel: "IFR H" },
];

const pageOptions: Array<{ id: AppPage; label: string; launcherLabel: string }> = [
  { id: "map", label: "CHART", launcherLabel: "CHT" },
  { id: "charts", label: "PLATE", launcherLabel: "PLT" },
  { id: "plan", label: "PLAN", launcherLabel: "PLN" },
];

const webUiStateStorageKey = "aerobag.web.uiState.v1";
const maxViewHistoryDepth = 64;
const loadedUiTheme = uiTheme as UiThemeJson;
const controlTheme = loadedUiTheme.controls;
const plateFolderTheme = loadedUiTheme.plate_folder;
const plateFolderCategoryOrder: PlateFolderCategory[] = ["airport-diagram", "csup", "takeoff-mins", "approach", "departure", "star"];
const VAMPS_POSITION = { lat: 47.3648944444444, lon: -121.980275 };
const situationRingSizesNm = [0.25, 0.5, 0.8, 1, 1.5, 2, 3, 5, 8, 10, 15, 20, 30, 50, 100, 150, 200] as const;
const vorOuterHexPoints = [
  { x: -8, y: 0 },
  { x: -4, y: -7 },
  { x: 4, y: -7 },
  { x: 8, y: 0 },
  { x: 4, y: 7 },
  { x: -4, y: 7 },
] as const;
const vorEdgeInsetDistances = [3.8, 1.9, 3.8, 1.9, 3.8, 1.9] as const;

type PersistedWebUiState = {
  page?: AppPage;
  selectedAirportId?: string;
  selectedChartId?: string;
  recentAirportIds?: string[];
};

type AppViewSnapshot = {
  page: AppPage;
  selectedMapId: string;
  mapViewport: MapViewportState;
  selectedAirportId: string;
  selectedChartId: string;
  selectedChartLabel: string;
  recentAirportIds: string[];
  chartViewport: ImageViewportState | null;
  chartFolderOpen: boolean;
};

type WebHistoryState = {
  __aerobag?: true;
  current?: AppViewSnapshot;
  stack?: AppViewSnapshot[];
};

type VorPoint = {
  x: number;
  y: number;
};

function polygonSignedArea(points: readonly VorPoint[]) {
  let area = 0;
  for (let index = 0; index < points.length; index += 1) {
    const current = points[index];
    const next = points[(index + 1) % points.length];
    area += current.x * next.y - next.x * current.y;
  }
  return area / 2;
}

function thumbPixels(multiplier: number) {
  if (typeof window === "undefined") {
    return 0;
  }
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--thumb").trim();
  const parsed = Number.parseFloat(raw.replace("px", ""));
  return (Number.isFinite(parsed) ? parsed : 0) * multiplier;
}

function intersectLines(originA: VorPoint, directionA: VorPoint, originB: VorPoint, directionB: VorPoint): VorPoint {
  const cross = directionA.x * directionB.y - directionA.y * directionB.x;
  if (Math.abs(cross) < 1e-6) {
    return originA;
  }
  const deltaX = originB.x - originA.x;
  const deltaY = originB.y - originA.y;
  const t = (deltaX * directionB.y - deltaY * directionB.x) / cross;
  return {
    x: originA.x + directionA.x * t,
    y: originA.y + directionA.y * t,
  };
}

function offsetPolygonByEdgeDistances(points: readonly VorPoint[], edgeDistances: readonly number[]) {
  const signedArea = polygonSignedArea(points);
  const inwardNormalForEdge = (from: VorPoint, to: VorPoint, distance: number): VorPoint => {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const length = Math.hypot(dx, dy) || 1;
    if (signedArea > 0) {
      return { x: (dy / length) * distance, y: (-dx / length) * distance };
    }
    return { x: (-dy / length) * distance, y: (dx / length) * distance };
  };
  return points.map((point, index) => {
    const prevIndex = (index + points.length - 1) % points.length;
    const nextIndex = (index + 1) % points.length;
    const prevPoint = points[prevIndex];
    const nextPoint = points[nextIndex];
    const prevShift = inwardNormalForEdge(prevPoint, point, edgeDistances[prevIndex]);
    const nextShift = inwardNormalForEdge(point, nextPoint, edgeDistances[index]);
    const prevOrigin = { x: prevPoint.x + prevShift.x, y: prevPoint.y + prevShift.y };
    const nextOrigin = { x: point.x + nextShift.x, y: point.y + nextShift.y };
    return intersectLines(
      prevOrigin,
      { x: point.x - prevPoint.x, y: point.y - prevPoint.y },
      nextOrigin,
      { x: nextPoint.x - point.x, y: nextPoint.y - point.y },
    );
  });
}

function polygonPathData(points: readonly VorPoint[]) {
  return points.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`).join(" ") + " Z";
}

const vorInnerHexPoints = offsetPolygonByEdgeDistances(vorOuterHexPoints, vorEdgeInsetDistances);
const vorOuterHexPath = polygonPathData(vorOuterHexPoints);
const vorBandPath = `${vorOuterHexPath} ${polygonPathData(vorInnerHexPoints)}`;
const airportFuelTabsPath = [
  "M -4 -17 H 4 V -11 H -4 Z",
  "M 11 -4 H 17 V 4 H 11 Z",
  "M -4 11 H 4 V 17 H -4 Z",
  "M -17 -4 H -11 V 4 H -17 Z",
].join(" ");

function demoSituation(): Situation {
  return {
    position: {
      kind: "lat_lon",
      lat: VAMPS_POSITION.lat,
      lon: VAMPS_POSITION.lon,
    },
    orientation_deg: 135,
    speed_kt: 105,
  };
}

function initialMapId() {
  return mapViews.find((view) => view.map_view.chart_family === "tac")?.id ?? mapViews[0].id;
}

export default function App() {
  const [sessionStartMs] = useState(() => Date.now());
  const uptimeLabel = useSessionUptimeLabel(sessionStartMs);
  const locationSearch = typeof window !== "undefined" ? window.location.search : "";
  const debugTileLabels = new URLSearchParams(locationSearch).has("debugTiles");
  const persistedUiState = useMemo(readPersistedWebUiState, []);
  const [page, setPage] = useState<AppPage>(persistedUiState.page ?? "map");
  const [pageHistory, setPageHistory] = useState<AppViewSnapshot[]>([]);
  const [appCoreAdapter, setAppCoreAdapter] = useState<AppCoreAdapter | null>(null);
  const [adapterBackend, setAdapterBackend] = useState<AdapterBackendKind>("wasm");
  const [adapterDetail, setAdapterDetail] = useState<string>("loading");
  const [sessionInitError, setSessionInitError] = useState<string | null>(null);
  const [selectedMapId, setSelectedMapId] = useState<string>(initialMapId());
  const initialRecentAirportIds = useMemo(
    () => mergeRecentAirportIds(chartPage.airports, persistedUiState.recentAirportIds ?? []),
    [persistedUiState],
  );
  const initialChartPageState = useMemo<DerivedChartPageState>(
    () => ({
      airports: chartPage.airports,
      recent_airport_ids: initialRecentAirportIds,
      selected_airport_id: resolveAirportId(chartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
      selected_chart_id: resolveChartId(
        chartPage.airports,
        resolveAirportId(chartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
        persistedUiState.selectedChartId,
      ),
    }),
    [initialRecentAirportIds, persistedUiState.selectedAirportId, persistedUiState.selectedChartId],
  );
  const [uiSession, setUiSession] = useState<UiSession | null>(null);
  const [sessionSnapshot, setSessionSnapshot] = useState<UiSessionSnapshot>({
    app_state: {
      active_plan: null,
      situation: demoSituation(),
      content_policy: "PreferLocal",
      last_content_requirements: [],
      last_content_report: null,
    },
    chart_page_state: {
      ordered_airport_ids: initialChartPageState.airports.map((airport) => airport.id),
      recent_airport_ids: initialChartPageState.recent_airport_ids,
      selected_airport_id: initialChartPageState.selected_airport_id,
      selected_chart_id: initialChartPageState.selected_chart_id,
    },
  });
  const appState: AppState = sessionSnapshot.app_state;
  const chartCatalog: ChartPageData = uiSession?.chartCatalog ?? chartPage;
  const chartAirportById = useMemo(
    () => new Map(chartCatalog.airports.map((airport) => [airport.id, airport])),
    [chartCatalog],
  );
  const chartPageData: ChartPageData = useMemo(
    () => ({
      airports: sessionSnapshot.chart_page_state.ordered_airport_ids
        .map((airportId) => chartAirportById.get(airportId))
        .filter((airport): airport is ChartPageData["airports"][number] => airport != null),
    }),
    [chartAirportById, sessionSnapshot.chart_page_state.ordered_airport_ids],
  );
  const currentPlan = appState.active_plan;
  const [planUiState, setPlanUiState] = useState<FlightPlanUiState | null>(null);
  const recentAirportIds = sessionSnapshot.chart_page_state.recent_airport_ids;
  const selectedAirportId = sessionSnapshot.chart_page_state.selected_airport_id;
  const selectedChartId = sessionSnapshot.chart_page_state.selected_chart_id;

  const selectedMap = useMemo(
    () => mapViews.find((view) => view.id === selectedMapId) ?? mapViews[0],
    [selectedMapId],
  );
  const [mapViewport, setMapViewport] = useState<MapViewportState>(() => {
    const center = latLonToWorld(VAMPS_POSITION.lat, VAMPS_POSITION.lon);
    return {
      centerWorldX: center.x,
      centerWorldY: center.y,
      zoom: 10.0,
    };
  });
  const [chartViewport, setChartViewport] = useState<ImageViewportState | null>(null);
  const [chartFolderOpen, setChartFolderOpen] = useState(false);
  const selectedFamily = useMemo(
    () => chartFamilies.find((family) => family.id === selectedMap.map_view.chart_family) ?? chartFamilies[0],
    [selectedMap],
  );
  const availableFamilies = useMemo(
    () => new Set(mapViews.map((view) => view.map_view.chart_family)),
    [],
  );
  const selectedFamilyMapViews = useMemo(
    () => mapViews.filter((view) => view.map_view.chart_family === selectedMap.map_view.chart_family),
    [selectedMap],
  );
  const selectedAirport = useMemo(
    () => chartPageData.airports.find((airport) => airport.id === selectedAirportId) ?? chartPageData.airports[0] ?? null,
    [chartPageData, selectedAirportId],
  );
  const orderedChartAirports = useMemo(
    () => orderAirportsByRecency(chartPageData.airports, recentAirportIds),
    [chartPageData, recentAirportIds],
  );
  const selectedChart = useMemo(
    () => selectedAirport?.charts.find((chart) => chart.id === selectedChartId) ?? selectedAirport?.charts[0] ?? null,
    [selectedAirport, selectedChartId],
  );
  const legSummary = useMemo(() => {
    const firstLeg = currentPlan?.legs[0];
    if (!firstLeg) {
      return "NO LEG";
    }
    const from = navRefLabel(firstLeg.from);
    const to = navRefLabel(firstLeg.to);
    return `${from} -> ${to} CRS 342`;
  }, [currentPlan]);

  useEffect(() => {
    let cancelled = false;
    loadBestAvailableAdapter().then((loaded) => {
      if (!cancelled) {
        setAppCoreAdapter(loaded.adapter);
        setAdapterBackend(loaded.backend);
        setAdapterDetail(loaded.detail);
        setSessionInitError(null);
      }
    }).catch((error) => {
      if (!cancelled) {
        const message = error instanceof Error ? error.message : String(error);
        setSessionInitError(`WASM adapter init failed: ${message}`);
        setAdapterDetail(`adapter init failed: ${message}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void getBrowserNavDb().catch((error) => {
      if (!cancelled) {
        console.error("failed to prewarm browser nav db", error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let nextSession: UiSession | null = null;
    if (!appCoreAdapter) {
      return;
    }
    buildSeededDevPlan(appCoreAdapter).then(async (initialPlan) => {
      const created = await appCoreAdapter.createUiSession(
        resourceIndex,
        initialPlan.plan,
        initialRecentAirportIds,
        initialChartPageState.selected_airport_id,
        initialChartPageState.selected_chart_id,
      );
      nextSession = created;
      if (!cancelled) {
        setUiSession(created);
        setPlanUiState(initialPlan.uiState);
      }
      const snapshot = await created.setSituation(demoSituation());
      if (!cancelled) {
        setSessionSnapshot(snapshot);
      }
    }).catch((error) => {
      console.error("failed to initialize web ui session", error);
    });
    return () => {
      cancelled = true;
      void nextSession?.destroy();
    };
  }, [adapterBackend, appCoreAdapter, initialChartPageState.selected_airport_id, initialChartPageState.selected_chart_id, initialRecentAirportIds]);

  useEffect(() => {
    setMapViewport((current) => preserveViewportForMap(current, selectedMap.map_view));
  }, [selectedMap]);

  useEffect(() => {
    let cancelled = false;
    if (!appCoreAdapter) {
      setPlanUiState(null);
      return;
    }
    if (!currentPlan) {
      setPlanUiState(null);
      return;
    }
    appCoreAdapter.buildFlightPlanUi(currentPlan).then((next) => {
      if (!cancelled) {
        setPlanUiState(next);
      }
    }).catch((error) => {
      console.error("failed to build flight plan ui state", error);
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter, currentPlan]);

  const appReady =
    appCoreAdapter !== null &&
    uiSession !== null &&
    currentPlan !== null &&
    planUiState !== null;

  useEffect(() => {
    writePersistedWebUiState({
      page,
      selectedAirportId,
      selectedChartId,
      recentAirportIds,
    });
  }, [page, recentAirportIds, selectedAirportId, selectedChartId]);

  function currentSnapshot(): AppViewSnapshot {
    return {
      page,
      selectedMapId,
      mapViewport,
      selectedAirportId,
      selectedChartId,
      selectedChartLabel: selectedChart?.label ?? "",
      recentAirportIds,
      chartViewport,
      chartFolderOpen,
    };
  }

  function restoreSnapshot(snapshot: AppViewSnapshot, history: AppViewSnapshot[]) {
    setPageHistory(history);
    setPage(snapshot.page);
    setSelectedMapId(snapshot.selectedMapId);
    setMapViewport(snapshot.mapViewport);
    setChartViewport(snapshot.chartViewport);
    setChartFolderOpen(snapshot.chartFolderOpen);
    if (uiSession) {
      void uiSession.restoreChartPageState(
        snapshot.recentAirportIds,
        snapshot.selectedAirportId || undefined,
        snapshot.selectedChartId || undefined,
      ).then((nextSnapshot) => {
        setSessionSnapshot(nextSnapshot);
      }).catch(() => {});
    }
  }

  function boundedHistory(history: AppViewSnapshot[]) {
    return history.length <= maxViewHistoryDepth ? history : history.slice(history.length - maxViewHistoryDepth);
  }

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      const state: WebHistoryState = {
        __aerobag: true,
        current: currentSnapshot(),
        stack: pageHistory,
      };
      window.history.replaceState(state, "");
    }, 120);
    return () => window.clearTimeout(timeoutId);
  }, [page, pageHistory, selectedMapId, mapViewport, selectedAirportId, selectedChartId, recentAirportIds, chartViewport, chartFolderOpen]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    function handlePopState(event: PopStateEvent) {
      const state = (event.state ?? {}) as WebHistoryState;
      if (state.__aerobag && state.current) {
        restoreSnapshot(state.current, Array.isArray(state.stack) ? state.stack : []);
      }
    }
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  function navigateToPage(nextPage: AppPage) {
    if (nextPage === page) {
      return;
    }
    const nextHistory = boundedHistory([...pageHistory, currentSnapshot()]);
    setPageHistory(nextHistory);
    setPage(nextPage);
    if (typeof window !== "undefined") {
      const nextCurrent: AppViewSnapshot = {
        ...currentSnapshot(),
        page: nextPage,
      };
      const state: WebHistoryState = {
        __aerobag: true,
        current: nextCurrent,
        stack: nextHistory,
      };
      window.history.pushState(state, "");
    }
  }

  function pushViewSnapshot(next: Partial<AppViewSnapshot> & Pick<AppViewSnapshot, "page">) {
    const nextHistory = boundedHistory([...pageHistory, currentSnapshot()]);
    const nextCurrent: AppViewSnapshot = {
      ...currentSnapshot(),
      ...next,
    };
    setPageHistory(nextHistory);
    restoreSnapshot(nextCurrent, nextHistory);
    if (typeof window !== "undefined") {
      window.history.pushState(
        {
          __aerobag: true,
          current: nextCurrent,
          stack: nextHistory,
        } satisfies WebHistoryState,
        "",
      );
    }
  }

  const themeVars = useMemo(
    () =>
      ({
        "--theme-button-bg": controlTheme.button_bg,
        "--theme-header-button": controlTheme.header_button,
        "--theme-button-fg": controlTheme.button_fg,
        "--theme-panel-bg": controlTheme.panel_bg,
        "--theme-panel-border": controlTheme.panel_border,
        "--theme-panel-fg": controlTheme.panel_fg,
        "--theme-panel-muted": controlTheme.panel_muted,
        "--theme-chart-surface-bg": controlTheme.chart_surface_bg,
      }) as CSSProperties,
    [],
  );

  if (sessionInitError) {
    return (
      <main className="appFrame">
        <section className="appPage planPage">
          <div className="planGuidanceSummary">{sessionInitError}</div>
        </section>
      </main>
    );
  }

  if (!appReady || !currentPlan || !planUiState) {
    return (
      <main className="appFrame">
        <section className="appPage planPage">
          <div className="planGuidanceSummary">INITIALIZING CORE…</div>
        </section>
      </main>
    );
  }

  return (
    <main className="appShell" style={themeVars}>
      <div className={`pageLayer${page === "map" ? " isActive" : ""}`} aria-hidden={page !== "map"}>
        <MapPage
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          debugTileLabels={debugTileLabels}
          selectedMapId={selectedMapId}
          selectedMap={selectedMap}
          selectedFamilyMapViews={selectedFamilyMapViews}
          selectedFamily={selectedFamily}
          availableFamilies={availableFamilies}
          viewport={mapViewport}
          onViewportChange={setMapViewport}
          onSelectMapId={(mapId) => {
            pushViewSnapshot({
              page: "map",
              selectedMapId: mapId,
            });
          }}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          legSummary={legSummary}
          locationSearch={locationSearch}
          situation={appState.situation}
          plan={currentPlan}
          planUiState={planUiState}
          uiSession={uiSession}
          adapterBackend={adapterBackend}
          adapterDetail={adapterDetail}
        />
      </div>

      <div className={`pageLayer${page === "plan" ? " isActive" : ""}`} aria-hidden={page !== "plan"}>
        <FlightPlanPage
          appCoreAdapter={appCoreAdapter}
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          legSummary={legSummary}
          plan={currentPlan}
          planUiState={planUiState}
          onSelectPage={navigateToPage}
          onOpenCharts={(airportId) => {
            if (!airportId) {
              return;
            }
            const airport = chartPageData.airports.find((entry) => entry.id === airportId);
            if (uiSession) {
              void uiSession.selectAirport(airportId).then((nextSnapshot) => {
                setSessionSnapshot(nextSnapshot);
              }).catch(() => {});
            }
            pushViewSnapshot({
              page: "charts",
              selectedAirportId: airportId,
              selectedChartId: airport?.charts[0]?.id ?? "",
              selectedChartLabel: airport?.charts[0]?.label ?? "",
              recentAirportIds: moveAirportToFront(recentAirportIds, airportId, chartPageData.airports),
              chartViewport: null,
              chartFolderOpen: true,
            });
          }}
          onMoveComponent={async (componentIndex, delta) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.moveComponentUi(currentPlan, componentIndex, delta);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onActivateLeg={async (legIndex) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.activateLegUi(currentPlan, legIndex);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onDeleteComponent={async (componentIndex) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.deleteComponentUi(currentPlan, componentIndex);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onActivateNextLeg={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.activateNextLegUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onSuspendSequencing={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.suspendSequencingUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onUnsuspendSequencing={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.unsuspendSequencingUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onSequenceActiveLeg={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.sequenceActiveLegUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onInsertAirway={async (startComponentIndex, endComponentIndex, entryIndex, exitIndex, presentation, originAnchor, destinationAnchor) => {
            if (!appCoreAdapter) return;
            const entry = airwayEntryCandidateFromPresentation(presentation, entryIndex);
            const exit = airwayExitCandidatesFromPresentation(presentation, entryIndex)[exitIndex];
            const materialized = await materializeAirwaySelection(
              startComponentIndex,
              entry,
              exit,
              originAnchor,
              destinationAnchor,
            );
            const mutation = await appCoreAdapter.insertAirwayMaterializedUi(
              currentPlan,
              startComponentIndex,
              endComponentIndex,
              materialized.selection,
              materialized.airway,
              materialized.resolvedLegs,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onReplaceAirway={async (componentIndex, entryIndex, exitIndex, presentation, originAnchor, destinationAnchor) => {
            if (!appCoreAdapter) return;
            const entry = airwayEntryCandidateFromPresentation(presentation, entryIndex);
            const exit = airwayExitCandidatesFromPresentation(presentation, entryIndex)[exitIndex];
            const materialized = await materializeAirwaySelection(
              componentIndex,
              entry,
              exit,
              originAnchor,
              destinationAnchor,
            );
            const mutation = await appCoreAdapter.replaceAirwayMaterializedUi(
              currentPlan,
              componentIndex,
              materialized.selection,
              materialized.airway,
              materialized.resolvedLegs,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          onInsertProcedure={async (startComponentIndex, endComponentIndex, built) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.insertProcedureMaterializedUi(
              currentPlan,
              startComponentIndex,
              endComponentIndex,
              built,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
        />
      </div>

      <div className={`pageLayer${page === "charts" ? " isActive" : ""}`} aria-hidden={page !== "charts"}>
        <ChartsPage
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          airports={orderedChartAirports}
          selectedAirport={selectedAirport}
          selectedChart={selectedChart}
          folderOpen={chartFolderOpen}
          viewport={chartViewport}
          onViewportChange={setChartViewport}
          onFolderOpenChange={(next) => {
            pushViewSnapshot({
              page: "charts",
              chartFolderOpen: next,
            });
          }}
          onSelectPage={navigateToPage}
          onSelectAirport={(airportId) => {
            const airport = chartPageData.airports.find((entry) => entry.id === airportId);
            if (uiSession) {
              void uiSession.selectAirport(airportId).then((nextSnapshot) => {
                setSessionSnapshot(nextSnapshot);
              }).catch(() => {});
            }
            pushViewSnapshot({
              page: "charts",
              selectedAirportId: airportId,
              selectedChartId: airport?.charts[0]?.id ?? "",
              selectedChartLabel: airport?.charts[0]?.label ?? "",
              recentAirportIds: moveAirportToFront(recentAirportIds, airportId, chartPageData.airports),
              chartViewport: null,
              chartFolderOpen: false,
            });
          }}
          onSelectChart={(chartId) => {
            const nextChart = selectedAirport?.charts.find((chart) => chart.id === chartId);
            if (uiSession) {
              void uiSession.selectChart(chartId).then((nextSnapshot) => {
                setSessionSnapshot(nextSnapshot);
              }).catch(() => {});
            }
            pushViewSnapshot({
              page: "charts",
              selectedChartId: chartId,
              selectedChartLabel: nextChart?.label ?? "",
              chartViewport: null,
              chartFolderOpen: false,
            });
          }}
          situation={appState.situation}
        />
      </div>
    </main>
  );
}

function MapPage(props: {
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  debugTileLabels: boolean;
  selectedMapId: string;
  selectedMap: (typeof mapViews)[number];
  selectedFamilyMapViews: (typeof mapViews);
  selectedFamily: (typeof chartFamilies)[number];
  availableFamilies: Set<string>;
  viewport: MapViewportState;
  onViewportChange: (next: MapViewportState) => void;
  onSelectMapId: (mapId: string) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  legSummary: string;
  locationSearch: string;
  situation: Situation;
  plan: typeof samplePlan;
  planUiState: FlightPlanUiState | null;
  uiSession: UiSession | null;
  adapterBackend: AdapterBackendKind;
  adapterDetail: string;
}) {
  const {
    debugTileLabels,
    page,
    pageHistory,
    uptimeLabel,
    selectedMap,
    selectedFamilyMapViews,
    selectedFamily,
    availableFamilies,
    viewport,
    onViewportChange,
    onSelectMapId,
    onSelectPage,
    onOpenPlan,
    legSummary,
    locationSearch,
    situation,
    plan,
    planUiState,
    uiSession,
    adapterBackend,
    adapterDetail,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const trayGroup = useModalTrayGroup(["page", "family"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const [mapOverlay, setMapOverlay] = useState<MapOverlayQueryResult>({
    needed_point_tiles: [],
    visible_features: [],
    warnings: [],
  });
  const [flightPlanRoute, setFlightPlanRoute] = useState<FlightPlanRouteSegment[]>([]);
  const [mapOverlayViewport, setMapOverlayViewport] = useState<MapViewportState | null>(null);
  const viewportRef = useRef<MapViewportState>(viewport);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });

  useEffect(() => {
    if (activePointersRef.current.size > 0) {
      return;
    }
    viewportRef.current = viewport;
  }, [viewport]);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      setSurfaceSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const center = useMemo(() => viewportCenterLatLon(viewport), [viewport]);
  const tiles = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return renderTiles(selectedFamilyMapViews.map((view) => ({ ...view.map_view, id: view.id })), viewport, surfaceSize.width, surfaceSize.height);
  }, [selectedFamilyMapViews, surfaceSize, viewport]);
  const debugSummary = useMemo(() => {
    const tileZooms = [...new Set(tiles.map((tile) => tile.zoom))].sort((a, b) => a - b);
    const packages = [...new Set(tiles.map((tile) => tile.packageName).filter((value): value is string => Boolean(value)))].sort();
    const mapIds = selectedFamilyMapViews.map((view) => view.id);
    return {
      tileZooms,
      packages,
      mapIds,
      tileCount: tiles.length,
    };
  }, [selectedFamilyMapViews, tiles]);
  const situationOverlay = useMemo(
    () => resolveSituationOverlay(situation, viewport, surfaceSize.width, surfaceSize.height),
    [situation, viewport, surfaceSize.height, surfaceSize.width],
  );
  const routeScreenSegments = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return flightPlanRoute.map((segment) => ({
      ...segment,
      from: worldToScreen(viewport, latLonToWorld(segment.from.lat, segment.from.lon), surfaceSize.width, surfaceSize.height),
      to: worldToScreen(viewport, latLonToWorld(segment.to.lat, segment.to.lon), surfaceSize.width, surfaceSize.height),
    }));
  }, [flightPlanRoute, surfaceSize.height, surfaceSize.width, viewport]);

  useEffect(() => {
    let cancelled = false;

    async function resolveFlightPlanRoute() {
      const rawLegs = plan.resolved_legs ?? [];
      const uiLegs = planUiState?.resolved_legs ?? [];
      if (rawLegs.length === 0 || uiLegs.length === 0) {
        setFlightPlanRoute([]);
        return;
      }
      const resolvedPositions = new Map<string, Promise<LatLon>>();
      const resolveCachedPosition = (navRef: NavRef, procedureAirportId?: string | null) => {
        const key = `${navRefKey(navRef)}:${procedureAirportId ?? ""}`;
        let promise = resolvedPositions.get(key);
        if (!promise) {
          promise = resolveNavRefPosition(navRef, procedureAirportId);
          resolvedPositions.set(key, promise);
        }
        return promise;
      };
      const segmentEntries = await Promise.all(rawLegs.map(async (leg, index) => {
        try {
          const airportId = leg.procedure_provenance?.airport_id ?? null;
          return {
            id: leg.id,
            from: await resolveCachedPosition(leg.from, airportId),
            to: await resolveCachedPosition(leg.to, airportId),
            status: routeStatusForLeg(planUiState, uiLegs[index]?.leg_index ?? index),
          };
        } catch (error) {
          console.error("failed to resolve flight plan segment", leg, error);
          return null;
        }
      }));
      const segments = segmentEntries.filter((segment): segment is FlightPlanRouteSegment => segment !== null);
      debugLog("map.route.segments", {
        count: segments.length,
        segments: segments.map((segment) => ({
          id: segment.id,
          from: segment.from,
          to: segment.to,
          status: segment.status,
        })),
      });
      if (!cancelled) {
        setFlightPlanRoute(segments);
      }
    }

    resolveFlightPlanRoute().catch((error: unknown) => {
      console.error("failed to resolve flight plan route", error);
      if (!cancelled) {
        setFlightPlanRoute([]);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [plan, planUiState]);

  useEffect(() => {
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      setMapOverlay({
        needed_point_tiles: [],
        visible_features: [],
        warnings: [],
      });
      return;
    }
    const session = uiSession;
    const controller = new AbortController();
    let cancelled = false;

    async function syncMapOverlay() {
      let overlay: MapOverlayQueryResult;
      const startedAt = performance.now();
      try {
        debugLog("map.overlay.query.start", {
          zoom: viewport.zoom,
          width: surfaceSize.width,
          height: surfaceSize.height,
        });
        overlay = await session.queryMapOverlay(viewport, surfaceSize.width, surfaceSize.height);
        debugLog("map.overlay.query.done", {
          zoom: viewport.zoom,
          elapsed_ms: Math.round(performance.now() - startedAt),
          needed_point_tiles: overlay.needed_point_tiles.length,
          visible_features: overlay.visible_features.length,
          warnings: overlay.warnings.map((warning) => warning.code),
        });
      } catch (error) {
        debugLog("map.overlay.query.error", {
          zoom: viewport.zoom,
          elapsed_ms: Math.round(performance.now() - startedAt),
          error: error instanceof Error ? error.message : String(error),
        });
        throw error;
      }
      if (overlay.needed_point_tiles.length > 0) {
        const tileFetchStartedAt = performance.now();
        debugLog("map.overlay.tiles.fetch.start", {
          zoom: viewport.zoom,
          count: overlay.needed_point_tiles.length,
        });
        const tiles = await Promise.all(
          overlay.needed_point_tiles.map(async (tile) => {
            const response = await fetch(pointTileUrl(tile.layer, tile.z, tile.x, tile.y), {
              signal: controller.signal,
            });
            if (response.status === 404) {
              return {
                schema_version: 1,
                layer: tile.layer,
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: [],
              } satisfies PointTilePayload;
            }
            if (!response.ok) {
              throw new Error(`failed to load vector tile ${tile.z}/${tile.x}/${tile.y}: ${response.status}`);
            }
            return (await response.json()) as PointTilePayload;
          }),
        );
        debugLog("map.overlay.tiles.fetch.done", {
          zoom: viewport.zoom,
          count: tiles.length,
          elapsed_ms: Math.round(performance.now() - tileFetchStartedAt),
        });
        const ingestStartedAt = performance.now();
        await session.ingestPointTiles(tiles);
        debugLog("map.overlay.tiles.ingest.done", {
          zoom: viewport.zoom,
          count: tiles.length,
          elapsed_ms: Math.round(performance.now() - ingestStartedAt),
        });
        const refreshStartedAt = performance.now();
        overlay = await session.queryMapOverlay(viewport, surfaceSize.width, surfaceSize.height);
        debugLog("map.overlay.query.refresh.done", {
          zoom: viewport.zoom,
          elapsed_ms: Math.round(performance.now() - refreshStartedAt),
          needed_point_tiles: overlay.needed_point_tiles.length,
          visible_features: overlay.visible_features.length,
          warnings: overlay.warnings.map((warning) => warning.code),
        });
      }
      if (!cancelled) {
        setMapOverlay(overlay);
        setMapOverlayViewport(viewport);
      }
    }

    syncMapOverlay().catch((error: unknown) => {
      if ((error as { name?: string } | null)?.name === "AbortError") {
        return;
      }
      console.error(error);
    });

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [surfaceSize.height, surfaceSize.width, uiSession, viewport]);

  const overlayTransform = useMemo(() => {
    if (!mapOverlayViewport) {
      return undefined;
    }
    const currentScale = scaleForZoom(viewport.zoom);
    const overlayScale = scaleForZoom(mapOverlayViewport.zoom);
    const scaleRatio = currentScale / overlayScale;
    const dx = (mapOverlayViewport.centerWorldX - viewport.centerWorldX) * currentScale;
    const dy = (mapOverlayViewport.centerWorldY - viewport.centerWorldY) * currentScale;
    return `translate(${dx}px, ${dy}px) scale(${scaleRatio})`;
  }, [mapOverlayViewport, viewport]);

  function updateViewport(next: MapViewportState) {
    viewportRef.current = next;
    onViewportChange(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen) {
      return;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    activePointersRef.current.set(event.pointerId, point);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2 && surfaceSize.width > 0 && surfaceSize.height > 0) {
      const [first, second] = Array.from(activePointersRef.current.values());
      pinchRef.current = createPinchSnapshot(
        viewportRef.current,
        first,
        second,
        surfaceSize.width,
        surfaceSize.height,
      );
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    const pointers = Array.from(activePointersRef.current.entries());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      updateViewport(dragViewport(viewportRef.current, dx, dy));
      dragRef.current = { id: event.pointerId, last: point };
      return;
    }
    if (pointers.length >= 2) {
      const [first, second] = pointers;
      if (!pinchRef.current) {
        pinchRef.current = createPinchSnapshot(
          viewportRef.current,
          first[1],
          second[1],
          surfaceSize.width,
          surfaceSize.height,
        );
      }
      updateViewport(
        applyPinchGesture(
          pinchRef.current,
          first[1],
          second[1],
          selectedMap.map_view,
          surfaceSize.width,
          surfaceSize.height,
        ),
      );
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    updateViewport(
      zoomAroundPoint(
        viewportRef.current,
        selectedMap.map_view,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        viewportRef.current.zoom - event.deltaY / 360,
      ),
    );
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    updateViewport(
      zoomAroundPoint(
        viewportRef.current,
        selectedMap.map_view,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        viewportRef.current.zoom + 0.75,
      ),
    );
  }

  return (
    <section className="pageSurface">
      <div
        ref={containerRef}
        className="mapSurface chartSurface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        <div className="mapBackdrop" />
        {trayGroup.scrimOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}
        {tiles.map((tile) => (
          <div
            key={`${tile.zoom}-${tile.x}-${tile.yTms}`}
            className="mapTile"
            style={{
              left: `${tile.left}px`,
              top: `${tile.top}px`,
              width: `${tile.size}px`,
              height: `${tile.size}px`,
            }}
          >
            <img className="mapTileImage" src={tile.src} alt="" draggable={false} />
            {debugTileLabels ? (
              <div className="tileLabel">
                z{tile.zoom} x{tile.x} y{tile.yTms}
              </div>
            ) : null}
          </div>
        ))}
        {routeScreenSegments.length > 0 ? (
          <svg className="vectorOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
            {routeScreenSegments.map((segment) => (
              <Fragment key={segment.id}>
                <line
                  x1={segment.from.x}
                  y1={segment.from.y}
                  x2={segment.to.x}
                  y2={segment.to.y}
                  stroke="rgba(0, 0, 0, 0.55)"
                  strokeWidth="7"
                  strokeLinecap="round"
                />
                <line
                  x1={segment.from.x}
                  y1={segment.from.y}
                  x2={segment.to.x}
                  y2={segment.to.y}
                  stroke={routeSegmentColor(segment.status)}
                  strokeWidth="3.5"
                  strokeLinecap="round"
                />
              </Fragment>
            ))}
          </svg>
        ) : null}
        {mapOverlay.visible_features.length > 0 ? (
          <svg
            className="vectorOverlay"
            viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
            preserveAspectRatio="none"
            style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
          >
            {mapOverlay.visible_features.map((feature) => {
              const isAirport = feature.style_class === "airport" || feature.kind.toLowerCase() === "airport";
              const isVor = feature.kind.toLowerCase().includes("vor") || feature.style_class === "nav";
              const airportClass = feature.towered ? "airportMarker airportTowered" : "airportMarker airportUntowered";
              const airportLabelClass = feature.towered ? "airportLabel airportToweredLabel" : "airportLabel airportUntoweredLabel";
              return (
                <g key={feature.id} transform={`translate(${feature.screen_x} ${feature.screen_y})`}>
                  {isAirport
                    ? (
                      <>
                        <circle r="12" className={airportClass} />
                        {feature.fuel_available ? <path d={airportFuelTabsPath} className={airportClass} /> : null}
                        {feature.longest_runway_heading_true_deg != null ? (
                          <>
                            {(() => {
                              const halfLength = 8 * Math.max(feature.runway_length_ratio, 0.2);
                              return (
                                <>
                            <line
                              x1="0"
                              y1={-halfLength}
                              x2="0"
                              y2={halfLength}
                              className="airportRunwayBarUnder"
                              transform={`rotate(${feature.longest_runway_heading_true_deg})`}
                            />
                            <line
                              x1="0"
                              y1={-halfLength}
                              x2="0"
                              y2={halfLength}
                              className="airportRunwayBar"
                              transform={`rotate(${feature.longest_runway_heading_true_deg})`}
                            />
                                </>
                              );
                            })()}
                          </>
                        ) : null}
                        <text x="18" y="5" textAnchor="start" className={airportLabelClass}>
                          {feature.label}
                        </text>
                      </>
                      )
                    : isVor
                      ? (
                        <>
                          <path d={vorBandPath} className="vorBand" fillRule="evenodd" />
                          <path d={vorOuterHexPath} className="vorBorder" />
                          <text x="0" y="20" textAnchor="middle" className="vorLabel">
                            {feature.label}
                          </text>
                        </>
                        )
                      : (
                        <>
                          <path d="M 0 -8 L 7 6 L -7 6 Z" className="fixMarker" />
                          <text x="0" y="20" textAnchor="middle" className="fixLabel">
                            {feature.label}
                          </text>
                        </>
                        )}
                </g>
              );
            })}
          </svg>
        ) : null}
        <SituationStatusBadge situation={situation} />
        {situationOverlay ? (
          <>
            <svg className="situationOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
              <circle
                cx={situationOverlay.point.x}
                cy={situationOverlay.point.y}
                r={situationOverlay.ring.radiusPx}
                fill="none"
                stroke="rgba(0, 0, 0, 0.4)"
                strokeWidth="8"
              />
              <circle
                cx={situationOverlay.point.x}
                cy={situationOverlay.point.y}
                r={situationOverlay.ring.radiusPx}
                fill="none"
                stroke="#ffffff"
                strokeWidth="3"
              />
              {situationOverlay.ring.tickMarks.map((tick, index) => (
                <Fragment key={index}>
                  <line
                    x1={tick.inner.x}
                    y1={tick.inner.y}
                    x2={tick.outer.x}
                    y2={tick.outer.y}
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="8"
                    strokeLinecap="round"
                  />
                  <line
                    x1={tick.inner.x}
                    y1={tick.inner.y}
                    x2={tick.outer.x}
                    y2={tick.outer.y}
                    stroke="#ffffff"
                    strokeWidth="3"
                    strokeLinecap="round"
                  />
                </Fragment>
              ))}
              <circle
                cx={situationOverlay.point.x}
                cy={situationOverlay.point.y}
                r={situationOverlay.ring.radiusPx}
                fill="none"
                stroke="#ffffff"
                strokeWidth="3"
              />
              <text
                x={situationOverlay.ring.label.point.x}
                y={situationOverlay.ring.label.point.y}
                fill="none"
                stroke="rgba(0, 0, 0, 0.4)"
                strokeWidth="5"
                strokeLinejoin="round"
                fontSize="16"
                fontWeight="700"
                textAnchor="middle"
                dominantBaseline="middle"
                transform={`rotate(${situationOverlay.ring.label.rotationDeg} ${situationOverlay.ring.label.point.x} ${situationOverlay.ring.label.point.y})`}
              >
                {situationOverlay.ring.label.text}
              </text>
              <text
                x={situationOverlay.ring.label.point.x}
                y={situationOverlay.ring.label.point.y}
                fill="#ffffff"
                fontSize="16"
                fontWeight="700"
                textAnchor="middle"
                dominantBaseline="middle"
                transform={`rotate(${situationOverlay.ring.label.rotationDeg} ${situationOverlay.ring.label.point.x} ${situationOverlay.ring.label.point.y})`}
              >
                {situationOverlay.ring.label.text}
              </text>
              {situationOverlay.predictor ? (
                <g>
                  {(() => {
                    const shaftEnd = arrowShaftEndPoint(situationOverlay.point, situationOverlay.predictor);
                    return (
                      <>
                  <line
                    x1={situationOverlay.point.x}
                    y1={situationOverlay.point.y}
                    x2={shaftEnd.x}
                    y2={shaftEnd.y}
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="8"
                    strokeLinecap="round"
                  />
                  <line
                    x1={situationOverlay.point.x}
                    y1={situationOverlay.point.y}
                    x2={shaftEnd.x}
                    y2={shaftEnd.y}
                    stroke="#ffffff"
                    strokeWidth="6"
                    strokeLinecap="round"
                  />
                  <polygon
                    points={arrowHeadPoints(situationOverlay.point, situationOverlay.predictor)}
                    fill="#ffffff"
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="1.5"
                    strokeLinejoin="round"
                  />
                      </>
                    );
                  })()}
                </g>
              ) : null}
            </svg>
            <SituationAircraft
              iconSrc={planViewIcon}
              point={situationOverlay.point}
              headingDeg={situationOverlay.headingDeg}
            />
          </>
        ) : null}

        <div className="chartDock">
          <TrayDock
            launcherLabel={pageOptions.find((option) => option.id === page)?.launcherLabel ?? "CHT"}
            open={trayGroup.isOpen("page")}
            blocked={trayGroup.blocked("page")}
            onToggle={() => trayGroup.toggle("page")}
            ariaLabel="Page"
            options={pageOptions.map((option) => ({
              id: option.id,
              label: option.label,
              active: option.id === page,
              onSelect: () => {
                onSelectPage(option.id);
                trayGroup.close("page");
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedFamily.launcherLabel}
            open={trayGroup.isOpen("family")}
            blocked={trayGroup.blocked("family")}
            onToggle={() => trayGroup.toggle("family")}
            ariaLabel="Chart family"
            options={chartFamilies.map((family) => {
              const available = availableFamilies.has(family.id);
              const active = selectedMap.map_view.chart_family === family.id;
              return {
                id: family.id,
                label: family.label,
                active,
                disabled: !available,
                onSelect: () => {
                  const nextMap = mapViews.find((view) => view.map_view.chart_family === family.id);
                  if (nextMap) {
                    onSelectMapId(nextMap.id);
                  }
                  trayGroup.close("family");
                },
              };
            })}
          />
        </div>

        <button
          type="button"
          className="navElement"
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        >
          <span className="navElementTop">{legSummary}</span>
          <span className="navElementBottom">° ° ^| ° °</span>
        </button>

        <div className="debugDock">
          <DebugDock
            open={debugOpen}
            warn={mapOverlay.warnings.length > 0}
            onToggle={() => setDebugOpen((open) => !open)}
          >
            <div className="debugLine">page {pageLabel(page)}</div>
            <div className="debugLine">core {adapterBackend}</div>
            <div className="debugLine">{adapterDetail}</div>
            <div className="debugLine">session {uiSession ? "ready" : "null"} surf {Math.round(surfaceSize.width)}x{Math.round(surfaceSize.height)}</div>
            <div className="debugLine">up {uptimeLabel}</div>
            <div className="debugLine">stack {formatPageStack(pageHistory, { page, selectedMapId: selectedMap.id, selectedChartId: "", selectedChartLabel: "", chartFolderOpen: false })}</div>
            <div className="debugLine">family {selectedFamily.launcherLabel}</div>
            <div className="debugLine">{center.lat.toFixed(3)}/{center.lon.toFixed(3)} z{viewport.zoom.toFixed(2)}</div>
            <div className="debugLine">vec pts={mapOverlay.visible_features.length} need={mapOverlay.needed_point_tiles.length} warn={mapOverlay.warnings.length}</div>
            {mapOverlay.warnings.map((warning) => (
              <div key={warning.code} className="debugLine">warn {warning.code}</div>
            ))}
            <div className="debugLine">src z {debugSummary.tileZooms.length > 0 ? debugSummary.tileZooms.join(", ") : "(none)"}</div>
            <div className="debugLine">pkg {debugSummary.packages.length > 0 ? debugSummary.packages.join(", ") : "(none)"}</div>
            <div className="debugLine">maps {debugSummary.mapIds.join(", ")}</div>
            <div className="debugLine">search {locationSearch || "(empty)"}</div>
            <div className="debugLine">{debugTileLabels ? "debugTiles=on" : "debugTiles=off"}</div>
          </DebugDock>
        </div>
      </div>
    </section>
  );
}

function FlightPlanPage(props: {
  appCoreAdapter: AppCoreAdapter | null;
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  legSummary: string;
  plan: typeof samplePlan;
  planUiState: FlightPlanUiState | null;
  onSelectPage: (page: AppPage) => void;
  onOpenCharts: (airportId: string | null) => void;
  onMoveComponent: (componentIndex: number, delta: number) => void | Promise<void>;
  onActivateLeg: (index: number) => void | Promise<void>;
  onDeleteComponent: (componentIndex: number) => void | Promise<void>;
  onActivateNextLeg: () => void | Promise<void>;
  onSuspendSequencing: () => void | Promise<void>;
  onUnsuspendSequencing: () => void | Promise<void>;
  onSequenceActiveLeg: () => void | Promise<void>;
  onInsertAirway: (
    startComponentIndex: number,
    endComponentIndex: number | null,
    entryIndex: number,
    exitIndex: number,
    presentation: AirwayPresentationPlan,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ) => void | Promise<void>;
  onReplaceAirway: (
    componentIndex: number,
    entryIndex: number,
    exitIndex: number,
    presentation: AirwayPresentationPlan,
    originAnchor: NavRef,
    destinationAnchor: NavRef | null,
  ) => void | Promise<void>;
  onInsertProcedure: (
    startComponentIndex: number,
    endComponentIndex: number,
    built: MaterializedProcedure,
  ) => void | Promise<void>;
}) {
  const [selectedWaypointIndex, setSelectedWaypointIndex] = useState<number | null>(null);
  const [pendingSelectedComponentIndex, setPendingSelectedComponentIndex] = useState<number | null>(null);
  const [selectedWaypointAnchor, setSelectedWaypointAnchor] = useState<{ top: number; height: number } | null>(null);
  const [reorderOpen, setReorderOpen] = useState(false);
  const [airwayPicker, setAirwayPicker] = useState<{
    loading: boolean;
    error: string | null;
    mode: "insert" | "replace";
    componentIndex: number | null;
    startComponentIndex: number | null;
    endComponentIndex: number | null;
    originAnchor: NavRef;
    destinationAnchor: NavRef | null;
    suggestions: AirwaySuggestion[];
    selectedAirwayName: string | null;
    presentation: AirwayPresentationPlan | null;
    selectedEntryIndex: number | null;
  } | null>(null);
  const [procedurePicker, setProcedurePicker] = useState<{
    loading: boolean;
    error: string | null;
    airportId: string;
    startComponentIndex: number;
    endComponentIndex: number;
    procedures: ProcedureSummary[];
    selectedProcedureId: string | null;
    options: ProcedureOptions | null;
  } | null>(null);
  const trayGroup = useModalTrayGroup(["page"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const pageRef = useRef<HTMLElement | null>(null);
  const planScrollSurfaceRef = useRef<HTMLDivElement | null>(null);
  const waypointModalRef = useRef<HTMLElement | null>(null);
  const trayOpen = trayGroup.scrimOpen;
  const guidance = props.planUiState?.guidance ?? null;
  const structuredSurfaceRef = useRef<HTMLDivElement | null>(null);
  const structuredTableRef = useRef<HTMLDivElement | null>(null);
  const structuredRowRefs = useRef(new Map<string, HTMLElement>());
  const [structuredArrow, setStructuredArrow] = useState<{ path: string; head: string } | null>(null);
  const [structuredGroupBoxes, setStructuredGroupBoxes] = useState<Array<{ key: string; top: number; left: number; width: number; height: number }>>([]);
  const [waypointModalTop, setWaypointModalTop] = useState<number | null>(null);
  const [waypointModalMaxHeight, setWaypointModalMaxHeight] = useState<number | null>(null);
  const componentViews = useMemo(
    () => props.planUiState?.components ?? buildLegacyComponentViews(props.plan),
    [props.plan, props.planUiState?.components],
  );
  if (props.planUiState && props.planUiState.resolved_legs.length > 0 && componentViews.length === 0) {
    throw new Error("FlightPlanUiState invariant failed: resolved legs present but components are empty");
  }
  const showComponentViews = useMemo(
    () => componentViews.some((component) => component.kind !== "waypoint"),
    [componentViews],
  );
  const hierarchicalRows = useMemo(() => {
    const nextRows: Array<{
      id: string;
      kind: "group" | "waypoint" | "discontinuity";
      label: string;
      navRef: NavRef | null;
      depth: number;
      groupKey: string | null;
      componentIndex: number | null;
    }> = [];

    for (const component of componentViews) {
      if (component.kind === "waypoint") {
        const navRef = componentWaypointNavRef(component);
        nextRows.push({
          id: `component:${component.component_index}`,
          kind: "waypoint",
          label: navRef ? navRefLabel(navRef) : component.summary,
          navRef,
          depth: 0,
          groupKey: null,
          componentIndex: component.component_index,
        });
        continue;
      }

      const groupKey = `group:${component.component_index}`;
      nextRows.push({
        id: groupKey,
        kind: "group",
        label: structuredComponentLabel(component),
        navRef: null,
        depth: 0,
        groupKey,
        componentIndex: component.component_index,
      });

      component.items.forEach((item, index) => {
        nextRows.push({
          id: `group:${component.component_index}:item:${index}`,
          kind: item.kind === "waypoint" ? "waypoint" : "discontinuity",
          label: concretizedNavItemLabel(item),
          navRef: item.kind === "waypoint" ? item.nav_ref : null,
          depth: 1,
          groupKey,
          componentIndex: component.component_index,
        });
      });
    }

    return nextRows;
  }, [componentViews]);
  const displayRows = useMemo(() => {
    const projectedRows = props.planUiState?.display_rows;
    if (projectedRows && projectedRows.length > 0) {
      return projectedRows.map((row, index) => ({
        id:
          row.row_kind === "group"
            ? `group:${row.component_index ?? index}`
            : row.row_kind === "discontinuity"
              ? `disc:${row.component_index ?? "x"}:${index}`
              : row.depth === 0
                ? `component:${row.component_index ?? index}`
                : `item:${row.component_index ?? "x"}:${row.label}:${index}`,
        label: row.label,
        distance: row.row_kind === "group" ? "" : row.leg_index !== null ? "11.2" : "—",
        ete: row.row_kind === "group" ? "" : row.leg_index !== null ? "0:04" : "—",
        course: row.row_kind === "group" ? "" : row.active ? "ACT" : row.leg_index !== null ? "161" : "—",
        active: row.active,
        depth: row.depth,
        rowKind: row.row_kind,
        refKey:
          row.row_kind === "group"
            ? `group:${row.component_kind ?? "group"}:${row.label}:${row.origin_anchor ? navRefKey(row.origin_anchor) : "none"}:${row.destination_anchor ? navRefKey(row.destination_anchor) : "none"}`
            : row.row_kind === "discontinuity"
              ? `disc:${row.component_kind ?? "row"}:${index}`
              : row.depth === 0
                ? `waypoint:${row.nav_ref ? navRefKey(row.nav_ref) : "none"}`
                : `child:${row.component_kind ?? "row"}:${row.nav_ref ? navRefKey(row.nav_ref) : "none"}:${index}`,
        chartAirportId: row.chart_airport_id,
        legIndex: row.leg_index,
        removeLegIndex: null as number | null,
        startComponentIndex: row.start_component_index,
        endComponentIndex: row.end_component_index,
        originAnchor: row.origin_anchor,
        destinationAnchor: row.destination_anchor,
        navRef: row.nav_ref,
        groupKey: row.row_kind === "group" || row.depth > 0 ? `group:${row.component_index ?? index}` : null,
        componentIndex: row.component_index,
        componentKind: row.component_kind,
        canAddAirwayAfter: row.can_add_airway_after,
        canAddProcedureBefore: row.can_add_procedure_before,
        canChangeAirway: row.can_change_airway,
        canRemoveComponent: row.can_remove_component,
        canReorderComponent: row.can_reorder_component,
        precedingWaypoint: row.preceding_waypoint,
        followingWaypoint: row.following_waypoint,
        actions: row.actions,
      }));
    }
    const resolvedLegs = props.planUiState?.resolved_legs ?? [];
    const componentByIndex = new Map(componentViews.map((component) => [component.component_index, component]));
    let nextLegCursor = 0;
    return hierarchicalRows.map((row) => {
      let matchingLeg = null as (typeof resolvedLegs)[number] | null;
      if (row.kind === "waypoint" && row.navRef) {
        for (let index = nextLegCursor; index < resolvedLegs.length; index += 1) {
          const leg = resolvedLegs[index];
          if (navRefsEqual(leg.to, row.navRef)) {
            matchingLeg = leg;
            nextLegCursor = index + 1;
            break;
          }
        }
      }

      const chartAirportId = row.navRef && "Airport" in row.navRef ? row.navRef.Airport : null;
      const nextTopLevelWaypoint = row.depth === 0 && row.kind === "waypoint"
        ? hierarchicalRows.slice(hierarchicalRows.indexOf(row) + 1).find((candidate) => candidate.depth === 0 && candidate.kind === "waypoint" && candidate.navRef)
        : null;

      return {
        id: row.id,
        label: row.label,
        distance: row.kind === "group" ? "" : matchingLeg ? "11.2" : "—",
        ete: row.kind === "group" ? "" : matchingLeg ? "0:04" : "—",
        course: row.kind === "group" ? "" : matchingLeg?.active ? "ACT" : matchingLeg?.suspend_boundary_after ? "SUSP" : matchingLeg ? "161" : "—",
        active: matchingLeg?.active ?? false,
        depth: row.depth,
        rowKind: row.kind,
        refKey: row.id,
        chartAirportId,
        legIndex: row.kind === "waypoint" ? matchingLeg?.leg_index ?? null : null,
        removeLegIndex: null as number | null,
        startComponentIndex:
          row.depth === 0 && row.kind === "waypoint" && row.componentIndex !== null
            ? row.componentIndex
            : null as number | null,
        endComponentIndex:
          row.depth === 0 && row.kind === "waypoint" && nextTopLevelWaypoint && nextTopLevelWaypoint.componentIndex !== null
            ? nextTopLevelWaypoint.componentIndex
            : null as number | null,
        originAnchor:
          row.depth === 0 && row.kind === "waypoint" && row.navRef
            ? row.navRef
            : null as NavRef | null,
        destinationAnchor:
          row.depth === 0 && row.kind === "waypoint" && nextTopLevelWaypoint?.navRef
            ? nextTopLevelWaypoint.navRef
            : null as NavRef | null,
        navRef: row.navRef,
        groupKey: row.groupKey,
        componentIndex: row.componentIndex,
        componentKind: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.kind ?? null : null,
        canAddAirwayAfter: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.can_add_airway_after ?? false : false,
        canAddProcedureBefore: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.can_add_procedure_before ?? false : false,
        canChangeAirway: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.can_change_airway ?? false : false,
        canRemoveComponent: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.can_remove ?? false : false,
        canReorderComponent: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.can_reorder ?? false : false,
        precedingWaypoint: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.preceding_waypoint ?? null : null,
        followingWaypoint: row.componentIndex !== null ? componentByIndex.get(row.componentIndex)?.following_waypoint ?? null : null,
        actions: [] as Array<{ id: string; enabled: boolean }>,
      };
    });
  }, [componentViews, hierarchicalRows, props.planUiState?.display_rows, props.planUiState?.resolved_legs]);
  const selectedRow = selectedWaypointIndex !== null ? displayRows[selectedWaypointIndex] ?? null : null;

  useEffect(() => {
    if (pendingSelectedComponentIndex === null) {
      return;
    }
    const nextIndex = displayRows.findIndex(
      (row) =>
        row.componentIndex === pendingSelectedComponentIndex &&
        row.depth === 0 &&
        (row.rowKind === "waypoint" || row.rowKind === "group"),
    );
    if (nextIndex >= 0) {
      setSelectedWaypointIndex(nextIndex);
    } else {
      setSelectedWaypointIndex(null);
      setReorderOpen(false);
    }
    setPendingSelectedComponentIndex(null);
  }, [displayRows, pendingSelectedComponentIndex]);

  const rowActions = useMemo(() => {
    if (!selectedRow) {
      return [] as Array<{ id: string; label: string; enabled: boolean; onSelect: () => void }>;
    }

    const closeTray = () => {
      setReorderOpen(false);
      setSelectedWaypointIndex(null);
      setAirwayPicker(null);
      setProcedurePicker(null);
    };

    return (selectedRow.actions as Array<{ id: string; enabled: boolean }>).map((action) => {
      return {
        id: action.id,
        label: flightPlanActionLabel(action.id),
        enabled: action.enabled,
        onSelect: () => {
          if (!action.enabled) {
            return;
          }
          if (action.id === "activate_leg") {
            void props.onActivateLeg(selectedRow.legIndex!);
            closeTray();
            return;
          }
          if (action.id === "remove" || action.id === "remove_airway" || action.id === "remove_procedure") {
            void props.onDeleteComponent(selectedRow.componentIndex!);
            closeTray();
            return;
          }
          if (action.id === "reorder") {
            setReorderOpen(true);
            return;
          }
          if (action.id === "add_airway") {
            const adapter = props.appCoreAdapter;
            if (!adapter) {
              return;
            }
            setAirwayPicker({
              loading: true,
              error: null,
              mode: "insert",
              componentIndex: null,
              startComponentIndex: selectedRow.startComponentIndex!,
              endComponentIndex: selectedRow.endComponentIndex!,
              originAnchor: selectedRow.originAnchor!,
              destinationAnchor: selectedRow.destinationAnchor!,
              suggestions: [],
              selectedAirwayName: null,
              presentation: null,
              selectedEntryIndex: null,
            });
            window.requestAnimationFrame(() => {
              void suggestAirwaysNearAnchor(adapter, selectedRow.originAnchor!).then((suggestions) => {
                setAirwayPicker((current) => current ? {
                  ...current,
                  loading: false,
                  suggestions,
                } : current);
              }).catch((error) => {
                setAirwayPicker((current) => current ? {
                  ...current,
                  loading: false,
                  error: error instanceof Error ? error.message : String(error),
                } : current);
              });
            });
            return;
          }
          if (action.id === "select_procedure") {
            if (selectedRow.componentIndex === null || !selectedRow.chartAirportId) {
              return;
            }
            const startComponentIndex = selectedRow.componentIndex - 1;
            const endComponentIndex = selectedRow.componentIndex;
            setProcedurePicker({
              loading: true,
              error: null,
              airportId: selectedRow.chartAirportId,
              startComponentIndex,
              endComponentIndex,
              procedures: [],
              selectedProcedureId: null,
              options: null,
            });
            window.requestAnimationFrame(() => {
              void props.appCoreAdapter!.listProcedures(selectedRow.chartAirportId!, "approach").then((procedures) => {
                setProcedurePicker((current) => current ? {
                  ...current,
                  loading: false,
                  procedures,
                } : current);
              }).catch((error) => {
                setProcedurePicker((current) => current ? {
                  ...current,
                  loading: false,
                  error: error instanceof Error ? error.message : String(error),
                } : current);
              });
            });
            return;
          }
          if (action.id === "charts" || action.id === "plates") {
            props.onOpenCharts(selectedRow.chartAirportId);
            closeTray();
          }
        },
      };
    });
  }, [props, selectedRow]);

  useEffect(() => {
    if (!showComponentViews) {
      setStructuredGroupBoxes([]);
      return;
    }
    const surface = structuredSurfaceRef.current;
    const table = structuredTableRef.current;
    if (!surface || !table) {
      setStructuredGroupBoxes([]);
      return;
    }

    let animationFrame = 0;
    let settleTimer = 0;

    const measureGroupBoxes = () => {
      const surfaceRect = surface.getBoundingClientRect();
      const tableRect = table.getBoundingClientRect();
      const computedStyle = window.getComputedStyle(table);
      const rowGap = Number.parseFloat(computedStyle.rowGap || computedStyle.gap || "0") || 0;
      const columnGap = Number.parseFloat(computedStyle.columnGap || computedStyle.gap || "0") || 0;
      const verticalInset = rowGap * 0.6;
      const horizontalInset = columnGap * 0.6;
      const orderedGroupKeys = hierarchicalRows
        .filter((row) => row.kind === "group" && row.groupKey)
        .map((row) => row.groupKey as string);

      const nextBoxes = orderedGroupKeys.flatMap((groupKey) => {
        const groupRows = hierarchicalRows.filter((row) => row.groupKey === groupKey);
        const firstRow = groupRows[0];
        const lastRow = groupRows[groupRows.length - 1];
        if (!firstRow || !lastRow) {
          return [];
        }
        const firstElement = structuredRowRefs.current.get(firstRow.id);
        const lastElement = structuredRowRefs.current.get(lastRow.id);
        if (!firstElement || !lastElement) {
          return [];
        }
        const firstRect = firstElement.getBoundingClientRect();
        const lastRect = lastElement.getBoundingClientRect();
        return [{
          key: groupKey,
          left: tableRect.left - surfaceRect.left - horizontalInset,
          width: tableRect.width + horizontalInset * 2,
          top: firstRect.top - surfaceRect.top - verticalInset,
          height: lastRect.bottom - firstRect.top + verticalInset * 2,
        }];
      });
      setStructuredGroupBoxes(nextBoxes);
    };

    const scheduleMeasure = () => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(measureGroupBoxes);
    };

    const handleTransitionEnd = () => {
      scheduleMeasure();
    };

    scheduleMeasure();
    settleTimer = window.setTimeout(measureGroupBoxes, 220);
    table.addEventListener("transitionend", handleTransitionEnd);

    return () => {
      window.cancelAnimationFrame(animationFrame);
      window.clearTimeout(settleTimer);
      table.removeEventListener("transitionend", handleTransitionEnd);
    };
  }, [hierarchicalRows, reorderOpen, showComponentViews]);

  useEffect(() => {
    if (!showComponentViews || !guidance?.active_leg) {
      setStructuredArrow(null);
      return;
    }
    const activeLeg = guidance.active_leg;
    const surface = structuredSurfaceRef.current;
    if (!surface) {
      setStructuredArrow(null);
      return;
    }

    const fromIndex = displayRows.findIndex((row) => row.rowKind === "waypoint" && navRefsEqual(row.navRef, activeLeg.from));
    if (fromIndex < 0) {
      setStructuredArrow(null);
      return;
    }

    let toIndex = -1;
    for (let index = fromIndex + 1; index < displayRows.length; index += 1) {
      const row = displayRows[index];
      if (row.rowKind === "waypoint" && navRefsEqual(row.navRef, activeLeg.to)) {
        toIndex = index;
        break;
      }
    }
    if (toIndex < 0) {
      toIndex = displayRows.findIndex((row) => row.rowKind === "waypoint" && navRefsEqual(row.navRef, activeLeg.to));
    }
    if (toIndex < 0) {
      setStructuredArrow(null);
      return;
    }

    const fromElement = structuredRowRefs.current.get(displayRows[fromIndex]?.id ?? "");
    const toElement = structuredRowRefs.current.get(displayRows[toIndex]?.id ?? "");
    if (!fromElement || !toElement) {
      setStructuredArrow(null);
      return;
    }

    const surfaceRect = surface.getBoundingClientRect();
    const fromRect = fromElement.getBoundingClientRect();
    const toRect = toElement.getBoundingClientRect();
    const fromPoint = {
      x: fromRect.left - surfaceRect.left,
      y: fromRect.top - surfaceRect.top + fromRect.height / 2,
    };
    const toPoint = {
      x: toRect.left - surfaceRect.left,
      y: toRect.top - surfaceRect.top + toRect.height / 2,
    };
    const elbowX = thumbPixels(0.12);
    const headLength = 20;
    const shaftEnd = { x: Math.max(elbowX, toPoint.x - headLength + 5), y: toPoint.y };

    setStructuredArrow({
      path: `M ${fromPoint.x} ${fromPoint.y} H ${elbowX} V ${toPoint.y} H ${shaftEnd.x}`,
      head: arrowHeadPoints(shaftEnd, toPoint),
    });

    const handle = window.requestAnimationFrame(() => {
      fromElement.scrollIntoView({ block: "nearest", inline: "nearest" });
      toElement.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(handle);
  }, [displayRows, guidance?.active_leg, showComponentViews]);

  useEffect(() => {
    if (selectedWaypointIndex === null) {
      setWaypointModalTop(null);
      setWaypointModalMaxHeight(null);
      return;
    }
    const page = pageRef.current;
    const pane = planScrollSurfaceRef.current;
    const modal = waypointModalRef.current;
    const anchor = selectedWaypointAnchor;
    if (!page || !pane || !modal || !anchor) {
      return;
    }
    const pageRect = page.getBoundingClientRect();
    const paneRect = pane.getBoundingClientRect();
    const paneTop = paneRect.top - pageRect.top;
    const paneBottom = paneRect.bottom - pageRect.top;
    const topPadding = thumbPixels(0.1);
    const bottomPadding = thumbPixels(0.1);
    const desiredTop = anchor.top;
    const minTop = paneTop + topPadding;
    const maxTop = Math.max(minTop, paneBottom - modal.offsetHeight - bottomPadding);
    const clampedTop = Math.max(minTop, Math.min(desiredTop, maxTop));
    const maxHeight = Math.max(thumbPixels(1), paneBottom - clampedTop - bottomPadding);
    setWaypointModalTop(clampedTop);
    setWaypointModalMaxHeight(maxHeight);
  }, [airwayPicker, reorderOpen, selectedWaypointAnchor, selectedWaypointIndex, rowActions.length]);

  useEffect(() => {
    if (!airwayPicker || airwayPicker.loading) {
      return;
    }
    if (!airwayPicker.presentation || airwayPicker.selectedAirwayName === null) {
      return;
    }
    const modal = waypointModalRef.current;
    if (!modal) {
      return;
    }
    const handle = window.requestAnimationFrame(() => {
      const suggested = modal.querySelector<HTMLButtonElement>(".airwayChoiceButton.isSuggested");
      suggested?.scrollIntoView({ block: "center", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(handle);
  }, [
    airwayPicker?.loading,
    airwayPicker?.presentation,
    airwayPicker?.selectedAirwayName,
    airwayPicker?.selectedEntryIndex,
  ]);

  useEffect(() => {
    if (!reorderOpen || selectedWaypointIndex === null) {
      return;
    }
    const row = displayRows[selectedWaypointIndex];
    if (!row?.refKey) {
      return;
    }
    const element = structuredRowRefs.current.get(row.refKey);
    if (!element) {
      return;
    }
    const handle = window.requestAnimationFrame(() => {
      element.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(handle);
  }, [displayRows, reorderOpen, selectedWaypointIndex]);

  return (
    <section className="appPage planPage" ref={pageRef}>
      {trayOpen ? <TrayScrim ariaLabel="Close page tray" onClose={trayGroup.closeAll} /> : null}

      <div className="chartDock">
        <TrayDock
          launcherLabel={pageOptions.find((option) => option.id === props.page)?.launcherLabel ?? "PLN"}
          open={trayGroup.isOpen("page")}
          blocked={selectedWaypointIndex !== null}
          onToggle={() => trayGroup.toggle("page")}
          ariaLabel="Page"
          options={pageOptions.map((option) => ({
            id: option.id,
            label: option.label,
            active: option.id === props.page,
            onSelect: () => {
              props.onSelectPage(option.id);
              trayGroup.close("page");
            },
          }))}
        />
      </div>

      <div className="planScrollSurface" ref={planScrollSurfaceRef}>
        <div className={`planTableWrap${showComponentViews ? " isStructured" : ""}${reorderOpen ? " isReordering" : ""}`} ref={structuredSurfaceRef}>
          {showComponentViews ? (
            <div className="planStructuredGroupBoxLayer" aria-hidden="true">
              {structuredGroupBoxes.map((box) => (
                <div
                  key={box.key}
                  className="planStructuredGroupBoxOverlay"
                  style={{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px`, height: `${box.height}px` }}
                />
              ))}
            </div>
          ) : null}
          {showComponentViews && structuredArrow ? (
            <svg className="planStructuredArrowLayer" aria-hidden="true">
              <path className="planStructuredArrowPath" d={structuredArrow.path} />
              <polygon className="planStructuredArrowHead" points={structuredArrow.head} />
            </svg>
          ) : null}
          <div className="planTable" ref={structuredTableRef}>
            <div className="planHeader planWaypointCell">Waypoint</div>
            <div className="planHeader">Dist (nm)</div>
            <div className="planHeader">ETE (h:m)</div>
            <div className="planHeader">Course (°)</div>
            {displayRows.map((row, index) => (
              <Fragment key={row.id}>
                <button
                  key={`${row.id}:waypoint`}
                  type="button"
                  ref={(node) => {
                    if (!showComponentViews || row.refKey === null) {
                      return;
                    }
                    if (node) {
                      structuredRowRefs.current.set(row.refKey, node);
                    } else {
                      structuredRowRefs.current.delete(row.refKey);
                    }
                  }}
                  className={[
                    "planWaypointCell",
                    "planWaypointButton",
                    selectedWaypointIndex === index ? "isSelected" : "",
                    row.active ? "isActiveLeg" : "",
                    showComponentViews ? "planStructuredWaypointCell" : "",
                    showComponentViews && row.rowKind === "group" ? "isGroupHeader" : "",
                    showComponentViews && row.depth > 0 ? "isChildRow" : "",
                    showComponentViews && row.rowKind === "discontinuity" ? "isDiscontinuityItem" : "",
                  ].filter(Boolean).join(" ")}
                  onClick={(event) => {
                    const page = pageRef.current;
                    if (page) {
                      const pageRect = page.getBoundingClientRect();
                      const rowRect = event.currentTarget.getBoundingClientRect();
                      setSelectedWaypointAnchor({
                        top: rowRect.top - pageRect.top,
                        height: rowRect.height,
                      });
                    }
                    setSelectedWaypointIndex(index);
                    setReorderOpen(false);
                    setAirwayPicker(null);
                    setProcedurePicker(null);
                  }}
                >
                  <span className={`planStructuredLabel${showComponentViews && row.depth > 0 ? " isIndented" : ""}`}>{row.label}</span>
                </button>
                <div
                  className={[
                    "planCell",
                    showComponentViews && row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
                  ].filter(Boolean).join(" ")}
                >
                  {row.distance}
                </div>
                <div
                  className={[
                    "planCell",
                    showComponentViews && row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
                  ].filter(Boolean).join(" ")}
                >
                  {row.ete}
                </div>
                <div
                  className={[
                    "planCell",
                    showComponentViews && row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
                  ].filter(Boolean).join(" ")}
                >
                  {row.course}
                </div>
              </Fragment>
            ))}
          </div>
        </div>
      </div>

      <div className="planControls">
        <button type="button" className="trayButton planControlButton" disabled={!guidance?.can_activate_next_leg} onClick={() => void props.onActivateNextLeg()}>
          Next Leg
        </button>
        <button
          type="button"
          className="trayButton planControlButton"
          disabled={!guidance?.can_sequence_active_leg}
          onClick={() => void props.onSequenceActiveLeg()}
        >
          Sequence
        </button>
        <button type="button" className="trayButton planControlButton" disabled={!guidance?.can_suspend} onClick={() => void props.onSuspendSequencing()}>
          Suspend
        </button>
        <button type="button" className="trayButton planControlButton" disabled={!guidance?.can_unsuspend} onClick={() => void props.onUnsuspendSequencing()}>
          Unsusp
        </button>
      </div>

      <div className="planFooter">
        <div>{props.legSummary}</div>
        {guidance ? (
          <div className="planGuidanceSummary">
            {guidance.sequencing_mode.toUpperCase()}
            {guidance.active_leg ? ` · ${navRefLabel(guidance.active_leg.from)} -> ${navRefLabel(guidance.active_leg.to)}` : ""}
          </div>
        ) : null}
      </div>

      <div className="debugDock">
        <DebugDock open={debugOpen} onToggle={() => setDebugOpen((open) => !open)}>
          <div className="debugLine">page {pageLabel(props.page)}</div>
          <div className="debugLine">up {props.uptimeLabel}</div>
          <div className="debugLine">stack {formatPageStack(props.pageHistory, { page: props.page, selectedMapId: "", selectedChartId: "", selectedChartLabel: "", chartFolderOpen: false })}</div>
          <div className="debugLine">components {componentViews.length}</div>
          <div className="debugLine">grouped {showComponentViews ? "yes" : "no"}</div>
          <div className="debugLine">rows {displayRows.length}</div>
        </DebugDock>
      </div>

      {selectedWaypointIndex !== null ? (
        <>
          <button
            type="button"
            className="trayScrim"
            aria-label="Close waypoint actions"
            onClick={() => {
              setSelectedWaypointIndex(null);
              setSelectedWaypointAnchor(null);
              setReorderOpen(false);
              setAirwayPicker(null);
              setProcedurePicker(null);
            }}
          />
          <section
            ref={waypointModalRef}
            className={`waypointModal${reorderOpen ? " isReorder" : ""}`}
            aria-label="Waypoint actions"
            style={waypointModalTop === null ? undefined : {
              top: `${waypointModalTop}px`,
              maxHeight: waypointModalMaxHeight === null ? undefined : `${waypointModalMaxHeight}px`,
            }}
          >
            {procedurePicker ? (
              <div className="waypointActionTray">
                <div className="planGuidanceSummary">
                  APPROACH {procedurePicker.airportId}
                </div>
                {procedurePicker.error ? <div className="planGuidanceSummary">{procedurePicker.error}</div> : null}
                {procedurePicker.loading ? (
                  <div className="airwayLoadingPanel" aria-live="polite">
                    <div className="spinner" aria-hidden="true" />
                    <div className="planGuidanceSummary">Loading…</div>
                  </div>
                ) : procedurePicker.selectedProcedureId === null ? (
                  <>
                    {procedurePicker.procedures.map((procedure) => (
                      <button
                        key={procedure.procedure_id}
                        type="button"
                        className="trayButton airwayChoiceButton"
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            const options = await props.appCoreAdapter!.describeProcedureOptions(
                              procedurePicker.airportId,
                              procedure.procedure_id,
                              "approach",
                            );
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              selectedProcedureId: procedure.procedure_id,
                              options,
                            } : current);
                          } catch (error) {
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {procedure.procedure_id}
                      </button>
                    ))}
                  </>
                ) : procedurePicker.options ? (
                  <>
                    {procedurePicker.options.valid_choices.map((choice, index) => (
                      <button
                        key={`${procedurePicker.selectedProcedureId}:${choice.enroute_transition ?? "none"}:${index}`}
                        type="button"
                        className="trayButton airwayChoiceButton"
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            const built = await props.appCoreAdapter!.materializeProcedure(
                              procedurePicker.airportId,
                              procedurePicker.selectedProcedureId!,
                              "approach",
                              null,
                              choice.enroute_transition,
                              procedurePicker.startComponentIndex + 1,
                            );
                            await props.onInsertProcedure(
                              procedurePicker.startComponentIndex,
                              procedurePicker.endComponentIndex,
                              built,
                            );
                            setProcedurePicker(null);
                            setSelectedWaypointIndex(null);
                          } catch (error) {
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {choice.enroute_transition ?? "No Transition"}
                      </button>
                    ))}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setProcedurePicker((current) => current ? {
                        ...current,
                        selectedProcedureId: null,
                        options: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                ) : null}
              </div>
            ) : airwayPicker ? (
              <div className="waypointActionTray">
                <div className="planGuidanceSummary">
                  AIRWAY {navRefLabel(airwayPicker.originAnchor)}
                  {airwayPicker.destinationAnchor ? ` → ${navRefLabel(airwayPicker.destinationAnchor)}` : ""}
                </div>
                {airwayPicker.error ? <div className="planGuidanceSummary">{airwayPicker.error}</div> : null}
                {airwayPicker.loading ? (
                  <div className="airwayLoadingPanel" aria-live="polite">
                    <div className="spinner" aria-hidden="true" />
                    <div className="planGuidanceSummary">Loading…</div>
                  </div>
                ) : airwayPicker.selectedAirwayName === null ? (
                  <div className="airwaySuggestionGrid">
                    {airwayPicker.suggestions.map((suggestion) => (
                        <button
                          key={`${suggestion.airway_name}:${suggestion.nearest_branch_key ?? ""}`}
                          type="button"
                          className="trayButton trayButtonSquare airwaySuggestionButton"
                          onPointerDown={stopPointer}
                          onPointerUp={stopPointer}
                          onClick={async () => {
                            const adapter = props.appCoreAdapter;
                            if (!adapter) {
                              return;
                            }
                            setAirwayPicker((current) => current ? { ...current, loading: true, error: null } : current);
                            try {
                              const presentation = await prepareAirwayPresentationForAnchors(
                                adapter,
                                suggestion.airway_name,
                                airwayPicker.originAnchor,
                                airwayPicker.destinationAnchor,
                              );
                              setAirwayPicker((current) => current ? {
                                ...current,
                                loading: false,
                                selectedAirwayName: suggestion.airway_name,
                                presentation,
                              } : current);
                            } catch (error) {
                              setAirwayPicker((current) => current ? {
                                ...current,
                                loading: false,
                                error: error instanceof Error ? error.message : String(error),
                              } : current);
                            }
                          }}
                        >
                          {suggestion.airway_name}
                        </button>
                      ))}
                  </div>
                ) : airwayPicker.selectedEntryIndex === null && airwayPicker.presentation ? (
                  <>
                    {airwayPicker.presentation.points.map((point, index) => (
                      <button
                        key={`${airwayPicker.presentation?.branch_key}:${point.branch_point_index}`}
                        type="button"
                        className={`trayButton airwayChoiceButton${index === airwayPicker.presentation?.suggested_entry_index ? " isSuggested" : ""}`}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={() => {
                          setAirwayPicker((current) => current ? {
                            ...current,
                            selectedEntryIndex: index,
                          } : current);
                        }}
                      >
                        {index === airwayPicker.presentation?.suggested_entry_index ? "▸ " : ""}
                        {navRefLabel(point.nav_ref)}
                      </button>
                    ))}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setAirwayPicker((current) => current ? {
                        ...current,
                        selectedAirwayName: null,
                        presentation: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                ) : (
                  <>
                    {airwayPicker.presentation ? airwayExitCandidatesFromPresentation(
                      airwayPicker.presentation,
                      airwayPicker.selectedEntryIndex ?? 0,
                    ).map((exit, index) => (
                      <button
                        key={`${exit.airway_name}:${exit.branch_key}:${exit.branch_point_index}`}
                        type="button"
                        className={`trayButton airwayChoiceButton${index === airwayPicker.presentation?.suggested_exit_index ? " isSuggested" : ""}`}
                        disabled={exit.is_entry}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          if (exit.is_entry) {
                            return;
                          }
                          const presentation = airwayPicker.presentation;
                          const selectedEntryIndex = airwayPicker.selectedEntryIndex;
                          if (!presentation || selectedEntryIndex === null) {
                            return;
                          }
                          const selectedEntry = airwayEntryCandidateFromPresentation(
                            presentation,
                            selectedEntryIndex,
                          );
                          setAirwayPicker((current) => current ? { ...current, loading: true, error: null } : current);
                          try {
                            if (airwayPicker.mode === "replace" && airwayPicker.componentIndex !== null) {
                              await props.onReplaceAirway(
                                airwayPicker.componentIndex,
                                selectedEntryIndex,
                                index,
                                presentation,
                                airwayPicker.originAnchor,
                                airwayPicker.destinationAnchor,
                              );
                            } else if (airwayPicker.startComponentIndex !== null) {
                              await props.onInsertAirway(
                                airwayPicker.startComponentIndex,
                                airwayPicker.endComponentIndex,
                                selectedEntryIndex,
                                index,
                                presentation,
                                airwayPicker.originAnchor,
                                airwayPicker.destinationAnchor,
                              );
                            } else {
                              throw new Error("airway picker missing insertion span");
                            }
                            setAirwayPicker(null);
                            setSelectedWaypointIndex(null);
                          } catch (error) {
                            setAirwayPicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {index === airwayPicker.presentation?.suggested_exit_index ? "▸ " : ""}
                        {navRefLabel(exit.nav_ref)}
                      </button>
                    )) : null}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setAirwayPicker((current) => current ? {
                        ...current,
                        selectedEntryIndex: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                )}
              </div>
            ) : reorderOpen ? (
              <div className="waypointReorderTray">
                <button
                  type="button"
                  className="trayButton trayButtonSquare"
                  disabled={selectedRow?.componentIndex == null || selectedRow.componentIndex <= 0}
                  onPointerDown={stopPointer}
                  onPointerUp={stopPointer}
                  onClick={async () => {
                    if (selectedRow?.componentIndex == null) {
                      return;
                    }
                    await props.onMoveComponent(selectedRow.componentIndex, -1);
                    setPendingSelectedComponentIndex(selectedRow.componentIndex - 1);
                  }}
                >
                  Up
                </button>
                <button
                  type="button"
                  className="trayButton trayButtonSquare"
                  disabled={
                    selectedRow?.componentIndex == null ||
                    selectedRow.componentIndex >= componentViews.length - 1
                  }
                  onPointerDown={stopPointer}
                  onPointerUp={stopPointer}
                  onClick={async () => {
                    if (selectedRow?.componentIndex == null) {
                      return;
                    }
                    await props.onMoveComponent(selectedRow.componentIndex, 1);
                    setPendingSelectedComponentIndex(selectedRow.componentIndex + 1);
                  }}
                >
                  Down
                </button>
              </div>
            ) : rowActions.map((action) => {
              return (
              <button
                key={action.id}
                type="button"
                className="trayButton"
                disabled={!action.enabled}
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
                onClick={action.onSelect}
              >
                {action.label}
              </button>
            );
            })}
          </section>
        </>
      ) : null}
    </section>
  );
}

function TrayDock(props: {
  launcherLabel: string;
  open: boolean;
  onToggle: () => void;
  ariaLabel: string;
  blocked?: boolean;
  style?: TrayDockStyle;
  launcherAccentColor?: string;
  options: TrayOption[];
}) {
  const { launcherLabel, open, onToggle, ariaLabel, blocked = false, style = "compact", launcherAccentColor, options } = props;
  const launcherWide = style === "plate_wide";
  const trayWide = style === "plate_narrow" || style === "plate_wide";
  const launcherBlocked = blocked && !open;
  return (
    <div className="chartDockColumn">
      <button
        type="button"
        className={`chartButton${launcherWide ? " chartButtonWide" : ""}${open ? " isOpen" : ""}${launcherBlocked ? " isBlocked" : ""}`}
        aria-disabled={launcherBlocked}
        tabIndex={launcherBlocked ? -1 : undefined}
        style={{
          ...(launcherBlocked ? { pointerEvents: "none" } : undefined),
          ...(launcherAccentColor ? ({ ["--tray-accent" as string]: launcherAccentColor } as CSSProperties) : undefined),
        }}
        onPointerDown={launcherBlocked ? undefined : stopPointer}
        onPointerUp={launcherBlocked ? undefined : stopPointer}
        onDoubleClick={launcherBlocked ? undefined : stopDoubleClick}
        onClick={launcherBlocked ? undefined : onToggle}
      >
        <span className={`chartButtonLabel${launcherWide ? " chartButtonLabelWide" : ""}`}>{launcherLabel}</span>
      </button>
      <section className={`chartTray${trayWide ? " chartTrayWide" : ""}${open ? " isOpen" : ""}`} aria-label={ariaLabel} onPointerDown={stopPointer} onPointerUp={stopPointer}>
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`trayButton${option.active ? " isActive" : ""}`}
            disabled={option.disabled}
            style={option.accentColor ? ({ ["--tray-accent" as string]: option.accentColor } as CSSProperties) : undefined}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={option.onSelect}
          >
            {option.label}
          </button>
        ))}
      </section>
    </div>
  );
}

function ChartsPage(props: {
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  airports: ChartPageData["airports"];
  selectedAirport: ChartPageData["airports"][number] | null;
  selectedChart: ChartAsset | null;
  folderOpen: boolean;
  viewport: ImageViewportState | null;
  onViewportChange: (next: ImageViewportState | null) => void;
  onFolderOpenChange: (next: boolean) => void;
  onSelectPage: (page: AppPage) => void;
  onSelectAirport: (airportId: string) => void;
  onSelectChart: (chartId: string) => void;
  situation: Situation;
}) {
  const { page, pageHistory, uptimeLabel, airports, selectedAirport, selectedChart, folderOpen, viewport, onViewportChange, onFolderOpenChange, onSelectPage, onSelectAirport, onSelectChart, situation } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [imageSize, setImageSize] = useState<{ chartId: string; width: number; height: number } | null>(null);
  const viewportRef = useRef<ImageViewportState | null>(null);
  const lastLocalViewportRef = useRef<ImageViewportState | null>(null);
  const wheelGestureUntilRef = useRef(0);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<{ zoom: number; distance: number; midpoint: ScreenPoint } | null>(null);
  const lastChartLayoutKeyRef = useRef("");
  const trayGroup = useModalTrayGroup(["page", "airport", "chart"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const sortedCharts = useMemo(() => sortChartsForFolder(selectedAirport?.charts ?? []), [selectedAirport]);
  const selectedImageSize = imageSize && imageSize.chartId === (selectedChart?.id ?? "") ? imageSize : null;
  const fallbackViewport = useMemo(() => {
    if (!selectedImageSize || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    return createInitialImageViewport(selectedImageSize.width, selectedImageSize.height, surfaceSize.width, surfaceSize.height);
  }, [selectedImageSize, surfaceSize.height, surfaceSize.width]);
  const effectiveViewport = viewport ?? fallbackViewport;
  const displaySize = useMemo(() => {
    if (!selectedImageSize || !effectiveViewport || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    return imageDisplaySize(
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      effectiveViewport.zoom,
    );
  }, [selectedImageSize, surfaceSize.height, surfaceSize.width, effectiveViewport]);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      setSurfaceSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setImageSize(null);
    viewportRef.current = null;
    lastLocalViewportRef.current = null;
    lastChartLayoutKeyRef.current = "";
  }, [selectedChart?.id]);

  useEffect(() => {
    const img = imageRef.current;
    if (!selectedChart || !img) {
      return;
    }
    if (!img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
      return;
    }
    setImageSize((current) => {
      if (
        current &&
        current.chartId === selectedChart.id &&
        current.width === img.naturalWidth &&
        current.height === img.naturalHeight
      ) {
        return current;
      }
      return {
        chartId: selectedChart.id,
        width: img.naturalWidth,
        height: img.naturalHeight,
      };
    });
  }, [selectedChart?.id, selectedChart?.asset_url]);

  useEffect(() => {
    if (viewport === null) {
      viewportRef.current = null;
      lastLocalViewportRef.current = null;
      return;
    }
    if (activePointersRef.current.size > 0) {
      return;
    }
    if (Date.now() < wheelGestureUntilRef.current) {
      return;
    }
    if (lastLocalViewportRef.current === viewport) {
      return;
    }
    viewportRef.current = viewport;
  }, [viewport]);

  useEffect(() => {
    if (!selectedImageSize || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const layoutKey = `${selectedChart?.id ?? ""}:${selectedImageSize.width}:${selectedImageSize.height}:${surfaceSize.width}:${surfaceSize.height}`;
    if (viewport === null) {
      const next = createInitialImageViewport(selectedImageSize.width, selectedImageSize.height, surfaceSize.width, surfaceSize.height);
      viewportRef.current = next;
      lastLocalViewportRef.current = next;
      lastChartLayoutKeyRef.current = layoutKey;
      onViewportChange(next);
      return;
    }
    if (lastChartLayoutKeyRef.current === layoutKey) {
      return;
    }
    const normalized = clampImageViewport(
      viewport,
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      overscrollPx,
    );
    viewportRef.current = normalized;
    lastLocalViewportRef.current = normalized;
    lastChartLayoutKeyRef.current = layoutKey;
    if (normalized.left !== viewport.left || normalized.top !== viewport.top || normalized.zoom !== viewport.zoom) {
      onViewportChange(normalized);
    }
  }, [selectedImageSize, selectedChart?.id, surfaceSize.width, surfaceSize.height, viewport, onViewportChange]);

  const trayOpen = trayGroup.scrimOpen;
  const overscrollPx = 64;

  function localPointFromPointerEvent(
    event:
      | React.PointerEvent<HTMLDivElement>
      | React.MouseEvent<HTMLDivElement>
      | React.WheelEvent<HTMLDivElement>,
  ) {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    };
  }

  function updateViewport(next: ImageViewportState) {
    viewportRef.current = next;
    lastLocalViewportRef.current = next;
    onViewportChange(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    activePointersRef.current.set(event.pointerId, point);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2) {
      const [first, second] = Array.from(activePointersRef.current.values());
      pinchRef.current = {
        zoom: viewportRef.current.zoom,
        distance: distanceBetween(first, second),
        midpoint: midpoint(first, second),
      };
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    const pointers = Array.from(activePointersRef.current.values());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      const next = dragImageViewport(
        viewportRef.current,
        dx,
        dy,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      updateViewport(next);
      dragRef.current = { id: event.pointerId, last: point };
      return;
    }
    if (pointers.length >= 2 && pinchRef.current) {
      const [first, second] = pointers;
      const nextDistance = distanceBetween(first, second);
      const nextMidpoint = midpoint(first, second);
      const zoomDelta = pinchRef.current.distance > 0 ? Math.log2(nextDistance / pinchRef.current.distance) : 0;
      let next = zoomImageAroundPoint(
        viewportRef.current,
        pinchRef.current.midpoint.x,
        pinchRef.current.midpoint.y,
        clampImageZoom(pinchRef.current.zoom + zoomDelta),
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      next = dragImageViewport(
        next,
        nextMidpoint.x - pinchRef.current.midpoint.x,
        nextMidpoint.y - pinchRef.current.midpoint.y,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      updateViewport(next);
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (folderOpen) {
      return;
    }
    if (!viewportRef.current || !selectedImageSize || trayOpen) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    wheelGestureUntilRef.current = Date.now() + 160;
    const point = localPointFromPointerEvent(event);
    const zoomTarget = viewportRef.current.zoom - event.deltaY / 360;
    const next = zoomImageAroundPoint(
      viewportRef.current,
      point.x,
      point.y,
      zoomTarget,
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      overscrollPx,
    );
    updateViewport(next);
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    updateViewport(
      zoomImageAroundPoint(
        viewportRef.current,
        point.x,
        point.y,
        viewportRef.current.zoom + 0.75,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      ),
    );
  }

  return (
    <section className="pageSurface">
      <div
        ref={containerRef}
        className="mapSurface chartSurface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        <div className="mapBackdrop" />
        <SituationStatusBadge situation={situation} />
        {trayOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}

        {folderOpen ? (
          <div className="plateFolderGrid" onPointerDown={stopPointer} onPointerUp={stopPointer} onDoubleClick={stopDoubleClick}>
            {sortedCharts.map((chart) => (
              <button
                key={chart.id}
                type="button"
                className={`plateThumb${chart.id === selectedChart?.id ? " isActive" : ""}`}
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
                onDoubleClick={stopDoubleClick}
                onClick={() => {
                  onSelectChart(chart.id);
                }}
              >
                <div className="plateThumbMedia" style={{ backgroundColor: plateFolderTheme.thumbnail_bg }}>
                  {chart.thumbnail_url ? <img className="plateThumbImage" src={chart.thumbnail_url} alt="" draggable={false} /> : null}
                  <div className="plateThumbLabel" style={{ backgroundColor: plateFolderColor(chart.folder_category) }}>
                    {chart.label}
                  </div>
                </div>
              </button>
            ))}
          </div>
        ) : selectedChart ? (
          <img
            key={selectedChart.id}
            ref={imageRef}
            className="chartImage"
            src={selectedChart.asset_url}
            alt={selectedChart.label}
            draggable={false}
            onLoad={(event) =>
              setImageSize({
                chartId: selectedChart.id,
                width: event.currentTarget.naturalWidth,
                height: event.currentTarget.naturalHeight,
              })
            }
            style={{
              left: `${selectedImageSize && effectiveViewport ? effectiveViewport.left : 0}px`,
              top: `${selectedImageSize && effectiveViewport ? effectiveViewport.top : 0}px`,
              width: displaySize ? `${displaySize.width}px` : undefined,
              height: displaySize ? `${displaySize.height}px` : undefined,
              visibility: selectedImageSize && effectiveViewport ? "visible" : "hidden",
            }}
          />
        ) : null}

        <div className="chartDock chartDockDouble">
          <TrayDock
            launcherLabel={pageOptions.find((option) => option.id === page)?.launcherLabel ?? "PLT"}
            open={trayGroup.isOpen("page")}
            blocked={trayGroup.blocked("page")}
            onToggle={() => trayGroup.toggle("page")}
            ariaLabel="Page"
            options={pageOptions.map((option) => ({
              id: option.id,
              label: option.label,
              active: option.id === page,
              onSelect: () => {
                onSelectPage(option.id);
                trayGroup.close("page");
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedAirport?.label ?? "---"}
            open={trayGroup.isOpen("airport")}
            blocked={trayGroup.blocked("airport")}
            onToggle={() => trayGroup.toggle("airport")}
            ariaLabel="Airport"
            style="plate_narrow"
            options={airports.map((airport) => ({
              id: airport.id,
              label: airport.label,
              active: airport.id === selectedAirport?.id,
              onSelect: () => {
                onSelectAirport(airport.id);
                trayGroup.close("airport");
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedChart?.label ?? "---"}
            open={trayGroup.isOpen("chart")}
            blocked={trayGroup.blocked("chart")}
            launcherAccentColor={selectedChart ? plateFolderColor(selectedChart.folder_category) : undefined}
            onToggle={() => trayGroup.toggle("chart")}
            ariaLabel="Chart"
            style="plate_wide"
            options={sortedCharts.map((chart) => ({
              id: chart.id,
              label: chart.label,
              active: chart.id === selectedChart?.id,
              accentColor: plateFolderColor(chart.folder_category),
              onSelect: () => {
                onSelectChart(chart.id);
                trayGroup.close("chart");
              },
            }))}
          />
          <button
            type="button"
            className={`chartButton${folderOpen ? " isOpen" : ""}${trayOpen ? " isBlocked" : ""}`}
            aria-disabled={trayOpen || folderOpen}
            tabIndex={trayOpen ? -1 : undefined}
            style={trayOpen ? { pointerEvents: "none" } : undefined}
            onPointerDown={trayOpen || folderOpen ? undefined : stopPointer}
            onPointerUp={trayOpen || folderOpen ? undefined : stopPointer}
            onDoubleClick={trayOpen || folderOpen ? undefined : stopDoubleClick}
            onClick={trayOpen || folderOpen ? undefined : () => onFolderOpenChange(true)}
            aria-pressed={folderOpen}
            aria-label="Open plate folder view"
          >
            <span className="chartButtonLabel">FLDR</span>
          </button>
        </div>

        <div className="debugDock">
          <DebugDock open={debugOpen} onToggle={() => setDebugOpen((open) => !open)}>
            <div className="debugLine">page {pageLabel(page)}</div>
            <div className="debugLine">up {uptimeLabel}</div>
            <div className="debugLine">stack {formatPageStack(pageHistory, { page, selectedMapId: "", selectedChartId: selectedChart?.id ?? "", selectedChartLabel: selectedChart?.label ?? "", chartFolderOpen: folderOpen })}</div>
            <div className="debugLine">apt {selectedAirport?.label ?? "---"}</div>
            <div className="debugLine">chart {selectedChart?.label ?? "---"}</div>
            <div className="debugLine">{viewport ? `z${viewport.zoom.toFixed(2)}` : "viewport (none)"}</div>
          </DebugDock>
        </div>

      </div>
    </section>
  );
}

function DebugDock(props: { open: boolean; warn?: boolean; onToggle: () => void; children: React.ReactNode }) {
  return (
    <>
      <button
        type="button"
        className={`debugLauncher${props.warn ? " isWarn" : ""}`}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={props.onToggle}
        aria-expanded={props.open}
        aria-label="Toggle debug details"
      >
        DBG
      </button>
      <section
        className={`debugPanel${props.open ? " isOpen" : ""}`}
        aria-label="Debug metadata"
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
      >
        {props.children}
      </section>
    </>
  );
}

function readPersistedWebUiState(): PersistedWebUiState {
  if (typeof window === "undefined") {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(webUiStateStorageKey);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as PersistedWebUiState;
    return {
      page: parsed.page,
      selectedAirportId: parsed.selectedAirportId,
      selectedChartId: parsed.selectedChartId,
      recentAirportIds: Array.isArray(parsed.recentAirportIds) ? parsed.recentAirportIds.filter((value): value is string => typeof value === "string") : [],
    };
  } catch {
    return {};
  }
}

function writePersistedWebUiState(state: PersistedWebUiState) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(webUiStateStorageKey, JSON.stringify(state));
}

function mergeRecentAirportIds(
  airports: ChartPageData["airports"],
  storedIds: string[],
) {
  const validIds = new Set(airports.map((airport) => airport.id));
  const orderedIds = storedIds.filter((id, index) => validIds.has(id) && storedIds.indexOf(id) === index);
  for (const airport of airports) {
    if (!orderedIds.includes(airport.id)) {
      orderedIds.push(airport.id);
    }
  }
  return orderedIds;
}

function useSessionUptimeLabel(sessionStartMs: number) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const interval = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, []);
  return formatUptimeMs(nowMs - sessionStartMs);
}

function formatUptimeMs(elapsedMs: number) {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function orderAirportsByRecency(
  airports: ChartPageData["airports"],
  recentAirportIds: string[],
) {
  const airportById = new Map(airports.map((airport) => [airport.id, airport]));
  return recentAirportIds
    .map((airportId) => airportById.get(airportId))
    .filter((airport): airport is ChartPageData["airports"][number] => airport !== undefined);
}

function moveAirportToFront(
  currentIds: string[],
  airportId: string,
  airports: ChartPageData["airports"],
) {
  return mergeRecentAirportIds(airports, [airportId, ...currentIds.filter((id) => id !== airportId)]);
}

function resolveAirportId(
  airports: ChartPageData["airports"],
  candidateAirportId: string | undefined,
  recentAirportIds: string[],
) {
  if (candidateAirportId && airports.some((airport) => airport.id === candidateAirportId)) {
    return candidateAirportId;
  }
  return recentAirportIds[0] ?? airports[0]?.id ?? "";
}

function plateFolderColor(category: PlateFolderCategory) {
  return plateFolderTheme.label_colors[category as keyof typeof plateFolderTheme.label_colors] ?? plateFolderTheme.label_colors.other ?? "#52656d";
}

function sortChartsForFolder(charts: ChartAsset[]) {
  return [...charts].sort((left, right) => {
    const rank = plateFolderCategoryOrder.indexOf(left.folder_category) - plateFolderCategoryOrder.indexOf(right.folder_category);
    return rank !== 0 ? rank : left.label.localeCompare(right.label);
  });
}

function flightPlanActionLabel(actionId: string): string {
  switch (actionId) {
    case "activate_leg":
      return "Activate Leg";
    case "remove":
      return "Remove";
    case "insert":
      return "Insert";
    case "reorder":
      return "Reorder";
    case "waypoint_info":
      return "Waypoint Info";
    case "add_airway":
      return "Add Airway";
    case "select_procedure":
      return "Select Procedure";
    case "charts":
    case "plates":
      return "Plates";
    case "change_airway":
      return "Change Airway";
    case "remove_airway":
      return "Remove Airway";
    case "remove_procedure":
      return "Remove Procedure";
    default:
      return actionId;
  }
}

function resolveChartId(
  airports: ChartPageData["airports"],
  airportId: string,
  candidateChartId: string | undefined,
) {
  const airport = airports.find((entry) => entry.id === airportId);
  if (candidateChartId && airport?.charts.some((chart) => chart.id === candidateChartId)) {
    return candidateChartId;
  }
  return airport?.charts[0]?.id ?? "";
}


function sameIds(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function pageLabel(page: AppPage) {
  return pageOptions.find((option) => option.id === page)?.launcherLabel ?? page.toUpperCase();
}

function formatSnapshot(snapshot: Pick<AppViewSnapshot, "page" | "selectedMapId" | "selectedChartId" | "selectedChartLabel" | "chartFolderOpen">) {
  const label = pageLabel(snapshot.page);
  if (snapshot.page === "map") {
    const map = mapViews.find((entry) => entry.id === snapshot.selectedMapId);
    const family = chartFamilies.find((entry) => entry.id === map?.map_view.chart_family)?.launcherLabel ?? "";
    return family ? `${label}-${family}` : label;
  }
  if (snapshot.page !== "charts") {
    return label;
  }
  if (snapshot.chartFolderOpen) {
    return `${label}-FLDR`;
  }
  const suffixSource = snapshot.selectedChartLabel || resolveChartLabel(snapshot.selectedChartId) || snapshot.selectedChartId;
  const suffix = suffixSource.slice(-3).toUpperCase();
  return suffix ? `${label}-${suffix}` : label;
}

function formatPageStack(pageHistory: AppViewSnapshot[], currentSnapshot: Pick<AppViewSnapshot, "page" | "selectedMapId" | "selectedChartId" | "selectedChartLabel" | "chartFolderOpen">) {
  return [currentSnapshot, ...pageHistory.slice().reverse()].map(formatSnapshot).join(" > ");
}

function SituationStatusBadge(props: { situation: Situation }) {
  const tone =
    props.situation.position.kind === "unknown"
      ? "unknown"
      : props.situation.position.kind === "flight_plan_location"
        ? "simulated"
        : "live";
  const label =
    tone === "unknown"
      ? "Location Unknown"
      : tone === "simulated"
        ? "Simulated Position"
        : "Live Position";
  return <div className={`situationStatus situationStatus-${tone}`}>{label}</div>;
}

function SituationAircraft(props: {
  iconSrc: string;
  point: { x: number; y: number };
  headingDeg: number;
}) {
  return (
    <img
      className="situationAircraft"
      src={props.iconSrc}
      alt=""
      draggable={false}
      style={{
        left: `${props.point.x}px`,
        top: `${props.point.y}px`,
        transform: `translate(-50%, -50%) rotate(${props.headingDeg}deg)`,
      }}
    />
  );
}

function resolveSituationOverlay(
  situation: Situation,
  viewport: MapViewportState,
  width: number,
  height: number,
) {
  if (width <= 0 || height <= 0 || situation.position.kind === "unknown") {
    return null;
  }
  const point = latLonToScreen(situation.position.lat, situation.position.lon, viewport, width, height);
  const headingDeg = situation.orientation_deg ?? 0;
  const ring = selectSituationRing(situation.position.lat, situation.position.lon, viewport, width, height);
  const ahead =
    situation.speed_kt !== null
      ? projectAhead(situation.position.lat, situation.position.lon, headingDeg, situation.speed_kt / 60)
      : null;
  const predictor = ahead ? latLonToScreen(ahead.lat, ahead.lon, viewport, width, height) : null;
  return { point, predictor, headingDeg, ring };
}

function latLonToScreen(lat: number, lon: number, viewport: MapViewportState, width: number, height: number) {
  const world = latLonToWorld(lat, lon);
  const scale = scaleForZoom(viewport.zoom);
  return {
    x: ((world.x - viewport.centerWorldX) * scale) + width / 2,
    y: ((world.y - viewport.centerWorldY) * scale) + height / 2,
  };
}

function projectAhead(lat: number, lon: number, bearingDeg: number, distanceNm: number) {
  const angularDistance = distanceNm / 3440.065;
  const bearing = (bearingDeg * Math.PI) / 180;
  const startLat = (lat * Math.PI) / 180;
  const startLon = (lon * Math.PI) / 180;
  const nextLat = Math.asin(
    Math.sin(startLat) * Math.cos(angularDistance) +
      Math.cos(startLat) * Math.sin(angularDistance) * Math.cos(bearing),
  );
  const nextLon =
    startLon +
    Math.atan2(
      Math.sin(bearing) * Math.sin(angularDistance) * Math.cos(startLat),
      Math.cos(angularDistance) - Math.sin(startLat) * Math.sin(nextLat),
    );
  return {
    lat: (nextLat * 180) / Math.PI,
    lon: (nextLon * 180) / Math.PI,
  };
}

function arrowHeadPoints(from: { x: number; y: number }, to: { x: number; y: number }) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  const size = 20;
  const left = {
    x: to.x - size * Math.cos(angle - Math.PI / 6),
    y: to.y - size * Math.sin(angle - Math.PI / 6),
  };
  const right = {
    x: to.x - size * Math.cos(angle + Math.PI / 6),
    y: to.y - size * Math.sin(angle + Math.PI / 6),
  };
  return `${to.x},${to.y} ${left.x},${left.y} ${right.x},${right.y}`;
}

function arrowShaftEndPoint(from: { x: number; y: number }, to: { x: number; y: number }) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  const headLength = 14;
  return {
    x: to.x - headLength * Math.cos(angle),
    y: to.y - headLength * Math.sin(angle),
  };
}

function selectSituationRing(
  lat: number,
  lon: number,
  viewport: MapViewportState,
  width: number,
  height: number,
) {
  const center = latLonToScreen(lat, lon, viewport, width, height);
  const smaller = Math.min(width, height);
  const minDiameter = smaller * 0.5;
  const maxDiameter = smaller * 0.8;
  const targetDiameter = smaller * 0.65;
  const candidates = situationRingSizesNm.map((radiusNm) => {
    const edge = projectAhead(lat, lon, 90, radiusNm);
    const edgePoint = latLonToScreen(edge.lat, edge.lon, viewport, width, height);
    const radiusPx = Math.hypot(edgePoint.x - center.x, edgePoint.y - center.y);
    const diameterPx = radiusPx * 2;
    const outOfBounds =
      diameterPx < minDiameter ? minDiameter - diameterPx : diameterPx > maxDiameter ? diameterPx - maxDiameter : 0;
    const score = outOfBounds > 0 ? 10000 + outOfBounds : Math.abs(diameterPx - targetDiameter);
    return { radiusNm, radiusPx, score };
  });
  const best = candidates.reduce((currentBest, candidate) => (candidate.score < currentBest.score ? candidate : currentBest));
  const labelAngle = -45;
  const labelPoint = pointOnCircle(center, best.radiusPx + 16, labelAngle);
  return {
    radiusNm: best.radiusNm,
    radiusPx: best.radiusPx,
    tickMarks: buildRingTickMarks(center, best.radiusPx),
    label: {
      point: labelPoint,
      rotationDeg: 45,
      text: formatRingDistance(best.radiusNm),
    },
  };
}

function buildRingTickMarks(center: { x: number; y: number }, radiusPx: number) {
  return Array.from({ length: 12 }, (_, index) => {
    const angleDeg = index * 30;
    return {
      inner: pointOnCircle(center, radiusPx - 14, angleDeg),
      outer: pointOnCircle(center, radiusPx, angleDeg),
    };
  });
}

function pointOnCircle(center: { x: number; y: number }, radiusPx: number, angleDeg: number) {
  const radians = (angleDeg * Math.PI) / 180;
  return {
    x: center.x + radiusPx * Math.cos(radians),
    y: center.y + radiusPx * Math.sin(radians),
  };
}

function formatRingDistance(radiusNm: number) {
  return `${Number.isInteger(radiusNm) ? radiusNm.toFixed(0) : radiusNm.toString()}nm`;
}

async function applyFlightPlanMutation(
  uiSession: UiSession | null,
  setSessionSnapshot: Dispatch<SetStateAction<UiSessionSnapshot>>,
  setPlanUiState: Dispatch<SetStateAction<FlightPlanUiState | null>>,
  mutation: FlightPlanUiMutation,
) {
  if (!uiSession) {
    throw new Error("flight plan mutation requires live core session");
  }
  const nextSnapshot = await uiSession.replaceFlightPlan(mutation.plan);
  setSessionSnapshot(nextSnapshot);
  setPlanUiState(mutation.ui_state);
}

async function buildSeededDevPlan(adapter: AppCoreAdapter): Promise<{ plan: typeof samplePlan; uiState: FlightPlanUiState }> {
  const originAnchor: NavRef = { Airport: "KRNT" };
  const destinationAnchor: NavRef = { Airport: "KUAO" };
  const presentation = await prepareAirwayPresentationForAnchors(
    adapter,
    "V23",
    originAnchor,
    destinationAnchor,
  );
  const entryIndex = presentation.points.findIndex((point) => navRefLabel(point.nav_ref) === "SEA");
  if (entryIndex < 0) {
    throw new Error("failed to seed V23 airway: SEA not found in presentation");
  }
  const entry = airwayEntryCandidateFromPresentation(presentation, entryIndex);
  const exit = airwayExitCandidatesFromPresentation(presentation, entryIndex).find(
    (candidate) => navRefLabel(candidate.nav_ref) === "RAWER",
  );
  if (!exit) {
    throw new Error("failed to seed V23 airway: RAWER not found in exit candidates");
  }
  const materialized = await materializeAirwaySelection(
    0,
    entry,
    exit,
    originAnchor,
    destinationAnchor,
  );
  const mutation = await adapter.insertAirwayMaterializedUi(
    samplePlan,
    0,
    1,
    materialized.selection,
    materialized.airway,
    materialized.resolvedLegs,
  );
  return {
    plan: mutation.plan,
    uiState: mutation.ui_state,
  };
}

function buildLegacyComponentViews(plan: typeof samplePlan): FlightPlanUiState["components"] {
  if (plan.legs.length === 0) {
    return [];
  }

  const waypoints: NavRef[] = [plan.legs[0].from, ...plan.legs.map((leg) => leg.to)];
  return waypoints.map((waypoint, index) => ({
    component_index: index,
    kind: "waypoint",
    summary: navRefLabel(waypoint),
    items: [{ kind: "waypoint", nav_ref: waypoint }],
    active: false,
    can_add_airway_after: true,
    can_add_procedure_before: index > 0 && "Airport" in waypoint,
    can_change_airway: false,
    can_remove: true,
    can_reorder: waypoints.length > 1,
    can_reorder_up: index > 0,
    can_reorder_down: index < waypoints.length - 1,
    preceding_waypoint: index > 0 ? waypoints[index - 1] : null,
    following_waypoint: index + 1 < waypoints.length ? waypoints[index + 1] : null,
  }));
}

function concretizedNavItemLabel(item: FlightPlanUiState["components"][number]["items"][number]) {
  if (item.kind === "waypoint") {
    return navRefLabel(item.nav_ref);
  }
  return item.label;
}

function structuredComponentLabel(component: FlightPlanUiState["components"][number]) {
  if (component.kind === "airway") {
    return component.summary.split("(")[0].trim();
  }
  return component.summary;
}

function componentWaypointNavRef(component: FlightPlanUiState["components"][number] | undefined): NavRef | null {
  if (!component) {
    return null;
  }
  const item = component.items[0];
  return item && item.kind === "waypoint" ? item.nav_ref : null;
}

function navRefsEqual(left: NavRef | null, right: NavRef | null) {
  if (!left || !right) {
    return false;
  }
  if ("Airport" in left && "Airport" in right) return left.Airport === right.Airport;
  if ("Navaid" in left && "Navaid" in right) return left.Navaid === right.Navaid;
  if ("Fix" in left && "Fix" in right) return left.Fix === right.Fix;
  if ("LatLon" in left && "LatLon" in right) {
    return left.LatLon.lat === right.LatLon.lat && left.LatLon.lon === right.LatLon.lon;
  }
  return false;
}

function navRefKey(value: NavRef) {
  if ("Airport" in value) return `airport:${value.Airport}`;
  if ("Navaid" in value) return `navaid:${value.Navaid}`;
  if ("Fix" in value) return `fix:${value.Fix}`;
  return `latlon:${value.LatLon.lat}:${value.LatLon.lon}`;
}

function navRefLabel(value: NavRef) {
  if ("Airport" in value) return value.Airport;
  if ("Navaid" in value) return value.Navaid;
  if ("Fix" in value) return value.Fix;
  return `${value.LatLon.lat.toFixed(3)}, ${value.LatLon.lon.toFixed(3)}`;
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

function routeSegmentColor(status: FlightPlanRouteSegment["status"]) {
  if (status === "completed") {
    return "#8c9dad";
  }
  if (status === "active") {
    return "#ff4fcf";
  }
  return "#ffffff";
}

function resolveChartLabel(chartId: string) {
  for (const airport of chartPage.airports) {
    const chart = airport.charts.find((entry) => entry.id === chartId);
    if (chart) {
      return chart.label;
    }
  }
  return "";
}

function distanceBetween(first: ScreenPoint, second: ScreenPoint) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function stopPointer(event: React.PointerEvent<HTMLElement>) {
  event.stopPropagation();
}

function stopDoubleClick(event: React.MouseEvent<HTMLElement>) {
  event.preventDefault();
  event.stopPropagation();
}

function TrayScrim(props: { ariaLabel: string; onClose: () => void }) {
  return (
    <button
      type="button"
      className="trayScrim"
      aria-label={props.ariaLabel}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onClose}
    />
  );
}

function useModalTrayGroup<const T extends string>(ids: readonly T[]) {
  const [openId, setOpenId] = useState<T | null>(null);
  const allowedIds = useMemo(() => new Set<T>(ids), [ids]);

  function isOpen(id: T) {
    return openId === id;
  }

  function blocked(id: T) {
    return openId !== null && openId !== id;
  }

  function toggle(id: T) {
    if (!allowedIds.has(id)) {
      return;
    }
    setOpenId((current) => (current === id ? null : current === null ? id : current));
  }

  function close(id: T) {
    setOpenId((current) => (current === id ? null : current));
  }

  function closeAll() {
    setOpenId(null);
  }

  return {
    close,
    closeAll,
    blocked,
    isOpen,
    openId,
    scrimOpen: openId !== null,
    toggle,
  };
}

function midpoint(first: ScreenPoint, second: ScreenPoint): ScreenPoint {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}
