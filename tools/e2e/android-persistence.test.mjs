// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import test from "node:test";
import { persistenceReceiptFromNode, restartAndroidAfterPersistence } from "./android-persistence.mjs";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { AndroidSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { androidInteractiveRuntime, waitForAndroidInteractiveRuntime } from "./android-harness.mjs";
import { E2E_TIMING, observeUntil, ObservationTimeoutError, TransientObservationError } from "./transition-contract.mjs";

function receipt({ revision = 2, saved = 1, session = 10 } = {}) {
  return { schema_version: 1, owner: 7, process: "process-a", session_revision: session, available: true,
    blocked_reason: null, documents: {
      "aerobag.core.settings.v1": { revision, saved_revision: saved, pending: saved !== revision, error: null },
    } };
}

function model(t, step = () => {}) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-persistence-"));
  t.after(() => rmSync(artifactDir, { force: true, recursive: true }));
  const state = { receipt: receipt(), stops: 0, starts: 0, waits: 0, clock: 0, startupRevision: 10 };
  const restart = () => restartAndroidAfterPersistence({
    readStartup: async () => ({ ready: "true", disclaimer_required: "false", session_revision: String(state.startupRevision) }),
    readPersistence: async () => { if (state.error) throw state.error; return state.receipt; },
    restart: async () => { state.stops++; },
    waitForInteractive: async options => {
      assert.equal(options.requirePreservedDisclaimer, true);
      state.starts++;
    },
    scheduler: { now: () => state.clock, setTimeout, clearTimeout },
    waitForObservation: async () => { state.waits++; state.clock += 1000; await step(state); },
  });
  const driver = new AndroidSemanticJourneyDriver("modeled-device", { resetApp: restart, reloadApp: restart });
  const runtime = createJourneyRuntime({ journey: { id: "test.persisted-restart", assertions: [] },
    platform: "android", driver, artifactDir });
  return { state, runtime };
}

for (const operation of ["reset", "reload"]) {
  test(`${operation} cannot kill the app while the accepted settings revision is pending`, async t => {
    const { state, runtime } = model(t, state => {
      assert.equal(state.stops, 0, "must not kill the only owner of an unsaved edit");
      if (state.waits === 1) state.receipt = receipt({ revision: 3, saved: 2 });
      else state.receipt = receipt({ revision: 3, saved: 3 });
    });
    await runtime[operation]();
    assert.equal(state.waits, 2, "an older write's completion cannot acknowledge a newer edit");
    assert.equal(state.stops, 1);
    assert.equal(state.starts, 1);
  });
}

test("an old saved projection cannot authorize restarting a newer rendered revision", async t => {
  const { state, runtime } = model(t, state => {
    assert.equal(state.stops, 0);
    state.receipt = receipt({ saved: 2 });
  });
  state.receipt = receipt({ saved: 2, session: 9 });
  await runtime.reset();
  assert.equal(state.waits, 1);
});

test("unavailable and absent observations cannot stand in for durable completion", async t => {
  const { state, runtime } = model(t, state => {
    assert.equal(state.stops, 0);
    state.error = null;
    state.receipt = state.waits === 1 ? null : receipt({ saved: 2 });
  });
  state.error = new TransientObservationError("provider busy");
  await runtime.reload();
  assert.equal(state.waits, 2);
});

test("a permanently pending save fails with its receipt and never stops the app", async t => {
  const { state, runtime } = model(t);
  await assert.rejects(runtime.reset(), error => {
    assert.ok(error instanceof ObservationTimeoutError);
    assert.equal(error.diagnostics.last_value.persistence.documents["aerobag.core.settings.v1"].saved_revision, 1);
    return true;
  });
  assert.equal(state.stops, 0);
});

for (const problem of ["write failure", "blocked read", "missing storage", "missing schema", "transport failure", "owner replacement", "process replacement"]) {
  test(`${problem} fails without a restart or hidden repair`, async t => {
    const { state, runtime } = model(t, state => {
      if (problem === "process replacement") state.receipt.process = "process-b";
      else state.receipt.owner++;
    });
    if (problem === "write failure") state.receipt.documents["aerobag.core.settings.v1"].error = "disk full";
    if (problem === "blocked read") state.receipt.blocked_reason = "corrupt settings";
    if (problem === "missing storage") state.receipt.available = false;
    if (problem === "missing schema") delete state.receipt.schema_version;
    if (problem === "transport failure") state.error = new Error("provider crashed");
    await assert.rejects(runtime.reload(), /disk full|corrupt settings|invalid\/unavailable|provider crashed|owner changed/);
    assert.equal(state.stops, 0);
  });
}

test("receipt decoding requires the indexed publisher's process identity", () => {
  assert.equal(persistenceReceiptFromNode(null), null);
  assert.throws(() => persistenceReceiptFromNode({ "state-description": "{}" }), /process identity/);
  const node = { "state-description": JSON.stringify(receipt()), "semantic-path": "projection-provider:incarnation:22" };
  assert.equal(persistenceReceiptFromNode(node).process, "incarnation");
});

test("saved acceptance lost on restart remains an immediate application failure", async () => {
  const wait = runInNewContext(`(${waitForAndroidInteractiveRuntime})`, {
    observeUntil, E2E_TIMING, androidInteractiveRuntime,
    queryAndroidStartupState: () => ({ ready: "true", disclaimer_required: "true" }),
    queryAndroidExactProjection: () => assert.fail("must diagnose before querying the covered HOME control"),
    waitForAndroidSemanticEvent: () => assert.fail("must fail immediately, not wait sixty seconds"),
  });
  await assert.rejects(wait("device", 60_000, { requirePreservedDisclaimer: true }), /durably saved disclaimer acceptance was lost/);
});

test("real Android reset and reload wiring shares the durable restart boundary", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const helper = source.slice(source.indexOf("async function launchInteractiveAndroidApp"), source.indexOf("async function ensurePlanPage"));
  assert.match(helper, /restartAndroidAfterPersistence/);
  assert.match(helper, /queryAndroidExactProjection\(serial, LOCAL_PERSISTENCE_PROJECTION\)/);
  assert.match(source, /resetApp: \(\) => launchInteractiveAndroidApp/);
  assert.match(source, /reloadApp: \(\) => launchInteractiveAndroidApp/);
});
