// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  assertNoAerobagAnr,
  androidImeShownFromDumpsys,
  androidImeVisible,
  androidInteractiveRuntime,
  androidOfflinePackagesVisible,
  androidRuntimeReadyForJourney,
  androidRuntimeUiVisible,
  androidStartupProjection,
  androidStartupState,
  androidJourneyEpochMs,
  androidSemanticNodeIsActionable,
  classifyAndroidRendererFailure,
  classifyAerobagLogcat,
  displayBoundsFromXml,
  destinationCenterEvidence,
  destinationCenterProjectionEvidence,
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

test("recovers only an entirely black screenshot with Android HWUI disabled", () => {
  assert.equal(classifyAndroidRendererFailure({
    drawingEnabled: "0",
    maxChannel: 0,
  }), true);
  assert.equal(classifyAndroidRendererFailure({
    drawingEnabled: "1",
    maxChannel: 0,
  }), false);
  assert.equal(classifyAndroidRendererFailure({
    drawingEnabled: "0",
    maxChannel: 1,
  }), false);
  assert.equal(classifyAndroidRendererFailure({
    drawingEnabled: "0",
    maxChannel: Number.NaN,
  }), false);
});

test("Android renderer recovery is bounded to one retry per journey repetition", () => {
  const source = readFileSync(new URL("release_journey_lab.sh", import.meta.url), "utf8");
  const repetitions = source.slice(
    source.indexOf("run_repetitions()"),
    source.indexOf("android_renderer_recover()"),
  );
  assert.match(repetitions, /renderer_retry=0/);
  assert.match(
    repetitions,
    /\[\[ "\$renderer_retry" == "0" \]\] && android_renderer_recover/,
  );
  assert.match(repetitions, /renderer_retry=1[\s\S]*?continue/);
  assert.match(repetitions, /return "\$status"/);
});

test("Android emulator readiness enables real HWUI drawing", () => {
  const launcher = readFileSync(
    new URL("../../ui/android-app/scripts/start_emulator_stack.sh", import.meta.url),
    "utf8",
  );
  assert.match(
    launcher,
    /setprop debug\.hwui\.drawing_enabled 1/,
    "ATD emulators must not qualify an all-black UI through semantics alone",
  );
  assert.match(
    launcher,
    /getprop debug\.hwui\.drawing_enabled[\s\S]*?!= "1"/,
    "emulator readiness must verify that HWUI accepted the setting",
  );
});

