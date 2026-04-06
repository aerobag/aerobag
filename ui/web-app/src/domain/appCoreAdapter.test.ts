import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter, MockAppCoreAdapter } from "./appCoreAdapter";
import type { CatalogJson } from "./types";

const chartCatalog: CatalogJson = {
  schema_version: 1,
  cycle: "2026-04-16",
  catalog_revision: "test",
  families: [],
  regions: [],
  packages: [],
  charts: [
    {
      id: { family: "tac", name: "Boston TAC", cycle: "2026-04-16" },
      family_id: "tac",
      name: "Boston TAC",
      display_name: "Boston TAC",
      cycle: "2026-04-16",
      region_ids: ["ne"],
      max_zoom: 11,
      tile_path_template: "tiles/1/{z}/{x}/{y}.webp",
      coverage: {
        kind: "polygon_ref",
        value: { polygon_id: "tac:boston" },
      },
    },
  ],
  plates: [],
  supplements: [],
};

const geometry = {
  polygons: [
    {
      id: "tac:boston",
      points: [
        [-71.2, 42.2],
        [-70.8, 42.2],
        [-70.8, 42.5],
        [-71.2, 42.5],
        [-71.2, 42.2],
      ],
    },
  ],
};

describe("loadBestAvailableAdapter", () => {
  it("falls back to the mock adapter when the generated wasm module is missing", async () => {
    const loaded = await loadBestAvailableAdapter(async () => {
      throw new Error("module not found");
    });

    expect(loaded.backend).toBe("mock");
    expect(loaded.detail).toContain("module not found");
  });

  it("uses the wasm adapter when the generated module exports the expected API", async () => {
    const loaded = await loadBestAvailableAdapter(async () => ({
      replace_flight_plan_state: async (stateJson: string) => stateJson,
      set_content_policy_state: async (stateJson: string) => stateJson,
      refresh_content_state: async (stateJson: string) => stateJson,
      chart_for_position: async () => "null",
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });

  it("mock chart lookup finds the configured TAC polygon", async () => {
    const adapter = new MockAppCoreAdapter();
    const chart = await adapter.chartForPosition(
      chartCatalog,
      geometry,
      "tac",
      42.35,
      -71.0,
    );

    expect(chart?.display_name).toBe("Boston TAC");
  });
});
