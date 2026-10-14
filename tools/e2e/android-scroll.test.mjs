// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  scrollAndroidAndAwait, readAndroidScrollSurface, findNodeByScrolling,
  scrollUntilTag, scrollUntilTagPrefix,
} from "./android-harness.mjs";
import { findScrollTarget } from "./scroll-search.mjs";
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

test("a shrinking list can move, clamp back to its starting offset, then settle at the boundary", async () => {
  // September 27 hosted trace: startup warnings cleared during traversal.
  const initial = { ...surface, position: "0,0" };
  const boundary = { ...initial, forward: "false", bounds: "[0,0][100,100]" };
  const h = harness([
    { ...initial, position: "0,164", backward: "true", moving: "true" },
    { ...boundary, moving: "true" },
    boundary,
  ]);
  assert.equal(await h.scroll("test", initial, "down"), true);
  assert.deepEqual(h.counts(), { reads: 3, gestures: 1 });
});

test("a newly reached boundary does not require sampling an intermediate offset", async () => {
  for (const [direction, boundaryKey] of [["down", "forward"], ["up", "backward"]]) {
    const initial = { ...surface, backward: "true", forward: "true" };
    const h = harness([{ ...initial, [boundaryKey]: "false" }]);
    assert.equal(await h.scroll("test", initial, direction), true);
    assert.deepEqual(h.counts(), { reads: 1, gestures: 1 });
  }
});

test("movement that returns to its starting offset without reaching a boundary is not completion", async () => {
  const h = harness([{ ...surface, position: "40", moving: "true" }, surface]);
  await assert.rejects(h.scroll("test", surface, "down"), /timed out/);
  assert.equal(h.counts().gestures, 1);
});

test("traversal rechecks the target after a shrinking list reaches its boundary", async () => {
  const boundary = { ...surface, forward: "false" };
  const h = harness([boundary]);
  const target = { "resource-id": "parity:notification-action" };
  const find = runInNewContext(`(${findNodeByScrolling})`, {
    findScrollTarget,
    queryAndroidSemanticNodes: () => h.counts().reads > 0 ? [target] : [],
    scrollAndroidSemanticSurfaceAndAwait: (serial, _orientation, direction) =>
      h.scroll(serial, h.counts().reads > 0 ? boundary : surface, direction),
  });
  assert.equal(await find("test", node => node["resource-id"] === target["resource-id"]), target);
  assert.deepEqual(h.counts(), { reads: 1, gestures: 1 });
});

function listSearch({ start = 7, target = 4, last = 27, maxSwipes = 20 } = {}) {
  let position = start;
  const gestures = [], boundaries = new Set();
  const node = { "resource-id": "parity:plan-airway-exit:YKM", visible: "true", "center-reachable": "true" };
  const read = () => position === target ? [node] : [];
  const context = {
    findScrollTarget,
    queryAndroidExactProjection: read,
    queryAndroidSemanticNodes: read,
    scrollAndroidSemanticSurfaceAndAwait: async (_serial, _orientation, direction) => {
      if (direction === "up" && position === 0 || direction === "down" && position === last) {
        boundaries.add(direction);
        return false;
      }
      gestures.push(direction);
      position += direction === "up" ? -1 : 1;
      return true;
    },
  };
  return {
    gestures, boundaries, node,
    async search(kind) {
      if (kind === "predicate") {
        return runInNewContext(`(${findNodeByScrolling})`, context)("device", entry => entry === node, maxSwipes);
      }
      const fn = kind === "exact" ? scrollUntilTag : scrollUntilTagPrefix;
      return runInNewContext(`(${fn})`, context)("device", node["resource-id"], maxSwipes, true);
    },
  };
}

