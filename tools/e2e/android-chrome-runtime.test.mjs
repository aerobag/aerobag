// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { CdpSocket, chromeCommandLine, openChromeJourneyPage, prepareChromeDevice, prepareChromeLaunch, waitForChromeEmulatorTheme, waitForLiveFeedStatus } from "./run-android-chrome-livefeed-e2e.mjs";
import { ObservationTimeoutError, TerminalObservationError } from "./transition-contract.mjs";

const googleServiceClients = [
  "com.google.android.youtube",
  "com.google.android.apps.youtube.music",
  "com.google.android.apps.messaging",
  "com.google.android.gm",
  "com.google.android.googlequicksearchbox",
];

function installedPackages(packages) {
  return packages.map(name => `package:${name}\n`).join("");
}

test("disposable Chrome emulator isolates background clients before their Google services dependency", () => {
  const commands = [];
  prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(serial, args) {
    assert.equal(serial, "emulator-test");
    commands.push(args);
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return args.includes("-d")
      ? installedPackages([args.at(-1)])
      : installedPackages([...googleServiceClients, "com.android.chrome", "com.android.systemui"]);
    if (args.includes("dumpsys")) return "android.permission.POST_NOTIFICATIONS: granted=true, flags=[]";
    return "";
  } });
  assert.deepEqual(commands, [
    ["wait-for-device"],
    ["shell", "getprop", "ro.kernel.qemu"],
    ["shell", "pm", "list", "packages", "--user", "0"],
    ...googleServiceClients.flatMap(name => [
      ["shell", "pm", "disable-user", "--user", "0", name],
      ["shell", "pm", "list", "packages", "-d", "--user", "0", name],
    ]),
    ["shell", "pm", "disable-user", "--user", "0", "com.google.android.gms"],
    ["shell", "pm", "list", "packages", "-d", "--user", "0", "com.google.android.gms"],
    ["shell", "pm", "grant", "com.android.chrome", "android.permission.POST_NOTIFICATIONS"],
    ["shell", "dumpsys", "package", "com.android.chrome"],
  ]);
});

test("absent optional clients are skipped and substring package matches are not disabled", () => {
  const disabled = [];
  prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(_serial, args) {
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return args.includes("-d")
      ? installedPackages([args.at(-1)])
      : installedPackages(["com.google.android.youtube.extra", "com.google.android.gm",
        "com.android.chrome", "com.android.systemui"]);
    if (args.includes("disable-user")) disabled.push(args.at(-1));
    if (args.includes("dumpsys")) return "android.permission.POST_NOTIFICATIONS: granted=true, flags=[]";
    return "";
  } });
  assert.deepEqual(disabled, ["com.google.android.gm", "com.google.android.gms"]);
});

test("an unverified client disablement stops before disabling its services or launching Chrome", () => {
  const commands = [];
  assert.throws(() => prepareChromeDevice("emulator-test", { disposableEmulator: true, runAdb(_serial, args) {
    commands.push(args);
    if (args.includes("getprop")) return "1\n";
    if (args.includes("list")) return args.includes("-d")
      ? installedPackages(["com.google.android.youtube.extra", "com.google.android.gms"])
      : installedPackages(["com.google.android.youtube"]);
    return "";
  } }), /com\.google\.android\.youtube.*still enabled/);
  assert.equal(commands.some(args => args.includes("com.google.android.gms")), false);
  assert.equal(commands.some(args => args.includes("grant")), false);
});

test("ordinary device runs leave Google services alone", () => {
  const commands = [];
  prepareChromeDevice("tablet", { runAdb: (_serial, args) => commands.push(args) });
  assert.deepEqual(commands, [["wait-for-device"]]);
});

function disposableSetupAdb(_serial, args) {
  if (args.includes("getprop")) return "1\n";
  if (args.includes("list")) return "package:com.google.android.gms\n";
  if (args.includes("dumpsys")) return "android.permission.POST_NOTIFICATIONS: granted=true, flags=[]";
  return "";
}

test("Chrome launch waits for late emulator theme publication after unlock", async () => {
  let completeTheme;
  const theme = new Promise(resolve => { completeTheme = resolve; });
  const calls = [];
  let prepared = false;
  const preparing = prepareChromeLaunch("emulator-test", {
    disposableEmulator: true, runAdb: disposableSetupAdb,
    wake: () => calls.push("unlock"),
    waitForTheme: async () => { calls.push("theme"); await theme; },
  }).then(() => { prepared = true; });
  await Promise.resolve();
  assert.deepEqual(calls, ["unlock", "theme"]);
  assert.equal(prepared, false, "the browser must not launch while Android can still recreate it");
  completeTheme();
  await preparing;
  assert.equal(prepared, true);
});

test("failed theme readiness stops setup instead of launching or retrying Chrome", async () => {
  await assert.rejects(prepareChromeLaunch("emulator-test", {
    disposableEmulator: true, runAdb: disposableSetupAdb, wake() {},
    waitForTheme: async () => { throw new Error("theme publication failed"); },
  }), /theme publication failed/);
});

