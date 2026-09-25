// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { captureAndroidThreadStacks, captureAndroidFailureDiagnostics } from "./android-harness.mjs";

test("thread capture targets only the app on the selected emulator and bounds both commands", () => {
  const calls = [];
  const capture = runInNewContext(`(${captureAndroidThreadStacks})`, {
    ANDROID_PACKAGE: "org.aerobag.app",
    adb: (serial, args, options) => {
      calls.push({ serial, args: [...args], options: { ...options } });
      return calls.length === 1 ? "1234\n" : "blocked owner stack\n";
    },
  });
  assert.equal(capture("emulator-5658"), "blocked owner stack\n");
  assert.deepEqual(calls, [
    { serial: "emulator-5658", args: ["shell", "pidof", "org.aerobag.app"], options: { timeout: 1000 } },
    { serial: "emulator-5658", args: ["shell", "su", "0", "debuggerd", "-b", "1234"],
      options: { timeout: 5000, maxBuffer: 4 * 1024 * 1024 } },
  ]);
  for (const serial of [undefined, "", "10.110.10.164:12345", "usb-tablet"]) {
    assert.match(capture(serial), /not an emulator/);
  }
  assert.equal(calls.length, 2, "physical devices must not receive diagnostic privilege requests");
});

test("missing or ambiguous app processes do not cause an untargeted dump", () => {
  for (const pid of ["", "1234 5678", "not a pid"]) {
    let calls = 0;
    const capture = runInNewContext(`(${captureAndroidThreadStacks})`, {
      ANDROID_PACKAGE: "org.aerobag.app", adb: () => { calls++; return pid; },
    });
    assert.throws(() => capture("emulator-5658"), /Expected one/);
    assert.equal(calls, 1);
  }
});

test("thread capture is first and its failure does not suppress other evidence", () => {
  for (const failure of [null, new Error("dump permission denied"), new Error("dump timed out")]) {
    const events = [], files = new Map();
    const capture = runInNewContext(`(${captureAndroidFailureDiagnostics})`, {
      mkdirSync() {}, join: (...parts) => parts.join("/"),
      writeFileSync: (path, value) => files.set(path, value),
      captureAndroidThreadStacks: () => { events.push("threads"); if (failure) throw failure; return "stacks"; },
      androidSemanticDriverRequestState: () => { events.push("driver"); return "{}"; },
      adb: (_serial, args) => { events.push(args.join(" ")); return "device evidence"; },
      screencapPng: () => { events.push("screen"); return "png"; },
      dumpAndroid: () => { events.push("hierarchy"); return "<hierarchy/>"; },
    });
    const artifacts = capture("emulator-5658", "/evidence", "Settings stalled");
    assert.deepEqual(events.slice(0, 2), ["threads", "driver"]);
    for (const file of ["semantic-driver.json", "logcat.txt", "screenshot.png", "ui.xml", "activity.txt", "window.txt"]) {
      assert.ok(artifacts.includes(`/evidence/${file}`));
    }
    if (failure) assert.match(files.get("/evidence/diagnostic-errors.txt"), /threads.txt: dump/);
    else assert.equal(files.get("/evidence/threads.txt"), "stacks");
  }
});