test("Android physical taps use bounded preflight and exactly one delivered touch", () => {
  const harness = readFileSync(new URL("android-harness.mjs", import.meta.url), "utf8");
  const click = harness.slice(
    harness.indexOf("export function clickAndroidSemanticNode"),
    harness.indexOf("function androidPhysicalTapTarget"),
  );
  assert.match(click, /for \(let attempt = 0; attempt < 4; attempt \+= 1\)/);
  assert.match(click, /waitForAndroidSemanticEvent\(serial, 250\)/);
  assert.match(click, /if \(!refreshed\) continue/);
  assert.equal(
    click.match(/"shell", "input", "tap"/g)?.length,
    1,
    "preflight retries must never duplicate the physical tap",
  );
  assert.match(click, /awaitAndroidPhysicalTouch[\s\S]*?return true/);
  assert.match(click, /physical tap was not received/);

  const activity = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt",
    import.meta.url,
  ), "utf8");
  const dispatch = activity.slice(
    activity.indexOf("override fun dispatchTouchEvent"),
    activity.indexOf("override fun dispatchKeyEvent"),
  );
  assert.match(dispatch, /val handled = super\.dispatchTouchEvent\(event\)/);
  assert.match(dispatch, /event\.actionMasked == MotionEvent\.ACTION_UP/);
  assert.match(dispatch, /E2eProjectionRegistry\.publishTouchReceipt/);

  const indexedControl = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionView.kt",
    import.meta.url,
  ), "utf8");
  assert.match(indexedControl, /awaitPointerEvent\(PointerEventPass\.Initial\)/);
  assert.match(indexedControl, /!it\.previousPressed && it\.pressed/);
  assert.match(indexedControl, /publishTouchReceipt\(semanticTag = semanticTag\)/);

  const service = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  assert.match(service, /case "\/await-touch"/);
  assert.match(service, /receipt\.sequence > sequence && receipt\.handled/);
  assert.match(service, /expectedBounds\.contains\(receipt\.rawX, receipt\.rawY\)/);
  assert.match(service, /ProviderProjection projection = providerProjection\(tag, true\)/);
  const providerTapStart = service.indexOf(
    'if (semanticPath.startsWith("projection-provider:"))',
  );
  const providerTap = service.slice(
    providerTapStart,
    service.indexOf('AccessibilityNodeInfo node = resolveRenderedNode', providerTapStart),
  );
  assert.doesNotMatch(providerTap, /resolveRenderedNode|AccessibilityNodeInfo/);
  assert.match(providerTap, /return currentBounds/);
  assert.match(service, /getRootInActiveWindow\(\)/);
  assert.match(service, /findRenderedNodeAtPoint\([\s\S]*semanticPath\.startsWith\("projection-provider:"\)/);

  const mapExplorer = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt",
    import.meta.url,
  ), "utf8");
  const airportInfoFact = mapExplorer.slice(
    mapExplorer.indexOf("private fun AirportInfoFact("),
    mapExplorer.indexOf("private fun AirportRunwayDiagram("),
  );
  const mapSelectionItemButton = mapExplorer.slice(
    mapExplorer.indexOf("internal fun MapSelectionItemButton("),
    mapExplorer.indexOf("internal fun MapSelectionItemIcon("),
  );
  assert.match(mapExplorer, /semanticTag = fact\.actionId\?\.let[\s\S]*airport-info-time-toggle/);
  assert.match(airportInfoFact, /e2eIndexedControl/);
  assert.match(airportInfoFact, /testTag\(semanticTag\)/);
  assert.match(mapSelectionItemButton, /e2eIndexedControl\([\s\S]*semanticTag = testTag/);
  assert.match(mapSelectionItemButton, /testTag\(testTag\)[\s\S]*clickable/);
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

test("reads Android's explicit input-method visibility state", () => {
  assert.equal(androidImeShownFromDumpsys("Input method client state:\n  mInputShown=true\n"), true);
  assert.equal(androidImeShownFromDumpsys("Input method client state:\n  mInputShown=false\n"), false);
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

  const projection = (state) => [{ state }];
  assert.equal(destinationCenterProjectionEvidence(
    projection("selected:KUKI:category:airport:text:KUKI:centered:KUKI:offset-px:0"),
    "KPLU",
  ).matched, false);
  assert.equal(destinationCenterProjectionEvidence(
    projection("selected:KPLU:category:navaid:text:KPLU:centered:KPLU:offset-px:0"),
    "KPLU",
  ).matched, false);
  assert.equal(destinationCenterProjectionEvidence(
    projection("selected:KPLU:category:airport:text:KPLU:centered:KPLU:offset-px:120"),
    "KPLU",
  ).matched, false);
  assert.deepEqual(destinationCenterProjectionEvidence(
    projection("selected:KPLU:category:airport:text:KPLU:centered:KPLU:offset-px:3:detail:none"),
    "KPLU",
  ), {
    matched: true,
    selected: "KPLU",
    category: "airport",
    centered: "KPLU",
    probeTag: "parity:map-selection-center:KPLU:offset-px:3",
    offsetPx: 3,
  });
  assert.equal(destinationCenterProjectionEvidence(
    projection("selected:KPLU:category:airport:text:KPLU:centered:KPLU:offset-px:3:detail:none:future:value"),
    "KPLU",
  ).matched, true);
  assert.equal(destinationCenterProjectionEvidence(
    projection("selected:KPLU:category:airport:text:KPLU:centered:KPLU:offset-px::detail:none"),
    "KPLU",
  ).matched, false);
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
