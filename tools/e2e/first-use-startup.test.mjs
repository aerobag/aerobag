// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { androidObservationBackend, rejectObservationOverrides } from "./android-observation-contract.mjs";
import { acceptDisclaimer, readGuidedTourPanel, readStartupState } from "./first-use-startup.mjs";
import { AndroidSemanticJourneyDriver, androidElementSemanticTag } from "./semantic-journey-driver.mjs";
import { acceptDisclaimerIfPresent, androidIndexedControlIsActionReady, queryAndroidExactProjection } from "./android-harness.mjs";
import {
  E2E_TIMING, ObservationTimeoutError, TerminalObservationError,
  TransientObservationError, observeUntil, performTransition,
} from "./transition-contract.mjs";

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
      async readElement(id, options) {
        if (id === "guided-tour-panel") {
          assert.equal(options, undefined, "observation ownership does not belong to the journey");
          return tour ? { id } : null;
        }
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

// Execute the production lookup and wire encoder with only the device I/O
// replaced. A mock readElement returning null cannot catch a lost provider_only
// flag (the bug that passed locally and stalled four hosted jobs).
function indexedDeviceContext(probe) {
  const context = {
    androidObservationBackend, rejectObservationOverrides,
    URLSearchParams, TransientObservationError, androidElementSemanticTag,
    requiredSemanticDriver: () => ({ port: 19191 }),
    semanticDriverObservationUnavailable: response => response.status === 28,
    semanticDriverObservationRequest(_port, path) {
      const url = new URL(path, "http://device");
      assert.equal(url.pathname, "/exact-projection");
      assert.equal(url.searchParams.get("provider_only"), "true",
        "accessibility traversal is unavailable; use the app-owned index even for absence");
      return probe(url.searchParams.get("tag"));
    },
    queryAndroidSemanticNodes: () => assert.fail("no legacy/prefix hierarchy fallback"),
    androidProjectedElement: node => node,
  };
  context.queryAndroidExactProjection = runInNewContext(`(${queryAndroidExactProjection})`, context);
  const driverSource = readFileSync(new URL("semantic-journey-driver.mjs", import.meta.url), "utf8");
  const lookup = driverSource.slice(
    driverSource.indexOf("function queryFirstAndroidSemanticNode("),
    driverSource.indexOf("function readinessEvidenceMatchesTag("),
  );
  context.queryFirstAndroidSemanticNode = runInNewContext(`(${lookup.trim()})`, context);
  context.readElement = runInNewContext(
    `({ ${AndroidSemanticJourneyDriver.prototype.readElement} }).readElement`, context,
  );
  return context;
}

test("indexed panel observation proves never-mounted and unmounted absence without a tree", async () => {
  let node = null;
  let unavailable = false;
  let reads = 0;
  const context = indexedDeviceContext(tag => {
    assert.equal(tag, "parity:guided-tour-panel");
    reads++;
    return unavailable
      ? { status: 28, stdout: "", stderr: "provider IPC busy" }
      : { status: 0, stdout: JSON.stringify(node ? [node] : []) };
  });
  const runtime = { driver: { readElement: context.readElement } };
  assert.equal(await readGuidedTourPanel(runtime), null, "never mounted in this process");
  node = { visible: "true", text: "Welcome" };
  assert.equal((await readGuidedTourPanel(runtime)).text, "Welcome");
  node = null;
  assert.equal(await readGuidedTourPanel(runtime), null, "disposed after closing");
  unavailable = true;
  await assert.rejects(readGuidedTourPanel(runtime), TransientObservationError);
  assert.equal(reads, 4, "one request per observation; no hidden retries or fallback");
  // Direct readers and startup helpers must use the same indexed path.
  unavailable = false;
  assert.equal(await context.readElement("guided-tour-panel"), null);
  assert.equal(reads, 5);
});

for (const initiallyPresent of [false, true]) {
  test(`startup observes busy then ${initiallyPresent ? "present" : "absent"} panel before acting`, async () => {
    const model = startupModel({ disclaimer: false, pending: false, tour: initiallyPresent });
    const readElement = model.runtime.driver.readElement;
    let reads = 0;
    model.runtime.driver.readElement = async (...args) => {
      if (args[0] === "guided-tour-panel" && ++reads === 1) {
        throw new TransientObservationError("provider IPC busy");
      }
      return readElement(...args);
    };
    model.runtime.eventually = async (description, probe) =>
      (await observeUntil(description, probe, { timeoutMs: 100, intervalMs: 1 })).value;
    await acceptDisclaimer(model.runtime);
    assert.ok(reads >= 2);
    assert.deepEqual(model.actions, initiallyPresent ? ["guided-tour-close", "page:map"] : []);
  });
}

for (const transient of [true, false]) {
  test(`startup never treats ${transient ? "persistent unavailability" : "terminal failure"} as absence`, async () => {
    const model = startupModel({ disclaimer: false, pending: false });
    model.runtime.platform = "android";
    model.runtime.driver.readElement = async () => {
      throw transient ? new TransientObservationError("provider IPC busy") : new TerminalObservationError("provider", "crashed");
    };
    model.runtime.eventually = async (description, probe) =>
      (await observeUntil(description, probe, { timeoutMs: 40, intervalMs: 1 })).value;
    await assert.rejects(acceptDisclaimer(model.runtime), transient ? ObservationTimeoutError : TerminalObservationError);
    assert.deepEqual(model.actions, []);
  });
}

test("native bootstrap observes a physically closed tour without any accessibility-tree request", async () => {
  let disclaimer = true;
  let tour = true;
  let page = "Map";
  const taps = [];
  const state = () => ({ disclaimer_required: String(disclaimer), tour_pending: "false", page });
  const context = indexedDeviceContext(tag => {
    const present = tag !== "parity:guided-tour-panel" || tour;
    return { status: 0, stdout: JSON.stringify(present ? [{
      "resource-id": tag, visible: "true", enabled: "true", bounds: "[10,20][30,40]",
      "center-reachable": "true", "semantic-path": "projection-provider:12",
    }] : []) };
  });
  Object.assign(context, {
    E2E_TIMING, observeUntil, performTransition,
    queryAndroidStartupProjection: state, queryAndroidStartupState: state,
    androidIndexedControlIsActionReady,
    activateAndroidNode(_serial, node) {
      const tag = node["resource-id"];
      taps.push(tag);
      if (tag === "parity:disclaimer-accept-button") disclaimer = false;
      else if (tag === "parity:guided-tour-close") { tour = false; page = "Home"; }
      else if (tag === "parity:home-button:Chart") page = "Map";
      else assert.fail(`unexpected tap ${tag}`);
    },
  });
  const accept = runInNewContext(`(${acceptDisclaimerIfPresent})`, context);
  assert.equal(await accept("test-device"), true);
  assert.deepEqual(taps, ["parity:disclaimer-accept-button", "parity:guided-tour-close", "parity:home-button:Chart"]);
  assert.equal(page, "Map");
});
