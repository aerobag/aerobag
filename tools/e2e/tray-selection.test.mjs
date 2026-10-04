// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import {
  chooseDifferentAltitudeOption, selectTrayOptionMatching,
} from "./release-journey-implementations.mjs";
import { ObservationTimeoutError } from "./transition-contract.mjs";

function selectionModel(t, platform, launcherId, {
  selectionTiming = "on-click", outcome = "selected", initiallyOpen = false, controlId = null,
} = {}) {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-tray-selection-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const chart = launcherId === "plate-chart-button";
  const id = controlId ? "alternate" : chart ? "chart-reference:tac:legend:seattle-tac"
    : platform === "web" ? "airport:KPAE" : "KPAE";
  const label = controlId ? "TEST CHOICE" : chart ? "SEATTLE TAC LEGEND" : "KPAE";
  const launcherLabel = controlId
    ? `${controlId === "aircraft" ? "AIRCRAFT" : "PROFILE"}\n${label}` : label;
  const needle = chart ? "Seattle TAC" : label;
  const prefix = platform === "web" ? "tray-option-" : controlId
    ? `parity:altitude-planner-option:${controlId}:` : "parity:tray-option:";
  const tilePrefix = platform === "web" ? "plate-folder-tile:" : "parity:plate-folder-tile:";
  const entry = { id: `${prefix}${id}`, text: label, enabled: true, selected: false };
  const optionActionId = platform === "android" && controlId ? entry.id : `tray-option:${id}`;
  const actions = [];
  let open = initiallyOpen;
  let selected = selectionTiming === "already-selected";
  let displayedLabel = null;
  let pending = null;
  let clock = 0;
  const driver = {
    platform,
    async readSessionRevision() { return 1; },
    async readElement(target) {
      assert.equal(target, launcherId);
      return { id: launcherId, enabled: true, text: displayedLabel ?? (selected ? launcherLabel : "OLD SELECTION") };
    },
    async readAction(target) {
      if (target === launcherId) return this.readElement(target);
      assert.equal(target, optionActionId);
      assert.equal(open, true);
      if (selectionTiming === "during-action-readiness") selected = true;
      return { ...entry, selected, bounds: "[0,0][100,100]" };
    },
    async readProjection(request) {
      if (request === prefix) return open ? [entry] : [];
      if (request === tilePrefix) return [];
      assert.equal(request, "parity:plate-viewport:");
      return selected ? [{ id: `parity:plate-viewport:chart:${id}:zoom:1` }] : [];
    },
    async revealProjectionMatching(request, text) {
      assert.equal(request, prefix);
      assert.equal(text, needle);
      assert.equal(open, true);
      if (selectionTiming === "during-reveal") selected = true;
      return entry;
    },
    async revealElement(target) { return this.readElement(target); },
    async performAction(target) {
      actions.push(target);
      if (target === launcherId) {
        assert.equal(open, false, "opening an already-open picker would close it");
        open = true;
        if (selectionTiming === "during-open") selected = true;
        return;
      }
      assert.equal(target, optionActionId);
      if (outcome === "lost-click") return;
      if (outcome === "label-first") {
        selected = true;
        pending = () => { open = false; };
        return;
      }
      if (outcome === "closure-first") {
        open = false;
        pending = () => { selected = true; };
        return;
      }
      selected = true;
      if (outcome !== "still-open") open = false;
      if (outcome === "wrong-selection") displayedLabel = `${launcherLabel} OTHER DOCUMENT`;
      if (outcome === "whitespace") displayedLabel = launcherLabel.replace(/\s+/g, " \n  ");
    },
    async back() { actions.push("back"); open = false; },
  };
  const runtime = createJourneyRuntime({
    journey: { id: "test.tray-selection", assertions: [] }, platform, driver, artifactDir,
  });
  const transition = runtime.transition;
  runtime.transition = (description, contract) => transition(description, {
    ...contract,
    // Advance only the observation clock; no sleeps or runner-speed dependency.
    scheduler: { now: () => clock, setTimeout, clearTimeout },
    waitForObservation: async () => {
      clock += 50;
      pending?.();
      pending = null;
    },
  });
  return { runtime, actions, entry, optionActionId, needle, get open() { return open; } };
}

