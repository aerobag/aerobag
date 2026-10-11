// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { simulateCloudClientUpdate, selectNextTestClient } from "./cloud-client-update.mjs";
import { journeyById } from "./release-journey-registry.mjs";
import { androidJourneyEpochMs, launchFreshAndroidApp, restartAndroidAppAcrossSemanticLifecycle } from "./android-harness.mjs";
import { AndroidSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { restartAndroidAfterPersistence } from "./android-persistence.mjs";

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

// Execute the real update, driver, save barrier, launcher and process lifecycle.
// Only device I/O is modeled. In particular reload is not a stub that conceals
// a caller killing the process before its persistence evidence can be read.
function androidUpdateModel({ saveError = null, stopError = null, updateError = null } = {}) {
  const events = [];
  let running = true, saved = false, stoppedReads = 0, clock = 0;
  let settings = JSON.stringify({ version: 1, cloud: {
    account: { encrypted: "unchanged" }, records: { pending_keys: ["flight_plan/current"] },
    test_next_client: false,
  } });
  const launch = runInNewContext(`(${launchFreshAndroidApp})`, {
    ANDROID_PACKAGE: "org.aerobag.app", ANDROID_ACTIVITY: "org.aerobag.app/.MainActivity",
    APP_START_TIMEOUT_MS: 30000,
    wakeAndUnlock() {}, grantAerobagRuntimePermissions() {}, ensureAndroidSemanticDriver() {},
    firstAerobagProcessNode() { assert.equal(running, true); return { "resource-id": "new-process" }; },
    androidAppLifecyclePresent() { stoppedReads++; return stoppedReads < 2; },
    restartAndroidAppAcrossSemanticLifecycle,
    adb(_serial, args) {
      if (args.includes("stop-app")) {
        assert.equal(saved, true, "cannot kill an unacknowledged write");
        if (stopError) throw new Error(stopError);
        running = false; events.push("stop");
      } else if (args.includes("start")) {
        assert.ok(stoppedReads >= 3, "wait for confirmed OS teardown before starting");
        assert.equal(JSON.parse(settings).cloud.test_next_client, true);
        running = true; events.push("start");
      }
      return "";
    },
  });
  const driver = new AndroidSemanticJourneyDriver("modeled-device", {
    reloadApp: ({ whileStopped } = {}) => restartAndroidAfterPersistence({
      readStartup: () => running ? { ready: "true", disclaimer_required: "false", session_revision: "10" } : null,
      readPersistence: () => running ? {
        schema_version: 1, owner: 7, process: "old-process", session_revision: 10, available: true,
        documents: { "aerobag.core.settings.v1": { revision: 2, saved_revision: saved ? 2 : 1, pending: !saved, error: saveError } },
      } : null,
      restart: () => launch("modeled-device", { clearUiPrefs: false, whileStopped }),
      waitForInteractive: () => { assert.equal(running, true); events.push("interactive"); },
      waitForObservation: () => { assert.deepEqual(events, []); saved = true; events.push("saved"); clock += 1000; },
      scheduler: { now: () => clock, setTimeout, clearTimeout },
    }),
  });
  const update = runInNewContext(`(${simulateCloudClientUpdate})`, {
    selectNextTestClient,
    execFileSync(_command, args, options) {
      // This branch also deliberately reproduces the former caller-side stop.
      if (args.includes("force-stop")) { running = false; events.push("premature-stop"); return ""; }
      assert.equal(running, false, "must never edit a running owner's settings");
      assert.ok(stoppedReads >= 3, "must prove OS teardown before the offline edit");
      if (args.includes("exec-out")) { events.push("read"); return settings; }
      if (updateError) throw new Error(updateError);
      settings = options.input; events.push("write"); return "";
    },
  });
  return { run: () => update(driver), events, settings: () => JSON.parse(settings) };
}

test("Android client update waits for saves, confirms stop, edits offline, then starts once", async () => {
  const model = androidUpdateModel();
  await model.run();
  assert.deepEqual(model.events, ["saved", "stop", "read", "write", "start", "interactive"]);
  assert.deepEqual(model.settings().cloud, {
    account: { encrypted: "unchanged" }, records: { pending_keys: ["flight_plan/current"] }, test_next_client: true,
  });
});

for (const [problem, events] of [
  ["saveError", []], ["stopError", ["saved"]], ["updateError", ["saved", "stop", "read"]],
]) {
  test(`Android client update does not proceed or retry after ${problem}`, async () => {
    const model = androidUpdateModel({ [problem]: "injected failure" });
    await assert.rejects(model.run(), /injected failure/);
    assert.deepEqual(model.events, events);
    assert.equal(model.settings().cloud.test_next_client, false);
  });
}

test("offline updates cannot silently bypass the managed Android restart", async () => {
  const driver = new AndroidSemanticJourneyDriver("modeled-device", {});
  await assert.rejects(driver.reload({ whileStopped: () => assert.fail("must not mutate") }), /managed restart/);
});
