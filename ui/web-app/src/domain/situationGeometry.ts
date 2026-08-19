// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { OwnshipRenderState, SituationRingCandidate } from "./types";
import { latLonToWorld, scaleForZoom, type MapViewportState } from "./mapViewport";

// Pointer-rate mirror of app_core::ui_geometry; shared conformance vectors prevent platform drift.

type ScreenPoint = { x: number; y: number };

export function resolveSituationOverlay(
  ownship: OwnshipRenderState,
  viewport: MapViewportState,
  width: number,
  height: number,
  ringCandidates: SituationRingCandidate[],
) {
  if (width <= 0 || height <= 0 || !ownship.draw_aircraft || !ownship.position) {
    return null;
  }
  const point = situationLatLonToScreen(ownship.position.lat, ownship.position.lon, viewport, width, height);
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
      ? projectSituationAhead(ownship.position.lat, ownship.position.lon, headingDeg, ownship.speed_kt / 60)
      : null;
  const predictor = ahead
    ? situationLatLonToScreen(ahead.lat, ahead.lon, viewport, width, height)
    : null;
  return { point, predictor, headingDeg, ring };
}

export function situationLatLonToScreen(
  lat: number,
  lon: number,
  viewport: MapViewportState,
  width: number,
  height: number,
): ScreenPoint {
  const world = latLonToWorld(lat, lon);
  const scale = scaleForZoom(viewport.zoom);
  const wrappedX = world.x + Math.round((viewport.centerWorldX - world.x) / 256) * 256;
  return {
    x: ((wrappedX - viewport.centerWorldX) * scale) + width / 2,
    y: ((world.y - viewport.centerWorldY) * scale) + height / 2,
  };
}

export function projectSituationAhead(
  lat: number,
  lon: number,
  bearingDeg: number,
  distanceNm: number,
) {
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

export function selectSituationRing(
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
  const center = situationLatLonToScreen(lat, lon, viewport, width, height);
  const smaller = Math.min(width, height);
  const minDiameter = smaller * 0.5;
  const maxDiameter = smaller * 0.8;
  const targetDiameter = smaller * 0.65;
  const candidates = ringCandidates.map((candidate) => {
    const edge = projectSituationAhead(lat, lon, 90, candidate.radius_nm);
    const edgePoint = situationLatLonToScreen(edge.lat, edge.lon, viewport, width, height);
    const radiusPx = Math.hypot(edgePoint.x - center.x, edgePoint.y - center.y);
    const diameterPx = radiusPx * 2;
    const outOfBounds =
      diameterPx < minDiameter ? minDiameter - diameterPx : diameterPx > maxDiameter ? diameterPx - maxDiameter : 0;
    const score = outOfBounds > 0 ? 10000 + outOfBounds : Math.abs(diameterPx - targetDiameter);
    return { ...candidate, radiusPx, score };
  });
  const best = candidates.reduce((currentBest, candidate) =>
    candidate.score < currentBest.score ? candidate : currentBest);
  return {
    radiusPx: best.radiusPx,
    tickMarks: magneticVariationDeg === null ? [] : buildRingTickMarks(center, best.radiusPx, magneticVariationDeg),
    cardinalLabels: magneticVariationDeg === null
      ? []
      : buildRingCardinalLabels(center, best.radiusPx, magneticVariationDeg),
    label: {
      point: pointOnCircle(center, best.radiusPx + 16, -45),
      rotationDeg: 45,
      text: best.label,
    },
  };
}

function buildRingCardinalLabels(center: ScreenPoint, radiusPx: number, magneticVariationDeg: number) {
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

function buildRingTickMarks(center: ScreenPoint, radiusPx: number, magneticVariationDeg: number) {
  return Array.from({ length: 12 }, (_, index) => {
    const angleDeg = index * 30 + magneticVariationDeg;
    return {
      inner: pointOnCircle(center, radiusPx - 14, angleDeg),
      outer: pointOnCircle(center, radiusPx, angleDeg),
    };
  });
}

function pointOnCircle(center: ScreenPoint, radiusPx: number, angleDeg: number) {
  const radians = (angleDeg * Math.PI) / 180;
  return {
    x: center.x + radiusPx * Math.cos(radians),
    y: center.y + radiusPx * Math.sin(radians),
  };
}
