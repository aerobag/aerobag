import type { ChartCoverageJson, GeometryJson, MapViewJson } from "./types";

type MapView = MapViewJson & { id?: string; coverage?: ChartCoverageJson | null };
type Polygon = number[][];
type PolygonSetLookup = Map<string, Polygon[]>;

export type MapViewportState = {
  centerWorldX: number;
  centerWorldY: number;
  zoom: number;
};

export type ScreenPoint = {
  x: number;
  y: number;
};

export type RenderTile = {
  x: number;
  yTms: number;
  left: number;
  top: number;
  size: number;
  zoom: number;
  zIndex: number;
  src: string;
  mapViewId: string;
  packageName: string | null;
  chartFamily: MapView["chart_family"];
};

const WORLD_SIZE = 256;
const MAX_LATITUDE = 85.05112878;

function tileSrcForMapView(mapView: MapView, zoom: number, x: number, yTms: number): string {
  const template = mapView.tile_path_template ?? `${mapView.chart_index}/{z}/{x}/{y}.webp`;
  const path = template
    .replaceAll("{z}", String(zoom))
    .replaceAll("{x}", String(x))
    .replaceAll("{y}", String(yTms))
    .replaceAll("{y_tms}", String(yTms));
  return `${mapView.tile_url_root}/${path}`;
}

function pointInRect(lon: number, lat: number, rect: TileBounds): boolean {
  return lon >= rect.west && lon <= rect.east && lat >= rect.south && lat <= rect.north;
}

function pointInPolygon(lon: number, lat: number, polygon: Polygon): boolean {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i, i += 1) {
    const [xi, yi] = polygon[i];
    const [xj, yj] = polygon[j];
    const intersects = ((yi > lat) !== (yj > lat))
      && (lon < ((xj - xi) * (lat - yi)) / ((yj - yi) || Number.EPSILON) + xi);
    if (intersects) {
      inside = !inside;
    }
  }
  return inside;
}

function orientation(ax: number, ay: number, bx: number, by: number, cx: number, cy: number): number {
  return (by - ay) * (cx - bx) - (bx - ax) * (cy - by);
}

function onSegment(ax: number, ay: number, bx: number, by: number, cx: number, cy: number): boolean {
  return Math.min(ax, cx) <= bx && bx <= Math.max(ax, cx) && Math.min(ay, cy) <= by && by <= Math.max(ay, cy);
}

function segmentsIntersect(a1: [number, number], a2: [number, number], b1: [number, number], b2: [number, number]): boolean {
  const o1 = orientation(a1[0], a1[1], a2[0], a2[1], b1[0], b1[1]);
  const o2 = orientation(a1[0], a1[1], a2[0], a2[1], b2[0], b2[1]);
  const o3 = orientation(b1[0], b1[1], b2[0], b2[1], a1[0], a1[1]);
  const o4 = orientation(b1[0], b1[1], b2[0], b2[1], a2[0], a2[1]);

  if ((o1 > 0) !== (o2 > 0) && (o3 > 0) !== (o4 > 0)) {
    return true;
  }
  if (o1 === 0 && onSegment(a1[0], a1[1], b1[0], b1[1], a2[0], a2[1])) return true;
  if (o2 === 0 && onSegment(a1[0], a1[1], b2[0], b2[1], a2[0], a2[1])) return true;
  if (o3 === 0 && onSegment(b1[0], b1[1], a1[0], a1[1], b2[0], b2[1])) return true;
  if (o4 === 0 && onSegment(b1[0], b1[1], a2[0], a2[1], b2[0], b2[1])) return true;
  return false;
}

type TileBounds = {
  south: number;
  north: number;
  west: number;
  east: number;
};

function tileBoundsFor(zoom: number, x: number, yTms: number): TileBounds {
  const levelScale = 2 ** zoom;
  const yXyz = (levelScale - 1) - yTms;
  const tileWorldSize = WORLD_SIZE / levelScale;
  const northwest = worldToLatLon(x * tileWorldSize, yXyz * tileWorldSize);
  const southeast = worldToLatLon((x + 1) * tileWorldSize, (yXyz + 1) * tileWorldSize);
  return {
    south: Math.min(northwest.lat, southeast.lat),
    north: Math.max(northwest.lat, southeast.lat),
    west: Math.min(northwest.lon, southeast.lon),
    east: Math.max(northwest.lon, southeast.lon),
  };
}

function rectCorners(rect: TileBounds): Array<[number, number]> {
  return [
    [rect.west, rect.north],
    [rect.east, rect.north],
    [rect.east, rect.south],
    [rect.west, rect.south],
  ];
}

