import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

describe("map route effect", () => {
  it("is driven by route invalidations, not volatile plan object identities", () => {
    const effectMatch = appSource.match(
      /session\.projectFlightPlanRoute\(\)[\s\S]*?\}, \[([^\]]+)\]\);/,
    );

    expect(effectMatch, "projectFlightPlanRoute effect").not.toBeNull();
    const deps = effectMatch?.[1] ?? "";
    expect(deps).toContain("uiInvalidationRevisions.flight_plan_route");
    expect(deps).not.toContain("plan.guidance");
    expect(deps).not.toContain("plan.resolved_legs");
  });
});
