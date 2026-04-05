import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter } from "./appCoreAdapter";

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
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });
});
