// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  AndroidSemanticJourneyDriver, androidElementEnabled, androidElementSemanticTag,
  androidSemanticTag, androidDataStatusRowsFromStateTag, androidMapInspectionPoint,
  androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll, establishRevealedElement,
  androidPageTag,
  androidProjectionMayRequireVerticalScan,
  androidActionCandidates,
} from "./semantic-journey-driver.mjs";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { seekReceiverReplay, selectAndroidSettingsSliderStop, selectTrayOptionMatching } from "./release-journey-implementations.mjs";
import { androidTag, queryAndroidExactProjection, queryAndroidSemanticNodes, rectOfBounds,
  clickAndroidSemanticNode, androidSemanticTargetStateMatches, androidSemanticReadinessStateMatches,
} from "./android-harness.mjs";
import { chooseUnobscuredMapPoint } from "./gesture-geometry.mjs";
import { ObservationTimeoutError, performTransition, TransientObservationError } from "./transition-contract.mjs";
import { androidObservationBackend, rejectObservationOverrides } from "./android-observation-contract.mjs";

const source = readFileSync(new URL("semantic-journey-driver.mjs", import.meta.url), "utf8");
const projections = runInNewContext(source.match(
  /const ANDROID_EXACT_SCALAR_PROJECTIONS = (new Map\([\s\S]*?\));/,
)[1]);