test("ordinary devices do not require the disposable emulator theme contract", async () => {
  await prepareChromeLaunch("tablet", {
    runAdb() {}, wake() {}, waitForTheme: () => assert.fail("must not inspect personal-device themes"),
  });
});

const completeTheme = "mIsMonetEnabled=true\nmNeedsOverlayCreation=false\nmDeferredThemeEvaluation=false\n";
const publishedOverlays = "android\n[x] com.android.systemui:neutral\n[ ] com.android.systemui:accent\n[x] com.android.systemui:dynamic\n";
const drainedTheme = "Loopers drained!\nTest barrier passed\nFinished application barriers!\n";

test("theme readiness requires producer completion, exact registered overlays and notification delivery", async () => {
  const snapshots = [
    ["", "android\n"],
    [completeTheme.replace("mNeedsOverlayCreation=false", "mNeedsOverlayCreation=true"), publishedOverlays],
    [completeTheme.replace("mDeferredThemeEvaluation=false", "mDeferredThemeEvaluation=true"), publishedOverlays],
    [completeTheme, publishedOverlays.replace(":dynamic", ":dynamic_extra")],
    [completeTheme, publishedOverlays.replace("[x] com.android.systemui:dynamic", "--- com.android.systemui:dynamic")],
    [completeTheme, publishedOverlays],
  ];
  let snapshot = 0;
  let barriers = 0;
  await waitForChromeEmulatorTheme("emulator-test", { intervalMs: 0, timeoutMs: 1000,
    runAdb(_serial, args, options) {
      assert.ok(options.timeout > 0 && options.timeout <= 1000);
      if (args.includes("dumpsys")) return snapshots[snapshot][0];
      if (args.includes("overlay")) return snapshots[snapshot++][1];
      assert.deepEqual(args, ["shell", "am", "wait-for-broadcast-barrier",
        "--flush-broadcast-loopers", "--flush-application-threads"]);
      assert.equal(snapshot, snapshots.length, "do not flush before publication completes");
      barriers += 1;
      return drainedTheme;
    },
  });
  assert.equal(barriers, 1);
});

test("missing theme publication times out with its state and never flushes or launches", async () => {
  await assert.rejects(waitForChromeEmulatorTheme("emulator-test", { intervalMs: 0, timeoutMs: 15,
    runAdb(_serial, args) {
      assert.equal(args.includes("wait-for-broadcast-barrier"), false);
      return args.includes("dumpsys") ? completeTheme : "android\n";
    },
  }), error => error instanceof ObservationTimeoutError
    && error.diagnostics.last_value.needs_creation === false
    && error.diagnostics.last_value.registered.length === 0);
});

test("theme query and barrier failures remain terminal", async () => {
  for (const failure of ["query", "barrier", "unsupported-barrier", "unfinished-apps"]) {
    let barriers = 0;
    await assert.rejects(waitForChromeEmulatorTheme("emulator-test", { intervalMs: 0, timeoutMs: 1000,
      runAdb(_serial, args) {
        if (args.includes("dumpsys")) {
          if (failure === "query") throw new Error("theme service query failed");
          return completeTheme;
        }
        if (args.includes("overlay")) return publishedOverlays;
        barriers += 1;
        if (failure === "barrier") throw new Error("barrier adb timeout");
        return failure === "unsupported-barrier" ? "Unknown command\n" : "Test barrier passed\n";
      },
    }), /query failed|barrier/);
    assert.equal(barriers, failure === "query" ? 0 : 1);
  }
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
  } }), /com\.google\.android\.gms is still enabled/);
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

