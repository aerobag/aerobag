import { Fragment, Profiler, useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type Dispatch, type MouseEvent, type PointerEvent, type ProfilerOnRenderCallback, type ReactNode, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import type {
  AirwayPresentationPlan,
  AirwaySuggestion,
  ChartPageData,
  ChartFamilyId,
  FlightPlan,
  FlightPlanEntryPreview,
  FlightPlanRouteSegment,
  FlightPlanUiState,
  FlightDataBannerModel,
  LatLon,
  NavSymbolFeature,
  NavElementUiView,
  NavRef,
  PlaybackUiState,
  MapFollowUiState,
  OwnshipControlModel,
  OwnshipRenderState,
  PlateGeoref,
  ProcedureOptions,
  ProcedureLoadOption,
  ProcedureSummary,
  SituationControlInput,
  SituationSample,
  SituationRingCandidate,
  WaypointIdentifierSuggestion,
} from "./domain/types";
import uiTheme from "@shared-ui-theme";
import planViewIcon from "./assets/plan-view-icon.svg";
import {
  airportCircleMarkerPath,
  airportFuelMarkerPath,
  airportOpenMarkerSymbol,
  dataStatusWarningSymbol,
  heliportHPath,
  mapSelectionSpotSymbol,
  metarBknSymbol,
  metarClearSymbol,
  metarFewSymbol,
  metarMissingSymbol,
  metarOvcSymbol,
  metarSctSymbol,
  obstacleDotRadius,
  obstacleShortDotY,
  obstacleShortPath,
  obstacleTallDotY,
  obstacleTallPath,
  pirepGenericSymbol,
  pirepLightIcingSymbol,
  pirepLightTurbulenceSymbol,
  pirepModerateIcingSymbol,
  pirepModerateTurbulenceSymbol,
  pirepSevereIcingSymbol,
  pirepSevereTurbulenceSymbol,
  seaplaneAnchorPath,
  vorBandPath,
  vorOuterHexPath,
} from "./generated/navSymbols";
import {
  loadBestAvailableAdapter,
  type AdapterBackendKind,
  type AppCoreAdapter,
  type DerivedChartPageState,
  type DebugFlagId,
  type MapLayerId,
  type MapSelectionItem,
  type MapSelectionQueryResult,
  type RasterMapUiState,
  type RasterTileDraw,
  type SessionSnapshotRefreshDecision,
  type SessionSnapshotRefreshPriority,
  type UiMapLayerState,
  type UiMapLayerToggleState,
  type UiDebugState,
  type UiDataStatusPageState,
  type UiDataStatusState,
  type UiSession,
  type UiSessionSnapshot,
  type UiInvalidation,
} from "./domain/appCoreAdapter";
import {
  applyPinchGesture,
  createPinchSnapshot,
  dragViewport,
  latLonToWorld,
  preserveViewportForMap,
  scaleForZoom,
  screenToWorld,
  viewportCenterLatLon,
  worldToLatLon,
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
import type {
  AirspaceDisplayPath,
  MapOverlayQueryResult,
  NexradOverlayQueryResult,
  NexradOverlayTile,
  TerrainOverlayQueryResult,
  TerrainOverlayTileRequest,
  VisibleMetarFeature,
  VisiblePirepFeature,
} from "./domain/appCoreAdapter";
import { airwayExitCandidatesFromPresentation } from "./domain/airwayPresentation";
import { debugLog, debugTiming, installGlobalErrorLogging } from "./domain/debugLog";
import { TerrainOverlayRenderer } from "./domain/terrainOverlayRenderer";

type SurfaceSize = {
  width: number;
  height: number;
};

type UiInvalidationRevisions = Record<UiInvalidation, number>;

function initialUiInvalidationRevisions(): UiInvalidationRevisions {
  return {
    session_snapshot: 0,
    raster_tiles: 0,
    map_overlay: 0,
    nexrad_overlay: 0,
    terrain_overlay: 0,
    flight_plan_route: 0,
    debug_panel: 0,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function finiteOrNull(value: number | null | undefined): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function isInvalidUiSessionHandleError(error: unknown): boolean {
  return error instanceof Error && error.message.includes("invalid ui session handle");
}

type StartupProgress = {
  phase: string;
  detail?: string;
  updatedAtMs: number;
};

type StartupFatalError = {
  source: string;
  message: string;
  phase: string;
  detail?: string;
  elapsedMs: number;
  stack?: string;
};

const startupStalledWarningMs = 15_000;
const startupFatalStalledMs = 45_000;
const mainThreadLagProbeIntervalMs = 25;
const mainThreadLagWarnMs = 50;
const animationFrameGapWarnMs = 50;
const dragViewportReactCommitThrottleMs = 90;

function installMainThreadResponsivenessInstrumentation(): () => void {
  if (typeof window === "undefined" || typeof performance === "undefined") {
    return () => {};
  }

  let cancelled = false;
  let lagTimer: number | null = null;
  let rafHandle: number | null = null;
  let nextExpectedProbe = performance.now() + mainThreadLagProbeIntervalMs;
  let previousFrameAt = performance.now();

  const longTaskObserver = installLongTaskObserver();

  function probeLag() {
    if (cancelled) {
      return;
    }
    const now = performance.now();
    const lagMs = now - nextExpectedProbe;
    if (lagMs >= mainThreadLagWarnMs) {
      debugLog("main_thread.event_loop_lag", {
        lag_ms: Math.round(lagMs),
        probe_interval_ms: mainThreadLagProbeIntervalMs,
      });
    }
    nextExpectedProbe = now + mainThreadLagProbeIntervalMs;
    lagTimer = window.setTimeout(probeLag, mainThreadLagProbeIntervalMs);
  }

  function probeFrame(frameAt: number) {
    if (cancelled) {
      return;
    }
    const gapMs = frameAt - previousFrameAt;
    if (gapMs >= animationFrameGapWarnMs) {
      debugLog("main_thread.raf_gap", {
        gap_ms: Math.round(gapMs),
      });
    }
    previousFrameAt = frameAt;
    rafHandle = window.requestAnimationFrame(probeFrame);
  }

  lagTimer = window.setTimeout(probeLag, mainThreadLagProbeIntervalMs);
  rafHandle = window.requestAnimationFrame(probeFrame);

  return () => {
    cancelled = true;
    if (lagTimer !== null) {
      window.clearTimeout(lagTimer);
    }
    if (rafHandle !== null) {
      window.cancelAnimationFrame(rafHandle);
    }
    longTaskObserver?.disconnect();
  };
}

function installLongTaskObserver(): PerformanceObserver | null {
  if (typeof PerformanceObserver === "undefined") {
    debugLog("main_thread.longtask.support", { supported: false, reason: "missing_performance_observer" });
    return null;
  }
  const supportedEntryTypes = PerformanceObserver.supportedEntryTypes ?? [];
  if (!supportedEntryTypes.includes("longtask")) {
    debugLog("main_thread.longtask.support", { supported: false, reason: "unsupported_entry_type" });
    return null;
  }
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      debugLog("main_thread.longtask", {
        name: entry.name,
        start_time_ms: Math.round(entry.startTime),
        duration_ms: Math.round(entry.duration),
      });
    }
  });
  observer.observe({ entryTypes: ["longtask"] });
  debugLog("main_thread.longtask.support", { supported: true });
  return observer;
}

function logAfterNextPaint(tag: string, startedAt: number, data: Record<string, unknown>) {
  if (typeof window === "undefined" || typeof performance === "undefined") {
    return;
  }
  const landedAt = performance.now();
  window.requestAnimationFrame(() => {
    const firstFrameAt = performance.now();
    debugLog(`${tag}.first_frame`, {
      ...data,
      first_frame_ms: Math.round(firstFrameAt - landedAt),
      elapsed_ms: Math.round(firstFrameAt - startedAt),
    });
    window.requestAnimationFrame(() => {
      const afterPaintAt = performance.now();
      debugLog(tag, {
        ...data,
        first_frame_ms: Math.round(firstFrameAt - landedAt),
        frame_gap_ms: Math.round(afterPaintAt - firstFrameAt),
        after_paint_ms: Math.round(afterPaintAt - landedAt),
        elapsed_ms: Math.round(afterPaintAt - startedAt),
      });
    });
  });
}

const reactProfilerActualDurationLogMs = 1;
const reactProfilerCommitDelayLogMs = 8;
const reactProfilerCommitDelayIds = new Set(["MapSurface", "RasterLayer", "VectorLayer"]);

const logReactProfilerRender: ProfilerOnRenderCallback = (
  id,
  phase,
  actualDuration,
  baseDuration,
  startTime,
  commitTime,
) => {
  const commitDelayMs = commitTime - startTime;
  const shouldLogActual = phase === "mount" || actualDuration >= reactProfilerActualDurationLogMs;
  const shouldLogCommitDelay = reactProfilerCommitDelayIds.has(id) && commitDelayMs >= reactProfilerCommitDelayLogMs;
  if (
    shouldLogActual
    || shouldLogCommitDelay
  ) {
    debugLog("react.profiler.render", {
      id,
      phase,
      actual_duration_ms: Math.round(actualDuration),
      base_duration_ms: Math.round(baseDuration),
      commit_delay_ms: Math.round(commitDelayMs),
      start_time_ms: Math.round(startTime),
      commit_time_ms: Math.round(commitTime),
    });
  }
};

function sleepMs(delayMs: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, delayMs));
}

function createLiveDragPerfRunId(): string {
  const suffix = Math.random().toString(36).slice(2, 10);
  return `live-drag-${Date.now().toString(36)}-${suffix}`;
}

function initialStartupProgress(): StartupProgress {
  return {
    phase: "app_shell",
    detail: "Starting web UI",
    updatedAtMs: Date.now(),
  };
}

function startupErrorFromUnknown(
  source: string,
  error: unknown,
  progress: StartupProgress,
  startedAtMs: number,
): StartupFatalError {
  return {
    source,
    message: errorMessage(error),
    phase: progress.phase,
    detail: progress.detail,
    elapsedMs: Math.max(0, Date.now() - startedAtMs),
    stack: error instanceof Error ? error.stack : undefined,
  };
}

