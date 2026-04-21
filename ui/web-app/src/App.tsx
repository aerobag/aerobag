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
  FlightPlanRouteSegment,
  FlightPlanUiMutation,
  FlightPlanUiState,
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
import {
  airspaceFeatureUrl,
  airspaceLabelTileUrl,
  airspaceReferenceTileUrl,
  pointTileUrl,
  type PointTilePayload,
} from "./domain/vectorTiles";
import type {
  AirspaceDisplayPath,
  AirspaceFeaturePayload,
  AirspaceLabelTilePayload,
  AirspaceReferenceTilePayload,
  MapOverlayQueryResult,
  TerrainOverlayQueryResult,
  TerrainOverlayTileRequest,
} from "./domain/appCoreAdapter";
import { airwayEntryCandidateFromPresentation, airwayExitCandidatesFromPresentation } from "./domain/airwayPresentation";
import { debugLog, debugTiming } from "./domain/debugLog";
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

type AppPage = "map" | "plan" | "charts" | "settings";

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
    cdi_pointer: string;
  };
  aviation: {
    class_b_d_blue: string;
    class_c_magenta: string;
    intersection_cyan: string;
    dark_gray: string;
  };
  plate_folder: {
    thumbnail_bg: string;
    label_colors: Record<string, string>;
  };
};

type AviationThemeColorKey = keyof UiThemeJson["aviation"];

type TrayDockStyle = "compact" | "plate_narrow" | "plate_wide";
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

const chartFamilies: Array<{ id: ChartFamilyId; label: string; launcherLabel: string }> = [
  { id: "sec", label: "SECTIONAL", launcherLabel: "SEC" },
  { id: "tac", label: "TAC", launcherLabel: "TAC" },
  { id: "enr-l", label: "IFR-LOW", launcherLabel: "IFR L" },
  { id: "enr-h", label: "IFR-HIGH", launcherLabel: "IFR H" },
  { id: "shaded-relief", label: "SHADED RELIEF", launcherLabel: "RELIEF" },
];

function mapViewsForDisplayedFamily(
  allMapViews: MapViewOptionJson[],
  familyId: ChartFamilyId,
): MapViewOptionJson[] {
  if (familyId === "tac") {
    return allMapViews.filter((view) => {
      const chartFamily = view.map_view.chart_family;
      return chartFamily === "sec" || chartFamily === "tac";
    });
  }
  return allMapViews.filter((view) => view.map_view.chart_family === familyId);
}

function preferredFamilyMap(
  allMapViews: MapViewOptionJson[],
  familyId: ChartFamilyId,
  fallbackRegionId: string | null,
): MapViewOptionJson | undefined {
  const familyMaps = allMapViews.filter((view) => view.map_view.chart_family === familyId);
  return (
    familyMaps.find((view) => view.region_id === fallbackRegionId)
    ?? familyMaps[0]
  );
}

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
const VAMPS_POSITION = { lat: 47.3648944444444, lon: -121.980275 };
const defaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json";
const startupHighLatencyWarningGraceMs = 10_000;
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

