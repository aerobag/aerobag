// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

const assert = require("node:assert/strict");
const { parse } = require("./chart_cutline_editor_assets/coordinate-input.js");

for (const [text, axis, expected] of [
  ["58 15", "latitude", 58.25],
  ["  58   15  ", "latitude", 58.25],
  ["-134 30", "longitude", -134.5],
  ["-0 30", "longitude", -0.5],
  ["58 15.5", "latitude", 58.258333333],
  ["58.25", "latitude", 58.25],
  ["+.5", "latitude", 0.5],
  ["90 0", "latitude", 90],
  ["-180 0", "longitude", -180],
  ["", "latitude", null],
  [" ", "longitude", null],
]) assert.equal(parse(text, axis), expected, text);

for (const text of ["58 60", "58 -1", "58 -0", "58.5 15", "58 15 30", "NaN", "Infinity",
  "1e2", "0x20", "90 1", "91", "58N", "58 .", "-90 1", "1".repeat(400)]) {
  assert.throws(() => parse(text, "latitude"), undefined, text);
}
assert.throws(() => parse("180 1", "longitude"));
assert.throws(() => parse("58", "nonsense"));
console.log("Coordinate input tests passed");
