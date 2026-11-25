// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { activateNextPlanLeg, planAction } from "./release-journey-implementations.mjs";
import { ObservationTimeoutError } from "./transition-contract.mjs";

const uid = label => `component:fpc:${{ KRNT: "0002", KPAE: "0003", S88: "0006", KPLU: "0007" }[label]}:Waypoint:waypoint`;

function navigationModel(t, platform, { initial = "late", activation = "closure-first", next = "correct" } = {}) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-plan-navigation-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const rows = Object.fromEntries(["KRNT", "KPAE", "S88", "KPLU"].map(label => [label, {
    id: `parity:plan-row:${uid(label)}`, text: label, enabled: true,
  }]));
  let guidance = initial === "late" ? { fromUid: uid("S88"), toUid: uid("KPLU") }
    : initial === "next" ? { fromUid: uid("KPAE"), toUid: uid("S88") }
      : { fromUid: uid("KRNT"), toUid: uid("KPAE") };
  let open = false;
  let active = "none";
  let pending = null;
  let clock = 0;
  const actions = [];
  const target = { fromUid: uid("KRNT"), toUid: uid("KPAE") };
  const driver = {
    platform,
    async readSessionRevision() { return 1; },
    async revealProjectionMatching(prefix, label) { assert.equal(prefix, "parity:plan-row:"); return rows[label]; },
    async findProjectionMatching(prefix, label) { assert.equal(prefix, "parity:plan-row:"); return rows[label]; },
    async readElement(id) {
      assert.equal(id, "plan-row-tray-scrim");
      return open ? { id, enabled: true } : null;
    },
    async readProjection(prefix) {
      if (prefix === "parity:plan-row-action:") return open ? [{ id: "parity:plan-row-action:activate_leg" }] : [];
      assert.equal(prefix, "parity:plan-state:");
      return guidance ? [{ id: `parity:plan-state:rows:5:active:${active}:from:${guidance.fromUid}:to:${guidance.toUid}` }] : [];
    },
    async readAction(id) {
      if (id === "activate_next_leg") {
        // Reproduce the hosted ordering: the previous row action's rendered
        // guidance arrives between NEXT's baseline read and action readiness.
        pending?.();
        pending = null;
        if (next === "unrelated-readiness-change") active = "highlight-changed";
      }
      return { id, enabled: true, bounds: "[0,0][100,100]" };
    },
    async performAction(id) {
      actions.push(id);
      if (id.startsWith("plan-row:")) { open = true; return; }
      if (id === "plan-row-action:activate_leg") {
        if (activation === "lost-click") return;
        if (activation === "guidance-first") {
          guidance = target;
          pending = () => { open = false; };
        } else if (activation === "still-open") {
          guidance = target;
        } else {
          open = false;
          if (activation === "closure-first") pending = () => { guidance = target; };
          if (activation === "wrong-leg") guidance = { ...target, toUid: `${uid("KPAE")}-other` };
          if (activation === "missing-guidance") guidance = null;
        }
        return;
      }
      assert.equal(id, "activate_next_leg");
      if (next === "lost-click") return;
      if (next === "highlight-only") { active = "highlight-changed"; return; }
      if (next === "missing-guidance") { guidance = null; return; }
      guidance = { fromUid: uid("KPAE"), toUid: uid(next === "wrong-leg" ? "KPLU" : "S88") };
    },
  };
  const runtime = createJourneyRuntime({ journey: { id: "test.plan-navigation", assertions: [] }, platform, driver, artifactDir });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract,
    scheduler: { now: () => clock, setTimeout, clearTimeout },
    waitForObservation: async () => { clock += 50; pending?.(); pending = null; },
  });
  return { runtime, actions, get guidance() { return guidance; }, get open() { return open; } };
}

for (const platform of ["android", "web"]) {
  for (const activation of ["closure-first", "guidance-first"]) {
    test(`${platform}: activation waits for both menu closure and the requested leg (${activation})`, async t => {
      const model = navigationModel(t, platform, { activation });
      await planAction(model.runtime, "KPAE", "activate_leg");
      assert.equal(model.open, false);
      assert.deepEqual(model.guidance, { fromUid: uid("KRNT"), toUid: uid("KPAE") });
      assert.deepEqual(model.actions, [`plan-row:${uid("KPAE")}`, "plan-row-action:activate_leg"]);
    });
  }
  test(`${platform}: delayed activation cannot masquerade as NEXT completing before its click`, async t => {
    const model = navigationModel(t, platform);
    await planAction(model.runtime, "KPAE", "activate_leg");
    await activateNextPlanLeg(model.runtime, "KPAE", "S88");
    assert.deepEqual(model.guidance, { fromUid: uid("KPAE"), toUid: uid("S88") });
    assert.deepEqual(model.actions, [`plan-row:${uid("KPAE")}`, "plan-row-action:activate_leg", "activate_next_leg"]);
  });
  test(`${platform}: selecting an already active leg still performs the menu interaction`, async t => {
    const model = navigationModel(t, platform, { initial: "target" });
    await planAction(model.runtime, "KPAE", "activate_leg");
    assert.equal(model.open, false);
    assert.deepEqual(model.actions, [`plan-row:${uid("KPAE")}`, "plan-row-action:activate_leg"]);
  });
  for (const activation of ["lost-click", "still-open", "wrong-leg", "missing-guidance"]) {
    test(`${platform}: activation rejects ${activation} without another click`, async t => {
      const model = navigationModel(t, platform, { activation });
      await assert.rejects(planAction(model.runtime, "KPAE", "activate_leg"), error => {
        assert.ok(error instanceof ObservationTimeoutError, error.message);
        assert.equal(error.diagnostics.last_value.open, activation === "lost-click" || activation === "still-open");
        if (activation === "wrong-leg") assert.equal(error.diagnostics.last_value.guidance.toUid, `${uid("KPAE")}-other`);
        if (activation === "missing-guidance") assert.equal(error.diagnostics.last_value.guidance, null);
        return true;
      });
      assert.deepEqual(model.actions, [`plan-row:${uid("KPAE")}`, "plan-row-action:activate_leg"]);
    });
  }
  for (const next of ["correct", "unrelated-readiness-change"]) {
    test(`${platform}: NEXT observes exact guidance despite ${next}`, async t => {
      const model = navigationModel(t, platform, { initial: "target", next });
      await activateNextPlanLeg(model.runtime, "KPAE", "S88");
      assert.deepEqual(model.guidance, { fromUid: uid("KPAE"), toUid: uid("S88") });
      assert.deepEqual(model.actions, ["activate_next_leg"]);
    });
  }
  for (const next of ["lost-click", "highlight-only", "wrong-leg", "missing-guidance"]) {
    test(`${platform}: NEXT rejects ${next} without another click`, async t => {
      const model = navigationModel(t, platform, { initial: "target", next });
      await assert.rejects(activateNextPlanLeg(model.runtime, "KPAE", "S88"), error => {
        assert.ok(error instanceof ObservationTimeoutError, error.message);
        if (next === "wrong-leg") assert.equal(error.diagnostics.last_value.toUid, uid("KPLU"));
        return true;
      });
      assert.deepEqual(model.actions, ["activate_next_leg"]);
    });
  }
  test(`${platform}: NEXT refuses an already completed leg before sending input`, async t => {
    const model = navigationModel(t, platform, { initial: "next" });
    await assert.rejects(activateNextPlanLeg(model.runtime, "KPAE", "S88"), /already satisfied before the action/);
    assert.deepEqual(model.actions, []);
  });
}