type VectorPointSymbolFeature = {
  kind: string;
  label: string;
  style_class: string;
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
          <path d={airportCircleMarkerPath} className={`${airportClass} airportOpenMarker`} />
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
            d="M 0 -9 L 0 5 M -4 -5 A 4 4 0 1 1 4 -5 M -7 2 C -5 8 5 8 7 2 M -9 2 L -5 2 M 9 2 L 5 2"
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

function initialMapId(mapViews: MapViewOptionJson[]) {
  return preferredFamilyMap(mapViews, "tac", "nw")?.id ?? mapViews[0]?.id ?? "";
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
  const startupVisualReadyRef = useRef(false);
  const highLatencyWarningsSuppressedRef = useRef(true);
  const highLatencyWarningTimerRef = useRef<number | null>(null);
  const [mapViews, setMapViews] = useState<MapViewOptionJson[]>([]);
  const [mapViewsLoadError, setMapViewsLoadError] = useState<string | null>(null);
  const [chartPageCatalog, setChartPageCatalog] = useState<ChartPageData | null>(null);
  const [chartPageCatalogLoadError, setChartPageCatalogLoadError] = useState<string | null>(null);
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
  });
  const [playbackSourcePath, setPlaybackSourcePath] = useState(defaultPlaybackTracePath);
  const [debugWarningActive, setDebugWarningActive] = useState(false);
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
  const chartCatalog: ChartPageData = uiSession?.chartCatalog ?? chartPageCatalog ?? emptyChartPage;
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
  const recentAirportIds = sessionSnapshot.chart_page_state.recent_airport_ids;
  const selectedAirportId = sessionSnapshot.chart_page_state.selected_airport_id;
  const selectedChartId = sessionSnapshot.chart_page_state.selected_chart_id;

  const selectedMap = useMemo(
    () => mapViews.find((view) => view.id === selectedMapId) ?? mapViews[0] ?? null,
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
    () => chartFamilies.find((family) => family.id === selectedMap?.map_view.chart_family) ?? chartFamilies[0],
    [selectedMap],
  );
  const availableFamilies = useMemo(
    () => new Set(mapViews.map((view) => view.map_view.chart_family)),
    [mapViews],
  );
  const selectedFamilyMapViews = useMemo(
    () => selectedMap ? mapViewsForDisplayedFamily(mapViews, selectedMap.map_view.chart_family) : [],
    [mapViews, selectedMap],
  );
  const selectedAirport = useMemo(
    () => chartPageData.airports.find((airport) => airport.id === selectedAirportId) ?? chartPageData.airports[0] ?? null,
    [chartPageData, selectedAirportId],
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
    if (!appCoreAdapter || !chartPageCatalog) {
      return;
    }
    debugTiming("startup.session.create", () => buildSeededDevPlan().then(async (initialPlan) => {
      const created = await debugTiming("startup.session.create.core", () => appCoreAdapter.createUiSession(
        chartPageCatalog,
        initialPlan.plan,
        initialRecentAirportIds,
        initialChartPageState.selected_airport_id,
        initialChartPageState.selected_chart_id,
      ));
      const createdSnapshot = await debugTiming("startup.session.snapshot", () => created.snapshot());
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
  }, [adapterBackend, appCoreAdapter, chartPageCatalog, initialChartPageState.selected_airport_id, initialChartPageState.selected_chart_id, initialRecentAirportIds]);

  useEffect(() => {
    let cancelled = false;
    debugTiming("startup.chart_catalog.load", () => runCoreHadOperation<MapViewOptionJson[] | null>({ kind: "chart_catalog" })).then((loaded) => {
      if (cancelled) {
        return;
      }
      if (!loaded || loaded.length === 0) {
        throw new Error("nav_kv chart/catalog is missing or empty");
      }
      setMapViews(loaded);
      setSelectedMapId((current) =>
        loaded.some((view) => view.id === current)
          ? current
          : initialMapId(loaded),
      );
    }).catch((error) => {
      if (!cancelled) {
        setMapViewsLoadError(`failed to load chart catalog: ${errorMessage(error)}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    debugTiming("startup.chart_page_catalog.load", () => buildSeededDevPlan().then(async ({ plan }) => {
      const airportIds = airportIdsNeededForInitialChartPage(
        plan,
        initialRecentAirportIds,
        persistedUiState.selectedAirportId,
      );
      if (persistedUiState.selectedChartId) {
        const selectedChart = await runCoreHadOperation<ChartPageData["airports"][number]["charts"][number] | null>({
          kind: "plate_by_id",
          plate_id: persistedUiState.selectedChartId,
        });
        if (selectedChart?.airport_id) {
          airportIds.add(selectedChart.airport_id);
        }
      }
      debugLog("startup.chart_page_catalog.airports", { count: airportIds.size, airport_ids: [...airportIds] });
      const airports = (await Promise.all([...airportIds].map((airportId) => debugTiming(
        "startup.chart_page_catalog.airport.load",
        () => runCoreHadOperation<ChartPageData["airports"][number] | null>({ kind: "plate_airport", airport_id: airportId }),
        { airport_id: airportId },
      ))))
        .filter((airport): airport is ChartPageData["airports"][number] => airport !== null);
      return { airports };
    })).then((loaded) => {
      if (cancelled) {
        return;
      }
      setChartPageCatalog(loaded);
    }).catch((error) => {
      if (!cancelled) {
        setChartPageCatalogLoadError(`failed to load chart page catalog: ${errorMessage(error)}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [initialRecentAirportIds, persistedUiState.selectedAirportId, persistedUiState.selectedChartId]);

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
        "--theme-cdi-pointer": controlTheme.cdi_pointer,
        "--theme-class-b-d-blue": loadedUiTheme.aviation.class_b_d_blue,
        "--theme-class-c-magenta": loadedUiTheme.aviation.class_c_magenta,
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
      mapViewsLoadError !== null ||
      chartPageCatalogLoadError !== null ||
      (appReady &&
        currentPlan !== null &&
        planUiState !== null);
    if (shouldHideStartupShell) {
      window.__aerobag_hide_startup_shell?.();
    }
  }, [appReady, chartPageCatalogLoadError, currentPlan, mapViewsLoadError, planUiState, sessionInitError]);

  if (sessionInitError || mapViewsLoadError || chartPageCatalogLoadError) {
    return (
      <main className="appFrame">
        <section className="appPage planPage">
          <div className="planGuidanceSummary">{sessionInitError ?? mapViewsLoadError ?? chartPageCatalogLoadError}</div>
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
          selectedMapId={selectedMapId}
          mapViews={mapViews}
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
          adapterBackend={adapterBackend}
          adapterDetail={adapterDetail}
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
          ownship={appUiState.ownship.render}
          onApplyMutation={async (mutation) => {
            await applyFlightPlanMutation(uiSession, setSessionSnapshot, mutation);
          }}
          playbackUiState={playbackUiState}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={setSessionSnapshot}
          uiSession={uiSession}
          debugWarningActive={debugWarningActive}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </div>

      <div className={`pageLayer${page === "settings" ? " isActive" : ""}`} aria-hidden={page !== "settings"}>
        <SettingsPage
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
  selectedMapId: string;
  mapViews: MapViewOptionJson[];
  selectedMap: MapViewOptionJson;
  selectedFamilyMapViews: MapViewOptionJson[];
  selectedFamily: (typeof chartFamilies)[number];
  availableFamilies: Set<string>;
  viewport: MapViewportState;
  onViewportChange: (next: MapViewportState) => void;
  onSelectMapId: (mapId: string) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  legSummary: string;
  locationSearch: string;
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
  adapterBackend: AdapterBackendKind;
  adapterDetail: string;
  debugWarningActive: boolean;
  onDebugWarning: (tag: string, data?: unknown) => void;
  onHighLatencyWarning: (tag: string, data?: unknown) => void;
  onFirstVisualReady: () => void;
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
    ownship,
    plan,
    planUiState,
    uiSession,
    onPlaybackSnapshotChange,
    mapFollowUiState,
    mapFollowTargetViewport,
    adapterBackend,
    adapterDetail,
    debugWarningActive,
    onDebugWarning,
    onHighLatencyWarning,
    onFirstVisualReady,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const trayGroup = useModalTrayGroup(["page", "family"] as const);
  const [debugOpen, setDebugOpen] = useState(false);
  const [mapOverlay, setMapOverlay] = useState<MapOverlayQueryResult>({
    needed_point_tiles: [],
    needed_airspace_ref_tiles: [],
    needed_airspace_features: [],
    needed_airspace_label_tiles: [],
    visible_features: [],
    airspace_paths: [],
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
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const firstVisualReadyRef = useRef(false);
  const lastOverlayWarningKeyRef = useRef("");
  const lastGuidanceGeometryKeyRef = useRef<string | null>(null);
  const lastGuidanceGeometrySessionRef = useRef<UiSession | null>(null);
  const lastGuidanceGeometryPlanKeyRef = useRef<string | null>(null);

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
  const mapIsVisible = page === "map";
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
    () => resolveSituationOverlay(ownship, viewport, surfaceSize.width, surfaceSize.height),
    [ownship, viewport, surfaceSize.height, surfaceSize.width],
  );
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
      from: worldToScreen(viewport, latLonToWorld(segment.from.lat, segment.from.lon), surfaceSize.width, surfaceSize.height),
      to: worldToScreen(viewport, latLonToWorld(segment.to.lat, segment.to.lon), surfaceSize.width, surfaceSize.height),
    }));
  }, [flightPlanRoute, surfaceSize.height, surfaceSize.width, viewport]);

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    setNexradStatus({ state: "loading" });

    async function loadNexrad() {
      const response = await fetch("/fast-products/nexrad/nexrad.json", { signal: controller.signal });
      if (response.status === 404) {
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
  }, []);

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
  }, [mapIsVisible, surfaceSize.height, surfaceSize.width, terrainAltitudeBucket, uiSession, viewport]);

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

    function guidanceGeometryKey(segments: FlightPlanRouteSegment[]) {
      return segments
        .map((segment) => `${segment.id}:${segment.from.lat.toFixed(7)},${segment.from.lon.toFixed(7)}>${segment.to.lat.toFixed(7)},${segment.to.lon.toFixed(7)}`)
        .join("|");
    }

    function guidancePlanKey() {
      const guidance = plan.guidance;
      return [
        plan.id,
        plan.version,
        guidance?.sequencing_mode ?? "none",
        guidance?.active_leg_index ?? "none",
        guidance?.direct_to?.target_leg_id ?? "none",
        guidance?.direct_to?.resume_leg_id ?? "none",
        (plan.resolved_legs ?? []).map((leg) => leg.id).join(","),
      ].join(":");
    }

    async function updateGuidanceGeometry(segments: FlightPlanRouteSegment[], phase: string) {
      if (!uiSession || cancelled) {
        return;
      }
      if (lastGuidanceGeometrySessionRef.current !== uiSession) {
        lastGuidanceGeometrySessionRef.current = uiSession;
        lastGuidanceGeometryKeyRef.current = null;
      }
      const planKey = guidancePlanKey();
      if (lastGuidanceGeometryPlanKeyRef.current !== planKey) {
        lastGuidanceGeometryPlanKeyRef.current = planKey;
        lastGuidanceGeometryKeyRef.current = null;
      }
      const key = guidanceGeometryKey(segments);
      if (key === lastGuidanceGeometryKeyRef.current) {
        return;
      }
      lastGuidanceGeometryKeyRef.current = key;
      const startedAt = performance.now();
      const snapshot = await uiSession.setGuidanceLegGeometry(
        segments.map((segment) => ({
          leg_id: segment.id,
          from: segment.from,
          to: segment.to,
        })),
      );
      const elapsedMs = Math.round(performance.now() - startedAt);
      debugLog("map.route.guidance_geometry.set", {
        phase,
        count: segments.length,
        elapsed_ms: elapsedMs,
      });
      if (elapsedMs > 250) {
        onHighLatencyWarning("map.route.guidance_geometry.slow", {
          phase,
          count: segments.length,
          elapsed_ms: elapsedMs,
        });
      }
      if (!cancelled) {
        onPlaybackSnapshotChange(snapshot);
      }
    }

    async function resolveFlightPlanRoute() {
      if ((plan.resolved_legs ?? []).length === 0 || (planUiState?.resolved_legs ?? []).length === 0) {
        setFlightPlanRoute([]);
        await updateGuidanceGeometry([], "empty");
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
        await updateGuidanceGeometry(segments, "resolved");
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
  }, [appCoreAdapter, onHighLatencyWarning, onPlaybackSnapshotChange, plan, planUiState, uiSession]);

  useEffect(() => {
    if (!mapIsVisible) {
      return;
    }
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      setMapOverlay({
        needed_point_tiles: [],
        needed_airspace_ref_tiles: [],
        needed_airspace_features: [],
        needed_airspace_label_tiles: [],
        visible_features: [],
        airspace_paths: [],
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
        overlay.needed_airspace_ref_tiles.length > 0 ||
        overlay.needed_airspace_features.length > 0 ||
        overlay.needed_airspace_label_tiles.length > 0
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
        ingested = true;
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
        ingested = true;
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
          ingested = true;
        }
        debugLog("map.overlay.airspace_features.done", {
          zoom: viewport.zoom,
          count: features.length,
          missing: overlay.needed_airspace_features.length - features.length,
          elapsed_ms: Math.round(performance.now() - startedAt),
        });
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
        ingested = true;
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
          needed_airspace_ref_tiles: overlay.needed_airspace_ref_tiles.length,
          needed_airspace_features: overlay.needed_airspace_features.length,
          needed_airspace_label_tiles: overlay.needed_airspace_label_tiles.length,
          visible_features: overlay.visible_features.length,
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
      for (let pass = 0; pass < 4 && overlayNeedsInputs(overlay); pass += 1) {
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
          needed_airspace_ref_tiles: overlay.needed_airspace_ref_tiles.length,
          needed_airspace_features: overlay.needed_airspace_features.length,
          needed_airspace_label_tiles: overlay.needed_airspace_label_tiles.length,
          visible_features: overlay.visible_features.length,
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
  }, [mapIsVisible, onDebugWarning, surfaceSize.height, surfaceSize.width, uiSession, viewport]);

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

  function reportFirstVisualReady() {
    if (firstVisualReadyRef.current) {
      return;
    }
    firstVisualReadyRef.current = true;
    onFirstVisualReady();
  }

  const visibleTerrainImages = terrainOverlay.query
    ? terrainOverlay.images
      .filter((image) => terrainOverlay.query?.tile_requests.some((request) => request.key === image.key))
      .map((image) => terrainImageForViewport(image, viewport, surfaceSize.width, surfaceSize.height))
    : [];

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
              // Fractional overzoomed tile sizes can expose subpixel seams between rasters.
              width: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
              height: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
            }}
          >
            <img className="mapTileImage" src={tile.src} alt="" draggable={false} onLoad={reportFirstVisualReady} />
            {debugTileLabels ? (
              <div className="tileLabel">
                z{tile.zoom} x{tile.x} y{tile.yTms}
              </div>
            ) : null}
          </div>
        ))}
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
        {mapIsVisible && (mapOverlay.airspace_paths.length > 0 || mapOverlay.airspace_labels.length > 0) ? (
          <svg
            className="airspaceOverlay"
            viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
            preserveAspectRatio="none"
            style={overlayTransform ? { transform: overlayTransform, transformOrigin: "center center" } : undefined}
          >
            {mapOverlay.airspace_paths.map((feature) => (
              <g key={feature.id}>
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
                    props.mapViews,
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

        <NavElementButton
          navElement={planUiState?.guidance?.nav_element}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        />

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
            warn={debugWarningActive || mapOverlay.warnings.length > 0}
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
            <div className="debugLine">nexrad {nexradStatus.state === "available" ? `${nexradFrameIndex + 1}/${nexradFrames.length}` : nexradStatus.state}</div>
            {nexradStatus.state === "unavailable" ? <div className="debugLine">nexrad reason {nexradStatus.reason}</div> : null}
            <div className="debugLine">terrain {terrainOverlay.query ? terrainOverlay.query.status.state : "idle"} img={terrainOverlay.images.length}</div>
            <div className="debugLine">vec pts={mapOverlay.visible_features.length} need={mapOverlay.needed_point_tiles.length} warn={mapOverlay.warnings.length}</div>
            <div className="debugLine">airspace paths={mapOverlay.airspace_paths.length} labels={mapOverlay.airspace_labels.length} need={mapOverlay.needed_airspace_ref_tiles.length + mapOverlay.needed_airspace_features.length + mapOverlay.needed_airspace_label_tiles.length}</div>
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
  onOpenPlan: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenCharts: (airportId: string | null, chartId?: string | null) => void;
  onMoveComponent: (componentIndex: number, delta: number) => void | Promise<void>;
  onInsertAirportWaypoint: (componentIndex: number, before: boolean, airportId: string) => void | Promise<void>;
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
        ete: row.row_kind === "group" ? "" : row.leg_index !== null ? "0:04" : "—",
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
  }, [displayRows, guidance?.active_leg]);

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
          {structuredArrow ? (
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
                  {row.ete}
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
        <NavElementButton
          navElement={planUiState.guidance?.nav_element}
          className="navElement navElementStatic"
          onClick={props.onOpenPlan}
        />
      </div>

      <div className="debugDock">
        <DebugDock open={debugOpen} warn={props.debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
          <div className="debugLine">page {pageLabel(props.page)}</div>
          <div className="debugLine">up {props.uptimeLabel}</div>
	          <div className="debugLine">stack {formatPageStack(props.pageHistory, { page: props.page, selectedMapId: "", selectedChartId: "", selectedChartLabel: "", chartFolderOpen: false })}</div>
	          <div className="debugLine">components {componentViews.length}</div>
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
  const pinchRef = useRef<{ zoom: number; distance: number; midpoint: ScreenPoint } | null>(null);
  const lastChartLayoutKeyRef = useRef("");
  const firstVisualReadyRef = useRef(false);
  const trayGroup = useModalTrayGroup(["page", "airport", "chart", "load"] as const);
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

        <NavElementButton
          navElement={planUiState?.guidance?.nav_element}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        />

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
          <DebugDock open={debugOpen} warn={props.debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
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
  planUiState: FlightPlanUiState | null;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  debugWarningActive: boolean;
}) {
  const { page, pageHistory, uptimeLabel, planUiState, onSelectPage, onOpenPlan, debugWarningActive } = props;
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

      <NavElementButton
        navElement={planUiState?.guidance?.nav_element}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={onOpenPlan}
      />

      <div className="debugDock">
        <DebugDock open={debugOpen} warn={debugWarningActive} onToggle={() => setDebugOpen((open) => !open)}>
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

function flightPlanActionLabel(actionId: string): string {
  switch (actionId) {
    case "activate_leg":
      return "Activate Leg";
    case "remove":
      return "Remove";
    case "insert_before":
      return "Insert Before";
    case "insert_after":
      return "Insert After";
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
  ownship: OwnshipRenderState,
  viewport: MapViewportState,
  width: number,
  height: number,
) {
  if (width <= 0 || height <= 0 || !ownship.draw_aircraft || !ownship.position) {
    return null;
  }
  const point = latLonToScreen(ownship.position.lat, ownship.position.lon, viewport, width, height);
  const headingDeg = ownship.orientation_deg ?? 0;
  const ring = selectSituationRing(ownship.position.lat, ownship.position.lon, viewport, width, height);
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

async function buildSeededDevPlan(): Promise<{ plan: FlightPlan }> {
  const waypoints: Array<{ Airport: string } | { Navaid: string } | { Fix: string }> = [
    { Airport: "KPAO" },
    { Fix: "VPDUB" },
    { Airport: "KVCB" },
    { Airport: "KWLW" },
    { Airport: "WN08" },
    { Airport: "4WA9" },
    { Airport: "W36" },
    { Airport: "2S1" },
    { Airport: "WT22" },
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
    id: "dev-kpao-vpdub-kvcb-kwlw-wn08-4wa9-w36-2s1-wt22",
    name: "KPAO VPDUB KVCB KWLW WN08 4WA9 W36 2S1 WT22",
    legs: resolvedLegs.map((leg) => ({ from: leg.from, to: leg.to, airway: null })),
    route_components: routeComponents,
    resolved_legs: resolvedLegs,
    guidance: { active_leg_index: 0, sequencing_mode: "follow_plan" as const, direct_to: null },
    departure: "KPAO",
    destination: "WT22",
    updated_at_epoch_ms: Date.now(),
    version: samplePlan.version + 1,
  };
  return {
    plan,
  };
}

function airportIdsNeededForInitialChartPage(
  plan: FlightPlan,
  recentAirportIds: string[],
  selectedAirportId: string | null | undefined,
) {
  const airportIds = new Set<string>();
  const add = (airportId: string | null | undefined) => {
    const normalized = airportId?.trim().toUpperCase();
    if (normalized) {
      airportIds.add(normalized);
    }
  };
  add(plan.departure);
  add(plan.destination);
  add(plan.alternate);
  for (const component of plan.route_components) {
    switch (component.kind) {
      case "waypoint":
        if ("Airport" in component.waypoint) {
          add(component.waypoint.Airport);
        }
        break;
      case "airway":
        if ("Airport" in component.airway.entry) {
          add(component.airway.entry.Airport);
        }
        if ("Airport" in component.airway.exit) {
          add(component.airway.exit.Airport);
        }
        break;
      case "procedure":
        add(component.procedure.airport_id);
        break;
    }
  }
  for (const airportId of recentAirportIds) {
    add(airportId);
  }
  add(selectedAirportId);
  return airportIds;
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
