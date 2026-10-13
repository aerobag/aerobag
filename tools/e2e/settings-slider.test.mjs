// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { selectAndroidSettingsSliderStop } from "./release-journey-implementations.mjs";
import { ObservationTimeoutError } from "./transition-contract.mjs";

function model(t, { rowId = "display_dim_timeout", from = "2m", to = "10s", outcome = "success" } = {}) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-settings-slider-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const prefix = `parity:settings-slider:${rowId}:`;
  let value = from;
  let clock = 0;
  let pending = false;
  const actions = [];
  const entry = () => ({ id: `${prefix}${value}`, test_id: `${prefix}${value}`, state: value,
    enabled: true, bounds: "[100,1585][980,1711]", semantic_path: "projection-provider:test:4" });
  const driver = {
    async readSessionRevision() { return 1; },
    async revealElement(id) { return this.readElement(id); },
    async readElement(id) { return id === `${prefix}${value}` ? entry() : null; },
    async readProjection(requested) { assert.equal(requested, prefix); return value === null ? [] : [entry()]; },
    async drag(id, delta, ready) {
      actions.push({ kind: "drag", id, delta, ready });
      // The retained failure and sparse-MOVE Compose reproduction land here.
      value = "30s";
    },
    async setProgress(id, index, ready) {
      actions.push({ kind: "progress", id, index, ready });
      assert.equal(id, `settings-slider:${rowId}:${from}`);
      assert.equal(index, 0);
      assert.equal(ready.test_id, `parity:${id}`);
      assert.equal(ready.semantic_path, "projection-provider:test:4");
      if (outcome === "rejected") throw new Error("progress action rejected");
      if (outcome === "delayed") pending = true;
      else if (outcome === "wrong-stop") value = "30s";
      else if (outcome === "missing") value = null;
      else if (outcome !== "lost-input") value = to;
    },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.settings-slider", assertions: [] }, platform: "android", driver, artifactDir,
  });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract,
    scheduler: { now: () => clock, setTimeout, clearTimeout },
    waitForObservation: async () => { clock += 50; if (pending) { value = to; pending = false; } },
  });
  return { actions, runtime, run: () => selectAndroidSettingsSliderStop(runtime, {
    rowId, from, to, index: 0, description: `change ${rowId}`,
  }) };
}

for (const options of [
  {}, { rowId: "inactivity_sleep_timeout", from: "1h", to: "30m" }, { outcome: "delayed" },
]) {
  test(`slider selection uses one rendered-range tap: ${JSON.stringify(options)}`, async (t) => {
    const subject = model(t, options);
    const observed = await subject.run();
    assert.equal(observed[0].state, options.to ?? "10s");
    assert.equal(subject.actions.length, 1);
    assert.equal(subject.actions[0].kind, "progress");
  });
}

for (const [outcome, state] of [["wrong-stop", "30s"], ["lost-input", "2m"], ["missing", null]]) {
  test(`slider ${outcome} fails with actual state and no retry`, async (t) => {
    const subject = model(t, { outcome });
    await assert.rejects(subject.run(), error => {
      assert.ok(error instanceof ObservationTimeoutError);
      assert.deepEqual(error.diagnostics.last_value.map(entry => entry.state), state === null ? [] : [state]);
      return true;
    });
    assert.equal(subject.actions.length, 1);
  });
}

test("rejected slider input is terminal, without another tap", async (t) => {
  const subject = model(t, { outcome: "rejected" });
  await assert.rejects(subject.run(), /progress action rejected/);
  assert.equal(subject.actions.length, 1);
});
