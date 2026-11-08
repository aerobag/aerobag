// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { CdpSocket, chromeCommandLine, openChromeJourneyPage, prepareChromeDevice, waitForLiveFeedStatus } from "./run-android-chrome-livefeed-e2e.mjs";
import { ObservationTimeoutError, TerminalObservationError } from "./transition-contract.mjs";

test("disposable Chrome emulator disables and verifies Google services before launch", () => {
  const commands = [];
  prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(serial, args) {
    assert.equal(serial, "emulator-test");
    commands.push(args);
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return "package:com.google.android.gms\n";
    if (args.includes("dumpsys")) return "android.permission.POST_NOTIFICATIONS: granted=true, flags=[]";
    return "";
  } });
  assert.deepEqual(commands, [
    ["wait-for-device"],
    ["shell", "getprop", "ro.kernel.qemu"],
    ["shell", "pm", "disable-user", "--user", "0", "com.google.android.gms"],
    ["shell", "pm", "list", "packages", "-d", "--user", "0", "com.google.android.gms"],
    ["shell", "pm", "grant", "com.android.chrome", "android.permission.POST_NOTIFICATIONS"],
    ["shell", "dumpsys", "package", "com.android.chrome"],
  ]);
});

test("ordinary device runs leave Google services alone", () => {
  const commands = [];
  prepareChromeDevice("tablet", { runAdb: (_serial, args) => commands.push(args) });
  assert.deepEqual(commands, [["wait-for-device"]]);
});

test("explicit isolation refuses a physical tablet before changing any packages", () => {
  const commands = [];
  assert.throws(() => prepareChromeDevice("tablet", { disposableEmulator: true, runAdb(_serial, args) {
    commands.push(args);
    return "0\n";
  } }), /requires a disposable emulator/);
  assert.equal(commands.length, 2);
});

test("an unverified Google service disablement stops setup", () => {
  assert.throws(() => prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(_serial, args) {
    return args.includes("getprop") ? "1\n" : "package:com.google.android.gms.supervision\n";
  } }), /Google Play Services is still enabled/);
});

test("an adb error during isolation is fatal, not a reason to run unisolated", () => {
  const failure = new Error("package manager unavailable");
  assert.throws(() => prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(_serial, args) {
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

test("disposable browser setup grants and verifies notification permission for the selected package", () => {
  const commands = [];
  prepareChromeDevice("emulator-test", { disposableEmulator: true, packageName: "com.chrome.beta", runAdb(_serial, args) {
    commands.push(args);
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return "package:com.google.android.gms\n";
    if (args.includes("dumpsys")) return "android.permission.POST_NOTIFICATIONS: granted=true, flags=[]";
    return "";
  } });
  assert.deepEqual(commands.slice(-2), [
    ["shell", "pm", "grant", "com.chrome.beta", "android.permission.POST_NOTIFICATIONS"],
    ["shell", "dumpsys", "package", "com.chrome.beta"],
  ]);
});

test("failed or unverified notification permission stops browser setup", () => {
  for (const mode of ["grant-error", "still-denied", "wrong-permission"]) {
    assert.throws(() => prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(_serial, args) {
      if (args.includes("getprop")) return "1\n";
      if (args.includes("list")) return "package:com.google.android.gms\n";
      if (args.includes("grant") && mode === "grant-error") throw new Error("permission grant failed");
      if (args.includes("dumpsys")) return mode === "still-denied"
        ? "android.permission.POST_NOTIFICATIONS: granted=false, flags=[]"
        : "android.permission.ACCESS_FINE_LOCATION: granted=true, flags=[]";
      return "";
    } }), /permission/i, mode);
  }
});

test("the emulator compositor workaround does not disable hardware rendering on physical devices", () => {
  assert.match(chromeCommandLine({ disposableEmulator: true }), /--disable-features=AndroidSurfaceControl\b/);
  assert.doesNotMatch(chromeCommandLine(), /AndroidSurfaceControl/);
  assert.doesNotMatch(chromeCommandLine({ disposableEmulator: true }), /--disable-gpu(?:\s|$)/);
});

function pageSetupHarness() {
  const calls = [];
  let releaseFrame;
  let frameRequested;
  const frame = new Promise((resolve) => { releaseFrame = resolve; });
  const requested = new Promise((resolve) => { frameRequested = resolve; });
  const cdp = {
    async send(method, params = {}) {
      calls.push({ method, params });
      if (method === "Runtime.evaluate") { frameRequested(); return frame; }
      return method === "Page.navigate" ? { frameId: "main", loaderId: "app" } : {};
    },
    close() { calls.push({ method: "close" }); },
  };
  const args = {
    serial: "emulator-test", cdpPort: 9222, url: "http://fixture.test/",
    async launch(serial, url) { calls.push({ method: "launch", url }); },
    async connect(port, url) { calls.push({ method: "connect", url }); return cdp; },
  };
  return { calls, args, cdp, requested, releaseFrame };
}

test("Aerobag loads exactly once, only after the blank browser page renders", async () => {
  const h = pageSetupHarness();
  const opening = openChromeJourneyPage(h.args);
  // Drain the setup's immediate awaits; this is a controlled held response, not a readiness delay.
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  assert.deepEqual(h.calls.filter(c => c.method === "launch" || c.method === "connect").map(c => c.url),
    ["about:blank", "about:blank"]);
  await h.requested;
  assert.equal(h.calls.filter(c => c.method === "Page.navigate").length, 0);
  const probe = h.calls.find(c => c.method === "Runtime.evaluate").params;
  assert.match(probe.expression, /requestAnimationFrame/);
  assert.equal(probe.awaitPromise, true);
  h.releaseFrame({ result: { value: { url: "about:blank", visibility: "visible" } } });
  assert.equal(await opening, h.cdp);
  assert.deepEqual(h.calls.filter(c => c.method === "Page.navigate").map(c => c.params.url), [h.args.url]);
});

test("bad browser readiness or failed navigation closes CDP without retrying the app", async () => {
  for (const bad of ["hidden", "wrong-page", "exception", "navigation"]) {
    const h = pageSetupHarness();
    if (bad === "navigation") {
      const send = h.cdp.send;
      h.cdp.send = async (...args) => {
        const result = await send(...args);
        return args[0] === "Page.navigate" ? { errorText: "net::ERR_FAILED" } : result;
      };
    }
    h.releaseFrame(bad === "exception" ? { exceptionDetails: { text: "browser probe failed" } } : {
      result: { value: { url: bad === "wrong-page" ? "chrome://welcome" : "about:blank",
        visibility: bad === "hidden" ? "hidden" : "visible" } },
    });
    await assert.rejects(openChromeJourneyPage(h.args), /browser|navigation|net::ERR_FAILED/i);
    assert.equal(h.calls.filter(c => c.method === "Page.navigate").length, bad === "navigation" ? 1 : 0);
    assert.equal(h.calls.filter(c => c.method === "launch").length, 1);
    assert.equal(h.calls.at(-1).method, "close");
  }
});