function polygonIntersectsRect(polygon: Polygon, rect: TileBounds): boolean {
  if (polygon.some(([lon, lat]) => pointInRect(lon, lat, rect))) {
    return true;
  }
  const corners = rectCorners(rect);
  if (corners.some(([lon, lat]) => pointInPolygon(lon, lat, polygon))) {
    return true;
  }
  const rectEdges: Array<[[number, number], [number, number]]> = [
    [corners[0], corners[1]],
    [corners[1], corners[2]],
    [corners[2], corners[3]],
    [corners[3], corners[0]],
  ];
  for (let i = 0; i < polygon.length - 1; i += 1) {
    const edge: [[number, number], [number, number]] = [polygon[i] as [number, number], polygon[i + 1] as [number, number]];
    if (rectEdges.some(([from, to]) => segmentsIntersect(edge[0], edge[1], from, to))) {
      return true;
    }
  }
  return false;
}

function buildPolygonSetLookup(geometry?: GeometryJson | null): PolygonSetLookup {
  if (!geometry) {
    return new Map();
  }
  const polygonsById = new Map((geometry.polygons ?? []).map((polygon) => [polygon.id, polygon.points]));
  const polygonSets = geometry.polygon_sets ?? [];
  return new Map(polygonSets.map((polygonSet) => {
    const polygons = polygonSet.polygon_ids.map((id) => {
      const polygon = polygonsById.get(id);
      if (!polygon) {
        throw new Error(`missing polygon ${id} for polygon set ${polygonSet.id}`);
      }
      return polygon;
    });
    return [polygonSet.id, polygons];
  }));
}

function tileIntersectsCoverage(
  mapView: MapView,
  polygonSets: PolygonSetLookup,
  zoom: number,
  x: number,
  yTms: number,
): boolean {
  const coverage = mapView.coverage;
  if (!coverage) {
    return true;
  }
  const polygons = polygonSets.get(coverage.value.polygon_set_id);
  if (!polygons) {
    throw new Error(`missing polygon set ${coverage.value.polygon_set_id}`);
  }
  const tileBounds = tileBoundsFor(zoom, x, yTms);
  return polygons.some((polygon) => polygonIntersectsRect(polygon, tileBounds));
}

export function createInitialViewport(mapView: MapView): MapViewportState {
  const center = latLonToWorld(mapView.initial_viewport.lat, mapView.initial_viewport.lon);
  return {
    centerWorldX: center.x,
    centerWorldY: center.y,
    zoom: clampZoom(mapView.initial_viewport.zoom, mapView),
  };
}

export function preserveViewportForMap(
  viewport: MapViewportState,
  _mapView: MapView,
): MapViewportState {
  return {
    centerWorldX: viewport.centerWorldX,
    centerWorldY: viewport.centerWorldY,
    zoom: viewport.zoom,
  };
}

export function clampZoom(zoom: number, mapView: MapView): number {
  return Math.min(mapView.max_zoom, Math.max(mapView.min_zoom, zoom));
}

export function latLonToWorld(lat: number, lon: number): { x: number; y: number } {
  const clampedLat = Math.max(-MAX_LATITUDE, Math.min(MAX_LATITUDE, lat));
  return {
    x: ((lon + 180) / 360) * WORLD_SIZE,
    y: ((1 - Math.asinh(Math.tan((clampedLat * Math.PI) / 180)) / Math.PI) / 2) * WORLD_SIZE,
  };
}

export function worldToLatLon(worldX: number, worldY: number): { lat: number; lon: number } {
  const lon = (worldX / WORLD_SIZE) * 360 - 180;
  const n = Math.PI - (2 * Math.PI * worldY) / WORLD_SIZE;
  const lat = (180 / Math.PI) * Math.atan(Math.sinh(n));
  return { lat, lon };
}

export function scaleForZoom(zoom: number): number {
  return 2 ** zoom;
}

export function dragViewport(
  viewport: MapViewportState,
  dx: number,
  dy: number,
): MapViewportState {
  const scale = scaleForZoom(viewport.zoom);
  return {
    ...viewport,
    centerWorldX: viewport.centerWorldX - dx / scale,
    centerWorldY: viewport.centerWorldY - dy / scale,
  };
}

export function screenToWorld(
  viewport: MapViewportState,
  point: ScreenPoint,
  width: number,
  height: number,
): { x: number; y: number } {
  const scale = scaleForZoom(viewport.zoom);
  return {
    x: viewport.centerWorldX + (point.x - width / 2) / scale,
    y: viewport.centerWorldY + (point.y - height / 2) / scale,
  };
}

