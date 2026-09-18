// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  AndroidSemanticJourneyDriver, androidElementEnabled, androidElementSemanticTag,
  androidSemanticTag, androidDataStatusRowsFromStateTag, androidMapInspectionPoint,
  androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll, establishRevealedElement,
} from "./semantic-journey-driver.mjs";
import { androidTag, queryAndroidExactProjection, queryAndroidSemanticNodes, rectOfBounds } from "./android-harness.mjs";
import { chooseUnobscuredMapPoint } from "./gesture-geometry.mjs";
import { TransientObservationError } from "./transition-contract.mjs";

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
    URLSearchParams, TransientObservationError, androidElementSemanticTag, androidSemanticTag,
    androidElementEnabled, androidTag, androidDataStatusRowsFromStateTag, rectOfBounds,
    chooseUnobscuredMapPoint, androidMapInspectionPoint,
    androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll,
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
  const driver = new AndroidSemanticJourneyDriver("no-device", {});
  for (const name of ["readScalarProjection", "readProjection", "readElement", "revealElement", "findMapInspectionPoint", "readMapInteractionSnapshot"]) {
    driver[name] = runInNewContext(`({ ${AndroidSemanticJourneyDriver.prototype[name]} }).${name}`, context);
  }
  return {
    driver, snapshots, requests,
    respondWith(value) { responseOverride = value; },
    onTraversal(callback) { context.scrollUntilTag = callback; },
  };
}

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
  const read = () => driver.readElement("airport-info-modal:KSEA", { indexed: true });
  assert.equal(await read(), null);
  snapshots.set(tag, {
    "resource-id": tag, "semantic-path": "projection-provider:4", "state-description": "enabled:true",
    text: "", visible: "true", enabled: "true", bounds: "[0,100][900,1200]",
  });
  assert.equal((await read()).bounds, "[0,100][900,1200]");
  snapshots.delete(tag);
  assert.equal(await read(), null);
  const implementation = readFileSync(new URL("release-journey-implementations.mjs", import.meta.url), "utf8");
  assert.match(implementation, /"scroll airport info", \{\s*ready: \(\) => runtime\.driver\.readElement\(`airport-info-modal:\$\{complexAirport\}`, \{ indexed: true \}\)/);
});

test("provider unavailability and permanent failures stay errors, never absence or a fallback", async () => {
  const modeled = device();
  modeled.respondWith({ status: 28, stdout: "", stderr: "provider busy" });
  assert.throws(() => modeled.driver.readScalarProjection("parity:airport-info-scroll:"), TransientObservationError);
  await assert.rejects(modeled.driver.readElement("airport-info-modal:KSEA", { indexed: true }), TransientObservationError);
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
  assert.match(lock, /\("\/exact-projection"\.equals\(endpoint\) \|\| "\/query"\.equals\(endpoint\)\) &&\s*"true"\.equals\(query\.getOrDefault\("provider_only", "false"\)\)\) \{\s*return false/);
  const handler = service.slice(service.indexOf("private void handleQuery"), service.indexOf("private void handleExactProjection"));
  assert.match(handler, /prefix\s*\? providerProjectionPrefix\(tag\)/);
  assert.match(handler, /providerOnly \? new JSONArray\(\)\s*: renderNodeQuery/);
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
  const panel = () => driver.readElement("data-status-panel", { indexed: true });
  assert.equal(await panel(), null, "never-mounted popup");
  snapshots.set(panelTag, { "resource-id": panelTag, visible: "true", bounds: "[10,10][400,600]" });
  assert.equal((await panel()).bounds, "[10,10][400,600]");
  snapshots.delete(panelTag);
  assert.equal(await panel(), null, "dismissed popup");
  const bodies = () => driver.readProjection("parity:service:body:", { indexed: true });
  assert.equal((await bodies()).length, 0, "folded history");
  const bodyTag = `parity:service:body:${"a".repeat(64)}`;
  snapshots.set(bodyTag, { "resource-id": bodyTag, text: "notice body", enabled: "true" });
  assert.equal((await bodies())[0].text, "notice body");
  snapshots.delete(bodyTag);
  assert.equal((await bodies()).length, 0, "folded again");
  const rowTag = "parity:data-status-box-plate:procedure_geometry:0";
  snapshots.set(rowTag, { "resource-id": rowTag, text: "This publication reports a warning", enabled: "true" });
  const rows = await driver.readProjection("data-status-box-plate:procedure_geometry:", { indexed: true });
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
  for (const kind of ["slider", "help"]) {
    assert.match(page, new RegExp(`\\.e2eIndexedControl\\(\\s*semanticTag = "parity:settings-${kind}:`));
  }
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
  onTraversal(async (_serial, target, _count, reachable, avoidNavigation, options) => {
    assert.equal(target, tag);
    assert.equal(reachable, true);
    assert.equal(avoidNavigation, true);
    assert.equal(options.providerOnly, true, "absence during traversal must also bypass the tree");
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
