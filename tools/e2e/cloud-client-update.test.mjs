// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { selectNextTestClient } from "./cloud-client-update.mjs";
import { journeyById } from "./release-journey-registry.mjs";
import { androidJourneyEpochMs } from "./android-harness.mjs";

test("simulated client update changes only the E2E selector, not data or consent", () => {
  const before = { version: 1, cloud: {
    account: { encrypted: "untouched" }, records: { pending_keys: ["flight_plan/current"] },
    test_next_client: false,
  } };
  const after = JSON.parse(selectNextTestClient(JSON.stringify(before)));
  assert.deepEqual(after, { ...before, cloud: { ...before.cloud, test_next_client: true } });
  assert.throws(() => selectNextTestClient(JSON.stringify({cloud: {account: {}}})), /E2E-enabled/);
});

test("both cloud journeys use a disposable server and the real authentication clock", () => {
  for (const id of ["shared.cloud-crossfill", "shared.cloud-account-upgrade"]) {
    assert.equal(journeyById(id).cloud_server, true);
    assert.equal(journeyById(id).android_isolated, true);
    assert.equal(androidJourneyEpochMs(id, 100, 200), 200);
  }
});