export function worldToScreen(
  viewport: MapViewportState,
  world: { x: number; y: number },
  width: number,
  height: number,
): ScreenPoint {
  const scale = scaleForZoom(viewport.zoom);
  return {
    x: (world.x - viewport.centerWorldX) * scale + width / 2,
    y: (world.y - viewport.centerWorldY) * scale + height / 2,
  };
}

export function zoomAroundPoint(
  viewport: MapViewportState,
  mapView: MapView,
  anchor: ScreenPoint,
  width: number,
  height: number,
  nextZoom: number,
): MapViewportState {
  const clamped = clampZoom(nextZoom, mapView);
  const anchorWorld = screenToWorld(viewport, anchor, width, height);
  const nextScale = scaleForZoom(clamped);
  return {
    zoom: clamped,
    centerWorldX: anchorWorld.x - (anchor.x - width / 2) / nextScale,
    centerWorldY: anchorWorld.y - (anchor.y - height / 2) / nextScale,
  };
}

export function createPinchSnapshot(
  viewport: MapViewportState,
  first: ScreenPoint,
  second: ScreenPoint,
  width: number,
  height: number,
): {
  viewport: MapViewportState;
  anchorOneWorld: { x: number; y: number };
  anchorTwoWorld: { x: number; y: number };
  first: ScreenPoint;
  second: ScreenPoint;
} {
  return {
    viewport,
    anchorOneWorld: screenToWorld(viewport, first, width, height),
    anchorTwoWorld: screenToWorld(viewport, second, width, height),
    first,
    second,
  };
}

export function applyPinchGesture(
  snapshot: ReturnType<typeof createPinchSnapshot>,
  currentFirst: ScreenPoint,
  currentSecond: ScreenPoint,
  mapView: MapView,
  width: number,
  height: number,
): MapViewportState {
  const startDistance = Math.hypot(snapshot.second.x - snapshot.first.x, snapshot.second.y - snapshot.first.y);
  const currentDistance = Math.hypot(currentSecond.x - currentFirst.x, currentSecond.y - currentFirst.y);
  const zoomDelta = startDistance > 0 ? Math.log2(currentDistance / startDistance) : 0;
  const nextZoom = clampZoom(snapshot.viewport.zoom + zoomDelta, mapView);
  const nextScale = scaleForZoom(nextZoom);
  const centerOne = {
    x: snapshot.anchorOneWorld.x - (currentFirst.x - width / 2) / nextScale,
    y: snapshot.anchorOneWorld.y - (currentFirst.y - height / 2) / nextScale,
  };
  const centerTwo = {
    x: snapshot.anchorTwoWorld.x - (currentSecond.x - width / 2) / nextScale,
    y: snapshot.anchorTwoWorld.y - (currentSecond.y - height / 2) / nextScale,
  };
  return {
    zoom: nextZoom,
    centerWorldX: (centerOne.x + centerTwo.x) / 2,
    centerWorldY: (centerOne.y + centerTwo.y) / 2,
  };
}

export function viewportCenterLatLon(viewport: MapViewportState): { lat: number; lon: number } {
  return worldToLatLon(viewport.centerWorldX, viewport.centerWorldY);
}

export function renderTiles(
  mapViews: Array<MapView & { id?: string }>,
  geometry: GeometryJson | null | undefined,
  viewport: MapViewportState,
  width: number,
  height: number,
): RenderTile[] {
  const polygonSets = buildPolygonSetLookup(geometry);
  const tiles: RenderTile[] = [];
  const mapViewsByFamily = new Map<MapView["chart_family"], Array<MapView & { id?: string }>>();
  for (const mapView of mapViews) {
    const group = mapViewsByFamily.get(mapView.chart_family);
    if (group) {
      group.push(mapView);
    } else {
      mapViewsByFamily.set(mapView.chart_family, [mapView]);
    }
  }
  for (const familyMapViews of mapViewsByFamily.values()) {
    tiles.push(...renderTilesForFamily(familyMapViews, polygonSets, viewport, width, height));
  }
  return dedupeTiles(tiles).sort((left, right) => {
    const zoomDelta = left.zoom - right.zoom;
    if (zoomDelta !== 0) {
      return zoomDelta;
    }
    return chartFamilyRenderPriority(left.chartFamily) - chartFamilyRenderPriority(right.chartFamily);
  });
}

