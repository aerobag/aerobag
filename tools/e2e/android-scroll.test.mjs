// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { androidScrollGeometrySignature } from "./android-harness.mjs";
import { observeChangedValueUntilStable, ObservationTimeoutError } from "./transition-contract.mjs";

function frame(y, tick, row = "KSEA") {
  return `<hierarchy>
    <node semantic-path="0/1" bounds="[0,0][400,800]" class="list" scrollable="true">
      <node semantic-path="0/1/0" bounds="[0,${y}][400,${y + 100}]" class="button" clickable="true" resource-id="row:${row}" text="${row}" />
      <node semantic-path="0/1/0/0" bounds="[100,${y}][200,${y + 100}]" class="text" resource-id="eta:${tick}" text="${tick}" />
    </node>
    <node semantic-path="0/10" bounds="[0,800][400,900]" class="footer" resource-id="ownship:${tick}" text="${tick}" />
  </hierarchy>`;
}

test("scroll geometry ignores live text and unrelated UI but tracks row identity and motion", () => {
  const key = xml => androidScrollGeometrySignature(xml, "0/1");
  assert.equal(key(frame(100, 1)), key(frame(100, 2)));
  assert.notEqual(key(frame(100, 1)), key(frame(90, 1)));
  assert.notEqual(key(frame(100, 1)), key(frame(100, 1, "YKM")));
});

test("scroll settles after real geometry changes even when every XML frame has a new clock", async () => {
  const positions = [100, 90, 80, 80, 80];
  let reads = 0;
  const result = await observeChangedValueUntilStable("scroll", async () => frame(positions[reads], ++reads), {
    initialValue: frame(100, 0),
    valueKey: xml => androidScrollGeometrySignature(xml, "0/1"),
    stableSamples: 3, intervalMs: 0, timeoutMs: 100,
  });
  assert.equal(reads, positions.length);
  assert.equal(androidScrollGeometrySignature(result.value, "0/1"), androidScrollGeometrySignature(frame(80, 0), "0/1"));
});

test("clock ticks alone never prove that a scroll happened", async () => {
  let reads = 0;
  await assert.rejects(observeChangedValueUntilStable("scroll", async () => frame(100, ++reads), {
    initialValue: frame(100, 0),
    valueKey: xml => androidScrollGeometrySignature(xml, "0/1"),
    stableSamples: 3, intervalMs: 1, timeoutMs: 40,
  }), ObservationTimeoutError);
});
