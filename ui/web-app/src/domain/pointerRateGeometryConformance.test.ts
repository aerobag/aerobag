// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import conformance from "../generated/uiGeometryConformance.json";
import {
  layoutFlightPlanRouteDistancePills,
  spacedRouteChevronPlacements,
} from "./flightPlanRouteRender";
import {
  projectSituationAhead,
  selectSituationRing,
  situationLatLonToScreen,
} from "./situationGeometry";
import type { FlightPlanRouteDistanceAnnotation } from "./types";

function expectPoint(actual: { x: number; y: number }, expected: { x: number; y: number }) {
  expect(actual.x).toBeCloseTo(expected.x, 8);
  expect(actual.y).toBeCloseTo(expected.y, 8);
}

describe("pointer-rate geometry conformance", () => {
  it("matches core situation-ring and predictor vectors", () => {
    const vector = conformance.situation_overlay;
    const viewport = {
      centerWorldX: vector.viewport.center_world_x,
      centerWorldY: vector.viewport.center_world_y,
      zoom: vector.viewport.zoom,
      rotationDeg: vector.viewport.rotation_deg,
    };
    const center = situationLatLonToScreen(
      vector.position.lat,
      vector.position.lon,
      viewport,
      vector.width,
      vector.height,
    );
    expectPoint(center, vector.expected.center);

    const predictorPosition = projectSituationAhead(
      vector.position.lat,
      vector.position.lon,
      vector.predictor.heading_deg,
      vector.predictor.speed_kt * vector.predictor.minutes / 60,
    );
    expectPoint(
      situationLatLonToScreen(
        predictorPosition.lat,
        predictorPosition.lon,
        viewport,
        vector.width,
        vector.height,
      ),
      vector.expected.predictor,
    );

    const ring = selectSituationRing(
      vector.position.lat,
      vector.position.lon,
      viewport,
      vector.width,
      vector.height,
      vector.ring_candidates,
      vector.magnetic_variation_deg,
    );
    expect(ring).not.toBeNull();
    expect(ring!.label.text).toBe(vector.expected.ring.label);
    expect(ring!.radiusPx).toBeCloseTo(vector.expected.ring.radius, 8);
    expectPoint(ring!.label.point, vector.expected.ring.label_point);
    expect(ring!.label.rotationDeg).toBe(vector.expected.ring.label_rotation_degrees);
    expect(ring!.tickMarks).toHaveLength(vector.expected.ring.ticks.length);
    ring!.tickMarks.forEach((tick, index) => {
      expectPoint(tick.inner, vector.expected.ring.ticks[index].inner);
      expectPoint(tick.outer, vector.expected.ring.ticks[index].outer);
    });
    expect(ring!.cardinalLabels).toHaveLength(vector.expected.ring.cardinals.length);
    ring!.cardinalLabels.forEach((cardinal, index) => {
      const expected = vector.expected.ring.cardinals[index];
      expect(cardinal.text).toBe(expected.text);
      expectPoint(cardinal.point, expected.point);
      expect(cardinal.rotationDeg).toBe(expected.rotation_degrees);
    });
  });

  it("matches core route-chevron and distance-pill vectors", () => {
    const chevrons = conformance.route_chevrons;
    const placements = spacedRouteChevronPlacements(chevrons.path, chevrons.spacing);
    expect(placements).toHaveLength(chevrons.expected.length);
    placements.forEach((placement, index) => {
      const expected = chevrons.expected[index];
      expectPoint(placement, expected);
      expect(placement.angleDegrees).toBeCloseTo(expected.angle_degrees, 8);
    });

    const pill = conformance.route_distance_pill;
    const annotation: FlightPlanRouteDistanceAnnotation = {
      id: "conformance-pill",
      segment_indexes: pill.segment_indexes,
      text: pill.text,
      distance_nm: 20,
      status: "active",
      required_feature_ids: pill.required_feature_ids,
      minimum_path_to_pill_width_ratio: pill.minimum_path_to_pill_width_ratio,
    };
    const [layout] = layoutFlightPlanRouteDistancePills(
      [annotation],
      pill.screen_paths.map((path) => ({ status: "active" as const, path })),
      new Set(pill.visible_feature_ids),
      () => pill.measured_width,
      pill.map_up_deg,
    );
    expect(layout).toBeDefined();
    expectPoint(layout.center, pill.expected.center);
    expect(layout.width).toBeCloseTo(pill.expected.width, 8);
    expect(layout.rotationDegrees).toBeCloseTo(pill.expected.rotation_degrees, 8);
  });
});
