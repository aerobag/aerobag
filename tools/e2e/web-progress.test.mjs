// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { WebSemanticTransport } from "./web-semantic-transport.mjs";
import { TransientObservationError } from "./transition-contract.mjs";

function control() {
  const sends = [];
  const transport = new WebSemanticTransport({
    send: async (method, args) => sends.push({ method, ...args }),
  }, { url: "http://localhost/" });
  const ready = { bounds: { width: 300 }, progress_range: { min: 0, max: 90, inset: 7 } };
  transport.readElement = async () => ready;
  transport.elementPoint = async (_selector, x, y) => ({ x: x * 300, y: y * 40 });
  transport.deliverInput = async (_selector, _point, evidence, types, send) => {
    assert.equal(evidence, ready);
    assert.deepEqual(types, ["pointerdown", "pointerup"]);
    await send();
  };
  return { transport, ready, sends };
}

test("physical progress input uses the rendered track, including its knob margins", async () => {
  const { transport, ready, sends } = control();
  await transport.setProgress("#scrubber", 18, ready);
  assert.equal(sends.length, 2);
  assert.equal(sends[0].type, "mousePressed");
  assert.equal(sends[1].type, "mouseReleased");
  assert.equal(sends[0].x, 7 + .2 * 286);
  assert.equal(sends[1].x, sends[0].x);
});

test("changed progress range forbids input rather than seeking the wrong trace", async () => {
  const { transport, ready, sends } = control();
  transport.readElement = async () => ({ ...ready, progress_range: { ...ready.progress_range, max: 120 } });
  await assert.rejects(transport.setProgress("#scrubber", 18, ready), TransientObservationError);
  assert.deepEqual(sends, []);
});

test("out of range progress and impossible track geometry fail without input", async () => {
  for (const mutate of [
    state => { state.value = 91; },
    state => { state.ready.progress_range.inset = 200; },
  ]) {
    const state = { ...control(), value: 18 };
    mutate(state);
    await assert.rejects(state.transport.setProgress("#scrubber", state.value, state.ready));
    assert.deepEqual(state.sends, []);
  }
});
