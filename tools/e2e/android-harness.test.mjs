// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  assertNoAerobagAnr,
  androidImeVisible,
  androidInteractiveRuntime,
  androidOfflinePackagesVisible,
  androidRuntimeReadyForJourney,
  androidRuntimeUiVisible,
  androidStartupProjection,
  androidStartupState,
  androidJourneyEpochMs,
  androidSemanticNodeIsActionable,
  classifyAerobagLogcat,
  displayBoundsFromXml,
  destinationCenterEvidence,
  findHorizontalScrollSurface,
  findVerticalScrollSurface,
  findNode,
  findAerobagAnrDialog,
  layerToggleNode,
  layerToggleTag,
  renderedFlightPlanSignature,
  restartAndroidAppAcrossSemanticLifecycle,
  verticalScrollTargetIsReachable,
} from "./android-harness.mjs";

test("persistent semantic dumps refresh accessibility roots before traversal", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  assert.match(source, /AccessibilityNodeInfo root = window\.getRoot\(\);[\s\S]*?root\.refresh\(\);/);
  assert.match(source, /AccessibilityNodeInfo activeRoot = getRootInActiveWindow\(\);[\s\S]*?activeRoot\.refresh\(\);/);
  assert.match(
    source,
    /refreshSubtree \|\| viewId\.startsWith\("parity:"\)[\s\S]*?if \(refreshThisSubtree\) node\.refresh\(\);/,
  );
  assert.match(
    source,
    /appendNode\(\s*output,\s*child,\s*childIndex,\s*semanticPath \+ "\/" \+ childIndex,\s*refreshThisSubtree\s*\)/,
  );
  assert.match(source, /attribute\(output, "state-description", string\(node\.getStateDescription\(\)\)\);/);
});

test(
  "Android restart discards the stopped process's semantic tree before accepting startup",
  async () => {
    const stale = {
      "resource-id": "parity:app-process:old",
      "semantic-path": "0/0/4",
      bounds: "[0,0][100,100]",
    };
    const fresh = {
      "resource-id": "parity:app-process:new",
      "semantic-path": "0/0/1",
      bounds: "[0,0][100,100]",
    };
    const beforeStart = [stale, null, null, null];
    const afterStart = [fresh, fresh];
    const operatingSystemStopped = [false, true, true];
    const events = [];
    let started = false;
    const readProcessNode = async () => (started ? afterStart.shift() : beforeStart.shift());

    const observed = await restartAndroidAppAcrossSemanticLifecycle({
      stopApp: async () => events.push("stop"),
      prepareSemanticDriver: async () => events.push("prepare"),
      startApp: async () => {
        assert.deepEqual(beforeStart, []);
        assert.deepEqual(operatingSystemStopped, []);
        started = true;
        events.push("start");
      },
      readProcessNode,
      readStoppedState: async () =>
        (await readProcessNode()) === null && operatingSystemStopped.shift(),
      timeoutMs: 1_000,
      intervalMs: 0,
    });

    assert.deepEqual(events, ["stop", "prepare", "start"]);
    assert.deepEqual(observed, fresh);
    assert.deepEqual(afterStart, []);
  },
);

test("cloud journeys use host time while deterministic data journeys use fixture time", () => {
  assert.equal(androidJourneyEpochMs("shared.cloud-crossfill", 100, 200), 200);
  assert.equal(androidJourneyEpochMs("shared.replay-track-up", 100, 200), 100);
});

test("Android semantic actions wait for their rendered surface to reach the screen", () => {
  const button = {
    enabled: "true",
    clickable: "true",
    visible: "true",
    "center-reachable": "false",
  };
  assert.equal(androidSemanticNodeIsActionable(button), false);
  assert.equal(androidSemanticNodeIsActionable({
    ...button,
    "center-reachable": "true",
  }), true);
});

test("Android runtime startup rejects a semantic tree whose surface is not reachable", () => {
  const state = { ready: "true", disclaimer_required: "false", page: "Map" };
  const home = {
    enabled: "true",
    clickable: "true",
    visible: "true",
    "center-reachable": "false",
  };
  assert.equal(androidInteractiveRuntime(state, home), null);
  assert.deepEqual(androidInteractiveRuntime(state, {
    ...home,
    "center-reachable": "true",
  }), {
    state,
    home: { ...home, "center-reachable": "true" },
  });
});

test("detects only a rendered Android input-method window", () => {
  assert.equal(androidImeVisible(
    '<hierarchy><node package="com.android.inputmethod.latin" class="android.inputmethodservice.KeyboardView" /></hierarchy>',
  ), true);
  assert.equal(androidImeVisible(
    '<hierarchy><node package="org.aerobag.app" class="android.widget.EditText" /></hierarchy>',
  ), false);
});

