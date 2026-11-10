// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { refreshOfflineCatalog } from "./release-journey-implementations.mjs";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { ObservationTimeoutError } from "./transition-contract.mjs";

function catalogModel(t, outcome = "success") {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-offline-catalog-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const actions = [];
  let requested = false;
  let stage = 0;
  let clock = 0;
  t.mock.method(globalThis, "fetch", async () => ({
    ok: true,
    json: async () => requested ? [{ method: "GET", url: "/packages/current_artifacts.json" }] : [],
  }));
  const driver = {
    platform: "android",
    async readSessionRevision() { return stage + 1; },
    async readElement(id) {
      if (id === "offline-packages-panel") {
        const fetched = stage >= 2 && outcome !== "failed" ? 200 : 100;
        return {
          id, enabled: true,
          disabled_reason: `catalog-fetched-at:${fetched}:catalog-loading:${stage === 1}:operation-active:${stage === 1 || stage === 2}`,
        };
      }
      assert.ok(["offline-refresh-button", "offline-sync-button"].includes(id), id);
      // The preceding rendered frame survives while the catalog request begins.
      return { id, enabled: stage !== 1, text: "APPLY CHANGES", bounds: "[0,0][100,100]" };
    },
    async readAction(id) { return this.readElement(id); },
    async performAction(id) {
      actions.push(id);
      assert.equal(id, "offline-refresh-button");
      requested = true;
      if (outcome === "instant") stage = 3;
    },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.offline-catalog", assertions: [] }, platform: "android",
    driver, artifactDir, fixtureOrigin: "http://fixture.invalid",
  });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract,
    scheduler: { now: () => clock, setTimeout, clearTimeout },
    waitForObservation: async () => {
      clock += 100;
      if (outcome !== "stuck") stage = Math.min(stage + 1, 3);
    },
  });
  return { runtime, actions, get stage() { return stage; } };
}

test("catalog refresh waits past HTTP start, stale enabled frame, and operation cleanup", async t => {
  const model = catalogModel(t);
  await refreshOfflineCatalog(model.runtime);
  assert.equal(model.stage, 3, "do not authorize sync against the old catalog or unfinished refresh");
  assert.deepEqual(model.actions, ["offline-refresh-button"]);
});

test("an immediately completed refresh does not require observing its busy frame", async t => {
  const model = catalogModel(t, "instant");
  await refreshOfflineCatalog(model.runtime);
  assert.equal(model.stage, 3);
  assert.deepEqual(model.actions, ["offline-refresh-button"]);
});

for (const outcome of ["stuck", "failed"]) {
  test(`catalog ${outcome} cannot complete from a request or the old idle button`, async t => {
    const model = catalogModel(t, outcome);
    await assert.rejects(refreshOfflineCatalog(model.runtime), error => {
      assert.ok(error instanceof ObservationTimeoutError, error.message);
      assert.equal(error.diagnostics.last_value.catalog.fetchedAt, 100);
      assert.ok(error.diagnostics.last_value.button, "retain rendered control diagnostics");
      return true;
    });
    assert.deepEqual(model.actions, ["offline-refresh-button"]);
  });
}
