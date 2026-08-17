// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const adapterSource = readFileSync(new URL("./appCoreAdapter.ts", import.meta.url), "utf8");
const workerSource = readFileSync(new URL("./appCore.worker.ts", import.meta.url), "utf8");

function propertySource(name: string, nextName: string): string {
  const start = adapterSource.indexOf(`      ${name}:`);
  expect(start, name).toBeGreaterThanOrEqual(0);
  const end = nextName
    ? adapterSource.indexOf(`\n      ${nextName}:`, start)
    : adapterSource.indexOf("\n    };", start);
  expect(end, `${name} end`).toBeGreaterThan(start);
  return adapterSource.slice(start, end);
}

describe("web UI session work boundary", () => {
  it("routes every resource-capable web UI session operation through the retained runner", () => {
    const scheduledMethods = [
      ["resolveChartAssetUrl", "selectMapFamily", "chart_asset"],
      ["queryMapOverlay", "queryMapSelection", "map_overlay"],
      ["queryMapSelection", "queryMapSelectionDistance", "map_selection"],
      ["queryMapSelectionForNavRef", "queryTerrainOverlay", "map_selection_for_nav_ref"],
      ["queryTerrainOverlay", "queryNexradOverlay", "terrain_overlay"],
      ["queryNexradOverlay", "queryRasterTilePlan", "nexrad_overlay"],
      ["renderTerrainOverlayTileByKey", "projectFlightPlanRoute", "terrain_tile"],
    ] as const;

    for (const [method, nextMethod, kind] of scheduledMethods) {
      const source = propertySource(method, nextMethod);
      expect(source, method).toContain("uiSessionWorkRunner.run(");
      expect(source, method).toContain(`\"${kind}\"`);
    }
  });

  it("leaves bounded resource-free reads outside the paging scheduler", () => {
    expect(propertySource("queryMapSelectionDistance", "queryMapSelectionForNavRef"))
      .not.toContain("uiSessionWorkRunner");
    expect(propertySource("queryRasterTilePlan", "renderTerrainOverlayTileByKey"))
      .not.toContain("uiSessionWorkRunner");
  });

  it("uses app-core policy inside the existing Worker and closes it before the session", () => {
    expect(adapterSource).toContain("this.module.ui_session_work_scheduler_request");
    expect(adapterSource).toContain("this.module.ui_session_work_scheduler_complete");
    expect(workerSource).toContain("loadWasmAdapterOnThisThread");
    expect(workerSource).not.toContain("WebUiSessionWorkRunner");

    const destroyStart = adapterSource.lastIndexOf("\n      destroy:");
    const destroyEnd = adapterSource.indexOf("\n    };", destroyStart);
    const destroySource = adapterSource.slice(destroyStart, destroyEnd);
    expect(destroySource).toContain("await uiSessionWorkRunner.close()");
    expect(destroySource.indexOf("await uiSessionWorkRunner.close()"))
      .toBeLessThan(destroySource.indexOf("this.module.destroy_session(handle)"));
  });

  it("rolls back every native owner acquired during failed session startup", () => {
    const bootstrapStart = adapterSource.indexOf("const createSession = async (");
    const schedulerStart = adapterSource.indexOf("let snapshotRefreshSchedulerHandle: number;", bootstrapStart);
    const returnedSession = adapterSource.indexOf("\n    return {", schedulerStart);
    const bootstrap = adapterSource.slice(bootstrapStart, schedulerStart);
    const scheduledStartup = adapterSource.slice(schedulerStart, returnedSession);

    expect(bootstrap).toContain("module.destroy_session(created.handle)");
    expect(scheduledStartup).toContain(
      "await this.module.destroy_session_snapshot_refresh_scheduler(snapshotRefreshSchedulerHandle)",
    );
    expect(scheduledStartup.lastIndexOf("destroy_session_snapshot_refresh_scheduler"))
      .toBeLessThan(scheduledStartup.lastIndexOf("this.module.destroy_session(handle)"));
  });
});
