// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { releaseJourneyImplementation } from "./release-journey-implementations.mjs";
import { journeyById } from "./release-journey-registry.mjs";
import { androidActionCandidates, androidElementSemanticTag } from "./semantic-journey-driver.mjs";

const journeyId = "shared.flight-plan-find-route";

// Exercise the real journey's observations against a small visible UI model.
// Each defect below must fail its completion/assertion, not get recorded green.
function modelRuntime(platform, defect = null) {
  let rows = [], original = [], input = "", page = "flight_plan", tray = false;
  let draft = false, mode = "gnss", previous = null, future = null;
  const checks = [], actions = [];
  const row = (label) => rows.includes(label) ? { id: `parity:plan-row:${label}`, text: label } : null;
  const driver = {
    async readElement(id) {
      if (id === "plan-row-tray-scrim") return tray ? { enabled: true } : null;
      if (id === "plan-append-route-input") return { enabled: true, focused: true };
      if (id.startsWith("plan-row:")) return row(id.slice("plan-row:".length));
      if (id === "plan-row-action:find_route") return tray ? { enabled: true } : null;
      if (id === "airway-routing-summary") {
        return draft && page === "map" ? { text: "Direct: 42 nm Airway: 45 nm +7%" } : null;
      }
      if (id.startsWith("airway-routing-control-")) {
        if (!draft || page !== "map") return null;
        const name = id.slice("airway-routing-control-".length);
        const selected = name === mode;
        return { enabled: name === "undo" ? previous !== null : name === "redo" ? future !== null : true,
          ...(platform === "web" ? { pressed: String(selected) } : { selected }) };
      }
      if (id === "undo") return { enabled: rows.length > original.length };
      return null;
    },
    async readProjection(prefix) {
      if (prefix === "parity:startup-state:") return [{ id: `${prefix}ready:true:disclaimer_required:false` }];
      if (prefix === "parity:plan-append-route-state:") return [{ id: `${prefix}can_commit:true:loading:false` }];
      if (prefix === "parity:flight-plan-rows:") return [{ id: `${prefix}${rows.join("\u001f")}` }];
      if (prefix === "parity:plan-row:") return rows.map(row);
      throw Error(`unexpected projection ${prefix}`);
    },
    async findProjectionMatching(_prefix, label) { return row(label); },
    async submit() { rows = input.split(" "); original = [...rows]; },
    async back() { tray = false; },
  };
  const runtime = {
    platform, driver, checks, actions,
    capability: () => ({ entry: "MEDEA", exit: "YKM", airway: "V4" }),
    async reset() {},
    async openPage(id) { page = id; },
    async editText(_description, _id, value) { input = value; },
    async revealElement(id) { return driver.readElement(id); },
    async revealProjectionMatching(_prefix, label) { return row(label); },
    async eventually(description, observe) {
      const value = await observe();
      assert.ok(value, `${description} did not complete`);
      return value;
    },
    async transition(description, contract) {
      const ready = await contract.ready();
      assert.ok(ready, `${description} is not ready`);
      await contract.act(ready);
      return this.eventually(description, contract.complete);
    },
    async action(description, id, contract) {
      actions.push(id);
      const ready = await driver.readElement(id);
      assert.ok(ready && ready.enabled !== false, `${id} is not ready`);
      if (id.startsWith("plan-row:")) tray = true;
      else if (id === "plan-row-action:find_route") {
        draft = true; page = "map"; tray = false; previous = null; future = null;
        if (defect === "premature-apply") rows.splice(1, 0, "V4");
      } else if (id === "airway-routing-control-remove") draft = false;
      else if (id === "airway-routing-control-apply_route") {
        draft = false;
        if (defect !== "missing-apply") rows.splice(1, 0, "V4");
      } else if (id === "airway-routing-control-undo") {
        future = mode; mode = previous; previous = null;
      } else if (id === "airway-routing-control-redo") {
        previous = mode; mode = future; future = null;
      } else if (id === "undo") {
        if (defect !== "missing-plan-undo") rows = [...original];
      } else if (id.startsWith("airway-routing-control-")) {
        previous = mode;
        if (defect !== "unchanged-mode") mode = id.slice("airway-routing-control-".length);
      } else throw Error(`unexpected action ${id}`);
      return this.eventually(description, contract.complete);
    },
    check(id, pass) { assert.ok(pass, `${id} failed`); checks.push(id); },
  };
  return runtime;
}

for (const platform of ["web", "android"]) {
  test(`Find Route journey observes draft, mode/history, cancel, apply and plan Undo on ${platform}`, async () => {
    const runtime = modelRuntime(platform);
    await releaseJourneyImplementation(journeyId)(runtime);
    assert.deepEqual(runtime.checks, journeyById(journeyId).assertions);
    assert.equal(runtime.actions.filter((id) => id === "plan-row-action:find_route").length, 2);
  });
}

for (const [defect, expected] of [
  ["premature-apply", /plan.find-route-draft failed/],
  ["missing-apply", /applied airway row did not complete/],
  ["missing-plan-undo", /undo applied airway route did not complete/],
  ["unchanged-mode", /change route navigation mode did not complete/],
]) {
  test(`Find Route journey rejects ${defect}`, async () => {
    await assert.rejects(releaseJourneyImplementation(journeyId)(modelRuntime("web", defect)), expected);
  });
}

test("Find Route uses an exact Android flight-plan action alias", () => {
  assert.deepEqual(androidActionCandidates("plan-row-action:find_route"), ["parity:plan-row-action:find_route"]);
  assert.equal(androidElementSemanticTag("airway-routing-summary"), "parity:airway-routing-summary");
  assert.equal(androidElementSemanticTag("airway-routing-control-vor"), "parity:airway-routing-control-vor");
});
