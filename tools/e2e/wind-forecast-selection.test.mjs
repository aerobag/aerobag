// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { chooseForecastWindModel } from "./release-journey-implementations.mjs";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { E2E_TIMING, observeUntil, ObservationTimeoutError } from "./transition-contract.mjs";

function forecastModel(t, platform, {
  beforeAction = "calculation", feedback = "FETCHING MODEL", outcome = "selected",
} = {}) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-wind-forecast-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const actionId = (row) => `altitude-planner-wind-action${platform === "web" ? "-" : ":"}${row}`;
  const actions = [];
  let inputAttempts = 0;
  let actionReady = false;
  let requested = false;
  let ready = false;
  let selected = false;
  let clock = 0;
  const scheduler = { now: () => clock, setTimeout, clearTimeout };
  const waitForObservation = async () => {
    clock += 1000;
    if (requested && outcome !== "no-download") {
      if (ready && outcome !== "not-selected") selected = true;
      ready = true;
    }
  };
  const driver = {
    platform,
    async readSessionRevision() { return 1; },
    async readElement(id) {
      if (id === "altitude-comparison-loading") return null;
      if (id === actionId("no_wind")) return { enabled: true, selected: !selected };
      if (id === actionId("ready_forecast")) {
        return ready ? { enabled: true, selected } : null;
      }
      assert.equal(id, actionId("latest_forecast"));
      if (requested) return { enabled: false, text: feedback };
      if (actionReady && beforeAction === "error") throw new Error("publisher unavailable");
      return {
        enabled: !(actionReady && beforeAction === "calculation"),
        text: actionReady && beforeAction === "metadata" ? "FETCH MODEL · 12.2 MIB" : "FETCH MODEL · 12.1 MIB",
      };
    },
    async readAction(id) {
      const value = await this.readElement(id);
      actionReady = true;
      return value;
    },
    async performAction(id) {
      assert.equal(id, actionId("latest_forecast"), "fetching selects the model without another click");
      inputAttempts += 1;
      if (outcome === "input-rejected") throw new Error("forecast control disabled at input");
      // The calculation finishes before delivery in the successful model.
      // A still-disabled target must fail at the driver's input boundary.
      actions.push(id);
      requested = true;
      if (feedback === "ready") { ready = true; selected = true; }
    },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.wind-forecast", assertions: [] }, platform, driver, artifactDir,
  });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract, scheduler, waitForObservation,
  });
  // Keep production observations and deadlines, advancing only their clock.
  runtime.eventually = async (description, read, timeoutMs = E2E_TIMING.localReadyMs) =>
    (await observeUntil(description, read, { timeoutMs, scheduler, waitForNextProbe: waitForObservation })).value;
  return { runtime, actions, actionId, get inputAttempts() { return inputAttempts; } };
}

for (const platform of ["web", "android"]) {
  for (const beforeAction of ["calculation", "metadata"]) {
    test(`${platform}: ${beforeAction} changes before input are not forecast request completion`, async (t) => {
      const model = forecastModel(t, platform, { beforeAction });
      const result = await chooseForecastWindModel(model.runtime);
      assert.equal(result.downloaded, true);
      assert.equal(result.selected.selected, true);
      assert.deepEqual(model.actions, [model.actionId("latest_forecast")]);
    });
  }
  for (const feedback of ["RETRY FETCH", "FETCHING MODEL", "INSTALLING MODEL", "ready"]) {
    test(`${platform}: ${feedback} acknowledges the request and still requires selection`, async (t) => {
      const model = forecastModel(t, platform, { beforeAction: "none", feedback });
      const result = await chooseForecastWindModel(model.runtime);
      assert.equal(result.selected.selected, true);
      assert.deepEqual(model.actions, [model.actionId("latest_forecast")]);
    });
  }
  for (const feedback of ["FETCH MODEL · 12.1 MIB", "FETCH MODEL · 12.2 MIB"]) {
    test(`${platform}: disabled ${feedback} after a lost click is not request feedback`, async (t) => {
      const model = forecastModel(t, platform, { beforeAction: "none", feedback, outcome: "no-download" });
      await assert.rejects(chooseForecastWindModel(model.runtime), (error) => {
        assert.ok(error instanceof ObservationTimeoutError, error.message);
        assert.match(error.message, /request latest wind forecast completed/);
        assert.equal(error.diagnostics.last_value.latest.text, feedback);
        assert.equal(error.diagnostics.last_value.latest.enabled, false);
        return true;
      });
      assert.equal(model.actions.length, 1);
    });
  }
  for (const outcome of ["no-download", "not-selected"]) {
    test(`${platform}: acknowledged request still rejects ${outcome}`, async (t) => {
      const model = forecastModel(t, platform, { beforeAction: "none", outcome });
      await assert.rejects(chooseForecastWindModel(model.runtime), ObservationTimeoutError);
      assert.equal(model.actions.length, 1);
    });
  }
  test(`${platform}: a failed pre-action observation forbids input`, async (t) => {
    const model = forecastModel(t, platform, { beforeAction: "error" });
    await assert.rejects(chooseForecastWindModel(model.runtime), /publisher unavailable/);
    assert.deepEqual(model.actions, []);
  });
  test(`${platform}: input rejection remains terminal without retrying`, async (t) => {
    const model = forecastModel(t, platform, { outcome: "input-rejected" });
    await assert.rejects(chooseForecastWindModel(model.runtime), /forecast control disabled at input/);
    assert.equal(model.inputAttempts, 1);
    assert.deepEqual(model.actions, []);
  });
}
