import type { FlightPlanRouteSegment } from "./types";

export function flightPlanRouteSegmentRenderKey(
  segment: Pick<FlightPlanRouteSegment, "id">,
  segmentIndex: number,
): string {
  return `${segmentIndex}:${segment.id}`;
}
