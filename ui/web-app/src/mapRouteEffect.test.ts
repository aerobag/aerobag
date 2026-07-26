// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

describe("map route effect", () => {
  it("is driven by the core-owned route revision, not plan object identities", () => {
    const effectMatch = appSource.match(
      /session\.projectFlightPlanRoute\(\)[\s\S]*?\}, \[([^\]]+)\]\);/,
    );

    expect(effectMatch, "projectFlightPlanRoute effect").not.toBeNull();
    const deps = effectMatch?.[1] ?? "";
    expect(deps).toContain("flightPlanRouteRevision");
    expect(deps).not.toContain("plan.guidance");
    expect(deps).not.toContain("plan.resolvedLegs");
    expect(appSource).toContain(
      "flightPlanRouteProjection.flight_plan_route_revision === flightPlanRouteRevision",
    );
  });
});
