// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import {
  classifyRasterTileLoadRecovery,
  e2eRasterTileStallUrl,
  rasterTileLoadUrl,
} from "./rasterTileLoadRecovery";

describe("raster tile load recovery", () => {
  it("retries only unresolved tiles that retain a recovery attempt", () => {
    expect(classifyRasterTileLoadRecovery(
      ["loaded", "failed", "fresh", "retried"],
      new Set(["loaded"]),
      new Set(["failed"]),
      new Map([["retried", 1]]),
    )).toEqual({
      retry: ["fresh"],
      exhausted: ["retried"],
    });
  });

  it("never creates an unbounded retry loop", () => {
    expect(classifyRasterTileLoadRecovery(
      ["tile"],
      new Set(),
      new Set(),
      new Map([["tile", 7]]),
    )).toEqual({ retry: [], exhausted: ["tile"] });
  });

  it("cache-busts only recovery attempts while preserving the resource URL", () => {
    const source = "https://example.test/tiles/1/2/3.webp?v=package#tile";
    expect(rasterTileLoadUrl(source, 0)).toBe(source);
    expect(rasterTileLoadUrl(source, 1)).toBe(
      "https://example.test/tiles/1/2/3.webp?v=package&aerobag_retry=1#tile",
    );
    expect(rasterTileLoadUrl("/tiles/1.webp?v=package", 1)).toBe(
      "/tiles/1.webp?v=package&aerobag_retry=1",
    );
    expect(rasterTileLoadUrl("tiles/1.webp", 1)).toBe(
      "tiles/1.webp?aerobag_retry=1",
    );
  });

  it("provides a deterministic stalled resource for browser fault injection", () => {
    expect(e2eRasterTileStallUrl("https://example.test/tiles/1.webp?v=package")).toBe(
      "https://example.test/tiles/1.webp.aerobag-e2e-stall?v=package",
    );
    expect(e2eRasterTileStallUrl("/tiles/1.webp?v=package")).toBe(
      "/tiles/1.webp.aerobag-e2e-stall?v=package",
    );
  });
});
