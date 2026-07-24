// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { FlightPlanRouteSegment } from "./types";

export function flightPlanRouteSegmentRenderKey(
  segment: Pick<FlightPlanRouteSegment, "id">,
  segmentIndex: number,
): string {
  return `${segmentIndex}:${segment.id}`;
}
