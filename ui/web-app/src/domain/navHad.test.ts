import { describe, expect, it } from "vitest";
import {
  hadKeyComponent,
  hadUpperKeyComponent,
  plateAirportKey,
  plateByIdKey,
  plateCifpMatchKey,
  plateProcedureCandidatesKey,
  procedureDistinctRowsKey,
  procedureListKey,
  procedureMaterializationRowsKey,
} from "./navHad";

describe("navHad key contract", () => {
  it("normalizes lookup components before building keys", () => {
    expect(hadKeyComponent(" krdd ")).toBe("krdd");
    expect(hadUpperKeyComponent(" krdd ")).toBe("KRDD");
    expect(hadUpperKeyComponent("LOC 34")).toBe("LOC%2034");
  });

  it("names plate lookup keyspaces by query shape", () => {
    expect(plateAirportKey("krdd")).toBe("plate/airport/KRDD");
    expect(plateByIdKey("plate:KRDD:IAP-CA-ILS OR LOC RWY 34.png"))
      .toBe("plate/by-id/plate%3AKRDD%3AIAP-CA-ILS%20OR%20LOC%20RWY%2034.png");
    expect(plateCifpMatchKey("krdd", "i34")).toBe("plate/cifp/KRDD/I34");
    expect(plateProcedureCandidatesKey("plate:KRDD:IAP-CA-ILS OR LOC RWY 34.png"))
      .toBe("plate/procedure-candidates/plate%3AKRDD%3AIAP-CA-ILS%20OR%20LOC%20RWY%2034.png");
  });

  it("names procedure lookup keyspaces by query shape", () => {
    expect(procedureListKey("krdd", "approach")).toBe("procedure/list/KRDD/APPROACH");
    expect(procedureDistinctRowsKey("krdd", "i34")).toBe("procedure/distinct-rows/KRDD/I34");
    expect(procedureMaterializationRowsKey("krdd", "i34")).toBe("procedure/materialization-rows/KRDD/I34");
  });
});