// Execute the real reader, lookup helper and HTTP encoder. Only device I/O is
// replaced: an unavailable accessibility tree must be irrelevant, even for
// never-mounted/disposed projections. This fails deterministically on the old
// path without starting an emulator or hoping a slow runner reproduces it.
function device() {
  const snapshots = new Map();
  const requests = [];
  let responseOverride = null;
  const context = {
    androidObservationBackend, rejectObservationOverrides,
    URLSearchParams, TransientObservationError, androidElementSemanticTag, androidSemanticTag, androidPageTag,
    androidElementEnabled, androidTag, androidDataStatusRowsFromStateTag, rectOfBounds,
    androidActionCandidates,
    clickAndroidSemanticNode: () => assert.fail("unexpected physical input"),
    setAndroidSemanticProgress: () => assert.fail("unexpected progress input"),
    chooseUnobscuredMapPoint, androidMapInspectionPoint,
    androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll,
    androidProjectionMayRequireVerticalScan,
    observeUntil: async (_description, probe) => ({ value: await probe() }),
    E2E_TIMING: {},
    establishRevealedElement: options => establishRevealedElement({
      ...options,
      // Controlled observation completion; no wall-clock wait in these tests.
      observe: async (_description, probe) => ({ value: await probe() }),
    }),
    scrollUntilTag: () => assert.fail("unexpected traversal"),
    ANDROID_EXACT_SCALAR_PROJECTIONS: projections,
    requiredSemanticDriver: () => ({ port: 19191 }),
    semanticDriverObservationUnavailable: response => response.status === 28,
    semanticDriverObservationRequest(_port, path) {
      const url = new URL(path, "http://device");
      assert.ok(["/exact-projection", "/query"].includes(url.pathname));
      assert.equal(url.searchParams.get("provider_only"), "true", "no serialized accessibility queue");
      const tag = url.searchParams.get("tag");
      requests.push(tag);
      if (url.pathname === "/query") {
        assert.equal(url.searchParams.get("prefix"), "true");
        return responseOverride ?? {
          status: 0, stdout: JSON.stringify([...snapshots.entries()]
            .filter(([id]) => id.startsWith(tag)).map(([, node]) => node)),
        };
      }
      return (typeof responseOverride === "function" ? responseOverride() : responseOverride) ?? {
        status: 0, stdout: JSON.stringify(snapshots.has(tag) ? [snapshots.get(tag)] : []),
      };
    },
    dumpAndroid: () => assert.fail("no full-tree fallback"),
  };
  const helper = (name, next) => runInNewContext(`(${source.slice(
    source.indexOf(`function ${name}(`), source.indexOf(`function ${next}(`),
  ).trim()})`, context);
  context.queryAndroidExactProjection = runInNewContext(`(${queryAndroidExactProjection})`, context);
  context.queryAndroidSemanticNodes = runInNewContext(`(${queryAndroidSemanticNodes})`, context);
  context.queryFirstAndroidSemanticNode = helper("queryFirstAndroidSemanticNode", "readinessEvidenceMatchesTag");
  context.androidProjectedElement = helper("androidProjectedElement", "queryFirstAndroidSemanticNode");
  context.readinessEvidenceMatchesTag = runInNewContext(`(${source.slice(
    source.indexOf("function readinessEvidenceMatchesTag("), source.indexOf("export async function establishRevealedElement("),
  ).trim()})`, context);
  context.activateAndroidSemanticTag = runInNewContext(`(${source.slice(
    source.indexOf("function activateAndroidSemanticTag("), source.indexOf("export function androidPageIdFromStartupStateTag("),
  ).trim()})`, context);
  const driver = new AndroidSemanticJourneyDriver("no-device", {});
  for (const name of ["readAction", "performAction", "readScalarProjection", "readProjection", "scanProjection", "revealProjectionMatching", "readElement", "readPage", "readPageRoot", "revealElement", "findMapInspectionPoint", "readMapInteractionSnapshot", "setProgress"]) {
    driver[name] = runInNewContext(`({ ${AndroidSemanticJourneyDriver.prototype[name]} }).${name}`, context);
  }
  return {
    driver, snapshots, requests,
    respondWith(value) { responseOverride = value; },
    onTraversal(callback) { context.scrollUntilTag = callback; },
    onTap(callback) { context.clickAndroidSemanticNode = callback; },
    onProgress(callback) { context.setAndroidSemanticProgress = callback; },
  };
}

for (const intent of ["select", "activate"]) {
  for (const change of ["selected", "checked", "enabled", "identity", "lost-click"]) {
    test(`Android ${intent} action revalidates ${change} after stale pre-input evidence`, async t => {
      const { driver, snapshots, onTap } = device();
      const tag = "parity:tray-option:plate:KOMA:IAP-NE-ILS OR LOC RWY 32R.png";
      const node = { "resource-id": tag, text: "ILS OR LOC RWY 32R", enabled: "true",
        selected: "true", checked: "false", visible: "true", "center-reachable": "true",
        bounds: "[185,1398][757,1545]", "semantic-path": "projection-provider:process:1413" };
      snapshots.set(tag, node);
      let taps = 0, attempts = 0, clock = 0, closed = false;
      driver.readSessionRevision = async () => 123;
      driver.readElement = async () => ({ enabled: true, text: node.text });
      driver.waitForObservation = async () => { clock += 50; };
      const click = runInNewContext(`(${clickAndroidSemanticNode})`, {
        requiredSemanticDriver: () => ({ port: 19191 }),
        waitForAndroidSemanticEvent: () => {},
        androidSemanticTargetStateMatches, androidSemanticReadinessStateMatches,
        queryAndroidExactProjection: () => [snapshots.get(tag)],
        androidPhysicalTapTarget: (_port, id, bounds, path) => {
          attempts++;
          if (attempts === 1) {
            snapshots.set(tag, { ...node, "semantic-path": "projection-provider:process:1438",
              ...(change === "selected" ? { selected: "false" } : {}),
              ...(change === "checked" ? { checked: "true" } : {}),
              ...(change === "enabled" ? { enabled: "false" } : {}),
              ...(change === "identity" ? { "resource-id": "different-plate" } : {}),
            });
            return null; // Native driver rejects stale evidence BEFORE delivering input.
          }
          assert.equal(id, tag);
          assert.equal(bounds, node.bounds);
          assert.equal(path, "projection-provider:process:1438");
          taps++;
          closed = change !== "lost-click";
          if (closed) {
            snapshots.delete(tag);
            const viewportId = "org.aerobag.app:id/e2e_plate_viewport_projection";
            snapshots.set(viewportId, { "resource-id": viewportId,
              "state-description": `chart:${tag.slice("parity:tray-option:".length)}:zoom:1`, visible: "true" });
          }
          return { delivered: true };
        },
      });
      onTap(click);
      const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-choice-revalidation-"));
      t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
      const runtime = createJourneyRuntime({ journey: { id: "test.choice-revalidation", assertions: [] },
        platform: "android", driver, artifactDir });
      const transition = runtime.transition;
      runtime.transition = (description, contract) => transition(description, {
        ...contract, scheduler: { now: () => clock, setTimeout, clearTimeout },
      });
      const operation = intent === "select"
        ? selectTrayOptionMatching(runtime, "plate-chart-button", node.text)
        : runtime.action("toggle", tag, { complete: () => closed });
      if (intent === "select" && ["selected", "checked"].includes(change)) {
        await operation;
        assert.equal(taps, 1);
      } else {
        await assert.rejects(operation, /state changed|timed out/);
        assert.equal(taps, change === "lost-click" ? 1 : 0);
      }
    });
  }
}

test("Settings slider helper passes rendered identity and range input through the real Android driver", async t => {
  const { driver, snapshots, onProgress } = device();
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-settings-progress-"));
  t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
  const oldTag = "parity:settings-slider:display_dim_timeout:2m";
  const newTag = "parity:settings-slider:display_dim_timeout:10s";
  const node = { "resource-id": oldTag, enabled: "true", visible: "true", "center-reachable": "true",
    bounds: "[105,1627][974,1669]", "semantic-path": "projection-provider:process:149",
    "state-description": "kind:horizontal-progress:min:0.0:max:5.0:enabled:true:state:2m:window-focus:true" };
  snapshots.set(oldTag, node);
  let inputs = 0;
  driver.readSessionRevision = async () => 37;
  driver.waitForObservation = async () => {};
  onProgress((serial, tag, value, bounds, path) => {
    inputs++;
    assert.equal(serial, "no-device");
    assert.equal(tag, oldTag);
    assert.equal(value, 0);
    assert.equal(bounds, node.bounds);
    assert.equal(path, node["semantic-path"]);
    snapshots.delete(oldTag);
    snapshots.set(newTag, { ...node, "resource-id": newTag,
      "state-description": node["state-description"].replace("state:2m", "state:10s") });
  });
  const runtime = createJourneyRuntime({ journey: { id: "test.settings-progress", assertions: [] },
    platform: "android", driver, artifactDir });
  const result = await selectAndroidSettingsSliderStop(runtime, {
    rowId: "display_dim_timeout", from: "2m", to: "10s", index: 0, description: "select dim timeout",
  });
  assert.equal(inputs, 1);
  assert.equal(result[0].id, newTag);
});

for (const kind of ["replay", "settings"]) {
  for (const outcome of ["ready", "unfocused", "disabled", "absent", "rejected", "lost", "wrong-value"]) {
    test(`Android ${kind} progress waits for restored window focus: ${outcome}`, async t => {
      const { driver, snapshots, onProgress } = device();
      const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-progress-focus-"));
      t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
      const tag = kind === "replay" ? "parity:playback-overview"
        : "parity:settings-slider:display_dim_timeout:2m";
      const nextTag = "parity:settings-slider:display_dim_timeout:10s";
      const node = { "resource-id": tag, enabled: "true", visible: "true", "center-reachable": "false",
        bounds: "[41,2015][1054,2138]", "semantic-path": "projection-provider:process:1661",
        "state-description": "kind:horizontal-progress:min:0:max:90:enabled:true:window-focus:false" };
      snapshots.set(tag, node);
      let clock = 0, attempts = 0, released = false;
      function publishPlayback(cursor) {
        const id = "org.aerobag.app:id/e2e_playback_widget_projection";
        snapshots.set(id, { "resource-id": id, visible: "true",
          "state-description": `status:paused:cursor:${cursor}:duration:90:rate:1:gaps:0` });
      }
      publishPlayback(18);
      if (kind === "settings") {
        snapshots.set(tag, { ...node, "center-reachable": "true",
          "state-description": node["state-description"].replace("window-focus:false", "window-focus:true") });
        const reveal = driver.revealElement.bind(driver);
        driver.revealElement = async (...args) => {
          const result = await reveal(...args);
          snapshots.set(tag, node);
          return result;
        };
      }
      driver.readSessionRevision = async () => 126;
      driver.waitForObservation = async () => {
        clock += 50;
        if (clock < 200 || released) return;
        released = true;
        if (outcome === "absent") snapshots.delete(tag);
        else if (outcome !== "unfocused") snapshots.set(tag, { ...node,
          enabled: outcome === "disabled" ? "false" : "true", "center-reachable": "true",
          "semantic-path": "projection-provider:process:1662",
          "state-description": node["state-description"].replace("window-focus:false", "window-focus:true") });
      };
      onProgress((_serial, actualTag, value, bounds, path) => {
        attempts++;
        const current = snapshots.get(actualTag);
        assert.equal(actualTag, tag);
        assert.equal(current?.["center-reachable"], "true", "unfocused control is visible but not ready for input");
        assert.equal(path, "projection-provider:process:1662", "use the newly focused rendering, not stale evidence");
        assert.equal(bounds, node.bounds);
        assert.equal(value, kind === "replay" ? 2 : 0);
        if (outcome === "rejected") throw new Error("progress action rejected");
        if (outcome === "lost") return;
        if (kind === "replay") publishPlayback(outcome === "wrong-value" ? 7 : 2);
        else if (outcome !== "wrong-value") {
          snapshots.delete(tag);
          snapshots.set(nextTag, { ...current, "resource-id": nextTag });
        }
      });
      const runtime = createJourneyRuntime({ journey: { id: "test.progress-focus", assertions: [] },
        platform: "android", driver, artifactDir });
      const transition = runtime.transition;
      runtime.transition = (description, contract) => transition(description, {
        ...contract, scheduler: { now: () => clock, setTimeout, clearTimeout },
      });
      const operation = kind === "replay" ? seekReceiverReplay(runtime, 2)
        : selectAndroidSettingsSliderStop(runtime, {
          rowId: "display_dim_timeout", from: "2m", to: "10s", index: 0, description: "select dim timeout",
        });
      if (outcome === "ready") await operation;
      else if (outcome === "rejected") await assert.rejects(operation, /progress action rejected/);
      else await assert.rejects(operation, ObservationTimeoutError);
      assert.equal(attempts, ["unfocused", "disabled", "absent"].includes(outcome) ? 0 : 1);
    });
  }
}

for (const outcome of ["ready", "unfocused", "disabled", "absent", "rejected", "lost"]) {
  test(`Android action near the readiness deadline: ${outcome}`, async t => {
    const { driver, snapshots, onTap } = device();
    const tag = "parity:ownship-source:__playback_trace__";
    const node = { "resource-id": tag, text: "REPLAY", enabled: "true", visible: "true",
      "center-reachable": "false", bounds: "[771,400][918,547]", "semantic-path": "projection-provider:process:75" };
    snapshots.set(tag, node);
    let clock = 0, taps = 0, completed = false;
    driver.readSessionRevision = async () => 28;
    driver.waitForObservation = async () => {
      clock = clock === 0 ? 2755 : clock + 200;
      if (outcome === "absent") snapshots.delete(tag);
      else if (outcome === "disabled") snapshots.set(tag, { ...node, enabled: "false", "center-reachable": "true" });
      else if (outcome !== "unfocused") snapshots.set(tag, { ...node, "center-reachable": "true" });
    };
    onTap((_serial, id, bounds, path) => {
      assert.ok(clock >= 2755 && clock < 3000, "use actual readiness within the existing budget");
      assert.equal(id, tag); assert.equal(bounds, node.bounds); assert.equal(path, node["semantic-path"]);
      taps++;
      if (outcome === "rejected") throw new Error("physical tap target rejected");
      completed = outcome !== "lost";
      return true;
    });
    const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-action-ready-"));
    t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
    const runtime = createJourneyRuntime({ journey: { id: "test.action-ready", assertions: [] },
      platform: "android", driver, artifactDir });
    const action = runtime.action("select Replay", tag, {
      complete: () => completed,
      scheduler: { now: () => clock, setTimeout, clearTimeout },
    });
    if (outcome === "ready") {
      await action;
      assert.equal(taps, 1);
    } else {
      await assert.rejects(action, /timed out|physical tap target rejected/);
      assert.equal(taps, ["rejected", "lost"].includes(outcome) ? 1 : 0, "never retry input");
    }
  });
}

test("indexed controls own absence without caller hints, for exact and collection observations", async () => {
  const { driver, snapshots, respondWith } = device();
  for (const tag of ["parity:service:notice:test", "parity:data-status-panel", "parity:airport-info-modal:KSEA", "parity:plan-row:1", "parity:tray-option:Vectors"]) {
    for (const read of [() => driver.readElement(tag), () => driver.readProjection(tag)]) {
      const absent = value => assert.ok(value === null || value.length === 0);
      absent(await read());
      snapshots.set(tag, { "resource-id": tag, visible: "true", text: "Rendered", bounds: "[0,0][90,120]" });
      const present = await read();
      assert.equal((Array.isArray(present) ? present[0] : present).text, "Rendered");
      snapshots.delete(tag);
      absent(await read());
      respondWith({ status: 28, stderr: "provider busy", stdout: "" });
      await assert.rejects(read(), TransientObservationError);
      respondWith({ status: 7, stderr: "connection refused", stdout: "" });
      await assert.rejects(read(), /persistent Android/);
      respondWith(null);
    }
  }
});

test("modal observations follow rendering, never the model's intent to open or close", async () => {
  const { driver, snapshots, respondWith } = device();
  for (const host of ["parity:map-selection-state:", "parity:flight-plan-overlay-state:"]) {
    for (const id of ["airport-info-modal:KSEA", "weather-detail-modal", "map-selection-detail-modal:TFR"]) {
      const hostId = projections.get(host);
      snapshots.set(hostId, {
        "resource-id": hostId, "state-description": `detail:${encodeURIComponent(id)}`,
      });
      assert.equal(await driver.readModal(id), null, "open intent precedes modal layout");
      const tag = `parity:${id}`;
      snapshots.set(tag, { "resource-id": tag, visible: "true", text: "Rendered modal", bounds: "[0,0][90,120]" });
      assert.equal((await driver.readModal(id)).text, "Rendered modal");
      snapshots.set(hostId, { "resource-id": hostId, "state-description": "detail:none" });
      assert.ok(await driver.readModal(id), "close intent precedes modal removal");
      snapshots.delete(tag);
      assert.equal(await driver.readModal(id), null);
      snapshots.set(hostId, { "resource-id": hostId, "state-description": `detail:${encodeURIComponent(id)}` });
      assert.equal(await driver.readModal(id), null, "retained model on remount is not rendered");
      respondWith({ status: 28, stderr: "provider busy", stdout: "" });
      await assert.rejects(() => driver.readModal(id), TransientObservationError);
      respondWith(null);
      snapshots.delete(hostId);
    }
  }
});

test("collection scanning shares the target observation instead of traversing another backend", async () => {
  const { driver, snapshots } = device();
  assert.equal((await driver.scanProjection("parity:plan-row:")).length, 0);
  snapshots.set("parity:plan-row:1", { "resource-id": "parity:plan-row:1", text: "KSEA", visible: "true" });
  assert.equal((await driver.scanProjection("parity:plan-row:"))[0].text, "KSEA");
});

test("collection reveal cannot turn observation failure into target absence", async () => {
  const { driver, respondWith } = device();
  respondWith({ status: 7, stderr: "connection refused", stdout: "" });
  await assert.rejects(driver.revealProjectionMatching("parity:plan-row:", "KSEA"), /persistent Android/);
});

test("both page readers use the positioned page index throughout navigation", async () => {
  const { driver, snapshots, requests, respondWith } = device();
  for (const page of ["map", "charts", "flight_plan", "altitude_planner", "data_status", "settings", "home", "cloud", "offline_packages"]) {
    const tag = androidPageTag(page);
    const element = `page:${page === "charts" ? "plate" : page}`;
    const reads = [() => driver.readPage(page), () => driver.readElement(element), () => driver.readElement(tag)];
    for (const read of reads) assert.equal(await read(), null, "never mounted");
    const node = { "resource-id": tag, visible: "true", bounds: "[0,0][900,1200]" };
    snapshots.set(tag, node);
    assert.equal((await driver.readPage(page)).pageId, page);
    assert.equal((await driver.readElement(element)).bounds, node.bounds);
    assert.equal((await driver.readElement(tag)).test_id, tag);
    snapshots.set(tag, { ...node, visible: "false" });
    for (const read of reads) assert.equal(await read(), null, "not visible");
    snapshots.delete(tag);
    for (const read of reads) assert.equal(await read(), null, "left page");
    respondWith({ status: 28, stdout: "", stderr: "provider busy" });
    for (const read of reads) await assert.rejects(read(), TransientObservationError);
    respondWith(null);
  }
  assert.equal(requests.length, 9 * 15, "one indexed request per read, no fallback or hidden retry");
});

test("chart supplement transition checks absence before delivering exactly one click", async () => {
  const { driver, snapshots, requests } = device();
  const tag = androidPageTag("charts");
  let clicks = 0;
  const { value } = await performTransition("open airport chart supplement", {
    ready: () => ({ test_id: "parity:map-selection-action:csup" }),
    act: () => {
      assert.deepEqual(requests, [tag], "destination was checked before the click");
      clicks += 1;
      snapshots.set(tag, { "resource-id": tag, visible: "true", bounds: "[0,0][900,1200]" });
    },
    complete: () => driver.readElement("page:plate"),
    readinessSamples: 1,
    completionSamples: 1,
  });
  assert.equal(clicks, 1);
  assert.equal(value.test_id, tag);
  assert.deepEqual(requests, [tag, tag]);
});

test("every fixed state projection bypasses the tree for presence, updates and absence", () => {
  const { driver, snapshots, requests } = device();
  for (const [prefix, tag] of projections) {
    assert.equal(driver.readScalarProjection(prefix).length, 0, "never mounted");
    snapshots.set(tag, { "state-description": "first" });
    assert.equal(driver.readScalarProjection(prefix)[0]["state-description"], "first");
    snapshots.set(tag, { "state-description": "changed" });
    assert.equal(driver.readScalarProjection(prefix)[0]["state-description"], "changed");
    snapshots.delete(tag);
    assert.equal(driver.readScalarProjection(prefix).length, 0, "disposed");
  }
  assert.equal(requests.length, projections.size * 4, "one request per observation, no hidden retry");
});

test("map-layer probes use the indexed snapshot, preserving names and toggle state", async () => {
  const { driver, snapshots } = device();
  const tag = projections.get("parity:map-layers:");
  assert.ok(tag, "map layers need a fixed provider identity");
  assert.equal((await driver.readProjection("parity:map-layer:Vectors:")).length, 0);
  snapshots.set(tag, { "state-description": "Vectors:visible:true:enabled:true|Nexrad:visible:false:enabled:true" });
  let entries = await driver.readProjection("parity:map-layer:Vectors:");
  assert.equal(entries.length, 1);
  assert.equal(entries[0].id, "parity:map-layer:Vectors:visible:true:enabled:true");
  snapshots.set(tag, { "state-description": "Vectors:visible:false:enabled:false" });
  entries = await driver.readProjection("parity:map-layer:Vectors:");
  assert.equal(entries[0].id, "parity:map-layer:Vectors:visible:false:enabled:false");
  assert.equal(entries[0].enabled, false);
  assert.equal((await driver.readProjection("parity:map-layer:Nexrad:")).length, 0);
  const mapPage = readFileSync(new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt", import.meta.url), "utf8");
  assert.match(mapPage, /E2eProjectionView\(\s*viewId = R\.id\.e2e_map_layers_projection,\s*state = buildMapLayersProjectionState\(mapLayerState\)/);
});

test("data-status rows treat an absent indexed snapshot as absence, not permission to traverse", async () => {
  const { driver, snapshots } = device();
  const read = () => driver.readProjection("parity:data-status-row:");
  assert.equal((await read()).length, 0);
  const tag = projections.get("parity:data-status-state:");
  snapshots.set(tag, { "state-description": "client=ok" });
  assert.equal((await read())[0].id, "parity:data-status-row:client:severity:ok");
  snapshots.delete(tag);
  assert.equal((await read()).length, 0);
});

test("airport scroll readiness uses positioned bounds, not the modal's changing descendant text", async () => {
  const { driver, snapshots } = device();
  const tag = "parity:airport-info-modal:KSEA";
  const read = () => driver.readElement("airport-info-modal:KSEA");
  assert.equal(await read(), null);
  snapshots.set(tag, {
    "resource-id": tag, "semantic-path": "projection-provider:4", "state-description": "enabled:true",
    text: "", visible: "true", enabled: "true", bounds: "[0,100][900,1200]",
  });
  assert.equal((await read()).bounds, "[0,100][900,1200]");
  snapshots.delete(tag);
  assert.equal(await read(), null);
  const implementation = readFileSync(new URL("release-journey-implementations.mjs", import.meta.url), "utf8");
  assert.match(implementation, /"scroll airport info", \{\s*ready: \(\) => runtime\.driver\.readElement\(`airport-info-modal:\$\{complexAirport\}`\)/);
});

test("provider unavailability and permanent failures stay errors, never absence or a fallback", async () => {
  const modeled = device();
  modeled.respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  assert.throws(() => modeled.driver.readScalarProjection("parity:airport-info-scroll:"), TransientObservationError);
  await assert.rejects(modeled.driver.readElement("airport-info-modal:KSEA"), TransientObservationError);
  modeled.respondWith({ status: 7, stdout: "", stderr: "connection refused" });
  assert.throws(() => modeled.driver.readScalarProjection("parity:airport-info-scroll:"), /persistent Android/);
  assert.equal(modeled.requests.length, 3);
});

test("map gesture geometry bypasses the tree without ignoring controls or stale surfaces", async () => {
  const { driver, snapshots, respondWith } = device();
  const surface = {
    bounds: "[0,0][1000,1000]", "resource-id": "parity:map-surface", visible: "true",
  };
  // Exercise the surface lookup too, as both shared taps and native CTR drags
  // do. Supplying fake geometry here missed its tree-queue dependency.
  const read = async () => {
    const ready = await driver.readElement("map-surface");
    return ready && driver.findMapInspectionPoint(ready);
  };
  assert.equal(await read(), null, "an absent surface is not evidence of an unobscured map");
  snapshots.set(surface["resource-id"], surface);
  snapshots.set("parity:instrument-panel", {
    "resource-id": "parity:instrument-panel", bounds: "[200,600][400,800]",
  });
  const point = await read();
  assert.ok(point);
  assert.equal(point.screenX, 700, "skip the first candidate because an instrument covers it");
  assert.equal(point.screenY, 700);
  const ready = await driver.readElement("map-surface");
  snapshots.set(surface["resource-id"], { ...surface, bounds: "[0,0][500,1000]" });
  assert.equal(await driver.findMapInspectionPoint(ready), null, "re-observe after a layout change");
  respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  await assert.rejects(read(), TransientObservationError);
});

test("provider-only batch requests bypass the server's accessibility queue and fallback", () => {
  const service = readFileSync(new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url), "utf8");
  const lock = service.slice(service.indexOf("private static boolean requiresSerializedAccessibility"), service.indexOf("private void handleSetText"));
  assert.match(lock, /"\/exact-projection"\.equals\(endpoint\) \|\| "\/query"\.equals\(endpoint\) \|\| "\/scroll"\.equals\(endpoint\) \|\| "\/set-progress"\.equals\(endpoint\)\) \{\s*return false/);
  const handler = service.slice(service.indexOf("private void handleQuery"), service.indexOf("private void handleExactProjection"));
  assert.match(handler, /providerProjectionPrefix\(tag\)/);
  assert.match(handler, /providerProjection\(tag, false\)/);
  assert.doesNotMatch(handler, /renderNodeQuery|unhandled|new JSONArray/);
});

test("native gesture readiness takes one device snapshot, retaining follow and obstacle checks", async () => {
  const { driver, snapshots, requests, respondWith } = device();
  const native = readFileSync(new URL("run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const drag = native.slice(native.indexOf("async function dragMapWhileFollowing"), native.indexOf("async function zoomMapOneStepWhileFollowing"));
  const context = { driver, performance, readiness: [] };
  context.parseMapFollowTag = runInNewContext(`(${native.slice(
    native.indexOf("function parseMapFollowTag("), native.indexOf("function mapFollowOffsetPx("),
  ).trim()})`);
  const ready = runInNewContext(`(async () => {${drag.match(/ready: async \(\) => \{([\s\S]*?)\n      \},\n      diagnose:/)[1]}})`, context);
  const read = async () => {
    const before = requests.length;
    try { return await ready(); } finally {
      assert.equal(requests.length - before, 1, "never serialize separate state/surface/obstacle round trips");
    }
  };
  assert.equal(await read(), null);
  snapshots.set("parity:map-surface", {
    "resource-id": "parity:map-surface", visible: "true", bounds: "[0,0][1000,1000]",
  });
  const followId = projections.get("parity:map-follow-state:");
  const follow = (following) => snapshots.set(followId, {
    "resource-id": followId,
    "state-description": `following:${following}:ownship-x:500:ownship-y:500:center-x:500:center-y:500:zoom-centi:1139`,
  });
  assert.equal(await read(), null, "geometry without follow state is not readiness");
  follow(0);
  assert.equal(await read(), null, "CTR must be engaged");
  follow(1);
  snapshots.set("parity:instrument-panel", {
    "resource-id": "parity:instrument-panel", bounds: "[200,600][400,800]",
  });
  assert.equal((await read()).point.screenX, 700, "avoid the first candidate behind instruments");
  snapshots.delete("parity:map-surface");
  assert.equal(await read(), null, "follow state alone is insufficient");
  respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  await assert.rejects(read(), TransientObservationError);
  assert.match(drag, /error\.nativeResult = result;\s*throw error/);
  assert.match(native, /const result = createTestResult\(test\.id\);[\s\S]*test\.run\(\{ \.\.\.args, result \}\)/);
  assert.match(native, /const failed = error\.nativeResult \?\? result/);
  assert.match(native, /const result = args\.result \?\? createTestResult\("android\.map-follow-ctr-gesture-smoke"\)/);
  assert.match(native, /writeFileSync\(join\(E2E_ARTIFACT_DIR, "result\.json"\)/);
});

test("native map-follow state uses the shared scalar reader for presence and absence", () => {
  const { driver, snapshots, respondWith } = device();
  const native = readFileSync(new URL("run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const context = {
    nativeSemanticDriver: () => driver,
    MAP_FOLLOW_PREFIX: "parity:map-follow-state:",
    queryAndroidExactProjection: () => assert.fail("native state must share the provider-only reader"),
  };
  const helper = (name, next) => runInNewContext(`(${native.slice(
    native.indexOf(`function ${name}(`), native.indexOf(next),
  ).trim()})`, context);
  context.parseMapFollowTag = helper("parseMapFollowTag", "function mapFollowOffsetPx(");
  const read = helper("queryMapFollowProbe", "async function waitForMapFollowProbe(");
  assert.equal(read("no-device"), null);
  const tag = projections.get("parity:map-follow-state:");
  snapshots.set(tag, { "state-description": "following:1:ownship-x:300:ownship-y:700:center-x:500:center-y:500:zoom-centi:1139" });
  assert.equal(read("no-device").following, true);
  assert.equal(read("no-device").ownshipX, 300);
  snapshots.delete(tag);
  assert.equal(read("no-device"), null);
  respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  assert.throws(() => read("no-device"), TransientObservationError);
});

test("status popup and service collections prove absence without tree access", async () => {
  const { driver, snapshots, respondWith } = device();
  const panelTag = "parity:data-status-panel";
  const panel = () => driver.readElement("data-status-panel");
  assert.equal(await panel(), null, "never-mounted popup");
  snapshots.set(panelTag, { "resource-id": panelTag, visible: "true", bounds: "[10,10][400,600]" });
  assert.equal((await panel()).bounds, "[10,10][400,600]");
  snapshots.delete(panelTag);
  assert.equal(await panel(), null, "dismissed popup");
  const bodies = () => driver.readProjection("parity:service:body:");
  assert.equal((await bodies()).length, 0, "folded history");
  const bodyTag = `parity:service:body:${"a".repeat(64)}`;
  snapshots.set(bodyTag, { "resource-id": bodyTag, text: "notice body", enabled: "true" });
  assert.equal((await bodies())[0].text, "notice body");
  snapshots.delete(bodyTag);
  assert.equal((await bodies()).length, 0, "folded again");
  const rowTag = "parity:data-status-box-plate:procedure_geometry:0";
  snapshots.set(rowTag, { "resource-id": rowTag, text: "This publication reports a warning", enabled: "true" });
  const rows = await driver.readProjection("data-status-box-plate:procedure_geometry:");
  assert.equal(rows[0].text, "This publication reports a warning");
  respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  await assert.rejects(panel(), TransientObservationError);
  await assert.rejects(bodies(), TransientObservationError);
  const page = readFileSync(new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/DataStatusPage.kt", import.meta.url), "utf8");
  assert.match(page, /\.e2eIndexedControl\("parity:\$testTagPrefix-panel", enabled = true\)/);
});


test("all Settings controls register with the same index their driver reads", () => {
  const page = readFileSync(new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/SettingsPage.kt", import.meta.url), "utf8");
  // Provider-only reads cannot find a control that only has a test tag. The
  // physical SettingsPageTest also verifies slider geometry, input and disposal
  // with indexing enabled; this guard runs in the fixture-free harness suite.
  assert.doesNotMatch(page, /\.testTag\("parity:settings-/);
  assert.match(page, /\.e2eIndexedHorizontalProgress\(\s*semanticTag = "parity:settings-slider:/);
  assert.match(page, /InlineHelpButton\(helpText, "parity:settings-help:/);
  const help = readFileSync(new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/InlineHelpButton.kt", import.meta.url), "utf8");
  assert.match(help, /\.e2eIndexedControl\(semanticTag = tag, enabled = true, text = text\)/);
  assert.match(help, /showActionToast\(context, text, long = true\)/);
});

test("Settings reveal uses the control index before and after lazy-list traversal", async () => {
  const { driver, snapshots, requests, onTraversal } = device();
  const id = "settings-section-debug_diagnostics";
  const tag = "parity:settings-section:debug_diagnostics";
  const node = {
    "resource-id": tag, "semantic-path": "projection-provider:1",
    visible: "true", "center-reachable": "true", enabled: "true",
    bounds: "[0,100][900,200]",
  };
  let traversals = 0;
  onTraversal(async (_serial, target, _count, reachable, avoidNavigation, ...options) => {
    assert.equal(target, tag);
    assert.equal(reachable, true);
    assert.equal(avoidNavigation, true);
    assert.equal(options.length, 0, "traversal cannot override the target's backend");
    assert.equal(androidObservationBackend(target), "indexed");
    traversals += 1;
    snapshots.set(tag, node);
  });
  assert.equal((await driver.revealElement(id)).test_id, tag);
  assert.equal(traversals, 1);
  assert.equal(requests.length, 2);
  assert.equal((await driver.revealElement(id)).test_id, tag);
  assert.equal(traversals, 1, "already reachable controls do not scroll");
  snapshots.delete(tag);
  assert.equal(await driver.readElement(id), null, "disposal uses the same authoritative index");
});

test("a busy first Settings reveal read cannot escape or trigger a speculative scroll", async () => {
  const { driver, snapshots, respondWith } = device();
  const tag = "parity:settings-section:debug_diagnostics";
  snapshots.set(tag, {
    "resource-id": tag, visible: "true", "center-reachable": "true",
    bounds: "[0,100][900,200]",
  });
  let reads = 0;
  respondWith(() => ++reads === 1 ? { status: 28, stdout: "", stderr: "provider busy" } : null);
  assert.equal((await driver.revealElement("settings-section-debug_diagnostics")).test_id, tag);
  assert.equal(reads, 2);
});
