// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { FlightPlanRouteSegment, PlateGeoref } from "./types";
import { projectPlateFlightPlanSegments } from "./plateOverlay";

const georef: PlateGeoref = {
  kind: "plate_transform_v1",
  pixels_per_longitude: 100,
  pixels_per_latitude: -100,
  top_left_lon: -123,
  top_left_lat: 48,
};

function segment(overrides: Partial<FlightPlanRouteSegment> = {}): FlightPlanRouteSegment {
  return {
    id: "route-1",
    leg_id: "leg-1",
    from: { lat: 47.5, lon: -122.5 },
    to: { lat: 47, lon: -122 },
    path: [
      { lat: 47.5, lon: -122.5 },
      { lat: 47, lon: -122 },
    ],
    style: "solid",
    distance_nm: 10,
    course_deg: 90,
    status: "active",
    ...overrides,
  };
}

const display = {
  georef,
  imageSize: { width: 200, height: 200 },
  viewport: { left: 10, top: 20, zoom: 1 },
  displaySize: { width: 400, height: 400 },
  surfaceSize: { width: 500, height: 500 },
};

describe("plate flight-plan projection", () => {
  it("projects cooked route geometry through the plate pan and scale", () => {
    expect(projectPlateFlightPlanSegments({ ...display, segments: [segment()] })).toEqual([
      {
        id: "route-1",
        status: "active",
        path: [
          { x: 110, y: 120 },
          { x: 210, y: 220 },
        ],
      },
    ]);
  });

  it("falls back to segment endpoints when the cooked path is empty", () => {
    const projected = projectPlateFlightPlanSegments({
      ...display,
      segments: [segment({ path: [] })],
    });
    expect(projected[0]?.path).toEqual([
      { x: 110, y: 120 },
      { x: 210, y: 220 },
    ]);
  });

  it("omits route geometry wholly outside the visible plate surface", () => {
    const projected = projectPlateFlightPlanSegments({
      ...display,
      segments: [segment({
        from: { lat: 40, lon: -100 },
        to: { lat: 39, lon: -99 },
        path: [{ lat: 40, lon: -100 }, { lat: 39, lon: -99 }],
      })],
    });
    expect(projected).toEqual([]);
  });
});
