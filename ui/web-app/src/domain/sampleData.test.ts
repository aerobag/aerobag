import { describe, expect, it } from "vitest";
import { sampleCatalog } from "./sampleData";

describe("sampleCatalog", () => {
  it("loads the canonical product catalog artifact with canonical family ids", () => {
    expect(sampleCatalog.schema_version).toEqual(expect.any(Number));
    expect(sampleCatalog.charts.length).toBeGreaterThan(0);
    const familyIds = new Set(sampleCatalog.families.map((family) => family.id));
    expect(familyIds.has("tac")).toBe(true);
    expect(familyIds.has("sectional" as never)).toBe(false);
    expect(familyIds.has("ifr_low" as never)).toBe(false);
    expect(familyIds.has("ifr_high" as never)).toBe(false);
    for (const familyId of familyIds) {
      expect(["sec", "tac", "wac", "enr-l", "enr-h", "ifr_area", "flyway", "heli", "misc"]).toContain(familyId);
    }
  });
});
