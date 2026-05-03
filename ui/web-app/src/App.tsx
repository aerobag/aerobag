import { Fragment, useCallback, useEffect, useId, useMemo, useRef, useState, type CSSProperties, type Dispatch, type MouseEvent, type PointerEvent, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import bootstrapJson from "@shared-bootstrap";
import type {
  AirwayPresentationPlan,
  AirwaySuggestion,
  ChartPageData,
  ChartFamilyId,
  DevBootstrapJson,
  FlightPlan,
  FlightPlanEntryPreview,
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
  GeometryJson,
  LatLon,
  MapViewOptionJson,
  MaterializedProcedure,
  NavSymbolFeature,
  NavElementUiView,
  NavRef,
  PlaybackUiState,
  MapFollowUiState,
  OwnshipRenderState,
  PlateGeoref,
  ProcedureOptions,
  ProcedureLoadOption,
  ProcedureSummary,
  SituationRingCandidate,
  WaypointIdentifierSuggestion,
} from "./domain/types";
import { runCoreHadOperation } from "./domain/navKv";
import uiTheme from "@shared-ui-theme";
import planViewIcon from "./assets/plan-view-icon.svg";
import {
  loadBestAvailableAdapter,
  type AdapterBackendKind,
  type AppCoreAdapter,
  type DerivedChartPageState,
  type DerivedMapSelectorState,
  type MapLayerId,
  type MapSelectionItem,
  type MapSelectionQueryResult,
  type RasterTileDraw,
  type UiMapLayerState,
  type UiMapLayerToggleState,
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
import {
  airspaceFeatureUrl,
  airspaceLabelTileUrl,
  airspaceReferenceTileUrl,
  metarTileUrl,
  pointTileUrl,
  type PointTilePayload,
} from "./domain/vectorTiles";
import type {
  AirspaceDisplayPath,
  AirspaceFeaturePayload,
  AirspaceLabelTilePayload,
  AirspaceReferenceTilePayload,
  MapOverlayQueryResult,
  MetarTilePayload,
  MetarProductPayload,
  TerrainOverlayQueryResult,
  TerrainOverlayTileRequest,
  TfrProductPayload,
  VisibleMapFeature,
  VisibleMetarFeature,
} from "./domain/appCoreAdapter";
import { airwayEntryCandidateFromPresentation, airwayExitCandidatesFromPresentation } from "./domain/airwayPresentation";
import { debugLog, debugTiming, installGlobalErrorLogging } from "./domain/debugLog";
import { TerrainRenderWorkerClient } from "./domain/terrainRenderWorkerClient";

type SurfaceSize = {
  width: number;
  height: number;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isInvalidUiSessionHandleError(error: unknown): boolean {
  return error instanceof Error && error.message.includes("invalid ui session handle");
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

function airspaceDashArray(dashPx: number[]): string | undefined {
  return dashPx.length > 0 ? dashPx.join(" ") : undefined;
}

function svgStrokeLinecap(lineCap: string): "butt" | "round" | "square" {
  return lineCap === "butt" || lineCap === "square" ? lineCap : "round";
}

function airspaceLabelParts(text: string): { upper: string; lower: string } | null {
  const parts = text.split("/");
  if (parts.length !== 2) {
    return null;
  }
  const upper = parts[0].trim();
  const lower = parts[1].trim();
  if (!upper || !lower) {
    return null;
  }
  return { upper, lower };
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
          d={airspaceSvgPathListD(decoration.paths)}
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

type AppPage = "map" | "plan" | "charts" | "home";

type WebPageTilePaintTiming = {
  id: number;
  fromPage: AppPage;
  startedAt: number;
};

type ChartAsset = NonNullable<ChartPageData["airports"][number]>["charts"][number];
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
    header_button: string;
    disabled_button: string;
    button_fg: string;
    panel_bg: string;
    panel_border: string;
    panel_fg: string;
    panel_muted: string;
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
  plate_folder: {
    thumbnail_bg: string;
    label_colors: Record<string, string>;
  };
};

type AviationThemeColorKey = keyof UiThemeJson["aviation"];

type TrayDockStyle = "compact" | "plate_narrow" | "plate_wide" | "wide";
type PlateFolderCategory = ChartAsset["folder_category"];

type NexradManifest = {
  schema_version: number;
  version_label: string;
  frame_count: number;
  projection: string;
  frames: NexradFrame[];
};

type NexradFrame = {
  filename: string;
  observed_at_utc: string;
  width: number;
  height: number;
  bounds: {
    west: number;
    south: number;
    east: number;
    north: number;
  };
};

const bootstrap = bootstrapJson as DevBootstrapJson;
const samplePlan = bootstrap.flight_plan;
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
    default:
      return undefined;
  }
}

function layerIconSrc(layerId: MapLayerId): string {
  switch (layerId) {
    case "vectors":
      return LAYER_VECTORS_ICON_SRC;
    case "metars":
      return LAYER_VECTORS_ICON_SRC;
    case "nexrad":
      return LAYER_NEXRAD_ICON_SRC;
    case "terrain_warning":
      return LAYER_TERRAIN_WARNING_ICON_SRC;
  }
}

type NexradOverlayFrame = NexradFrame & {
  url: string;
};

type NexradLayerStatus =
  | { state: "loading" }
  | { state: "available"; frame_count: number }
  | { state: "unavailable"; reason: string };

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
  altitudeBucket: number | null;
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

function terrainSourceCacheKey(sourceTile: { product_id: string; path: string }) {
  return `${sourceTile.product_id}/${sourceTile.path}`;
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
  const productCounts = new Map<string, number>();
  for (const request of requests) {
    zoomCounts.set(request.z, (zoomCounts.get(request.z) ?? 0) + 1);
    for (const sourceTile of terrainSourceTiles(request)) {
      productCounts.set(sourceTile.product_id, (productCounts.get(sourceTile.product_id) ?? 0) + 1);
    }
  }
  const summarize = <T extends string | number>(counts: Map<T, number>) =>
    Array.from(counts.entries())
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([key, count]) => `${key}:${count}`)
      .join(",");
  return {
    zooms: summarize(zoomCounts),
    products: summarize(productCounts),
  };
}

function terrainSourceTiles(request: TerrainOverlayTileRequest) {
  return request.source_tiles.length > 0
    ? request.source_tiles
    : [{ product_id: request.product_id, path: request.path }];
}

function packTerrainTileBytes(tileBytesList: Uint8Array[]) {
  const headerBytes = 4 + tileBytesList.length * 4;
  const byteCount = headerBytes + tileBytesList.reduce((total, tileBytes) => total + tileBytes.byteLength, 0);
  const packed = new Uint8Array(byteCount);
  const view = new DataView(packed.buffer);
  let cursor = 0;
  view.setUint32(cursor, tileBytesList.length, true);
  cursor += 4;
  for (const tileBytes of tileBytesList) {
    view.setUint32(cursor, tileBytes.byteLength, true);
    cursor += 4;
    packed.set(tileBytes, cursor);
    cursor += tileBytes.byteLength;
  }
  return packed;
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
const WEB_MERCATOR_HALF_WORLD_M = 20037508.342789244;
const NEXRAD_FRAME_INTERVAL_MS = 900;
const RASTER_TILE_OVERDRAW_PX = 1;

function mercatorMetersToWorld(xMeters: number, yMeters: number): { x: number; y: number } {
  const worldSpanMeters = WEB_MERCATOR_HALF_WORLD_M * 2;
  return {
    x: ((xMeters + WEB_MERCATOR_HALF_WORLD_M) / worldSpanMeters) * WEB_MERCATOR_WORLD_SIZE,
    y: ((WEB_MERCATOR_HALF_WORLD_M - yMeters) / worldSpanMeters) * WEB_MERCATOR_WORLD_SIZE,
  };
}

function formatNexradObservedTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toISOString().slice(11, 16);
}

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

const pageOptions: Array<{ id: AppPage; label: string; launcherLabel: string; iconSrc?: string }> = [
  { id: "map", label: "CHART", launcherLabel: "CHART", iconSrc: PAGE_CHART_ICON_SRC },
  { id: "charts", label: "PLATE", launcherLabel: "PLATE", iconSrc: PAGE_PLATE_ICON_SRC },
  { id: "plan", label: "FLIGHT PLAN", launcherLabel: "PLAN", iconSrc: PAGE_PLAN_ICON_SRC },
  { id: "home", label: "HOME", launcherLabel: "HOME" },
];

const webUiStateStorageKey = "aerobag.web.uiState.v1";
const maxViewHistoryDepth = 64;
const loadedUiTheme = uiTheme as UiThemeJson;
const controlTheme = loadedUiTheme.controls;
const plateFolderTheme = loadedUiTheme.plate_folder;
const VAMPS_POSITION = { lat: 47.3648944444444, lon: -121.980275 };
const NRVNA_POSITION = { lat: 47.37208888888889, lon: -122.16950277777778 };
const defaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json";
const startupHighLatencyWarningGraceMs = 10_000;
const vorOuterHexPoints = [
  { x: -8, y: 0 },
  { x: -4, y: -7 },
  { x: 4, y: -7 },
  { x: 8, y: 0 },
  { x: 4, y: 7 },
  { x: -4, y: 7 },
] as const;
const vorEdgeInsetDistances = [3.8, 1.9, 3.8, 1.9, 3.8, 1.9] as const;
const mapSelectionSpotPegPath = "M 0 0 C -9 -9 -12 -16 -12 -23 A 12 12 0 1 1 12 -23 C 12 -16 9 -9 0 0 Z";

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

type RasterRenderTile = {
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

function renderTileFromCore(tile: RasterTileDraw, cssScale = 1): RasterRenderTile {
  return {
    x: tile.x,
    yTms: tile.y_tms,
    left: tile.left_px * cssScale,
    top: tile.top_px * cssScale,
    size: tile.size_px * cssScale,
    zoom: tile.source_zoom,
    zIndex: tile.z_order,
    src: tile.primary.url,
    mapViewId: tile.primary.map_view_id,
    packageName: tile.primary.package_name,
    chartFamily: tile.family,
    fallbacks: tile.fallbacks,
  };
}

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
const airportFuelMarkerPath = [
  "M -4 -17 H 4 V -11.314",
  "A 12 12 0 0 1 11.314 -4",
  "H 17 V 4 H 11.314",
  "A 12 12 0 0 1 4 11.314",
  "V 17 H -4 V 11.314",
  "A 12 12 0 0 1 -11.314 4",
  "H -17 V -4 H -11.314",
  "A 12 12 0 0 1 -4 -11.314",
  "Z",
].join(" ");
const airportCircleMarkerPath = [
  "M 0 -12",
  "A 12 12 0 1 1 0 12",
  "A 12 12 0 1 1 0 -12",
  "Z",
].join(" ");
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
};

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
            <path d={airportCircleMarkerPath} className="airportOpenMarkerUnder" />
            <path d={airportCircleMarkerPath} className={`${airportClass} airportOpenMarker`} />
          </>
        ) : feature.fuel_available ? (
          <path d={airportFuelMarkerPath} className={airportClass} />
        ) : (
          <path d={airportCircleMarkerPath} className={airportClass} />
        )}
        {isHeliport ? (
          <text x="0" y="6" textAnchor="middle" className="airportSpecialGlyph airportHeliportGlyph">
            H
          </text>
        ) : isSeaplaneBase ? (
          <path
            d="M 0 -9 L 0 5 M -5 -5 L 5 -5 M -7 2 C -5 8 5 8 7 2"
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
          <text x="0" y={airportLabelY} textAnchor="middle" className={airportLabelClass}>
            {feature.label}
          </text>
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
          <text x="0" y={vorLabelY} textAnchor="middle" className="vorLabel">
            {feature.label}
          </text>
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
    const obstaclePath = isTallObstacle
      ? "M -8 7.2 Q -6.6 4.4 -4.2 -1.2 Q -2.4 -7.0 -1.2 -15.6 Q -0.4 -24.0 0 -34.0 Q 0.4 -24.0 1.2 -15.6 Q 2.4 -7.0 4.2 -1.2 Q 6.6 4.4 8 7.2"
      : "M -7.2 7.2 L 0 -14.4 L 7.2 7.2";
    const obstacleDotY = isTallObstacle ? 6.0 : 4.8;
    const obstacleDotRadius = isTallObstacle ? 2.05 : 2.05;
    return (
      <>
        <path d={obstaclePath} className={`${obstacleClass} obstacleMarkerUnder`} />
        <path d={obstaclePath} className={obstacleClass} />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className="obstacleDotUnder" />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className={obstacleDotClass} />
        {showLabel && feature.label ? (
          <text x="0" y={obstacleLabelY} textAnchor="middle" className="obstacleLabel">
            {feature.label}
          </text>
        ) : null}
      </>
    );
  }
  return (
    <>
      <path d="M 0 -8 L 7 6 L -7 6 Z" className="fixMarker" />
      {showLabel ? (
        <text x="0" y={fixLabelY} textAnchor="middle" className="fixLabel">
          {feature.label}
        </text>
      ) : null}
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
  const radius = 8;
  const quadrantPath = feature.ceiling_amount === "sct"
    ? `M 0 0 L 0 ${-radius} A ${radius} ${radius} 0 0 1 ${radius} 0 Z`
    : feature.ceiling_amount === "bkn"
      ? `M 0 0 L 0 ${-radius} A ${radius} ${radius} 0 1 1 ${-radius} 0 Z`
      : null;
  return (
    <g className={`metarSymbol ${categoryClass}`} aria-hidden="true">
      {feature.ceiling_amount === "ovc" ? <circle r={radius} className="metarFill" /> : null}
      {quadrantPath ? <path d={quadrantPath} className="metarFill" /> : null}
      <circle r={radius} className="metarCircleUnder" />
      <circle r={radius} className="metarCircle" />
      {feature.ceiling_amount === "few" ? (
        <>
          <line x1="0" y1={-radius + 1.5} x2="0" y2={radius - 1.5} className="metarBarUnder" />
          <line x1="0" y1={-radius + 1.5} x2="0" y2={radius - 1.5} className="metarBar" />
        </>
      ) : null}
      {feature.ceiling_amount === "missing" ? (
        <text x="0" y="4" textAnchor="middle" className="metarMissingGlyph">
          M
        </text>
      ) : null}
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
    vectors: { visible: true, enabled: true },
    metars: { visible: true, enabled: true },
    nexrad: { visible: false, enabled: true },
    terrain_warning: { visible: true, enabled: true },
  };
}

export default function App() {
  const [sessionStartMs] = useState(() => Date.now());
  const uptimeLabel = useSessionUptimeLabel(sessionStartMs);
  const [debugTileLabels, setDebugTileLabels] = useState(
    () => typeof window !== "undefined" && new URLSearchParams(window.location.search).has("debugTiles"),
  );
  const [debugPlaybackVisible, setDebugPlaybackVisible] = useState(false);
  const persistedUiState = useMemo(readPersistedWebUiState, []);
  const [page, setPage] = useState<AppPage>(persistedUiState.page ?? "map");
  const [pageHistory, setPageHistory] = useState<AppViewSnapshot[]>([]);
  const [appCoreAdapter, setAppCoreAdapter] = useState<AppCoreAdapter | null>(null);
  const [adapterBackend, setAdapterBackend] = useState<AdapterBackendKind>("wasm");
  const [adapterDetail, setAdapterDetail] = useState<string>("loading");
  const [sessionInitError, setSessionInitError] = useState<string | null>(null);
  const startupVisualReadyRef = useRef(false);
  const pageTilePaintTimingRef = useRef<WebPageTilePaintTiming | null>(null);
  const nextPageTilePaintTimingIdRef = useRef(1);
  const highLatencyWarningsSuppressedRef = useRef(true);
  const highLatencyWarningTimerRef = useRef<number | null>(null);
  const [mapSelectorState, setMapSelectorState] = useState<DerivedMapSelectorState>({
    selected_map_id: "",
    selected_map: null,
    displayed_maps: [],
    geometry: { schema_version: 1, polygons: [], polygon_sets: [] },
    family_options: [],
  });
  const [mapSelectorLoadError, setMapSelectorLoadError] = useState<string | null>(null);
  const [selectedMapId, setSelectedMapId] = useState<string>("");
  const initialRecentAirportIds = useMemo(
    () => mergeRecentAirportIds(emptyChartPage.airports, persistedUiState.recentAirportIds ?? []),
    [persistedUiState],
  );
  const initialChartPageState = useMemo<DerivedChartPageState>(
    () => ({
      airports: emptyChartPage.airports,
      recent_airport_ids: initialRecentAirportIds,
      selected_airport_id: resolveAirportId(emptyChartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
      selected_chart_id: resolveChartId(
        emptyChartPage.airports,
        resolveAirportId(emptyChartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
        persistedUiState.selectedChartId,
      ),
    }),
    [initialRecentAirportIds, persistedUiState.selectedAirportId, persistedUiState.selectedChartId],
  );
  const [uiSession, setUiSession] = useState<UiSession | null>(null);
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
          speed_kt: null,
          altitude_msl_ft: null,
          pressure_altitude_ft: null,
        },
        controls: {
          mode: "none",
          selection: { kind: "auto" },
          sources: [],
        },
      },
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
    caution_state: {
      obstacle_display_limited: false,
    },
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
  const playbackUiState = sessionSnapshot.playback_ui_state;
  const mapFollowUiState = sessionSnapshot.map_follow_ui_state;
  const chartPageData: ChartPageData = useMemo(
    () => ({ airports: derivedChartPageState.airports }),
    [derivedChartPageState.airports],
  );

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
  const planUiState = appUiState.active_plan;
  const recentAirportIds = derivedChartPageState.recent_airport_ids;
  const selectedAirportId = derivedChartPageState.selected_airport_id;
  const selectedChartId = derivedChartPageState.selected_chart_id;

  const selectedMap = mapSelectorState.selected_map;
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
    () => mapSelectorState.family_options.find((family) => family.active) ?? null,
    [mapSelectorState.family_options],
  );
  const selectedFamilyMapViews = mapSelectorState.displayed_maps;
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
    installGlobalErrorLogging();
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
    debugTiming("startup.session.create", () => buildSeededDevPlan().then(async (initialPlan) => {
      const created = await debugTiming("startup.session.create.core", () => appCoreAdapter.createUiSession(
        initialPlan.plan,
        initialPlan.recentAirportIds ?? initialRecentAirportIds,
        initialPlan.selectedAirportId ?? initialChartPageState.selected_airport_id,
        initialPlan.selectedChartId ?? initialChartPageState.selected_chart_id,
      ));
      const createdSnapshot = await debugTiming("startup.session.ownship_start", () => created.setSituation({
        position: { kind: "lat_lon", lat: NRVNA_POSITION.lat, lon: NRVNA_POSITION.lon },
        orientation_deg: 342,
        speed_kt: 0,
      }));
      debugLog("session.create.snapshot", {
        app_state_active_plan: createdSnapshot.app_state.active_plan?.id ?? null,
        app_ui_state_nav_element: createdSnapshot.app_ui_state.active_plan?.guidance?.nav_element ?? null,
      });
      nextSession = created;
      if (!cancelled) {
        setUiSession(created);
        setSessionSnapshot(createdSnapshot);
      }
    })).catch((error) => {
      console.error("failed to initialize web ui session", error);
    });
    return () => {
      cancelled = true;
      void nextSession?.destroy();
    };
  }, [adapterBackend, appCoreAdapter, initialChartPageState.selected_airport_id, initialChartPageState.selected_chart_id, initialRecentAirportIds]);

  useEffect(() => {
    let cancelled = false;
    if (!appCoreAdapter) {
      return;
    }
    debugTiming(
      "map.selector_state.load",
      () => appCoreAdapter.deriveMapSelectorState(selectedMapId || undefined),
    ).then((state) => {
      if (cancelled) {
        return;
      }
      setMapSelectorState(state);
      setSelectedMapId(state.selected_map_id);
      void uiSession?.installRasterMapCatalog(state).catch((error) => {
        console.error("failed to install raster map catalog in core session", error);
      });
      setMapSelectorLoadError(null);
    }).catch((error) => {
      if (!cancelled) {
        setMapSelectorLoadError(`failed to derive map selector state: ${errorMessage(error)}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter, selectedMapId, uiSession]);

  useEffect(() => {
    let cancelled = false;
    if (!appCoreAdapter || !currentPlan) {
      return;
    }
    debugTiming(
      "charts.page_state.load",
      () => appCoreAdapter.deriveChartPageState(
        currentPlan,
        sessionSnapshot.chart_page_state.recent_airport_ids,
        sessionSnapshot.chart_page_state.selected_airport_id || undefined,
        sessionSnapshot.chart_page_state.selected_chart_id || undefined,
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
    currentPlan,
    sessionSnapshot.chart_page_state.recent_airport_ids,
    sessionSnapshot.chart_page_state.selected_airport_id,
    sessionSnapshot.chart_page_state.selected_chart_id,
  ]);

  useEffect(() => {
    if (!selectedMap) {
      return;
    }
    setMapViewport((current) => preserveViewportForMap(current, selectedMap.map_view));
  }, [selectedMap]);

  const appReady =
    appCoreAdapter !== null &&
    uiSession !== null &&
    selectedMap !== null &&
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
      void uiSession.selectMap(snapshot.selectedMapId).catch(() => {});
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

  function openPlateTarget(airportId: string, target: "Folder" | "CSup") {
    const targetChartId = `Plate:${airportId}:${target}`;
    const nextRecentAirportIds = moveAirportToFront(recentAirportIds, airportId, chartPageData.airports);
    const localChartId = resolveChartId(chartPageData.airports, airportId, targetChartId);
    const localAirport = chartPageData.airports.find((entry) => entry.id === airportId);
    const localChart = localAirport?.charts.find((chart) => chart.id === localChartId);
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
      selectedChartId: localChartId || targetChartId,
      selectedChartLabel: localChart?.label ?? "",
      recentAirportIds: nextRecentAirportIds,
      chartViewport: null,
      chartFolderOpen: target === "Folder",
    });
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
      sessionInitError !== null ||
      mapSelectorLoadError !== null ||
      chartPageStateLoadError !== null ||
      (appReady &&
        currentPlan !== null &&
        planUiState !== null);
    if (shouldHideStartupShell) {
      window.__aerobag_hide_startup_shell?.();
    }
  }, [appReady, chartPageStateLoadError, currentPlan, mapSelectorLoadError, planUiState, sessionInitError]);

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
    return null;
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
          onDebugTileLabelsChange={setDebugTileLabels}
          debugPlaybackVisible={debugPlaybackVisible}
          onDebugPlaybackVisibleChange={setDebugPlaybackVisible}
          mapLayerState={sessionSnapshot.map_layer_state}
          selectedMapId={selectedMapId}
          selectedMap={selectedMap}
          selectedFamilyMapViews={selectedFamilyMapViews}
          geometry={mapSelectorState.geometry}
          selectedFamily={selectedFamily}
          familyOptions={mapSelectorState.family_options}
          viewport={mapViewport}
          pageTilePaintTiming={pageTilePaintTimingRef.current}
          onPageTilePaintTimingComplete={(id) => {
            if (pageTilePaintTimingRef.current?.id === id) {
              pageTilePaintTimingRef.current = null;
            }
          }}
          onViewportChange={setMapViewport}
          onSelectMapId={(mapId) => {
            pushViewSnapshot({
              page: "map",
              selectedMapId: mapId,
            });
          }}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenPlateTarget={openPlateTarget}
          legSummary={legSummary}
          ownship={appUiState.ownship.render}
          plan={currentPlan}
          planUiState={planUiState}
          playbackUiState={playbackUiState}
          mapFollowUiState={mapFollowUiState}
          mapFollowTargetViewport={sessionSnapshot.map_follow_target_viewport}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          uiSession={uiSession}
          debugWarningActive={debugWarningActive}
          onDebugWarning={logDebugWarning}
          onHighLatencyWarning={logHighLatencyWarning}
          onFirstVisualReady={reportStartupVisualReady}
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
          onMoveComponent={async (componentIndex, delta) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.moveComponentUi(currentPlan, componentIndex, delta);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onRemoveAllAbove={async (componentIndex) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.removeAllAboveUi(currentPlan, componentIndex);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onInsertAirportWaypoint={async (componentIndex, before, airportId) => {
            if (!appCoreAdapter) return;
            const waypoint = await appCoreAdapter.resolveWaypointIdentifier(airportId);
            if (!waypoint) {
              throw new Error(`Unknown waypoint ${airportId}`);
            }
            const mutation = await appCoreAdapter.insertWaypointUi(currentPlan, componentIndex, before, waypoint);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onPreviewFlightPlanEntry={async (input) => {
            if (!appCoreAdapter) {
              throw new Error("app core adapter unavailable");
            }
            return appCoreAdapter.previewFlightPlanEntry(currentPlan, input);
          }}
          onAppendFlightPlanEntry={async (input) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.appendFlightPlanEntry(currentPlan, input);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onActivateLeg={async (legIndex) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.activateLegUi(currentPlan, legIndex);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onDeleteComponent={async (componentIndex) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.deleteComponentUi(currentPlan, componentIndex);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onActivateNextLeg={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.activateNextLegUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onSuspendSequencing={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.suspendSequencingUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onUnsuspendSequencing={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.unsuspendSequencingUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onSequenceActiveLeg={async () => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.sequenceActiveLegUi(currentPlan);
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
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
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
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
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onInsertProcedure={async (startComponentIndex, endComponentIndex, built) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.insertProcedureMaterializedUi(
              currentPlan,
              startComponentIndex,
              endComponentIndex,
              built,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          onReplaceProcedure={async (componentIndex, built) => {
            if (!appCoreAdapter) return;
            const mutation = await appCoreAdapter.replaceProcedureMaterializedUi(
              currentPlan,
              componentIndex,
              built,
            );
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          debugWarningActive={debugWarningActive}
        />
      </div>

      <div className={`pageLayer${page === "charts" ? " isActive" : ""}`} aria-hidden={page !== "charts"}>
        <ChartsPage
          appCoreAdapter={appCoreAdapter}
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
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
          onApplyMutation={async (mutation) => {
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          playbackUiState={playbackUiState}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          debugPlaybackVisible={debugPlaybackVisible}
          onDebugPlaybackVisibleChange={setDebugPlaybackVisible}
          uiSession={uiSession}
          debugWarningActive={debugWarningActive}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </div>

      <div className={`pageLayer${page === "home" ? " isActive" : ""}`} aria-hidden={page !== "home"}>
        <HomePage
          page={page}
          pageHistory={pageHistory}
          uptimeLabel={uptimeLabel}
          planUiState={planUiState}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          debugWarningActive={debugWarningActive}
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
  onDebugTileLabelsChange: (enabled: boolean) => void;
  debugPlaybackVisible: boolean;
  onDebugPlaybackVisibleChange: (enabled: boolean) => void;
  mapLayerState: UiMapLayerState;
  selectedMapId: string;
  selectedMap: MapViewOptionJson;
  selectedFamilyMapViews: MapViewOptionJson[];
  geometry: GeometryJson;
  selectedFamily: DerivedMapSelectorState["family_options"][number] | null;
  familyOptions: DerivedMapSelectorState["family_options"];
  viewport: MapViewportState;
  pageTilePaintTiming: WebPageTilePaintTiming | null;
  onPageTilePaintTimingComplete: (id: number) => void;
  onViewportChange: (next: MapViewportState) => void;
  onSelectMapId: (mapId: string) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  onOpenPlateTarget: (airportId: string, target: "Folder" | "CSup") => void;
  legSummary: string;
  ownship: OwnshipRenderState;
  plan: FlightPlan;
  planUiState: FlightPlanUiState | null;
  playbackUiState: PlaybackUiState;
  mapFollowUiState: MapFollowUiState;
  mapFollowTargetViewport: { center: LatLon; zoom: number; rotation_deg: number; pitch_deg: number } | null;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  uiSession: UiSession | null;
  debugWarningActive: boolean;
  onDebugWarning: (tag: string, data?: unknown) => void;
  onHighLatencyWarning: (tag: string, data?: unknown) => void;
  onFirstVisualReady: () => void;
}) {
  const {
    appCoreAdapter,
    debugTileLabels,
    onDebugTileLabelsChange,
    debugPlaybackVisible,
    onDebugPlaybackVisibleChange,
    mapLayerState,
    page,
    pageHistory,
    uptimeLabel,
    selectedMap,
    selectedFamilyMapViews,
    geometry,
    selectedFamily,
    familyOptions,
    viewport,
    pageTilePaintTiming,
    onPageTilePaintTimingComplete,
    onViewportChange,
    onSelectMapId,
    onSelectPage,
    onOpenPlan,
    onOpenPlateTarget,
    legSummary,
    ownship,
    plan,
    planUiState,
    uiSession,
    onPlaybackSnapshotChange,
    mapFollowUiState,
    mapFollowTargetViewport,
    debugWarningActive,
    onDebugWarning,
    onHighLatencyWarning,
    onFirstVisualReady,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const trayGroup = useModalTrayGroup(["family", "layers"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
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
    needed_point_tiles: [],
    needed_metar_tiles: [],
    needed_airspace_ref_tiles: [],
    needed_airspace_features: [],
    needed_airspace_label_tiles: [],
    needed_metars: false,
    needed_tfrs: false,
    visible_features: [],
    visible_metars: [],
    airspace_paths: [],
    tfr_paths: [],
    airspace_labels: [],
    warnings: [],
  });
  const [nexradFrames, setNexradFrames] = useState<NexradOverlayFrame[]>([]);
  const [nexradFrameIndex, setNexradFrameIndex] = useState(0);
  const [nexradStatus, setNexradStatus] = useState<NexradLayerStatus>({ state: "loading" });
  const [terrainOverlay, setTerrainOverlay] = useState<TerrainOverlayUiState>({ query: null, images: [] });
  const terrainTileCacheRef = useRef<Map<string, TerrainTileCacheEntry>>(new Map());
  const terrainSourceByteCacheRef = useRef<Map<string, Uint8Array>>(new Map());
  const terrainTileInFlightRef = useRef<Set<string>>(new Set());
  const terrainRenderQueueRef = useRef<Map<string, TerrainTileRenderTask>>(new Map());
  const terrainRenderPumpActiveRef = useRef(false);
  const terrainRenderSessionRef = useRef<UiSession | null>(null);
  const terrainRenderWorkerRef = useRef<TerrainRenderWorkerClient | null>(null);
  const terrainCurrentBucketRef = useRef<number | null>(null);
  const terrainPendingFrameRef = useRef<TerrainPendingFrame | null>(null);
  const terrainFrameStartRef = useRef<Map<string, number>>(new Map());
  const lastTerrainRenderPlanKeyRef = useRef("");
  const [flightPlanRoute, setFlightPlanRoute] = useState<FlightPlanRouteSegment[]>([]);
  const [mapOverlayViewport, setMapOverlayViewport] = useState<MapViewportState | null>(null);
  const viewportRef = useRef<MapViewportState>(viewport);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const clickCandidateRef = useRef<{ pointerId: number; start: ScreenPoint; latest: ScreenPoint } | null>(null);
  const gestureActiveRef = useRef(false);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [mapSelection, setMapSelection] = useState<{
    point: ScreenPoint;
    result: MapSelectionQueryResult;
    selectedItem: MapSelectionItem | null;
  } | null>(null);
  const firstVisualReadyRef = useRef(false);
  const lastOverlayWarningKeyRef = useRef("");

  function pumpTerrainRenderQueue() {
    if (terrainRenderPumpActiveRef.current) {
      return;
    }
    terrainRenderPumpActiveRef.current = true;
    void (async () => {
      try {
        while (terrainRenderQueueRef.current.size > 0) {
          const session = terrainRenderSessionRef.current;
          if (!session) {
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
            let fetchElapsedMs = 0;
            const tileBytesList: Uint8Array[] = [];
            for (const sourceTile of terrainSourceTiles(task.request)) {
              const sourceCacheKey = terrainSourceCacheKey(sourceTile);
              let sourceBytes = terrainSourceByteCacheRef.current.get(sourceCacheKey);
              if (!sourceBytes) {
                const fetchStartedAt = performance.now();
                const response = await fetch(`/terrain-products/${sourceTile.product_id}/${sourceTile.path}`);
                if (response.status === 404) {
                  debugLog("terrain.overlay.tile.missing", {
                    key: task.request.key,
                    source: sourceCacheKey,
                  });
                  continue;
                }
                if (!response.ok) {
                  throw new Error(`terrain product request failed ${sourceCacheKey}: ${response.status}`);
                }
                sourceBytes = new Uint8Array(await response.arrayBuffer());
                fetchElapsedMs += performance.now() - fetchStartedAt;
                terrainSourceByteCacheRef.current.set(sourceCacheKey, sourceBytes);
              }
              tileBytesList.push(sourceBytes);
            }
            if (tileBytesList.length === 0) {
              throw new Error(`no terrain product sources available for ${task.request.key}`);
            }
            if (task.altitudeBucket !== terrainCurrentBucketRef.current) {
              continue;
            }
            const renderStartedAt = performance.now();
            const worker = terrainRenderWorkerRef.current;
            const rawBytes = worker
              ? (
                  tileBytesList.length === 1
                    ? await worker.renderTile(tileBytesList[0].slice(), task.altitudeBucket ?? Number.NaN)
                    : await worker.renderPackedTiles(packTerrainTileBytes(tileBytesList), task.altitudeBucket ?? Number.NaN)
                )
              : (
                  tileBytesList.length === 1
                    ? await session.renderTerrainOverlayTile(tileBytesList[0], task.altitudeBucket ?? Number.NaN)
                    : await session.renderTerrainOverlayTiles(packTerrainTileBytes(tileBytesList), task.altitudeBucket ?? Number.NaN)
                );
            const renderElapsedMs = performance.now() - renderStartedAt;
            const parsed = parseTerrainRawRgba(rawBytes);
            terrainTileCacheRef.current.set(cacheKey, parsed);
            debugLog("terrain.overlay.tile.done", {
              key: task.request.key,
              altitude_bucket: task.altitudeBucket,
              source_count: tileBytesList.length,
              raw_bytes: rawBytes.byteLength,
              image_width: parsed.imageWidth,
              image_height: parsed.imageHeight,
              fetch_ms: Math.round(fetchElapsedMs),
              render_ms: Math.round(renderElapsedMs),
              elapsed_ms: Math.round(performance.now() - tileStartedAt),
              render_thread: worker ? "worker" : "main",
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
  const loadedRasterTileKeysRef = useRef<Set<string>>(new Set());
  const completedPageTilePaintTimingIdsRef = useRef<Set<number>>(new Set());
  const rasterTileKey = useCallback((tile: RasterRenderTile) =>
    `${tile.chartFamily}-${tile.packageName ?? tile.mapViewId}-${tile.zoom}-${tile.x}-${tile.yTms}`,
  []);
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
  useEffect(() => {
    let cancelled = false;
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      setTiles([]);
      setRasterTileViewport(null);
      return;
    }
    const planStartedAt = performance.now();
    const devicePixelRatio = window.devicePixelRatio || 1;
    const deviceViewport = {
      ...viewport,
      zoom: viewport.zoom + Math.log2(devicePixelRatio),
    };
    uiSession.queryRasterTilePlan(
      deviceViewport,
      surfaceSize.width * devicePixelRatio,
      surfaceSize.height * devicePixelRatio,
    )
      .then((plan) => {
        if (!cancelled) {
          pageTilePaintTiming && debugLog("web.page-to-map.plan", {
            id: pageTilePaintTiming.id,
            from_page: pageTilePaintTiming.fromPage,
            elapsed_ms: Math.round(performance.now() - pageTilePaintTiming.startedAt),
            plan_ms: Math.round(performance.now() - planStartedAt),
            tiles: plan.tiles.length,
            device_pixel_ratio: devicePixelRatio,
          });
          loadedRasterTileKeysRef.current = new Set();
          setTiles(plan.tiles.map((tile) => renderTileFromCore(tile, 1 / devicePixelRatio)));
          setRasterTileViewport(viewport);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          console.error("failed to query raster tile plan", error);
          setTiles([]);
          setRasterTileViewport(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [pageTilePaintTiming, surfaceSize.height, surfaceSize.width, uiSession, viewport]);
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
  const situationRingCandidates = useMemo(() => appCoreAdapter.situationRingCandidates(), [appCoreAdapter]);
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
  const nexradOverlay = useMemo(() => {
    const frame = nexradFrames[nexradFrameIndex];
    if (!frame || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    const northwestWorld = mercatorMetersToWorld(frame.bounds.west, frame.bounds.north);
    const southeastWorld = mercatorMetersToWorld(frame.bounds.east, frame.bounds.south);
    const northwest = worldToScreen(viewport, northwestWorld, surfaceSize.width, surfaceSize.height);
    const southeast = worldToScreen(viewport, southeastWorld, surfaceSize.width, surfaceSize.height);
    return {
      frame,
      style: {
        left: `${northwest.x}px`,
        top: `${northwest.y}px`,
        width: `${southeast.x - northwest.x}px`,
        height: `${southeast.y - northwest.y}px`,
      } satisfies CSSProperties,
    };
  }, [nexradFrameIndex, nexradFrames, surfaceSize.height, surfaceSize.width, viewport]);
  const routeScreenSegments = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return flightPlanRoute.map((segment) => ({
      ...segment,
      path: (segment.path.length > 0 ? segment.path : [segment.from, segment.to])
        .map((point) => worldToScreen(viewport, latLonToWorld(point.lat, point.lon), surfaceSize.width, surfaceSize.height)),
    }));
  }, [flightPlanRoute, surfaceSize.height, surfaceSize.width, viewport]);

  useEffect(() => {
    if (!mapLayerState.nexrad.visible || !mapLayerState.nexrad.enabled) {
      setNexradStatus({ state: "unavailable", reason: "hidden" });
      setNexradFrames([]);
      setNexradFrameIndex(0);
      return;
    }
    const controller = new AbortController();
    let cancelled = false;
    setNexradStatus({ state: "loading" });

    async function loadNexrad() {
      const response = await fetch("/fast-products/nexrad/nexrad.json", { signal: controller.signal });
      if (response.status === 404) {
        if (uiSession) {
          void uiSession.setMapLayerEnabled("nexrad", false).then(onPlaybackSnapshotChange).catch(() => {});
        }
        return {
          status: { state: "unavailable", reason: "not_found" } satisfies NexradLayerStatus,
          frames: [],
        };
      }
      if (!response.ok) {
        return {
          status: { state: "unavailable", reason: `http_${response.status}` } satisfies NexradLayerStatus,
          frames: [],
        };
      }
      const manifest = (await response.json()) as NexradManifest;
      if (manifest.projection !== "EPSG:3857") {
        return {
          status: { state: "unavailable", reason: `unsupported_projection_${manifest.projection}` } satisfies NexradLayerStatus,
          frames: [],
        };
      }
      const frames = [...manifest.frames]
        .reverse()
        .map((frame) => ({
          ...frame,
          url: `/fast-products/nexrad/${frame.filename}`,
        }));
      return {
        status: { state: "available", frame_count: frames.length } satisfies NexradLayerStatus,
        frames,
      };
    }

    loadNexrad().then(({ status, frames }) => {
      if (!cancelled) {
        setNexradStatus(status);
        setNexradFrames(frames);
        setNexradFrameIndex(0);
      }
    }).catch((error: unknown) => {
      if ((error as { name?: string } | null)?.name === "AbortError") {
        return;
      }
      console.warn("NEXRAD unavailable", error);
      if (!cancelled) {
        setNexradStatus({
          state: "unavailable",
          reason: error instanceof Error ? error.message : String(error),
        });
        setNexradFrames([]);
        setNexradFrameIndex(0);
      }
    });

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [mapLayerState.nexrad.enabled, mapLayerState.nexrad.visible, onPlaybackSnapshotChange, uiSession]);

  useEffect(() => {
    if (nexradFrames.length <= 1) {
      return;
    }
    const intervalId = window.setInterval(() => {
      setNexradFrameIndex((index) => (index + 1) % nexradFrames.length);
    }, NEXRAD_FRAME_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [nexradFrames.length]);

  useEffect(() => {
    const cache = terrainTileCacheRef.current;
    const sourceCache = terrainSourceByteCacheRef.current;
    terrainRenderWorkerRef.current = new TerrainRenderWorkerClient();
    return () => {
      cache.clear();
      sourceCache.clear();
      terrainRenderWorkerRef.current?.destroy();
      terrainRenderWorkerRef.current = null;
      terrainPendingFrameRef.current = null;
      terrainFrameStartRef.current.clear();
      terrainTileInFlightRef.current.clear();
      terrainRenderQueueRef.current.clear();
      terrainRenderSessionRef.current = null;
    };
  }, []);

  const terrainAltitudeBucket = terrainAltitudeBucketForOwnship(ownship);
  terrainCurrentBucketRef.current = terrainAltitudeBucket;

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      terrainRenderSessionRef.current = null;
      terrainCurrentBucketRef.current = null;
      terrainPendingFrameRef.current = null;
      terrainRenderQueueRef.current.clear();
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    if (!mapLayerState.terrain_warning.visible) {
      terrainRenderSessionRef.current = null;
      terrainCurrentBucketRef.current = null;
      terrainPendingFrameRef.current = null;
      terrainRenderQueueRef.current.clear();
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    const session = uiSession;
    terrainRenderSessionRef.current = session;
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
        setTerrainOverlay({ query, images: [] });
        return;
      }
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
          terrainRenderQueueRef.current.set(key, { request, altitudeBucket: terrainAltitudeBucket });
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
          request_products: requestSummary.products,
          missing_zooms: missingSummary.zooms,
          missing_products: missingSummary.products,
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
  }, [mapIsVisible, mapLayerState.terrain_warning.visible, surfaceSize.height, surfaceSize.width, terrainAltitudeBucket, uiSession, viewport]);

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
    let cancelled = false;

    async function resolveFlightPlanRoute() {
      if ((plan.resolved_legs ?? []).length === 0 || (planUiState?.resolved_legs ?? []).length === 0) {
        setFlightPlanRoute([]);
        return;
      }
      const startedAt = performance.now();
      const segments = await appCoreAdapter.projectFlightPlanRoute(plan, planUiState);
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
  }, [appCoreAdapter, onHighLatencyWarning, plan, planUiState]);

  useEffect(() => {
    if (!mapIsVisible) {
      return;
    }
    if (!mapLayerState.vectors.visible && !mapLayerState.metars.visible) {
      setMapOverlay({
        needed_point_tiles: [],
        needed_metar_tiles: [],
        needed_airspace_ref_tiles: [],
        needed_airspace_features: [],
        needed_airspace_label_tiles: [],
        needed_metars: false,
        needed_tfrs: false,
        visible_features: [],
        visible_metars: [],
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        warnings: [],
      });
      setMapOverlayViewport(null);
      return;
    }
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      setMapOverlay({
        needed_point_tiles: [],
        needed_metar_tiles: [],
        needed_airspace_ref_tiles: [],
        needed_airspace_features: [],
        needed_airspace_label_tiles: [],
        needed_metars: false,
        needed_tfrs: false,
        visible_features: [],
        visible_metars: [],
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        warnings: [],
      });
      return;
    }
    const session = uiSession;
    const controller = new AbortController();
    let cancelled = false;

    function reportOverlayWarnings(overlay: MapOverlayQueryResult, phase: string) {
      const warningKey = overlay.warnings.map((warning) => warning.code).sort().join(",");
      if (!warningKey || warningKey === lastOverlayWarningKeyRef.current) {
        if (!warningKey) {
          lastOverlayWarningKeyRef.current = "";
        }
        return;
      }
      lastOverlayWarningKeyRef.current = warningKey;
      onDebugWarning("map.overlay.warning", {
        phase,
        zoom: viewport.zoom,
        visible_features: overlay.visible_features.length,
        warnings: overlay.warnings,
      });
    }

    function overlayNeedsInputs(overlay: MapOverlayQueryResult): boolean {
      return (
        overlay.needed_point_tiles.length > 0 ||
        overlay.needed_metar_tiles.length > 0 ||
        overlay.needed_airspace_ref_tiles.length > 0 ||
        overlay.needed_airspace_features.length > 0 ||
        overlay.needed_airspace_label_tiles.length > 0 ||
        overlay.needed_metars ||
        overlay.needed_tfrs
      );
    }

    async function fetchMissingOverlayInputs(overlay: MapOverlayQueryResult): Promise<boolean> {
      let ingested = false;
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
            try {
              return (await response.json()) as PointTilePayload;
            } catch (error) {
              if (tile.layer === "obstacle") {
                debugLog("map.overlay.obstacle_tile.parse_fallback", {
                  z: tile.z,
                  x: tile.x,
                  y: tile.y,
                  error: errorMessage(error),
                });
                return {
                  schema_version: 1,
                  layer: tile.layer,
                  z: tile.z,
                  x: tile.x,
                  y: tile.y,
                  records: [],
                } satisfies PointTilePayload;
              }
              throw error;
            }
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
        return true;
      }
      if (overlay.needed_metar_tiles.length > 0) {
        const startedAt = performance.now();
        const tiles = await Promise.all(
          overlay.needed_metar_tiles.map(async (tile) => {
            const response = await fetch(metarTileUrl(tile.z, tile.x, tile.y), {
              signal: controller.signal,
            });
            if (response.status === 404) {
              return {
                schema_version: 1,
                layer: "metars",
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: [],
              } satisfies MetarTilePayload;
            }
            if (!response.ok) {
              throw new Error(`failed to load METAR tile ${tile.z}/${tile.x}/${tile.y}: ${response.status}`);
            }
            return (await response.json()) as MetarTilePayload;
          }),
        );
        await session.ingestMetarTiles(tiles);
        debugLog("map.overlay.metar_tiles.done", {
          zoom: viewport.zoom,
          count: tiles.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        return true;
      }
      if (overlay.needed_airspace_ref_tiles.length > 0) {
        const startedAt = performance.now();
        const tiles = await Promise.all(
          overlay.needed_airspace_ref_tiles.map(async (tile) => {
            const response = await fetch(airspaceReferenceTileUrl(tile.z, tile.x, tile.y), {
              signal: controller.signal,
            });
            if (response.status === 404) {
              return {
                schema_version: 1,
                layer: "airspace",
                z: tile.z,
                x: tile.x,
                y: tile.y,
                refs: [],
              } satisfies AirspaceReferenceTilePayload;
            }
            if (!response.ok) {
              throw new Error(`failed to load airspace ref tile ${tile.z}/${tile.x}/${tile.y}: ${response.status}`);
            }
            return (await response.json()) as AirspaceReferenceTilePayload;
          }),
        );
        await session.ingestAirspaceRefTiles(tiles);
        debugLog("map.overlay.airspace_refs.done", {
          zoom: viewport.zoom,
          count: tiles.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        return true;
      }
      if (overlay.needed_airspace_features.length > 0) {
        const startedAt = performance.now();
        const features = (
          await Promise.all(
            overlay.needed_airspace_features.map(async (feature) => {
              const response = await fetch(airspaceFeatureUrl(feature.path), {
                signal: controller.signal,
              });
              if (response.status === 404) {
                debugLog("map.overlay.airspace_feature.missing", {
                  id: feature.id,
                  path: feature.path,
                });
                return null;
              }
              if (!response.ok) {
                throw new Error(`failed to load airspace feature ${feature.path}: ${response.status}`);
              }
              return (await response.json()) as AirspaceFeaturePayload;
            }),
          )
        ).filter((feature): feature is AirspaceFeaturePayload => feature !== null);
        if (features.length > 0) {
          await session.ingestAirspaceFeatures(features);
        }
        debugLog("map.overlay.airspace_features.done", {
          zoom: viewport.zoom,
          count: features.length,
          missing: overlay.needed_airspace_features.length - features.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        if (features.length > 0) {
          return true;
        }
      }
      if (overlay.needed_airspace_label_tiles.length > 0) {
        const startedAt = performance.now();
        const tiles = await Promise.all(
          overlay.needed_airspace_label_tiles.map(async (tile) => {
            const response = await fetch(airspaceLabelTileUrl(tile.z, tile.x, tile.y), {
              signal: controller.signal,
            });
            if (response.status === 404) {
              return {
                schema_version: 1,
                layer: "airspace-labels",
                z: tile.z,
                x: tile.x,
                y: tile.y,
                labels: [],
              } satisfies AirspaceLabelTilePayload;
            }
            if (!response.ok) {
              throw new Error(`failed to load airspace label tile ${tile.z}/${tile.x}/${tile.y}: ${response.status}`);
            }
            return (await response.json()) as AirspaceLabelTilePayload;
          }),
        );
        await session.ingestAirspaceLabelTiles(tiles);
        debugLog("map.overlay.airspace_labels.done", {
          zoom: viewport.zoom,
          count: tiles.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        return true;
      }
      if (overlay.needed_tfrs) {
        const startedAt = performance.now();
        const response = await fetch("/fast-products/tfrs/tfrs.json", {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`failed to load TFR product: ${response.status}`);
        }
        const payload = (await response.json()) as TfrProductPayload;
        await session.ingestTfrs(payload);
        debugLog("map.overlay.tfrs.ingest.done", {
          zoom: viewport.zoom,
          areas: payload.areas.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        return true;
      }
      if (overlay.needed_metars) {
        const startedAt = performance.now();
        const response = await fetch("/fast-products/metars/metars.json", {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`failed to load METAR product: ${response.status}`);
        }
        const payload = (await response.json()) as MetarProductPayload;
        await session.ingestMetars(payload);
        debugLog("map.overlay.metars.ingest.done", {
          zoom: viewport.zoom,
          records: payload.metar_count ?? Object.keys(payload.metars_by_station).length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
        return true;
      }
      return ingested;
    }

    async function syncMapOverlay() {
      let overlay: MapOverlayQueryResult;
      const startedAt = performance.now();
      function publishOverlay(nextOverlay: MapOverlayQueryResult) {
        if (cancelled) {
          return;
        }
        setMapOverlay(nextOverlay);
        setMapOverlayViewport(viewport);
      }
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
          needed_metar_tiles: overlay.needed_metar_tiles.length,
          needed_airspace_ref_tiles: overlay.needed_airspace_ref_tiles.length,
          needed_airspace_features: overlay.needed_airspace_features.length,
          needed_airspace_label_tiles: overlay.needed_airspace_label_tiles.length,
          visible_features: overlay.visible_features.length,
          visible_metars: overlay.visible_metars.length,
          airspace_paths: overlay.airspace_paths.length,
          airspace_labels: overlay.airspace_labels.length,
          warnings: overlay.warnings.map((warning) => warning.code),
        });
        reportOverlayWarnings(overlay, "initial");
        publishOverlay(overlay);
      } catch (error) {
        if (isInvalidUiSessionHandleError(error)) {
          debugLog("map.overlay.query.stale_session", {
            zoom: viewport.zoom,
            elapsed_ms: Math.round(performance.now() - startedAt),
            error: errorMessage(error),
          });
          return;
        }
        debugLog("map.overlay.query.error", {
          zoom: viewport.zoom,
          elapsed_ms: Math.round(performance.now() - startedAt),
          error: errorMessage(error),
        });
        throw error;
      }
      for (let pass = 0; pass < 8 && overlayNeedsInputs(overlay); pass += 1) {
        const ingested = await fetchMissingOverlayInputs(overlay);
        if (!ingested) {
          break;
        }
        const refreshStartedAt = performance.now();
        try {
          overlay = await session.queryMapOverlay(viewport, surfaceSize.width, surfaceSize.height);
        } catch (error) {
          if (isInvalidUiSessionHandleError(error)) {
            debugLog("map.overlay.query.refresh.stale_session", {
              zoom: viewport.zoom,
              elapsed_ms: Math.round(performance.now() - refreshStartedAt),
              error: errorMessage(error),
            });
            return;
          }
          throw error;
        }
        debugLog("map.overlay.query.refresh.done", {
          zoom: viewport.zoom,
          elapsed_ms: Math.round(performance.now() - refreshStartedAt),
          needed_point_tiles: overlay.needed_point_tiles.length,
          needed_metar_tiles: overlay.needed_metar_tiles.length,
          needed_airspace_ref_tiles: overlay.needed_airspace_ref_tiles.length,
          needed_airspace_features: overlay.needed_airspace_features.length,
          needed_airspace_label_tiles: overlay.needed_airspace_label_tiles.length,
          visible_features: overlay.visible_features.length,
          visible_metars: overlay.visible_metars.length,
          airspace_paths: overlay.airspace_paths.length,
          airspace_labels: overlay.airspace_labels.length,
          warnings: overlay.warnings.map((warning) => warning.code),
        });
        reportOverlayWarnings(overlay, "refresh");
        publishOverlay(overlay);
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
  }, [
    mapLayerState.metars.visible,
    mapLayerState.vectors.visible,
    mapIsVisible,
    mapOverlayOwnshipKey,
    onDebugWarning,
    surfaceSize.height,
    surfaceSize.width,
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
  }, [mapOverlay.airspace_paths, mapOverlay.tfr_paths, mapOverlay.visible_features, mapSelection?.selectedItem?.highlight, surfaceSize.height, surfaceSize.width, viewport]);
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

  function updateViewport(next: MapViewportState) {
    viewportRef.current = next;
    onViewportChange(next);
  }

  function syncFollowStateForViewport(nextViewport: MapViewportState) {
    if (!uiSession || !mapFollowUiState.following || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    debugLog("map.follow.sync.request", {
      zoom: nextViewport.zoom,
      center_world_x: nextViewport.centerWorldX,
      center_world_y: nextViewport.centerWorldY,
      gesture_active: gestureActiveRef.current,
    });
    void uiSession
      .syncMapFollow(nextViewport, surfaceSize.width, surfaceSize.height)
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
    if (gestureActiveRef.current) {
      debugLog("map.follow.target.skip_during_gesture", {
        zoom: mapFollowTargetViewport.zoom,
        center_lat: mapFollowTargetViewport.center.lat,
        center_lon: mapFollowTargetViewport.center.lon,
      });
      return;
    }
    const nextViewport = mapViewportFromCore(mapFollowTargetViewport);
    if (!sameMapViewport(nextViewport, viewport)) {
      debugLog("map.follow.target.apply", {
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
      });
      updateViewport(nextViewport);
    }
  }, [mapFollowTargetViewport, mapFollowUiState.following, viewport]);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection) {
      return;
    }
    if (event.pointerType === "mouse") {
      activePointersRef.current.clear();
      dragRef.current = null;
      pinchRef.current = null;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    activePointersRef.current.set(event.pointerId, point);
    gestureActiveRef.current = activePointersRef.current.size > 0;
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
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
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
      debugLog("map.drag.viewport", {
        dx,
        dy,
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
        following: mapFollowUiState.following,
      });
      updateViewport(nextViewport);
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
        selectedMap.map_view,
        surfaceSize.width,
        surfaceSize.height,
      );
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
    gestureActiveRef.current = activePointersRef.current.size > 0;
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
        .queryMapSelection(viewportRef.current, surfaceSize.width, surfaceSize.height, click, thumbPixels(0.5))
        .then((result) => {
          setMapSelection({
            point: clickCandidate.latest,
            result,
            selectedItem: null,
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
    gestureActiveRef.current = activePointersRef.current.size > 0;
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

  function setViewportZoom(nextZoom: number) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap.map_view,
      { x: surfaceSize.width / 2, y: surfaceSize.height / 2 },
      surfaceSize.width,
      surfaceSize.height,
      nextZoom,
    );
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
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
      await recenterOnNavRef(navRef);
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
    const timing = pageTilePaintTiming;
    if (!timing || page !== "map" || tiles.length === 0) {
      return;
    }
    const loadedKeys = loadedRasterTileKeysRef.current;
    const allLoaded = tiles.every((entry) => loadedKeys.has(rasterTileKey(entry)));
    if (!allLoaded) {
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
    debugLog("map.raster.tile.error", {
      selected_map_id: selectedMap.id,
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
      id: "vectors",
      label: "Vectors",
      iconSrc: layerIconSrc("vectors"),
      toggleState: mapLayerState.vectors,
      disabled: !mapLayerState.vectors.enabled,
      onSelect: () => void setMapLayerVisible("vectors", !mapLayerState.vectors.visible),
    },
    {
      id: "metars",
      label: "METARs",
      iconSrc: layerIconSrc("metars"),
      toggleState: mapLayerState.metars,
      disabled: !mapLayerState.metars.enabled,
      onSelect: () => void setMapLayerVisible("metars", !mapLayerState.metars.visible),
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
  ];

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
        {mapSelection ? (
          <>
            <TrayScrim ariaLabel="Close map selection" onClose={() => setMapSelection(null)} />
            <MapSelectionTray
              point={mapSelection.point}
              result={mapSelection.result}
              selectedItem={mapSelection.selectedItem}
              onSelectItem={(item) => setMapSelection((current) => current ? { ...current, selectedItem: item } : current)}
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
                if (!item.nav_ref) {
                  return;
                }
                try {
                  if (!uiSession) {
                    throw new Error("map selection flight-plan action requires live core session");
                  }
                  const nextSnapshot = action.id === "remove_from_flight_plan"
                    ? await uiSession.removeTopLevelWaypointByNavRef(item.nav_ref)
                    : action.id === "insert"
                      ? await uiSession.insertWaypointBestPosition(item.nav_ref)
                      : null;
                  if (!nextSnapshot) {
                    return;
                  }
                  onPlaybackSnapshotChange(nextSnapshot);
                  setMapSelection(null);
                } catch (error) {
                  debugLog("map.selection.flight_plan_action.failed", {
                    action_id: action.id,
                    nav_ref: item.nav_ref,
                    error: errorMessage(error),
                  });
                }
              }}
            />
          </>
        ) : null}
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
              <img
                className="mapTileImage"
                src={tile.src}
                alt=""
                draggable={false}
                onLoad={() => reportRasterTileLoaded(tile)}
                onError={() => reportRasterTileError(tile)}
              />
              {debugTileLabels ? (
                <div className="tileLabel">
                  z{tile.zoom} x{tile.x} y{tile.yTms}
                </div>
              ) : null}
            </div>
          ))}
        </div>
        {nexradOverlay ? (
          <div className="nexradOverlay" aria-hidden="true">
            <img
              className="nexradFrame"
              src={nexradOverlay.frame.url}
              alt=""
              draggable={false}
              style={nexradOverlay.style}
            />
            <div className="nexradBadge">
              NEXRAD {formatNexradObservedTime(nexradOverlay.frame.observed_at_utc)}Z {nexradFrameIndex + 1}/{nexradFrames.length}
            </div>
          </div>
        ) : null}
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
              const parts = airspaceLabelParts(label.text);
              if (!parts) {
                return (
                  <g
                    key={`${label.feature_id}:${label.text}:${label.screen_x}:${label.screen_y}`}
                    className={`airspaceLabel airspaceLabel-${label.style_key}`}
                    transform={`translate(${label.screen_x} ${label.screen_y})`}
                  >
                    <text className="airspaceLabel" x="0" y="0">
                      {label.text}
                    </text>
                  </g>
                );
              }
              const dividerWidth = airspaceLabelDividerWidth(parts);
              return (
                <g
                  key={`${label.feature_id}:${label.text}:${label.screen_x}:${label.screen_y}`}
                  className={`airspaceFractionLabel airspaceLabel-${label.style_key}`}
                  transform={`translate(${label.screen_x} ${label.screen_y})`}
                >
                  <text className="airspaceLabel" x="0" y="-7">
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
                  <text className="airspaceLabel" x="0" y="9">
                    {parts.lower}
                  </text>
                </g>
              );
            })}
          </svg>
        ) : null}
        {mapIsVisible && routeScreenSegments.length > 0 ? (
          <svg className="vectorOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
            {routeScreenSegments.map((segment) => (
              <Fragment key={segment.id}>
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
        {mapIsVisible && mapOverlay.visible_features.length > 0 ? (
          <svg
            className="vectorOverlay"
            viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
            preserveAspectRatio="none"
            style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
          >
            {mapOverlay.visible_features.map((feature) => {
              return (
                <g key={feature.id} transform={`translate(${feature.screen_x} ${feature.screen_y})`}>
                  <VectorPointSymbol feature={feature} />
                </g>
              );
            })}
          </svg>
        ) : null}
        {mapIsVisible && mapOverlay.visible_metars.length > 0 ? (
          <svg
            className="metarOverlay"
            viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
            preserveAspectRatio="none"
            style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
          >
            {mapOverlay.visible_metars.map((feature) => (
              <g key={feature.station_id} transform={`translate(${feature.screen_x} ${feature.screen_y})`}>
                <MetarSymbol feature={feature} />
              </g>
            ))}
          </svg>
        ) : null}
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
            ) : (
              <g transform={`translate(${selectedMapHighlight.point.x} ${selectedMapHighlight.point.y})`}>
                <path className="mapSelectionSpotPegUnder" d={mapSelectionSpotPegPath} />
                <path className="mapSelectionSpotPeg" d={mapSelectionSpotPegPath} />
                <circle className="mapSelectionSpotPegDot" cx="0" cy="-23" r="4" />
              </g>
            )}
          </svg>
        ) : null}
        <SituationStatusBadge ownship={ownship} />
        {mapIsVisible && situationOverlay ? (
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
            options={familyOptions.map((family) => ({
              id: family.id,
              label: family.label,
              iconSrc: chartFamilyIconSrc(family.id),
              active: family.active,
              disabled: !family.enabled || !family.next_map_id,
              onSelect: () => {
                if (family.next_map_id) {
                  onSelectMapId(family.next_map_id);
                }
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
              void recenterOnNavRef(suggestion.nav_ref)
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

        {debugPlaybackVisible ? (
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
          minZoom={selectedMap.map_view.min_zoom}
          maxZoom={selectedMap.map_view.max_zoom}
          onZoomChange={setViewportZoom}
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

        <div className="debugDock isRightAligned" style={{ left: "auto", right: "calc(var(--thumb) + (var(--thumb-gap) * 2))" }}>
          <DebugDock
            open={debugOpen}
            warn={debugWarningActive || mapOverlay.warnings.length > 0}
            onToggle={() => setDebugOpen((open) => !open)}
          >
            <div className="debugLine">up: {uptimeLabel}</div>
            <div className="debugLine">{center.lat.toFixed(3)}/{center.lon.toFixed(3)} z{viewport.zoom.toFixed(2)}</div>
            <label className="debugToggle">
              <input
                type="checkbox"
                checked={debugTileLabels}
                onChange={(event) => onDebugTileLabelsChange(event.currentTarget.checked)}
              />
              tile labels
            </label>
            <label className="debugToggle">
              <input
                type="checkbox"
                checked={debugPlaybackVisible}
                onChange={(event) => onDebugPlaybackVisibleChange(event.currentTarget.checked)}
              />
              playback
            </label>
          </DebugDock>
        </div>
      </div>
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
  if (!navElement) {
    return null;
  }
  return (
    <button
      type="button"
      className={className}
      onPointerDown={onPointerDown}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      onClick={onClick}
    >
      <NavElementView navElement={navElement} />
    </button>
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
  const canSeek = durationSeconds > 0;
  const summary = playbackUiState.title_label;
  const overviewWidth = 320;
  const overviewHeight = 34;
  const knobRadius = 7;
  const scrubSurfaceHeight = 50;
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
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  legSummary: string;
  plan: FlightPlan;
  planUiState: FlightPlanUiState | null;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenCharts: (airportId: string | null, chartId?: string | null) => void;
  onMoveComponent: (componentIndex: number, delta: number) => void | Promise<void>;
  onRemoveAllAbove: (componentIndex: number) => void | Promise<void>;
  onInsertAirportWaypoint: (componentIndex: number, before: boolean, airportId: string) => void | Promise<void>;
  onPreviewFlightPlanEntry: (input: string) => Promise<FlightPlanEntryPreview>;
  onAppendFlightPlanEntry: (input: string) => void | Promise<void>;
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
  debugWarningActive: boolean;
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
  const [airportInsert, setAirportInsert] = useState<{
    componentIndex: number;
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
  const [debugOpen, setDebugOpen] = useState(false);
  const pageRef = useRef<HTMLElement | null>(null);
  const planScrollSurfaceRef = useRef<HTMLDivElement | null>(null);
  const waypointModalRef = useRef<HTMLElement | null>(null);
  const planControlsRef = useRef<HTMLDivElement | null>(null);
  const planFooterRef = useRef<HTMLDivElement | null>(null);
  const trayOpen = false;
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
  const componentViews = useMemo(() => planUiState.components, [planUiState.components]);
  if (planUiState.resolved_legs.length > 0 && componentViews.length === 0) {
    throw new Error("FlightPlanUiState invariant failed: resolved legs present but components are empty");
  }
  const waypointSuggestionPlanKey = useMemo(() => JSON.stringify(props.plan), [props.plan]);
  useEffect(() => {
    const editor = airportInsert;
    const adapter = props.appCoreAdapter;
    if (!editor || !adapter) {
      return;
    }
    const prefix = editor.airportId.trim().toUpperCase();
    if (!prefix) {
      setAirportInsert((current) => current ? { ...current, loading: false, suggestions: [] } : current);
      return;
    }
    let cancelled = false;
    setAirportInsert((current) => current ? { ...current, loading: true } : current);
    adapter
      .suggestWaypointIdentifiers(props.plan, editor.componentIndex, editor.before, prefix, 8)
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
  }, [airportInsert?.airportId, airportInsert?.before, airportInsert?.componentIndex, props.appCoreAdapter, waypointSuggestionPlanKey]);
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
    props.onPreviewFlightPlanEntry(routeEntryText)
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
  }, [props.onPreviewFlightPlanEntry, routeEntryText, waypointSuggestionPlanKey]);
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
        distance: row.row_kind === "group" ? "" : formatPlanDistance(row.distance_nm),
        eta: row.eta_text,
        legTime: row.leg_time_text,
        fuel: row.fuel_gal_text,
        course: row.row_kind === "group" ? "" : formatPlanCourse(row.course_deg),
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
        symbolFeature: row.symbol_feature,
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
      setAirportInsert(null);
    };

    return (selectedRow.actions as Array<{ id: string; label: string; enabled: boolean }>).map((action) => {
      return {
        id: action.id,
        label: action.label,
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
          if (action.id === "remove_all_above") {
            if (selectedRow.componentIndex == null) {
              return;
            }
            void Promise.resolve(props.onRemoveAllAbove(selectedRow.componentIndex))
              .catch(() => {});
            closeTray();
            return;
          }
          if (action.id === "reorder") {
            setReorderOpen(true);
            return;
          }
          if (action.id === "insert_before" || action.id === "insert_after") {
            if (selectedRow.componentIndex === null) {
              return;
            }
            setAirportInsert({
              componentIndex: selectedRow.componentIndex,
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
  }, [displayRows, reorderOpen]);

  useEffect(() => {
    if (!guidance?.active_leg) {
      setStructuredArrow(null);
      return;
    }
    const activeLeg = guidance.active_leg;
    const scrollPane = planScrollSurfaceRef.current;
    const content = planScrollContentRef.current;
    if (!scrollPane || !content) {
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

    let animationFrame = 0;
    const measureArrow = () => {
      const surfaceRect = content.getBoundingClientRect();
      const fromRect = fromElement.getBoundingClientRect();
      const toRect = toElement.getBoundingClientRect();
      const leftGutterX = thumbPixels(0.12);
      const waypointColumnLeftX = thumbPixels(0.5);
      const waypointColumnHeadInsetX = thumbPixels(0.08);
      const fromPoint = {
        x: waypointColumnLeftX,
        y: fromRect.top - surfaceRect.top + fromRect.height / 2,
      };
      const toPoint = {
        x: waypointColumnLeftX + waypointColumnHeadInsetX,
        y: toRect.top - surfaceRect.top + toRect.height / 2,
      };
      const elbowX = leftGutterX;
      const headLength = 20;
      const shaftEnd = { x: Math.max(elbowX, toPoint.x - headLength + 5), y: toPoint.y };

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
      fromElement.scrollIntoView({ block: "nearest", inline: "nearest" });
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
    if (selectedWaypointIndex === null) {
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
    const top = thumbPixels(0.5);
    const bottomPadding = thumbPixels(0.1);
    const blockers = [planControlsRef.current, planFooterRef.current]
      .flatMap((element) => (element ? [element.getBoundingClientRect().top - pageRect.top] : []));
    const bottomLimit = blockers.length > 0 ? Math.min(...blockers) : page.clientHeight;
    const maxHeight = Math.max(thumbPixels(1), bottomLimit - top - bottomPadding);

    setWaypointModalTop(top);
    setWaypointModalMaxHeight(maxHeight);
  }, [airwayPicker, reorderOpen, selectedWaypointIndex, rowActions.length]);

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
      <div className="chartDock">
        <HomeNavButton active={props.page === "home"} onClick={() => props.onSelectPage("home")} />
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
            <div className={`planTableWrap isStructured${reorderOpen ? " isReordering" : ""}`} ref={structuredSurfaceRef}>
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
                <div className="planHeader">Dist (nm)</div>
                <div className="planHeader">ETA (h:m)</div>
                <div className="planHeader">Leg (h:m)</div>
                <div className="planHeader">Fuel (gal)</div>
                <div className="planHeader">Course (°)</div>
                {displayRows.map((row, index) => (
                  <Fragment key={row.id}>
                    <button
                      key={`${row.id}:waypoint`}
	                  type="button"
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
	                    "planStructuredWaypointCell",
	                    row.rowKind === "group" ? "isGroupHeader" : "",
	                    row.depth > 0 ? "isChildRow" : "",
	                    row.rowKind === "discontinuity" ? "isDiscontinuityItem" : "",
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
	                  <span className={`planStructuredLabel${row.depth > 0 ? " isIndented" : ""}`}>{row.label}</span>
                    <PlanWaypointSymbol feature={row.symbolFeature} />
                    </button>
	                <div
	                  className={[
	                    "planCell",
	                    row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
	                  ].filter(Boolean).join(" ")}
                >
                  {row.distance}
                </div>
	                <div
	                  className={[
	                    "planCell",
	                    row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
	                  ].filter(Boolean).join(" ")}
	                >
                  {row.eta}
                </div>
	                <div
	                  className={[
	                    "planCell",
	                    row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
	                  ].filter(Boolean).join(" ")}
	                >
                  {row.legTime}
                </div>
	                <div
	                  className={[
	                    "planCell",
	                    row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
	                  ].filter(Boolean).join(" ")}
	                >
                  {row.fuel}
                </div>
	                <div
	                  className={[
	                    "planCell",
	                    row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
	                  ].filter(Boolean).join(" ")}
	                >
                  {row.course}
                    </div>
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
                <div className="planEntryFeedback">{routeEntryError}</div>
              ) : routeEntryPreview.issues[0] ? (
                <div className="planEntryFeedback">{routeEntryPreview.issues[0].message}</div>
              ) : routeEntryLoading ? (
                <div className="planEntryFeedback">Checking...</div>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      <div className="planControls" ref={planControlsRef}>
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

      <div className="planFooter" ref={planFooterRef}>
        <NavElementButton
          navElement={planUiState.guidance?.nav_element}
          className="navElement navElementStatic"
          onClick={props.onOpenRecentChartOrPlate}
        />
      </div>

      <div className="debugDock">
        <DebugDock open={debugOpen} warn={props.debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
          <div className="debugLine">up: {props.uptimeLabel}</div>
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
              setAirportInsert(null);
            }}
          />
          <section
            ref={waypointModalRef}
            className={`waypointModal${reorderOpen ? " isReorder" : ""}${airportInsert ? " isAirportInsert" : ""}`}
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
                    await props.onInsertAirportWaypoint(airportInsert.componentIndex, airportInsert.before, airportId);
                    setAirportInsert(null);
                    setSelectedWaypointIndex(null);
                  } catch (error) {
                    setAirportInsert((current) => current ? {
                      ...current,
                      error: error instanceof Error ? error.message : String(error),
                    } : current);
                  }
                }}
              >
                <div className="airportInsertInputRow">
                  <div className="planGuidanceSummary airportInsertTitle">
                    {airportInsert.before ? "INSERT BEFORE" : "INSERT AFTER"}
                  </div>
                  <input
                    className="airportInsertInput"
                    autoFocus
                    value={airportInsert.airportId}
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
                  <button type="submit" className="trayButton airwayChoiceButton airportInsertEnter" onPointerDown={stopPointer} onPointerUp={stopPointer}>
                    Enter
                  </button>
                </div>
                {airportInsert.error ? <div className="planGuidanceSummary">{airportInsert.error}</div> : null}
                {airportInsert.loading ? <div className="planGuidanceSummary">Searching...</div> : null}
                {airportInsert.suggestions.length > 0 ? (
                  <div className="airportInsertSuggestions">
                    {airportInsert.suggestions.map((suggestion) => (
                      <button
                        key={`${suggestion.kind}:${suggestion.identifier}`}
                        type="button"
                        className="trayButton airwayChoiceButton airportInsertSuggestion"
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          try {
                            await props.onInsertAirportWaypoint(
                              airportInsert.componentIndex,
                              airportInsert.before,
                              suggestion.identifier,
                            );
                            setAirportInsert(null);
                            setSelectedWaypointIndex(null);
                          } catch (error) {
                            setAirportInsert((current) => current ? {
                              ...current,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        <span className="airportInsertSuggestionMain">
                          <span>{suggestion.identifier}</span>
                          {suggestion.display_name ? <span className="airportInsertSuggestionName">{suggestion.display_name}</span> : null}
                        </span>
                        <span className="airportInsertSuggestionMeta">{suggestion.kind.toUpperCase()} {suggestion.distance_from_anchor_nm.toFixed(1)}nm</span>
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
                className="trayButton airwayChoiceButton"
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

function HomeNavButton(props: {
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`chartButton${props.active ? " isOpen" : ""}`}
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
}) {
  const { launcherLabel, launcherImageSrc, launcherStyle, open, onToggle, ariaLabel, disabled = false, style = "compact", launcherClassName, launcherAccentColor, options } = props;
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const trayRef = useRef<HTMLElement | null>(null);
  const [trayPosition, setTrayPosition] = useState<{ left: number; top: number } | null>(null);
  const [trayThemeStyle, setTrayThemeStyle] = useState<CSSProperties | null>(null);
  const launcherWide = style === "plate_wide" || style === "wide";
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
                  className={`trayButton${option.active ? " isActive" : ""}${option.iconSrc ? " trayButtonWithIcon" : ""}${option.toggleState ? " trayButtonHasToggle" : ""}${option.toggleState?.visible && option.toggleState.enabled ? " isOn" : ""}${option.toggleState && option.toggleState.enabled && !option.toggleState.visible ? " isOff" : ""}`}
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
              className="trayButton airwayChoiceButton airportInsertSuggestion chartSearchSuggestion"
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => onSelect(suggestion)}
            >
              <span className="airportInsertSuggestionMain">
                <span>{suggestion.identifier}</span>
                {suggestion.display_name ? <span className="airportInsertSuggestionName">{suggestion.display_name}</span> : null}
              </span>
              <span className="airportInsertSuggestionMeta">{suggestion.kind.toUpperCase()} {suggestion.distance_from_anchor_nm.toFixed(1)}nm</span>
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
  onSelectAction: (item: MapSelectionItem, action: MapSelectionItem["actions"][number]) => void | Promise<void>;
}) {
  const { point, result, selectedItem, onSelectItem, onSelectAction } = props;
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
  const horizontalStyle = point.x < window.innerWidth / 2
    ? { right: `${edgePad}px` }
    : { left: `${edgePad}px` };
  const verticalStyle = point.y < window.innerHeight / 2
    ? { bottom: `${edgePad}px` }
    : { top: `${edgePad}px` };

  return (
    <section
      className="mapSelectionTray"
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
        <div className="mapSelectionActionTitle">{selectedItem?.label ?? "\u00a0"}</div>
        <div className="mapSelectionActionGrid">
          {actionSlots.slice(0, 6).map((action) => (
            <button
              key={action.id}
              type="button"
              className={`mapSelectionAction${action.display_only ? " isDisplayOnly" : ""}${action.placeholder ? " isPlaceholder" : ""}`}
              disabled={!action.enabled}
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => {
                if (selectedItem && action.enabled && !action.display_only) {
                  void onSelectAction(selectedItem, action);
                }
              }}
              aria-hidden={action.placeholder ? "true" : undefined}
              tabIndex={action.placeholder ? -1 : undefined}
            >
              {action.label}
            </button>
          ))}
        </div>
      </div>
    </section>
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
  if (item.highlight.kind === "spot") {
    return (
      <svg className="mapSelectionItemIcon mapSelectionSpotIcon" viewBox="-20 -40 40 46" aria-hidden="true">
        <path className="mapSelectionSpotPegUnder" d={mapSelectionSpotPegPath} />
        <path className="mapSelectionSpotPeg" d={mapSelectionSpotPegPath} />
        <circle className="mapSelectionSpotPegDot" cx="0" cy="-23" r="4" />
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
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
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
  onApplyMutation: (mutation: FlightPlanUiMutation) => void | Promise<void>;
  playbackUiState: PlaybackUiState;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  debugPlaybackVisible: boolean;
  onDebugPlaybackVisibleChange: (enabled: boolean) => void;
  uiSession: UiSession | null;
  ownship: OwnshipRenderState;
  debugWarningActive: boolean;
  onFirstVisualReady: () => void;
}) {
  const { appCoreAdapter, page, pageHistory, uptimeLabel, plan, planUiState, airports, selectedAirport, selectedChart, folderOpen, viewport, onViewportChange, onFolderOpenChange, onSelectPage, onOpenPlan, onSelectAirport, onSelectChart, onApplyMutation, ownship, onFirstVisualReady } = props;
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
  const lastChartLayoutKeyRef = useRef("");
  const firstVisualReadyRef = useRef(false);
  const trayGroup = useModalTrayGroup(["airport", "chart", "load"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const [plateProcedureLoads, setPlateProcedureLoads] = useState<ProcedureLoadOption[]>([]);
  const trayOpen = trayGroup.scrimOpen;
  const sortedCharts = selectedAirport?.charts ?? [];
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
        <SituationStatusBadge ownship={ownship} />
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
          <>
            <img
              key={selectedChart.id}
              ref={imageRef}
              className="chartImage"
              src={selectedChart.asset_url}
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

        <NavElementButton
          navElement={planUiState?.guidance?.nav_element}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        />

        {props.debugPlaybackVisible ? (
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

        <div className="debugDock">
          <DebugDock open={debugOpen} warn={props.debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
            <div className="debugLine">up: {uptimeLabel}</div>
            <div className="debugLine">{viewport ? `z${viewport.zoom.toFixed(2)}` : "viewport (none)"}</div>
            <label className="debugToggle">
              <input
                type="checkbox"
                checked={props.debugPlaybackVisible}
                onChange={(event) => props.onDebugPlaybackVisibleChange(event.currentTarget.checked)}
              />
              playback
            </label>
          </DebugDock>
        </div>

      </div>
    </section>
  );
}

function HomePage(props: {
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  uptimeLabel: string;
  planUiState: FlightPlanUiState | null;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  debugWarningActive: boolean;
}) {
  const { page, pageHistory, uptimeLabel, planUiState, onSelectPage, onOpenPlan, debugWarningActive } = props;
  const [debugOpen, setDebugOpen] = useState(false);
  const homeButtons: Array<{ id: string; label: string; page: AppPage; iconSrc?: string }> = [
    { id: "chart", label: "CHART", page: "map", iconSrc: PAGE_CHART_ICON_SRC },
    { id: "plate", label: "PLATE", page: "charts", iconSrc: PAGE_PLATE_ICON_SRC },
    { id: "flight-plan", label: "FLIGHT\nPLAN", page: "plan" },
  ];
  const placeholderLabels = ["S4", "S5", "S6", "S7", "S8", "S9"];

  return (
    <section className="appPage planPage">
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

      <div className="debugDock">
        <DebugDock open={debugOpen} warn={debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
          <div className="debugLine">up: {uptimeLabel}</div>
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
      page: page === "map" || page === "plan" || page === "charts" || page === "home" ? page : undefined,
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

function moveAirportToFront(
  currentIds: string[],
  airportId: string,
  airports: ChartPageData["airports"],
) {
  const mergedIds = mergeRecentAirportIds(airports, [airportId, ...currentIds.filter((id) => id !== airportId)]);
  return mergedIds.includes(airportId) ? mergedIds : [airportId, ...mergedIds];
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

function resolveChartId(
  airports: ChartPageData["airports"],
  airportId: string,
  candidateChartId: string | undefined,
) {
  const airport = airports.find((entry) => entry.id === airportId);
  if (candidateChartId === `Plate:${airportId}:CSup`) {
    return airport?.charts.find((chart) => chart.kind === "csup" || chart.folder_category === "csup")?.id ?? "";
  }
  if (candidateChartId === `Plate:${airportId}:Folder`) {
    return airport?.charts[0]?.id ?? "";
  }
  if (candidateChartId && airport?.charts.some((chart) => chart.id === candidateChartId)) {
    return candidateChartId;
  }
  return airport?.charts[0]?.id ?? "";
}

function airportIdFromNavRef(navRef: NavRef | null | undefined): string | null {
  return navRef && "Airport" in navRef ? navRef.Airport : null;
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
    return snapshot.selectedMapId ? `${label}-${snapshot.selectedMapId.toUpperCase()}` : label;
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

function filterRenderableFamilyMapViews(
  selectedMap: MapViewOptionJson,
  familyMapViews: MapViewOptionJson[],
  viewport: MapViewportState,
): MapViewOptionJson[] {
  const grouped = new Map<string, MapViewOptionJson[]>();
  for (const view of familyMapViews) {
    const key = view.map_view.chart_family;
    const group = grouped.get(key);
    if (group) {
      group.push(view);
    } else {
      grouped.set(key, [view]);
    }
  }
  return [...grouped.values()]
    .flatMap((views) => {
      const fullCoverageZooms = views
        .map((view) => view.map_view.full_coverage_zoom)
        .filter((zoom): zoom is number => zoom != null);
      const collapseBelowZoom = fullCoverageZooms.length > 0 ? Math.min(...fullCoverageZooms) : null;
      if (collapseBelowZoom == null || viewport.zoom > collapseBelowZoom || views.length <= 1) {
        return views;
      }
      return [views.find((view) => view.region_id === selectedMap.region_id) ?? views[0]];
    })
    .sort((left, right) => {
      const familyDelta = left.map_view.chart_family.localeCompare(right.map_view.chart_family);
      if (familyDelta !== 0) {
        return familyDelta;
      }
      return left.id.localeCompare(right.id);
    });
}

function SituationStatusBadge(props: { ownship: OwnshipRenderState }) {
  const tone =
    props.ownship.mode === "none"
      ? "unknown"
      : props.ownship.mode === "simulated"
        ? "simulated"
        : "live";
  const label = props.ownship.banner_text;
  return <div className={`situationStatus situationStatus-${tone}`}>{label}</div>;
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
  const ring = selectSituationRing(ownship.position.lat, ownship.position.lon, viewport, width, height, ringCandidates);
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
  ringCandidates: SituationRingCandidate[],
) {
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
    tickMarks: buildRingTickMarks(center, best.radiusPx),
    cardinalLabels: buildRingCardinalLabels(center, best.radiusPx),
    label: {
      point: labelPoint,
      rotationDeg: 45,
      text: best.label,
    },
  };
}

function buildRingCardinalLabels(center: { x: number; y: number }, radiusPx: number) {
  const labelRadius = Math.max(0, radiusPx - 30);
  return [
    { text: "N", angleDeg: -90, rotationDeg: 0 },
    { text: "E", angleDeg: 0, rotationDeg: 90 },
    { text: "S", angleDeg: 90, rotationDeg: 0 },
    { text: "W", angleDeg: 180, rotationDeg: -90 },
  ].map((label) => ({
    ...label,
    point: pointOnCircle(center, labelRadius, label.angleDeg),
  }));
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

function formatPlanDistance(distanceNm: number | null) {
  if (distanceNm === null) {
    return "—";
  }
  if (distanceNm < 10) {
    return distanceNm.toFixed(1);
  }
  return distanceNm.toFixed(0);
}

function formatPlanCourse(courseDeg: number | null) {
  if (courseDeg === null) {
    return "—";
  }
  const rounded = Math.round(courseDeg) % 360;
  return rounded === 0 ? "360" : rounded.toString().padStart(3, "0");
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

async function applyFlightPlanMutation(
  uiSession: UiSession | null,
  setSessionSnapshot: Dispatch<SetStateAction<UiSessionSnapshot>>,
  mutation: FlightPlanUiMutation,
) {
  if (!uiSession) {
    throw new Error("flight plan mutation requires live core session");
  }
  const nextSnapshot = await uiSession.replaceFlightPlan(mutation.plan);
  setSessionSnapshot(nextSnapshot);
}

async function buildSeededDevPlan(): Promise<{
  plan: FlightPlan;
  selectedAirportId?: string;
  selectedChartId?: string;
  recentAirportIds?: string[];
}> {
  const waypoints: Array<{ Airport: string } | { Navaid: string } | { Fix: string }> = [
    { Airport: "KRNT" },
    { Navaid: "SEA" },
    { Airport: "KPAE" },
  ];
  const routeComponents = waypoints.map((waypoint) => ({ kind: "waypoint" as const, waypoint }));
  const resolvedLegs = waypoints.slice(0, -1).map((from, index) => ({
    id: `component-${index}-${index + 1}`,
    from,
    to: waypoints[index + 1],
    source: { kind: "route_component" as const, component_index: index },
  }));
  const plan = {
    ...samplePlan,
    id: "dev-krnt-sea-kpae",
    name: "KRNT SEA KPAE",
    legs: resolvedLegs.map((leg) => ({ from: leg.from, to: leg.to, airway: null })),
    route_components: routeComponents,
    resolved_legs: resolvedLegs,
    guidance: { active_leg_index: 0, active_detail_index: 0, sequencing_mode: "follow_plan" as const, direct_to: null },
    departure: "KRNT",
    destination: "KPAE",
    updated_at_epoch_ms: Date.now(),
    version: samplePlan.version + 1,
  };
  return {
    plan,
    selectedAirportId: "KPAE",
    recentAirportIds: ["KPAE", "KRNT"],
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
