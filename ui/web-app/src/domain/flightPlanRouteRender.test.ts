// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import {
  flightPlanRouteSegmentRenderKey,
  spacedRouteChevronPlacements,
} from "./flightPlanRouteRender";

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
});
