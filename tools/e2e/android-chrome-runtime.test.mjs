// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { CdpSocket, prepareChromeDevice, waitForLiveFeedStatus } from "./run-android-chrome-livefeed-e2e.mjs";
import { ObservationTimeoutError, TerminalObservationError } from "./transition-contract.mjs";

test("disposable Chrome emulator disables and verifies Google services before launch", () => {
  const commands = [];
  prepareChromeDevice("emulator-test", { isolateGoogleServices: true, runAdb(serial, args) {
    assert.equal(serial, "emulator-test");
    commands.push(args);
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return "package:com.google.android.gms\n";
    return "";
  } });
  assert.deepEqual(commands, [
    ["wait-for-device"],
    ["shell", "getprop", "ro.kernel.qemu"],
    ["shell", "pm", "disable-user", "--user", "0", "com.google.android.gms"],
    ["shell", "pm", "list", "packages", "-d", "--user", "0", "com.google.android.gms"],
  ]);
});

test("ordinary device runs leave Google services alone", () => {
  const commands = [];
  prepareChromeDevice("tablet", { runAdb: (_serial, args) => commands.push(args) });
  assert.deepEqual(commands, [["wait-for-device"]]);
});

test("explicit isolation refuses a physical tablet before changing any packages", () => {
  const commands = [];
  assert.throws(() => prepareChromeDevice("tablet", { isolateGoogleServices: true, runAdb(_serial, args) {
    commands.push(args);
    return "0\n";
  } }), /requires a disposable emulator/);
  assert.equal(commands.length, 2);
});

test("an unverified Google service disablement stops setup", () => {
  assert.throws(() => prepareChromeDevice("emulator-test", { isolateGoogleServices: true, runAdb(_serial, args) {
    return args.includes("getprop") ? "1\n" : "package:com.google.android.gms.supervision\n";
  } }), /Google Play Services is still enabled/);
});

test("an adb error during isolation is fatal, not a reason to run unisolated", () => {
  const failure = new Error("package manager unavailable");
  assert.throws(() => prepareChromeDevice("emulator-test", { isolateGoogleServices: true, runAdb(_serial, args) {
    if (args.includes("disable-user")) throw failure;
    return "1\n";
  } }), (error) => error === failure);
});

test("browser death while waiting for METARs is terminal, not an absent status", async () => {
  const socket = new EventEmitter();
  let writes = 0;
  socket.write = () => {
    writes += 1;
    queueMicrotask(() => {
      socket.destroyed = true;
      socket.emit("close");
    });
  };
  const cdp = new CdpSocket(socket);
  let probes = 0;
  const client = { send(...args) { probes += 1; return cdp.send(...args); } };
  await assert.rejects(
    waitForLiveFeedStatus(client, (status) => status?.product_versions?.metars === "v1", 1000, "initial METAR v1", 0),
    (error) => {
      assert.ok(error instanceof TerminalObservationError);
      assert.match(error.message, /initial METAR v1.*CDP socket closed/);
      return true;
    },
  );
  assert.equal(probes, 1, "a dead browser must not be polled again");
  assert.equal(writes, 1);
  assert.equal(cdp.pending.size, 0);
});

test("valid pending status remains observable until the expected METAR arrives", async () => {
  const statuses = [null, { product_versions: {} }, { product_versions: { metars: "v1" } }];
  let probes = 0;
  const cdp = { async send() { return { result: { value: statuses[probes++] } }; } };
  const result = await waitForLiveFeedStatus(cdp, (status) => status?.product_versions?.metars === "v1", 1000, "initial METAR v1", 0);
  assert.equal(result.product_versions.metars, "v1");
  assert.equal(probes, 3);
});

test("page evaluation errors are not retried as missing live feeds", async () => {
  let probes = 0;
  const cdp = { async send() { probes += 1; return { exceptionDetails: { text: "hook exploded" } }; } };
  await assert.rejects(
    waitForLiveFeedStatus(cdp, Boolean, 1000, "live feeds", 0),
    (error) => error instanceof TerminalObservationError && /hook exploded/.test(error.message),
  );
  assert.equal(probes, 1);
});

test("a real live-feed timeout preserves the wrong state in diagnostics", async () => {
  const status = { connection: { value: "CONNECTED" }, product_versions: { metars: "v2" } };
  const cdp = { async send() { return { result: { value: status } }; } };
  await assert.rejects(
    waitForLiveFeedStatus(cdp, (value) => value?.product_versions?.metars === "v3", 100, "METAR v3", 10),
    (error) => {
      assert.ok(error instanceof ObservationTimeoutError);
      assert.deepEqual(error.diagnostics.last_value, status);
      return true;
    },
  );
});