function renderTilesForFamily(
  familyMapViews: Array<MapView & { id?: string }>,
  polygonSets: PolygonSetLookup,
  viewport: MapViewportState,
  width: number,
  height: number,
): RenderTile[] {
  const tiles: RenderTile[] = [];
  const familyFullCoverageZoom = familyMapViews
    .map((mapView) => mapView.full_coverage_zoom)
    .filter((zoom): zoom is number => zoom != null)
    .reduce<number | null>((best, zoom) => (best == null ? zoom : Math.min(best, zoom)), null);
  const lowZoomRepresentativeId = familyMapViews[0]?.id ?? familyMapViews[0]?.chart_name ?? null;
  const scale = scaleForZoom(viewport.zoom);
  const minWorldX = viewport.centerWorldX - width / 2 / scale;
  const maxWorldX = viewport.centerWorldX + width / 2 / scale;
  const minWorldY = viewport.centerWorldY - height / 2 / scale;
  const maxWorldY = viewport.centerWorldY + height / 2 / scale;

  for (const mapView of familyMapViews) {
    const levels = levelsForMapView(mapView, viewport.zoom);
    for (const level of levels) {
      if (
        familyFullCoverageZoom != null &&
        level.zoom <= familyFullCoverageZoom &&
        (mapView.id ?? mapView.chart_name) !== lowZoomRepresentativeId
      ) {
        continue;
      }
      const tileWorldSize = WORLD_SIZE / (2 ** level.zoom);
      const tileScreenSize = tileWorldSize * scale;
      const xStart = Math.floor(minWorldX / tileWorldSize);
      const xEnd = Math.floor(maxWorldX / tileWorldSize);
      const yStart = Math.floor(minWorldY / tileWorldSize);
      const yEnd = Math.floor(maxWorldY / tileWorldSize);
      const levelScale = 2 ** level.zoom;

      for (let yXyz = yStart; yXyz <= yEnd; yXyz += 1) {
        for (let displayX = xStart; displayX <= xEnd; displayX += 1) {
          const x = positiveModulo(displayX, levelScale);
          const yTms = (levelScale - 1) - yXyz;
          if (x < level.x_min || x > level.x_max || yTms < level.y_tms_min || yTms > level.y_tms_max) {
            continue;
          }
          if (!tileIntersectsCoverage(mapView, polygonSets, level.zoom, x, yTms)) {
            continue;
          }
          const left = ((displayX * tileWorldSize - viewport.centerWorldX) * scale) + width / 2;
          const top = ((yXyz * tileWorldSize - viewport.centerWorldY) * scale) + height / 2;
          tiles.push({
            x,
            yTms,
            left,
            top,
            size: tileScreenSize,
            zoom: level.zoom,
            zIndex: rasterTileZIndex(level.zoom, mapView.chart_family),
            src: tileSrcForMapView(mapView, level.zoom, x, yTms),
            mapViewId: mapView.id ?? mapView.chart_name,
            packageName: mapView.package_name,
            chartFamily: mapView.chart_family,
          });
        }
      }
    }
  }

  return tiles;
}

function levelsForMapView(mapView: MapView, zoom: number): MapView["levels"] {
  const desiredLevel = pickLevel(mapView, zoom);
  if (mapView.storage_kind === "static_product") {
    return [desiredLevel];
  }
  // Keep coarser levels as a fallback under chart-package desired levels.
  // Do not collapse chart packages to "desired level only": that regresses real
  // missing-tile gaps, notably IFR-L in SE Alaska, where lower-zoom tiles are
  // needed to avoid holes. Tile z-index keeps coarse fallback rasters underneath
  // more detailed levels when multiple fallback layers are present.
  return mapView.levels
    .filter((level) => level.zoom <= desiredLevel.zoom)
    .sort((left, right) => left.zoom - right.zoom);
}

function dedupeTiles(tiles: RenderTile[]): RenderTile[] {
  const byScreenKey = new Map<string, RenderTile>();
  for (const tile of tiles) {
    const key = `${tile.zoom}:${tile.x}:${tile.yTms}:${tile.left}:${tile.mapViewId}`;
    if (!byScreenKey.has(key)) {
      byScreenKey.set(key, tile);
    }
  }
  return [...byScreenKey.values()];
}

function positiveModulo(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

function chartFamilyRenderPriority(chartFamily: MapView["chart_family"]): number {
  switch (chartFamily) {
    case "shaded-relief":
      return -10;
    case "sec":
      return 0;
    case "tac":
      return 1;
    default:
      return 0;
  }
}

function rasterTileZIndex(zoom: number, chartFamily: MapView["chart_family"]): number {
  return zoom * 10 + chartFamilyRenderPriority(chartFamily);
}

function pickLevel(mapView: MapView, zoom: number): MapView["levels"][number] {
  return mapView.levels.reduce((best, current) => {
    if (Math.abs(current.zoom - zoom) < Math.abs(best.zoom - zoom)) {
      return current;
    }
    return best;
  });
}
