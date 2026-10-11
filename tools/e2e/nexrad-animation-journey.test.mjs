// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import {
  observeNexradHeldLatest, setNexradAnimationHeld,
} from "./release-journey-implementations.mjs";
import { ObservationTimeoutError } from "./transition-contract.mjs";

const initial = { tiles: 184, frame: 1, frames: 2 };

function model(t, platform, states) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-nexrad-contract-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  let clock = 0, index = 0, open = false, animating = false;
  const actions = [];
  const driver = {
    platform,
    async readAction(id) { return this.readElement(id); },
    async readElement(id) {
      if (id === "flight-data-cell:nexrad_age") return { test_id: id, enabled: true };
      assert.equal(id, "nexrad_age-animate");
      return open ? { test_id: id, enabled: true, selected: animating } : null;
    },
    async performAction(id) {
      actions.push(id);
      if (id === "flight-data-cell:nexrad_age") open = true;
      else {
        assert.equal(id, "nexrad_age-animate");
        animating = true;
      }
    },
    async back() { actions.push("back"); open = false; },
    async readProjection(prefix) {
      assert.equal(prefix, "parity:nexrad-state:");
      const state = states[Math.min(index++, states.length - 1)];
      if (!state) return [];
      const [frame, frames, tiles = 184] = state;
      return [{ id: `${prefix}tiles:${tiles}:frame:${frame}:frames:${frames}` }];
    },
    // Control observation time, not the app. No sleeps or repeated input.
    async waitForObservation() { clock += 500; },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.nexrad-animation", assertions: ["livefeed.nexrad-hold"] },
    platform, driver, artifactDir,
  });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract, scheduler: { now: () => clock, setTimeout, clearTimeout },
  });
  return { runtime, actions, now: () => clock };
}

for (const platform of ["web", "android"]) {
  for (const [name, states] of [
    ["unchanged history", [[1, 2]]],
    ["history grows while held (hosted failure)", [[1, 2], [2, 3]]],
    ["history shrinks while held", [[1, 2], [0, 1]]],
  ]) {
    test(`${platform} NEXRAD latest follows ${name}`, async (t) => {
      const { runtime, actions, now } = model(t, platform, states);
      const result = await observeNexradHeldLatest(runtime, initial, now);
      assert.equal(result.frame, result.frames - 1);
      assert.ok(now() >= 3_500, "observe beyond the ordinary newest-frame dwell");
      assert.equal(runtime.result.checks[0].pass, true);
      assert.deepEqual(actions, []);
    });
  }

  for (const [name, bad] of [
    ["cycles to an older frame", [0, 2]],
    ["keeps the old index after history grows", [1, 3]],
    ["blank animation phase", ["none", 3, 0]],
    ["missing projection", null],
    ["no painted tiles", [2, 3, 0]],
  ]) {
    test(`${platform} NEXRAD hold rejects ${name}, even if it recovers`, async (t) => {
      const { runtime, now } = model(t, platform, [[1, 2], bad, [2, 3]]);
      await assert.rejects(observeNexradHeldLatest(runtime, initial, now), /livefeed.nexrad-hold failed/);
    });
  }

  test(`${platform} NEXRAD resume rejects history growth without animation`, async (t) => {
    const { runtime, actions } = model(t, platform, [[2, 3]]);
    await assert.rejects(setNexradAnimationHeld(runtime, false), ObservationTimeoutError);
    assert.deepEqual(actions, ["flight-data-cell:nexrad_age", "nexrad_age-animate"]);
  });

  test(`${platform} NEXRAD resume requires a painted older frame in the current history`, async (t) => {
    const { runtime, actions } = model(t, platform, [[2, 3], [2, 3], [2, 3], [0, 3]]);
    const result = await setNexradAnimationHeld(runtime, false);
    assert.ok(result.frame < result.frames - 1);
    assert.deepEqual(actions, ["flight-data-cell:nexrad_age", "nexrad_age-animate", "back"]);
  });
}
