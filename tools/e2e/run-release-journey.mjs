#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  chromeProcessDiagnostics, connectToBrowser, launchChrome, stopProcess,
} from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { journeyById } from "./release-journey-registry.mjs";
import { loadReleaseJourneyFixture } from "./release-journey-fixture.mjs";
import { releaseJourneyImplementation } from "./release-journey-implementations.mjs";
import {
  executeReleaseJourney, persistJourneyResult, summarizeFixtureRequests,
} from "./release-journey-runtime.mjs";
import { WebSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { E2E_TIMING } from "./transition-contract.mjs";
import { advancingVirtualClockScript } from "./virtual-clock.mjs";
import { recreateWebJourneyPage, WebSemanticTransport } from "./web-semantic-transport.mjs";

const workerErrorCaptureScript = String.raw`
(() => {
  const NativeWorker = globalThis.Worker;
  if (typeof NativeWorker !== "function") return;
  const errors = [];
  const record = (value) => {
    errors.push(value);
    if (errors.length > 50) errors.splice(0, errors.length - 50);
  };
  globalThis.__aerobagE2eWorkerErrors = errors;
  globalThis.Worker = class E2eObservedWorker extends NativeWorker {
    constructor(...args) {
      super(...args);
      this.addEventListener("message", (event) => {
        const message = event.data;
        if (message?.kind !== "response" || message.ok !== false) return;
        record({
          kind: "response-error",
          id: message.id,
          error: message.error,
          captured_at_ms: performance.now(),
        });
      });
      this.addEventListener("error", (event) => {
        record({
          kind: "worker-error",
          error: { message: event.message, filename: event.filename, lineno: event.lineno },
          captured_at_ms: performance.now(),
        });
      });
      this.addEventListener("messageerror", () => {
        record({ kind: "message-error", captured_at_ms: performance.now() });
      });
    }
  };
})();`;

function parseArgs(values) {
  const result = {
    platform: "web",
    url: process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/",
    fixture: process.env.AEROBAG_RELEASE_JOURNEY_FIXTURE ?? "",
    fixtureOrigin: process.env.AEROBAG_RELEASE_JOURNEY_ORIGIN ?? "",
    artifactDir: process.env.AEROBAG_E2E_ARTIFACT_DIR ?? "",
    width: 1000,
    height: 900,
  };
  for (let index = 0; index < values.length; index += 1) {
    const argument = values[index];
    if (argument === "--platform") result.platform = values[++index];
    else if (argument === "--journey") result.journey = values[++index];
    else if (argument === "--url") result.url = values[++index];
    else if (argument === "--fixture") result.fixture = values[++index];
    else if (argument === "--fixture-origin") result.fixtureOrigin = values[++index];
    else if (argument === "--artifact-dir") result.artifactDir = values[++index];
    else if (argument === "--width") result.width = Number(values[++index]);
    else if (argument === "--height") result.height = Number(values[++index]);
    else throw new Error(`unknown argument ${argument}`);
  }
  return result;
}

const args = parseArgs(process.argv.slice(2));
if (!args.journey) throw new Error("--journey is required");
if (!args.fixture) throw new Error("--fixture is required");
if (args.platform !== "web") {
  throw new Error("the standalone release journey runner currently launches web; Android is hosted by run-android-e2e-suite.mjs");
}
const journey = journeyById(args.journey);
if (!journey) throw new Error(`unknown release journey ${args.journey}`);
if (!journey.platforms.includes(args.platform)) {
  throw new Error(`${args.journey} does not run on ${args.platform}`);
}
const implementation = releaseJourneyImplementation(args.journey);
if (!implementation) throw new Error(`${args.journey} has no implemented release journey`);
const fixture = loadReleaseJourneyFixture(args.fixture);
const artifactDir = args.artifactDir || join(process.cwd(), "test-results", args.journey, args.platform);
await mkdir(artifactDir, { recursive: true });
const explicitNetLog = process.env.AEROBAG_CHROME_NET_LOG;
const retainNetLog = Boolean(explicitNetLog) || process.env.AEROBAG_E2E_RETAIN_NET_LOG === "1";
const netLogPath = explicitNetLog?.replace("{repeat}", process.env.AEROBAG_E2E_REPEAT_INDEX ?? "1")
  || join(artifactDir, "chrome-netlog.json");
const userDataDir = await mkdtemp(join(tmpdir(), "aerobag-release-journey-"));
let passed = false;
let chrome;
let browser;
let transport;
let page;
let phase = "chrome.launch";
let runnerFailure;
const persistRunnerFailure = async () => {
  await writeFile(join(artifactDir, "runner-failure.json"), `${JSON.stringify(runnerFailure, null, 2)}\n`)
    .catch((error) => console.error(`Could not retain runner failure diagnostics: ${error.message}`));
};

try {
  chrome = await launchChrome({ userDataDir, width: args.width, height: args.height, netLogPath });
  phase = "chrome.connect";
  browser = await connectToBrowser(chrome.endpoint);
  phase = "page.configure";
  const configurePage = async (configuredPage) => {
    await configuredPage.send("Page.enable");
    await configuredPage.send("Runtime.enable");
    await configuredPage.send("Log.enable");
    // Use Chrome's netlog for passive page and worker network diagnostics;
    // don't enable the DevTools Network domain just to observe qualification.
    await configuredPage.send("Page.addScriptToEvaluateOnNewDocument", {
      source: workerErrorCaptureScript,
    });
    if (journey.id !== "shared.cloud-crossfill") {
      await configuredPage.send("Page.addScriptToEvaluateOnNewDocument", {
        source: advancingVirtualClockScript(fixture.capabilities.reference_epoch_ms),
      });
    }
    await configuredPage.send("Emulation.setDeviceMetricsOverride", {
      width: args.width,
      height: args.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    const cpuThrottleRate = Number(process.env.AEROBAG_E2E_CPU_THROTTLE_RATE ?? 1);
    if (Number.isFinite(cpuThrottleRate) && cpuThrottleRate > 1) {
      await configuredPage.send("Emulation.setCPUThrottlingRate", { rate: cpuThrottleRate });
    }
    page = configuredPage;
    return configuredPage;
  };
  page = await configurePage(await browser.createPage());
  transport = new WebSemanticTransport(page, {
    url: args.url,
    recreatePage: (previousPage, options) =>
      recreateWebJourneyPage(browser, previousPage, configurePage, options),
  });
  const driver = new WebSemanticJourneyDriver(transport);
  phase = "journey.execute";
  const result = await executeReleaseJourney(
    {
      journey,
      platform: "web",
      driver,
      fixture,
      fixtureOrigin: args.fixtureOrigin || new URL(args.url).origin,
      artifactDir,
    },
    implementation,
  );
  if (page.diagnostics.some((entry) => entry.method === "Runtime.exceptionThrown")) {
    throw new Error(`browser exceptions observed: ${JSON.stringify(page.diagnostics.slice(-10))}`);
  }
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  passed = true;
} catch (error) {
  // Startup failures precede executeReleaseJourney and have no journeyResult.
  // Save process evidence before teardown, without masking the initiating error.
  chrome ??= error.chrome;
  runnerFailure = {
    phase,
    error: { message: error?.message ?? String(error), stack: error?.stack ?? null },
    chrome: chromeProcessDiagnostics(chrome),
    net_log: netLogPath,
  };
  await persistRunnerFailure();
  if (error?.journeyResult) {
    const snapshot = await transport?.snapshot().catch((snapshotError) => ({ error: snapshotError.message }));
    const fixtureRequests = await fetch(new URL("/__requests", args.fixtureOrigin || args.url))
      .then((response) => response.ok ? response.json() : [])
      .catch(() => []);
    if (snapshot && "test_ids" in snapshot) delete snapshot.test_ids;
    error.journeyResult.diagnostics.browser = {
      page: snapshot,
      net_log: netLogPath,
      chrome_stderr: chrome?.getStderr?.() ?? "",
      // Preserve the initiating failure as well as teardown cancellations.
      events: page?.diagnostics.slice(-200) ?? [],
      worker_errors: await page?.evaluate(
        "globalThis.__aerobagE2eWorkerErrors ?? []",
      ).catch(() => []) ?? [],
      fixture_requests: summarizeFixtureRequests(fixtureRequests),
    };
    persistJourneyResult(error.journeyResult, artifactDir);
  }
  throw error;
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  if (runnerFailure) {
    // A failed pipe write can precede stderr delivery and the child exit event.
    // Keep the original snapshot and also retain evidence drained in teardown.
    runnerFailure.chrome_after_teardown = chromeProcessDiagnostics(chrome);
    await persistRunnerFailure();
  }
  // Network evidence includes worker fetches without attaching a debugger to
  // the worker. Retain failures; successful qualification runs need no netlogs.
  if (passed && !retainNetLog) await rm(netLogPath, { force: true });
  await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
