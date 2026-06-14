import { describe, expect, it } from "vitest";

import { flightPlanRouteSegmentRenderKey } from "./flightPlanRouteRender";

describe("flight plan route rendering", () => {
  it("does not use canonical guidance ids as globally unique React keys", () => {
    const first = flightPlanRouteSegmentRenderKey({ id: "airway--0#0" }, 0);
    const second = flightPlanRouteSegmentRenderKey({ id: "airway--0#0" }, 12);

    expect(first).toBe("0:airway--0#0");
    expect(second).toBe("12:airway--0#0");
    expect(first).not.toBe(second);
  });
});