function anrDialogXml(title, { waitEnabled = true } = {}) {
  return `<hierarchy>
    <node text="${title}" resource-id="android:id/alertTitle" package="android" />
    <node text="Close app" resource-id="android:id/aerr_close" package="android" enabled="true" />
    <node text="Wait" resource-id="android:id/aerr_wait" package="android" enabled="${waitEnabled}" bounds="[70,1300][1010,1426]" />
  </hierarchy>`;
}

test("parses UIAutomator single-quoted attributes containing JSON", () => {
  const tag = 'parity:tray-option:{"procedure_id":"I32R","enroute_transition":"OVR"}';
  const xml = `<hierarchy><node resource-id='${tag}' class="android.view.View" /></hierarchy>`;
  assert.equal(findNode(xml, (node) => node["resource-id"] === tag)?.class, "android.view.View");
});

test("distinguishes the offline package bootstrap from its navigable runtime page", () => {
  const packagePanel = '<node resource-id="parity:offline-packages-panel" />';
  assert.equal(androidRuntimeUiVisible(`<hierarchy>${packagePanel}</hierarchy>`), false);
  const runtimeBehindPackageGate =
    `<hierarchy>${packagePanel}<node resource-id="parity:primary-navigation" /></hierarchy>`;
  assert.equal(androidRuntimeUiVisible(runtimeBehindPackageGate), true);
  assert.equal(androidOfflinePackagesVisible(runtimeBehindPackageGate), true);
  assert.equal(androidRuntimeReadyForJourney(runtimeBehindPackageGate), false);
  assert.equal(androidRuntimeReadyForJourney(
    '<hierarchy><node resource-id="parity:primary-navigation" /></hierarchy>',
  ), true);
});

test("parses explicit Android startup acknowledgements", () => {
  const xml = `<hierarchy><node resource-id="parity:startup-state:ready:true:disclaimer_required:false:persisted_page:Settings" /></hierarchy>`;
  assert.deepEqual(androidStartupState(xml), {
    ready: "true",
    disclaimer_required: "false",
    persisted_page: "Settings",
  });
});

test("disclaimer completion can observe core state before full startup readiness", () => {
  const xml = `<hierarchy><node resource-id="parity:startup-state:ready:false:disclaimer_required:false:persisted_page:Map" /></hierarchy>`;
  assert.deepEqual(androidStartupProjection(xml), {
    ready: "false",
    disclaimer_required: "false",
    persisted_page: "Map",
  });
  assert.equal(androidStartupState(xml), null);
});

test("vertical scrolling skips flight-plan horizontal scrollers", () => {
  const xml = `<hierarchy>
    <node class="android.widget.HorizontalScrollView" package="org.aerobag.app" scrollable="true" bounds="[388,143][1065,243]" />
    <node resource-id="parity:plan-list" class="android.view.View" package="org.aerobag.app" scrollable="true" bounds="[89,248][1065,2014]" />
  </hierarchy>`;
  assert.equal(findVerticalScrollSurface(xml)?.["resource-id"], "parity:plan-list");
});

test("horizontal scrolling recognizes an untagged Altitude Planner control strip", () => {
  const xml = `<hierarchy>
    <node class="android.widget.HorizontalScrollView" package="org.aerobag.app" scrollable="true" bounds="[15,208][1065,355]" />
    <node resource-id="parity:altitude-comparison-panel" class="android.view.View" package="org.aerobag.app" scrollable="true" bounds="[15,620][1065,1650]" />
  </hierarchy>`;
  assert.equal(findHorizontalScrollSurface(xml)?.class, "android.widget.HorizontalScrollView");
});

test("vertical scrolling rejects controls clipped beyond the list viewport", () => {
  const xml = `<hierarchy>
    <node class="android.view.View" package="org.aerobag.app" scrollable="true" bounds="[59,274][1021,2117]" />
    <node resource-id="parity:settings-toggle:debug_tile_labels" package="org.aerobag.app" bounds="[59,2058][1021,2164]" />
    <node resource-id="parity:settings-toggle:debug_nexrad_tile_labels" package="org.aerobag.app" bounds="[59,2164][1021,2180]" />
  </hierarchy>`;
  assert.equal(
    verticalScrollTargetIsReachable(xml, "parity:settings-toggle:debug_tile_labels"),
    true,
  );
  assert.equal(
    verticalScrollTargetIsReachable(xml, "parity:settings-toggle:debug_nexrad_tile_labels"),
    false,
  );
});

test("recognizes an Aerobag ANR without dismissing it", () => {
  assert.equal(
    findAerobagAnrDialog(anrDialogXml("Aerobag isn't responding"))?.text,
    "Aerobag isn't responding",
  );
  assert.equal(findAerobagAnrDialog(anrDialogXml("Another app isn't responding")), null);
});

