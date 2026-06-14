import type { MapViewJson } from "./types";

type ViewportMap = Pick<MapViewJson, "min_zoom" | "max_zoom" | "initial_viewport">;

export type MapViewportState = {
  centerWorldX: number;
  centerWorldY: number;
  zoom: number;
};

export type ScreenPoint = {
  x: number;
  y: number;
};

export type MapDisplayFrame = {
  viewport: MapViewportState;
  width: number;
  height: number;
};

const VIEWPORT_EPSILON = 1e-9;
const WORLD_SIZE = 256;
const MAX_LATITUDE = 85.05112878;

export function createInitialViewport(mapView: ViewportMap): MapViewportState {
  const center = latLonToWorld(mapView.initial_viewport.lat, mapView.initial_viewport.lon);
  return {
    centerWorldX: center.x,
    centerWorldY: center.y,
    zoom: clampZoom(mapView.initial_viewport.zoom, mapView),
  };
}

export function preserveViewportForMap(
  viewport: MapViewportState,
  _mapView: ViewportMap,
): MapViewportState {
  return {
    centerWorldX: viewport.centerWorldX,
    centerWorldY: viewport.centerWorldY,
    zoom: viewport.zoom,
  };
}

export function clampZoom(zoom: number, mapView: ViewportMap): number {
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

export function sameMapViewport(left: MapViewportState, right: MapViewportState): boolean {
  return (
    Math.abs(left.centerWorldX - right.centerWorldX) < VIEWPORT_EPSILON &&
    Math.abs(left.centerWorldY - right.centerWorldY) < VIEWPORT_EPSILON &&
    Math.abs(left.zoom - right.zoom) < VIEWPORT_EPSILON
  );
}

export function isStaleMapFollowTargetViewport(
  targetViewport: MapViewportState,
  awaitedTargetViewport: MapViewportState | null,
): boolean {
  return awaitedTargetViewport !== null && !sameMapViewport(targetViewport, awaitedTargetViewport);
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

function displayFrameTransformParts(from: MapDisplayFrame, to: MapDisplayFrame): {
  scale: number;
  translateX: number;
  translateY: number;
} {
  const fromScale = scaleForZoom(from.viewport.zoom);
  const toScale = scaleForZoom(to.viewport.zoom);
  const scale = toScale / fromScale;
  return {
    scale,
    translateX: to.width / 2 - (from.width / 2) * scale + (from.viewport.centerWorldX - to.viewport.centerWorldX) * toScale,
    translateY: to.height / 2 - (from.height / 2) * scale + (from.viewport.centerWorldY - to.viewport.centerWorldY) * toScale,
  };
}

export function transformScreenPointBetweenFrames(
  from: MapDisplayFrame,
  to: MapDisplayFrame,
  point: ScreenPoint,
): ScreenPoint {
  const transform = displayFrameTransformParts(from, to);
  return {
    x: point.x * transform.scale + transform.translateX,
    y: point.y * transform.scale + transform.translateY,
  };
}

export function displayFrameCssTransform(from: MapDisplayFrame, to: MapDisplayFrame): string | undefined {
  if (
    sameMapViewport(from.viewport, to.viewport)
    && Math.abs(from.width - to.width) < VIEWPORT_EPSILON
    && Math.abs(from.height - to.height) < VIEWPORT_EPSILON
  ) {
    return undefined;
  }
  const transform = displayFrameTransformParts(from, to);
  return `matrix(${transform.scale}, 0, 0, ${transform.scale}, ${transform.translateX}, ${transform.translateY})`;
}

export function zoomAroundPoint(
  viewport: MapViewportState,
  mapView: ViewportMap,
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
  mapView: ViewportMap,
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
