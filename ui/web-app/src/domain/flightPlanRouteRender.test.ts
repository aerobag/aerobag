// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import {
  flightPlanRouteSegmentRenderKey,
  layoutFlightPlanRouteDistancePills,
  spacedRouteChevronPlacements,
} from "./flightPlanRouteRender";
import type { FlightPlanRouteDistanceAnnotation } from "./types";

describe("flight plan route rendering", () => {
  it("does not use canonical guidance ids as globally unique React keys", () => {
    const first = flightPlanRouteSegmentRenderKey({ id: "airway--0#0" }, 0);
    const second = flightPlanRouteSegmentRenderKey({ id: "airway--0#0" }, 12);

    expect(first).toBe("0:airway--0#0");
    expect(second).toBe("12:airway--0#0");
    expect(first).not.toBe(second);
  });

  it("places directional chevrons at regular distances along the displayed path", () => {
    const placements = spacedRouteChevronPlacements(
      [
        { x: 0, y: 0 },
        { x: 30, y: 0 },
        { x: 30, y: 40 },
      ],
      16,
    );

    expect(placements).toHaveLength(4);
    expect(placements[0]).toEqual({ x: 8, y: 0, angleDegrees: 0 });
    expect(placements[1]).toEqual({ x: 24, y: 0, angleDegrees: 0 });
    expect(placements[2]).toEqual({ x: 30, y: 10, angleDegrees: 90 });
    expect(placements[3]).toEqual({ x: 30, y: 26, angleDegrees: 90 });
  });

  it("still presents one directional chevron on a short manual heading", () => {
    expect(spacedRouteChevronPlacements([{ x: 2, y: 4 }, { x: 10, y: 4 }], 24)).toEqual([
      { x: 6, y: 4, angleDegrees: 0 },
    ]);
  });

  it("shows a pill only when all endpoint labels are visible and the path is long enough", () => {
    const annotation: FlightPlanRouteDistanceAnnotation = {
      id: "leg-1",
      segment_indexes: [0],
      text: "53nm",
      distance_nm: 53,
      status: "remaining",
      required_feature_ids: ["flight-plan:start", "flight-plan:end"],
      minimum_path_to_pill_width_ratio: 3.2,
    };
    const segment = { status: "remaining" as const, path: [{ x: 0, y: 0 }, { x: 320, y: 0 }] };

    expect(layoutFlightPlanRouteDistancePills(
      [annotation],
      [segment],
      new Set(["flight-plan:start"]),
      () => 100,
    )).toEqual([]);
    expect(layoutFlightPlanRouteDistancePills(
      [annotation],
      [segment],
      new Set(annotation.required_feature_ids),
      () => 101,
    )).toEqual([]);
    expect(layoutFlightPlanRouteDistancePills(
      [annotation],
      [segment],
      new Set(annotation.required_feature_ids),
      () => 100,
    )).toEqual([{ annotation, center: { x: 160, y: 0 }, width: 100, rotationDegrees: 0 }]);
  });

  it("places and rotates one logical-leg pill along its aggregated drawable path", () => {
    const annotation: FlightPlanRouteDistanceAnnotation = {
      id: "procedure-leg",
      segment_indexes: [0, 1],
      text: "20nm",
      distance_nm: 20,
      status: "active",
      required_feature_ids: [],
      minimum_path_to_pill_width_ratio: 3.2,
    };
    const layouts = layoutFlightPlanRouteDistancePills(
      [annotation],
      [
        { status: "active", path: [{ x: 0, y: 0 }, { x: 20, y: 0 }] },
        { status: "active_leg_remaining", path: [{ x: 20, y: 0 }, { x: 20, y: 180 }] },
      ],
      new Set(),
      () => 50,
    );

    expect(layouts).toEqual([{
      annotation,
      center: { x: 20, y: 60 },
      width: 50,
      rotationDegrees: 90,
    }]);
  });

  it("reverses an upward baseline so the text reads in the downish direction", () => {
    const annotation: FlightPlanRouteDistanceAnnotation = {
      id: "northbound-leg",
      segment_indexes: [0],
      text: "53nm",
      distance_nm: 53,
      status: "remaining",
      required_feature_ids: [],
      minimum_path_to_pill_width_ratio: 3.2,
    };
    const [layout] = layoutFlightPlanRouteDistancePills(
      [annotation],
      [{ status: "remaining", path: [{ x: 0, y: 100 }, { x: 0, y: -100 }] }],
      new Set(),
      () => 50,
    );

    expect(layout.center).toEqual({ x: 0, y: 20 });
    expect(layout.rotationDegrees).toBe(90);
  });

  it("chooses the upright equivalent after applying map rotation", () => {
    const annotation: FlightPlanRouteDistanceAnnotation = {
      id: "northeast-leg",
      segment_indexes: [0],
      text: "730nm",
      distance_nm: 730,
      status: "remaining",
      required_feature_ids: [],
      minimum_path_to_pill_width_ratio: 3.2,
    };
    const segment = {
      status: "remaining" as const,
      path: [{ x: 0, y: 20 }, { x: 200, y: 0 }],
    };
    const [northUp] = layoutFlightPlanRouteDistancePills(
      [annotation],
      [segment],
      new Set(),
      () => 50,
      0,
    );
    const [southUp] = layoutFlightPlanRouteDistancePills(
      [annotation],
      [segment],
      new Set(),
      () => 50,
      180,
    );

    expect(northUp.rotationDegrees).toBeCloseTo(-5.7106, 3);
    expect(southUp.rotationDegrees).toBeCloseTo(174.2894, 3);
    expect(southUp.rotationDegrees - 180).toBeCloseTo(northUp.rotationDegrees, 8);
  });
});