test("fails a journey when Aerobag has an ANR dialog even if its UI remains visible", () => {
  const xml = `${anrDialogXml("Aerobag isn't responding")}<node package="org.aerobag.app" />`;
  assert.throws(() => assertNoAerobagAnr(xml), /Aerobag ANR detected/);
});

test("classifies only Aerobag fatal, ANR, death, and consumed projection evidence", () => {
  assert.deepEqual(classifyAerobagLogcat(`
08-03 AndroidRuntime E FATAL EXCEPTION: main
08-03 AndroidRuntime E Process: com.android.systemui, PID: 50
08-03 ActivityManager E ANR in com.android.systemui
`), []);
  const evidence = classifyAerobagLogcat(`
08-03 AndroidRuntime E FATAL EXCEPTION: main
08-03 AndroidRuntime E Process: org.aerobag.app, PID: 123
08-03 ActivityManager E ANR in org.aerobag.app (org.aerobag.app/.MainActivity)
08-03 ActivityTaskManager W Force finishing activity org.aerobag.app/.MainActivity
08-03 AndroidLiveFeeds E prepared notams/v1 projection is unavailable
`);
  assert.equal(evidence.length, 4);
  assert.match(evidence.join("\n"), /FATAL EXCEPTION/);
  assert.match(evidence.join("\n"), /ANR in org\.aerobag\.app/);
  assert.match(evidence.join("\n"), /Force finishing/);
  assert.match(evidence.join("\n"), /projection is unavailable/);
});

test("extracts real bounds and an ordered rendered plan signature", () => {
  const xml = `<hierarchy>
    <node package="org.aerobag.app" bounds="[0,24][1920,1080]" content-desc="parity:plan-state:rows:3:active:row-b:from:row-a:to:row-b" />
    <node package="org.aerobag.app" bounds="[20,100][400,180]" content-desc="parity:plan-row:row-a" />
    <node package="org.aerobag.app" bounds="[30,110][200,160]" text="KRNT" />
    <node package="org.aerobag.app" bounds="[20,200][400,280]" content-desc="parity:plan-row:row-b" />
    <node package="org.aerobag.app" bounds="[30,210][200,260]" text="KPWT" />
  </hierarchy>`;
  assert.deepEqual(displayBoundsFromXml(xml), {
    left: 0, top: 24, right: 1920, bottom: 1080, width: 1920, height: 1056,
  });
  assert.deepEqual(renderedFlightPlanSignature(xml), {
    rowCount: 3,
    stateTag: "parity:plan-state:rows:3:active:row-b:from:row-a:to:row-b",
    rows: [
      { tag: "parity:plan-row:row-a", label: "KRNT" },
      { tag: "parity:plan-row:row-b", label: "KPWT" },
    ],
  });
});

test("destination centering rejects stale trays and geographically displaced results", () => {
  const selectionXml = (airport, selected, offsetPx) => `<hierarchy>
    <node content-desc="parity:map-selection-tray" />
    <node content-desc="parity:map-selection-item:airport-${airport}" />
    <node content-desc="parity:map-selection-selected:${selected}" />
    <node content-desc="parity:map-selection-center:${selected}:offset-px:${offsetPx}" />
  </hierarchy>`;

  assert.equal(destinationCenterEvidence(selectionXml("KUKI", "KUKI", 0), "KPLU").matched, false);
  assert.equal(destinationCenterEvidence(selectionXml("KPLU", "KPLU", 120), "KPLU").matched, false);
  assert.deepEqual(destinationCenterEvidence(selectionXml("KPLU", "KPLU", 3), "KPLU"), {
    matched: true,
    airportItemTag: "parity:map-selection-item:airport-KPLU",
    selectedTag: "parity:map-selection-selected:KPLU",
    probeTag: "parity:map-selection-center:KPLU:offset-px:3",
    offsetPx: 3,
  });
});

test("maps core layer IDs to Android's exported parity tags", () => {
  const xml = `<hierarchy>
    <node resource-id="parity:tray-option:TerrainWarning" checked="true" />
    <node resource-id="parity:tray-option:Nexrad" checked="false" />
  </hierarchy>`;

  assert.equal(layerToggleTag("terrain_warning"), "parity:tray-option:TerrainWarning");
  assert.equal(layerToggleTag("nexrad"), "parity:tray-option:Nexrad");
  assert.equal(layerToggleNode(xml, "terrain_warning")?.checked, "true");
  assert.equal(layerToggleNode(xml, "nexrad")?.checked, "false");
  assert.throws(() => layerToggleTag("unknown"), /unsupported E2E map layer/);
});