for (const platform of ["web", "android"]) {
  for (const launcherId of ["plate-chart-button", "plate-airport-button"]) {
    for (const selectionTiming of [
      "already-selected", "during-open", "during-reveal", "during-action-readiness", "on-click",
    ]) {
      test(`${platform} ${launcherId}: ${selectionTiming} still completes the picker interaction`, async (t) => {
        const model = selectionModel(t, platform, launcherId, { selectionTiming });
        const result = await selectTrayOptionMatching(model.runtime, launcherId, model.needle);
        assert.equal(result, model.entry, "retain the option identity, not the launcher");
        assert.equal(model.open, false);
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
    test(`${platform} ${launcherId}: reuse an open picker without toggling it closed`, async (t) => {
      const model = selectionModel(t, platform, launcherId, { initiallyOpen: true });
      await selectTrayOptionMatching(model.runtime, launcherId, model.needle);
      assert.deepEqual(model.actions, [model.optionActionId]);
    });
    for (const outcome of ["label-first", "closure-first", "whitespace"]) {
      test(`${platform} ${launcherId}: ${outcome} requires both selection and closure`, async (t) => {
        const model = selectionModel(t, platform, launcherId, { outcome });
        await selectTrayOptionMatching(model.runtime, launcherId, model.needle);
        assert.equal(model.open, false);
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
    for (const outcome of ["lost-click", "still-open", "wrong-selection"]) {
      test(`${platform} ${launcherId}: rejects ${outcome} without retrying the action`, async (t) => {
        const model = selectionModel(t, platform, launcherId, {
          selectionTiming: "during-action-readiness", outcome,
        });
        await assert.rejects(
          selectTrayOptionMatching(model.runtime, launcherId, model.needle),
          (error) => {
            assert.ok(error instanceof ObservationTimeoutError, error.message);
            assert.ok(error.diagnostics.last_value.launcher, "retain wrong-state diagnostics");
            assert.equal(error.diagnostics.last_value.options.length, outcome === "wrong-selection" ? 0 : 1);
            return true;
          },
        );
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
  }
}

for (const platform of ["web", "android"]) {
  for (const controlId of ["aircraft", "aircraft_profile"]) {
    const launcherId = `altitude-planner-control${platform === "web" ? "-" : ":"}${controlId}`;
    for (const selectionTiming of ["during-action-readiness", "on-click"]) {
      test(`${platform} ${controlId}: ${selectionTiming} still chooses and closes`, async (t) => {
        const model = selectionModel(t, platform, launcherId, { controlId, selectionTiming });
        const result = await chooseDifferentAltitudeOption(model.runtime, controlId);
        assert.equal(result.option, model.entry);
        assert.equal(model.open, false);
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
    for (const outcome of ["label-first", "closure-first", "whitespace"]) {
      test(`${platform} ${controlId}: ${outcome} requires both selection and closure`, async (t) => {
        const model = selectionModel(t, platform, launcherId, { controlId, outcome });
        await chooseDifferentAltitudeOption(model.runtime, controlId);
        assert.equal(model.open, false);
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
    for (const outcome of ["lost-click", "still-open", "wrong-selection"]) {
      test(`${platform} ${controlId}: rejects ${outcome} without retrying`, async (t) => {
        const model = selectionModel(t, platform, launcherId, {
          controlId, selectionTiming: "during-action-readiness", outcome,
        });
        await assert.rejects(chooseDifferentAltitudeOption(model.runtime, controlId), ObservationTimeoutError);
        assert.deepEqual(model.actions, [launcherId, model.optionActionId]);
      });
    }
  }
}
