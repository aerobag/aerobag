// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { FlightPlanRouteDistanceAnnotation, FlightPlanRouteSegment } from "./types";

// Pointer-rate mirror of app_core::ui_geometry; shared conformance vectors prevent platform drift.

type ScreenPoint = { x: number; y: number };

export type FlightPlanRouteScreenSegment = Pick<FlightPlanRouteSegment, "status"> & {
  path: ScreenPoint[];
};

export type FlightPlanRouteDistancePillLayout = {
  annotation: FlightPlanRouteDistanceAnnotation;
  center: ScreenPoint;
  width: number;
  rotationDegrees: number;
};

export const FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX = 20;
export const FLIGHT_PLAN_ROUTE_DISTANCE_PILL_FONT_PX = 12;
const FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HORIZONTAL_PADDING_PX = 12;
const FLIGHT_PLAN_ROUTE_DISTANCE_PILL_MIN_WIDTH_PX = 26;

let distancePillMeasurementContext: CanvasRenderingContext2D | null = null;

export type RouteChevronPlacement = ScreenPoint & { angleDegrees: number };

export function flightPlanRouteSegmentRenderKey(
  segment: Pick<FlightPlanRouteSegment, "id">,
  segmentIndex: number,
): string {
  return `${segmentIndex}:${segment.id}`;
}

export function spacedRouteChevronPlacements(
  path: readonly ScreenPoint[],
  spacingPx: number,
): RouteChevronPlacement[] {
  if (path.length < 2 || !Number.isFinite(spacingPx) || spacingPx <= 0) return [];

  const sections = path.slice(1).flatMap((end, index) => {
    const start = path[index];
    const length = Math.hypot(end.x - start.x, end.y - start.y);
    return length > 0 ? [{ start, end, length }] : [];
  });
  const totalLength = sections.reduce((total, section) => total + section.length, 0);
  if (totalLength <= 0) return [];

  const distances: number[] = [];
  if (totalLength <= spacingPx) {
    distances.push(totalLength / 2);
  } else {
    for (
      let distance = spacingPx / 2;
      distance <= totalLength - spacingPx / 2 + 1e-6;
      distance += spacingPx
    ) {
      distances.push(distance);
    }
  }

  let sectionIndex = 0;
  let sectionStartDistance = 0;
  return distances.map((distance) => {
    while (
      sectionIndex + 1 < sections.length &&
      distance > sectionStartDistance + sections[sectionIndex].length
    ) {
      sectionStartDistance += sections[sectionIndex].length;
      sectionIndex += 1;
    }
    const section = sections[sectionIndex];
    const fraction = (distance - sectionStartDistance) / section.length;
    return {
      x: section.start.x + (section.end.x - section.start.x) * fraction,
      y: section.start.y + (section.end.y - section.start.y) * fraction,
      angleDegrees:
        (Math.atan2(section.end.y - section.start.y, section.end.x - section.start.x) * 180) /
        Math.PI,
    };
  });
}

export function measureFlightPlanRouteDistancePillWidth(text: string): number {
  if (!distancePillMeasurementContext) {
    distancePillMeasurementContext = document.createElement("canvas").getContext("2d");
  }
  const context = distancePillMeasurementContext;
  if (!context) {
    throw new Error("2D canvas context is required to measure route distance labels");
  }
  const fontFamily = getComputedStyle(document.documentElement).fontFamily;
  context.font = `800 ${FLIGHT_PLAN_ROUTE_DISTANCE_PILL_FONT_PX}px ${fontFamily}`;
  return Math.max(
    FLIGHT_PLAN_ROUTE_DISTANCE_PILL_MIN_WIDTH_PX,
    context.measureText(text).width + FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HORIZONTAL_PADDING_PX,
  );
}

export function layoutFlightPlanRouteDistancePills(
  annotations: FlightPlanRouteDistanceAnnotation[],
  screenSegments: FlightPlanRouteScreenSegment[],
  visibleFeatureIds: ReadonlySet<string>,
  measurePillWidth: (text: string) => number,
  mapUpDeg = 0,
): FlightPlanRouteDistancePillLayout[] {
  const layouts: FlightPlanRouteDistancePillLayout[] = [];
  for (const annotation of annotations) {
    if (annotation.required_feature_ids.some((featureId) => !visibleFeatureIds.has(featureId))) {
      continue;
    }
    const path = annotation.segment_indexes.flatMap((segmentIndex, index) => {
      const points = screenSegments[segmentIndex]?.path ?? [];
      return index === 0 ? points : points.slice(1);
    });
    if (path.length < 2) {
      continue;
    }
    const segmentLengths = path.slice(1).map((point, index) =>
      Math.hypot(point.x - path[index].x, point.y - path[index].y),
    );
    const pathLength = segmentLengths.reduce((sum, length) => sum + length, 0);
    const width = measurePillWidth(annotation.text);
    if (pathLength < width * annotation.minimum_path_to_pill_width_ratio) {
      continue;
    }
    const anchorDistance = width * annotation.minimum_path_to_pill_width_ratio / 2;
    let traversed = 0;
    let center = path[0];
    let rotationDegrees = 0;
    for (let index = 0; index < segmentLengths.length; index += 1) {
      const length = segmentLengths[index];
      if (length > 0 && traversed + length >= anchorDistance) {
        const fraction = (anchorDistance - traversed) / length;
        center = {
          x: path[index].x + (path[index + 1].x - path[index].x) * fraction,
          y: path[index].y + (path[index + 1].y - path[index].y) * fraction,
        };
        const deltaX = path[index + 1].x - path[index].x;
        const deltaY = path[index + 1].y - path[index].y;
        rotationDegrees = uprightLocalRotationDegrees(deltaX, deltaY, mapUpDeg);
        break;
      }
      traversed += length;
    }
    layouts.push({ annotation, center, width, rotationDegrees });
  }
  return layouts;
}

function uprightLocalRotationDegrees(deltaX: number, deltaY: number, mapUpDeg: number): number {
  const routeRotationDeg = Math.atan2(deltaY, deltaX) * 180 / Math.PI;
  const displayedRotationDeg = normalizeSignedDegrees(routeRotationDeg - mapUpDeg);
  return displayedRotationDeg <= -90 || displayedRotationDeg > 90
    ? normalizeSignedDegrees(routeRotationDeg + 180)
    : routeRotationDeg;
}

function normalizeSignedDegrees(degrees: number): number {
  return ((degrees + 180) % 360 + 360) % 360 - 180;
}