for (const kind of ["exact", "prefix", "predicate"]) {
  test(`${kind} traversal leaves an already visible target in place`, async () => {
    const h = listSearch({ target: 7 });
    assert.ok(await h.search(kind));
    assert.equal(h.gestures.length, 0);
  });
  test(`${kind} traversal finds a target above the initial offset of a long list`, async () => {
    // CI found MEDEA ~7 screens down, then scanned 20 screens to the bottom.
    // Twenty return swipes merely retraced that territory; YKM was still above.
    const h = listSearch();
    assert.ok(await h.search(kind));
    assert.ok(h.gestures.length <= 40, "retain the original overall gesture budget");
  });
  test(`${kind} traversal searches the full other side, not a per-direction slice`, async () => {
    const h = listSearch({ target: 26 });
    assert.ok(await h.search(kind));
    assert.ok(h.gestures.length <= 40);
  });
  test(`${kind} absence requires both real list boundaries`, async () => {
    const h = listSearch({ target: -1 });
    assert.ok(!await h.search(kind));
    assert.deepEqual([...h.boundaries].sort(), ["down", "up"]);
  });
  test(`${kind} traversal exhaustion is a diagnostic failure, not an absent control`, async () => {
    const h = listSearch({ target: -1, last: 100 });
    await assert.rejects(h.search(kind), /scroll search exhausted.*40 gestures/);
    assert.equal(h.gestures.length, 40);
  });
}

test("scroll search propagates observation and gesture failures without changing direction", async () => {
  for (const source of ["read", "scroll"]) {
    const failure = new Error(`${source} unavailable`);
    const directions = [];
    await assert.rejects(findScrollTarget({
      read: async () => { if (source === "read") throw failure; return null; },
      scroll: async direction => { directions.push(direction); throw failure; },
    }), error => error === failure);
    assert.deepEqual(directions, source === "scroll" ? ["up"] : []);
  }
});

test("scroll discovery cannot select a clipped surface", async () => {
  const read = runInNewContext(`(${readAndroidScrollSurface})`, {
    observeUntil, E2E_TIMING: { localReadyMs: 50, pollIntervalMs: 0 },
    queryAndroidSemanticNodes: () => [
      surface, { ...surface, "resource-id": "parity:scroll:3", visible: "false" },
    ],
  });
  assert.equal((await read("test"))["resource-id"], surface["resource-id"]);
});

test("a newly drawn popup must gain input focus before discovery can choose its scroll owner", async () => {
  for (const olderPage of [[], [surface]]) {
    let reads = 0;
    const popup = { ...surface, "resource-id": "parity:scroll:2" };
    const read = runInNewContext(`(${readAndroidScrollSurface})`, {
      observeUntil, E2E_TIMING: { localReadyMs: 50, pollIntervalMs: 0 },
      queryAndroidSemanticNodes: () => [...olderPage,
        { ...popup, "center-reachable": ++reads >= 3 ? "true" : "false" }],
    });
    assert.equal((await read("test"))?.["resource-id"], popup["resource-id"],
      "pending popup focus is neither an absent list nor permission to scroll the covered page");
    assert.equal(reads, 3);
  }
});

test("a popup that never becomes actionable fails with its actual state, not end-of-list", async () => {
  const blocked = { ...surface, "center-reachable": "false" };
  const read = runInNewContext(`(${readAndroidScrollSurface})`, {
    observeUntil, E2E_TIMING: { localReadyMs: 20, pollIntervalMs: 0 },
    queryAndroidSemanticNodes: () => [blocked],
  });
  await assert.rejects(async () => read("test"), error => {
    assert.equal(error.diagnostics.last_value["resource-id"], surface["resource-id"]);
    assert.equal(error.diagnostics.last_value["center-reachable"], "false");
    return true;
  });
});

test("scroll discovery preserves genuine absence and transport failure as distinct outcomes", async () => {
  for (const failure of [false, true]) {
    let reads = 0;
    const read = runInNewContext(`(${readAndroidScrollSurface})`, {
      observeUntil, E2E_TIMING: { localReadyMs: 50, pollIntervalMs: 0 },
      queryAndroidSemanticNodes: () => { reads++; if (failure) throw new Error("broken provider"); return []; },
    });
    if (failure) await assert.rejects(read("test"), /broken provider/);
    else assert.equal(await read("test"), null);
    assert.equal(reads, 1);
  }
});
