// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { findSystemUiAnrWaitButton } from "./android-harness.mjs";

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
