// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { gesturePlate, parsePlateViewport, plateGestureCompleted } from "./plate-gestures.mjs";
import { observeUntil, performTransition } from "./transition-contract.mjs";
import { selectInspectorSpot } from "./release-journey-implementations.mjs";
import { auditJourneyStructure } from "./journey-structure-audit.mjs";

const first = { chartId: "plate:CHINS FIVE.png", zoom: 1, left: 200, top: 0 };
function projection(value) {
  return value ? [{ id: `parity:plate-viewport:chart:${value.chartId}:zoom:${value.zoom}:left:${value.left}:top:${value.top}` }] : [];
}

test("viewport parser preserves document identity and numeric geometry", () => {
  assert.deepEqual(parsePlateViewport(projection(first)), first);
  assert.equal(parsePlateViewport([]), null);
});

test("observation readers and predicates cannot replay mutations or invent deadlines", () => {
  const violations = auditJourneyStructure(`
    async function bad(runtime) {
      await runtime.observe("bad", () => runtime.driver.zoom("plate-surface", 1),
        () => runtime.driver.drag("plate-surface", {x: 1, y: 1}), 999999);
      await runtime.transition("bad", {
        ready: () => true, act: () => {}, complete: () => true,
        completionSatisfied: () => runtime.driver.zoom("plate-surface", 1),
      });
    }
  `).map((violation) => violation.message);
  assert.ok(violations.includes("observe callback invokes mutating operation zoom"));
  assert.ok(violations.includes("observe predicate invokes mutating operation drag"));
  assert.ok(violations.includes("transition completionSatisfied callback invokes mutating operation zoom"));
  assert.ok(violations.includes("raw observe deadline is forbidden; use a named E2E_TIMING class"));
});

test("gesture outcomes reject unrelated movement, wrong direction, and a different document", () => {
  const zoom = { zoom: -360 };
  const pan = { pan: { x: 0, y: -600 } };
  assert.equal(plateGestureCompleted(first, { ...first, left: 10 }, zoom), false);
  assert.equal(plateGestureCompleted(first, { ...first, zoom: 0.5 }, zoom), false);
  assert.equal(plateGestureCompleted(first, { ...first, chartId: "other", zoom: 2 }, zoom), false);
  assert.equal(plateGestureCompleted(first, { ...first, zoom: 2 }, zoom), true);
  assert.equal(plateGestureCompleted(first, { ...first, top: 10 }, pan), false);
  assert.equal(plateGestureCompleted(first, { ...first, zoom: 2, top: -10 }, pan), false);
  assert.equal(plateGestureCompleted(first, { ...first, top: -10 }, pan), true);
});

test("plate gesture waits for its document, delivers once, and retains before/after geometry", async () => {
  let state = { ...first, chartId: "stale" };
  let deliveries = 0;
  let receipt;
  const runtime = {
    driver: {
      readProjection: async () => projection(state),
      readElement: async () => ({ test_id: "plate-surface", actionable: true }),
      zoom: async (_id, amount, ready) => {
        deliveries++;
        assert.equal(ready.viewport.chartId, first.chartId);
        state = { ...state, zoom: state.zoom - amount / 360 };
      },
    },
    transition: async (description, contract) => (await performTransition(description, {
      ...contract,
      waitForObservation: async () => { state = { ...first }; },
      onTiming: (timing) => { receipt = timing; },
    })).value,
  };
  assert.deepEqual(await gesturePlate(runtime, "zoom", first.chartId, { zoom: -360 }),
    { before: first, after: { ...first, zoom: 2 } });
  assert.equal(deliveries, 1);
  assert.equal(receipt.ready_state.viewport.chartId, first.chartId);
  assert.equal(receipt.observation.zoom, 2);
});

test("unsatisfied observations retain state, including false and zero, without replaying actions", async () => {
  let attempts = 0;
  await assert.rejects(observeUntil("wait for zoom", async () => ({ chartId: "wrong", zoom: ++attempts }), {
    timeoutMs: 20, intervalMs: 1, accept: (state) => state.chartId === "wanted",
  }), (error) => {
    assert.equal(error.diagnostics.last_value.chartId, "wrong");
    assert.ok(error.diagnostics.observations.length > 0);
    assert.ok(error.diagnostics.observations.length <= 8);
    return true;
  });
  assert.equal((await observeUntil("zero is a value", async () => 0, { accept: (v) => v === 0 })).value, 0);
});

for (const platform of ["web", "android"]) {
  for (const initial of ["SPOT", "WN96", "FIX"]) {
    test(`${platform} selects SPOT explicitly when ${initial} wins the map hit test`, async () => {
      let selection = initial;
      const actions = [];
      const runtime = {
        platform,
        driver: { readProjection: async (id) => {
          assert.equal(id, "parity:map-selection-selected:SPOT");
          return selection === "SPOT" ? [{ text: "SPOT · Elevation 275 ft" }] : [];
        } },
        action: async (_description, id, { complete }) => {
          assert.equal(await complete(), null);
          actions.push(id); selection = "SPOT";
          return complete();
        },
      };
      assert.equal((await selectInspectorSpot(runtime)).text, "SPOT · Elevation 275 ft");
      assert.deepEqual(actions, initial === "SPOT" ? [] : [platform === "web"
        ? "map-selection-item-navaid-SPOT" : "map-selection-item:navaid-SPOT"]);
    });
  }
}
