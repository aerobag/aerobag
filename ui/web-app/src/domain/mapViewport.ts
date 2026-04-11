import type { ContentFixtureBundle } from "./types";

type MapView = ContentFixtureBundle["map_view"];

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
  src: string;
  mapViewId: string;
  packageName: string | null;
  chartFamily: MapView["chart_family"];
};

const WORLD_SIZE = 256;
const MAX_LATITUDE = 85.05112878;

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
  return Math.min(mapView.max_zoom + 1, Math.max(mapView.min_zoom, zoom));
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
  viewport: MapViewportState,
  width: number,
  height: number,
): RenderTile[] {
  const tiles: RenderTile[] = [];
  for (const mapView of mapViews) {
    tiles.push(...renderTilesForMapView(mapView, viewport, width, height));
  }
  return dedupeTiles(tiles).sort((left, right) => left.zoom - right.zoom);
}

function renderTilesForMapView(
  mapView: MapView & { id?: string },
  viewport: MapViewportState,
  width: number,
  height: number,
): RenderTile[] {
  const level = pickLevel(mapView, viewport.zoom);
  const scale = scaleForZoom(viewport.zoom);
  const tileWorldSize = WORLD_SIZE / (2 ** level.zoom);
  const tileScreenSize = tileWorldSize * scale;
  const minWorldX = viewport.centerWorldX - width / 2 / scale;
  const maxWorldX = viewport.centerWorldX + width / 2 / scale;
  const minWorldY = viewport.centerWorldY - height / 2 / scale;
  const maxWorldY = viewport.centerWorldY + height / 2 / scale;
  const xStart = Math.floor(minWorldX / tileWorldSize);
  const xEnd = Math.floor(maxWorldX / tileWorldSize);
  const yStart = Math.floor(minWorldY / tileWorldSize);
  const yEnd = Math.floor(maxWorldY / tileWorldSize);
  const levelScale = 2 ** level.zoom;
  const tiles: RenderTile[] = [];

  for (let yXyz = yStart; yXyz <= yEnd; yXyz += 1) {
    for (let x = xStart; x <= xEnd; x += 1) {
      const yTms = (levelScale - 1) - yXyz;
      if (x < level.x_min || x > level.x_max || yTms < level.y_tms_min || yTms > level.y_tms_max) {
        continue;
      }
      const left = ((x * tileWorldSize - viewport.centerWorldX) * scale) + width / 2;
      const top = ((yXyz * tileWorldSize - viewport.centerWorldY) * scale) + height / 2;
      tiles.push({
        x,
        yTms,
        left,
        top,
        size: tileScreenSize,
        zoom: level.zoom,
        src: `${mapView.tile_url_root}/${mapView.chart_index}/${level.zoom}/${x}/${yTms}.webp`,
        mapViewId: mapView.id ?? mapView.chart_name,
        packageName: mapView.package_name,
        chartFamily: mapView.chart_family,
      });
    }
  }

  return tiles;
}

function dedupeTiles(tiles: RenderTile[]): RenderTile[] {
  const byScreenKey = new Map<string, RenderTile>();
  for (const tile of tiles) {
    const key = `${tile.zoom}:${tile.x}:${tile.yTms}`;
    const existing = byScreenKey.get(key);
    if (!existing) {
      byScreenKey.set(key, tile);
      continue;
    }
    if (existing.chartFamily !== "tac" && tile.chartFamily === "tac") {
      byScreenKey.set(key, tile);
    }
  }
  return [...byScreenKey.values()];
}

function pickLevel(mapView: MapView, zoom: number): MapView["levels"][number] {
  return mapView.levels.reduce((best, current) => {
    if (Math.abs(current.zoom - zoom) < Math.abs(best.zoom - zoom)) {
      return current;
    }
    return best;
  });
}
