#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  connectToBrowser, launchChrome, stopProcess,
} from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { journeyById } from "./release-journey-registry.mjs";
import { loadReleaseJourneyFixture } from "./release-journey-fixture.mjs";
import { releaseJourneyImplementation } from "./release-journey-implementations.mjs";
import { executeReleaseJourney } from "./release-journey-runtime.mjs";
import { WebSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { advancingVirtualClockScript, WebSemanticTransport } from "./web-semantic-transport.mjs";

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
const userDataDir = await mkdtemp(join(tmpdir(), "aerobag-release-journey-"));
let chrome;
let browser;

try {
  chrome = await launchChrome({ userDataDir, width: args.width, height: args.height });
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Log.enable");
  if (journey.id !== "shared.cloud-crossfill") {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: advancingVirtualClockScript(fixture.capabilities.reference_epoch_ms),
    });
  }
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: args.width,
    height: args.height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  const transport = new WebSemanticTransport(page, { url: args.url });
  const driver = new WebSemanticJourneyDriver(transport);
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
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
