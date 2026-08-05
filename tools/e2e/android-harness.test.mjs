// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import {
  assertNoAerobagAnr,
  classifyAerobagLogcat,
  displayBoundsFromXml,
  destinationCenterEvidence,
  findAerobagAnrDialog,
  findSystemUiAnrWaitButton,
  renderedFlightPlanSignature,
} from "./android-harness.mjs";

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
