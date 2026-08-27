// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import {
  assertNoAerobagAnr,
  androidImeVisible,
  androidOfflinePackagesVisible,
  androidClearTextCommand,
  androidSelectAllTextCommand,
  androidRuntimeReadyForJourney,
  androidRuntimeUiVisible,
  androidJourneyEpochMs,
  androidTextInputCommands,
  classifyAerobagLogcat,
  displayBoundsFromXml,
  destinationCenterEvidence,
  findHorizontalScrollSurface,
  findVerticalScrollSurface,
  findNode,
  findAerobagAnrDialog,
  findSystemUiAnrWaitButton,
  layerToggleNode,
  layerToggleTag,
  renderedFlightPlanSignature,
  verticalScrollTargetIsReachable,
  verticalScrollGesture,
} from "./android-harness.mjs";

test("plans Android input for a keyboard-safe encoded fixture path", () => {
  assert.deepEqual(
    androidTextInputCommands("/releasejourney/7265706c61792f747261636b2d6761702e6a736f6e"),
    [
      ["shell", "input", "keyevent", "KEYCODE_SLASH"],
      ["shell", "input", "text", "releasejourney"],
      ["shell", "input", "keyevent", "KEYCODE_SLASH"],
      ["shell", "input", "text", "7265706c61792f747261636b2d6761702e6a736f6e"],
    ],
  );
});

test("cloud journeys use host time while deterministic data journeys use fixture time", () => {
  assert.equal(androidJourneyEpochMs("shared.cloud-crossfill", 100, 200), 200);
  assert.equal(androidJourneyEpochMs("shared.replay-track-up", 100, 200), 100);
});

test("clears focused Android text in one ordered keyevent stream", () => {
  assert.deepEqual(
    androidClearTextCommand(3),
    [
      "shell", "input", "keyevent", "KEYCODE_MOVE_END",
      "KEYCODE_DEL", "KEYCODE_DEL", "KEYCODE_DEL",
    ],
  );
});

test("selects the complete Android text value before a verified replacement", () => {
  assert.deepEqual(
    androidSelectAllTextCommand(),
    ["shell", "input", "keycombination", "KEYCODE_CTRL_LEFT", "KEYCODE_A"],
  );
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

test("recognizes the hosted emulator System UI ANR wait action", () => {
  assert.deepEqual(
    findSystemUiAnrWaitButton(anrDialogXml("System UI isn't responding")),
    {
      text: "Wait",
      "resource-id": "android:id/aerr_wait",
      package: "android",
      enabled: "true",
      bounds: "[70,1300][1010,1426]",
    },
  );
});

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

test("vertical scrolling stays below Android's status-bar gesture zone", () => {
  assert.deepEqual(
    verticalScrollGesture({ left: 0, top: 0, right: 1080, bottom: 1200, width: 1080, height: 1200 }, "up"),
    { x: 540, startY: 369, endY: 831 },
  );
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

test("does not hide an Aerobag ANR", () => {
  assert.equal(findSystemUiAnrWaitButton(anrDialogXml("Aerobag isn't responding")), null);
});

test("does not select a disabled System UI wait action", () => {
  assert.equal(
    findSystemUiAnrWaitButton(anrDialogXml("System UI isn't responding", { waitEnabled: false })),
    null,
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

test("scroll searches retain enough viewport overlap to avoid skipping rows", () => {
  const bounds = {
    left: 56,
    top: 261,
    right: 1024,
    bottom: 2120,
    width: 968,
    height: 1859,
  };
  const down = verticalScrollGesture(bounds, "down");
  const up = verticalScrollGesture(bounds, "up");
  const viewportHeight = bounds.height;
  const travel = Math.abs(down.startY - down.endY);

  assert.ok(down.startY > down.endY);
  assert.deepEqual(up, {
    x: down.x,
    startY: down.endY,
    endY: down.startY,
  });
  assert.ok(travel <= viewportHeight * 0.60, `${travel}px scroll exceeded overlap budget`);
  assert.ok(travel >= viewportHeight * 0.40, `${travel}px scroll made too little progress`);
});
