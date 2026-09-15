// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  AndroidSemanticJourneyDriver, androidElementEnabled, androidElementSemanticTag,
  androidSemanticTag, androidDataStatusRowsFromStateTag,
} from "./semantic-journey-driver.mjs";
import { androidTag, queryAndroidExactProjection } from "./android-harness.mjs";
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
    androidElementEnabled, androidTag, androidDataStatusRowsFromStateTag,
    ANDROID_EXACT_SCALAR_PROJECTIONS: projections,
    requiredSemanticDriver: () => ({ port: 19191 }),
    semanticDriverObservationUnavailable: response => response.status === 28,
    semanticDriverObservationRequest(_port, path) {
      const url = new URL(path, "http://device");
      assert.equal(url.pathname, "/exact-projection");
      assert.equal(url.searchParams.get("provider_only"), "true", "no serialized accessibility queue");
      const tag = url.searchParams.get("tag");
      requests.push(tag);
      return responseOverride ?? {
        status: 0, stdout: JSON.stringify(snapshots.has(tag) ? [snapshots.get(tag)] : []),
      };
    },
    queryAndroidSemanticNodes: () => assert.fail("no prefix-tree fallback"),
    dumpAndroid: () => assert.fail("no full-tree fallback"),
  };
  const helper = (name, next) => runInNewContext(`(${source.slice(
    source.indexOf(`function ${name}(`), source.indexOf(`function ${next}(`),
  ).trim()})`, context);
  context.queryAndroidExactProjection = runInNewContext(`(${queryAndroidExactProjection})`, context);
  context.queryFirstAndroidSemanticNode = helper("queryFirstAndroidSemanticNode", "readinessEvidenceMatchesTag");
  context.androidProjectedElement = helper("androidProjectedElement", "queryFirstAndroidSemanticNode");
  const driver = new AndroidSemanticJourneyDriver("no-device", {});
  for (const name of ["readScalarProjection", "readProjection", "readElement"]) {
    driver[name] = runInNewContext(`({ ${AndroidSemanticJourneyDriver.prototype[name]} }).${name}`, context);
  }
  return { driver, snapshots, requests, respondWith(value) { responseOverride = value; } };
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
