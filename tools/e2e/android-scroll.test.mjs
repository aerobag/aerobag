// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { scrollAndroidAndAwait, readAndroidScrollSurface } from "./android-harness.mjs";
import { observeUntil, TransientObservationError } from "./transition-contract.mjs";

const surface = { "resource-id": "parity:scroll:1", "semantic-path": "projection-provider:process:1",
  incarnation: "process", position: "0", forward: "true", backward: "false", moving: "false",
  bounds: "[0,0][100,200]", orientation: "vertical", visible: "true", "center-reachable": "true" };

function harness(samples) {
  let reads = 0, gestures = 0;
  const scroll = runInNewContext(`(${scrollAndroidAndAwait})`, {
    E2E_TIMING: { userTransitionDeadlineMs: 50, pollIntervalMs: 0 }, observeUntil,
    queryAndroidExactProjection: () => {
      const sample = samples[Math.min(reads++, samples.length - 1)];
      if (sample instanceof Error) throw sample;
      return sample ? [sample] : [];
    },
    scrollAndroidSemanticNode: () => { gestures++; },
    dumpAndroid: () => { throw new Error("tree access forbidden"); },
  });
  return { scroll, counts: () => ({ reads, gestures }) };
}

test("scroll waits for real movement AND end-of-scroll, ignoring unrelated clock updates", async () => {
  const h = harness([
    { ...surface, text: "clock 1", moving: "true" },
    { ...surface, position: "50", moving: "true" },
    { ...surface, position: "80", moving: "false" },
  ]);
  assert.equal(await h.scroll("test", surface, "down"), true);
  assert.deepEqual(h.counts(), { reads: 3, gestures: 1 });
});

test("a genuine published boundary needs neither a gesture nor extra observations", async () => {
  const h = harness([]);
  assert.equal(await h.scroll("test", surface, "up"), false);
  assert.deepEqual(h.counts(), { reads: 0, gestures: 0 });
});

test("transport errors, disappearance and replacement never masquerade as scroll boundaries", async () => {
  for (const sample of [new Error("broken source"), null, { ...surface, incarnation: "new-process" }]) {
    const h = harness([sample]);
    await assert.rejects(h.scroll("test", surface, "down"));
    assert.equal(h.counts().gestures, 1);
  }
});

test("transient read recovery does not replay the physical gesture", async () => {
  const h = harness([new TransientObservationError("busy"), { ...surface, position: "20" }]);
  assert.equal(await h.scroll("test", surface, "down"), true);
  assert.deepEqual(h.counts(), { reads: 2, gestures: 1 });
});

test("successful input delivery without scrolling is a failure, not an edge", async () => {
  const h = harness([{ ...surface, text: "clock ticks forever" }]);
  await assert.rejects(h.scroll("test", surface, "down"), error => {
    assert.match(error.message, /timed out/);
    assert.equal(error.diagnostics.last_value.position, surface.position);
    assert.equal(error.diagnostics.last_value.moving, "false");
    return true;
  });
  assert.equal(h.counts().gestures, 1);
});

test("scroll discovery cannot select an unfocused or clipped surface", () => {
  const read = runInNewContext(`(${readAndroidScrollSurface})`, {
    queryAndroidSemanticNodes: () => [
      surface, { ...surface, "resource-id": "parity:scroll:2", "center-reachable": "false" },
      { ...surface, "resource-id": "parity:scroll:3", visible: "false" },
    ],
  });
  assert.equal(read("test")["resource-id"], surface["resource-id"]);
});