test("a failed probe preserves preceding live-feed observations without retrying the failure", async () => {
  const status = { connection: { value: "CONNECTED" }, product_versions: { metars: null } };
  const failure = new Error("CDP timeout: Runtime.evaluate");
  let probes = 0;
  const cdp = { async send() {
    probes++;
    if (probes === 1) return { result: { value: null } };
    if (probes === 2) return { result: { value: status } };
    throw failure;
  } };
  await assert.rejects(
    waitForLiveFeedStatus(cdp, (value) => value?.product_versions?.metars === "v1", 1000, "METAR v1", 0),
    (error) => {
      assert.ok(error instanceof TerminalObservationError);
      assert.equal(error.cause, failure);
      assert.deepEqual(error.diagnostics.last_value, status);
      assert.deepEqual(error.diagnostics.observations.map(sample => sample.value), [null, status]);
      assert.equal(error.diagnostics.attempts, 3);
      assert.equal(error.diagnostics.last_error, failure.message);
      return true;
    },
  );
  assert.equal(probes, 3);
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
    async launch(serial, url) { calls.push({ method: "launch", url }); return { packageName: "com.android.chrome" }; },
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

test("startup diagnostics attach after blank-page readiness but before the only app navigation", async () => {
  const h = pageSetupHarness();
  let releaseDiagnostics;
  const diagnosticGate = new Promise(resolve => { releaseDiagnostics = resolve; });
  let attached;
  const attaching = new Promise(resolve => { attached = resolve; });
  h.args.beforeNavigate = async () => { attached(); await diagnosticGate; };
  const opening = openChromeJourneyPage(h.args);
  await h.requested;
  h.releaseFrame({ result: { value: { url: "about:blank", visibility: "visible" } } });
  // If the callback is skipped the old implementation navigates and returns.
  assert.equal(await Promise.race([attaching.then(() => "attached"), opening.then(() => "navigated")]), "attached");
  assert.equal(h.calls.filter(c => c.method === "Page.navigate").length, 0);
  releaseDiagnostics();
  await opening;
  assert.equal(h.calls.filter(c => c.method === "Page.navigate").length, 1);
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

test("disposable Chrome waits for origin-scoped location denial before app navigation", async () => {
  const h = pageSetupHarness();
  h.args.disposableEmulator = true;
  h.args.url = "http://fixture.test:8085/path?run=123";
  let acknowledge;
  let requested;
  const permission = new Promise(resolve => { acknowledge = resolve; });
  const request = new Promise(resolve => { requested = resolve; });
  const send = h.cdp.send;
  h.cdp.send = async (method, params) => {
    const result = await send(method, params);
    if (method === "Browser.setPermission") { requested(); await permission; }
    return result;
  };
  h.releaseFrame({ result: { value: { url: "about:blank", visibility: "visible" } } });
  const opening = openChromeJourneyPage(h.args);
  const first = await Promise.race([request.then(() => "permission"), opening.then(() => "navigation")]);
  assert.equal(first, "permission", "unanswered permission policy must block navigation");
  assert.equal(h.calls.some(c => c.method === "Page.navigate"), false);
  assert.deepEqual(h.calls.find(c => c.method === "Browser.setPermission").params, {
    origin: "http://fixture.test:8085", permission: { name: "geolocation" }, setting: "denied",
  });
  acknowledge();
  assert.equal(await opening, h.cdp);
  assert.equal(h.cdp.androidPackageName, "com.android.chrome");
});

test("permission setup errors fail before navigation without retrying", async () => {
  const h = pageSetupHarness();
  h.args.disposableEmulator = true;
  const send = h.cdp.send;
  h.cdp.send = async (method, params) => {
    if (method === "Browser.setPermission") throw new Error("permission setup failed");
    return send(method, params);
  };
  h.releaseFrame({ result: { value: { url: "about:blank", visibility: "visible" } } });
  await assert.rejects(openChromeJourneyPage(h.args), /permission setup failed/);
  assert.equal(h.calls.some(c => c.method === "Page.navigate"), false);
  assert.equal(h.calls.at(-1).method, "close");
});

test("ordinary Chrome runs do not change origin permissions", async () => {
  const h = pageSetupHarness();
  h.releaseFrame({ result: { value: { url: "about:blank", visibility: "visible" } } });
  await openChromeJourneyPage(h.args);
  assert.equal(h.calls.some(c => c.method === "Browser.setPermission"), false);
});

test("CDP failure evidence retains bounded command timings and pending requests without payloads", async () => {
  const socket = new EventEmitter();
  socket.write = () => {};
  const cdp = new CdpSocket(socket);
  function reply(value) {
    const payload = Buffer.from(JSON.stringify(value));
    socket.emit("data", Buffer.concat([Buffer.from([0x81, payload.length]), payload]));
  }
  for (let id = 1; id <= 25; id++) {
    const response = cdp.send("Runtime.evaluate", { expression: "private content" });
    reply({ id, result: {} });
    await response;
  }
  const pending = cdp.send("Runtime.evaluate", { expression: "private content" });
  try {
    const evidence = cdp.diagnostics();
    assert.equal(evidence.commands.length, 20);
    assert.equal(evidence.commands.at(-2).status, "completed");
    assert.deepEqual(evidence.pending_ids, [26]);
    assert.equal(evidence.commands.at(-1).status, "pending");
    assert.ok(evidence.commands.every(c => c.elapsed_ms >= 0 && c.method === "Runtime.evaluate"));
    assert.doesNotMatch(JSON.stringify(evidence), /private content/);
  } finally {
    socket.emit("close");
    await assert.rejects(pending, /CDP socket closed/);
  }
  assert.equal(cdp.diagnostics().commands.at(-1).status, "failed");
  assert.deepEqual(cdp.diagnostics().pending_ids, []);
});

test("an unanswered CDP request still times out and retains that failure in diagnostics", async () => {
  const socket = new EventEmitter();
  socket.write = () => {};
  const cdp = new CdpSocket(socket);
  await assert.rejects(cdp.send("Runtime.evaluate"), /CDP timeout: Runtime.evaluate/);
  const evidence = cdp.diagnostics();
  assert.deepEqual(evidence.pending_ids, []);
  assert.equal(evidence.commands[0].status, "timed_out");
  assert.equal(evidence.command_timeout_ms, 3000);
  socket.emit("close");
  assert.equal(cdp.diagnostics().commands[0].status, "timed_out");
});
