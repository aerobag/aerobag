import { Fragment, useEffect, useMemo, useRef, useState, type CSSProperties, type Dispatch, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import { chartPage, mapViews, resourceIndex, sampleCatalog, samplePlan } from "./domain/sampleData";
import type {
  AirwayPresentationPlan,
  AirwaySuggestion,
  AppState,
  ChartPageData,
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
  LatLon,
  MaterializedProcedure,
  NavElementUiView,
  NavRef,
  PlaybackUiState,
  MapFollowUiState,
  ProcedureOptions,
  ProcedureLoadOption,
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
import { airwayEntryCandidateFromPresentation, airwayExitCandidatesFromPresentation } from "./domain/airwayPlanner";
import { debugLog } from "./domain/debugLog";

type SurfaceSize = {
  width: number;
  height: number;
};

type AppPage = "map" | "plan" | "charts" | "settings";

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
    disabled_button: string;
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

function mapViewsForDisplayedFamily(
  allMapViews: typeof mapViews,
  familyId: ChartFamilyId,
): typeof mapViews {
  if (familyId === "tac") {
    return allMapViews.filter((view) => {
      const chartFamily = view.map_view.chart_family;
      return chartFamily === "sec" || chartFamily === "tac";
    });
  }
  return allMapViews.filter((view) => view.map_view.chart_family === familyId);
}

function preferredFamilyMap(
  allMapViews: typeof mapViews,
  familyId: ChartFamilyId,
  fallbackRegionId: string | null,
): (typeof mapViews)[number] | undefined {
  const familyMaps = allMapViews.filter((view) => view.map_view.chart_family === familyId);
  return (
    familyMaps.find((view) => view.region_id === fallbackRegionId)
    ?? familyMaps[0]
  );
}

const pageOptions: Array<{ id: AppPage; label: string; launcherLabel: string }> = [
  { id: "map", label: "CHART", launcherLabel: "CHT" },
  { id: "charts", label: "PLATE", launcherLabel: "PLT" },
  { id: "plan", label: "PLAN", launcherLabel: "PLN" },
  { id: "settings", label: "SETTINGS", launcherLabel: "STGS" },
];

const webUiStateStorageKey = "aerobag.web.uiState.v1";
const maxViewHistoryDepth = 64;
const loadedUiTheme = uiTheme as UiThemeJson;
const controlTheme = loadedUiTheme.controls;
const plateFolderTheme = loadedUiTheme.plate_folder;
const plateFolderCategoryOrder: PlateFolderCategory[] = ["airport-diagram", "csup", "takeoff-mins", "approach", "departure", "star"];
const VAMPS_POSITION = { lat: 47.3648944444444, lon: -121.980275 };
const defaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json";
const rasterTileDebugTargets = [
  { zoom: 8, x: 42, yTms: 166 },
  { zoom: 8, x: 41, yTms: 166 },
] as const;
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

function isRasterTileDebugTarget(tile: { zoom: number; x: number; yTms: number }): boolean {
  return rasterTileDebugTargets.some((target) => target.zoom === tile.zoom && target.x === tile.x && target.yTms === tile.yTms);
}

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
  const parsed = Number.parseFloat(raw);
  if (!Number.isFinite(parsed)) {
    return 0;
  }
  if (raw.endsWith("rem")) {
    const rootFontSize = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
    return parsed * (Number.isFinite(rootFontSize) ? rootFontSize : 16) * multiplier;
  }
  return parsed * multiplier;
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

function emptyPlaybackUiState(): PlaybackUiState {
  return {
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
}

function emptyMapFollowUiState(): MapFollowUiState {
  return {
    can_center_here: false,
    following: true,
  };
}

function initialMapId() {
  return preferredFamilyMap(mapViews, "tac", "nw")?.id ?? mapViews[0].id;
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
    app_ui_state: {
      active_plan: null,
      content_policy: "PreferLocal",
      last_content_requirements: [],
      last_content_report: null,
    },
    playback_ui_state: emptyPlaybackUiState(),
    map_follow_ui_state: emptyMapFollowUiState(),
    map_follow_target_viewport: null,
    chart_page_state: {
      ordered_airport_ids: initialChartPageState.airports.map((airport) => airport.id),
      recent_airport_ids: initialChartPageState.recent_airport_ids,
      selected_airport_id: initialChartPageState.selected_airport_id,
      selected_chart_id: initialChartPageState.selected_chart_id,
    },
  });
  const [playbackSourcePath, setPlaybackSourcePath] = useState(defaultPlaybackTracePath);
  const appState: AppState = sessionSnapshot.app_state;
  const appUiState = sessionSnapshot.app_ui_state;
  const playbackUiState = sessionSnapshot.playback_ui_state;
  const mapFollowUiState = sessionSnapshot.map_follow_ui_state;
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

  useEffect(() => {
    if (playbackUiState.source_path) {
      setPlaybackSourcePath(playbackUiState.source_path);
    }
  }, [playbackUiState.source_path]);

  useEffect(() => {
    if (!uiSession || playbackUiState.status !== "playing") {
      return;
    }
    let cancelled = false;
    const tick = () => {
      void uiSession.tickPlayback(Date.now()).then((nextSnapshot) => {
        if (!cancelled) {
          setSessionSnapshot(nextSnapshot);
        }
      }).catch(() => {});
    };
    tick();
    const timer = window.setInterval(tick, 250);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [playbackUiState.status, uiSession]);
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
    () => mapViewsForDisplayedFamily(mapViews, selectedMap.map_view.chart_family),
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
    void appCoreAdapter?.prewarm().catch((error) => {
      if (!cancelled) {
        console.error("failed to prewarm web adapter", error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter]);

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
      const createdSnapshot = await created.snapshot();
      debugLog("session.create.snapshot", {
        app_state_active_plan: createdSnapshot.app_state.active_plan?.id ?? null,
        app_ui_state_nav_element: createdSnapshot.app_ui_state.active_plan?.guidance?.nav_element ?? null,
      });
      nextSession = created;
      if (!cancelled) {
        setUiSession(created);
        setPlanUiState(initialPlan.uiState);
      }
      const snapshot = await created.setSituation(demoSituation());
      debugLog("session.set_situation.snapshot", {
        app_state_active_plan: snapshot.app_state.active_plan?.id ?? null,
        app_ui_state_active_plan: snapshot.app_ui_state.active_plan?.guidance?.nav_element ?? null,
        situation: snapshot.app_state.situation,
      });
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
        "--theme-disabled-button": controlTheme.disabled_button,
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
          appCoreAdapter={appCoreAdapter}
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
          sessionPlanUiState={appUiState.active_plan}
          playbackUiState={playbackUiState}
          mapFollowUiState={mapFollowUiState}
          mapFollowTargetViewport={sessionSnapshot.map_follow_target_viewport}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
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
          sessionPlanUiState={appUiState.active_plan}
          onOpenPlan={() => navigateToPage("plan")}
          onSelectPage={navigateToPage}
          onOpenCharts={(airportId, chartId) => {
            if (!airportId) {
              return;
            }
            const airport = chartPageData.airports.find((entry) => entry.id === airportId);
            const resolvedChartId =
              (chartId && airport?.charts.find((chart) => chart.id === chartId)?.id) ??
              airport?.charts[0]?.id ??
              "";
            const resolvedChartLabel = airport?.charts.find((chart) => chart.id === resolvedChartId)?.label ?? airport?.charts[0]?.label ?? "";
            if (uiSession) {
              debugLog("charts.open.request", {
                airport_id: airportId,
                chart_id: resolvedChartId,
                chart_label: resolvedChartLabel,
              });
              void uiSession.restoreChartPageState(
                moveAirportToFront(recentAirportIds, airportId, chartPageData.airports),
                airportId,
                resolvedChartId || undefined,
              ).then((nextSnapshot) => {
                debugLog("charts.open.snapshot", {
                  requested_airport_id: airportId,
                  requested_chart_id: resolvedChartId,
                  selected_airport_id: nextSnapshot.chart_page_state.selected_airport_id,
                  selected_chart_id: nextSnapshot.chart_page_state.selected_chart_id,
                });
                setSessionSnapshot(nextSnapshot);
              }).catch(() => {});
            }
            pushViewSnapshot({
              page: "charts",
              selectedAirportId: airportId,
              selectedChartId: resolvedChartId,
              selectedChartLabel: resolvedChartLabel,
              recentAirportIds: moveAirportToFront(recentAirportIds, airportId, chartPageData.airports),
              chartViewport: null,
              chartFolderOpen: !chartId,
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
            const materialized = await appCoreAdapter.materializeAirwaySelection(
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
            const materialized = await appCoreAdapter.materializeAirwaySelection(
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
          onReplaceProcedure={async (componentIndex, built) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.replaceProcedureMaterializedUi(
              currentPlan,
              componentIndex,
              built,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
        />
      </div>

      <div className={`pageLayer${page === "charts" ? " isActive" : ""}`} aria-hidden={page !== "charts"}>
        <ChartsPage
          appCoreAdapter={appCoreAdapter}
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          plan={currentPlan}
          sessionPlanUiState={appUiState.active_plan}
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
          onOpenPlan={() => navigateToPage("plan")}
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
          onApplyMutation={async (mutation) => {
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, setPlanUiState, mutation);
          }}
          situation={appState.situation}
          playbackUiState={playbackUiState}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          uiSession={uiSession}
        />
      </div>

      <div className={`pageLayer${page === "settings" ? " isActive" : ""}`} aria-hidden={page !== "settings"}>
        <SettingsPage
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          sessionPlanUiState={appUiState.active_plan}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
        />
      </div>
    </main>
  );
}

function MapPage(props: {
  appCoreAdapter: AppCoreAdapter;
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
  sessionPlanUiState: FlightPlanUiState | null;
  playbackUiState: PlaybackUiState;
  mapFollowUiState: MapFollowUiState;
  mapFollowTargetViewport: { center: LatLon; zoom: number; rotation_deg: number; pitch_deg: number } | null;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  uiSession: UiSession | null;
  adapterBackend: AdapterBackendKind;
  adapterDetail: string;
}) {
  const {
    appCoreAdapter,
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
    sessionPlanUiState,
    uiSession,
    mapFollowUiState,
    mapFollowTargetViewport,
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
  useEffect(() => {
    const matchingTiles = tiles
      .filter((tile) => isRasterTileDebugTarget(tile))
      .map((tile) => ({
        zoom: tile.zoom,
        x: tile.x,
        y_tms: tile.yTms,
        family: tile.chartFamily,
        map_view_id: tile.mapViewId,
        package_name: tile.packageName,
        src: tile.src,
      }));
    debugLog("map.raster.debug_tiles.selected", {
      selected_map_id: selectedMap.id,
      selected_family_id: selectedFamily.id,
      matching_tiles: matchingTiles,
    });
  }, [selectedFamily.id, selectedMap.id, tiles]);
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
    debugLog("map.nav_element.render", {
      app_state_active_plan: plan?.id ?? null,
      session_nav_element: sessionPlanUiState?.guidance?.nav_element ?? null,
      local_plan_guidance: planUiState?.guidance?.nav_element ?? null,
      situation,
    });
  }, [plan, planUiState, sessionPlanUiState, situation]);

  useEffect(() => {
    let cancelled = false;

    async function resolveFlightPlanRoute() {
      if ((plan.resolved_legs ?? []).length === 0 || (planUiState?.resolved_legs ?? []).length === 0) {
        setFlightPlanRoute([]);
        return;
      }
      const segments = await appCoreAdapter.projectFlightPlanRoute(plan, planUiState);
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
  }, [appCoreAdapter, plan, planUiState]);

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

  function syncFollowStateForViewport(nextViewport: MapViewportState) {
    if (!uiSession || !mapFollowUiState.following || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const overlay = resolveSituationOverlay(situation, nextViewport, surfaceSize.width, surfaceSize.height);
    if (!overlay) {
      void uiSession.disengageMapFollow(nextViewport).then(props.onPlaybackSnapshotChange).catch(() => {});
      return;
    }
    const point = overlay.point;
    if (!point || point.x < 0 || point.x > surfaceSize.width || point.y < 0 || point.y > surfaceSize.height) {
      void uiSession.disengageMapFollow(nextViewport).then(props.onPlaybackSnapshotChange).catch(() => {});
      return;
    }
    void uiSession
      .setMapFollowOffset(nextViewport, point.x - surfaceSize.width / 2, point.y - surfaceSize.height / 2)
      .then(props.onPlaybackSnapshotChange)
      .catch(() => {});
  }

  useEffect(() => {
    if (!uiSession || !mapFollowUiState.following || mapFollowTargetViewport) {
      return;
    }
    let cancelled = false;
    void uiSession.engageMapFollow(viewport).then((nextSnapshot) => {
      if (!cancelled) {
        props.onPlaybackSnapshotChange(nextSnapshot);
      }
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [mapFollowTargetViewport, mapFollowUiState.following, props.onPlaybackSnapshotChange, uiSession, viewport]);

  useEffect(() => {
    if (!mapFollowUiState.following || !mapFollowTargetViewport) {
      return;
    }
    const nextViewport = mapViewportFromCore(mapFollowTargetViewport);
    if (!sameMapViewport(nextViewport, viewport)) {
      updateViewport(nextViewport);
    }
  }, [mapFollowTargetViewport, mapFollowUiState.following, viewport]);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen) {
      return;
    }
    if (event.pointerType === "mouse") {
      activePointersRef.current.clear();
      dragRef.current = null;
      pinchRef.current = null;
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
      const nextViewport = dragViewport(viewportRef.current, dx, dy);
      updateViewport(nextViewport);
      syncFollowStateForViewport(nextViewport);
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
      const nextViewport = applyPinchGesture(
        pinchRef.current,
        first[1],
        second[1],
        selectedMap.map_view,
        surfaceSize.width,
        surfaceSize.height,
      );
      updateViewport(nextViewport);
      syncFollowStateForViewport(nextViewport);
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

  function handleLostPointerCapture(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    dragRef.current = remaining.length === 1 ? { id: remaining[0][0], last: remaining[0][1] } : null;
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap.map_view,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom - event.deltaY / 360,
    );
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap.map_view,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom + 0.75,
    );
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
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
        onLostPointerCapture={handleLostPointerCapture}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        <div className="mapBackdrop" />
        {trayGroup.scrimOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}
        {tiles.map((tile) => (
          <div
            key={`${tile.chartFamily}-${tile.packageName ?? tile.mapViewId}-${tile.zoom}-${tile.x}-${tile.yTms}`}
            className="mapTile"
            style={{
              left: `${tile.left}px`,
              top: `${tile.top}px`,
              width: `${tile.size}px`,
              height: `${tile.size}px`,
            }}
          >
            <img
              className="mapTileImage"
              src={tile.src}
              alt=""
              draggable={false}
              onLoad={() => {
                if (!isRasterTileDebugTarget(tile)) {
                  return;
                }
                debugLog("map.raster.debug_tile.load", {
                  zoom: tile.zoom,
                  x: tile.x,
                  y_tms: tile.yTms,
                  family: tile.chartFamily,
                  map_view_id: tile.mapViewId,
                  package_name: tile.packageName,
                  src: tile.src,
                });
              }}
              onError={() => {
                if (!isRasterTileDebugTarget(tile)) {
                  return;
                }
                debugLog("map.raster.debug_tile.error", {
                  zoom: tile.zoom,
                  x: tile.x,
                  y_tms: tile.yTms,
                  family: tile.chartFamily,
                  map_view_id: tile.mapViewId,
                  package_name: tile.packageName,
                  src: tile.src,
                });
              }}
            />
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
                  const nextMap = preferredFamilyMap(
                    mapViews,
                    family.id,
                    selectedMap.region_id,
                  );
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
          <NavElementView navElement={sessionPlanUiState?.guidance?.nav_element ?? { active_leg_summary: "", cdi_indicator_dots: null }} />
        </button>

        <PlaybackWidget
          uiSession={uiSession}
          playbackUiState={props.playbackUiState}
          sourcePath={props.playbackSourcePath}
          onSourcePathChange={props.onPlaybackSourcePathChange}
          onSnapshotChange={props.onPlaybackSnapshotChange}
          surfaceWidth={surfaceSize.width}
          dock="left"
        />

        <button
          type="button"
          className={`centerHereButton${mapFollowUiState.following ? " isActive" : ""}`}
          disabled={!mapFollowUiState.can_center_here}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={() => {
            if (!uiSession) {
              return;
            }
            void uiSession.engageMapFollow(viewport).then(props.onPlaybackSnapshotChange).catch(() => {});
          }}
        >
          CTR
        </button>

        <div className="debugDock" style={{ left: "auto", right: "calc(var(--thumb) + (var(--thumb-gap) * 2))" }}>
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

function NavElementView(props: { navElement: NavElementUiView }) {
  const { navElement } = props;
  const width = 180;
  const height = 18;
  const unit = width / 4.5;
  const dotXs = [0.25, 1.25, 3.25, 4.25].map((value) => value * unit);
  const centerX = 2.25 * unit;
  const baselineY = height * 0.5;
  const dotRadius = unit * 0.04375;
  const triangleHalfWidth = unit * 0.25;
  const triangleTopY = 0;
  const triangleBottomY = height;
  const pointerPosition = navElement.cdi_indicator_dots;
  const pointerX =
    pointerPosition === null
      ? null
      : Math.max(0.25, Math.min(4.25, pointerPosition + 2.25)) * unit;
  return (
    <>
      <span className="navElementTop">{navElement.active_leg_summary}</span>
      <svg className="navElementBottom" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
        {pointerX !== null ? (
          <path
            className="navElementCdiPointer"
            d={`M ${pointerX - triangleHalfWidth} ${triangleBottomY} L ${pointerX + triangleHalfWidth} ${triangleBottomY} L ${pointerX} ${triangleTopY} Z`}
          />
        ) : null}
        <line className="navElementCdiBar" x1={centerX} y1={0} x2={centerX} y2={height} />
        {dotXs.map((x, index) => (
          <circle key={index} className="navElementCdiDot" cx={x} cy={baselineY} r={dotRadius} />
        ))}
      </svg>
    </>
  );
}

function playbackWidgetMaxWidthPx(surfaceWidth: number) {
  if (surfaceWidth <= 0) {
    return 0;
  }
  const thumb = thumbPixels(1);
  const gap = thumbPixels(0.1);
  const navWidth = thumb * 3;
  const navRightEdge = surfaceWidth / 2 + navWidth / 2;
  return Math.max(thumb * 2.8, surfaceWidth - navRightEdge - gap * 2);
}

function formatPlaybackClock(seconds: number) {
  const clampedSeconds = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(clampedSeconds / 3600);
  const minutes = Math.floor((clampedSeconds % 3600) / 60);
  const remainder = clampedSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`;
  }
  return `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function profilePathData(
  samples: Array<number | null>,
  width: number,
  height: number,
  leftInset: number,
  rightInset: number,
) {
  const usable = samples
    .map((value, index) => ({ value, index }))
    .filter((entry): entry is { value: number; index: number } => typeof entry.value === "number");
  if (usable.length === 0) {
    return "";
  }
  const lastIndex = Math.max(samples.length - 1, 1);
  const usableWidth = Math.max(width - leftInset - rightInset, 0);
  return usable
    .map(({ value, index }, pointIndex) => {
      const x = leftInset + (index / lastIndex) * usableWidth;
      const y = height - value * height;
      return `${pointIndex === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");
}

function PlaybackWidget(props: {
  uiSession: UiSession | null;
  playbackUiState: PlaybackUiState;
  sourcePath: string;
  onSourcePathChange: Dispatch<SetStateAction<string>>;
  onSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  surfaceWidth: number;
  dock?: "left" | "right";
}) {
  const {
    uiSession,
    playbackUiState,
    sourcePath,
    onSourcePathChange,
    onSnapshotChange,
    surfaceWidth,
    dock = "right",
  } = props;
  const [isBusy, setIsBusy] = useState(false);
  const [scrubCursorSeconds, setScrubCursorSeconds] = useState<number | null>(null);
  const seekRequestIdRef = useRef(0);
  const scrubRef = useRef<HTMLDivElement | null>(null);
  const maxWidthPx = playbackWidgetMaxWidthPx(surfaceWidth);
  const durationSeconds = Math.max(playbackUiState.duration_seconds, 0);
  const committedCursorSeconds = Math.min(Math.max(playbackUiState.cursor_seconds, 0), durationSeconds || 0);
  const cursorSeconds =
    scrubCursorSeconds === null
      ? committedCursorSeconds
      : Math.min(Math.max(scrubCursorSeconds, 0), durationSeconds || 0);
  const canControl = uiSession !== null;
  const canSeek = durationSeconds > 0;
  const label = playbackUiState.registration ?? playbackUiState.icao ?? "Trace";
  const summary = playbackUiState.point_count > 0
    ? `${label} ${formatPlaybackClock(cursorSeconds)} / ${formatPlaybackClock(durationSeconds)}`
    : "Playback";
  const overviewWidth = 320;
  const overviewHeight = 34;
  const knobRadius = 7;
  const scrubSurfaceHeight = 50;
  const cursorRatio = durationSeconds > 0 ? cursorSeconds / durationSeconds : 0;
  const cursorX = knobRadius + cursorRatio * Math.max(overviewWidth - knobRadius * 2, 0);
  const speedPath = profilePathData(playbackUiState.speed_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);
  const altitudePath = profilePathData(playbackUiState.altitude_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);

  useEffect(() => {
    if (scrubCursorSeconds === null) {
      return;
    }
    if (Math.abs(scrubCursorSeconds - committedCursorSeconds) < 1e-6) {
      debugLog("playback.seek.scrub_cleared", {
        scrub_cursor_seconds: scrubCursorSeconds,
        committed_cursor_seconds: committedCursorSeconds,
      });
      setScrubCursorSeconds(null);
    }
  }, [committedCursorSeconds, scrubCursorSeconds]);

  useEffect(() => {
    debugLog("playback.seek.state", {
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
      displayed_cursor_seconds: cursorSeconds,
      duration_seconds: durationSeconds,
      status: playbackUiState.status,
    });
  }, [committedCursorSeconds, cursorSeconds, durationSeconds, playbackUiState.status, scrubCursorSeconds]);

  async function loadTrace() {
    if (!uiSession || !sourcePath.trim()) {
      return;
    }
    setIsBusy(true);
    try {
      const response = await fetch(sourcePath);
      if (!response.ok) {
        throw new Error(`trace load failed: ${response.status}`);
      }
      const traceJson = await response.text();
      const nextSnapshot = await uiSession.loadPlaybackTrace(sourcePath, traceJson);
      onSnapshotChange(nextSnapshot);
    } catch (error) {
      console.error(error);
    } finally {
      setIsBusy(false);
    }
  }

  async function playPause() {
    if (!uiSession) {
      return;
    }
    try {
      const nextSnapshot =
        playbackUiState.status === "playing"
          ? await uiSession.pausePlayback(Date.now())
          : await uiSession.playPlayback(Date.now());
      onSnapshotChange(nextSnapshot);
    } catch (error) {
      console.error(error);
    }
  }

  async function commitSeek(nextCursorSeconds: number, options?: { clearScrub?: boolean }) {
    if (!uiSession) {
      return;
    }
    const clearScrub = options?.clearScrub ?? true;
    const requestId = seekRequestIdRef.current + 1;
    seekRequestIdRef.current = requestId;
    debugLog("playback.seek.commit_start", {
      request_id: requestId,
      requested_cursor_seconds: nextCursorSeconds,
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
    });
    try {
      const nextSnapshot = await uiSession.seekPlayback(nextCursorSeconds, Date.now());
      debugLog("playback.seek.commit_done", {
        request_id: requestId,
        requested_cursor_seconds: nextCursorSeconds,
        returned_cursor_seconds: nextSnapshot.playback_ui_state.cursor_seconds,
        current_request_id: seekRequestIdRef.current,
      });
      if (seekRequestIdRef.current === requestId) {
        onSnapshotChange(nextSnapshot);
        if (clearScrub) {
          setScrubCursorSeconds(null);
        }
      }
    } catch (error) {
      debugLog("playback.seek.commit_error", {
        request_id: requestId,
        requested_cursor_seconds: nextCursorSeconds,
        error: error instanceof Error ? error.message : String(error),
      });
      console.error(error);
    }
  }

  function cursorSecondsForPointer(clientX: number) {
    const rect = scrubRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0 || durationSeconds <= 0) {
      return 0;
    }
    const usableWidth = Math.max(rect.width - knobRadius * 2, 1);
    const rawX = clientX - rect.left;
    const clampedX = Math.min(Math.max(rawX - knobRadius, 0), usableWidth);
    return (clampedX / usableWidth) * durationSeconds;
  }

  function beginScrub(clientX: number) {
    const nextCursorSeconds = cursorSecondsForPointer(clientX);
    debugLog("playback.seek.pointer_move", {
      pointer_x: clientX,
      next_cursor_seconds: nextCursorSeconds,
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
    });
    setScrubCursorSeconds(nextCursorSeconds);
    void commitSeek(nextCursorSeconds, { clearScrub: false });
  }

  return (
    <section
      className={`playbackWidget${dock === "left" ? " isLeftDocked" : ""}`}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      style={maxWidthPx > 0 ? ({ width: `${maxWidthPx}px` } as CSSProperties) : undefined}
    >
      <div className="playbackWidgetTop">
        <span className="playbackWidgetTitle">{summary}</span>
        <span className="playbackWidgetMeta">{playbackUiState.rate.toFixed(1)}x</span>
      </div>
      <div className="playbackWidgetRow">
        <input
          className="playbackWidgetInput"
          value={sourcePath}
          onChange={(event) => onSourcePathChange(event.target.value)}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
        <button type="button" className="playbackWidgetButton" disabled={!canControl || isBusy} onClick={() => void loadTrace()}>
          LOAD
        </button>
      </div>
      <div className="playbackWidgetRow">
        <button type="button" className="playbackWidgetButton" disabled={!canControl || playbackUiState.status === "empty"} onClick={() => void playPause()}>
          {playbackUiState.status === "playing" ? "PAUSE" : "PLAY"}
        </button>
        <label className="playbackWidgetRateLabel">
          SPD
          <input
            className="playbackWidgetRate"
            type="range"
            min={0.25}
            max={11}
            step={0.25}
            value={playbackUiState.rate}
            disabled={!canControl || playbackUiState.status === "empty"}
            onChange={(event) => {
              if (!uiSession) {
                return;
              }
              void uiSession.setPlaybackRate(Number(event.target.value), Date.now()).then(onSnapshotChange).catch((error) => {
                console.error(error);
              });
            }}
          />
        </label>
      </div>
      <div
        ref={scrubRef}
        className="playbackWidgetOverview"
        onPointerDown={(event) => {
          stopPointer(event);
          event.currentTarget.setPointerCapture(event.pointerId);
          beginScrub(event.clientX);
        }}
        onPointerMove={(event) => {
          if ((event.buttons & 1) !== 1) {
            return;
          }
          beginScrub(event.clientX);
        }}
        onPointerUp={(event) => {
          stopPointer(event);
          const nextCursorSeconds = cursorSecondsForPointer(event.clientX);
          debugLog("playback.seek.pointer_up_custom", {
            pointer_x: event.clientX,
            next_cursor_seconds: nextCursorSeconds,
            committed_cursor_seconds: committedCursorSeconds,
            scrub_cursor_seconds: scrubCursorSeconds,
          });
          setScrubCursorSeconds(nextCursorSeconds);
          void commitSeek(nextCursorSeconds, { clearScrub: true });
        }}
      >
        <svg className="playbackWidgetOverviewSvg" viewBox={`0 0 ${overviewWidth} ${overviewHeight}`} preserveAspectRatio="none" aria-hidden="true">
          {altitudePath ? <path className="playbackWidgetAltitudeProfile" d={altitudePath} /> : null}
          {speedPath ? <path className="playbackWidgetSpeedProfile" d={speedPath} /> : null}
          <line className="playbackWidgetCursorLine" x1={cursorX} y1={0} x2={cursorX} y2={overviewHeight} />
          <circle className="playbackWidgetCursorKnob" cx={cursorX} cy={overviewHeight - 1} r={knobRadius} />
        </svg>
      </div>
      <div className="playbackWidgetSeekRow">
        <span className="playbackWidgetClock">{formatPlaybackClock(cursorSeconds)}</span>
        <span className="playbackWidgetClock">{formatPlaybackClock(durationSeconds)}</span>
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
  sessionPlanUiState: FlightPlanUiState | null;
  onOpenPlan: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenCharts: (airportId: string | null, chartId?: string | null) => void;
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
  onReplaceProcedure: (componentIndex: number, built: MaterializedProcedure) => void | Promise<void>;
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
    replaceComponentIndex: number | null;
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
  const planUiState = props.planUiState;
  if (!planUiState) {
    throw new Error("FlightPlanPage requires core-projected FlightPlanUiState");
  }
  const guidance = planUiState.guidance ?? null;
  const structuredSurfaceRef = useRef<HTMLDivElement | null>(null);
  const structuredTableRef = useRef<HTMLDivElement | null>(null);
  const structuredRowRefs = useRef(new Map<string, HTMLElement>());
  const [structuredArrow, setStructuredArrow] = useState<{ path: string; head: string } | null>(null);
  const [structuredGroupBoxes, setStructuredGroupBoxes] = useState<Array<{ key: string; top: number; left: number; width: number; height: number }>>([]);
  const [waypointModalTop, setWaypointModalTop] = useState<number | null>(null);
  const [waypointModalMaxHeight, setWaypointModalMaxHeight] = useState<number | null>(null);
  const componentViews = useMemo(() => planUiState.components, [planUiState.components]);
  if (planUiState.resolved_legs.length > 0 && componentViews.length === 0) {
    throw new Error("FlightPlanUiState invariant failed: resolved legs present but components are empty");
  }
  const showComponentViews = useMemo(
    () => componentViews.some((component) => component.kind !== "waypoint"),
    [componentViews],
  );
  const displayRows = useMemo(() => {
    return planUiState.display_rows.map((row, index) => ({
        showPlateTargetId:
          typeof (row as { show_plate_target_id?: unknown }).show_plate_target_id === "string"
            ? (row as { show_plate_target_id?: string | null }).show_plate_target_id ?? null
            : null,
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
        replaceProcedureComponentIndex: row.replace_procedure_component_index,
        originAnchor: row.origin_anchor,
        destinationAnchor: row.destination_anchor,
        navRef: row.nav_ref,
        groupKey: row.row_kind === "group" || row.depth > 0 ? `group:${row.component_index ?? index}` : null,
        componentIndex: row.component_index,
        componentKind: row.component_kind,
        procedureId: row.procedure_id,
        procedureKind: row.procedure_kind,
        canAddAirwayAfter: row.can_add_airway_after,
        canAddProcedureBefore: row.can_add_procedure_before,
        canChangeAirway: row.can_change_airway,
        canRemoveComponent: row.can_remove_component,
        canReorderComponent: row.can_reorder_component,
        precedingWaypoint: row.preceding_waypoint,
        followingWaypoint: row.following_waypoint,
        actions: row.actions,
      }));
  }, [planUiState.display_rows]);
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
              void adapter.suggestAirwaysNearAnchor(selectedRow.originAnchor!).then((suggestions) => {
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
              replaceComponentIndex: selectedRow.replaceProcedureComponentIndex ?? null,
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
          if (action.id === "show_plate") {
            if (!selectedRow.chartAirportId || !selectedRow.showPlateTargetId) {
              return;
            }
            debugLog("plan.show_plate.match", {
              airport_id: selectedRow.chartAirportId,
              procedure_id: selectedRow.procedureId,
              plate_id: selectedRow.showPlateTargetId,
            });
            props.onOpenCharts(selectedRow.chartAirportId, selectedRow.showPlateTargetId);
            closeTray();
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
      const orderedGroupKeys = displayRows
        .filter((row) => row.rowKind === "group" && row.groupKey)
        .map((row) => row.groupKey as string);

      const nextBoxes = orderedGroupKeys.flatMap((groupKey) => {
        const groupRows = displayRows.filter((row) => row.groupKey === groupKey);
        const firstRow = groupRows[0];
        const lastRow = groupRows[groupRows.length - 1];
        if (!firstRow || !lastRow) {
          return [];
        }
        const firstElement = structuredRowRefs.current.get(firstRow.refKey);
        const lastElement = structuredRowRefs.current.get(lastRow.refKey);
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
  }, [displayRows, reorderOpen, showComponentViews]);

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

    const fromElement = structuredRowRefs.current.get(displayRows[fromIndex]?.refKey ?? "");
    const toElement = structuredRowRefs.current.get(displayRows[toIndex]?.refKey ?? "");
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
        <button type="button" className="navElement navElementStatic" onClick={props.onOpenPlan}>
          <NavElementView navElement={props.sessionPlanUiState?.guidance?.nav_element ?? { active_leg_summary: "", cdi_indicator_dots: null }} />
        </button>
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
                            if (procedurePicker.replaceComponentIndex !== null) {
                              await props.onReplaceProcedure(procedurePicker.replaceComponentIndex, built);
                            } else {
                              await props.onInsertProcedure(
                                procedurePicker.startComponentIndex,
                                procedurePicker.endComponentIndex,
                                built,
                              );
                            }
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
                              const presentation = await adapter.prepareAirwayPresentationForAnchors(
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
  disabled?: boolean;
  style?: TrayDockStyle;
  launcherAccentColor?: string;
  options: TrayOption[];
}) {
  const { launcherLabel, open, onToggle, ariaLabel, disabled = false, style = "compact", launcherAccentColor, options } = props;
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const trayRef = useRef<HTMLElement | null>(null);
  const [trayPosition, setTrayPosition] = useState<{ left: number; top: number } | null>(null);
  const [trayThemeStyle, setTrayThemeStyle] = useState<CSSProperties | null>(null);
  const launcherWide = style === "plate_wide";
  const trayWide = style === "plate_narrow" || style === "plate_wide";
  const launcherDisabled = disabled && !open;

  useEffect(() => {
    if (!open) {
      setTrayPosition(null);
      return;
    }

    function updatePosition() {
      const launcher = launcherRef.current;
      const tray = trayRef.current;
      if (!launcher || !tray) {
        return;
      }
      const launcherStyle = getComputedStyle(launcher);
      const launcherRect = launcher.getBoundingClientRect();
      const gap = thumbPixels(0.1);
      const minInset = gap;
      const maxLeft = Math.max(minInset, window.innerWidth - tray.offsetWidth - gap);
      const maxTop = Math.max(minInset, window.innerHeight - tray.offsetHeight - gap);
      setTrayPosition({
        left: Math.min(Math.max(minInset, launcherRect.left), maxLeft),
        top: Math.min(Math.max(minInset, launcherRect.bottom + gap), maxTop),
      });
      setTrayThemeStyle({
        ["--theme-button-bg" as string]: launcherStyle.getPropertyValue("--theme-button-bg"),
        ["--theme-disabled-button" as string]: launcherStyle.getPropertyValue("--theme-disabled-button"),
        ["--theme-button-fg" as string]: launcherStyle.getPropertyValue("--theme-button-fg"),
      });
    }

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open, options.length, style]);

  return (
    <div className="chartDockColumn">
      <button
        ref={launcherRef}
        type="button"
        className={`chartButton${launcherWide ? " chartButtonWide" : ""}${open ? " isOpen" : ""}${launcherDisabled ? " isDisabled" : ""}`}
        aria-disabled={launcherDisabled}
        style={{
          ...(launcherAccentColor ? ({ ["--tray-accent" as string]: launcherAccentColor } as CSSProperties) : undefined),
        }}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={launcherDisabled ? undefined : onToggle}
      >
        <span className={`chartButtonLabel${launcherWide ? " chartButtonLabelWide" : ""}`}>{launcherLabel}</span>
      </button>
      {open && typeof document !== "undefined"
        ? createPortal(
            <section
              ref={trayRef}
              className={`chartTray chartTrayPortal${trayWide ? " chartTrayWide" : ""} isOpen`}
              aria-label={ariaLabel}
              style={
                trayPosition
                  ? { ...trayThemeStyle, left: `${trayPosition.left}px`, top: `${trayPosition.top}px` }
                  : { ...trayThemeStyle, visibility: "hidden" }
              }
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
            >
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
            </section>,
            document.body,
          )
        : null}
    </div>
  );
}

function ChartsPage(props: {
  appCoreAdapter: AppCoreAdapter | null;
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  plan: typeof samplePlan;
  sessionPlanUiState: FlightPlanUiState | null;
  airports: ChartPageData["airports"];
  selectedAirport: ChartPageData["airports"][number] | null;
  selectedChart: ChartAsset | null;
  folderOpen: boolean;
  viewport: ImageViewportState | null;
  onViewportChange: (next: ImageViewportState | null) => void;
  onFolderOpenChange: (next: boolean) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  onSelectAirport: (airportId: string) => void;
  onSelectChart: (chartId: string) => void;
  onApplyMutation: (mutation: FlightPlanUiMutation) => void | Promise<void>;
  situation: Situation;
  playbackUiState: PlaybackUiState;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  uiSession: UiSession | null;
}) {
  const { appCoreAdapter, page, pageHistory, uptimeLabel, plan, sessionPlanUiState, airports, selectedAirport, selectedChart, folderOpen, viewport, onViewportChange, onFolderOpenChange, onSelectPage, onOpenPlan, onSelectAirport, onSelectChart, onApplyMutation, situation } = props;
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
  const trayGroup = useModalTrayGroup(["page", "airport", "chart", "load"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const [plateProcedureLoads, setPlateProcedureLoads] = useState<ProcedureLoadOption[]>([]);
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

  useEffect(() => {
    if (!appCoreAdapter || !selectedChart) {
      setPlateProcedureLoads([]);
      return;
    }
    let cancelled = false;
    debugLog("charts.load_procedure.query", { plate_id: selectedChart.id });
    void appCoreAdapter.describePlateProcedureLoads(plan, selectedChart.id).then((loads) => {
      debugLog("charts.load_procedure.result", {
        plate_id: selectedChart.id,
        load_count: loads.length,
        loads,
      });
      if (!cancelled) {
        setPlateProcedureLoads(loads);
      }
    }).catch(() => {
      debugLog("charts.load_procedure.error", { plate_id: selectedChart.id });
      if (!cancelled) {
        setPlateProcedureLoads([]);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter, plan, selectedChart?.id]);

  const loadProcedureOptions = useMemo(() => {
    return plateProcedureLoads.map((load, index) => ({
        id: `${load.procedure_id}:${load.runway_transition ?? "none"}:${load.enroute_transition ?? "none"}:${index}`,
        label: load.label,
        active: false,
        onSelect: () => {
          if (!appCoreAdapter) {
            return;
          }
          void appCoreAdapter.materializeProcedure(
            load.airport_id,
            load.procedure_id,
            load.kind,
            load.runway_transition ?? null,
            load.enroute_transition ?? null,
            load.replace_component_index ?? load.start_component_index,
          ).then(async (built) => {
            const mutation =
              load.replace_component_index != null
                ? await appCoreAdapter.replaceProcedureMaterializedUi(plan, load.replace_component_index, built)
                : await appCoreAdapter.insertProcedureMaterializedUi(plan, load.start_component_index, load.end_component_index, built);
            await onApplyMutation(mutation);
            trayGroup.close("load");
          }).catch(() => {});
        },
      }));
  }, [appCoreAdapter, onApplyMutation, plan, plateProcedureLoads, trayGroup]);
  const loadApproachEnabled = loadProcedureOptions.length > 0;

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
          <TrayDock
            launcherLabel={"LOAD\nAPPCH"}
            open={trayGroup.isOpen("load")}
            disabled={!loadApproachEnabled}
            onToggle={() => trayGroup.toggle("load")}
            ariaLabel="Load procedure"
            options={loadProcedureOptions}
          />
          <button
            type="button"
            className={`chartButton${folderOpen ? " isOpen" : ""}`}
            aria-disabled={trayOpen || folderOpen}
            tabIndex={trayOpen ? -1 : undefined}
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

        <button
          type="button"
          className="navElement"
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        >
          <NavElementView navElement={sessionPlanUiState?.guidance?.nav_element ?? { active_leg_summary: "", cdi_indicator_dots: null }} />
        </button>

        <PlaybackWidget
          uiSession={props.uiSession}
          playbackUiState={props.playbackUiState}
          sourcePath={props.playbackSourcePath}
          onSourcePathChange={props.onPlaybackSourcePathChange}
          onSnapshotChange={props.onPlaybackSnapshotChange}
          surfaceWidth={surfaceSize.width}
          dock="left"
        />

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

function SettingsPage(props: {
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  sessionPlanUiState: FlightPlanUiState | null;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
}) {
  const { page, pageHistory, uptimeLabel, sessionPlanUiState, onSelectPage, onOpenPlan } = props;
  const trayGroup = useModalTrayGroup(["page"] as const);
  const [debugOpen, setDebugOpen] = useState(false);

  return (
    <section className="appPage planPage">
      {trayGroup.scrimOpen ? <TrayScrim ariaLabel="Close page tray" onClose={trayGroup.closeAll} /> : null}

      <div className="chartDock">
        <TrayDock
          launcherLabel={pageOptions.find((option) => option.id === page)?.launcherLabel ?? "STGS"}
          open={trayGroup.isOpen("page")}
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
      </div>

      <div className="settingsGrid" aria-label="Settings placeholders">
        {Array.from({ length: 9 }, (_, index) => (
          <button key={index} type="button" className="chartButton chartButtonDouble settingsButton" disabled>
            <span className="chartButtonLabel chartButtonLabelDouble">{`S${index + 1}`}</span>
          </button>
        ))}
      </div>

      <button
        type="button"
        className="navElement"
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={onOpenPlan}
      >
        <NavElementView navElement={sessionPlanUiState?.guidance?.nav_element ?? { active_leg_summary: "", cdi_indicator_dots: null }} />
      </button>

      <div className="debugDock">
        <DebugDock open={debugOpen} onToggle={() => setDebugOpen((open) => !open)}>
          <div className="debugLine">page {pageLabel(page)}</div>
          <div className="debugLine">up {uptimeLabel}</div>
          <div className="debugLine">stack {formatPageStack(pageHistory, { page, selectedMapId: "", selectedChartId: "", selectedChartLabel: "", chartFolderOpen: false })}</div>
        </DebugDock>
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
    case "show_plate":
      return "Show Plate";
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

function mapViewportFromCore(viewport: {
  center: LatLon;
  zoom: number;
}) {
  const centerWorld = latLonToWorld(viewport.center.lat, viewport.center.lon);
  return {
    centerWorldX: centerWorld.x,
    centerWorldY: centerWorld.y,
    zoom: viewport.zoom,
  } satisfies MapViewportState;
}

function sameMapViewport(left: MapViewportState, right: MapViewportState) {
  return (
    Math.abs(left.centerWorldX - right.centerWorldX) < 1e-9 &&
    Math.abs(left.centerWorldY - right.centerWorldY) < 1e-9 &&
    Math.abs(left.zoom - right.zoom) < 1e-9
  );
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
  const presentation = await adapter.prepareAirwayPresentationForAnchors(
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
  const materialized = await adapter.materializeAirwaySelection(
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
    isOpen,
    openId,
    scrimOpen: openId !== null,
    toggle,
  };
}

function midpoint(first: ScreenPoint, second: ScreenPoint): ScreenPoint {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}