function StartupFatalErrorModal({ error }: { error: StartupFatalError }) {
  return (
    <div className="startupErrorScrim" role="alertdialog" aria-modal="true" aria-labelledby="startup-error-title">
      <section className="startupErrorModal">
        <h1 id="startup-error-title">Startup failed</h1>
        <p>{error.message}</p>
        <dl>
          <div>
            <dt>Phase</dt>
            <dd>{error.phase}</dd>
          </div>
          {error.detail ? (
            <div>
              <dt>Detail</dt>
              <dd>{error.detail}</dd>
            </div>
          ) : null}
          <div>
            <dt>Source</dt>
            <dd>{error.source}</dd>
          </div>
          <div>
            <dt>Elapsed</dt>
            <dd>{Math.round(error.elapsedMs / 1000)}s</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function airspaceSvgPathD(path: AirspaceDisplayPath["paths"][number]): string {
  if (path.points.length === 0) {
    return "";
  }
  const [first, ...rest] = path.points;
  const segments = [`M ${first.x} ${first.y}`];
  for (const point of rest) {
    segments.push(`L ${point.x} ${point.y}`);
  }
  if (path.closed) {
    segments.push("Z");
  }
  return segments.join(" ");
}

function airspaceSvgPathListD(paths: AirspaceDisplayPath["paths"]): string {
  return paths.map(airspaceSvgPathD).filter(Boolean).join(" ");
}

function airspaceSegmentListD(segments: NonNullable<AirspaceDisplayPath["decorations"][number]["segments"]>): string {
  return segments.map(([x1, y1, x2, y2]) => `M ${x1} ${y1} L ${x2} ${y2}`).join(" ");
}

function airspaceDecorationD(decoration: AirspaceDisplayPath["decorations"][number]): string {
  return [
    airspaceSvgPathListD(decoration.paths ?? []),
    airspaceSegmentListD(decoration.segments ?? []),
  ].filter(Boolean).join(" ");
}

function airspaceDashArray(dashPx: number[]): string | undefined {
  return dashPx.length > 0 ? dashPx.join(" ") : undefined;
}

function svgStrokeLinecap(lineCap: string): "butt" | "round" | "square" {
  return lineCap === "butt" || lineCap === "square" ? lineCap : "round";
}

function svgStrokeLinejoin(lineJoin: string): "miter" | "round" | "bevel" {
  return lineJoin === "miter" || lineJoin === "bevel" ? lineJoin : "round";
}

function airspaceLabelDividerWidth(parts: { upper: string; lower: string }): number {
  return Math.max(parts.upper.length, parts.lower.length, 2) * 7.2 + 6;
}

function colorWithOpacity(color: string, opacity: number): string {
  return `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;
}

function aviationThemeColor(colorKey: string): string {
  return loadedUiTheme.aviation[colorKey as AviationThemeColorKey] ?? loadedUiTheme.aviation.dark_gray;
}

function AirspaceDisplayPathGroup(props: { feature: AirspaceDisplayPath }) {
  const { feature } = props;
  return (
    <g>
      {feature.paths.map((path, index) => (
        <Fragment key={`${feature.id}:${index}`}>
          <path
            d={airspaceSvgPathD(path)}
            fill={path.closed ? colorWithOpacity(aviationThemeColor(feature.style.fill_color_key), feature.style.fill_opacity) : "none"}
            stroke="none"
          />
          {feature.style.strokes.map((stroke, strokeIndex) => (
            <path
              key={strokeIndex}
              d={airspaceSvgPathD(path)}
              fill="none"
              stroke={aviationThemeColor(stroke.color_key)}
              strokeWidth={stroke.width_px}
              strokeDasharray={airspaceDashArray(stroke.dash_px)}
              strokeLinecap={svgStrokeLinecap(stroke.line_cap)}
              strokeLinejoin="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}
        </Fragment>
      ))}
      {feature.decorations.map((decoration, index) => (
        <path
          key={`${feature.id}:decoration:${index}`}
          d={airspaceDecorationD(decoration)}
          fill="none"
          stroke={aviationThemeColor(decoration.color_key)}
          strokeWidth={decoration.width_px}
          strokeLinecap={svgStrokeLinecap(decoration.line_cap)}
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </g>
  );
}

function offlineRegionPoints(points: Array<{ x: number; y: number }>): string {
  return points.map((point) => `${point.x},${point.y}`).join(" ");
}

function offlineRegionSummaryLines(region: MapOverlayQueryResult["offline_regions"][number]): string[] {
  if (!region.summary.length) {
    return [];
  }
  const entries = region.summary.map((entry) => {
    const suffix = entry.count > 1 ? ` (${entry.count})` : "";
    return `${offlineRegionSummaryIcon(entry.action)} ${entry.cycle}${suffix}`;
  });
  const lines: string[] = [];
  for (let index = 0; index < entries.length; index += 2) {
    lines.push(entries.slice(index, index + 2).join("  "));
  }
  return lines;
}

function offlineRegionSummaryIcon(action: string): string {
  switch (action) {
    case "fetch":
      return "▶";
    case "pause":
      return "‖";
    case "delete":
      return "×";
    default:
      return "●";
  }
}

type AppPage = "map" | "plan" | "charts" | "home" | "data";

type WebPageTilePaintTiming = {
  id: number;
  fromPage: AppPage;
  startedAt: number;
};

type ChartAsset = NonNullable<ChartPageData["airports"][number]>["charts"][number];
type ResolvedChartUrls = {
  assetUrl?: string | null;
  thumbnailUrl?: string | null;
};
type TrayOption = {
  id: string;
  label: string;
  iconSrc?: string;
  toggleState?: UiMapLayerToggleState;
  active?: boolean;
  disabled?: boolean;
  accentColor?: string;
  onSelect: () => void;
};

type UiThemeJson = {
  controls: {
    button_bg: string;
    button_selected_bg: string;
    header_button: string;
    disabled_button: string;
    button_fg: string;
    panel_bg: string;
    panel_border: string;
    panel_fg: string;
    panel_muted: string;
    map_selection_display_bg: string;
    map_selection_display_fg: string;
    situation_status_bg: string;
    situation_status_fg: string;
    data_status_warning_bg: string;
    data_status_warning_stroke: string;
    data_status_quiet_bg: string;
    data_status_quiet_stroke: string;
    chart_surface_bg: string;
    cdi_pointer: string;
  };
  aviation: {
    class_b_d_blue: string;
    class_c_magenta: string;
    tfr_red: string;
    intersection_cyan: string;
    dark_gray: string;
  };
  flight_plan_route: {
    completed: string;
    active: string;
    active_leg_remaining: string;
    remaining: string;
  };
  plate_folder: {
    thumbnail_bg: string;
    label_colors: Record<string, string>;
  };
};

type AviationThemeColorKey = keyof UiThemeJson["aviation"];

type TrayDockStyle = "compact" | "plate_narrow" | "plate_wide" | "wide" | "situation";
type PlateFolderCategory = ChartAsset["folder_category"];

const emptyChartPage: ChartPageData = { airports: [] };
const O88_POSITION = { lat: 38.19338888888888, lon: -121.70363888888889 };
const PAGE_CHART_ICON_SRC = "/icons/icons/page-chart-icon.png?v=20260424b";
const PAGE_PLAN_ICON_SRC = "/icons/icons/page-plan1-icon.png?v=20260424b";
const PAGE_PLATE_ICON_SRC = "/icons/icons/page-plate-icon.png?v=20260424b";
const LAYER_VECTORS_ICON_SRC = "/icons/icons/layer-vectors-icon.png?v=20260424b";
const LAYER_NEXRAD_ICON_SRC = "/icons/icons/layer-nexrad-icon.png?v=20260424b";
const LAYER_TERRAIN_WARNING_ICON_SRC = "/icons/icons/layer-terrain-warning-icon.png?v=20260424b";

function chartFamilyIconSrc(familyId: ChartFamilyId | null | undefined): string | undefined {
  switch (familyId) {
    case "sec":
      return "/icons/icons/sectional-icon.png?v=20260424b";
    case "tac":
      return "/icons/icons/tac-icon.png?v=20260424b";
    case "enr-l":
      return "/icons/icons/ifr-l-icon.png?v=20260424b";
    case "enr-h":
      return "/icons/icons/ifr-h-icon.png?v=20260424b";
    case "shaded-relief":
      return "/icons/icons/shaded-relief-icon.png?v=20260424b";
    case "world-basemap":
      return "/icons/icons/shaded-relief-icon.png?v=20260424b";
    default:
      return undefined;
  }
}

function layerIconSrc(layerId: MapLayerId): string {
  switch (layerId) {
    case "world_basemap":
      return "/icons/icons/shaded-relief-icon.png?v=20260424b";
    case "vectors":
      return LAYER_VECTORS_ICON_SRC;
    case "metars":
      return LAYER_VECTORS_ICON_SRC;
    case "nexrad":
      return LAYER_NEXRAD_ICON_SRC;
    case "terrain_warning":
      return LAYER_TERRAIN_WARNING_ICON_SRC;
    case "offline_regions":
      return LAYER_VECTORS_ICON_SRC;
  }
}

type TerrainOverlayImage = TerrainOverlayTileRequest & {
  rgba: Uint8ClampedArray;
  imageWidth: number;
  imageHeight: number;
};

type TerrainOverlayUiState = {
  query: TerrainOverlayQueryResult | null;
  images: TerrainOverlayImage[];
};

type TerrainPendingFrame = {
  query: TerrainOverlayQueryResult;
  altitudeBucket: number | null;
};

type TerrainTileCacheEntry = {
  rgba: Uint8ClampedArray;
  imageWidth: number;
  imageHeight: number;
};

type TerrainTileRenderTask = {
  request: TerrainOverlayTileRequest;
  altitudeBucket: number;
  generation: number;
};

const TERRAIN_ALTITUDE_BUCKET_FT = 200;

function terrainCacheKey(request: TerrainOverlayTileRequest, altitudeBucket: number | null) {
  return `${request.key}@${altitudeBucket ?? "no-alt"}`;
}

function terrainFrameKey(query: TerrainOverlayQueryResult, altitudeBucket: number | null) {
  return `${altitudeBucket ?? "no-alt"}:${query.tile_requests.map((request) => request.key).join("|")}`;
}

function pruneTerrainFrameStarts(starts: Map<string, number>) {
  while (starts.size > 32) {
    const firstKey = starts.keys().next().value;
    if (!firstKey) {
      return;
    }
    starts.delete(firstKey);
  }
}

function wrappedFeatureRenderKey(id: string, screenX: number, screenY: number): string {
  return `${id}:${Math.round(screenX * 10)}:${Math.round(screenY * 10)}`;
}

function parseTerrainRawRgba(bytes: Uint8Array): TerrainTileCacheEntry {
  if (bytes.byteLength < 4) {
    throw new Error("terrain raw RGBA payload is missing header");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const imageWidth = view.getUint16(0, true);
  const imageHeight = view.getUint16(2, true);
  const expectedBytes = 4 + imageWidth * imageHeight * 4;
  if (bytes.byteLength < expectedBytes) {
    throw new Error(`terrain raw RGBA payload truncated: expected ${expectedBytes}, got ${bytes.byteLength}`);
  }
  return {
    imageWidth,
    imageHeight,
    rgba: new Uint8ClampedArray(bytes.slice(4, expectedBytes)),
  };
}

function terrainRequestSummary(requests: TerrainOverlayTileRequest[]) {
  const zoomCounts = new Map<number, number>();
  for (const request of requests) {
    zoomCounts.set(request.z, (zoomCounts.get(request.z) ?? 0) + 1);
  }
  const summarize = <T extends string | number>(counts: Map<T, number>) =>
    Array.from(counts.entries())
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([key, count]) => `${key}:${count}`)
      .join(",");
  return {
    zooms: summarize(zoomCounts),
  };
}

function terrainAltitudeBucketForOwnship(ownship: OwnshipRenderState) {
  const altitude = ownship.altitude_msl_ft ?? ownship.pressure_altitude_ft;
  return altitude == null || !Number.isFinite(altitude)
    ? null
    : Math.round(altitude / TERRAIN_ALTITUDE_BUCKET_FT) * TERRAIN_ALTITUDE_BUCKET_FT;
}

function cachedTerrainImageForDisplay(
  cache: Map<string, TerrainTileCacheEntry>,
  request: TerrainOverlayTileRequest,
  targetAltitudeBucket: number | null,
) {
  const exact = cache.get(terrainCacheKey(request, targetAltitudeBucket));
  return exact ? ({ ...request, ...exact } satisfies TerrainOverlayImage) : null;
}

function terrainImageForViewport(
  image: TerrainOverlayImage,
  viewport: MapViewportState,
  widthPx: number,
  heightPx: number,
): TerrainOverlayImage {
  const scale = scaleForZoom(viewport.zoom);
  const tilesAtZoom = 2 ** image.z;
  const tileWorldSize = WEB_MERCATOR_WORLD_SIZE / tilesAtZoom;
  const yXyz = (tilesAtZoom - 1) - image.y_tms;
  return {
    ...image,
    left: (image.x * tileWorldSize - viewport.centerWorldX) * scale + widthPx / 2,
    top: (yXyz * tileWorldSize - viewport.centerWorldY) * scale + heightPx / 2,
    size: tileWorldSize * scale,
  };
}

function terrainRequestSortDistance(
  request: TerrainOverlayTileRequest,
  ownship: OwnshipRenderState,
  viewport: MapViewportState,
  widthPx: number,
  heightPx: number,
) {
  const target = ownship.position
    ? latLonToScreen(ownship.position.lat, ownship.position.lon, viewport, widthPx, heightPx)
    : { x: widthPx / 2, y: heightPx / 2 };
  const centerX = request.left + request.size / 2;
  const centerY = request.top + request.size / 2;
  return (centerX - target.x) ** 2 + (centerY - target.y) ** 2;
}

function terrainImagesForCompleteQuery(
  cache: Map<string, TerrainTileCacheEntry>,
  query: TerrainOverlayQueryResult,
  altitudeBucket: number | null,
) {
  if (query.status.state !== "ready") {
    return null;
  }
  const images: TerrainOverlayImage[] = [];
  for (const request of query.tile_requests) {
    const cached = cachedTerrainImageForDisplay(cache, request, altitudeBucket);
    if (!cached) {
      return null;
    }
    images.push(cached);
  }
  return images;
}

const WEB_MERCATOR_WORLD_SIZE = 256;
const RASTER_TILE_OVERDRAW_PX = 1;

function TerrainOverlayCanvasTile({ tile }: { tile: TerrainOverlayImage }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    canvas.width = tile.imageWidth;
    canvas.height = tile.imageHeight;
    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }
    context.putImageData(new ImageData(tile.rgba as ImageDataArray, tile.imageWidth, tile.imageHeight), 0, 0);
  }, [tile.imageHeight, tile.imageWidth, tile.rgba]);

  return (
    <canvas
      ref={canvasRef}
      className="terrainOverlayTile"
      width={tile.imageWidth}
      height={tile.imageHeight}
      style={{
        left: `${tile.left}px`,
        top: `${tile.top}px`,
        width: `${tile.size}px`,
        height: `${tile.size}px`,
      }}
    />
  );
}

function nexradTileBounds(tile: NexradOverlayTile) {
  const xs = [tile.corners.nw.x, tile.corners.ne.x, tile.corners.se.x, tile.corners.sw.x];
  const ys = [tile.corners.nw.y, tile.corners.ne.y, tile.corners.se.y, tile.corners.sw.y];
  const left = Math.min(...xs);
  const top = Math.min(...ys);
  return {
    left,
    top,
    width: Math.max(...xs) - left,
    height: Math.max(...ys) - top,
  };
}

function preloadImage(src: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve();
    image.onerror = () => reject(new Error(`failed to load image: ${src}`));
    image.src = src;
  });
}

async function preloadNexradOverlayImages(query: NexradOverlayQueryResult): Promise<{ loaded: number; failed: number }> {
  if (query.status.state !== "ready" || query.tiles.length === 0) {
    return { loaded: 0, failed: 0 };
  }
  const srcs = Array.from(new Set(query.tiles.map((tile) => tile.src)));
  const results = await Promise.allSettled(srcs.map(preloadImage));
  return {
    loaded: results.filter((result) => result.status === "fulfilled").length,
    failed: results.filter((result) => result.status === "rejected").length,
  };
}

const pageOptions: Array<{ id: AppPage; label: string; launcherLabel: string; iconSrc?: string }> = [
  { id: "map", label: "CHART", launcherLabel: "CHART", iconSrc: PAGE_CHART_ICON_SRC },
  { id: "charts", label: "PLATE", launcherLabel: "PLATE", iconSrc: PAGE_PLATE_ICON_SRC },
  { id: "plan", label: "FLIGHT PLAN", launcherLabel: "PLAN", iconSrc: PAGE_PLAN_ICON_SRC },
  { id: "data", label: "DATA STATUS", launcherLabel: "DATA" },
  { id: "home", label: "HOME", launcherLabel: "HOME" },
];

const webUiStateStorageKey = "aerobag.web.uiState.v1";
const maxViewHistoryDepth = 64;
const loadedUiTheme = uiTheme as UiThemeJson;
const controlTheme = loadedUiTheme.controls;
const plateFolderTheme = loadedUiTheme.plate_folder;
const MATLK_POSITION = { lat: 27.826816666666662, lon: -80.95118611111111 };
const defaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json";
const startupHighLatencyWarningGraceMs = 10_000;
const browserGeolocationSourceId = "browser-geolocation";
const metersPerSecondToKnots = 1.9438444924406;
const metersToFeet = 3.280839895;
const flightDataBannerEdge: FlightDataBannerEdge = "right";

type PersistedWebUiState = {
  page?: AppPage;
  selectedAirportId?: string;
  selectedChartId?: string;
  recentAirportIds?: string[];
};

type FlightDataBannerEdge = "left" | "right";

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

type RasterRenderTile = {
  drawKey: string;
  x: number;
  yTms: number;
  left: number;
  top: number;
  size: number;
  zoom: number;
  zIndex: number;
  src: string;
  mapViewId: string;
  packageName?: string | null;
  chartFamily: ChartFamilyId;
  fallbacks: RasterTileDraw["fallbacks"];
};

function renderTileFromCore(
  tile: RasterTileDraw,
  cssScale = 1,
): RasterRenderTile {
  const packageName = tile.primary.package_name;
  if (!packageName) {
    throw new Error(`raster tile ${tile.draw_key} missing package_name`);
  }
  if (tile.primary.resource.kind !== "resolved_public_url") {
    throw new Error(`raster tile ${tile.draw_key} is not a public unpacked web resource`);
  }
  const src = tile.primary.resource.url;
  return {
    drawKey: tile.draw_key,
    x: tile.x,
    yTms: tile.y_tms,
    left: tile.left_px * cssScale,
    top: tile.top_px * cssScale,
    size: tile.size_px * cssScale,
    zoom: tile.source_zoom,
    zIndex: tile.z_order,
    src,
    mapViewId: tile.primary.map_view_id,
    packageName,
    chartFamily: tile.family,
    fallbacks: tile.fallbacks,
  };
}

function thumbPixels(multiplier = 1) {
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

function shouldLowerStatusControlDock(surfaceWidthPx: number, includesDataStatus: boolean) {
  const leftControlWidthThumbs = 6.4;
  const ownshipPillWidthThumbs = 2;
  const dataStatusWidthThumbs = includesDataStatus ? 0.6 : 0;
  const outerGutterThumbs = 1;
  return surfaceWidthPx > 0 && surfaceWidthPx < thumbPixels(leftControlWidthThumbs + ownshipPillWidthThumbs + dataStatusWidthThumbs + outerGutterThumbs);
}

function flightDataEdgeColumnCount(
  surfaceSize: SurfaceSize,
  cellCount: number,
  situationDockLowered: boolean,
) {
  if (cellCount <= 0 || surfaceSize.height <= 0) {
    return 1;
  }
  const thumb = thumbPixels();
  const gap = thumbPixels(0.06);
  const topReserve = thumbPixels(situationDockLowered ? 2.15 : 0.72);
  const bottomReserve = thumbPixels(1.25);
  const availableHeight = Math.max(thumb, surfaceSize.height - topReserve - bottomReserve);
  const readableCellHeight = thumbPixels(0.64);
  const rowsPerColumn = Math.max(1, Math.floor((availableHeight + gap) / (readableCellHeight + gap)));
  return Math.min(3, Math.max(1, Math.ceil(cellCount / rowsPerColumn)));
}

const airportLabelY = -24;
const vorLabelY = -24;
const fixLabelY = -15;
const obstacleLabelY = -14;

type VectorPointSymbolFeature = {
  kind: string;
  label: string;
  style_class: string;
  obstacle_variant?: "short" | "tall" | null;
  towered: boolean;
  fuel_available: boolean;
  has_paved_runway?: boolean | null;
  heliport?: boolean | null;
  has_water_runway?: boolean | null;
  runway_length_ratio: number;
  longest_runway_heading_true_deg: number | null;
  label_style?: VectorIdentLabelStyle;
};

type VectorIdentLabelStyle = "default" | "flight_plan" | "active_flight_plan";

function VectorIdentLabel(props: {
  label: string;
  y: number;
  className: string;
  labelStyle?: VectorIdentLabelStyle;
}) {
  const { label, y, className, labelStyle = "default" } = props;
  if (!label) return null;
  if (labelStyle === "default") {
    return (
      <text x="0" y={y} textAnchor="middle" className={className}>
        {label}
      </text>
    );
  }
  const width = Math.max(26, label.length * 9.5 + 14);
  const height = 15;
  const styleClass = labelStyle === "active_flight_plan"
    ? "vectorIdent vectorIdentActiveFlightPlan"
    : "vectorIdent vectorIdentFlightPlan";
  return (
    <g className={styleClass}>
      <rect
        x={-width / 2}
        y={y - height + 2}
        width={width}
        height={height}
        rx="2"
        ry="2"
        className="vectorIdentBox"
      />
      <text x="0" y={y} textAnchor="middle" className="vectorIdentText">
        {label}
      </text>
    </g>
  );
}

function VectorPointSymbol(props: { feature: VectorPointSymbolFeature; showLabel?: boolean }) {
  const { feature, showLabel = true } = props;
  const isAirport = feature.style_class === "airport" || feature.kind.toLowerCase() === "airport";
  const isVor = feature.kind.toLowerCase().includes("vor") || feature.style_class === "nav";
  const isObstacle = feature.style_class.startsWith("obstacle") || feature.kind.toLowerCase() === "obs" || feature.kind.toLowerCase() === "obstacle";
  const airportClass = feature.towered ? "airportMarker airportTowered" : "airportMarker airportUntowered";
  const airportLabelClass = feature.towered ? "airportLabel airportToweredLabel" : "airportLabel airportUntoweredLabel";
  if (isAirport) {
    const isHeliport = feature.heliport === true;
    const isSeaplaneBase = feature.has_water_runway === true;
    const usesOpenAirportCircle = isHeliport || isSeaplaneBase || feature.has_paved_runway === false;
    const halfLength = 8 * Math.max(feature.runway_length_ratio, 0.2);
    return (
      <>
        {usesOpenAirportCircle ? (
          <>
            {airportOpenMarkerSymbol.map((layer) => (
              <path
                key={layer.paint}
                d={layer.path}
                style={navSymbolLayerStyle(layer)}
              />
            ))}
          </>
        ) : feature.fuel_available ? (
          <path d={airportFuelMarkerPath} className={airportClass} />
        ) : (
          <path d={airportCircleMarkerPath} className={airportClass} />
        )}
        {isHeliport ? (
          <path d={heliportHPath} className="airportSpecialGlyph airportHeliportGlyph" />
        ) : isSeaplaneBase ? (
          <path
            d={seaplaneAnchorPath}
            transform="rotate(15)"
            className="airportSpecialGlyph airportAnchorGlyph"
          />
        ) : null}
        {!usesOpenAirportCircle && feature.longest_runway_heading_true_deg != null ? (
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
        ) : null}
        {showLabel ? (
          <VectorIdentLabel
            label={feature.label}
            y={airportLabelY}
            className={airportLabelClass}
            labelStyle={feature.label_style}
          />
        ) : null}
      </>
    );
  }
  if (isVor) {
    return (
      <>
        <path d={vorBandPath} className="vorBand" fillRule="evenodd" />
        <path d={vorOuterHexPath} className="vorBorder" />
        {showLabel ? (
          <VectorIdentLabel label={feature.label} y={vorLabelY} className="vorLabel" labelStyle={feature.label_style} />
        ) : null}
      </>
    );
  }
  if (isObstacle) {
    const obstacleClass = feature.style_class === "obstacle-danger"
      ? "obstacleMarker obstacleDanger"
      : feature.style_class === "obstacle-muted"
        ? "obstacleMarker obstacleMuted"
        : "obstacleMarker obstacleCaution";
    const obstacleDotClass = feature.style_class === "obstacle-danger"
      ? "obstacleDot obstacleDangerFill"
      : feature.style_class === "obstacle-muted"
        ? "obstacleDot obstacleMutedFill"
        : "obstacleDot obstacleCautionFill";
    const isTallObstacle = feature.obstacle_variant === "tall";
    const obstaclePath = isTallObstacle ? obstacleTallPath : obstacleShortPath;
    const obstacleDotY = isTallObstacle ? obstacleTallDotY : obstacleShortDotY;
    return (
      <>
        <path d={obstaclePath} className={`${obstacleClass} obstacleMarkerUnder`} />
        <path d={obstaclePath} className={obstacleClass} />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className="obstacleDotUnder" />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className={obstacleDotClass} />
        {showLabel && feature.label ? (
          <VectorIdentLabel label={feature.label} y={obstacleLabelY} className="obstacleLabel" labelStyle={feature.label_style} />
        ) : null}
      </>
    );
  }
  return (
    <>
      <path d="M 0 -8 L 7 6 L -7 6 Z" className="fixMarker" />
      {showLabel ? (
        <VectorIdentLabel label={feature.label} y={fixLabelY} className="fixLabel" labelStyle={feature.label_style} />
      ) : null}
    </>
  );
}

function spotSymbolClassName(paint: string): string {
  switch (paint) {
    case "map_selection_spot_under":
      return "mapSelectionSpotPegUnder";
    case "map_selection_spot_dot":
      return "mapSelectionSpotPegDot";
    default:
      return "mapSelectionSpotPeg";
  }
}

function navSymbolColor(token: string | null | undefined): string | undefined {
  switch (token) {
    case "none":
      return "none";
    case "white":
      return "white";
    case "ink_70":
      return "rgba(8, 18, 24, 0.7)";
    case "ink_75":
      return "rgba(8, 18, 24, 0.75)";
    case "class_c_magenta":
      return "var(--theme-class-c-magenta)";
    case "button_bg":
      return "var(--theme-button-bg)";
    case "white_90":
      return "rgba(255, 255, 255, 0.9)";
    case "white_68":
      return "rgba(255, 255, 255, 0.68)";
    case "paper":
      return "#fffef8";
    case "pirep_ink":
      return "#071015";
    case "metar_category":
      return "var(--metar-color)";
    case "pirep_symbol":
      return "var(--pirep-color)";
    case "data_status_symbol_ink":
      return "var(--data-status-symbol-ink)";
    default:
      return undefined;
  }
}

function navSymbolLayerStyle(layer: { fill?: string | null; stroke?: string | null; stroke_width?: number | null; line_cap?: string | null; line_join?: string | null }) {
  return {
    fill: navSymbolColor(layer.fill),
    stroke: navSymbolColor(layer.stroke),
    strokeWidth: layer.stroke_width ?? undefined,
    strokeLinecap: svgStrokeLinecap(layer.line_cap ?? "butt"),
    strokeLinejoin: svgStrokeLinejoin(layer.line_join ?? "miter"),
  };
}

function MapSelectionSpotSymbol() {
  return (
    <>
      {mapSelectionSpotSymbol.map((layer) => (
        <path
          key={layer.paint}
          className={spotSymbolClassName(layer.paint)}
          style={navSymbolLayerStyle(layer)}
          d={layer.path}
          transform={layer.transform_degrees != null ? `rotate(${layer.transform_degrees})` : undefined}
        />
      ))}
    </>
  );
}

function RenderNavSymbolLayers(props: { layers: readonly { path: string; paint: string; fill?: string | null; stroke?: string | null; stroke_width?: number | null; line_cap?: string | null; line_join?: string | null; transform_degrees?: number | null }[] }) {
  return (
    <>
      {props.layers.map((layer, index) => (
        <path
          key={`${layer.paint}:${index}`}
          style={navSymbolLayerStyle(layer)}
          d={layer.path}
          transform={layer.transform_degrees != null ? `rotate(${layer.transform_degrees})` : undefined}
        />
      ))}
    </>
  );
}

function metarCategoryClass(category: string): string {
  switch (category) {
    case "vfr":
      return "metarVfr";
    case "mvfr":
      return "metarMvfr";
    case "ifr":
      return "metarIfr";
    case "lifr":
      return "metarLifr";
    default:
      return "metarMissing";
  }
}

function MetarSymbol(props: { feature: VisibleMetarFeature }) {
  const { feature } = props;
  const categoryClass = metarCategoryClass(feature.flight_category);
  const layers = feature.ceiling_amount === "few"
    ? metarFewSymbol
    : feature.ceiling_amount === "sct"
      ? metarSctSymbol
      : feature.ceiling_amount === "bkn"
        ? metarBknSymbol
        : feature.ceiling_amount === "ovc"
          ? metarOvcSymbol
          : feature.ceiling_amount === "missing"
            ? metarMissingSymbol
            : metarClearSymbol;
  return (
    <g className={`metarSymbol ${categoryClass}`} aria-hidden="true">
      <RenderNavSymbolLayers layers={layers} />
    </g>
  );
}

function pirepStrokeColor(symbol: string): string {
  switch (symbol) {
    case "light-turbulence":
      return "#e9be5e";
    case "moderate-turbulence":
      return "#e79347";
    case "severe-turbulence":
      return "#d24700";
    case "light-icing":
      return "#64c6e9";
    case "moderate-icing":
      return "#3c7ee0";
    case "severe-icing":
      return "#0018e0";
    default:
      return "#071015";
  }
}

function PirepSymbol(props: { feature: VisiblePirepFeature; scale?: number }) {
  const { feature, scale = 1 } = props;
  const layers = feature.symbol === "light-turbulence"
    ? pirepLightTurbulenceSymbol
    : feature.symbol === "moderate-turbulence"
      ? pirepModerateTurbulenceSymbol
      : feature.symbol === "severe-turbulence"
        ? pirepSevereTurbulenceSymbol
        : feature.symbol === "light-icing"
          ? pirepLightIcingSymbol
          : feature.symbol === "moderate-icing"
            ? pirepModerateIcingSymbol
            : feature.symbol === "severe-icing"
              ? pirepSevereIcingSymbol
              : pirepGenericSymbol;
  return (
    <g
      className="pirepSymbol"
      transform={scale === 1 ? undefined : `scale(${scale})`}
      style={{ "--pirep-color": pirepStrokeColor(feature.symbol) } as CSSProperties}
      aria-hidden="true"
    >
      <RenderNavSymbolLayers layers={layers} />
    </g>
  );
}

function PlanWaypointSymbol(props: { feature: NavSymbolFeature | null }) {
  const { feature } = props;
  if (!feature) {
    return null;
  }
  return (
    <svg className="planWaypointSymbol" viewBox="-20 -20 40 40" aria-hidden="true">
      <VectorPointSymbol feature={feature} showLabel={false} />
    </svg>
  );
}

function waypointSuggestionName(suggestion: WaypointIdentifierSuggestion): string | null {
  const displayName = suggestion.display_name.replace(/\s+/g, " ").trim();
  if (
    !displayName ||
    displayName.toUpperCase() === suggestion.kind.toUpperCase() ||
    displayName.toUpperCase() === suggestion.identifier.toUpperCase()
  ) {
    return null;
  }
  return displayName;
}

function waypointSuggestionDistance(suggestion: WaypointIdentifierSuggestion): string {
  return suggestion.distance_text;
}

function WaypointButtonContent(props: {
  label: string;
  symbolFeature: NavSymbolFeature | null | undefined;
  details?: Array<string | null | undefined>;
  indented?: boolean;
}) {
  const details = (props.details ?? []).filter((detail): detail is string => Boolean(detail?.trim()));
  return (
    <>
      <span className={`planStructuredLabel${props.indented ? " isIndented" : ""}${details.length > 0 ? " hasDetails" : ""}`}>
        <span className="waypointButtonTitle">{props.label}</span>
        {details.map((detail, index) => (
          <span key={`${index}:${detail}`} className="waypointButtonDetail">{detail}</span>
        ))}
      </span>
      <PlanWaypointSymbol feature={props.symbolFeature ?? null} />
    </>
  );
}

function emptyPlaybackUiState(): PlaybackUiState {
  return {
    status: "empty",
    source_path: null,
    title_label: "Playback",
    registration: null,
    icao: null,
    aircraft_type: null,
    point_count: 0,
    duration_seconds: 0,
    cursor_seconds: 0,
    cursor_label: "0:00",
    duration_label: "0:00",
    rate: 1,
    speed_profile_norm: [],
    altitude_profile_norm: [],
    gap_spans: [],
  };
}

function emptyMapFollowUiState(): MapFollowUiState {
  return {
    can_center_here: false,
    following: true,
  };
}

function defaultUiMapLayerState(): UiMapLayerState {
  return {
    world_basemap: { visible: true, enabled: true },
    vectors: { visible: true, enabled: true },
    metars: { visible: true, enabled: true },
    nexrad: { visible: false, enabled: true },
    terrain_warning: { visible: true, enabled: true },
    offline_regions: { visible: false, enabled: true },
  };
}

function normalizeUiMapLayerState(state: Partial<UiMapLayerState> | null | undefined): UiMapLayerState {
  return {
    ...defaultUiMapLayerState(),
    ...state,
  };
}

function defaultUiDebugState(): UiDebugState {
  const debugTiles = typeof window !== "undefined" && new URLSearchParams(window.location.search).has("debugTiles");
  return {
    tile_labels: debugTiles,
    nexrad_tile_labels: false,
    playback_visible: false,
    fast_tiles: false,
    offline_simulated_clock_buttons: false,
    sequencing_finish_lines: false,
  };
}

function emptyNexradOverlayStats(): NexradOverlayQueryResult["stats"] {
  return {
    source_tile_count: 0,
    render_piece_count: 0,
    split_count: 0,
    max_affine_error_px: 0,
    level_pixel_span_px: 0,
    max_level_pixel_stretch_px: 0,
    max_stack_depth: 0,
    res: null,
    observed_at_utc: null,
  };
}

export default function App() {
  const [sessionStartMs] = useState(() => Date.now());
  const uptimeLabel = useSessionUptimeLabel(sessionStartMs);
  const initialDebugState = useMemo(defaultUiDebugState, []);
  const persistedUiState = useMemo(readPersistedWebUiState, []);
  const [page, setPage] = useState<AppPage>(persistedUiState.page ?? "map");
  const [pageHistory, setPageHistory] = useState<AppViewSnapshot[]>([]);
  const [appCoreAdapter, setAppCoreAdapter] = useState<AppCoreAdapter | null>(null);
  const [adapterBackend, setAdapterBackend] = useState<AdapterBackendKind>("wasm");
  const [sessionInitError, setSessionInitError] = useState<string | null>(null);
  const [startupProgress, setStartupProgress] = useState<StartupProgress>(initialStartupProgress);
  const [startupFatalError, setStartupFatalError] = useState<StartupFatalError | null>(null);
  const startupProgressRef = useRef<StartupProgress>(startupProgress);
  const startupFatalErrorRef = useRef<StartupFatalError | null>(null);
  const startupResolvedRef = useRef(false);
  const startupVisualReadyRef = useRef(false);
  const pageTilePaintTimingRef = useRef<WebPageTilePaintTiming | null>(null);
  const nextPageTilePaintTimingIdRef = useRef(1);
  const [debugOpen, setDebugOpen] = useState(false);
  const highLatencyWarningsSuppressedRef = useRef(true);
  const highLatencyWarningTimerRef = useRef<number | null>(null);
  const [rasterMapState, setRasterMapState] = useState<RasterMapUiState | null>(null);
  const [mapSelectorLoadError, setMapSelectorLoadError] = useState<string | null>(null);
  const initialRecentAirportIds = useMemo(
    () => mergeRecentAirportIds(emptyChartPage.airports, persistedUiState.recentAirportIds ?? []),
    [persistedUiState],
  );
  const initialChartPageState = useMemo<DerivedChartPageState>(
    () => ({
      airports: emptyChartPage.airports,
      recent_airport_ids: initialRecentAirportIds,
      selected_airport_id: persistedUiState.selectedAirportId ?? initialRecentAirportIds[0] ?? "",
      selected_chart_id: persistedUiState.selectedChartId ?? "",
    }),
    [initialRecentAirportIds, persistedUiState.selectedAirportId, persistedUiState.selectedChartId],
  );
  const [uiSession, setUiSession] = useState<UiSession | null>(null);
  const [uiInvalidationRevisions, setUiInvalidationRevisions] = useState<UiInvalidationRevisions>(
    initialUiInvalidationRevisions,
  );
  const sessionSnapshotRefreshInFlightRef = useRef(false);
  const sessionSnapshotRefreshTimerRef = useRef<number | null>(null);
  const [sessionSnapshot, setSessionSnapshot] = useState<UiSessionSnapshot>({
    app_state: {
      active_plan: null,
      content_policy: "PreferLocal",
      last_content_report: null,
    },
    app_ui_state: {
      active_plan: null,
      ownship: {
        render: {
          mode: "none",
          banner_text: "NO GPS POSITION",
          banner_severity: "warning",
          draw_aircraft: false,
          draw_predictor: false,
          draw_cdi: false,
          position: null,
          orientation_deg: null,
          magnetic_variation_deg: null,
          speed_kt: null,
          altitude_msl_ft: null,
          pressure_altitude_ft: null,
        },
        controls: {
          mode: "none",
          selection: { kind: "auto" },
          launcher_label: "No GPS",
          launcher_tone: "unavailable",
          sources: [],
          situation_controls: [],
        },
      },
      flight_data_banner: { cells: [] },
      content_policy: "PreferLocal",
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
    map_layer_state: defaultUiMapLayerState(),
    data_status_state: {
      boxes: [],
      launcher_count: null,
      launcher_severity: "info",
    },
    data_status_page_state: {
      title: "Data status",
      summary: "Status will appear after core session data loads.",
      rows: [],
    },
    debug_state: initialDebugState,
    raster_map: null,
  });
  const [playbackSourcePath, setPlaybackSourcePath] = useState(defaultPlaybackTracePath);
  const [debugWarningActive, setDebugWarningActive] = useState(false);
  const [derivedChartPageState, setDerivedChartPageState] = useState<DerivedChartPageState>(initialChartPageState);
  const [chartPageStateLoadError, setChartPageStateLoadError] = useState<string | null>(null);
  const logDebugWarning = useCallback((tag: string, data?: unknown) => {
    debugLog(tag, data);
    debugLog("debug.warn.latched", { tag, data });
    setDebugWarningActive(true);
  }, []);
  const logHighLatencyWarning = useCallback((tag: string, data?: unknown) => {
    if (highLatencyWarningsSuppressedRef.current) {
      debugLog(`${tag}.startup_suppressed`, data);
      return;
    }
    logDebugWarning(tag, data);
  }, [logDebugWarning]);
  const markStartupProgress = useCallback((phase: string, detail?: string) => {
    const progress = { phase, detail, updatedAtMs: Date.now() };
    startupProgressRef.current = progress;
    debugLog("startup.progress", { phase, detail });
    setStartupProgress(progress);
  }, []);
  const reportStartupFatalError = useCallback((source: string, error: unknown) => {
    if (startupResolvedRef.current || startupFatalErrorRef.current) {
      return;
    }
    const fatal = startupErrorFromUnknown(source, error, startupProgressRef.current, sessionStartMs);
    startupFatalErrorRef.current = fatal;
    debugLog("startup.fatal", fatal);
    setSessionInitError(`${fatal.source}: ${fatal.message}`);
    setStartupFatalError(fatal);
  }, [sessionStartMs]);
  const setDebugFlag = useCallback(async (flagId: DebugFlagId, enabled: boolean) => {
    if (uiSession === null) {
      setSessionSnapshot((snapshot) => ({
        ...snapshot,
        debug_state: { ...snapshot.debug_state, [flagId]: enabled },
      }));
      return;
    }
    setSessionSnapshot(await uiSession.setDebugFlag(flagId, enabled));
  }, [uiSession]);
  const applySituationControlInput = useCallback(async (input: SituationControlInput) => {
    if (!uiSession) {
      return;
    }
    setSessionSnapshot(await uiSession.applySituationControlInput(input, Date.now()));
  }, [uiSession]);
  const reportStartupVisualReady = useCallback(() => {
    if (startupVisualReadyRef.current) {
      return;
    }
    startupVisualReadyRef.current = true;
    if (typeof window === "undefined") {
      highLatencyWarningsSuppressedRef.current = false;
      return;
    }
    highLatencyWarningTimerRef.current = window.setTimeout(() => {
      highLatencyWarningsSuppressedRef.current = false;
      highLatencyWarningTimerRef.current = null;
    }, startupHighLatencyWarningGraceMs);
  }, []);
  const appState = sessionSnapshot.app_state;
  const appUiState = sessionSnapshot.app_ui_state;
  const mapLayerState = useMemo(
    () => normalizeUiMapLayerState(sessionSnapshot.map_layer_state),
    [sessionSnapshot.map_layer_state],
  );
  const debugOwnshipDriverActive = appUiState.ownship.controls.sources.some(
    (source) => source.source_kind === "debug_ownship_driver" && source.active,
  );
  const playbackUiState = sessionSnapshot.playback_ui_state;
  const mapFollowUiState = sessionSnapshot.map_follow_ui_state;
  const chartPageData: ChartPageData = useMemo(
    () => ({ airports: derivedChartPageState.airports }),
    [derivedChartPageState.airports],
  );

  useEffect(() => installMainThreadResponsivenessInstrumentation(), []);

  useEffect(() => {
    if (playbackUiState.source_path) {
      setPlaybackSourcePath(playbackUiState.source_path);
    }
  }, [playbackUiState.source_path]);

  useEffect(() => () => {
    if (highLatencyWarningTimerRef.current !== null) {
      window.clearTimeout(highLatencyWarningTimerRef.current);
    }
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") {
      highLatencyWarningsSuppressedRef.current = false;
      return;
    }
    const timer = window.setTimeout(() => {
      if (highLatencyWarningTimerRef.current === null) {
        highLatencyWarningsSuppressedRef.current = false;
      }
    }, startupHighLatencyWarningGraceMs);
    return () => window.clearTimeout(timer);
  }, []);

  const clearSessionSnapshotRefreshTimer = useCallback(() => {
    if (sessionSnapshotRefreshTimerRef.current === null) {
      return;
    }
    window.clearTimeout(sessionSnapshotRefreshTimerRef.current);
    sessionSnapshotRefreshTimerRef.current = null;
  }, []);

  const handleSessionSnapshotRefreshDecisionRef = useRef<(
    decision: SessionSnapshotRefreshDecision,
    source: string,
  ) => void>(() => {});

  const armSessionSnapshotRefreshTimer = useCallback((delayMs: number, reason: string, source: string) => {
    clearSessionSnapshotRefreshTimer();
    const roundedDelayMs = Math.max(0, Math.round(delayMs));
    sessionSnapshotRefreshTimerRef.current = window.setTimeout(() => {
      sessionSnapshotRefreshTimerRef.current = null;
      if (!uiSession) {
        return;
      }
      void uiSession.pollSessionSnapshotRefresh().then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "timer");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "timer",
          error: errorMessage(error),
        });
      });
    }, roundedDelayMs);
    debugLog("session.snapshot.refresh.scheduled", {
      reason,
      source,
      delay_ms: roundedDelayMs,
      in_flight: sessionSnapshotRefreshInFlightRef.current,
    });
  }, [clearSessionSnapshotRefreshTimer, uiSession]);

  const startSessionSnapshotRefresh = useCallback((reason: string) => {
    if (!uiSession) {
      return;
    }
    if (sessionSnapshotRefreshInFlightRef.current) {
      debugLog("session.snapshot.refresh.start_while_in_flight", { reason });
      void uiSession.requestSessionSnapshotRefresh("timely", reason).then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "start_while_in_flight");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "start_while_in_flight",
          error: errorMessage(error),
        });
      });
      return;
    }
    sessionSnapshotRefreshInFlightRef.current = true;
    debugLog("session.snapshot.refresh.start", { reason });
    void uiSession.snapshot().then((nextSnapshot) => {
      setSessionSnapshot(nextSnapshot);
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.error", { reason, error: errorMessage(error) });
    }).finally(() => {
      sessionSnapshotRefreshInFlightRef.current = false;
      void uiSession.sessionSnapshotRefreshCompleted().then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "after_in_flight");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "after_in_flight",
          error: errorMessage(error),
        });
      });
    });
  }, [uiSession]);

  const handleSessionSnapshotRefreshDecision = useCallback((
    decision: SessionSnapshotRefreshDecision,
    source: string,
  ) => {
    if (decision.kind === "idle") {
      clearSessionSnapshotRefreshTimer();
      return;
    }
    if (decision.kind === "schedule") {
      armSessionSnapshotRefreshTimer(decision.delay_ms, decision.reason, source);
      return;
    }
    clearSessionSnapshotRefreshTimer();
    startSessionSnapshotRefresh(`${source}:${decision.reason}`);
  }, [armSessionSnapshotRefreshTimer, clearSessionSnapshotRefreshTimer, startSessionSnapshotRefresh, uiSession]);

  useEffect(() => {
    handleSessionSnapshotRefreshDecisionRef.current = handleSessionSnapshotRefreshDecision;
  }, [handleSessionSnapshotRefreshDecision]);

  useEffect(() => clearSessionSnapshotRefreshTimer, [clearSessionSnapshotRefreshTimer]);

  const requestSessionSnapshotRefresh = useCallback((priority: SessionSnapshotRefreshPriority, reason: string) => {
    if (!uiSession) {
      return;
    }
    void uiSession.requestSessionSnapshotRefresh(priority, reason).then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, "request");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: "request",
        error: errorMessage(error),
      });
    });
  }, [uiSession]);

  const handleMapViewportGestureActiveChange = useCallback((active: boolean) => {
    if (!uiSession) {
      return;
    }
    void uiSession.sessionSnapshotViewportGestureActiveChanged(active).then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, active ? "gesture_active" : "gesture_end");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: active ? "gesture_active" : "gesture_end",
        error: errorMessage(error),
      });
    });
  }, [uiSession]);

  const handleMapViewportGestureActivity = useCallback(() => {
    if (!uiSession) {
      return;
    }
    void uiSession.sessionSnapshotViewportActivity().then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, "viewport_activity");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: "viewport_activity",
        error: errorMessage(error),
      });
    });
  }, [uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    uiSession.setInvalidationListener((invalidations) => {
      setUiInvalidationRevisions((current) => {
        const next = { ...current };
        for (const invalidation of invalidations) {
          next[invalidation] = (next[invalidation] ?? 0) + 1;
        }
        return next;
      });
      if (invalidations.includes("session_snapshot")) {
        const priority = invalidations.includes("flight_plan_route") ? "timely" : "low_priority";
        requestSessionSnapshotRefresh(priority, "invalidation");
      }
    });
    return () => {
      uiSession.setInvalidationListener(null);
    };
  }, [requestSessionSnapshotRefresh, uiSession]);

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

  useEffect(() => {
    if (!uiSession || !debugOwnshipDriverActive) {
      return;
    }
    let cancelled = false;
    let inFlight = false;
    const tick = () => {
      if (inFlight) {
        return;
      }
      inFlight = true;
      void uiSession.tickDebugOwnshipDriver(Date.now()).then((nextSnapshot) => {
        if (!cancelled) {
          setSessionSnapshot(nextSnapshot);
        }
      }).catch(() => {}).finally(() => {
        inFlight = false;
      });
    };
    tick();
    const timer = window.setInterval(tick, 250);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [debugOwnshipDriverActive, uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    const handler = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableTarget(event.target)) {
        return;
      }
      const input = situationInputForKey(event.key);
      if (!input) {
        return;
      }
      event.preventDefault();
      void applySituationControlInput(input);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [applySituationControlInput, uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    let cancelled = false;
    let watchId: number | null = null;
    let selectedAutoAfterFirstFix = false;

    const updateStatus = (connectionState: "unavailable" | "searching" | "connected" | "stale" | "failed", enabled: boolean, statusLabel: string) => {
      void uiSession.updateOwnshipSourceStatus({
        source_id: browserGeolocationSourceId,
        connection_state: connectionState,
        enabled,
        status_label: statusLabel,
      }).then((nextSnapshot) => {
        if (!cancelled) {
          setSessionSnapshot(nextSnapshot);
        }
      }).catch((error) => {
        debugLog("geolocation.status_update_failed", { error: errorMessage(error) });
      });
    };

    void (async () => {
      try {
        let nextSnapshot = await uiSession.registerOwnshipSource({
          source_id: browserGeolocationSourceId,
          source_kind: "device_gps",
          display_name: "Browser Location",
          selectable: true,
          auto_eligible: true,
        });
        if (cancelled) {
          return;
        }
        setSessionSnapshot(nextSnapshot);

        if (typeof navigator === "undefined" || !navigator.geolocation) {
          updateStatus("unavailable", false, "Browser geolocation unavailable");
          return;
        }

        nextSnapshot = await uiSession.updateOwnshipSourceStatus({
          source_id: browserGeolocationSourceId,
          connection_state: "searching",
          enabled: true,
          status_label: "Waiting for browser location",
        });
        if (cancelled) {
          return;
        }
        setSessionSnapshot(nextSnapshot);

        watchId = navigator.geolocation.watchPosition(
          (position) => {
            const coords = position.coords;
            const eventTimeEpochMs = Number.isFinite(position.timestamp) ? Math.trunc(position.timestamp) : Date.now();
            const receivedTimeEpochMs = Date.now();
            const speedMps = finiteOrNull(coords.speed);
            const altitudeM = finiteOrNull(coords.altitude);
            const sample: SituationSample = {
              source_id: browserGeolocationSourceId,
              source_kind: "device_gps",
              event_time_epoch_ms: eventTimeEpochMs,
              received_time_epoch_ms: receivedTimeEpochMs,
              position: { lat: coords.latitude, lon: coords.longitude },
              horizontal_accuracy_m: finiteOrNull(coords.accuracy),
              vertical_accuracy_m: finiteOrNull(coords.altitudeAccuracy),
              track_deg_true: finiteOrNull(coords.heading),
              heading_deg_true: null,
              ground_speed_kt: speedMps == null ? null : speedMps * metersPerSecondToKnots,
              altitude_msl_ft: altitudeM == null ? null : altitudeM * metersToFeet,
              pressure_altitude_ft: null,
            };
            void uiSession.pushSituationSample(sample).then(async (pushedSnapshot) => {
              if (cancelled) {
                return;
              }
              setSessionSnapshot(pushedSnapshot);
              if (selectedAutoAfterFirstFix) {
                return;
              }
              selectedAutoAfterFirstFix = true;
              const selectedSnapshot = await uiSession.selectOwnshipSource({ kind: "auto" });
              if (!cancelled) {
                setSessionSnapshot(selectedSnapshot);
              }
            }).catch((error) => {
              debugLog("geolocation.sample_failed", { error: errorMessage(error) });
            });
          },
          (error) => {
            const permissionDenied = error.code === error.PERMISSION_DENIED;
            updateStatus(
              permissionDenied ? "unavailable" : "failed",
              !permissionDenied,
              permissionDenied ? "Browser location permission denied" : error.message || "Browser location failed",
            );
          },
          {
            enableHighAccuracy: true,
            maximumAge: 1_000,
            timeout: 15_000,
          },
        );
      } catch (error) {
        updateStatus("failed", false, errorMessage(error));
      }
    })();

    return () => {
      cancelled = true;
      if (watchId !== null && typeof navigator !== "undefined" && navigator.geolocation) {
        navigator.geolocation.clearWatch(watchId);
      }
    };
  }, [uiSession]);
  const currentPlan = appState.active_plan;
  const chartPageStateRequest = useMemo(() => {
    if (!currentPlan) {
      return null;
    }
    const recentAirportIds = sessionSnapshot.chart_page_state.recent_airport_ids;
    const selectedAirportId = sessionSnapshot.chart_page_state.selected_airport_id || undefined;
    const selectedChartId = sessionSnapshot.chart_page_state.selected_chart_id || undefined;
    return {
      key: JSON.stringify([currentPlan, recentAirportIds, selectedAirportId ?? null, selectedChartId ?? null]),
      plan: currentPlan,
      recentAirportIds,
      selectedAirportId,
      selectedChartId,
    };
  }, [
    currentPlan,
    sessionSnapshot.chart_page_state.recent_airport_ids,
    sessionSnapshot.chart_page_state.selected_airport_id,
    sessionSnapshot.chart_page_state.selected_chart_id,
  ]);
  const planUiState = appUiState.active_plan;
  const recentAirportIds = derivedChartPageState.recent_airport_ids;
  const selectedAirportId = derivedChartPageState.selected_airport_id;
  const selectedChartId = derivedChartPageState.selected_chart_id;

  const selectedMap = rasterMapState;
  const [mapViewport, setMapViewport] = useState<MapViewportState>(() => {
    const center = latLonToWorld(O88_POSITION.lat, O88_POSITION.lon);
    return {
      centerWorldX: center.x,
      centerWorldY: center.y,
      zoom: 10.0,
    };
  });
  const [chartViewport, setChartViewport] = useState<ImageViewportState | null>(null);
  const [chartFolderOpen, setChartFolderOpen] = useState(false);
  const selectedFamily = useMemo(
    () => rasterMapState?.family_options.find((family) => family.active) ?? null,
    [rasterMapState],
  );
  const selectedAirport = useMemo(
    () => chartPageData.airports.find((airport) => airport.id === selectedAirportId) ?? chartPageData.airports[0] ?? null,
    [chartPageData, selectedAirportId],
  );
  const selectedChart = useMemo(
    () => selectedAirport?.charts.find((chart) => chart.id === selectedChartId) ?? selectedAirport?.charts[0] ?? null,
    [selectedAirport, selectedChartId],
  );

  useEffect(() => {
    debugLog("charts.selection.render", {
      selected_airport_id: selectedAirportId,
      selected_chart_id: selectedChartId,
      selected_chart_label: selectedChart?.label ?? null,
      selected_chart_asset_path: selectedChart?.asset_path ?? null,
    });
  }, [selectedAirportId, selectedChartId, selectedChart?.label, selectedChart?.asset_path]);
  useEffect(() => {
    let cancelled = false;
    markStartupProgress("adapter.load", "Loading app-core adapter");
    loadBestAvailableAdapter().then((loaded) => {
      if (!cancelled) {
        markStartupProgress("adapter.ready", loaded.detail);
        setAppCoreAdapter(loaded.adapter);
        setAdapterBackend(loaded.backend);
        setSessionInitError(null);
      }
    }).catch((error) => {
      if (!cancelled) {
        const message = error instanceof Error ? error.message : String(error);
        setSessionInitError(`WASM adapter init failed: ${message}`);
        reportStartupFatalError("adapter.load", error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [markStartupProgress, reportStartupFatalError]);

  useEffect(() => {
    installGlobalErrorLogging();
    const handleError = (event: ErrorEvent) => {
      reportStartupFatalError("window.error", event.error ?? new Error(event.message));
    };
    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      reportStartupFatalError("window.unhandledrejection", event.reason);
    };
    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleUnhandledRejection);
    return () => {
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleUnhandledRejection);
    };
  }, [reportStartupFatalError]);

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
    debugTiming("startup.session.create", async () => {
      markStartupProgress("session.empty_plan", "Creating empty flight plan");
      const initialPlan = await debugTiming("startup.session.empty_plan.core", () =>
        appCoreAdapter.emptyFlightPlan(),
      );
      markStartupProgress("session.create", "Creating core UI session");
      const created = await debugTiming("startup.session.create.core", () => appCoreAdapter.createUiSession(
        initialPlan,
        initialRecentAirportIds,
        initialChartPageState.selected_airport_id,
        initialChartPageState.selected_chart_id,
      ));
      markStartupProgress("session.initial_snapshot", "Using initial session snapshot");
      let createdSnapshot = debugTiming("startup.session.initial_snapshot", () => created.initialSnapshot());
      markStartupProgress("session.ownship_start", "Starting ownship source");
      createdSnapshot = await debugTiming("startup.session.ownship_start", () => created.setSituation({
        position: { kind: "lat_lon", lat: MATLK_POSITION.lat, lon: MATLK_POSITION.lon },
        orientation_deg: 342,
        speed_kt: 0,
      }));
      for (const flagId of Object.keys(initialDebugState) as DebugFlagId[]) {
        if (initialDebugState[flagId]) {
          createdSnapshot = await created.setDebugFlag(flagId, true);
        }
      }
      debugLog("session.create.snapshot", {
        app_state_active_plan: createdSnapshot.app_state.active_plan?.id ?? null,
        app_ui_state_nav_element: createdSnapshot.app_ui_state.active_plan?.guidance?.nav_element ?? null,
      });
      nextSession = created;
      if (!cancelled) {
        markStartupProgress("session.ready", "Initial session ready");
        setUiSession(created);
        setSessionSnapshot(createdSnapshot);
      }
    }).catch((error) => {
      console.error("failed to initialize web ui session", error);
      if (!cancelled) {
        reportStartupFatalError("session.create", error);
      }
    });
    return () => {
      cancelled = true;
      void nextSession?.destroy();
    };
  }, [adapterBackend, appCoreAdapter, initialChartPageState.selected_airport_id, initialChartPageState.selected_chart_id, initialDebugState, initialRecentAirportIds, markStartupProgress, reportStartupFatalError]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    let cancelled = false;
    void uiSession.startLiveFeedSubscription().catch((error) => {
      debugLog("live_feeds.subscription.failed", { message: error instanceof Error ? error.message : String(error) });
    });
    return () => {
      cancelled = true;
      void uiSession.stopLiveFeedSubscription().catch((error) => {
        if (!cancelled) {
          debugLog("live_feeds.subscription_stop.failed", { message: error instanceof Error ? error.message : String(error) });
        }
      });
    };
  }, [uiSession]);

  useEffect(() => {
    if (!sessionSnapshot.raster_map) {
      return;
    }
    setRasterMapState(sessionSnapshot.raster_map);
    setMapSelectorLoadError(null);
  }, [sessionSnapshot.raster_map]);

  useEffect(() => {
    let cancelled = false;
    if (!appCoreAdapter || !chartPageStateRequest || !uiSession) {
      return;
    }
    debugTiming(
      "charts.page_state.load",
      () => appCoreAdapter.deriveChartPageState(
        chartPageStateRequest.plan,
        chartPageStateRequest.recentAirportIds,
        chartPageStateRequest.selectedAirportId,
        chartPageStateRequest.selectedChartId,
      ),
    ).then((state) => {
      if (cancelled) {
        return;
      }
      setDerivedChartPageState(state);
      setChartPageStateLoadError(null);
    }).catch((error) => {
      if (!cancelled) {
        setChartPageStateLoadError(`failed to derive chart page state: ${errorMessage(error)}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [
    appCoreAdapter,
    chartPageStateRequest?.key,
    uiSession,
  ]);

  useEffect(() => {
    if (!selectedMap) {
      return;
    }
    setMapViewport((current) => preserveViewportForMap(current, selectedMap));
  }, [selectedMap]);

  const appReady =
    appCoreAdapter !== null &&
    uiSession !== null &&
    selectedMap !== null &&
    currentPlan !== null &&
    planUiState !== null;

  useEffect(() => {
    if (appReady) {
      startupResolvedRef.current = true;
      markStartupProgress("ready", "App ready");
    }
  }, [appReady, markStartupProgress]);

  useEffect(() => {
    if (appReady || startupFatalError) {
      return;
    }
    const intervalId = window.setInterval(() => {
      if (startupResolvedRef.current || startupFatalErrorRef.current) {
        return;
      }
      const idleMs = Date.now() - startupProgressRef.current.updatedAtMs;
      if (idleMs >= startupFatalStalledMs) {
        reportStartupFatalError(
          "startup.watchdog",
          new Error(`startup made no progress for ${Math.round(idleMs / 1000)}s`),
        );
      } else if (idleMs >= startupStalledWarningMs) {
        debugLog("startup.stalled", {
          phase: startupProgressRef.current.phase,
          detail: startupProgressRef.current.detail,
          idle_ms: idleMs,
        });
      }
    }, 1_000);
    return () => window.clearInterval(intervalId);
  }, [appReady, reportStartupFatalError, startupFatalError]);

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
      selectedMapId: rasterMapState?.selected_map_id ?? "",
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
    if (snapshot.selectedMapId && uiSession) {
      void uiSession.selectRasterMap(snapshot.selectedMapId).then((nextSnapshot) => {
        setSessionSnapshot(nextSnapshot);
      }).catch((error) => {
        setMapSelectorLoadError(`failed to restore map selector state: ${errorMessage(error)}`);
      });
    }
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
  }, [page, pageHistory, rasterMapState?.selected_map_id, mapViewport, selectedAirportId, selectedChartId, recentAirportIds, chartViewport, chartFolderOpen]);

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
    if (nextPage === "map") {
      const timing = {
        id: nextPageTilePaintTimingIdRef.current++,
        fromPage: page,
        startedAt: performance.now(),
      };
      pageTilePaintTimingRef.current = timing;
      debugLog("web.page-to-map.start", { id: timing.id, from_page: timing.fromPage });
      requestAnimationFrame(() => {
        debugLog("web.page-to-map.visible_frame", {
          id: timing.id,
          from_page: timing.fromPage,
          elapsed_ms: Math.round(performance.now() - timing.startedAt),
        });
      });
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

  function navigateToMostRecentChartOrPlate() {
    const target = pageHistory
      .slice()
      .reverse()
      .find((snapshot) => snapshot.page === "map" || snapshot.page === "charts");
    if (target) {
      pushViewSnapshot(target);
      return;
    }
    navigateToPage("map");
  }
  const mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory);

  function openPlateTarget(airportId: string, target: "Folder" | "CSup") {
    const targetChartId = `Plate:${airportId}:${target}`;
    const nextRecentAirportIds = moveAirportToFront(recentAirportIds, airportId, chartPageData.airports);
    if (uiSession) {
      void uiSession.restoreChartPageState(
        nextRecentAirportIds,
        airportId,
        targetChartId,
      ).then((nextSnapshot) => {
        setSessionSnapshot(nextSnapshot);
      }).catch((error) => {
        debugLog("plates.open.target.failed", {
          airport_id: airportId,
          target,
          error: errorMessage(error),
        });
      });
    }
    pushViewSnapshot({
      page: "charts",
      selectedAirportId: airportId,
      selectedChartId: targetChartId,
      selectedChartLabel: "",
      recentAirportIds: nextRecentAirportIds,
      chartViewport: null,
      chartFolderOpen: target === "Folder",
    });
  }

  const themeVars = useMemo(
    () =>
      ({
        "--theme-button-bg": controlTheme.button_bg,
        "--theme-button-selected-bg": controlTheme.button_selected_bg,
        "--theme-header-button": controlTheme.header_button,
        "--theme-disabled-button": controlTheme.disabled_button,
        "--theme-button-fg": controlTheme.button_fg,
        "--theme-panel-bg": controlTheme.panel_bg,
        "--theme-panel-border": controlTheme.panel_border,
        "--theme-panel-fg": controlTheme.panel_fg,
        "--theme-panel-muted": controlTheme.panel_muted,
        "--theme-map-selection-display-bg": controlTheme.map_selection_display_bg,
        "--theme-map-selection-display-fg": controlTheme.map_selection_display_fg,
        "--theme-situation-status-bg": controlTheme.situation_status_bg,
        "--theme-situation-status-fg": controlTheme.situation_status_fg,
        "--theme-data-status-warning-bg": controlTheme.data_status_warning_bg,
        "--theme-data-status-warning-stroke": controlTheme.data_status_warning_stroke,
        "--theme-data-status-quiet-bg": controlTheme.data_status_quiet_bg,
        "--theme-data-status-quiet-stroke": controlTheme.data_status_quiet_stroke,
        "--theme-chart-surface-bg": controlTheme.chart_surface_bg,
        "--theme-cdi-pointer": controlTheme.cdi_pointer,
        "--theme-class-b-d-blue": loadedUiTheme.aviation.class_b_d_blue,
        "--theme-class-c-magenta": loadedUiTheme.aviation.class_c_magenta,
        "--theme-tfr-red": loadedUiTheme.aviation.tfr_red,
        "--theme-intersection-cyan": loadedUiTheme.aviation.intersection_cyan,
        "--theme-aviation-dark-gray": loadedUiTheme.aviation.dark_gray,
      }) as CSSProperties,
    [],
  );

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const shouldHideStartupShell =
      startupFatalError !== null ||
      sessionInitError !== null ||
      mapSelectorLoadError !== null ||
      chartPageStateLoadError !== null ||
      (appReady &&
        currentPlan !== null &&
        planUiState !== null);
    if (shouldHideStartupShell) {
      const reason = startupFatalError !== null
        ? "startup_fatal_error"
        : sessionInitError !== null
          ? "session_init_error"
          : mapSelectorLoadError !== null
            ? "map_selector_load_error"
            : chartPageStateLoadError !== null
              ? "chart_page_state_load_error"
              : "app_ready";
      window.__aerobag_hide_startup_shell?.(reason);
    }
  }, [appReady, chartPageStateLoadError, currentPlan, mapSelectorLoadError, planUiState, sessionInitError, startupFatalError]);

  if (startupFatalError) {
    return (
      <main className="appShell" style={themeVars}>
        <StartupFatalErrorModal error={startupFatalError} />
      </main>
    );
  }

  if (sessionInitError || mapSelectorLoadError || chartPageStateLoadError) {
    return (
      <main className="appFrame">
        <section className="appPage planPage">
          <div className="planGuidanceSummary">{sessionInitError ?? mapSelectorLoadError ?? chartPageStateLoadError}</div>
        </section>
      </main>
    );
  }

  if (!appReady || !currentPlan || !planUiState || !selectedMap) {
    return (
      <main className="startupProgressHost" aria-live="polite">
        <span>{startupProgress.detail ?? startupProgress.phase}</span>
      </main>
    );
  }

  return (
    <main className="appShell" style={themeVars}>
      <div className={`pageLayer${page === "map" ? " isActive" : ""}`} aria-hidden={page !== "map"}>
        <MapPage
          appCoreAdapter={appCoreAdapter}
          page={page}
          uptimeLabel={uptimeLabel}
          debugState={sessionSnapshot.debug_state}
          mapLayerState={mapLayerState}
          selectedMap={selectedMap}
          selectedFamily={selectedFamily}
          familyOptions={rasterMapState.family_options}
          viewport={mapViewport}
          pageTilePaintTiming={pageTilePaintTimingRef.current}
          uiInvalidationRevisions={uiInvalidationRevisions}
          onPageTilePaintTimingComplete={(id) => {
            if (pageTilePaintTimingRef.current?.id === id) {
              pageTilePaintTimingRef.current = null;
            }
          }}
          onViewportChange={setMapViewport}
          onViewportGestureActiveChange={handleMapViewportGestureActiveChange}
          onViewportGestureActivity={handleMapViewportGestureActivity}
          onSelectMapFamily={(familyId) => {
            if (!uiSession) {
              return;
            }
            void uiSession.selectMapFamily(familyId).then(setSessionSnapshot).catch((error) => {
              setMapSelectorLoadError(`failed to select map family: ${errorMessage(error)}`);
            });
          }}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenPlateTarget={openPlateTarget}
          ownship={appUiState.ownship.render}
          ownshipControls={appUiState.ownship.controls}
          flightDataBanner={appUiState.flight_data_banner}
          dataStatusState={sessionSnapshot.data_status_state}
          onStatusAction={(actionId) => {
            if (!uiSession) {
              return;
            }
            void uiSession.performStatusAction(actionId).then(setSessionSnapshot);
          }}
          plan={currentPlan}
          planUiState={planUiState}
          playbackUiState={playbackUiState}
          mapFollowUiState={mapFollowUiState}
          mapFollowTargetViewport={sessionSnapshot.map_follow_target_viewport}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          onSituationControlInput={applySituationControlInput}
          uiSession={uiSession}
          debugOpen={debugOpen}
          onDebugToggle={() => setDebugOpen((open) => !open)}
          onDebugFlagChange={(flagId, enabled) => void setDebugFlag(flagId, enabled)}
          debugWarningActive={debugWarningActive}
          onDebugWarning={logDebugWarning}
          onHighLatencyWarning={logHighLatencyWarning}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </div>

      <div className={`pageLayer${page === "plan" ? " isActive" : ""}`} aria-hidden={page !== "plan"}>
        <FlightPlanPage
          appCoreAdapter={appCoreAdapter}
          uiSession={uiSession}
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          plan={currentPlan}
          planUiState={planUiState}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
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
          onInsertAirportWaypointAtRow={async (rowUid, before, airportId) => {
            if (!appCoreAdapter) return;
            const waypoint = await appCoreAdapter.resolveWaypointIdentifier(airportId);
            if (!waypoint) {
              throw new Error(`Unknown waypoint ${airportId}`);
            }
            await uiSession.insertWaypointAtFlightPlanRow(rowUid, before, waypoint);
          }}
          onPreviewFlightPlanEntry={async (input) => {
            if (!uiSession) throw new Error("flight plan preview requires live core session");
            return uiSession.previewFlightPlanEntry(input);
          }}
          onAppendFlightPlanEntry={async (input) => {
            if (!uiSession) return;
            await uiSession.appendFlightPlanEntry(input);
          }}
          onActivateNextLeg={async () => {
            if (!uiSession) return;
            const nextSnapshot = await uiSession.activateNextLeg();
            setSessionSnapshot(nextSnapshot);
          }}
          onSuspendSequencing={async () => {
            if (!uiSession) return;
            const nextSnapshot = await uiSession.suspendSequencing();
            setSessionSnapshot(nextSnapshot);
          }}
          onUnsuspendSequencing={async () => {
            if (!uiSession) return;
            const nextSnapshot = await uiSession.unsuspendSequencing();
            setSessionSnapshot(nextSnapshot);
          }}
          onSequenceActiveLeg={async () => {
            if (!uiSession) return;
            const nextSnapshot = await uiSession.sequenceActiveLeg();
            setSessionSnapshot(nextSnapshot);
          }}
          onRestoreDirectTo={async () => {
            if (!uiSession) return;
            const nextSnapshot = await uiSession.restoreDirectTo();
            setSessionSnapshot(nextSnapshot);
          }}
          onPerformFlightPlanRowAction={async (rowUid, actionUid) => {
            if (!uiSession) return;
            await uiSession.performFlightPlanRowAction(rowUid, actionUid);
          }}
          onInsertAirwayAtRow={async (rowUid, entryIndex, exitIndex, presentation) => {
            if (!uiSession) return;
            await uiSession.insertAirwayAtFlightPlanRow(rowUid, presentation, entryIndex, exitIndex);
          }}
          onSelectProcedureAtRow={async (rowUid, airportId, procedureId, enrouteTransition) => {
            if (!uiSession) return;
            await uiSession.selectProcedureAtFlightPlanRow(
              rowUid,
              airportId,
              procedureId,
              "approach",
              null,
              enrouteTransition,
            );
          }}
          debugWarningActive={debugWarningActive}
        />
      </div>

      <div className={`pageLayer${page === "charts" ? " isActive" : ""}`} aria-hidden={page !== "charts"}>
        <ChartsPage
          appCoreAdapter={appCoreAdapter}
          page={page}
          plan={currentPlan}
          planUiState={planUiState}
          airports={chartPageData.airports}
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
            debugLog("charts.select.request", {
              requested_airport_id: selectedAirport?.id ?? null,
              requested_chart_id: chartId,
              requested_chart_label: nextChart?.label ?? null,
              requested_chart_asset_path: nextChart?.asset_path ?? null,
            });
            if (uiSession) {
              void uiSession.selectChart(chartId).then((nextSnapshot) => {
                debugLog("charts.select.snapshot", {
                  requested_chart_id: chartId,
                  selected_airport_id: nextSnapshot.chart_page_state.selected_airport_id,
                  selected_chart_id: nextSnapshot.chart_page_state.selected_chart_id,
                });
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
          ownship={appUiState.ownship.render}
          ownshipControls={appUiState.ownship.controls}
          playbackUiState={playbackUiState}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          onSituationControlInput={applySituationControlInput}
          debugState={sessionSnapshot.debug_state}
          uiSession={uiSession}
          debugWarningActive={debugWarningActive}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </div>

      <div className={`pageLayer${page === "home" ? " isActive" : ""}`} aria-hidden={page !== "home"}>
        <HomePage
          page={page}
          planUiState={planUiState}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          debugWarningActive={debugWarningActive}
        />
      </div>

      <div className={`pageLayer${page === "data" ? " isActive" : ""}`} aria-hidden={page !== "data"}>
        <DataStatusPage
          page={page}
          state={sessionSnapshot.data_status_page_state}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
        />
      </div>
    </main>
  );
}

function MapPage(props: {
  appCoreAdapter: AppCoreAdapter;
  page: AppPage;
  uptimeLabel: string;
  debugState: UiDebugState;
  mapLayerState: UiMapLayerState;
  selectedMap: RasterMapUiState;
  selectedFamily: RasterMapUiState["family_options"][number] | null;
  familyOptions: RasterMapUiState["family_options"];
  viewport: MapViewportState;
  pageTilePaintTiming: WebPageTilePaintTiming | null;
  uiInvalidationRevisions: UiInvalidationRevisions;
  onPageTilePaintTimingComplete: (id: number) => void;
  onViewportChange: (next: MapViewportState) => void;
  onViewportGestureActiveChange: (active: boolean) => void;
  onViewportGestureActivity: () => void;
  onSelectMapFamily: (familyId: ChartFamilyId) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  onOpenPlateTarget: (airportId: string, target: "Folder" | "CSup") => void;
  ownship: OwnshipRenderState;
  ownshipControls: OwnshipControlModel;
  flightDataBanner: FlightDataBannerModel;
  dataStatusState: UiDataStatusState;
  onStatusAction: (actionId: string) => void | Promise<void>;
  plan: FlightPlan;
  planUiState: FlightPlanUiState | null;
  playbackUiState: PlaybackUiState;
  mapFollowUiState: MapFollowUiState;
  mapFollowTargetViewport: { center: LatLon; zoom: number; rotation_deg: number; pitch_deg: number } | null;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  onSituationControlInput: (input: SituationControlInput) => void;
  uiSession: UiSession | null;
  debugOpen: boolean;
  onDebugToggle: () => void;
  onDebugFlagChange: (flagId: DebugFlagId, enabled: boolean) => void;
  debugWarningActive: boolean;
  onDebugWarning: (tag: string, data?: unknown) => void;
  onHighLatencyWarning: (tag: string, data?: unknown) => void;
  onFirstVisualReady: () => void;
}) {
  const {
    appCoreAdapter,
    debugState,
    mapLayerState,
    page,
    uptimeLabel,
    selectedMap,
    selectedFamily,
    familyOptions,
    viewport,
    pageTilePaintTiming,
    uiInvalidationRevisions,
    onPageTilePaintTimingComplete,
    onViewportChange,
    onViewportGestureActiveChange,
    onViewportGestureActivity,
    onSelectMapFamily,
    onSelectPage,
    onOpenPlan,
    onOpenPlateTarget,
    ownship,
    ownshipControls,
    flightDataBanner,
    dataStatusState,
    onStatusAction,
    plan,
    planUiState,
    uiSession,
    debugOpen,
    onDebugToggle,
    onDebugFlagChange,
    onPlaybackSnapshotChange,
    onSituationControlInput,
    mapFollowUiState,
    mapFollowTargetViewport,
    debugWarningActive,
    onDebugWarning,
    onHighLatencyWarning,
    onFirstVisualReady,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapContentTransformRef = useRef<HTMLDivElement | null>(null);
  const trayGroup = useModalTrayGroup(["family", "layers", "status", "ownship"] as const);
  const [layerToggleBusyId, setLayerToggleBusyId] = useState<MapLayerId | null>(null);
  const [chartSearch, setChartSearch] = useState<{
    query: string;
    open: boolean;
    loading: boolean;
    error: string | null;
    suggestions: WaypointIdentifierSuggestion[];
  }>({
    query: "",
    open: false,
    loading: false,
    error: null,
    suggestions: [],
  });
  const [mapOverlay, setMapOverlay] = useState<MapOverlayQueryResult>({
    visible_features: [],
    visible_metars: [],
    visible_pireps: [],
    airspace_paths: [],
    tfr_paths: [],
    airspace_labels: [],
    offline_regions: [],
  });
  const [nexradOverlay, setNexradOverlay] = useState<NexradOverlayQueryResult>({
    status: { state: "hidden" },
    tiles: [],
    stats: emptyNexradOverlayStats(),
  });
  const [nexradTransferSamples, setNexradTransferSamples] = useState<
    Array<{ atMs: number; transferBytes: number; encodedBytes: number; decodedBytes: number }>
  >([]);
  const [nexradOverlayViewport, setNexradOverlayViewport] = useState<MapViewportState | null>(null);
  const nexradQueryRequestRef = useRef<{
    id: number;
    session: UiSession;
    viewport: MapViewportState;
    width: number;
    height: number;
    debugTileLabels: boolean;
  } | null>(null);
  const nexradQueryRequestIdRef = useRef(0);
  const nexradQueryPendingRef = useRef(false);
  const nexradQueryPumpActiveRef = useRef(false);
  const [terrainOverlay, setTerrainOverlay] = useState<TerrainOverlayUiState>({ query: null, images: [] });
  const terrainTileCacheRef = useRef<Map<string, TerrainTileCacheEntry>>(new Map());
  const terrainTileInFlightRef = useRef<Set<string>>(new Set());
  const terrainRenderQueueRef = useRef<Map<string, TerrainTileRenderTask>>(new Map());
  const terrainRenderPumpActiveRef = useRef(false);
  const terrainRendererRef = useRef<TerrainOverlayRenderer | null>(null);
  const terrainRenderGenerationRef = useRef(0);
  const terrainCurrentBucketRef = useRef<number | null>(null);
  const terrainPendingFrameRef = useRef<TerrainPendingFrame | null>(null);
  const terrainFrameStartRef = useRef<Map<string, number>>(new Map());
  const lastTerrainRenderPlanKeyRef = useRef("");
  const [flightPlanRoute, setFlightPlanRoute] = useState<FlightPlanRouteSegment[]>([]);
  const [mapOverlayViewport, setMapOverlayViewport] = useState<MapViewportState | null>(null);
  const mapOverlayQueryRequestRef = useRef<{
    id: number;
    requestedAt: number;
    session: UiSession;
    viewport: MapViewportState;
    center: LatLon;
    width: number;
    height: number;
    layerKey: string;
  } | null>(null);
  const mapOverlayQueryRequestIdRef = useRef(0);
  const landedMapOverlayQueryRequestIdRef = useRef(0);
  const mapOverlayQueryPendingRef = useRef(false);
  const mapOverlayQueryPumpActiveRef = useRef(false);
  const mapOverlayLandingTimingRef = useRef<{
    id: number;
    requestedAt: number;
    queryStartedAt: number;
    landStartedAt: number;
    landEndedAt: number;
    visibleFeatures: number;
    visibleMetars: number;
    visiblePireps: number;
    airspacePaths: number;
    tfrPaths: number;
    airspaceLabels: number;
    offlineRegions: number;
    flightPlanFeatures: number;
    committed: boolean;
  } | null>(null);
  const viewportRef = useRef<MapViewportState>(viewport);
  const committedViewportRef = useRef<MapViewportState>(viewport);
  const pendingReactViewportRef = useRef<MapViewportState | null>(null);
  const pendingReactViewportTimerRef = useRef<number | null>(null);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const clickCandidateRef = useRef<{ pointerId: number; start: ScreenPoint; latest: ScreenPoint } | null>(null);
  const gestureActiveRef = useRef(false);
  const viewportGestureUntilRef = useRef(0);
  const followSyncSerialRef = useRef(0);
  const deferredFollowSyncViewportRef = useRef<MapViewportState | null>(null);
  const [followSyncPendingSerial, setFollowSyncPendingSerial] = useState(0);
  const [followTargetRetryToken, setFollowTargetRetryToken] = useState(0);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [liveDragPerfRunning, setLiveDragPerfRunning] = useState(false);
  const [lastLiveDragPerfRunId, setLastLiveDragPerfRunId] = useState<string | null>(null);
  const [mapSelection, setMapSelection] = useState<{
    point: ScreenPoint;
    result: MapSelectionQueryResult;
    selectedItem: MapSelectionItem | null;
    detailModal: { title: string; text: string } | null;
  } | null>(null);
  const firstVisualReadyRef = useRef(false);
  const statusControlDockLowered = shouldLowerStatusControlDock(surfaceSize.width, dataStatusState.boxes.length > 0);
  const flightDataBannerEdgeLayout = surfaceSize.width > surfaceSize.height;
  const flightDataBannerEdgeColumnCount = flightDataBannerEdgeLayout
    ? flightDataEdgeColumnCount(surfaceSize, flightDataBanner.cells.length, statusControlDockLowered)
    : 1;

  function pumpTerrainRenderQueue() {
    if (terrainRenderPumpActiveRef.current) {
      return;
    }
    terrainRenderPumpActiveRef.current = true;
    void (async () => {
      try {
        while (terrainRenderQueueRef.current.size > 0) {
          const renderer = terrainRendererRef.current;
          if (!renderer) {
            terrainRenderQueueRef.current.clear();
            return;
          }
          const next = terrainRenderQueueRef.current.entries().next().value as
            | [string, TerrainTileRenderTask]
            | undefined;
          if (!next) {
            return;
          }
          const [cacheKey, task] = next;
          terrainRenderQueueRef.current.delete(cacheKey);
          if (terrainTileCacheRef.current.has(cacheKey) || terrainTileInFlightRef.current.has(cacheKey)) {
            continue;
          }
          if (task.altitudeBucket !== terrainCurrentBucketRef.current) {
            continue;
          }
          terrainTileInFlightRef.current.add(cacheKey);
          try {
            const tileStartedAt = performance.now();
            if (task.altitudeBucket !== terrainCurrentBucketRef.current) {
              continue;
            }
            const renderStartedAt = performance.now();
            const result = await renderer.renderTile({
              generation: task.generation,
              cacheKey,
              tileKey: task.request.key,
              altitudeBucketFt: task.altitudeBucket,
              sourceTiles: task.request.source_tiles,
            });
            const renderElapsedMs = performance.now() - renderStartedAt;
            const rawBytes = result.rawBytes;
            const parsed = parseTerrainRawRgba(rawBytes);
            terrainTileCacheRef.current.set(cacheKey, parsed);
            debugLog("terrain.overlay.tile.done", {
              key: task.request.key,
              altitude_bucket: task.altitudeBucket,
              generation: result.generation,
              raw_bytes: rawBytes.byteLength,
              image_width: parsed.imageWidth,
              image_height: parsed.imageHeight,
              render_ms: Math.round(renderElapsedMs),
              elapsed_ms: Math.round(performance.now() - tileStartedAt),
            });
            setTerrainOverlay((current) => {
              const pendingFrame = terrainPendingFrameRef.current;
              if (
                !pendingFrame
                || pendingFrame.altitudeBucket !== task.altitudeBucket
                || !pendingFrame.query.tile_requests.some((request) => terrainCacheKey(request, task.altitudeBucket) === cacheKey)
              ) {
                return current;
              }
              const readyImages = terrainImagesForCompleteQuery(terrainTileCacheRef.current, pendingFrame.query, task.altitudeBucket);
              if (!readyImages) {
                return current;
              }
              const frameKey = terrainFrameKey(pendingFrame.query, task.altitudeBucket);
              const frameStartedAt = terrainFrameStartRef.current.get(frameKey);
              terrainFrameStartRef.current.delete(frameKey);
              debugLog("terrain.overlay.frame.ready", {
                altitude_bucket: task.altitudeBucket,
                request_count: pendingFrame.query.tile_requests.length,
                image_count: readyImages.length,
                elapsed_ms: frameStartedAt == null ? null : Math.round(performance.now() - frameStartedAt),
              });
              return {
                query: pendingFrame.query,
                images: readyImages,
              };
            });
          } catch (error: unknown) {
            debugLog("terrain.overlay.tile.error", {
              key: task.request.key,
              error: errorMessage(error),
            });
          } finally {
            terrainTileInFlightRef.current.delete(cacheKey);
          }
          await new Promise((resolve) => window.setTimeout(resolve, 0));
        }
      } finally {
        terrainRenderPumpActiveRef.current = false;
        if (terrainRenderQueueRef.current.size > 0) {
          pumpTerrainRenderQueue();
        }
      }
    })();
  }

  function pumpNexradQueryQueue() {
    if (nexradQueryPumpActiveRef.current) {
      return;
    }
    nexradQueryPumpActiveRef.current = true;
    void (async () => {
      try {
        while (nexradQueryPendingRef.current) {
          nexradQueryPendingRef.current = false;
          const request = nexradQueryRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            const query = await request.session.queryNexradOverlay(request.viewport, request.width, request.height);
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            const preload = await preloadNexradOverlayImages(query);
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            setNexradOverlay(query);
            setNexradOverlayViewport(request.viewport);
            debugLog("nexrad.overlay.frame.ready", {
              status: query.status,
              tiles: query.tiles.length,
              loaded_images: preload.loaded,
              failed_images: preload.failed,
              elapsed_ms: Math.round(performance.now() - startedAt),
            });
            if (query.status.state !== "ready") {
              debugLog("nexrad.overlay.unavailable", { status: query.status });
            } else if (request.debugTileLabels) {
              debugLog("nexrad.overlay.mesh", query.stats);
            }
          } catch (error: unknown) {
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            setNexradOverlay({
              status: { state: "unavailable", reason: errorMessage(error) },
              tiles: [],
              stats: emptyNexradOverlayStats(),
            });
            setNexradOverlayViewport(null);
          }
        }
      } finally {
        nexradQueryPumpActiveRef.current = false;
        if (nexradQueryPendingRef.current) {
          pumpNexradQueryQueue();
        }
      }
    })();
  }

  function pumpMapOverlayQueryQueue() {
    if (mapOverlayQueryPumpActiveRef.current) {
      return;
    }
    mapOverlayQueryPumpActiveRef.current = true;
    void (async () => {
      try {
        while (mapOverlayQueryPendingRef.current) {
          mapOverlayQueryPendingRef.current = false;
          const request = mapOverlayQueryRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            debugLog("map.overlay.query.start", {
              zoom: request.viewport.zoom,
              center: request.center,
              width: request.width,
              height: request.height,
              queue_wait_ms: Math.round(startedAt - request.requestedAt),
            });
            const overlay = await request.session.queryMapOverlay(
              request.viewport,
              request.width,
              request.height,
            );
            const superseded = mapOverlayQueryRequestRef.current?.id !== request.id;
            if (superseded && !supersededMapOverlayCanLand(request)) {
              debugLog("map.overlay.query.stale_result", {
                request_id: request.id,
                current_request_id: mapOverlayQueryRequestRef.current?.id ?? null,
                newer_pending: mapOverlayQueryPendingRef.current,
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
              });
              continue;
            }
            landMapOverlayQuery(request, overlay, startedAt, superseded);
          } catch (error) {
            if (mapOverlayQueryRequestRef.current?.id !== request.id) {
              debugLog("map.overlay.query.stale_error", {
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              });
              continue;
            }
            if (isInvalidUiSessionHandleError(error)) {
              debugLog("map.overlay.query.stale_session", {
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              });
              continue;
            }
            debugLog("map.overlay.query.error", {
              zoom: request.viewport.zoom,
              elapsed_ms: Math.round(performance.now() - startedAt),
              error: errorMessage(error),
            });
            setMapOverlay({
              visible_features: [],
              visible_metars: [],
              visible_pireps: [],
              airspace_paths: [],
              tfr_paths: [],
              airspace_labels: [],
              offline_regions: [],
            });
            setMapOverlayViewport(null);
            console.error(error);
          }
        }
      } finally {
        mapOverlayQueryPumpActiveRef.current = false;
        if (mapOverlayQueryPendingRef.current) {
          pumpMapOverlayQueryQueue();
        }
      }
    })();
  }

  function supersededMapOverlayCanLand(request: NonNullable<typeof mapOverlayQueryRequestRef.current>) {
    const current = mapOverlayQueryRequestRef.current;
    return (
      current !== null
      && !mapOverlayQueryPendingRef.current
      && request.id > landedMapOverlayQueryRequestIdRef.current
      && current.session === request.session
      && current.width === request.width
      && current.height === request.height
      && current.layerKey === request.layerKey
    );
  }

  function landMapOverlayQuery(
    request: NonNullable<typeof mapOverlayQueryRequestRef.current>,
    overlay: MapOverlayQueryResult,
    startedAt: number,
    superseded: boolean,
  ) {
    const landStartedAt = performance.now();
    landedMapOverlayQueryRequestIdRef.current = request.id;
    setMapOverlay(overlay);
    const overlayStateQueuedAt = performance.now();
    setMapOverlayViewport(request.viewport);
    const landEndedAt = performance.now();
    const overlayCounts = {
      visible_features: overlay.visible_features.length,
      visible_metars: overlay.visible_metars.length,
      visible_pireps: overlay.visible_pireps.length,
      airspace_paths: overlay.airspace_paths.length,
      tfr_paths: overlay.tfr_paths.length,
      airspace_labels: overlay.airspace_labels.length,
      offline_regions: overlay.offline_regions.length,
      flight_plan_features: overlay.flight_plan_features?.length ?? 0,
    };
    mapOverlayLandingTimingRef.current = {
      id: request.id,
      requestedAt: request.requestedAt,
      queryStartedAt: startedAt,
      landStartedAt,
      landEndedAt,
      visibleFeatures: overlayCounts.visible_features,
      visibleMetars: overlayCounts.visible_metars,
      visiblePireps: overlayCounts.visible_pireps,
      airspacePaths: overlayCounts.airspace_paths,
      tfrPaths: overlayCounts.tfr_paths,
      airspaceLabels: overlayCounts.airspace_labels,
      offlineRegions: overlayCounts.offline_regions,
      flightPlanFeatures: overlayCounts.flight_plan_features,
      committed: false,
    };
    debugLog("map.overlay.query.land_steps", {
      id: request.id,
      superseded,
      ...overlayCounts,
      overlay_state_queue_ms: Math.round(overlayStateQueuedAt - landStartedAt),
      viewport_state_queue_ms: Math.round(landEndedAt - overlayStateQueuedAt),
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    });
    debugLog(superseded ? "map.overlay.query.superseded_result" : "map.overlay.query.done", {
      id: request.id,
      zoom: request.viewport.zoom,
      center: request.center,
      elapsed_ms: Math.round(performance.now() - startedAt),
      ...overlayCounts,
    });
    logAfterNextPaint("map.overlay.query.after_paint", startedAt, {
      id: request.id,
      superseded,
      ...overlayCounts,
    });
  }

  useLayoutEffect(() => {
    const timing = mapOverlayLandingTimingRef.current;
    if (!timing || timing.committed) {
      return;
    }
    timing.committed = true;
    const committedAt = performance.now();
    debugLog("map.overlay.query.commit", {
      id: timing.id,
      visible_features: timing.visibleFeatures,
      visible_metars: timing.visibleMetars,
      visible_pireps: timing.visiblePireps,
      airspace_paths: timing.airspacePaths,
      tfr_paths: timing.tfrPaths,
      airspace_labels: timing.airspaceLabels,
      offline_regions: timing.offlineRegions,
      flight_plan_features: timing.flightPlanFeatures,
      land_sync_ms: Math.round(timing.landEndedAt - timing.landStartedAt),
      land_to_commit_ms: Math.round(committedAt - timing.landStartedAt),
      request_to_commit_ms: Math.round(committedAt - timing.queryStartedAt),
      queue_to_commit_ms: Math.round(committedAt - timing.requestedAt),
    });
  }, [mapOverlay, mapOverlayViewport]);

  useLayoutEffect(() => {
    committedViewportRef.current = viewport;
    if (activePointersRef.current.size === 0 && !pendingReactViewportRef.current) {
      viewportRef.current = viewport;
    }
    applyImperativeMapContentTransform();
  }, [viewport]);

  useEffect(() => () => {
    clearPendingReactViewportCommit();
  }, []);

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
  useEffect(() => {
    const prefix = chartSearch.query.trim().toUpperCase();
    if (!chartSearch.open || prefix.length === 0) {
      setChartSearch((current) => ({ ...current, loading: false, error: null, suggestions: [] }));
      return;
    }
    let cancelled = false;
    setChartSearch((current) => ({ ...current, loading: true, error: null }));
    props.appCoreAdapter
      .suggestWaypointIdentifiersNear(center, prefix, 8)
      .then((suggestions) => {
        if (!cancelled) {
          setChartSearch((current) => ({ ...current, loading: false, error: null, suggestions }));
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setChartSearch((current) => ({
            ...current,
            loading: false,
            error: `Search failed: ${errorMessage(error)}`,
            suggestions: [],
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [center, chartSearch.open, chartSearch.query, props.appCoreAdapter]);
  const [tiles, setTiles] = useState<RasterRenderTile[]>([]);
  const [rasterTileViewport, setRasterTileViewport] = useState<MapViewportState | null>(null);
  const [failedRasterTileKeys, setFailedRasterTileKeys] = useState<Set<string>>(() => new Set());
  const loadedRasterTileKeysRef = useRef<Set<string>>(new Set());
  const completedPageTilePaintTimingIdsRef = useRef<Set<number>>(new Set());
  const rasterTilePlanRequestRef = useRef<{
    id: number;
    key: string;
    requestedAt: number;
    session: UiSession;
    viewport: MapViewportState;
    width: number;
    height: number;
    devicePixelRatio: number;
    selectedMapId: string;
    pageTilePaintTiming: WebPageTilePaintTiming | null;
  } | null>(null);
  const rasterTilePlanRequestIdRef = useRef(0);
  const rasterTilePlanRequestKeyRef = useRef<string | null>(null);
  const rasterTilePlanPendingRef = useRef(false);
  const rasterTilePlanPumpActiveRef = useRef(false);
  const landedRasterTilePlanRequestIdRef = useRef(0);
  const rasterTileImageLoadStartedAtRef = useRef<number | null>(null);
  const knownLoadedRasterTileKeysRef = useRef<Set<string>>(new Set());
  const rasterTilePlanLandingTimingRef = useRef<{
    id: number;
    key: string;
    requestedAt: number;
    queryStartedAt: number;
    landStartedAt: number;
    landEndedAt: number;
    tiles: number;
    previousTiles: number;
    sameTileKeys: boolean;
    sameViewport: boolean;
    committed: boolean;
  } | null>(null);
  const rasterTileKey = useCallback((tile: RasterRenderTile) =>
    `${tile.chartFamily}-${tile.packageName ?? tile.mapViewId}-${tile.drawKey}`,
  []);
  const rasterTilePlanKey = useCallback((
    nextViewport: MapViewportState,
    width: number,
    height: number,
    devicePixelRatio: number,
    selectedMapId: string,
  ) => [
    selectedMapId,
    width,
    height,
    devicePixelRatio,
    nextViewport.zoom.toFixed(6),
    nextViewport.centerWorldX.toFixed(3),
    nextViewport.centerWorldY.toFixed(3),
  ].join("|"), []);
  const reportRasterTilesReadyIfComplete = useCallback((tileList: RasterRenderTile[]) => {
    if (page !== "map" || tileList.length === 0) {
      return false;
    }
    const loadedKeys = loadedRasterTileKeysRef.current;
    const allLoaded = tileList.every((entry) => loadedKeys.has(rasterTileKey(entry)));
    if (!allLoaded) {
      return false;
    }
    const imageLoadStartedAt = rasterTileImageLoadStartedAtRef.current;
    if (imageLoadStartedAt !== null) {
      requestAnimationFrame(() => {
        debugLog("map.raster.images.done", {
          elapsed_ms: Math.round(performance.now() - imageLoadStartedAt),
          tiles: tileList.length,
        });
      });
      rasterTileImageLoadStartedAtRef.current = null;
    }
    return true;
  }, [page, rasterTileKey]);
  const completePageTilePaintTiming = useCallback((timing: WebPageTilePaintTiming, phase: "frame" | "images") => {
    if (completedPageTilePaintTimingIdsRef.current.has(timing.id)) {
      return;
    }
    requestAnimationFrame(() => {
      completedPageTilePaintTimingIdsRef.current.add(timing.id);
      debugLog("web.page-to-map.frame", {
        id: timing.id,
        from_page: timing.fromPage,
        phase,
        elapsed_ms: Math.round(performance.now() - timing.startedAt),
        tiles: tiles.length,
      });
      onPageTilePaintTimingComplete(timing.id);
    });
  }, [onPageTilePaintTimingComplete, tiles.length]);

  function supersededRasterPlanCanLand(request: NonNullable<typeof rasterTilePlanRequestRef.current>) {
    const current = rasterTilePlanRequestRef.current;
    return (
      current !== null
      && request.id > landedRasterTilePlanRequestIdRef.current
      && current.session === request.session
      && current.selectedMapId === request.selectedMapId
      && current.width === request.width
      && current.height === request.height
      && current.devicePixelRatio === request.devicePixelRatio
    );
  }

  function landRasterTilePlan(
    request: NonNullable<typeof rasterTilePlanRequestRef.current>,
    nextTiles: RasterRenderTile[],
    superseded: boolean,
    startedAt: number,
  ) {
    const landStartedAt = performance.now();
    landedRasterTilePlanRequestIdRef.current = request.id;
    const nextTileKeys = nextTiles.map(rasterTileKey);
    const previousTileKeys = tiles.map(rasterTileKey);
    const sameTileKeys = nextTileKeys.length === previousTileKeys.length
      && nextTileKeys.every((key, index) => key === previousTileKeys[index]);
    const sameViewport = rasterTileViewport !== null
      && rasterTileViewport.zoom === request.viewport.zoom
      && rasterTileViewport.centerWorldX === request.viewport.centerWorldX
      && rasterTileViewport.centerWorldY === request.viewport.centerWorldY;
    const keyingEndedAt = performance.now();
    loadedRasterTileKeysRef.current = new Set(
      nextTileKeys.filter((key) => knownLoadedRasterTileKeysRef.current.has(key)),
    );
    const loadedKeyResetEndedAt = performance.now();
    rasterTileImageLoadStartedAtRef.current = performance.now();
    const imageTimingResetEndedAt = performance.now();
    setFailedRasterTileKeys(new Set());
    const failedStateQueuedAt = performance.now();
    setTiles(nextTiles);
    const tilesStateQueuedAt = performance.now();
    setRasterTileViewport(request.viewport);
    const landEndedAt = performance.now();
    rasterTilePlanLandingTimingRef.current = {
      id: request.id,
      key: request.key,
      requestedAt: request.requestedAt,
      queryStartedAt: startedAt,
      landStartedAt,
      landEndedAt,
      tiles: nextTiles.length,
      previousTiles: tiles.length,
      sameTileKeys,
      sameViewport,
      committed: false,
    };
    debugLog("map.raster.plan.landed", {
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    });
    debugLog("map.raster.plan.land_steps", {
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
      keying_ms: Math.round(keyingEndedAt - landStartedAt),
      loaded_key_reset_ms: Math.round(loadedKeyResetEndedAt - keyingEndedAt),
      image_timing_reset_ms: Math.round(imageTimingResetEndedAt - loadedKeyResetEndedAt),
      failed_state_queue_ms: Math.round(failedStateQueuedAt - imageTimingResetEndedAt),
      tiles_state_queue_ms: Math.round(tilesStateQueuedAt - failedStateQueuedAt),
      viewport_state_queue_ms: Math.round(landEndedAt - tilesStateQueuedAt),
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    });
    logAfterNextPaint("map.raster.plan.after_paint", startedAt, {
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
    });
    reportRasterTilesReadyIfComplete(nextTiles);
  }

  useLayoutEffect(() => {
    const timing = rasterTilePlanLandingTimingRef.current;
    if (!timing || timing.committed) {
      return;
    }
    timing.committed = true;
    const committedAt = performance.now();
    debugLog("map.raster.plan.commit", {
      id: timing.id,
      key: timing.key,
      tiles: timing.tiles,
      previous_tiles: timing.previousTiles,
      same_tile_keys: timing.sameTileKeys,
      same_viewport: timing.sameViewport,
      land_sync_ms: Math.round(timing.landEndedAt - timing.landStartedAt),
      land_to_commit_ms: Math.round(committedAt - timing.landStartedAt),
      request_to_commit_ms: Math.round(committedAt - timing.queryStartedAt),
      queue_to_commit_ms: Math.round(committedAt - timing.requestedAt),
    });
  }, [rasterTileViewport, tiles]);

  function pumpRasterTilePlanQueue() {
    if (rasterTilePlanPumpActiveRef.current) {
      return;
    }
    rasterTilePlanPumpActiveRef.current = true;
    void (async () => {
      try {
        while (rasterTilePlanPendingRef.current) {
          rasterTilePlanPendingRef.current = false;
          const request = rasterTilePlanRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            debugLog("map.raster.plan.start", {
              id: request.id,
              key: request.key,
              selected_map_id: request.selectedMapId,
              zoom: request.viewport.zoom,
              center_world_x: request.viewport.centerWorldX,
              center_world_y: request.viewport.centerWorldY,
              width: request.width,
              height: request.height,
              device_pixel_ratio: request.devicePixelRatio,
              queue_wait_ms: Math.round(startedAt - request.requestedAt),
            });
            const plan = await request.session.queryRasterTilePlan(
              request.viewport,
              request.width,
              request.height,
              request.devicePixelRatio,
            );
            const planSuperseded = rasterTilePlanRequestRef.current?.id !== request.id;
            if (planSuperseded && !supersededRasterPlanCanLand(request)) {
              debugLog("map.raster.plan.stale_result", {
                id: request.id,
                elapsed_ms: Math.round(performance.now() - startedAt),
                tiles: plan.tiles.length,
              });
              continue;
            }
            if (planSuperseded) {
              debugLog("map.raster.plan.superseded_result", {
                id: request.id,
                latest_id: rasterTilePlanRequestRef.current?.id ?? null,
                elapsed_ms: Math.round(performance.now() - startedAt),
                tiles: plan.tiles.length,
              });
            }
            if (!planSuperseded && request.pageTilePaintTiming) {
              debugLog("web.page-to-map.plan", {
              id: request.pageTilePaintTiming.id,
              from_page: request.pageTilePaintTiming.fromPage,
              elapsed_ms: Math.round(performance.now() - request.pageTilePaintTiming.startedAt),
              tiles: plan.tiles.length,
              device_pixel_ratio: request.devicePixelRatio,
              });
            }
            debugLog("map.raster.plan.done", {
              id: request.id,
              key: request.key,
              superseded: planSuperseded,
              elapsed_ms: Math.round(performance.now() - startedAt),
              tiles: plan.tiles.length,
            });
            const resolveStartedAt = performance.now();
            const nextTiles = plan.tiles.map((tile) =>
              renderTileFromCore(tile, 1 / request.devicePixelRatio),
            );
            const urlsSuperseded = rasterTilePlanRequestRef.current?.id !== request.id;
            if (urlsSuperseded && !supersededRasterPlanCanLand(request)) {
              debugLog("map.raster.tiles.stale_result", {
                id: request.id,
                elapsed_ms: Math.round(performance.now() - resolveStartedAt),
                tiles: nextTiles.length,
              });
              continue;
            }
            if (urlsSuperseded && !planSuperseded) {
              debugLog("map.raster.tiles.superseded_result", {
                id: request.id,
                latest_id: rasterTilePlanRequestRef.current?.id ?? null,
                elapsed_ms: Math.round(performance.now() - resolveStartedAt),
                tiles: nextTiles.length,
              });
            }
            debugLog("map.raster.tiles.done", {
              id: request.id,
              key: request.key,
              superseded: urlsSuperseded,
              elapsed_ms: Math.round(performance.now() - resolveStartedAt),
              tiles: nextTiles.length,
            });
            landRasterTilePlan(request, nextTiles, urlsSuperseded, startedAt);
          } catch (error) {
            if (rasterTilePlanRequestRef.current?.id !== request.id) {
              debugLog("map.raster.plan.stale_error", {
                id: request.id,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              });
              continue;
            }
            debugLog("map.raster.plan.error", {
              id: request.id,
              elapsed_ms: Math.round(performance.now() - startedAt),
              error: errorMessage(error),
            });
            console.error("failed to query raster tile plan", error);
            setTiles([]);
            setRasterTileViewport(null);
          }
        }
      } finally {
        rasterTilePlanPumpActiveRef.current = false;
        if (rasterTilePlanPendingRef.current) {
          pumpRasterTilePlanQueue();
        }
      }
    })();
  }

  useEffect(() => {
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      rasterTilePlanRequestRef.current = null;
      rasterTilePlanPendingRef.current = false;
      setTiles([]);
      setRasterTileViewport(null);
      return;
    }
    const devicePixelRatio = window.devicePixelRatio || 1;
    const key = rasterTilePlanKey(
      viewport,
      surfaceSize.width,
      surfaceSize.height,
      devicePixelRatio,
      selectedMap.selected_map_id,
    );
    if (rasterTilePlanRequestKeyRef.current === key) {
      return;
    }
    rasterTilePlanRequestKeyRef.current = key;
    rasterTilePlanRequestRef.current = {
      id: ++rasterTilePlanRequestIdRef.current,
      key,
      requestedAt: performance.now(),
      session: uiSession,
      viewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
      devicePixelRatio,
      selectedMapId: selectedMap.selected_map_id,
      pageTilePaintTiming,
    };
    rasterTilePlanPendingRef.current = true;
    pumpRasterTilePlanQueue();
  }, [rasterTilePlanKey, selectedMap.selected_map_id, surfaceSize.height, surfaceSize.width, uiSession, viewport]);
  useEffect(() => {
    if (page !== "map" || !pageTilePaintTiming || tiles.length === 0) {
      return;
    }
    const loadedKeys = loadedRasterTileKeysRef.current;
    const tileKeys = tiles.map(rasterTileKey);
    const allAlreadyLoaded = tileKeys.length > 0 && tileKeys.every((key) => loadedKeys.has(key));
    if (allAlreadyLoaded) {
      completePageTilePaintTiming(pageTilePaintTiming, "frame");
    }
  }, [completePageTilePaintTiming, page, pageTilePaintTiming, rasterTileKey, tiles]);
  const mapIsVisible = page === "map";
  const [situationRingCandidates, setSituationRingCandidates] = useState<SituationRingCandidate[]>([]);
  useEffect(() => {
    let cancelled = false;
    setSituationRingCandidates(appCoreAdapter.situationRingCandidates());
    void appCoreAdapter.loadSituationRingCandidates().then((candidates) => {
      if (!cancelled) {
        setSituationRingCandidates(candidates);
      }
    }).catch((error) => {
      debugLog("situation_ring_candidates.load.failed", {
        message: error instanceof Error ? error.message : String(error),
      });
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter]);
  const situationOverlay = useMemo(
    () => resolveSituationOverlay(ownship, viewport, surfaceSize.width, surfaceSize.height, situationRingCandidates),
    [ownship, viewport, surfaceSize.height, surfaceSize.width, situationRingCandidates],
  );
  const mapOverlayOwnshipKey = [
    ownship.position?.lat.toFixed(6) ?? "none",
    ownship.position?.lon.toFixed(6) ?? "none",
    ownship.altitude_msl_ft?.toFixed(0) ?? ownship.pressure_altitude_ft?.toFixed(0) ?? "none",
    ownship.orientation_deg?.toFixed(0) ?? "none",
    ownship.speed_kt?.toFixed(0) ?? "none",
  ].join(":");
  const mapOverlayLayersVisible =
    mapLayerState.vectors.visible
    || mapLayerState.metars.visible
    || mapLayerState.offline_regions.visible;
  const setMapLayerVisible = useCallback(async (layerId: MapLayerId, visible: boolean) => {
    if (!uiSession || layerToggleBusyId !== null) {
      return;
    }
    setLayerToggleBusyId(layerId);
    try {
      const nextSnapshot = await uiSession.setMapLayerVisibility(layerId, visible);
      onPlaybackSnapshotChange(nextSnapshot);
      await new Promise((resolve) => window.setTimeout(resolve, 300));
      trayGroup.close("layers");
    } finally {
      setLayerToggleBusyId((current) => (current === layerId ? null : current));
    }
  }, [layerToggleBusyId, onPlaybackSnapshotChange, trayGroup, uiSession]);
  const routeScreenSegments = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return flightPlanRoute.map((segment) => ({
      ...segment,
      path: (segment.path.length > 0 ? segment.path : [segment.from, segment.to])
        .map((point) => worldToScreen(viewport, latLonToWorld(point.lat, point.lon), surfaceSize.width, surfaceSize.height)),
      finishLinePaths: (segment.finish_lines ?? []).map((line) =>
        [line.start, line.end].map((point) =>
          worldToScreen(viewport, latLonToWorld(point.lat, point.lon), surfaceSize.width, surfaceSize.height),
        ),
      ),
    }));
  }, [flightPlanRoute, surfaceSize.height, surfaceSize.width, viewport]);

  useEffect(() => {
    terrainRendererRef.current = new TerrainOverlayRenderer();
    const cache = terrainTileCacheRef.current;
    return () => {
      terrainRendererRef.current?.destroy();
      terrainRendererRef.current = null;
      cache.clear();
      terrainPendingFrameRef.current = null;
      terrainFrameStartRef.current.clear();
      terrainTileInFlightRef.current.clear();
      terrainRenderQueueRef.current.clear();
      terrainRenderGenerationRef.current += 1;
    };
  }, []);

  const terrainAltitudeBucket = terrainAltitudeBucketForOwnship(ownship);
  terrainCurrentBucketRef.current = terrainAltitudeBucket;

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      terrainCurrentBucketRef.current = null;
      terrainPendingFrameRef.current = null;
      terrainRenderQueueRef.current.clear();
      terrainRenderGenerationRef.current += 1;
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    if (!mapLayerState.terrain_warning.visible) {
      terrainCurrentBucketRef.current = null;
      terrainPendingFrameRef.current = null;
      terrainRenderQueueRef.current.clear();
      terrainRenderGenerationRef.current += 1;
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    if (terrainAltitudeBucket == null) {
      terrainPendingFrameRef.current = null;
      terrainRenderQueueRef.current.clear();
      terrainRenderGenerationRef.current += 1;
      setTerrainOverlay((current) => current.images.length === 0 ? current : { query: current.query, images: [] });
      return;
    }
    const session = uiSession;
    let cancelled = false;

    async function syncTerrainOverlay() {
      const query = await session.queryTerrainOverlay(
        viewport,
        surfaceSize.width,
        surfaceSize.height,
      );
      if (cancelled) {
        return;
      }
      if (query.status.state !== "ready") {
        debugLog("terrain.overlay.unavailable", {
          status: query.status,
          request_count: query.tile_requests.length,
          zoom: viewport.zoom,
        });
        terrainPendingFrameRef.current = null;
        terrainRenderGenerationRef.current += 1;
        setTerrainOverlay({ query, images: [] });
        return;
      }
      if (terrainAltitudeBucket == null) {
        debugLog("terrain.overlay.no_altitude_bucket", {
          status: query.status,
          request_count: query.tile_requests.length,
          zoom: viewport.zoom,
        });
        terrainPendingFrameRef.current = null;
        terrainRenderQueueRef.current.clear();
        terrainRenderGenerationRef.current += 1;
        setTerrainOverlay({ query, images: [] });
        return;
      }
      const generation = terrainRenderGenerationRef.current + 1;
      terrainRenderGenerationRef.current = generation;
      terrainPendingFrameRef.current = { query, altitudeBucket: terrainAltitudeBucket };
      const cache = terrainTileCacheRef.current;
      const inFlight = terrainTileInFlightRef.current;
      const readyImages = terrainImagesForCompleteQuery(cache, query, terrainAltitudeBucket);
      if (readyImages) {
        setTerrainOverlay({ query, images: readyImages });
      }
      const missingRequests = query.tile_requests.filter((request) => {
        const key = terrainCacheKey(request, terrainAltitudeBucket);
        return !cache.has(key) && !inFlight.has(key) && !terrainRenderQueueRef.current.has(key);
      }).sort((left, right) =>
        terrainRequestSortDistance(left, ownship, viewport, surfaceSize.width, surfaceSize.height)
          - terrainRequestSortDistance(right, ownship, viewport, surfaceSize.width, surfaceSize.height),
      );
      if (missingRequests.length === 0) {
        if (!readyImages) {
          setTerrainOverlay((current) => ({ query, images: current.images }));
        }
        return;
      }
      terrainRenderQueueRef.current.clear();
      for (const request of missingRequests) {
        const key = terrainCacheKey(request, terrainAltitudeBucket);
        if (!cache.has(key) && !inFlight.has(key)) {
          terrainRenderQueueRef.current.set(key, { request, altitudeBucket: terrainAltitudeBucket, generation });
        }
      }
      const renderPlanKey = `${terrainAltitudeBucket}:${readyImages?.length ?? 0}:${missingRequests.length}:${missingRequests.map((request) => request.key).join("|")}`;
      if (renderPlanKey !== lastTerrainRenderPlanKeyRef.current) {
        lastTerrainRenderPlanKeyRef.current = renderPlanKey;
        const frameKey = terrainFrameKey(query, terrainAltitudeBucket);
        if (!terrainFrameStartRef.current.has(frameKey)) {
          terrainFrameStartRef.current.set(frameKey, performance.now());
          pruneTerrainFrameStarts(terrainFrameStartRef.current);
        }
        const requestSummary = terrainRequestSummary(query.tile_requests);
        const missingSummary = terrainRequestSummary(missingRequests);
        debugLog("terrain.overlay.render.plan", {
          request_count: query.tile_requests.length,
          cached_count: readyImages?.length ?? 0,
          missing_count: missingRequests.length,
          altitude_bucket: terrainAltitudeBucket,
          request_zooms: requestSummary.zooms,
          missing_zooms: missingSummary.zooms,
        });
      }
      pumpTerrainRenderQueue();
    }

    syncTerrainOverlay().catch((error: unknown) => {
      if ((error as { name?: string } | null)?.name === "AbortError") {
        return;
      }
      console.warn("terrain overlay unavailable", error);
      if (!cancelled) {
        setTerrainOverlay({
          query: {
            status: { state: "no_altitude" },
            tile_requests: [],
          },
          images: [],
        });
      }
    });

    return () => {
      cancelled = true;
    };
  }, [mapIsVisible, mapLayerState.terrain_warning.visible, surfaceSize.height, surfaceSize.width, terrainAltitudeBucket, uiInvalidationRevisions.terrain_overlay, uiSession, viewport]);

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0 || !mapLayerState.nexrad.visible) {
      nexradQueryRequestRef.current = null;
      nexradQueryPendingRef.current = false;
      setNexradOverlay({ status: { state: "hidden" }, tiles: [], stats: emptyNexradOverlayStats() });
      setNexradOverlayViewport(null);
      return;
    }
    nexradQueryRequestRef.current = {
      id: ++nexradQueryRequestIdRef.current,
      session: uiSession,
      viewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
      debugTileLabels: debugState.nexrad_tile_labels,
    };
    nexradQueryPendingRef.current = true;
    pumpNexradQueryQueue();
  }, [debugState.nexrad_tile_labels, mapIsVisible, mapLayerState.nexrad.visible, surfaceSize.height, surfaceSize.width, uiInvalidationRevisions.nexrad_overlay, uiSession, viewport]);

  useEffect(() => {
    if (typeof PerformanceObserver === "undefined") {
      return;
    }
    const observer = new PerformanceObserver((list) => {
      const samples: Array<{ atMs: number; transferBytes: number; encodedBytes: number; decodedBytes: number }> = [];
      for (const entry of list.getEntries()) {
        if (!(entry instanceof PerformanceResourceTiming)) {
          continue;
        }
        if (!entry.name.includes("/live-feeds/states/nexrad/") || !entry.name.endsWith(".png")) {
          continue;
        }
        const transferBytes = entry.transferSize || 0;
        const encodedBytes = entry.encodedBodySize || 0;
        const decodedBytes = entry.decodedBodySize || 0;
        if (transferBytes <= 0 && encodedBytes <= 0 && decodedBytes <= 0) {
          continue;
        }
        samples.push({ atMs: Date.now(), transferBytes, encodedBytes, decodedBytes });
      }
      if (samples.length === 0) {
        return;
      }
      setNexradTransferSamples((current) => {
        const cutoff = Date.now() - 60_000;
        return [...current, ...samples].filter((sample) => sample.atMs >= cutoff);
      });
    });
    observer.observe({ type: "resource", buffered: true });
    return () => observer.disconnect();
  }, []);

  const nexradDebugLines = useMemo(() => {
    const lines: string[] = [];
    const observedAt = nexradOverlay.stats.observed_at_utc;
    if (observedAt) {
      const observedAtMs = Date.parse(observedAt);
      if (Number.isFinite(observedAtMs)) {
        lines.push(`NEXRAD obs: ${new Date(observedAtMs).toISOString().slice(11, 19)}Z`);
        lines.push(`NEXRAD age: ${formatUptimeMs(Date.now() - observedAtMs)}`);
      } else {
        lines.push(`NEXRAD obs: ${observedAt}`);
        lines.push("NEXRAD age: n/a");
      }
    } else {
      lines.push("NEXRAD obs: n/a");
      lines.push("NEXRAD age: n/a");
    }
    const cutoff = Date.now() - 60_000;
    const recentBytes = nexradTransferSamples
      .filter((sample) => sample.atMs >= cutoff)
      .reduce(
        (sum, sample) => ({
          transferBytes: sum.transferBytes + sample.transferBytes,
          encodedBytes: sum.encodedBytes + sample.encodedBytes,
          decodedBytes: sum.decodedBytes + sample.decodedBytes,
        }),
        { transferBytes: 0, encodedBytes: 0, decodedBytes: 0 },
      );
    lines.push(`NEXRAD net: ${formatMegabytesPerSecond(recentBytes.transferBytes / 60)}`);
    lines.push(`NEXRAD encoded: ${formatMegabytesPerSecond(recentBytes.encodedBytes / 60)}`);
    lines.push(`NEXRAD decoded: ${formatMegabytesPerSecond(recentBytes.decodedBytes / 60)}`);
    return lines;
  }, [nexradOverlay.stats.observed_at_utc, nexradTransferSamples, uptimeLabel]);

  useEffect(() => {
    debugLog("map.nav_element.render", {
      app_state_active_plan: plan?.id ?? null,
      plan_guidance: planUiState?.guidance?.nav_element ?? null,
      ownship_mode: ownship.mode,
      ownship_draw_cdi: ownship.draw_cdi,
      ownship_position: ownship.position,
    });
  }, [ownship.draw_cdi, ownship.mode, ownship.position, plan, planUiState]);

  useEffect(() => {
    if (!uiSession) {
      setFlightPlanRoute([]);
      return;
    }
    const session = uiSession;
    let cancelled = false;

    async function resolveFlightPlanRoute() {
      const startedAt = performance.now();
      const segments = await session.projectFlightPlanRoute();
      const elapsedMs = Math.round(performance.now() - startedAt);
      debugLog("map.route.segments", {
        count: segments.length,
        elapsed_ms: elapsedMs,
        segments: segments.map((segment) => ({
          id: segment.id,
          from: segment.from,
          to: segment.to,
          status: segment.status,
        })),
      });
      if (elapsedMs > 250) {
        onHighLatencyWarning("map.route.resolve.slow", {
          count: segments.length,
          elapsed_ms: elapsedMs,
        });
      }
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
  }, [onHighLatencyWarning, plan.id, plan.version, plan.guidance, plan.resolved_legs, uiInvalidationRevisions.flight_plan_route, uiSession]);

  useEffect(() => {
    if (!mapIsVisible || !mapOverlayLayersVisible) {
      mapOverlayQueryRequestRef.current = null;
      mapOverlayQueryPendingRef.current = false;
      setMapOverlay({
        visible_features: [],
        visible_metars: [],
        visible_pireps: [],
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        offline_regions: [],
      });
      return;
    }
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      mapOverlayQueryRequestRef.current = null;
      mapOverlayQueryPendingRef.current = false;
      setMapOverlay({
        visible_features: [],
        visible_metars: [],
        visible_pireps: [],
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        offline_regions: [],
      });
      return;
    }
    mapOverlayQueryRequestRef.current = {
      id: ++mapOverlayQueryRequestIdRef.current,
      requestedAt: performance.now(),
      session: uiSession,
      viewport,
      center,
      width: surfaceSize.width,
      height: surfaceSize.height,
      layerKey: [
        mapOverlayLayersVisible,
        mapLayerState.metars.visible,
        mapLayerState.offline_regions.visible,
        mapLayerState.vectors.visible,
      ].join("|"),
    };
    mapOverlayQueryPendingRef.current = true;
    pumpMapOverlayQueryQueue();
  }, [
    mapLayerState.metars.visible,
    mapLayerState.offline_regions.visible,
    mapLayerState.vectors.visible,
    mapOverlayLayersVisible,
    mapIsVisible,
    mapOverlayOwnshipKey,
    onDebugWarning,
    surfaceSize.height,
    surfaceSize.width,
    uiInvalidationRevisions.map_overlay,
    uiSession,
    viewport,
  ]);

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
  const selectedMapHighlight = useMemo(() => {
    const highlight = mapSelection?.selectedItem?.highlight;
    if (!highlight || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    if (highlight.kind === "spot") {
      const world = latLonToWorld(highlight.lat, highlight.lon);
      return {
        kind: "spot" as const,
        point: worldToScreen(viewport, world, surfaceSize.width, surfaceSize.height),
      };
    }
    if (highlight.kind === "metar") {
      const metarFeature = mapOverlay.visible_metars.find((feature) => feature.station_id === highlight.station_id);
      if (metarFeature) {
        return { kind: "metar" as const, feature: metarFeature };
      }
      const selectedMetarFeature = mapSelection?.selectedItem?.metar_feature;
      if (selectedMetarFeature?.station_id === highlight.station_id) {
        return { kind: "metar" as const, feature: selectedMetarFeature };
      }
      return null;
    }
    if (highlight.kind === "pirep") {
      const pirepFeature = mapOverlay.visible_pireps.find((feature) => feature.id === highlight.id);
      if (pirepFeature) {
        return { kind: "pirep" as const, feature: pirepFeature };
      }
      const selectedPirepFeature = mapSelection?.selectedItem?.pirep_feature;
      if (selectedPirepFeature?.id === highlight.id) {
        return { kind: "pirep" as const, feature: selectedPirepFeature };
      }
      return null;
    }
    if (highlight.kind === "offline_region") {
      const region = mapOverlay.offline_regions.find((feature) => feature.id === highlight.id);
      if (region) {
        return { kind: "offline_region" as const, feature: region };
      }
      return null;
    }
    const pointFeature = mapOverlay.visible_features.find((feature) => feature.id === highlight.id);
    if (pointFeature) {
      return { kind: "point" as const, feature: pointFeature };
    }
    const airspacePath = mapOverlay.airspace_paths.find((feature) => feature.id === highlight.id);
    if (airspacePath) {
      return { kind: "path" as const, feature: airspacePath };
    }
    const tfrPath = mapOverlay.tfr_paths.find((feature) => feature.id === highlight.id);
    if (tfrPath) {
      return { kind: "path" as const, feature: tfrPath };
    }
    return null;
  }, [mapOverlay.airspace_paths, mapOverlay.offline_regions, mapOverlay.tfr_paths, mapOverlay.visible_features, mapOverlay.visible_metars, mapOverlay.visible_pireps, mapSelection?.selectedItem, surfaceSize.height, surfaceSize.width, viewport]);
  const rasterTileTransform = useMemo(() => {
    if (!rasterTileViewport) {
      return undefined;
    }
    const currentScale = scaleForZoom(viewport.zoom);
    const tileScale = scaleForZoom(rasterTileViewport.zoom);
    const scaleRatio = currentScale / tileScale;
    const dx = (rasterTileViewport.centerWorldX - viewport.centerWorldX) * currentScale;
    const dy = (rasterTileViewport.centerWorldY - viewport.centerWorldY) * currentScale;
    return `translate(${dx}px, ${dy}px) scale(${scaleRatio})`;
  }, [rasterTileViewport, viewport]);
  const nexradOverlayTransform = useMemo(() => {
    if (!nexradOverlayViewport) {
      return undefined;
    }
    const currentScale = scaleForZoom(viewport.zoom);
    const overlayScale = scaleForZoom(nexradOverlayViewport.zoom);
    const scaleRatio = currentScale / overlayScale;
    const dx = (nexradOverlayViewport.centerWorldX - viewport.centerWorldX) * currentScale;
    const dy = (nexradOverlayViewport.centerWorldY - viewport.centerWorldY) * currentScale;
    return `translate(${dx}px, ${dy}px) scale(${scaleRatio})`;
  }, [nexradOverlayViewport, viewport]);

  function transientViewportTransform(renderedViewport: MapViewportState, nextViewport: MapViewportState) {
    if (sameMapViewport(renderedViewport, nextViewport)) {
      return "";
    }
    const currentScale = scaleForZoom(nextViewport.zoom);
    const renderedScale = scaleForZoom(renderedViewport.zoom);
    const scaleRatio = currentScale / renderedScale;
    const dx = (renderedViewport.centerWorldX - nextViewport.centerWorldX) * currentScale;
    const dy = (renderedViewport.centerWorldY - nextViewport.centerWorldY) * currentScale;
    return `translate(${dx}px, ${dy}px) scale(${scaleRatio})`;
  }

  function applyImperativeMapContentTransform() {
    const element = mapContentTransformRef.current;
    if (!element) {
      return;
    }
    const transform = transientViewportTransform(committedViewportRef.current, viewportRef.current);
    element.style.transform = transform;
    element.style.transformOrigin = "center center";
  }

  function clearPendingReactViewportCommit() {
    if (pendingReactViewportTimerRef.current !== null) {
      window.clearTimeout(pendingReactViewportTimerRef.current);
      pendingReactViewportTimerRef.current = null;
    }
  }

  function flushPendingReactViewportCommit() {
    clearPendingReactViewportCommit();
    const next = pendingReactViewportRef.current;
    pendingReactViewportRef.current = null;
    if (next && !sameMapViewport(next, committedViewportRef.current)) {
      debugLog("map.viewport.react_commit.flush", {
        zoom: next.zoom,
        center_world_x: next.centerWorldX,
        center_world_y: next.centerWorldY,
      });
      onViewportChange(next);
    }
  }

  function scheduleReactViewportCommit(next: MapViewportState) {
    pendingReactViewportRef.current = next;
    if (pendingReactViewportTimerRef.current !== null) {
      return;
    }
    pendingReactViewportTimerRef.current = window.setTimeout(() => {
      pendingReactViewportTimerRef.current = null;
      const pending = pendingReactViewportRef.current;
      pendingReactViewportRef.current = null;
      if (pending && !sameMapViewport(pending, committedViewportRef.current)) {
        debugLog("map.viewport.react_commit.throttled", {
          zoom: pending.zoom,
          center_world_x: pending.centerWorldX,
          center_world_y: pending.centerWorldY,
        });
        onViewportChange(pending);
      }
    }, dragViewportReactCommitThrottleMs);
  }

  function updateViewport(next: MapViewportState, options: { deferReactCommit?: boolean } = {}) {
    viewportRef.current = next;
    applyImperativeMapContentTransform();
    if (options.deferReactCommit) {
      scheduleReactViewportCommit(next);
      return;
    }
    pendingReactViewportRef.current = null;
    clearPendingReactViewportCommit();
    onViewportChange(next);
  }

  function noteViewportGesture(durationMs = 300) {
    viewportGestureUntilRef.current = Math.max(viewportGestureUntilRef.current, Date.now() + durationMs);
    if (!gestureActiveRef.current) {
      onViewportGestureActivity();
    }
  }

  function setViewportGestureActive(active: boolean) {
    if (!active) {
      flushPendingReactViewportCommit();
    }
    if (gestureActiveRef.current === active) {
      return;
    }
    gestureActiveRef.current = active;
    onViewportGestureActiveChange(active);
  }

  function syncFollowStateForViewport(nextViewport: MapViewportState) {
    if (!uiSession || !mapFollowUiState.following || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    if (gestureActiveRef.current) {
      deferredFollowSyncViewportRef.current = nextViewport;
      debugLog("map.follow.sync.deferred_for_gesture", {
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
      });
      return;
    }
    deferredFollowSyncViewportRef.current = null;
    const serial = followSyncSerialRef.current + 1;
    followSyncSerialRef.current = serial;
    setFollowSyncPendingSerial(serial);
    debugLog("map.follow.sync.request", {
      serial,
      zoom: nextViewport.zoom,
      center_world_x: nextViewport.centerWorldX,
      center_world_y: nextViewport.centerWorldY,
      gesture_active: gestureActiveRef.current,
    });
    void uiSession
      .syncMapFollow(nextViewport, surfaceSize.width, surfaceSize.height)
      .then((nextSnapshot) => {
        if (followSyncSerialRef.current !== serial) {
          debugLog("map.follow.sync.stale_response", { serial, latest_serial: followSyncSerialRef.current });
          return;
        }
        props.onPlaybackSnapshotChange(nextSnapshot);
      })
      .catch(() => {})
      .finally(() => {
        if (followSyncSerialRef.current === serial) {
          setFollowSyncPendingSerial(0);
        }
      });
  }

  function flushDeferredFollowSync() {
    const nextViewport = deferredFollowSyncViewportRef.current;
    if (!nextViewport) {
      return;
    }
    deferredFollowSyncViewportRef.current = null;
    syncFollowStateForViewport(nextViewport);
  }

  const runLiveDragPerf = useCallback(async () => {
    if (!uiSession || liveDragPerfRunning || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const runId = createLiveDragPerfRunId();
    const globalWithRunId = globalThis as typeof globalThis & { __aerobagPerfRunId?: unknown };
    const priorRunId = globalWithRunId.__aerobagPerfRunId;
    const hadPriorRunId = Object.prototype.hasOwnProperty.call(globalWithRunId, "__aerobagPerfRunId");
    const dragCount = 20;
    const stepsPerDrag = 8;
    const stepDelayMs = 12;
    const dragIntervalMs = 500;
    const settleMs = 4000;
    globalWithRunId.__aerobagPerfRunId = runId;
    activePointersRef.current.clear();
    dragRef.current = null;
    pinchRef.current = null;
    clickCandidateRef.current = null;
    setMapSelection(null);
    setLiveDragPerfRunning(true);
    setLastLiveDragPerfRunId(runId);
    let followDisabledForRun = false;
    if (mapFollowUiState.following) {
      try {
        const nextSnapshot = await uiSession.disengageMapFollow(viewportRef.current);
        onPlaybackSnapshotChange(nextSnapshot);
        followDisabledForRun = true;
      } catch (error) {
        debugLog("automated-test-disable-follow.failed", {
          run_id: runId,
          error: errorMessage(error),
        });
      }
    }
    debugLog("automated-test-begin", {
      run_id: runId,
      mode: "live_browser",
      drag_count: dragCount,
      steps_per_drag: stepsPerDrag,
      step_delay_ms: stepDelayMs,
      interval_ms: dragIntervalMs,
      following: mapFollowUiState.following && !followDisabledForRun,
      following_before_disable: mapFollowUiState.following,
      follow_disabled_for_run: followDisabledForRun,
      start_zoom: viewportRef.current.zoom,
      width: surfaceSize.width,
      height: surfaceSize.height,
    });
    try {
      for (let dragIndex = 0; dragIndex < dragCount; dragIndex += 1) {
        const dx = surfaceSize.width * 0.66 / stepsPerDrag;
        const dy = surfaceSize.height * 0.66 / stepsPerDrag;
        setViewportGestureActive(true);
        for (let stepIndex = 0; stepIndex < stepsPerDrag; stepIndex += 1) {
          const nextViewport = dragViewport(viewportRef.current, dx, dy);
          noteViewportGesture();
          debugLog("map.drag.viewport", {
            automated: true,
            run_id: runId,
            drag_index: dragIndex,
            step_index: stepIndex,
            dx,
            dy,
            zoom: nextViewport.zoom,
            center_world_x: nextViewport.centerWorldX,
            center_world_y: nextViewport.centerWorldY,
            following: mapFollowUiState.following,
          });
          updateViewport(nextViewport, { deferReactCommit: true });
          if (!followDisabledForRun) {
            syncFollowStateForViewport(nextViewport);
          }
          await sleepMs(stepDelayMs);
        }
        setViewportGestureActive(false);
        if (!followDisabledForRun) {
          flushDeferredFollowSync();
        }
        pumpRasterTilePlanQueue();
        pumpMapOverlayQueryQueue();
        debugLog("automated-test-drag", {
          run_id: runId,
          index: dragIndex,
          zoom: viewportRef.current.zoom,
          center_world_x: viewportRef.current.centerWorldX,
          center_world_y: viewportRef.current.centerWorldY,
        });
        await sleepMs(dragIntervalMs);
      }
      await sleepMs(settleMs);
      debugLog("automated-test-end", {
        run_id: runId,
        settle_ms: settleMs,
      });
    } finally {
      setViewportGestureActive(false);
      if (!followDisabledForRun) {
        flushDeferredFollowSync();
      }
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
      setLiveDragPerfRunning(false);
      if (hadPriorRunId) {
        globalWithRunId.__aerobagPerfRunId = priorRunId;
      } else {
        delete globalWithRunId.__aerobagPerfRunId;
      }
    }
  }, [liveDragPerfRunning, mapFollowUiState.following, onPlaybackSnapshotChange, surfaceSize.height, surfaceSize.width, uiSession]);

  useEffect(() => {
    const globalWithAutomation = globalThis as typeof globalThis & { __aerobagRunLiveDragPerf?: () => Promise<void> };
    globalWithAutomation.__aerobagRunLiveDragPerf = runLiveDragPerf;
    return () => {
      if (globalWithAutomation.__aerobagRunLiveDragPerf === runLiveDragPerf) {
        delete globalWithAutomation.__aerobagRunLiveDragPerf;
      }
    };
  }, [runLiveDragPerf]);

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
    const remainingGestureMs = viewportGestureUntilRef.current - Date.now();
    if (gestureActiveRef.current || followSyncPendingSerial !== 0 || remainingGestureMs > 0) {
      debugLog("map.follow.target.skip_during_gesture", {
        pending_sync_serial: followSyncPendingSerial,
        remaining_gesture_ms: Math.max(0, remainingGestureMs),
        zoom: mapFollowTargetViewport.zoom,
        center_lat: mapFollowTargetViewport.center.lat,
        center_lon: mapFollowTargetViewport.center.lon,
      });
      if (!gestureActiveRef.current && followSyncPendingSerial === 0 && remainingGestureMs > 0) {
        const timeout = window.setTimeout(() => {
          setFollowTargetRetryToken((token) => token + 1);
        }, remainingGestureMs + 16);
        return () => window.clearTimeout(timeout);
      }
      return;
    }
    const nextViewport = mapViewportFromCore(mapFollowTargetViewport);
    if (!sameMapViewport(nextViewport, viewport)) {
      debugLog("map.follow.target.apply", {
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
      });
      updateViewport(nextViewport, { deferReactCommit: true });
    }
  }, [followSyncPendingSerial, followTargetRetryToken, mapFollowTargetViewport, mapFollowUiState.following, viewport]);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection) {
      return;
    }
    if (event.pointerType === "mouse") {
      activePointersRef.current.clear();
      dragRef.current = null;
      pinchRef.current = null;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    activePointersRef.current.set(event.pointerId, point);
    setViewportGestureActive(activePointersRef.current.size > 0);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      clickCandidateRef.current = { pointerId: event.pointerId, start: point, latest: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2 && surfaceSize.width > 0 && surfaceSize.height > 0) {
      clickCandidateRef.current = null;
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
    if (trayGroup.scrimOpen || mapSelection || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    if (clickCandidateRef.current?.pointerId === event.pointerId) {
      clickCandidateRef.current = { ...clickCandidateRef.current, latest: point };
    }
    const pointers = Array.from(activePointersRef.current.entries());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      const nextViewport = dragViewport(viewportRef.current, dx, dy);
      noteViewportGesture();
      debugLog("map.drag.viewport", {
        dx,
        dy,
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
        following: mapFollowUiState.following,
      });
      updateViewport(nextViewport, { deferReactCommit: true });
      syncFollowStateForViewport(nextViewport);
      dragRef.current = { id: event.pointerId, last: point };
      if (clickCandidateRef.current && distanceBetween(clickCandidateRef.current.start, point) > 8) {
        clickCandidateRef.current = null;
      }
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
        selectedMap,
        surfaceSize.width,
        surfaceSize.height,
      );
      noteViewportGesture();
      debugLog("map.pinch.viewport", {
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
        following: mapFollowUiState.following,
      });
      updateViewport(nextViewport);
      syncFollowStateForViewport(nextViewport);
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    const clickCandidate = clickCandidateRef.current;
    activePointersRef.current.delete(event.pointerId);
    setViewportGestureActive(activePointersRef.current.size > 0);
    if (activePointersRef.current.size === 0) {
      flushDeferredFollowSync();
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
    }
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
    if (
      clickCandidate &&
      clickCandidate.pointerId === event.pointerId &&
      activePointersRef.current.size === 0 &&
      surfaceSize.width > 0 &&
      surfaceSize.height > 0 &&
      uiSession
    ) {
      clickCandidateRef.current = null;
      const world = screenToWorld(viewportRef.current, clickCandidate.latest, surfaceSize.width, surfaceSize.height);
      const click = worldToLatLon(world.x, world.y);
      void uiSession
        .queryMapSelection(viewportRef.current, surfaceSize.width, surfaceSize.height, click)
        .then((result) => {
          setMapSelection({
            point: clickCandidate.latest,
            result,
            selectedItem: null,
            detailModal: null,
          });
        })
        .catch((error) => {
          debugLog("map.selection.failed", { error: errorMessage(error) });
        });
    } else if (activePointersRef.current.size === 0) {
      clickCandidateRef.current = null;
    }
  }

  function handleLostPointerCapture(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    if (clickCandidateRef.current?.pointerId === event.pointerId) {
      clickCandidateRef.current = null;
    }
    setViewportGestureActive(activePointersRef.current.size > 0);
    if (activePointersRef.current.size === 0) {
      flushDeferredFollowSync();
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
    }
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
      selectedMap,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom - event.deltaY / 360,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom + 0.75,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function setViewportZoom(nextZoom: number) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap,
      { x: surfaceSize.width / 2, y: surfaceSize.height / 2 },
      surfaceSize.width,
      surfaceSize.height,
      nextZoom,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function selectMapSelectionItemForNavRef(result: MapSelectionQueryResult, navRef: NavRef) {
    const key = navRefKey(navRef);
    for (const category of result.categories) {
      const item = category.items.find((candidate) =>
        candidate.nav_ref ? navRefKey(candidate.nav_ref) === key : false,
      );
      if (item) {
        return item;
      }
    }
    return null;
  }

  async function recenterOnNavRef(navRef: NavRef) {
    const position = await props.appCoreAdapter.resolveNavRefPosition(navRef);
    const centerWorld = latLonToWorld(position.lat, position.lon);
    const nextViewport = {
      ...viewportRef.current,
      centerWorldX: centerWorld.x,
      centerWorldY: centerWorld.y,
    };
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
    return { position, viewport: nextViewport };
  }

  async function inspectNavRef(navRef: NavRef) {
    const { position, viewport: nextViewport } = await recenterOnNavRef(navRef);
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const point = worldToScreen(
      nextViewport,
      latLonToWorld(position.lat, position.lon),
      surfaceSize.width,
      surfaceSize.height,
    );
    const result = await uiSession.queryMapSelection(
      nextViewport,
      surfaceSize.width,
      surfaceSize.height,
      position,
    );
    const selectedItem = selectMapSelectionItemForNavRef(result, navRef);
    setMapSelection({
      point,
      result,
      selectedItem,
      detailModal: null,
    });
  }

  function submitChartSearch() {
    const query = chartSearch.query.trim().toUpperCase();
    if (!query) {
      return;
    }
    const selected = chartSearch.suggestions[0] ?? null;
    setChartSearch((current) => ({ ...current, loading: true, error: null }));
    void (async () => {
      const navRef = selected?.nav_ref ?? await props.appCoreAdapter.resolveWaypointIdentifier(query);
      if (!navRef) {
        setChartSearch((current) => ({
          ...current,
          loading: false,
          error: `No waypoint match for ${query}`,
          suggestions: [],
        }));
        return;
      }
      await inspectNavRef(navRef);
      setChartSearch({ query: "", open: false, loading: false, error: null, suggestions: [] });
    })().catch((error) => {
      setChartSearch((current) => ({
        ...current,
        loading: false,
        error: `Search failed: ${errorMessage(error)}`,
      }));
    });
  }

  function reportFirstVisualReady() {
    if (firstVisualReadyRef.current) {
      return;
    }
    firstVisualReadyRef.current = true;
    onFirstVisualReady();
  }

  function reportRasterTileLoaded(tile: RasterRenderTile) {
    reportFirstVisualReady();
    const key = rasterTileKey(tile);
    loadedRasterTileKeysRef.current.add(key);
    knownLoadedRasterTileKeysRef.current.add(key);
    if (page !== "map" || tiles.length === 0) {
      return;
    }
    if (!reportRasterTilesReadyIfComplete(tiles)) {
      return;
    }
    const timing = pageTilePaintTiming;
    if (!timing) {
      return;
    }
    debugLog("web.page-to-map.images", {
      id: timing.id,
      from_page: timing.fromPage,
      elapsed_ms: Math.round(performance.now() - timing.startedAt),
      tiles: tiles.length,
    });
    completePageTilePaintTiming(timing, "images");
  }

  function reportRasterTileError(tile: RasterRenderTile) {
    const key = rasterTileKey(tile);
    loadedRasterTileKeysRef.current.add(key);
    knownLoadedRasterTileKeysRef.current.add(key);
    setFailedRasterTileKeys((current) => {
      if (current.has(key)) {
        return current;
      }
      const next = new Set(current);
      next.add(key);
      return next;
    });
    loadedRasterTileKeysRef.current.add(key);
    debugLog("map.raster.tile.error", {
      selected_map_id: selectedMap.selected_map_id,
      selected_family_id: selectedFamily?.id ?? null,
      viewport_zoom: viewportRef.current.zoom,
      zoom: tile.zoom,
      x: tile.x,
      y_tms: tile.yTms,
      family: tile.chartFamily,
      map_view_id: tile.mapViewId,
      package_name: tile.packageName,
      src: tile.src,
    });
  }

  const visibleTerrainImages = terrainOverlay.query
    ? terrainOverlay.images
      .filter((image) => terrainOverlay.query?.tile_requests.some((request) => request.key === image.key))
      .map((image) => terrainImageForViewport(image, viewport, surfaceSize.width, surfaceSize.height))
    : [];
  const layerTrayOptions: TrayOption[] = [
    {
      id: "metars",
      label: "Observations",
      iconSrc: layerIconSrc("metars"),
      toggleState: mapLayerState.metars,
      disabled: !mapLayerState.metars.enabled,
      onSelect: () => void setMapLayerVisible("metars", !mapLayerState.metars.visible),
    },
    {
      id: "vectors",
      label: "Vectors",
      iconSrc: layerIconSrc("vectors"),
      toggleState: mapLayerState.vectors,
      disabled: !mapLayerState.vectors.enabled,
      onSelect: () => void setMapLayerVisible("vectors", !mapLayerState.vectors.visible),
    },
    {
      id: "nexrad",
      label: "NEXRAD",
      iconSrc: layerIconSrc("nexrad"),
      toggleState: mapLayerState.nexrad,
      disabled: !mapLayerState.nexrad.enabled,
      onSelect: () => void setMapLayerVisible("nexrad", !mapLayerState.nexrad.visible),
    },
    {
      id: "terrain_warning",
      label: "Terrain Warning",
      iconSrc: layerIconSrc("terrain_warning"),
      toggleState: mapLayerState.terrain_warning,
      disabled: !mapLayerState.terrain_warning.enabled,
      onSelect: () => void setMapLayerVisible("terrain_warning", !mapLayerState.terrain_warning.visible),
    },
    {
      id: "world_basemap",
      label: "World Map",
      iconSrc: layerIconSrc("world_basemap"),
      toggleState: mapLayerState.world_basemap,
      disabled: !mapLayerState.world_basemap.enabled,
      onSelect: () => void setMapLayerVisible("world_basemap", !mapLayerState.world_basemap.visible),
    },
    {
      id: "offline_regions",
      label: "Offline Regions",
      iconSrc: layerIconSrc("offline_regions"),
      toggleState: mapLayerState.offline_regions,
      disabled: !mapLayerState.offline_regions.enabled,
      onSelect: () => void setMapLayerVisible("offline_regions", !mapLayerState.offline_regions.visible),
    },
  ];
  const ownshipSourceOptions: TrayOption[] = ownshipControls.sources.map((source) => ({
    id: sourceIdString(source.source_id),
    label: source.label,
    active: source.active,
    disabled: !source.enabled || !uiSession,
    onSelect: () => {
      if (!uiSession) {
        return;
      }
      void uiSession
        .selectOwnshipSource({ kind: "source", source_id: sourceIdString(source.source_id) })
        .then(onPlaybackSnapshotChange)
        .finally(() => trayGroup.close("ownship"));
    },
  }));

  return (
    <section className="pageSurface">
      <Profiler id="MapSurface" onRender={logReactProfilerRender}>
        <div
          ref={containerRef}
          className="mapSurface chartSurface"
          data-testid="map-surface"
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
        <FlightDataBanner
          banner={flightDataBanner}
          edge={flightDataBannerEdge}
          edgeColumnCount={flightDataBannerEdgeColumnCount}
          edgeLayout={flightDataBannerEdgeLayout}
          lowered={statusControlDockLowered}
        />
        {trayGroup.scrimOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}
        {mapSelection ? (
          <>
            <TrayScrim ariaLabel="Close map selection" onClose={() => setMapSelection(null)} />
            {mapSelection.detailModal ? (
              <MapSelectionDetailModal
                title={mapSelection.detailModal.title}
                text={mapSelection.detailModal.text}
              />
            ) : (
              <MapSelectionTray
                point={mapSelection.point}
                result={mapSelection.result}
                selectedItem={mapSelection.selectedItem}
                onSelectItem={(item) => setMapSelection((current) => current ? { ...current, selectedItem: item, detailModal: null } : current)}
                onSelectDetail={(title, text) => setMapSelection((current) => current ? { ...current, detailModal: { title, text } } : current)}
                onSelectAction={async (item, action) => {
                  if (!appCoreAdapter) {
                    return;
                  }
                  if (action.id === "plates" || action.id === "csup") {
                    const airportId = airportIdFromNavRef(item.nav_ref);
                    if (!airportId) {
                      return;
                    }
                    onOpenPlateTarget(airportId, action.id === "csup" ? "CSup" : "Folder");
                    setMapSelection(null);
                    return;
                  }
                  if (action.id === "offline_region_mode") {
                    setMapSelection((current) => current ? {
                      ...current,
                      detailModal: {
                        title: action.label,
                        text: item.detail_text ?? "Offline package region controls are not connected in this web build yet.",
                      },
                    } : current);
                    return;
                  }
                  if (action.id === "offline_packages") {
                    setMapSelection((current) => current ? {
                      ...current,
                      detailModal: {
                        title: "Offline Packages",
                        text: "Offline Packages settings are not available in this web build yet.",
                      },
                    } : current);
                    return;
                  }
                  if (action.flight_plan_row_action) {
                    try {
                      if (!uiSession) {
                        throw new Error("map selection row action requires live core session");
                      }
                      await uiSession.performFlightPlanRowAction(
                        action.flight_plan_row_action.row_uid,
                        action.flight_plan_row_action.action_uid,
                      );
                      setMapSelection(null);
                    } catch (error) {
                      debugLog("map.selection.row_action.failed", {
                        action_id: action.id,
                        row_action: action.flight_plan_row_action,
                        error: errorMessage(error),
                      });
                    }
                    return;
                  }
                  if (action.session_action) {
                    try {
                      if (!uiSession) {
                        throw new Error("map selection session action requires live core session");
                      }
                      await uiSession.performMapSelectionAction(action.session_action);
                      setMapSelection(null);
                    } catch (error) {
                      debugLog("map.selection.session_action.failed", {
                        action_id: action.id,
                        error: errorMessage(error),
                      });
                    }
                    return;
                  }
                  debugLog("map.selection.action.unhandled", {
                    action_id: action.id,
                    item_id: item.id,
                  });
                }}
              />
            )}
          </>
        ) : null}
        <div ref={mapContentTransformRef} className="mapContentTransform">
          <Profiler id="RasterLayer" onRender={logReactProfilerRender}>
            <div
              className="rasterTileLayer"
              aria-hidden="true"
              style={rasterTileTransform ? { transform: rasterTileTransform, transformOrigin: "center center" } : undefined}
            >
              {tiles.map((tile) => (
                <div
                  key={rasterTileKey(tile)}
                  className="mapTile"
                  style={{
                    left: `${tile.left}px`,
                    top: `${tile.top}px`,
                    // Fractional overzoomed tile sizes can expose subpixel seams between rasters.
                    width: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
                    height: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
                    zIndex: tile.zIndex,
                  }}
                >
                  {failedRasterTileKeys.has(rasterTileKey(tile)) ? null : (
                    <img
                      className="mapTileImage"
                      src={tile.src}
                      alt=""
                      draggable={false}
                      onLoad={() => reportRasterTileLoaded(tile)}
                      onError={() => reportRasterTileError(tile)}
                    />
                  )}
                  {debugState.tile_labels ? (
                    <div className="tileLabel">
                      z{tile.zoom} x{tile.x} y{tile.yTms}
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </Profiler>
          {visibleTerrainImages.length > 0 ? (
            <div className="terrainOverlay" aria-hidden="true">
              {visibleTerrainImages.map((tile) => (
                <TerrainOverlayCanvasTile
                  key={tile.key}
                  tile={tile}
                />
              ))}
            </div>
          ) : null}
          {nexradOverlay.tiles.length > 0 ? (
            <svg
              className="nexradOverlay"
              viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
              preserveAspectRatio="none"
              aria-hidden="true"
              style={nexradOverlayTransform ? { transform: nexradOverlayTransform, transformOrigin: "center center" } : undefined}
            >
              {nexradOverlay.tiles.map((tile) => {
                const bounds = nexradTileBounds(tile);
                const label = `res${tile.res} x${tile.x} y${tile.y}`;
                const labelTone = (tile.res + tile.x + tile.y) % 2 === 0 ? " isBlue" : " isOrange";
                return (
                  <g key={tile.key}>
                    <svg
                      x={bounds.left}
                      y={bounds.top}
                      width={bounds.width}
                      height={bounds.height}
                      viewBox={`${tile.source_x} ${tile.source_y} ${tile.source_width} ${tile.source_height}`}
                      preserveAspectRatio="none"
                      overflow="hidden"
                    >
                      <image
                        href={tile.src}
                        x={0}
                        y={0}
                        width={tile.image_width}
                        height={tile.image_height}
                        preserveAspectRatio="none"
                      />
                    </svg>
                    {debugState.nexrad_tile_labels ? (
                      <g className={`nexradTileLabel${labelTone}`} transform={`translate(${bounds.left + 4} ${bounds.top + 15})`}>
                        <rect x={-3} y={-12} width={label.length * 7 + 6} height={16} rx={4} />
                        <text x={0} y={0}>{label}</text>
                      </g>
                    ) : null}
                  </g>
                );
              })}
            </svg>
          ) : null}
          <Profiler id="VectorLayer" onRender={logReactProfilerRender}>
            <>
              <>
                {mapIsVisible && (mapOverlay.airspace_paths.length > 0 || mapOverlay.tfr_paths.length > 0 || mapOverlay.airspace_labels.length > 0) ? (
                  <svg
                    className="airspaceOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {mapOverlay.airspace_paths.map((feature) => (
                      <AirspaceDisplayPathGroup key={feature.id} feature={feature} />
                    ))}
                    {mapOverlay.tfr_paths.map((feature) => (
                      <AirspaceDisplayPathGroup key={feature.id} feature={feature} />
                    ))}
                    {mapOverlay.airspace_labels.map((label) => {
                      return (
                        <g
                          key={`${label.feature_id}:${label.glyph.upper}:${label.glyph.lower}:${label.screen_x}:${label.screen_y}`}
                          className={`airspaceFractionLabel airspaceLabel-${label.glyph.style_key}`}
                          transform={`translate(${label.screen_x} ${label.screen_y})`}
                        >
                          <AirspaceLimitGlyph glyph={label.glyph} />
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.offline_regions.length > 0 ? (
                  <svg
                    className="offlineRegionsOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {mapOverlay.offline_regions.map((region) => {
                      const color = aviationThemeColor(region.color_key);
                      const summaryLines = offlineRegionSummaryLines(region);
                      return (
                        <g key={region.id}>
                          <polygon
                            points={offlineRegionPoints(region.points)}
                            fill="none"
                            stroke="rgba(255,255,255,0.8)"
                            strokeWidth="5"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                          <polygon
                            points={offlineRegionPoints(region.points)}
                            fill="none"
                            stroke={color}
                            strokeWidth="2.5"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                          <text
                            x={region.label_x}
                            y={region.label_y}
                            textAnchor="middle"
                            dominantBaseline="middle"
                            fill="white"
                            stroke="rgba(0,0,0,0.7)"
                            strokeWidth="4"
                            paintOrder="stroke"
                          >
                            <tspan x={region.label_x} dy={summaryLines.length ? "-0.72em" : "0"}>{region.label}</tspan>
                            {summaryLines.map((summary, index) => (
                              <tspan key={`${region.id}:summary:${index}`} x={region.label_x} dy="1.35em">{summary}</tspan>
                            ))}
                          </text>
                          <text
                            x={region.label_x}
                            y={region.label_y}
                            textAnchor="middle"
                            dominantBaseline="middle"
                            fill={color}
                          >
                            <tspan x={region.label_x} dy={summaryLines.length ? "-0.72em" : "0"}>{region.label}</tspan>
                            {summaryLines.map((summary, index) => (
                              <tspan key={`${region.id}:summary:${index}`} x={region.label_x} dy="1.35em">{summary}</tspan>
                            ))}
                          </text>
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && routeScreenSegments.length > 0 ? (
                  <svg className="flightPlanOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
                    {routeScreenSegments.map((segment) => (
                      <Fragment key={segment.id}>
                        {debugState.sequencing_finish_lines && segment.status === "active"
                          ? segment.finishLinePaths.map((finishLinePath, index) => (
                              <line
                                key={`finish-${index}`}
                                x1={finishLinePath[0].x}
                                y1={finishLinePath[0].y}
                                x2={finishLinePath[1].x}
                                y2={finishLinePath[1].y}
                                stroke="#b100ff"
                                strokeWidth="1.5"
                                strokeLinecap="round"
                                opacity="0.9"
                              />
                            ))
                          : null}
                        <polyline
                          points={segment.path.map((point) => `${point.x},${point.y}`).join(" ")}
                          fill="none"
                          stroke="rgba(0, 0, 0, 0.55)"
                          strokeWidth="7"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                        <polyline
                          points={segment.path.map((point) => `${point.x},${point.y}`).join(" ")}
                          fill="none"
                          stroke={routeSegmentColor(segment.status)}
                          strokeWidth="3.5"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </Fragment>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_features.length > 0 ? (
                  <svg
                    className="vectorOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {mapOverlay.visible_features.map((feature) => {
                      return (
                        <g
                          key={feature.id}
                          transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                          data-testid={feature.label ? `parity:map-feature:${feature.kind}:${feature.label}:${feature.id}` : undefined}
                        >
                          <VectorPointSymbol feature={feature} />
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_metars.length > 0 ? (
                  <svg
                    className="metarOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {mapOverlay.visible_metars.map((feature) => (
                      <g
                        key={wrappedFeatureRenderKey(feature.station_id, feature.screen_x, feature.screen_y)}
                        transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                      >
                        <MetarSymbol feature={feature} />
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_pireps.length > 0 ? (
                  <svg
                    className="metarOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {mapOverlay.visible_pireps.map((feature) => (
                      <g
                        key={wrappedFeatureRenderKey(feature.id, feature.screen_x, feature.screen_y)}
                        transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                      >
                        <PirepSymbol feature={feature} scale={0.32} />
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && (mapOverlay.flight_plan_features ?? []).length > 0 ? (
                  <svg
                    className="vectorOverlay flightPlanVectorOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {(mapOverlay.flight_plan_features ?? []).map((feature) => {
                      return (
                        <g
                          key={feature.id}
                          transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                          data-testid={feature.label ? `parity:map-fp-feature:${feature.kind}:${feature.label}:${feature.id}` : undefined}
                        >
                          <VectorPointSymbol feature={feature} />
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && selectedMapHighlight ? (
                  <svg
                    className="mapSelectionHighlightOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={selectedMapHighlight.kind === "spot" ? undefined : overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
                  >
                    {selectedMapHighlight.kind === "point" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <VectorPointSymbol feature={selectedMapHighlight.feature} />
                        </g>
                        <VectorPointSymbol feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "metar" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <MetarSymbol feature={selectedMapHighlight.feature} />
                        </g>
                        <MetarSymbol feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "pirep" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <PirepSymbol feature={selectedMapHighlight.feature} scale={0.32} />
                        </g>
                        <PirepSymbol feature={selectedMapHighlight.feature} scale={0.32} />
                      </g>
                    ) : selectedMapHighlight.kind === "path" ? (
                      <g>
                        {selectedMapHighlight.feature.paths.map((path, index) => (
                          <path
                            key={`${selectedMapHighlight.feature.id}:highlight:${index}`}
                            d={airspaceSvgPathD(path)}
                            fill="none"
                            stroke="white"
                            strokeWidth="9"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                        ))}
                        <AirspaceDisplayPathGroup feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "offline_region" ? (
                      <polygon
                        points={offlineRegionPoints(selectedMapHighlight.feature.points)}
                        fill="rgba(255,255,255,0.08)"
                        stroke="white"
                        strokeWidth="6"
                        strokeLinejoin="round"
                        vectorEffect="non-scaling-stroke"
                      />
                    ) : (
                      <g transform={`translate(${selectedMapHighlight.point.x} ${selectedMapHighlight.point.y})`}>
                        <MapSelectionSpotSymbol />
                      </g>
                    )}
                  </svg>
                ) : null}
              </>
            </>
          </Profiler>
        </div>
        <StatusControlDock
          controls={ownshipControls}
          dataStatusState={dataStatusState}
          lowered={statusControlDockLowered}
          ownshipOpen={trayGroup.isOpen("ownship")}
          statusOpen={trayGroup.isOpen("status")}
          onOwnshipToggle={() => trayGroup.toggle("ownship")}
          onStatusToggle={() => trayGroup.toggle("status")}
          onAction={onStatusAction}
          options={ownshipSourceOptions}
          transportControls={<SituationTransportRow controls={ownshipControls.situation_controls} onInput={onSituationControlInput} />}
        />
        {mapIsVisible && situationOverlay ? (
          <>
            <svg className="situationOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
              {situationOverlay.ring ? (
                <>
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
                  {situationOverlay.ring.cardinalLabels.map((label) => (
                    <Fragment key={label.text}>
                      <text
                        x={label.point.x}
                        y={label.point.y}
                        fill="none"
                        stroke="rgba(0, 0, 0, 0.4)"
                        strokeWidth="5"
                        strokeLinejoin="round"
                        fontSize="16"
                        fontWeight="700"
                        textAnchor="middle"
                        dominantBaseline="middle"
                        transform={`rotate(${label.rotationDeg} ${label.point.x} ${label.point.y})`}
                      >
                        {label.text}
                      </text>
                      <text
                        x={label.point.x}
                        y={label.point.y}
                        fill="#ffffff"
                        fontSize="16"
                        fontWeight="700"
                        textAnchor="middle"
                        dominantBaseline="middle"
                        transform={`rotate(${label.rotationDeg} ${label.point.x} ${label.point.y})`}
                      >
                        {label.text}
                      </text>
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
                </>
              ) : null}
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
              <SituationAircraftSvg
                iconSrc={planViewIcon}
                point={situationOverlay.point}
                headingDeg={situationOverlay.headingDeg}
                sizePx={thumbPixels(1.44)}
              />
            </svg>
          </>
        ) : null}

        <div className="chartDock">
          <HomeNavButton active={page === "home"} onClick={() => onSelectPage("home")} />
          <ChartPlateToggleButton page={page} onSelectPage={onSelectPage} />
          <TrayDock
            launcherLabel={selectedFamily?.launcher_label ?? "---"}
            launcherImageSrc={chartFamilyIconSrc(selectedFamily?.id)}
            launcherStyle={chartFamilyIconSrc(selectedFamily?.id)
              ? {
                  backgroundColor: "var(--theme-button-bg)",
                }
              : undefined}
            open={trayGroup.isOpen("family")}
            onToggle={() => trayGroup.toggle("family")}
            ariaLabel="Chart family"
            testId="chart-family-button"
            options={familyOptions.map((family) => ({
              id: family.id,
              label: family.label,
              iconSrc: chartFamilyIconSrc(family.id),
              active: family.active,
              disabled: !family.enabled,
              onSelect: () => {
                onSelectMapFamily(family.id);
                trayGroup.close("family");
              },
            }))}
          />
          <TrayDock
            launcherLabel="LAYERS"
            launcherImageSrc={layerIconSrc("vectors")}
            launcherStyle={{
              backgroundColor: "var(--theme-button-bg)",
            }}
            open={trayGroup.isOpen("layers")}
            onToggle={() => trayGroup.toggle("layers")}
            ariaLabel="Layers"
            testId="layers-button"
            options={layerTrayOptions}
          />
          <ChartSearchBox
            state={chartSearch}
            onQueryChange={(query) => setChartSearch((current) => ({ ...current, query, open: true }))}
            onFocus={() => setChartSearch((current) => ({ ...current, open: true }))}
            onClose={() => setChartSearch((current) => ({ ...current, open: false }))}
            onSubmit={submitChartSearch}
            onSelect={(suggestion) => {
              setChartSearch((current) => ({ ...current, loading: true, error: null }));
              void inspectNavRef(suggestion.nav_ref)
                .then(() => setChartSearch({ query: "", open: false, loading: false, error: null, suggestions: [] }))
                .catch((error) => {
                  setChartSearch((current) => ({
                    ...current,
                    loading: false,
                    error: `Search failed: ${errorMessage(error)}`,
                  }));
                });
            }}
          />
        </div>

        <NavElementButton
          navElement={planUiState?.guidance?.nav_element}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        />

        {debugState.playback_visible ? (
          <PlaybackWidget
            uiSession={uiSession}
            playbackUiState={props.playbackUiState}
            sourcePath={props.playbackSourcePath}
            onSourcePathChange={props.onPlaybackSourcePathChange}
            onSnapshotChange={props.onPlaybackSnapshotChange}
            surfaceWidth={surfaceSize.width}
            dock="left"
          />
        ) : null}

        <ZoomControl
          zoom={viewport.zoom}
          minZoom={selectedMap.min_zoom}
          maxZoom={selectedMap.max_zoom}
          onZoomChange={setViewportZoom}
        />

        <div className="mapBottomRightDock">
          <div className="debugDock mapDebugDock isRightAligned">
            <DebugDock open={debugOpen} warn={debugWarningActive} onToggle={onDebugToggle}>
              <CommonDebugPanel
                uptimeLabel={uptimeLabel}
                debugState={debugState}
                onDebugFlagChange={onDebugFlagChange}
                extraLines={nexradDebugLines}
                onRunDragPerf={runLiveDragPerf}
                dragPerfRunning={liveDragPerfRunning}
                lastDragPerfRunId={lastLiveDragPerfRunId}
              />
            </DebugDock>
          </div>
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
        </div>
        </div>
      </Profiler>
    </section>
  );
}

function NavElementButton(props: {
  navElement: NavElementUiView | null | undefined;
  className?: string;
  onClick?: () => void;
  onPointerDown?: (event: PointerEvent<HTMLElement>) => void;
  onPointerUp?: (event: PointerEvent<HTMLElement>) => void;
  onDoubleClick?: (event: MouseEvent<HTMLElement>) => void;
}) {
  const { navElement, className = "navElement", onClick, onPointerDown, onPointerUp, onDoubleClick } = props;
  const displayedNavElement = navElement ?? {
    active_leg_summary: "NO ACTIVE LEG",
    cdi_indicator_dots: null,
    cdi_offscale_readout: null,
  };
  return (
    <button
      type="button"
      className={`${className}${navElement ? "" : " isUnavailable"}`}
      data-testid="nav-cdi"
      onPointerDown={onPointerDown}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      onClick={onClick}
    >
      <NavElementView navElement={displayedNavElement} />
    </button>
  );
}

function FlightDataBanner(props: {
  banner: FlightDataBannerModel;
  edge: FlightDataBannerEdge;
  edgeColumnCount?: number;
  edgeLayout?: boolean;
  lowered?: boolean;
}) {
  const cells = props.banner.cells;
  if (cells.length === 0) {
    return null;
  }
  const edgeClass = props.edge === "left" ? " isLeftEdge" : " isRightEdge";
  const edgeColumnClass =
    props.edgeLayout && props.edgeColumnCount && props.edgeColumnCount > 1
      ? ` isEdgeColumns${props.edgeColumnCount}`
      : "";
  return (
    <div
      className={`flightDataBanner${props.edgeLayout ? ` isEdgeLayout${edgeClass}` : ""}${edgeColumnClass}${props.lowered ? " isLowered" : ""}`}
      aria-label="Flight data"
    >
      {cells.map((cell) => (
        <div key={cell.id} className="flightDataCell">
          <span className="flightDataLabel">{cell.label}</span>
          <span className={`flightDataValue${cell.value ? "" : " isMissing"}`}>
            {cell.value ?? "\u2014"}
          </span>
        </div>
      ))}
    </div>
  );
}

function NavElementView(props: { navElement: NavElementUiView }) {
  const { navElement } = props;
  const width = 180;
  const height = 18;
  const unit = width / 4.5;
  const fullScaleDots = 2;
  const offscaleDots = 2.1;
  const dotXs = [0.25, 1.25, 3.25, 4.25].map((value) => value * unit);
  const centerX = 2.25 * unit;
  const baselineY = height * 0.5;
  const dotRadius = unit * 0.04375;
  const centerTriangleHalfWidth = unit * 0.25;
  const centerTriangleTopY = 0;
  const centerTriangleBottomY = height + 1;
  const pointerPosition = navElement.cdi_indicator_dots;
  const clampedPointerPosition =
    pointerPosition === null
      ? null
      : Math.max(-fullScaleDots, Math.min(fullScaleDots, pointerPosition));
  const pointerX =
    clampedPointerPosition === null
      ? null
      : (clampedPointerPosition + 2.25) * unit;
  const offscaleDirection =
    pointerPosition === null || Math.abs(pointerPosition) <= offscaleDots
      ? null
      : pointerPosition > 0
        ? "R"
        : "L";
  const offscaleBaseX = offscaleDirection === "R"
    ? (2.25 + offscaleDots) * unit
    : offscaleDirection === "L"
      ? (2.25 - offscaleDots) * unit
      : null;
  const offscaleTipX = offscaleDirection === "R" ? width : offscaleDirection === "L" ? 0 : null;
  const offscaleTrianglePoints = offscaleDirection && offscaleBaseX !== null && offscaleTipX !== null
    ? `${offscaleBaseX},${height * 0.18} ${offscaleBaseX},${height * 0.82} ${offscaleTipX},${baselineY}`
    : null;
  const offscaleReadout = navElement.cdi_offscale_readout;
  const offscaleReadoutDotIndex = offscaleReadout && offscaleDirection === "R"
    ? 2
    : offscaleReadout && offscaleDirection === "L"
      ? 1
      : null;
  const offscaleReadoutX = offscaleReadoutDotIndex === null ? null : dotXs[offscaleReadoutDotIndex];
  const cdiTitle =
    pointerPosition === null
      ? "No CDI deviation"
      : `${Math.abs(pointerPosition).toFixed(1)} dots ${pointerPosition > 0 ? "right" : "left"} of center`;
  return (
    <>
      <span className="navElementTop">{navElement.active_leg_summary}</span>
      <svg className="navElementBottom" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
        <title>{cdiTitle}</title>
        <path
          className="navElementCdiCenter"
          d={`M ${centerX - centerTriangleHalfWidth} ${centerTriangleBottomY} L ${centerX + centerTriangleHalfWidth} ${centerTriangleBottomY} L ${centerX} ${centerTriangleTopY} Z`}
        />
        {dotXs.map((x, index) => (
          index === offscaleReadoutDotIndex ? null : (
            <circle key={index} className="navElementCdiDot" cx={x} cy={baselineY} r={dotRadius} />
          )
        ))}
        {offscaleTrianglePoints ? (
          <>
            {offscaleReadout ? (
              <text className="navElementCdiOffscaleReadout" x={offscaleReadoutX ?? centerX} y={baselineY} textAnchor="middle" dominantBaseline="central">
                {offscaleReadout}
              </text>
            ) : null}
            <polygon className="navElementCdiOffscalePointer" points={offscaleTrianglePoints} />
          </>
        ) : pointerX !== null ? (
          <line className="navElementCdiPointer" x1={pointerX} y1={0} x2={pointerX} y2={height} />
        ) : null}
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
  const gapPatternId = useId();
  const maxWidthPx = playbackWidgetMaxWidthPx(surfaceWidth);
  const durationSeconds = Math.max(playbackUiState.duration_seconds, 0);
  const committedCursorSeconds = Math.min(Math.max(playbackUiState.cursor_seconds, 0), durationSeconds || 0);
  const cursorSeconds =
    scrubCursorSeconds === null
      ? committedCursorSeconds
      : Math.min(Math.max(scrubCursorSeconds, 0), durationSeconds || 0);
  const canControl = uiSession !== null;
  const summary = playbackUiState.title_label;
  const overviewWidth = 320;
  const overviewHeight = 34;
  const knobRadius = 7;
  const cursorRatio = durationSeconds > 0 ? cursorSeconds / durationSeconds : 0;
  const cursorX = knobRadius + cursorRatio * Math.max(overviewWidth - knobRadius * 2, 0);
  const speedPath = profilePathData(playbackUiState.speed_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);
  const altitudePath = profilePathData(playbackUiState.altitude_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);
  const gapRects = playbackUiState.gap_spans
    .map((gap, index) => {
      if (durationSeconds <= 0 || gap.end_seconds <= gap.start_seconds) {
        return null;
      }
      const usableWidth = Math.max(overviewWidth - knobRadius * 2, 0);
      const startRatio = Math.min(Math.max(gap.start_seconds / durationSeconds, 0), 1);
      const endRatio = Math.min(Math.max(gap.end_seconds / durationSeconds, 0), 1);
      const x = knobRadius + startRatio * usableWidth;
      const width = Math.max((endRatio - startRatio) * usableWidth, 1);
      return { key: `${gap.start_seconds}:${gap.end_seconds}:${index}`, x, width };
    })
    .filter((rect): rect is { key: string; x: number; width: number } => rect !== null);

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
        <button
          type="button"
          className="playbackWidgetButton playbackWidgetMediaButton"
          disabled={!canControl || playbackUiState.status === "empty"}
          onClick={() => void playPause()}
          aria-label={playbackUiState.status === "playing" ? "Pause playback" : "Play playback"}
        >
          {playbackUiState.status === "playing" ? (
            <svg className="playbackWidgetMediaIcon" viewBox="0 0 24 24" aria-hidden="true">
              <rect x="7" y="6" width="3.5" height="12" rx="0.8" />
              <rect x="13.5" y="6" width="3.5" height="12" rx="0.8" />
            </svg>
          ) : (
            <svg className="playbackWidgetMediaIcon" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M8 6.5v11l9-5.5z" />
            </svg>
          )}
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
          <defs>
            <pattern id={gapPatternId} patternUnits="userSpaceOnUse" width="8" height="8" patternTransform="rotate(45)">
              <rect width="8" height="8" className="playbackWidgetGapPatternBase" />
              <line x1="0" y1="0" x2="0" y2="8" className="playbackWidgetGapPatternLine" />
            </pattern>
          </defs>
          {altitudePath ? <path className="playbackWidgetAltitudeProfile" d={altitudePath} /> : null}
          {speedPath ? <path className="playbackWidgetSpeedProfile" d={speedPath} /> : null}
          {gapRects.map((gap) => (
            <rect
              key={gap.key}
              className="playbackWidgetGapSpan"
              fill={`url(#${gapPatternId})`}
              x={gap.x}
              y={0}
              width={gap.width}
              height={overviewHeight}
            />
          ))}
          <line className="playbackWidgetCursorLine" x1={cursorX} y1={0} x2={cursorX} y2={overviewHeight} />
          <circle className="playbackWidgetCursorKnob" cx={cursorX} cy={overviewHeight - 1} r={knobRadius} />
        </svg>
      </div>
      <div className="playbackWidgetSeekRow">
        <span className="playbackWidgetClock">{playbackUiState.cursor_label}</span>
        <span className="playbackWidgetClock">{playbackUiState.duration_label}</span>
      </div>
    </section>
  );
}

function FlightPlanPage(props: {
  appCoreAdapter: AppCoreAdapter | null;
  uiSession: UiSession | null;
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  plan: FlightPlan;
  planUiState: FlightPlanUiState | null;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenCharts: (airportId: string | null, chartId?: string | null) => void;
  onInsertAirportWaypointAtRow: (rowUid: string, before: boolean, airportId: string) => void | Promise<void>;
  onPreviewFlightPlanEntry: (input: string) => Promise<FlightPlanEntryPreview>;
  onAppendFlightPlanEntry: (input: string) => void | Promise<void>;
  onActivateNextLeg: () => void | Promise<void>;
  onSuspendSequencing: () => void | Promise<void>;
  onUnsuspendSequencing: () => void | Promise<void>;
  onSequenceActiveLeg: () => void | Promise<void>;
  onRestoreDirectTo: () => void | Promise<void>;
  onPerformFlightPlanRowAction: (rowUid: string, actionUid: string) => void | Promise<void>;
  onInsertAirwayAtRow: (
    rowUid: string,
    entryIndex: number,
    exitIndex: number,
    presentation: AirwayPresentationPlan,
  ) => void | Promise<void>;
  onSelectProcedureAtRow: (rowUid: string, airportId: string, procedureId: string, enrouteTransition: string | null) => void | Promise<void>;
  debugWarningActive: boolean;
}) {
  const [selectedWaypointUid, setSelectedWaypointUid] = useState<string | null>(null);
  const [selectedWaypointAnchor, setSelectedWaypointAnchor] = useState<{ top: number; height: number } | null>(null);
  const [airwayPicker, setAirwayPicker] = useState<{
    loading: boolean;
    error: string | null;
    mode: "insert";
    rowUid: string | null;
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
    rowUid: string;
    airportId: string;
    procedures: ProcedureSummary[];
    selectedProcedureId: string | null;
    options: ProcedureOptions | null;
  } | null>(null);
  const [airportInsert, setAirportInsert] = useState<{
    rowUid: string;
    before: boolean;
    airportId: string;
    error: string | null;
    loading: boolean;
    suggestions: WaypointIdentifierSuggestion[];
  } | null>(null);
  const [routeEntryText, setRouteEntryText] = useState("");
  const [routeEntryPreview, setRouteEntryPreview] = useState<FlightPlanEntryPreview>({
    can_commit: false,
    tokens: [],
    issues: [],
  });
  const [routeEntryLoading, setRouteEntryLoading] = useState(false);
  const [routeEntryError, setRouteEntryError] = useState<string | null>(null);
  const [routeEntrySubmitting, setRouteEntrySubmitting] = useState(false);
  const previewFlightPlanEntryRef = useRef(props.onPreviewFlightPlanEntry);
  useEffect(() => {
    previewFlightPlanEntryRef.current = props.onPreviewFlightPlanEntry;
  }, [props.onPreviewFlightPlanEntry]);
  const pageRef = useRef<HTMLElement | null>(null);
  const planScrollSurfaceRef = useRef<HTMLDivElement | null>(null);
  const waypointModalRef = useRef<HTMLElement | null>(null);
  const planControlsRef = useRef<HTMLDivElement | null>(null);
  const planFooterRef = useRef<HTMLDivElement | null>(null);
  const planUiState = props.planUiState;
  if (!planUiState) {
    throw new Error("FlightPlanPage requires core-projected FlightPlanUiState");
  }
  const guidance = planUiState.guidance ?? null;
  const structuredSurfaceRef = useRef<HTMLDivElement | null>(null);
  const structuredTableRef = useRef<HTMLDivElement | null>(null);
  const planScrollViewportRef = useRef<HTMLDivElement | null>(null);
  const planScrollContentRef = useRef<HTMLDivElement | null>(null);
  const structuredRowRefs = useRef(new Map<string, HTMLElement>());
  const [structuredArrow, setStructuredArrow] = useState<{ path: string; head: string } | null>(null);
  const [structuredGroupBoxes, setStructuredGroupBoxes] = useState<Array<{ key: string; top: number; left: number; width: number; height: number }>>([]);
  const [waypointModalTop, setWaypointModalTop] = useState<number | null>(null);
  const [waypointModalMaxHeight, setWaypointModalMaxHeight] = useState<number | null>(null);
  const waypointSuggestionPlanKey = useMemo(() => JSON.stringify(props.plan), [props.plan]);
  useEffect(() => {
    const editor = airportInsert;
    if (!editor) {
      return;
    }
    if (!props.uiSession) {
      setAirportInsert((current) => current ? {
        ...current,
        loading: false,
        suggestions: [],
        error: "core session unavailable",
      } : current);
      return;
    }
    const prefix = editor.airportId.trim().toUpperCase();
    if (!prefix) {
      setAirportInsert((current) => current ? { ...current, loading: false, suggestions: [] } : current);
      return;
    }
    let cancelled = false;
    setAirportInsert((current) => current ? { ...current, loading: true } : current);
    props.uiSession
      .suggestWaypointIdentifiersAtFlightPlanRow(editor.rowUid, editor.before, prefix, 8)
      .then((suggestions) => {
        if (!cancelled) {
          setAirportInsert((current) => current ? { ...current, loading: false, suggestions } : current);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setAirportInsert((current) => current ? {
            ...current,
            loading: false,
            suggestions: [],
            error: error instanceof Error ? error.message : String(error),
          } : current);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [airportInsert?.airportId, airportInsert?.before, airportInsert?.rowUid, props.uiSession, waypointSuggestionPlanKey]);
  useEffect(() => {
    if (!routeEntryText.trim()) {
      setRouteEntryPreview({
        can_commit: false,
        tokens: [],
        issues: [],
      });
      setRouteEntryLoading(false);
      setRouteEntryError(null);
      return;
    }
    let cancelled = false;
    setRouteEntryLoading(true);
    previewFlightPlanEntryRef.current(routeEntryText)
      .then((preview) => {
        if (!cancelled) {
          setRouteEntryPreview(preview);
          setRouteEntryLoading(false);
          setRouteEntryError(null);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setRouteEntryLoading(false);
          setRouteEntryError(errorMessage(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [routeEntryText, waypointSuggestionPlanKey]);
  const displayRows = useMemo(() => {
    return planUiState.display_rows.map((row, index) => ({
        showPlateTargetId:
          typeof (row as { show_plate_target_id?: unknown }).show_plate_target_id === "string"
            ? (row as { show_plate_target_id?: string | null }).show_plate_target_id ?? null
            : null,
        id:
          row.row_kind === "group"
            ? row.uid
            : row.row_kind === "discontinuity"
              ? row.uid
              : row.depth === 0
                ? row.uid
                : row.uid,
        rowUid: row.uid,
        label: row.label,
        dataCells: row.data_cells.map((cell) => cell.value ?? "\u2014"),
        active: row.active,
        enabled: row.enabled ?? true,
        syntheticDirectTo: row.synthetic_direct_to ?? false,
        depth: row.depth,
        rowKind: row.row_kind,
        refKey:
          row.row_kind === "group"
            ? row.uid
            : row.row_kind === "discontinuity"
              ? row.uid
              : row.depth === 0
                ? row.uid
                : row.uid,
        chartAirportId: row.chart_airport_id,
        legIndex: row.leg_index,
        removeLegIndex: null as number | null,
        startComponentIndex: row.start_component_index,
        endComponentIndex: row.end_component_index,
        replaceProcedureComponentIndex: row.replace_procedure_component_index,
        originAnchor: row.origin_anchor,
        destinationAnchor: row.destination_anchor,
        navRef: row.nav_ref,
        symbolFeature: row.symbol_feature,
        groupKey: row.row_kind === "group" || row.depth > 0 ? `group:${row.component_uid ?? row.component_index ?? index}` : null,
        componentIndex: row.component_index,
        componentKind: row.component_kind,
        procedureId: row.procedure_id,
        procedureKind: row.procedure_kind,
        precedingWaypoint: row.preceding_waypoint,
        followingWaypoint: row.following_waypoint,
        actionMatrix: row.action_matrix ?? [],
      }));
  }, [planUiState.display_rows]);
  const planDataColumns = planUiState.data_columns;
  const selectedWaypointIndex = selectedWaypointUid === null
    ? null
    : displayRows.findIndex((row) => row.rowUid === selectedWaypointUid);
  const selectedRow = selectedWaypointIndex !== null && selectedWaypointIndex >= 0
    ? displayRows[selectedWaypointIndex] ?? null
    : null;

  const rowActionRows = useMemo(() => {
    if (!selectedRow) {
      return [] as Array<Array<{ id: string; uid: string; label: string; enabled: boolean; execution?: string; onSelect: () => void }>>;
    }

    const closeTray = () => {
      setSelectedWaypointUid(null);
      setAirwayPicker(null);
      setProcedurePicker(null);
      setAirportInsert(null);
    };

    const actionForUi = (action: { id: string; uid: string; label: string; enabled: boolean; execution?: string; dismiss_tray_on_success?: boolean }) => {
      return {
        id: action.id,
        uid: action.uid,
        label: action.label,
        enabled: action.enabled,
        execution: action.execution,
        dismissTrayOnSuccess: action.dismiss_tray_on_success ?? true,
        onSelect: () => {
          if (!action.enabled) {
            return;
          }
          if (action.execution === "core_session") {
            void props.onPerformFlightPlanRowAction(selectedRow.rowUid, action.uid);
            if (action.dismiss_tray_on_success ?? true) {
              closeTray();
            }
            return;
          }
          if (action.id === "insert_before" || action.id === "insert_after") {
            setAirportInsert({
              rowUid: selectedRow.rowUid,
              before: action.id === "insert_before",
              airportId: "",
              error: null,
              loading: false,
              suggestions: [],
            });
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
              rowUid: selectedRow.rowUid,
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
            if (!selectedRow.chartAirportId) {
              return;
            }
            const trace = {
              row_uid: selectedRow.rowUid,
              airport_id: selectedRow.chartAirportId,
            };
            debugLog("plan.procedure_picker.open.start", trace);
            setProcedurePicker({
              loading: true,
              error: null,
              rowUid: selectedRow.rowUid,
              airportId: selectedRow.chartAirportId,
              procedures: [],
              selectedProcedureId: null,
              options: null,
            });
            window.requestAnimationFrame(() => {
              void debugTiming(
                "plan.procedure_picker.list_procedures",
                () => props.appCoreAdapter!.listProcedures(selectedRow.chartAirportId!, "approach"),
                trace,
              ).then((procedures) => {
                debugLog("plan.procedure_picker.open.done", { ...trace, procedure_count: procedures.length });
                setProcedurePicker((current) => current ? {
                  ...current,
                  loading: false,
                  procedures,
                } : current);
              }).catch((error) => {
                debugLog("plan.procedure_picker.open.error", { ...trace, message: errorMessage(error) });
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
    };
    return (selectedRow.actionMatrix as Array<Array<{ id: string; uid: string; label: string; enabled: boolean; execution?: string; dismiss_tray_on_success?: boolean }>>).map((row) => row.map(actionForUi));
  }, [props, selectedRow]);
  const rowActions = useMemo(() => rowActionRows.flat(), [rowActionRows]);

  useEffect(() => {
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
  }, [displayRows]);

  useEffect(() => {
    if (!guidance?.active_leg) {
      setStructuredArrow(null);
      return;
    }
    const scrollPane = planScrollSurfaceRef.current;
    const content = planScrollContentRef.current;
    if (!scrollPane || !content) {
      setStructuredArrow(null);
      return;
    }

    const fromIndex = guidance.active_from_row_uid
      ? displayRows.findIndex((row) => row.rowUid === guidance.active_from_row_uid)
      : -1;
    const toIndex = guidance.active_to_row_uid
      ? displayRows.findIndex((row) => row.rowUid === guidance.active_to_row_uid)
      : -1;
    if (toIndex < 0) {
      setStructuredArrow(null);
      return;
    }

    const toElement = structuredRowRefs.current.get(displayRows[toIndex]?.refKey ?? "");
    const fromElement = fromIndex >= 0 ? structuredRowRefs.current.get(displayRows[fromIndex]?.refKey ?? "") : null;
    if (!toElement || (fromIndex >= 0 && !fromElement)) {
      setStructuredArrow(null);
      return;
    }

    let animationFrame = 0;
    const measureArrow = () => {
      const surfaceRect = content.getBoundingClientRect();
      const toRect = toElement.getBoundingClientRect();
      const leftGutterX = thumbPixels(0.12);
      const waypointColumnLeftX = thumbPixels(0.5);
      const waypointColumnHeadInsetX = thumbPixels(0.08);
      const toPoint = {
        x: waypointColumnLeftX + waypointColumnHeadInsetX,
        y: toRect.top - surfaceRect.top + toRect.height / 2,
      };
      const elbowX = leftGutterX;
      const headLength = 20;
      const shaftEnd = { x: Math.max(elbowX, toPoint.x - headLength + 5), y: toPoint.y };
      if (!fromElement) {
        const stubStart = { x: elbowX, y: toPoint.y };
        setStructuredArrow({
          path: `M ${stubStart.x} ${stubStart.y} H ${shaftEnd.x}`,
          head: arrowHeadPoints(shaftEnd, toPoint),
        });
        return;
      }

      const fromRect = fromElement.getBoundingClientRect();
      const fromPoint = {
        x: waypointColumnLeftX,
        y: fromRect.top - surfaceRect.top + fromRect.height / 2,
      };

      setStructuredArrow({
        path: `M ${fromPoint.x} ${fromPoint.y} H ${elbowX} V ${toPoint.y} H ${shaftEnd.x}`,
        head: arrowHeadPoints(shaftEnd, toPoint),
      });
    };
    const scheduleMeasure = () => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(measureArrow);
    };

    measureArrow();

    const handle = window.requestAnimationFrame(() => {
      fromElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
      toElement.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
    scrollPane?.addEventListener("scroll", scheduleMeasure, { passive: true });
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      window.cancelAnimationFrame(handle);
      window.cancelAnimationFrame(animationFrame);
      scrollPane?.removeEventListener("scroll", scheduleMeasure);
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [displayRows, guidance?.active_leg]);

  useEffect(() => {
    if (selectedRow === null) {
      setWaypointModalTop(null);
      setWaypointModalMaxHeight(null);
      return;
    }
    const page = pageRef.current;
    const modal = waypointModalRef.current;
    if (!page || !modal) {
      return;
    }
    const pageRect = page.getBoundingClientRect();
    const topPadding = thumbPixels(0.5);
    const bottomPadding = thumbPixels(0.1);
    const blockers = [planControlsRef.current, planFooterRef.current]
      .flatMap((element) => (element ? [element.getBoundingClientRect().top - pageRect.top] : []));
    const bottomLimit = blockers.length > 0 ? Math.min(...blockers) : page.clientHeight;
    const maxHeight = Math.max(thumbPixels(1), bottomLimit - topPadding - bottomPadding);
    const modalHeight = Math.min(modal.scrollHeight || modal.getBoundingClientRect().height, maxHeight);
    const anchorCenter = selectedWaypointAnchor
      ? selectedWaypointAnchor.top + selectedWaypointAnchor.height / 2
      : topPadding + modalHeight / 2;
    const centeredTop = anchorCenter - modalHeight / 2;
    const maxTop = Math.max(topPadding, bottomLimit - bottomPadding - modalHeight);
    const top = Math.min(Math.max(centeredTop, topPadding), maxTop);

    setWaypointModalTop(top);
    setWaypointModalMaxHeight(maxHeight);
  }, [airwayPicker, selectedRow, selectedWaypointAnchor, rowActions.length]);

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

  return (
    <section className="appPage planPage" ref={pageRef}>
      <div className="chartDock">
        <HomeNavButton active={props.page === "home"} onClick={() => props.onSelectPage("home")} />
        <ChartPlateReturnButton
          targetPage={props.mostRecentChartOrPlatePage}
          onClick={props.onOpenRecentChartOrPlate}
        />
      </div>

      <div className="planScrollViewport" ref={planScrollViewportRef}>
        <div className="planScrollContent" ref={planScrollContentRef}>
          {structuredArrow ? (
            <svg className="planStructuredArrowLayer" aria-hidden="true">
              <path className="planStructuredArrowPath" d={structuredArrow.path} />
              <polygon className="planStructuredArrowHead" points={structuredArrow.head} />
            </svg>
          ) : null}
          <div className="planScrollSurface" ref={planScrollSurfaceRef}>
            <div className="planTableWrap isStructured" ref={structuredSurfaceRef}>
              <div className="planStructuredGroupBoxLayer" aria-hidden="true">
                {structuredGroupBoxes.map((box) => (
                  <div
                    key={box.key}
                    className="planStructuredGroupBoxOverlay"
                    style={{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px`, height: `${box.height}px` }}
                  />
                ))}
              </div>
              <div className="planTable" ref={structuredTableRef}>
                <div className="planHeader planWaypointCell">Waypoint</div>
                {planDataColumns.map((column) => (
                  <div key={column.id} className="planHeader">{column.label}</div>
                ))}
                {displayRows.map((row, index) => (
                  <Fragment key={row.id}>
                    <button
                      key={`${row.id}:waypoint`}
	                  type="button"
                      data-testid={`plan-row-${row.rowUid}`}
	                  ref={(node) => {
	                    if (row.refKey === null) {
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
                    !row.enabled ? "isDisabled" : "",
                    row.syntheticDirectTo ? "isSyntheticDirectTo" : "",
	                    "planStructuredWaypointCell",
	                    row.rowKind === "group" ? "isGroupHeader" : "",
	                    row.depth > 0 ? "isChildRow" : "",
	                    row.rowKind === "discontinuity" ? "isDiscontinuityItem" : "",
	                  ].filter(Boolean).join(" ")}
                  onClick={(event) => {
                    if (!row.enabled && !row.syntheticDirectTo) {
                      return;
                    }
                    const page = pageRef.current;
                    if (page) {
                      const pageRect = page.getBoundingClientRect();
                      const rowRect = event.currentTarget.getBoundingClientRect();
                      setSelectedWaypointAnchor({
                        top: rowRect.top - pageRect.top,
                        height: rowRect.height,
                      });
                    }
                    setSelectedWaypointUid(row.rowUid);
                    setAirwayPicker(null);
                    setProcedurePicker(null);
                  }}
                >
                    <WaypointButtonContent
                      label={row.label}
                      symbolFeature={row.symbolFeature}
                      indented={row.depth > 0}
                    />
                    </button>
                    {row.dataCells.map((value, cellIndex) => (
                      <div
                        key={`${row.id}:data:${planDataColumns[cellIndex]?.id ?? cellIndex}`}
                        className={[
                          "planCell",
                          row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
                        ].filter(Boolean).join(" ")}
                      >
                        {value}
                      </div>
                    ))}
                  </Fragment>
                ))}
              </div>
            </div>
          </div>
          <div className="planEntryDock">
            <div className="planEntryCell">
              <form
                className="planEntryForm"
                onSubmit={async (event) => {
                  event.preventDefault();
                  if (!routeEntryPreview.can_commit || routeEntrySubmitting) {
                    return;
                  }
                  setRouteEntrySubmitting(true);
                  setRouteEntryError(null);
                  try {
                    await props.onAppendFlightPlanEntry(routeEntryText);
                    setRouteEntryText("");
                    setRouteEntryPreview({
                      can_commit: false,
                      tokens: [],
                      issues: [],
                    });
                  } catch (error) {
                    setRouteEntryError(errorMessage(error));
                  } finally {
                    setRouteEntrySubmitting(false);
                  }
                }}
              >
                <div className={`planEntryInputShell${routeEntryPreview.can_commit ? " isReady" : ""}`}>
                  {routeEntryText ? (
                    <div className="planEntryOverlay" aria-hidden="true">
                      {flightPlanEntryPreviewSegments(routeEntryText, routeEntryPreview).map((segment, index) => (
                        <span
                          key={`${index}:${segment.text}`}
                          className={[
                            "planEntrySegment",
                            `is${segment.tokenState[0].toUpperCase()}${segment.tokenState.slice(1)}`,
                            segment.issue ? "hasIssue" : "",
                          ].filter(Boolean).join(" ")}
                        >
                          {segment.text}
                        </span>
                      ))}
                    </div>
                  ) : (
                    <div className="planEntryPlaceholder" aria-hidden="true">Append route...</div>
                  )}
                  <input
                    className="planEntryInput"
                    data-testid="plan-append-route-input"
                    value={routeEntryText}
                    spellCheck={false}
                    autoCapitalize="characters"
                    autoCorrect="off"
                    onChange={(event) => {
                      setRouteEntryText(event.target.value.toUpperCase());
                      setRouteEntryError(null);
                    }}
                  />
                </div>
              </form>
              {routeEntryError ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">{routeEntryError}</div>
              ) : routeEntryPreview.issues[0] ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">{routeEntryPreview.issues[0].message}</div>
              ) : routeEntryLoading ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">Checking...</div>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      <div className="planControls" ref={planControlsRef}>
        <button type="button" className="trayButton planControlButton" data-testid="plan-control-next-leg" disabled={!guidance?.can_activate_next_leg} onClick={() => void props.onActivateNextLeg()}>
          Next Leg
        </button>
        {guidance?.can_restore_direct_to ? (
          <button type="button" className="trayButton planControlButton" onClick={() => void props.onRestoreDirectTo()}>
            Restore FP
          </button>
        ) : null}
        <button
          type="button"
          className="trayButton planControlButton"
          data-testid="plan-control-sequence"
          disabled={!guidance?.can_sequence_active_leg}
          onClick={() => void props.onSequenceActiveLeg()}
        >
          Sequence
        </button>
        <button type="button" className="trayButton planControlButton" data-testid="plan-control-suspend" disabled={!guidance?.can_suspend} onClick={() => void props.onSuspendSequencing()}>
          Suspend
        </button>
        <button type="button" className="trayButton planControlButton" data-testid="plan-control-unsuspend" disabled={!guidance?.can_unsuspend} onClick={() => void props.onUnsuspendSequencing()}>
          Unsusp
        </button>
      </div>

      <div className="planFooter" ref={planFooterRef}>
        <NavElementButton
          navElement={planUiState.guidance?.nav_element}
          className="navElement navElementStatic"
          onClick={props.onOpenRecentChartOrPlate}
        />
      </div>

      {selectedRow !== null ? (
        <>
          <button
            type="button"
            className="trayScrim"
            aria-label="Close waypoint actions"
            onClick={() => {
              setSelectedWaypointUid(null);
              setSelectedWaypointAnchor(null);
              setAirwayPicker(null);
              setProcedurePicker(null);
              setAirportInsert(null);
            }}
          />
          <section
            ref={waypointModalRef}
            className={`waypointModal${airportInsert ? " isAirportInsert" : ""}`}
            aria-label="Waypoint actions"
            style={waypointModalTop === null ? undefined : {
              top: `${waypointModalTop}px`,
              maxHeight: waypointModalMaxHeight === null ? undefined : `${waypointModalMaxHeight}px`,
            }}
          >
            {airportInsert ? (
              <form
                className="waypointActionTray airportInsertTray"
                onSubmit={async (event) => {
                  event.preventDefault();
                  const airportId = airportInsert.airportId.trim().toUpperCase();
                  if (!airportId) {
                    setAirportInsert((current) => current ? { ...current, error: "Enter airport id" } : current);
                    return;
                  }
                  try {
                    await props.onInsertAirportWaypointAtRow(airportInsert.rowUid, airportInsert.before, airportId);
                    setAirportInsert(null);
                    setSelectedWaypointUid(null);
                  } catch (error) {
                    setAirportInsert((current) => current ? {
                      ...current,
                      error: error instanceof Error ? error.message : String(error),
                    } : current);
                  }
                }}
              >
                <div className="airportInsertInputRow">
                  <input
                    className="chartSearchInput airportInsertInput"
                    autoFocus
                    value={airportInsert.airportId}
                    placeholder={airportInsert.before ? "INSERT BEFORE" : "INSERT AFTER"}
                    spellCheck={false}
                    autoCapitalize="characters"
                    autoCorrect="off"
                    onChange={(event) => {
                      setAirportInsert((current) => current ? {
                        ...current,
                        airportId: event.target.value.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 8),
                        error: null,
                      } : current);
                    }}
                  />
                </div>
                {airportInsert.error ? <div className="planGuidanceSummary">{airportInsert.error}</div> : null}
                {airportInsert.loading ? <div className="planGuidanceSummary">Searching...</div> : null}
                {airportInsert.suggestions.length > 0 ? (
                  <div className="airportInsertSuggestions">
                    {airportInsert.suggestions.map((suggestion) => (
                      <button
                        key={`${suggestion.kind}:${suggestion.identifier}`}
                        type="button"
                        className="trayButton airwayChoiceButton planWaypointButton airportInsertSuggestion"
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          try {
                            await props.onInsertAirportWaypointAtRow(
                              airportInsert.rowUid,
                              airportInsert.before,
                              suggestion.identifier,
                            );
                            setAirportInsert(null);
                            setSelectedWaypointUid(null);
                          } catch (error) {
                            setAirportInsert((current) => current ? {
                              ...current,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        <WaypointButtonContent
                          label={suggestion.identifier}
                          symbolFeature={suggestion.symbol_feature}
                          details={[
                            waypointSuggestionName(suggestion),
                            waypointSuggestionDistance(suggestion),
                          ]}
                        />
                      </button>
                    ))}
                  </div>
                ) : null}
              </form>
            ) : procedurePicker ? (
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
                          const trace = {
                            row_uid: procedurePicker.rowUid,
                            airport_id: procedurePicker.airportId,
                            procedure_id: procedure.procedure_id,
                          };
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            const options = await debugTiming(
                              "plan.procedure_picker.describe_options",
                              () => props.appCoreAdapter!.describeProcedureOptions(
                                procedurePicker.airportId,
                                procedure.procedure_id,
                                "approach",
                              ),
                              trace,
                            );
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              selectedProcedureId: procedure.procedure_id,
                              options,
                            } : current);
                          } catch (error) {
                            debugLog("plan.procedure_picker.describe_options.error", { ...trace, message: errorMessage(error) });
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
                          const trace = {
                            row_uid: procedurePicker.rowUid,
                            airport_id: procedurePicker.airportId,
                            procedure_id: procedurePicker.selectedProcedureId,
                            enroute_transition: choice.enroute_transition,
                          };
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            await debugTiming(
                              "plan.procedure_picker.select_choice",
                              () => props.onSelectProcedureAtRow(
                                procedurePicker.rowUid,
                                procedurePicker.airportId,
                                procedurePicker.selectedProcedureId!,
                                choice.enroute_transition,
                              ),
                              trace,
                            );
                            setProcedurePicker(null);
                            setSelectedWaypointUid(null);
                          } catch (error) {
                            debugLog("plan.procedure_picker.select_choice.error", { ...trace, message: errorMessage(error) });
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
                          setAirwayPicker((current) => current ? { ...current, loading: true, error: null } : current);
                          try {
                            if (airwayPicker.rowUid !== null) {
                              await props.onInsertAirwayAtRow(
                                airwayPicker.rowUid,
                                selectedEntryIndex,
                                index,
                                presentation,
                              );
                            } else {
                              throw new Error("airway picker missing insertion row");
                            }
                            setAirwayPicker(null);
                            setSelectedWaypointUid(null);
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
            ) : (
              <div className="waypointActionGrid">
                {rowActionRows.map((row, rowIndex) => (
                  <div key={`row-${rowIndex}`} className="waypointActionGridRow">
                    {row.map((action) => (
                      <button
                        key={action.id}
                        type="button"
                        className="trayButton airwayChoiceButton"
                        data-testid={`plan-row-action-${action.id}`}
                        disabled={!action.enabled}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={action.onSelect}
                      >
                        {action.label}
                      </button>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </section>
        </>
      ) : null}
    </section>
  );
}

function ChartPlateToggleButton(props: {
  page: AppPage;
  onSelectPage: (page: AppPage) => void;
}) {
  const chartSelected = props.page === "map";
  const option = chartSelected
    ? pageOptions.find((entry) => entry.id === "map")
    : pageOptions.find((entry) => entry.id === "charts");
  const targetPage: AppPage = chartSelected ? "charts" : "map";
  return (
    <button
      type="button"
      className="chartButton pageToggleButton"
      data-testid={chartSelected ? "page-button-plate" : "page-button-chart"}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={() => props.onSelectPage(targetPage)}
      aria-label={chartSelected ? "Open plate page" : "Open chart page"}
    >
      {option?.iconSrc ? <img className="chartButtonIcon" src={option.iconSrc} alt="" aria-hidden="true" /> : null}
      <span className={`pageToggleTrack${chartSelected ? " isChart" : " isPlate"}`} aria-hidden="true">
        <span className="pageToggleKnob" />
      </span>
      <span className="chartButtonLabel">{option?.launcherLabel ?? (chartSelected ? "CHART" : "PLATE")}</span>
    </button>
  );
}

function mostRecentChartOrPlatePageFromHistory(pageHistory: AppViewSnapshot[]): AppPage {
  return pageHistory
    .slice()
    .reverse()
    .find((snapshot) => snapshot.page === "map" || snapshot.page === "charts")
    ?.page ?? "map";
}

function ChartPlateReturnButton(props: {
  targetPage: AppPage;
  onClick: () => void;
}) {
  const chartSelected = props.targetPage !== "charts";
  const option = chartSelected
    ? pageOptions.find((entry) => entry.id === "map")
    : pageOptions.find((entry) => entry.id === "charts");
  return (
    <button
      type="button"
      className="chartButton"
      data-testid={chartSelected ? "page-button-return-chart" : "page-button-return-plate"}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onClick}
      aria-label={chartSelected ? "Return to chart page" : "Return to plate page"}
    >
      {option?.iconSrc ? <img className="chartButtonIcon" src={option.iconSrc} alt="" aria-hidden="true" /> : null}
      <span className="chartButtonLabel">{option?.launcherLabel ?? (chartSelected ? "CHART" : "PLATE")}</span>
    </button>
  );
}

function HomeNavButton(props: {
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`chartButton${props.active ? " isOpen" : ""}`}
      data-testid="page-button-home"
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onClick}
      aria-label="Open home page"
    >
      <span className="chartButtonLabel">HOME</span>
    </button>
  );
}

function TrayDock(props: {
  launcherLabel: string;
  launcherImageSrc?: string;
  launcherStyle?: CSSProperties;
  open: boolean;
  onToggle: () => void;
  ariaLabel: string;
  disabled?: boolean;
  style?: TrayDockStyle;
  launcherClassName?: string;
  launcherAccentColor?: string;
  options: TrayOption[];
  footer?: ReactNode;
  testId?: string;
}) {
  const { launcherLabel, launcherImageSrc, launcherStyle, open, onToggle, ariaLabel, disabled = false, style = "compact", launcherClassName, launcherAccentColor, options, footer, testId } = props;
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const trayRef = useRef<HTMLElement | null>(null);
  const [trayPosition, setTrayPosition] = useState<{ left: number; top: number } | null>(null);
  const [trayThemeStyle, setTrayThemeStyle] = useState<CSSProperties | null>(null);
  const launcherWide = style === "plate_wide" || style === "wide" || style === "situation";
  const trayWide = style === "plate_narrow" || style === "plate_wide" || style === "wide";
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
        ["--theme-button-selected-bg" as string]: launcherStyle.getPropertyValue("--theme-button-selected-bg"),
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
        className={`chartButton${launcherWide ? " chartButtonWide" : ""}${launcherClassName ? ` ${launcherClassName}` : ""}${open ? " isOpen" : ""}${launcherDisabled ? " isDisabled" : ""}`}
        data-testid={testId}
        aria-disabled={launcherDisabled}
        style={{
          ...launcherStyle,
          ...(launcherAccentColor ? ({ ["--tray-accent" as string]: launcherAccentColor } as CSSProperties) : undefined),
        }}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={launcherDisabled ? undefined : onToggle}
      >
        {launcherImageSrc ? <img className="chartButtonIcon" src={launcherImageSrc} alt="" aria-hidden="true" /> : null}
        <span className={`chartButtonLabel${launcherWide ? " chartButtonLabelWide" : ""}`}>{launcherLabel}</span>
      </button>
      {open && typeof document !== "undefined"
        ? createPortal(
            <section
              ref={trayRef}
              className={`chartTray chartTrayPortal${trayWide ? " chartTrayWide" : ""}${style === "situation" ? " chartTraySituation" : ""} isOpen`}
              aria-label={ariaLabel}
              style={
                trayPosition
                  ? {
                    ...trayThemeStyle,
                    left: `${trayPosition.left}px`,
                    top: `${trayPosition.top}px`,
                    ...(style === "situation" ? ({ ["--situation-source-count" as string]: String(Math.max(1, options.length)) } as CSSProperties) : null),
                  }
                  : {
                    ...trayThemeStyle,
                    visibility: "hidden",
                    ...(style === "situation" ? ({ ["--situation-source-count" as string]: String(Math.max(1, options.length)) } as CSSProperties) : null),
                  }
              }
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
            >
              <div className={style === "situation" ? "situationSourceRow" : "trayOptions"}>
                {options.map((option) => (
                  <button
                    key={option.id}
                    type="button"
                    className={`trayButton${option.active ? " isActive" : ""}${option.iconSrc ? " trayButtonWithIcon" : ""}${option.toggleState ? " trayButtonHasToggle" : ""}${option.toggleState?.visible && option.toggleState.enabled ? " isOn" : ""}${option.toggleState && option.toggleState.enabled && !option.toggleState.visible ? " isOff" : ""}`}
                    data-testid={`tray-option-${option.id}`}
                    disabled={option.disabled}
                    style={option.accentColor ? ({ ["--tray-accent" as string]: option.accentColor } as CSSProperties) : undefined}
                    onPointerDown={stopPointer}
                    onPointerUp={stopPointer}
                    onDoubleClick={stopDoubleClick}
                    onClick={option.disabled ? undefined : option.onSelect}
                  >
                    {option.iconSrc || option.toggleState ? (
                      <span className="trayButtonContent">
                        {option.iconSrc ? (
                          <span className="trayButtonIconFrame" aria-hidden="true">
                            <img className="trayButtonIcon" src={option.iconSrc} alt="" />
                          </span>
                        ) : null}
                        <span className="trayButtonText">{option.label}</span>
                        {option.toggleState ? (
                          <span
                            className={`trayButtonToggle${option.toggleState.visible ? " isOn" : ""}${option.toggleState.enabled ? "" : " isDisabled"}`}
                            aria-hidden="true"
                          >
                            <span className="trayButtonToggleKnob" />
                          </span>
                        ) : null}
                      </span>
                    ) : option.label}
                  </button>
                ))}
              </div>
              {footer}
            </section>,
            document.body,
          )
        : null}
    </div>
  );
}

function ChartSearchBox(props: {
  state: {
    query: string;
    open: boolean;
    loading: boolean;
    error: string | null;
    suggestions: WaypointIdentifierSuggestion[];
  };
  onQueryChange: (query: string) => void;
  onFocus: () => void;
  onClose: () => void;
  onSubmit: () => void;
  onSelect: (suggestion: WaypointIdentifierSuggestion) => void;
}) {
  const { state, onQueryChange, onFocus, onClose, onSubmit, onSelect } = props;
  const showTray = state.open && (state.query.trim() || state.loading || state.error || state.suggestions.length > 0);

  return (
    <div className="chartSearch">
      <input
        className="chartSearchInput"
        data-testid="chart-search-input"
        type="text"
        value={state.query}
        placeholder="SEARCH"
        autoCapitalize="characters"
        autoCorrect="off"
        spellCheck={false}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onFocus={onFocus}
        onChange={(event) => onQueryChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === "Enter") {
            event.preventDefault();
            onSubmit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      />
      {showTray ? (
        <section
          className="chartSearchTray"
          aria-label="Waypoint search results"
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
        >
          {state.loading ? <div className="chartSearchStatus">Searching...</div> : null}
          {state.error ? <div className="chartSearchStatus isError">{state.error}</div> : null}
          {!state.loading && !state.error && state.suggestions.length === 0 ? (
            <div className="chartSearchStatus">No matches</div>
          ) : null}
          {state.suggestions.map((suggestion) => (
            <button
              key={`${suggestion.kind}:${suggestion.identifier}`}
              type="button"
              className="trayButton airwayChoiceButton planWaypointButton airportInsertSuggestion chartSearchSuggestion"
              data-testid={`chart-search-suggestion-${suggestion.identifier}`}
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => onSelect(suggestion)}
            >
              <WaypointButtonContent
                label={suggestion.identifier}
                symbolFeature={suggestion.symbol_feature}
                details={[
                  waypointSuggestionName(suggestion),
                  waypointSuggestionDistance(suggestion),
                ]}
              />
            </button>
          ))}
        </section>
      ) : null}
    </div>
  );
}

function MapSelectionTray(props: {
  point: ScreenPoint;
  result: MapSelectionQueryResult;
  selectedItem: MapSelectionItem | null;
  onSelectItem: (item: MapSelectionItem) => void;
  onSelectDetail: (title: string, text: string) => void;
  onSelectAction: (item: MapSelectionItem, action: MapSelectionItem["actions"][number]) => void | Promise<void>;
}) {
  const { point, result, selectedItem, onSelectItem, onSelectDetail, onSelectAction } = props;
  const edgePad = thumbPixels(0.1);
  type MapSelectionActionSlot = MapSelectionItem["actions"][number] & { placeholder?: boolean };
  const actionSlots: MapSelectionActionSlot[] = selectedItem
    ? [...selectedItem.actions, ...Array.from({ length: Math.max(0, 6 - selectedItem.actions.length) }, (_, index) => ({
      id: `placeholder-${index}`,
      label: "",
      enabled: false,
      display_only: true,
      placeholder: true,
    }))]
    : Array.from({ length: 6 }, (_, index) => ({
      id: `placeholder-${index}`,
      label: "",
      enabled: false,
      display_only: true,
      placeholder: true,
	    }));
  const visibleActionSlots = selectedItem?.detail_text
    ? actionSlots.slice(0, 3)
    : actionSlots.slice(0, 6);
  const horizontalStyle = point.x < window.innerWidth / 2
    ? { right: `${edgePad}px` }
    : { left: `${edgePad}px` };
  const verticalStyle = point.y < window.innerHeight / 2
    ? { bottom: `${edgePad}px` }
    : { top: `${edgePad}px` };

  return (
    <section
      className="mapSelectionTray"
      data-testid="map-selection-tray"
      style={{ ...horizontalStyle, ...verticalStyle }}
      aria-label="Map selection"
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
    >
      {result.categories.map((category) => (
        <div key={category.id} className="mapSelectionCategory">
          <div className="mapSelectionRow">
            {category.items.length === 0 ? (
              <div className="mapSelectionEmpty" aria-hidden="true">
                no {category.label.toLowerCase()}s
              </div>
            ) : category.items.map((item) => (
              <button
                key={item.id}
                type="button"
                className={`mapSelectionItem${selectedItem?.id === item.id ? " isSelected" : ""}`}
                data-testid={`map-selection-item-${category.id}-${item.label}`}
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
                onDoubleClick={stopDoubleClick}
                onClick={() => onSelectItem(item)}
                title={item.sublabel}
              >
                <MapSelectionItemIcon item={item} />
                <span className="mapSelectionItemLabel">{item.label}</span>
              </button>
            ))}
          </div>
        </div>
      ))}
      <div className="mapSelectionActions">
        <div className="mapSelectionActionTitle">
          {selectedItem ? (
            <>
              <strong>{selectedItem.label}</strong>
              {selectedItem.description ? (
                <span className="mapSelectionActionDescription"> · {selectedItem.description}</span>
              ) : null}
            </>
          ) : "\u00a0"}
        </div>
        <div className="mapSelectionActionGrid">
          {visibleActionSlots.map((action) => (
            <button
              key={action.id}
              type="button"
              className={`mapSelectionAction${action.display_only ? " isDisplayOnly" : ""}${action.placeholder ? " isPlaceholder" : ""}`}
              data-testid={action.placeholder ? undefined : `map-selection-action-${action.id}`}
              disabled={!action.enabled}
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => {
                if (selectedItem && action.enabled && action.detail_text) {
                  onSelectDetail(action.label, action.detail_text);
                  return;
                }
                if (selectedItem && action.enabled && !action.display_only) {
                  void onSelectAction(selectedItem, action);
                }
              }}
              aria-hidden={action.placeholder ? "true" : undefined}
              tabIndex={action.placeholder ? -1 : undefined}
            >
              {action.airspace_limit ? (
                <svg className="mapSelectionAirspaceLimitGlyph" viewBox="-32 -32 64 64" aria-hidden="true">
                  <AirspaceLimitGlyph glyph={action.airspace_limit} scale={1.45} />
                </svg>
              ) : action.label}
            </button>
          ))}
          {selectedItem?.detail_text ? (
            <div className="mapSelectionDetailText mapSelectionInlineDetailText">{selectedItem.detail_text}</div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function MapSelectionDetailModal(props: { title: string; text: string }) {
  return (
    <section
      className="mapSelectionDetailModal"
      aria-label={props.title}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
    >
      <div className="mapSelectionDetailTitle">{props.title}</div>
      <div className="mapSelectionDetailText">{props.text}</div>
    </section>
  );
}

function AirspaceLimitGlyph(props: { glyph: { upper: string; lower: string; style_key: string; color_key: string }; scale?: number }) {
  const { glyph, scale = 1 } = props;
  const color = aviationThemeColor(glyph.color_key);
  const labelStyle = { fill: color, fontSize: `${14 * scale}px` };
  const parts = { upper: glyph.upper, lower: glyph.lower };
  const dividerWidth = airspaceLabelDividerWidth(parts) * scale;
  return (
    <g className={`airspaceLabel-${glyph.style_key}`} style={{ "--airspace-label-color": color } as React.CSSProperties}>
      <text className="airspaceLabel" style={labelStyle} x="0" y={-7 * scale}>
        {parts.upper}
      </text>
      <line
        className="airspaceLabelDividerContrast"
        x1={-dividerWidth / 2}
        y1="0"
        x2={dividerWidth / 2}
        y2="0"
      />
      <line
        className="airspaceLabelDivider"
        x1={-dividerWidth / 2}
        y1="0"
        x2={dividerWidth / 2}
        y2="0"
      />
      <text className="airspaceLabel" style={labelStyle} x="0" y={9 * scale}>
        {parts.lower}
      </text>
    </g>
  );
}

function MapSelectionItemIcon(props: { item: MapSelectionItem }) {
  const { item } = props;
  if (item.symbol_feature) {
    return (
      <span className="mapSelectionItemIcon" aria-hidden="true">
        <PlanWaypointSymbol feature={item.symbol_feature} />
      </span>
    );
  }
  if (item.metar_feature) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionMetarIcon" viewBox="-20 -20 40 40" aria-hidden="true">
        <MetarSymbol feature={item.metar_feature} />
      </svg>
    );
  }
  if (item.pirep_feature) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionMetarIcon" viewBox="-34 -34 74 78" aria-hidden="true">
        <PirepSymbol feature={item.pirep_feature} />
      </svg>
    );
  }
  if (item.highlight.kind === "spot") {
    return (
      <svg className="mapSelectionItemIcon mapSelectionSpotIcon" viewBox="-20 -40 40 46" aria-hidden="true">
        <MapSelectionSpotSymbol />
      </svg>
    );
  }
  if (item.airspace_icon) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionAirspaceIcon" viewBox="0 0 64 64" aria-hidden="true">
        <AirspaceDisplayPathGroup feature={item.airspace_icon} />
      </svg>
    );
  }
  return <span className="mapSelectionItemTextIcon">{item.sublabel || item.label}</span>;
}

function ChartsPage(props: {
  appCoreAdapter: AppCoreAdapter | null;
  page: AppPage;
  plan: FlightPlan;
  planUiState: FlightPlanUiState | null;
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
  playbackUiState: PlaybackUiState;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  onSituationControlInput: (input: SituationControlInput) => void;
  debugState: UiDebugState;
  uiSession: UiSession | null;
  ownship: OwnshipRenderState;
  ownshipControls: OwnshipControlModel;
  debugWarningActive: boolean;
  onFirstVisualReady: () => void;
}) {
  const { appCoreAdapter, page, plan, planUiState, airports, selectedAirport, selectedChart, folderOpen, viewport, onViewportChange, onFolderOpenChange, onSelectPage, onOpenPlan, onSelectAirport, onSelectChart, uiSession, ownship, ownshipControls, onFirstVisualReady } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [imageSize, setImageSize] = useState<{ chartId: string; width: number; height: number } | null>(null);
  const viewportRef = useRef<ImageViewportState | null>(null);
  const lastLocalViewportRef = useRef<ImageViewportState | null>(null);
  const wheelGestureUntilRef = useRef(0);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<{ viewport: ImageViewportState; distance: number; midpoint: ScreenPoint } | null>(null);
  const statusControlDockLowered = shouldLowerStatusControlDock(surfaceSize.width, false);
  const lastChartLayoutKeyRef = useRef("");
  const firstVisualReadyRef = useRef(false);
  const trayGroup = useModalTrayGroup(["airport", "chart", "load", "ownship"] as const);
  const [plateProcedureLoads, setPlateProcedureLoads] = useState<ProcedureLoadOption[]>([]);
  const [resolvedChartUrls, setResolvedChartUrls] = useState<Record<string, ResolvedChartUrls>>({});
  const trayOpen = trayGroup.scrimOpen;
  const sortedCharts = selectedAirport?.charts ?? [];
  const selectedImageSize = imageSize && imageSize.chartId === (selectedChart?.id ?? "") ? imageSize : null;
  const selectedChartAssetUrl = selectedChart ? resolvedChartUrls[selectedChart.id]?.assetUrl ?? null : null;
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
  const plateOwnshipOverlay = useMemo(
    () => resolvePlateOwnshipOverlay(ownship, selectedChart?.georef ?? null, selectedImageSize, effectiveViewport, displaySize),
    [displaySize, effectiveViewport, ownship, selectedChart?.georef, selectedImageSize],
  );

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
    if (!selectedChart || !uiSession) {
      return;
    }
    let cancelled = false;
    void uiSession.resolvePackageMemberUrl(selectedChart.package_id, selectedChart.asset_path)
      .then((assetUrl) => {
        if (cancelled) {
          return;
        }
        setResolvedChartUrls((current) => ({
          ...current,
          [selectedChart.id]: {
            ...current[selectedChart.id],
            assetUrl,
          },
        }));
      })
      .catch((error) => {
        debugLog("charts.asset.resolve_failed", {
          chart_id: selectedChart.id,
          package_id: selectedChart.package_id,
          asset_path: selectedChart.asset_path,
          error: errorMessage(error),
        });
        if (!cancelled) {
          setResolvedChartUrls((current) => ({
            ...current,
            [selectedChart.id]: {
              ...current[selectedChart.id],
              assetUrl: null,
            },
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [selectedChart?.id, selectedChart?.package_id, selectedChart?.asset_path, uiSession]);

  useEffect(() => {
    if (!folderOpen || !uiSession) {
      return;
    }
    let cancelled = false;
    const chartsToResolve = sortedCharts.filter((chart) =>
      chart.thumbnail_path && resolvedChartUrls[chart.id]?.thumbnailUrl === undefined,
    );
    if (chartsToResolve.length === 0) {
      return;
    }
    void Promise.all(chartsToResolve.map(async (chart) => {
      try {
        return {
          chart,
          thumbnailUrl: chart.thumbnail_path
            ? await uiSession.resolvePackageMemberUrl(chart.package_id, chart.thumbnail_path)
            : null,
        };
      } catch (error) {
        debugLog("charts.thumbnail.resolve_failed", {
          chart_id: chart.id,
          package_id: chart.package_id,
          thumbnail_path: chart.thumbnail_path,
          error: errorMessage(error),
        });
        return { chart, thumbnailUrl: null };
      }
    })).then((resolved) => {
      if (cancelled) {
        return;
      }
      setResolvedChartUrls((current) => {
        const next = { ...current };
        for (const { chart, thumbnailUrl } of resolved) {
          next[chart.id] = {
            ...next[chart.id],
            thumbnailUrl,
          };
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [folderOpen, resolvedChartUrls, sortedCharts, uiSession]);

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
  }, [selectedChart?.id, selectedChartAssetUrl]);

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
    }).catch((error: unknown) => {
      debugLog("charts.load_procedure.unavailable", {
        plate_id: selectedChart.id,
        error: errorMessage(error),
      });
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
        id: `${load.load_id}:${index}`,
        label: load.label,
        active: false,
        onSelect: () => {
          if (!props.uiSession) {
            return;
          }
          void props.uiSession.loadPlateProcedure(load.load_id).then(() => {
            trayGroup.close("load");
          }).catch(() => {});
        },
      }));
  }, [plateProcedureLoads, props, trayGroup]);
  const ownshipSourceOptions: TrayOption[] = ownshipControls.sources.map((source) => ({
    id: sourceIdString(source.source_id),
    label: source.label,
    active: source.active,
    disabled: !source.enabled || !props.uiSession,
    onSelect: () => {
      if (!props.uiSession) {
        return;
      }
      void props.uiSession
        .selectOwnshipSource({ kind: "source", source_id: sourceIdString(source.source_id) })
        .then(props.onPlaybackSnapshotChange)
        .finally(() => trayGroup.close("ownship"));
    },
  }));
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
        viewport: viewportRef.current,
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
        pinchRef.current.viewport,
        pinchRef.current.midpoint.x,
        pinchRef.current.midpoint.y,
        clampImageZoom(pinchRef.current.viewport.zoom + zoomDelta),
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

  function reportFirstVisualReady() {
    if (firstVisualReadyRef.current) {
      return;
    }
    firstVisualReadyRef.current = true;
    onFirstVisualReady();
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
        <StatusControlDock
          controls={ownshipControls}
          lowered={statusControlDockLowered}
          ownshipOpen={trayGroup.isOpen("ownship")}
          onOwnshipToggle={() => trayGroup.toggle("ownship")}
          options={ownshipSourceOptions}
          transportControls={<SituationTransportRow controls={ownshipControls.situation_controls} onInput={props.onSituationControlInput} />}
        />
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
                  {resolvedChartUrls[chart.id]?.thumbnailUrl ? (
                    <img
                      className="plateThumbImage"
                      src={resolvedChartUrls[chart.id]?.thumbnailUrl ?? undefined}
                      alt=""
                      draggable={false}
                    />
                  ) : null}
                  <div className="plateThumbLabel" style={{ backgroundColor: plateFolderColor(chart.folder_category) }}>
                    {chart.label}
                  </div>
                </div>
              </button>
            ))}
          </div>
        ) : selectedChart && selectedChartAssetUrl ? (
          <>
            <img
              key={selectedChart.id}
              ref={imageRef}
              className="chartImage"
              src={selectedChartAssetUrl}
              alt={selectedChart.label}
              draggable={false}
              onLoad={(event) =>
                {
                  setImageSize({
                    chartId: selectedChart.id,
                    width: event.currentTarget.naturalWidth,
                    height: event.currentTarget.naturalHeight,
                  });
                  reportFirstVisualReady();
                }
              }
              style={{
                left: `${selectedImageSize && effectiveViewport ? effectiveViewport.left : 0}px`,
                top: `${selectedImageSize && effectiveViewport ? effectiveViewport.top : 0}px`,
                width: displaySize ? `${displaySize.width}px` : undefined,
                height: displaySize ? `${displaySize.height}px` : undefined,
                visibility: selectedImageSize && effectiveViewport ? "visible" : "hidden",
              }}
            />
            {plateOwnshipOverlay ? (
              <SituationAircraft
                iconSrc={planViewIcon}
                point={plateOwnshipOverlay.point}
                headingDeg={plateOwnshipOverlay.headingDeg}
              />
            ) : null}
          </>
        ) : null}

        <div className="chartDock chartDockDouble plateDock">
          <HomeNavButton active={page === "home"} onClick={() => onSelectPage("home")} />
          <ChartPlateToggleButton page={page} onSelectPage={onSelectPage} />
          <TrayDock
            launcherLabel={selectedAirport?.id ?? "---"}
            open={trayGroup.isOpen("airport")}
            onToggle={() => trayGroup.toggle("airport")}
            ariaLabel="Airport"
            style="plate_narrow"
            testId="plate-airport-button"
            options={airports.map((airport) => ({
              id: airport.id,
              label: airport.id,
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
            launcherClassName="plateChartSelector"
            launcherAccentColor={selectedChart ? plateFolderColor(selectedChart.folder_category) : undefined}
            onToggle={() => trayGroup.toggle("chart")}
            ariaLabel="Chart"
            style="plate_wide"
            testId="plate-chart-button"
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
            testId="plate-load-button"
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
            data-testid="plate-folder-button"
          >
            <span className="chartButtonLabel">FLDR</span>
          </button>
        </div>

        <NavElementButton
          navElement={planUiState?.guidance?.nav_element}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        />

        {props.debugState.playback_visible ? (
          <PlaybackWidget
            uiSession={props.uiSession}
            playbackUiState={props.playbackUiState}
            sourcePath={props.playbackSourcePath}
            onSourcePathChange={props.onPlaybackSourcePathChange}
            onSnapshotChange={props.onPlaybackSnapshotChange}
            surfaceWidth={surfaceSize.width}
            dock="left"
          />
        ) : null}
      </div>
    </section>
  );
}

function HomePage(props: {
  page: AppPage;
  planUiState: FlightPlanUiState | null;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  debugWarningActive: boolean;
}) {
  const { page, planUiState, onSelectPage, onOpenPlan } = props;
  const homeButtons: Array<{ id: string; label: string; page: AppPage; iconSrc?: string }> = [
    { id: "chart", label: "CHART", page: "map", iconSrc: PAGE_CHART_ICON_SRC },
    { id: "plate", label: "PLATE", page: "charts", iconSrc: PAGE_PLATE_ICON_SRC },
    { id: "flight-plan", label: "FLIGHT\nPLAN", page: "plan" },
    { id: "data-status", label: "DATA\nSTATUS", page: "data" },
  ];
  const placeholderLabels = ["S5", "S6", "S7", "S8", "S9"];

  return (
    <section className="appPage planPage">
      <div className="chartDock">
        <HomeNavButton active={true} onClick={() => {}} />
        <ChartPlateReturnButton
          targetPage={props.mostRecentChartOrPlatePage}
          onClick={props.onOpenRecentChartOrPlate}
        />
      </div>

      <div className="homeGrid" aria-label="Home navigation">
        {homeButtons.map((button) => (
          <button
            key={button.id}
            type="button"
            className={`chartButton chartButtonDouble homeButton${button.page === page ? " isOpen" : ""}`}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={() => onSelectPage(button.page)}
          >
            {button.iconSrc ? <img className="chartButtonIcon" src={button.iconSrc} alt="" aria-hidden="true" /> : null}
            <span className="chartButtonLabel chartButtonLabelDouble">{button.label}</span>
          </button>
        ))}
        {placeholderLabels.map((label) => (
          <button key={label} type="button" className="chartButton chartButtonDouble homeButton" disabled>
            <span className="chartButtonLabel chartButtonLabelDouble">{label}</span>
          </button>
        ))}
      </div>

      <NavElementButton
        navElement={planUiState?.guidance?.nav_element}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={onOpenPlan}
      />
    </section>
  );
}

function DataStatusPage(props: {
  page: AppPage;
  state: UiDataStatusPageState;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
}) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    if (props.page !== "data") {
      return;
    }
    setNowMs(Date.now());
    const timer = window.setInterval(() => setNowMs(Date.now()), 10_000);
    return () => window.clearInterval(timer);
  }, [props.page]);

  return (
    <section className="appPage dataStatusPage">
      <div className="chartDock">
        <HomeNavButton active={props.page === "home"} onClick={() => props.onSelectPage("home")} />
        <ChartPlateReturnButton
          targetPage={props.mostRecentChartOrPlatePage}
          onClick={props.onOpenRecentChartOrPlate}
        />
      </div>

      <div className="dataStatusPagePanel" aria-label={props.state.title}>
        <header className="dataStatusPageHeader">
          <h1>{props.state.title}</h1>
          <p>{props.state.summary}</p>
        </header>
        <div className="dataStatusPageRows">
          {props.state.rows.map((row) => (
            <article key={row.id} className={`dataStatusPageRow statusSeverity-${row.severity}`}>
              <div className="dataStatusPageRowHeader">
                <span className="dataStatusPageRowLabel">{row.label}</span>
                <span className="dataStatusPageRowValue">{row.value}</span>
              </div>
              <div className="dataStatusPageRowDetail">{row.detail}</div>
              {row.facts.length > 0 ? (
                <dl className="dataStatusPageFacts">
                  {row.facts.map((fact) => (
                    <div key={`${row.id}:${fact.label}`} className="dataStatusPageFact">
                      <dt>{fact.label}</dt>
                      <dd>{formatDataStatusFactValue(fact, nowMs)}</dd>
                    </div>
                  ))}
                </dl>
              ) : null}
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

function formatDataStatusFactValue(
  fact: UiDataStatusPageState["rows"][number]["facts"][number],
  nowMs: number,
) {
  if (!fact.time_utc || !fact.time_display) {
    return fact.value;
  }
  const instantMs = Date.parse(fact.time_utc);
  if (!Number.isFinite(instantMs)) {
    return fact.value;
  }
  const suffix = dataStatusRelativeTimeSuffix(instantMs, nowMs, fact.time_display);
  return suffix ? `${fact.value} (${suffix})` : fact.value;
}

function dataStatusRelativeTimeSuffix(
  instantMs: number,
  nowMs: number,
  display: NonNullable<UiDataStatusPageState["rows"][number]["facts"][number]["time_display"]>,
) {
  const deltaMs = instantMs - nowMs;
  const magnitude = formatDataStatusDuration(Math.abs(deltaMs));
  if (display === "old") {
    return `${magnitude} old`;
  }
  if (display === "until") {
    return deltaMs >= 0 ? `in ${magnitude}` : `${magnitude} ago`;
  }
  return deltaMs >= 0 ? `in ${magnitude}` : `${magnitude} ago`;
}

function formatDataStatusDuration(durationMs: number) {
  const minutes = Math.floor(durationMs / 60_000);
  if (minutes < 60) {
    return `${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 48) {
    return `${hours}h`;
  }
  const days = Math.floor(hours / 24);
  if (days < 60) {
    return `${days}d`;
  }
  const months = Math.floor(days / 30);
  if (months < 24) {
    return `${months}mo`;
  }
  return `${Math.floor(days / 365)}y`;
}

function CommonDebugPanel(props: {
  uptimeLabel: string;
  debugState: UiDebugState;
  onDebugFlagChange: (flagId: DebugFlagId, enabled: boolean) => void;
  extraLines?: string[];
  onRunDragPerf?: () => void;
  dragPerfRunning?: boolean;
  lastDragPerfRunId?: string | null;
}) {
  const flags: Array<{ id: DebugFlagId; label: string }> = [
    { id: "tile_labels", label: "tile labels" },
    { id: "nexrad_tile_labels", label: "NEXRAD tile labels" },
    { id: "fast_tiles", label: "fast tiles" },
    { id: "offline_simulated_clock_buttons", label: "offline simulated clock buttons" },
    { id: "sequencing_finish_lines", label: "sequencing finish lines" },
  ];

  return (
    <>
      <div className="debugLine">up: {props.uptimeLabel}</div>
      {(props.extraLines ?? []).map((line) => (
        <div key={line} className="debugLine">{line}</div>
      ))}
      {props.lastDragPerfRunId ? (
        <div className="debugLine">drag run: {props.lastDragPerfRunId}</div>
      ) : null}
      {props.onRunDragPerf ? (
        <button
          type="button"
          className="debugActionButton"
          disabled={props.dragPerfRunning}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={props.onRunDragPerf}
        >
          {props.dragPerfRunning ? "drag perf running" : "run drag perf"}
        </button>
      ) : null}
      {flags.map((flag) => (
        <label key={flag.id} className="debugToggle">
          <input
            type="checkbox"
            checked={props.debugState[flag.id]}
            onChange={(event) => props.onDebugFlagChange(flag.id, event.currentTarget.checked)}
          />
          {flag.label}
        </label>
      ))}
    </>
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

function ZoomControl(props: { zoom: number; minZoom: number; maxZoom: number; onZoomChange: (zoom: number) => void }) {
  const step = 0.05;
  const buttonStep = 0.5;
  const zoom = Math.min(props.maxZoom, Math.max(props.minZoom, props.zoom));

  return (
    <div
      className="zoomControl"
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
    >
      <button
        type="button"
        className="zoomControlButton"
        aria-label="Zoom out"
        onClick={() => props.onZoomChange(zoom - buttonStep)}
      >
        −
      </button>
      <input
        className="zoomControlSlider"
        type="range"
        aria-label="Map zoom"
        min={props.minZoom}
        max={props.maxZoom}
        step={step}
        value={zoom}
        onChange={(event) => props.onZoomChange(Number(event.currentTarget.value))}
      />
      <button
        type="button"
        className="zoomControlButton"
        aria-label="Zoom in"
        onClick={() => props.onZoomChange(zoom + buttonStep)}
      >
        ＋
      </button>
    </div>
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
    const rawPage = (parsed as { page?: string }).page;
    const page = rawPage === "settings" ? "home" : rawPage;
    return {
      page: page === "map" || page === "plan" || page === "charts" || page === "home" || page === "data" ? page : undefined,
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

function formatMegabytesPerSecond(bytesPerSecond: number) {
  return `${(bytesPerSecond / (1024 * 1024)).toFixed(3)} MiB/s`;
}

function moveAirportToFront(
  currentIds: string[],
  airportId: string,
  airports: ChartPageData["airports"],
) {
  const mergedIds = mergeRecentAirportIds(airports, [airportId, ...currentIds.filter((id) => id !== airportId)]);
  return mergedIds.includes(airportId) ? mergedIds : [airportId, ...mergedIds];
}

function plateFolderColor(category: PlateFolderCategory) {
  return plateFolderTheme.label_colors[category as keyof typeof plateFolderTheme.label_colors] ?? plateFolderTheme.label_colors.other ?? "#52656d";
}

function airportIdFromNavRef(navRef: NavRef | null | undefined): string | null {
  return navRef && "Airport" in navRef ? navRef.Airport : null;
}

function SituationStatusBadge(props: {
  controls: OwnshipControlModel;
  open: boolean;
  onToggle: () => void;
  options: TrayOption[];
  transportControls?: ReactNode;
}) {
  return (
    <TrayDock
      launcherLabel={props.controls.launcher_label}
      launcherClassName={`situationStatusLauncher situationStatus-${props.controls.launcher_tone}`}
      open={props.open}
      onToggle={props.onToggle}
      ariaLabel="Ownship source"
      style="situation"
      options={props.options}
      footer={props.transportControls}
    />
  );
}

function StatusControlDock(props: {
  controls: OwnshipControlModel;
  dataStatusState?: UiDataStatusState | null;
  lowered?: boolean;
  ownshipOpen: boolean;
  statusOpen?: boolean;
  onOwnshipToggle: () => void;
  onStatusToggle?: () => void;
  onAction?: (actionId: string) => void | Promise<void>;
  options: TrayOption[];
  transportControls?: ReactNode;
}) {
  return (
    <div className={`statusControlDock${props.lowered ? " isLowered" : ""}`}>
      {props.dataStatusState && props.onStatusToggle && props.onAction ? (
        <DataStatusDock
          dataStatusState={props.dataStatusState}
          open={props.statusOpen ?? false}
          onToggle={props.onStatusToggle}
          onAction={props.onAction}
        />
      ) : null}
      <SituationStatusBadge
        controls={props.controls}
        open={props.ownshipOpen}
        onToggle={props.onOwnshipToggle}
        options={props.options}
        transportControls={props.transportControls}
      />
    </div>
  );
}

function DataStatusDock(props: {
  dataStatusState: UiDataStatusState;
  lowered?: boolean;
  open: boolean;
  onToggle: () => void;
  onAction: (actionId: string) => void | Promise<void>;
}) {
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);
  const [panelPosition, setPanelPosition] = useState<{ left: number; top: number } | null>(null);
  const launcherCount = props.dataStatusState.launcher_count;
  const hasLauncherCount = launcherCount != null;
  const hasStatus = props.dataStatusState.boxes.length > 0;
  useEffect(() => {
    if (!props.open) {
      setPanelPosition(null);
      return;
    }

    function updatePosition() {
      const launcher = launcherRef.current;
      const panel = panelRef.current;
      if (!launcher || !panel) {
        return;
      }
      const launcherRect = launcher.getBoundingClientRect();
      const gap = thumbPixels(0.1);
      const minInset = thumbPixels(0.1);
      const maxLeft = Math.max(minInset, window.innerWidth - panel.offsetWidth - minInset);
      const maxTop = Math.max(minInset, window.innerHeight - panel.offsetHeight - minInset);
      setPanelPosition({
        left: Math.min(Math.max(minInset, launcherRect.right - panel.offsetWidth), maxLeft),
        top: Math.min(Math.max(minInset, launcherRect.bottom + gap), maxTop),
      });
    }

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [props.open, props.dataStatusState.boxes.length]);

  if (!hasStatus) {
    return null;
  }
  const severity = props.dataStatusState.launcher_severity;

  return (
    <div className="dataStatusDock">
      <button
        ref={launcherRef}
        type="button"
        className={`dataStatusLauncher statusSeverity-${severity}${props.open ? " isOpen" : ""}${hasLauncherCount ? "" : " isQuiet"}`}
        aria-expanded={props.open}
        aria-label="Data status"
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={props.onToggle}
      >
        <svg className="dataStatusLauncherSymbol" viewBox="-50 -50 100 100" aria-hidden="true" focusable="false">
          <RenderNavSymbolLayers layers={dataStatusWarningSymbol} />
        </svg>
        {hasLauncherCount ? <span className="dataStatusLauncherCount">{launcherCount}</span> : null}
      </button>
      {props.open && typeof document !== "undefined" ? createPortal(
        <section
          ref={panelRef}
          className="dataStatusPanel"
          aria-label="Active data status"
          style={panelPosition ? {
            left: `${panelPosition.left}px`,
            top: `${panelPosition.top}px`,
          } : {
            visibility: "hidden",
          }}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
        >
          {props.dataStatusState.boxes.map((box) => (
            <div key={box.id} className={`dataStatusBox statusSeverity-${box.severity}${box.hushed ? " isHushed" : ""}`}>
              <div className="dataStatusBoxHeader">
                <span className="dataStatusBoxLabel">{box.label}</span>
                <span className="dataStatusBoxValue">{box.value ?? "—"}</span>
              </div>
              <div className="dataStatusBoxDetail">{box.detail}</div>
              {box.actions.length > 0 ? (
                <div className="dataStatusActions">
                  {box.actions.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      className={`dataStatusAction dataStatusAction-${action.style}`}
                      disabled={!action.enabled}
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onDoubleClick={stopDoubleClick}
                      onClick={action.enabled ? () => props.onAction(action.id) : undefined}
                    >
                      {action.label}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </section>,
        document.body,
      ) : null}
    </div>
  );
}

function SituationTransportRow(props: {
  controls: OwnshipControlModel["situation_controls"];
  onInput: (input: SituationControlInput) => void;
}) {
  return (
    <div className="situationTransportRow" role="group" aria-label="Plan preview and replay controls">
      {props.controls.map((button) => (
        <button
          key={button.input}
          type="button"
          className="trayButton trayButtonSquare situationTransportButton"
          aria-label={button.label}
          title={button.label}
          disabled={!button.enabled}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={button.enabled ? () => props.onInput(button.input) : undefined}
        >
          {button.label}
        </button>
      ))}
    </div>
  );
}

function situationInputForKey(key: string): SituationControlInput | null {
  switch (key) {
    case "<":
      return "skip_backward";
    case "(":
      return "fast_rewind";
    case ")":
      return "fast_forward";
    case ">":
      return "skip_forward";
    default:
      return null;
  }
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName === "input" || tagName === "textarea" || tagName === "select" || target.isContentEditable;
}

function sourceIdString(sourceId: { 0: string } | string): string {
  return typeof sourceId === "string" ? sourceId : sourceId[0];
}

function SituationAircraftSvg(props: {
  iconSrc: string;
  point: { x: number; y: number };
  headingDeg: number;
  sizePx: number;
}) {
  const half = props.sizePx / 2;
  return (
    <g transform={`translate(${props.point.x} ${props.point.y}) rotate(${props.headingDeg})`}>
      <image
        href={props.iconSrc}
        x={-half}
        y={-half}
        width={props.sizePx}
        height={props.sizePx}
        preserveAspectRatio="xMidYMid meet"
        style={{
          pointerEvents: "none",
          userSelect: "none",
          filter: "drop-shadow(0 1px 1px rgba(18, 26, 33, 0.45))",
        }}
      />
    </g>
  );
}

function SituationAircraft(props: {
  iconSrc: string;
  point: { x: number; y: number };
  headingDeg: number;
}) {
  return (
    <img
      src={props.iconSrc}
      alt=""
      draggable={false}
      style={{
        position: "absolute",
        zIndex: 2,
        width: `calc(var(--thumb) * 1.44)`,
        height: "auto",
        pointerEvents: "none",
        userSelect: "none",
        filter: "drop-shadow(0 1px 1px rgba(18, 26, 33, 0.45))",
        left: `${props.point.x}px`,
        top: `${props.point.y}px`,
        transform: `translate(-50%, -50%) rotate(${props.headingDeg}deg)`,
      }}
    />
  );
}

function resolveSituationOverlay(
  ownship: OwnshipRenderState,
  viewport: MapViewportState,
  width: number,
  height: number,
  ringCandidates: SituationRingCandidate[],
) {
  if (width <= 0 || height <= 0 || !ownship.draw_aircraft || !ownship.position) {
    return null;
  }
  const point = latLonToScreen(ownship.position.lat, ownship.position.lon, viewport, width, height);
  const headingDeg = ownship.orientation_deg ?? 0;
  const ring = selectSituationRing(
    ownship.position.lat,
    ownship.position.lon,
    viewport,
    width,
    height,
    ringCandidates,
    ownship.magnetic_variation_deg,
  );
  const ahead =
    ownship.draw_predictor && ownship.speed_kt !== null
      ? projectAhead(ownship.position.lat, ownship.position.lon, headingDeg, ownship.speed_kt / 60)
      : null;
  const predictor = ahead ? latLonToScreen(ahead.lat, ahead.lon, viewport, width, height) : null;
  return { point, predictor, headingDeg, ring };
}

function resolvePlateOwnshipOverlay(
  ownship: OwnshipRenderState,
  georef: PlateGeoref | null,
  imageSize: { chartId: string; width: number; height: number } | null,
  viewport: ImageViewportState | null,
  displaySize: { width: number; height: number } | null,
) {
  if (!ownship.draw_aircraft || !ownship.position || !georef || !imageSize || !viewport || !displaySize) {
    return null;
  }
  const imagePoint = plateImagePoint(ownship.position, georef);
  if (!imagePoint) {
    return null;
  }
  if (imagePoint.x < 0 || imagePoint.x > imageSize.width || imagePoint.y < 0 || imagePoint.y > imageSize.height) {
    return null;
  }
  const scaleX = displaySize.width / imageSize.width;
  const scaleY = displaySize.height / imageSize.height;
  return {
    point: {
      x: viewport.left + imagePoint.x * scaleX,
      y: viewport.top + imagePoint.y * scaleY,
    },
    headingDeg: ownship.orientation_deg ?? 0,
  };
}

function plateImagePoint(position: LatLon, georef: PlateGeoref) {
  switch (georef.kind) {
    case "plate_transform_v1":
      return {
        x: (position.lon - georef.top_left_lon) * georef.pixels_per_longitude,
        y: (position.lat - georef.top_left_lat) * georef.pixels_per_latitude,
      };
    case "airport_diagram_transform_v1":
      return {
        x:
          position.lon * georef.pixel_x_from_lon +
          position.lat * georef.pixel_x_from_lat +
          georef.pixel_x_offset,
        y:
          position.lon * georef.pixel_y_from_lon +
          position.lat * georef.pixel_y_from_lat +
          georef.pixel_y_offset,
      };
  }
}

function latLonToScreen(lat: number, lon: number, viewport: MapViewportState, width: number, height: number) {
  const world = latLonToWorld(lat, lon);
  const scale = scaleForZoom(viewport.zoom);
  const wrappedX = world.x + Math.round((viewport.centerWorldX - world.x) / WEB_MERCATOR_WORLD_SIZE) * WEB_MERCATOR_WORLD_SIZE;
  return {
    x: ((wrappedX - viewport.centerWorldX) * scale) + width / 2,
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
  ringCandidates: SituationRingCandidate[],
  magneticVariationDeg: number | null,
) {
  if (ringCandidates.length === 0) {
    return null;
  }
  const center = latLonToScreen(lat, lon, viewport, width, height);
  const smaller = Math.min(width, height);
  const minDiameter = smaller * 0.5;
  const maxDiameter = smaller * 0.8;
  const targetDiameter = smaller * 0.65;
  const candidates = ringCandidates.map((candidate) => {
    const edge = projectAhead(lat, lon, 90, candidate.radius_nm);
    const edgePoint = latLonToScreen(edge.lat, edge.lon, viewport, width, height);
    const radiusPx = Math.hypot(edgePoint.x - center.x, edgePoint.y - center.y);
    const diameterPx = radiusPx * 2;
    const outOfBounds =
      diameterPx < minDiameter ? minDiameter - diameterPx : diameterPx > maxDiameter ? diameterPx - maxDiameter : 0;
    const score = outOfBounds > 0 ? 10000 + outOfBounds : Math.abs(diameterPx - targetDiameter);
    return { ...candidate, radiusPx, score };
  });
  const best = candidates.reduce((currentBest, candidate) => (candidate.score < currentBest.score ? candidate : currentBest));
  const labelAngle = -45;
  const labelPoint = pointOnCircle(center, best.radiusPx + 16, labelAngle);
  return {
    radiusPx: best.radiusPx,
    tickMarks: magneticVariationDeg === null ? [] : buildRingTickMarks(center, best.radiusPx, magneticVariationDeg),
    cardinalLabels: magneticVariationDeg === null ? [] : buildRingCardinalLabels(center, best.radiusPx, magneticVariationDeg),
    label: {
      point: labelPoint,
      rotationDeg: 45,
      text: best.label,
    },
  };
}

function buildRingCardinalLabels(center: { x: number; y: number }, radiusPx: number, magneticVariationDeg: number) {
  const labelRadius = Math.max(0, radiusPx - 30);
  return [
    { text: "N", angleDeg: -90, rotationDeg: 0 },
    { text: "E", angleDeg: 0, rotationDeg: 90 },
    { text: "S", angleDeg: 90, rotationDeg: 0 },
    { text: "W", angleDeg: 180, rotationDeg: -90 },
  ].map((label) => ({
    ...label,
    point: pointOnCircle(center, labelRadius, label.angleDeg + magneticVariationDeg),
  }));
}

function buildRingTickMarks(center: { x: number; y: number }, radiusPx: number, magneticVariationDeg: number) {
  return Array.from({ length: 12 }, (_, index) => {
    const angleDeg = index * 30 + magneticVariationDeg;
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

function flightPlanEntryPreviewSegments(
  input: string,
  preview: FlightPlanEntryPreview,
): Array<{ text: string; tokenState: "neutral" | "recognized" | "invalid"; issue: boolean }> {
  const boundaries = new Set<number>([0, input.length]);
  for (const token of preview.tokens) {
    boundaries.add(token.start);
    boundaries.add(token.end);
  }
  for (const issue of preview.issues) {
    boundaries.add(issue.start);
    boundaries.add(issue.end);
  }
  const ordered = [...boundaries].sort((left, right) => left - right);
  const segments = [] as Array<{ text: string; tokenState: "neutral" | "recognized" | "invalid"; issue: boolean }>;
  for (let index = 0; index < ordered.length - 1; index += 1) {
    const start = ordered[index];
    const end = ordered[index + 1];
    if (start === end) {
      continue;
    }
    const token = preview.tokens.find((entry) => entry.start <= start && entry.end >= end);
    const issue = preview.issues.some((entry) => entry.start <= start && entry.end >= end);
    segments.push({
      text: input.slice(start, end),
      tokenState: token?.state ?? "neutral",
      issue,
    });
  }
  return segments;
}

function navRefKey(value: NavRef) {
  if ("Airport" in value) return `airport:${value.Airport}`;
  if ("Navaid" in value) return `navaid:${value.Navaid}`;
  if ("Fix" in value) return `fix:${value.Fix}`;
  if ("ArincNavaid" in value) {
    const nav = value.ArincNavaid;
    return `arinc-navaid:${nav.identifier}:${nav.icao_code}:${nav.section_code}:${nav.subsection_code}`;
  }
  if ("TerminalNavaid" in value) {
    const nav = value.TerminalNavaid;
    return `terminal-navaid:${nav.airport_id}:${nav.identifier}:${nav.icao_code}:${nav.section_code}:${nav.subsection_code}`;
  }
  if ("LatLon" in value) return `latlon:${value.LatLon.lat}:${value.LatLon.lon}`;
  return `spot:${value.Spot.lat}:${value.Spot.lon}`;
}

function navRefLabel(value: NavRef) {
  if ("Airport" in value) return value.Airport;
  if ("Navaid" in value) return value.Navaid;
  if ("Fix" in value) return value.Fix;
  if ("ArincNavaid" in value) return value.ArincNavaid.identifier;
  if ("TerminalNavaid" in value) return value.TerminalNavaid.identifier;
  if ("LatLon" in value) return `${value.LatLon.lat.toFixed(3)}, ${value.LatLon.lon.toFixed(3)}`;
  return `SPOT ${value.Spot.lat.toFixed(3)}, ${value.Spot.lon.toFixed(3)}`;
}

function routeSegmentColor(status: FlightPlanRouteSegment["status"]) {
  if (status === "completed") {
    return loadedUiTheme.flight_plan_route.completed;
  }
  if (status === "active") {
    return loadedUiTheme.flight_plan_route.active;
  }
  if (status === "active_leg_remaining") {
    return loadedUiTheme.flight_plan_route.active_leg_remaining;
  }
  return loadedUiTheme.flight_plan_route.remaining;
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
  function handlePointerDown(event: React.PointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    props.onClose();
  }

  function handleClick(event: React.MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    if (event.detail === 0) {
      props.onClose();
    }
  }

  return (
    <button
      type="button"
      className="trayScrim"
      aria-label={props.ariaLabel}
      onPointerDown={handlePointerDown}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={handleClick}
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
