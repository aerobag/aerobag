// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { FlightPlanRouteSegment } from "./types";

type RouteScreenPoint = { x: number; y: number };

export type RouteChevronPlacement = RouteScreenPoint & { angleDegrees: number };

export function flightPlanRouteSegmentRenderKey(
  segment: Pick<FlightPlanRouteSegment, "id">,
  segmentIndex: number,
): string {
  return `${segmentIndex}:${segment.id}`;
}

export function spacedRouteChevronPlacements(
  path: readonly RouteScreenPoint[],
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
