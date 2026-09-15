// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { acceptDisclaimer, readStartupState } from "./first-use-startup.mjs";
import { AndroidSemanticJourneyDriver } from "./semantic-journey-driver.mjs";

function startupModel({ disclaimer = true, tour = false, pending = true, missing = false } = {}) {
  const actions = [];
  let settleReads = 0;
  const runtime = {
    platform: "web",
    driver: {
      async readProjection() {
        if (missing) return [];
        return [{ id: `parity:startup-state:ready:true:disclaimer_required:${disclaimer}:tour_pending:${pending}` }];
      },
      async readElement(id) {
        if (id === "guided-tour-panel") return tour ? { id } : null;
        return null;
      },
    },
    async eventually(description, read) {
      // Explicitly step through pending -> temporarily missing -> offered.
      // No clocks, sleeps, browser, or test retries are involved.
      if (description === "first-use introduction settled" && pending) {
        assert.equal(await read(), null);
        missing = true;
        assert.equal(await read(), null);
        missing = false;
        pending = false;
        tour = true;
        settleReads++;
      }
      const value = await read();
      assert.ok(value, description);
      return value;
    },
    async action(_description, id, { complete }) {
      actions.push(id);
      if (id === "disclaimer-accept-button") disclaimer = false;
      else if (id === "guided-tour-close") tour = false;
      else assert.fail(`unexpected action ${id}`);
      assert.ok(await complete());
    },
    async openPage(page) {
      assert.equal(tour, false, "must not navigate behind the modal");
      actions.push(`page:${page}`);
    },
  };
  return { runtime, actions, get settleReads() { return settleReads; } };
}

test("fresh clients settle a delayed introduction before navigating", async () => {
  const model = startupModel();
  assert.equal(await acceptDisclaimer(model.runtime, { required: true }), true);
  assert.equal(model.settleReads, 1);
  assert.deepEqual(model.actions, ["disclaimer-accept-button", "guided-tour-close", "page:map"]);
});

test("already accepted profiles still settle an outstanding introduction", async () => {
  const model = startupModel({ disclaimer: false });
  assert.equal(await acceptDisclaimer(model.runtime), false);
  assert.deepEqual(model.actions, ["guided-tour-close", "page:map"]);
});

test("settled existing profiles do not replay navigation or acceptance", async () => {
  const model = startupModel({ disclaimer: false, pending: false });
  assert.equal(await acceptDisclaimer(model.runtime), false);
  assert.deepEqual(model.actions, []);
});

test("tour journeys preserve the introduction for their own assertions", async () => {
  const model = startupModel();
  assert.equal(await acceptDisclaimer(model.runtime, { keepIntroduction: true }), true);
  assert.equal(model.settleReads, 0);
  assert.deepEqual(model.actions, ["disclaimer-accept-button"]);
});

test("required disclaimer cannot silently pass on a reused profile", async () => {
  const model = startupModel({ disclaimer: false });
  await assert.rejects(acceptDisclaimer(model.runtime, { required: true }), /without presenting the disclaimer/);
  assert.deepEqual(model.actions, []);
});

test("startup failures stay terminal", async () => {
  const model = startupModel();
  model.runtime.driver.readElement = async () => ({ text: "WASM failed" });
  await assert.rejects(readStartupState(model.runtime), /application startup failed: WASM failed/);
  assert.deepEqual(model.actions, []);
});

test("Android shared startup reads the indexed state even with no visible hierarchy tag", async () => {
  const driver = new AndroidSemanticJourneyDriver("no-device-needed", {});
  let reads = 0;
  driver.readStartupProjection = () => {
    reads++;
    return { ready: "true", disclaimer_required: "false", tour_pending: "false", page: "Home" };
  };
  // Neither a UI hierarchy request nor a mutation is allowed by this driver.
  driver.readElement = async () => assert.fail("startup is a read-only indexed projection");
  const state = await readStartupState({ platform: "android", driver });
  assert.equal(state.page, "Home");
  assert.equal(reads, 1);
  for (const unavailable of [null, {}, { ready: "false" }]) {
    driver.readStartupProjection = () => unavailable;
    assert.equal(await readStartupState({ platform: "android", driver }), null);
  }
});
