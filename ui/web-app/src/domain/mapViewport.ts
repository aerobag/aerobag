// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { MapViewJson } from "./types";

// Pointer-rate mirror of app_core::ui_geometry; shared conformance vectors prevent platform drift.

type ViewportMap = Pick<MapViewJson, "min_zoom" | "max_zoom" | "initial_viewport">;

export type MapViewportState = {
  centerWorldX: number;
  centerWorldY: number;
  zoom: number;
  rotationDeg?: number;
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

export type MapOrientationMode = "north" | "track";

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
    rotationDeg: viewport.rotationDeg,
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
  mapUpDeg = viewport.rotationDeg ?? 0,
): MapViewportState {
  const scale = scaleForZoom(viewport.zoom);
  const worldAlignedDelta = rotateScreenOffset(dx, dy, mapUpDeg);
  return {
    ...viewport,
    centerWorldX: viewport.centerWorldX - worldAlignedDelta.x / scale,
    centerWorldY: viewport.centerWorldY - worldAlignedDelta.y / scale,
  };
}

export function sameMapViewport(left: MapViewportState, right: MapViewportState): boolean {
  return (
    Math.abs(left.centerWorldX - right.centerWorldX) < VIEWPORT_EPSILON &&
    Math.abs(left.centerWorldY - right.centerWorldY) < VIEWPORT_EPSILON &&
    Math.abs(left.zoom - right.zoom) < VIEWPORT_EPSILON &&
    Math.abs((left.rotationDeg ?? 0) - (right.rotationDeg ?? 0)) < VIEWPORT_EPSILON
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
  mapUpDeg = viewport.rotationDeg ?? 0,
): { x: number; y: number } {
  const scale = scaleForZoom(viewport.zoom);
  const worldAlignedOffset = rotateScreenOffset(
    point.x - width / 2,
    point.y - height / 2,
    mapUpDeg,
  );
  return {
    x: viewport.centerWorldX + worldAlignedOffset.x / scale,
    y: viewport.centerWorldY + worldAlignedOffset.y / scale,
  };
}

export function worldToScreen(
  viewport: MapViewportState,
  world: { x: number; y: number },
  width: number,
  height: number,
  mapUpDeg = viewport.rotationDeg ?? 0,
): ScreenPoint {
  const scale = scaleForZoom(viewport.zoom);
  const wrappedWorldX = world.x + Math.round((viewport.centerWorldX - world.x) / WORLD_SIZE) * WORLD_SIZE;
  const screenAlignedOffset = rotateScreenOffset(
    (wrappedWorldX - viewport.centerWorldX) * scale,
    (world.y - viewport.centerWorldY) * scale,
    -mapUpDeg,
  );
  return {
    x: screenAlignedOffset.x + width / 2,
    y: screenAlignedOffset.y + height / 2,
  };
}

export function rotatedViewportEnvelopeSize(
  width: number,
  height: number,
  mapUpDeg: number,
): { width: number; height: number } {
  const radians = (mapUpDeg * Math.PI) / 180;
  const absCos = Math.abs(Math.cos(radians));
  const absSin = Math.abs(Math.sin(radians));
  return {
    width: width * absCos + height * absSin,
    height: width * absSin + height * absCos,
  };
}

export function resolveMapUpDegrees(
  mode: MapOrientationMode,
  trackDegTrue: number | null | undefined,
  retainedTrackUpDeg = 0,
): number {
  if (mode !== "track") {
    return 0;
  }
  const mapUpDeg = typeof trackDegTrue === "number" && Number.isFinite(trackDegTrue)
    ? trackDegTrue
    : retainedTrackUpDeg;
  return Number.isFinite(mapUpDeg) ? normalizeRotationDegrees(mapUpDeg) : 0;
}

export function compassNeedleRotationDegrees(
  mapUpDeg: number,
  magneticVariationDeg: number | null | undefined,
): number {
  const magneticNorthDegTrue = typeof magneticVariationDeg === "number" && Number.isFinite(magneticVariationDeg)
    ? magneticVariationDeg
    : 0;
  return normalizeRotationDegrees(magneticNorthDegTrue - mapUpDeg);
}

function normalizeRotationDegrees(degrees: number): number {
  const normalized = ((degrees + 180) % 360 + 360) % 360 - 180;
  return Object.is(normalized, -0) ? 0 : normalized;
}

function rotateScreenOffset(x: number, y: number, degrees: number): ScreenPoint {
  const radians = (degrees * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  return {
    x: x * cos - y * sin,
    y: x * sin + y * cos,
  };
}

export function transformScreenPointBetweenFrames(
  from: MapDisplayFrame,
  to: MapDisplayFrame,
  point: ScreenPoint,
): ScreenPoint {
  return worldToScreen(
    to.viewport,
    screenToWorld(from.viewport, point, from.width, from.height),
    to.width,
    to.height,
  );
}

export function displayFrameCssTransform(from: MapDisplayFrame, to: MapDisplayFrame): string | undefined {
  if (
    sameMapViewport(from.viewport, to.viewport)
    && Math.abs(from.width - to.width) < VIEWPORT_EPSILON
    && Math.abs(from.height - to.height) < VIEWPORT_EPSILON
  ) {
    return undefined;
  }
  const origin = transformScreenPointBetweenFrames(from, to, { x: 0, y: 0 });
  const xBasis = transformScreenPointBetweenFrames(from, to, { x: 1, y: 0 });
  const yBasis = transformScreenPointBetweenFrames(from, to, { x: 0, y: 1 });
  return `matrix(${xBasis.x - origin.x}, ${xBasis.y - origin.y}, ${yBasis.x - origin.x}, ${yBasis.y - origin.y}, ${origin.x}, ${origin.y})`;
}

export function zoomAroundPoint(
  viewport: MapViewportState,
  mapView: ViewportMap,
  anchor: ScreenPoint,
  width: number,
  height: number,
  nextZoom: number,
  mapUpDeg = viewport.rotationDeg ?? 0,
): MapViewportState {
  const clamped = clampZoom(nextZoom, mapView);
  const anchorWorld = screenToWorld(viewport, anchor, width, height, mapUpDeg);
  const nextScale = scaleForZoom(clamped);
  const worldAlignedOffset = rotateScreenOffset(
    anchor.x - width / 2,
    anchor.y - height / 2,
    mapUpDeg,
  );
  return {
    zoom: clamped,
    centerWorldX: anchorWorld.x - worldAlignedOffset.x / nextScale,
    centerWorldY: anchorWorld.y - worldAlignedOffset.y / nextScale,
    rotationDeg: viewport.rotationDeg,
  };
}

export function createPinchSnapshot(
  viewport: MapViewportState,
  first: ScreenPoint,
  second: ScreenPoint,
  width: number,
  height: number,
  mapUpDeg = 0,
): {
  viewport: MapViewportState;
  anchorOneWorld: { x: number; y: number };
  anchorTwoWorld: { x: number; y: number };
  first: ScreenPoint;
  second: ScreenPoint;
  mapUpDeg: number;
} {
  return {
    viewport,
    anchorOneWorld: screenToWorld(viewport, first, width, height, mapUpDeg),
    anchorTwoWorld: screenToWorld(viewport, second, width, height, mapUpDeg),
    first,
    second,
    mapUpDeg,
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
  const currentFirstOffset = rotateScreenOffset(
    currentFirst.x - width / 2,
    currentFirst.y - height / 2,
    snapshot.mapUpDeg,
  );
  const currentSecondOffset = rotateScreenOffset(
    currentSecond.x - width / 2,
    currentSecond.y - height / 2,
    snapshot.mapUpDeg,
  );
  const centerOne = {
    x: snapshot.anchorOneWorld.x - currentFirstOffset.x / nextScale,
    y: snapshot.anchorOneWorld.y - currentFirstOffset.y / nextScale,
  };
  const centerTwo = {
    x: snapshot.anchorTwoWorld.x - currentSecondOffset.x / nextScale,
    y: snapshot.anchorTwoWorld.y - currentSecondOffset.y / nextScale,
  };
  return {
    zoom: nextZoom,
    centerWorldX: (centerOne.x + centerTwo.x) / 2,
    centerWorldY: (centerOne.y + centerTwo.y) / 2,
    rotationDeg: snapshot.viewport.rotationDeg,
  };
}

export function viewportCenterLatLon(viewport: MapViewportState): { lat: number; lon: number } {
  return worldToLatLon(viewport.centerWorldX, viewport.centerWorldY);
}
