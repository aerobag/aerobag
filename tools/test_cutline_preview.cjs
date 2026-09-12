// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

"use strict";
const assert = require("node:assert/strict");
const latestPreview = require("./chart_cutline_editor_assets/latest-preview.js");

async function main() {
  const requests = [], received = [], errors = [];
  const queue = latestPreview((value) => new Promise((resolve, reject) => {
    requests.push({ value, resolve, reject });
  }), (result) => received.push(result), (error) => errors.push(error));
  queue.submit("first");
  queue.submit("second");
  queue.submit("latest");
  assert.equal(requests.length, 1);
  requests[0].resolve("stale");
  await Promise.resolve();
  assert.deepEqual(received, []);
  assert.deepEqual(requests.map((r) => r.value), ["first", "latest"]);
  requests[1].resolve("current");
  await Promise.resolve();
  assert.deepEqual(received, ["current"]);
  queue.submit("old chart");
  queue.reset();
  queue.submit("new chart");
  requests[2].reject("old error");
  await Promise.resolve();
  assert.deepEqual(errors, []);
  requests[3].resolve("new outline");
  await Promise.resolve();
  assert.deepEqual(received, ["current", "new outline"]);
  queue.submit("invalid");
  requests[4].reject("invalid outline");
  await Promise.resolve();
  assert.deepEqual(errors, ["invalid outline"]);
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
