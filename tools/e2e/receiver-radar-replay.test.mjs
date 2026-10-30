// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { exerciseReceiverRadarReplay } from "./release-journey-implementations.mjs";
import { E2E_TIMING, ObservationTimeoutError, observeUntil } from "./transition-contract.mjs";

function model(t, platform, failure = null) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-receiver-radar-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  let clock = 0, cursor = 18, status = "paused", playStarted = 0;
  let painted = { tiles: 1, frame: 1, frames: 2 }, pending = null;
  const actions = [];
  const driver = {
    platform,
    async readAction(id) { return { test_id: id, enabled: true }; },
    async setProgress(id, seconds) {
      assert.equal(id, "playback-overview");
      actions.push(["seek", seconds]);
      if (failure === "lost-seek") return;
      cursor = seconds;
      status = failure === "not-paused" ? "playing" : "paused";
      pending = { at: clock + 2_000, frame: seconds === 16.5 ? 0 : 1 };
    },
    async performAction(id) {
      assert.equal(id, "playback-play-toggle");
      actions.push(["play"]);
      status = "playing";
      playStarted = clock;
    },
    async readProjection(prefix) {
      if (prefix === "parity:playback-widget:") {
        const elapsed = status === "playing" && failure !== "frozen-clock" ? (clock - playStarted) / 1000 : 0;
        return [{ id: `${prefix}status:${status}:cursor:${cursor + elapsed}:duration:90:rate:1:gaps:0` }];
      }
      assert.equal(prefix, "parity:nexrad-state:");
      if (status === "playing") {
        // The hosted schedule samples newest/blank repeatedly; neither the
        // producer nor observer is guaranteed to visit the one-second old phase.
        painted = Math.floor(clock / 2_000) % 2 === 0
          ? { tiles: 1, frame: 1, frames: 2 } : { tiles: 0, frame: "none", frames: 0 };
      } else if (pending && clock >= pending.at) {
        painted = { tiles: 1, frame: pending.frame, frames: 2 };
        pending = null;
      }
      const state = failure === "held-latest" ? { tiles: 1, frame: 1, frames: 2 }
        : failure === "missing-tiles" ? { ...painted, tiles: 0 }
        : failure === "wrong-history" ? { ...painted, frames: 3 } : painted;
      return [{ id: `${prefix}tiles:${state.tiles}:frame:${state.frame}:frames:${state.frames}` }];
    },
    async waitForObservation() { clock += 100; },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.receiver-radar", assertions: [] }, platform, driver, artifactDir,
  });
  const scheduler = { now: () => clock, setTimeout, clearTimeout };
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, { ...contract, scheduler });
  // Keep the real bounded observer, substituting only its clock and driver IO.
  runtime.eventually = async (description, read, timeoutMs) => (await observeUntil(description, read, {
    timeoutMs, scheduler, waitForNextProbe: driver.waitForObservation,
  })).value;
  runtime.observe = async (description, read, accept, timeoutMs = E2E_TIMING.localReadyMs) =>
    (await observeUntil(description, read, { accept, timeoutMs, scheduler,
      waitForNextProbe: driver.waitForObservation })).value;
  return { runtime, actions };
}

for (const platform of ["web", "android"]) {
  test(`${platform} receiver radar inspects held phases despite slow rendering`, async t => {
    const { runtime, actions } = model(t, platform);
    const frames = await exerciseReceiverRadarReplay(runtime);
    assert.equal(frames.first.frame, 0);
    assert.equal(frames.next.frame, 1);
    assert.deepEqual(actions, [["seek", 16.5], ["seek", 18.5], ["play"]]);
  });
  for (const failure of ["lost-seek", "not-paused", "held-latest", "missing-tiles", "wrong-history", "frozen-clock"]) {
    test(`${platform} receiver radar rejects ${failure} without retrying input`, async t => {
      const { runtime, actions } = model(t, platform, failure);
      await assert.rejects(exerciseReceiverRadarReplay(runtime), ObservationTimeoutError);
      assert.deepEqual(actions, failure === "frozen-clock"
        ? [["seek", 16.5], ["seek", 18.5], ["play"]] : [["seek", 16.5]]);
    });
  }
}
