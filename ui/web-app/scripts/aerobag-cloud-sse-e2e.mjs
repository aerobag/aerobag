#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";

import {
  connectToBrowser,
  launchChrome,
  stopProcess,
  waitFor,
} from "./chrome-cdp.mjs";

const profileRoot = await mkdtemp(path.join(os.tmpdir(), "aerobag-cloud-sse-e2e-"));
const resources = [];
const requestedUrl = process.argv.find((value) => value.startsWith("http"))
  ?? process.env.AEROBAG_E2E_URL;
const url = requestedUrl
  ?? (process.argv.includes("--spawn-fixture")
    ? await launchDevStackFixture()
    : "http://127.0.0.1:8083/");

try {
  const first = await launchIsolatedPage("first");
  const second = await launchIsolatedPage("second");
  await Promise.all([openCloud(first.page), openCloud(second.page)]);

  await createAerobagCloudAccount(first.page);
  const setupCode = await revealSetupCode(first.page);
  await receiveSetupCode(second.page, setupCode);
  await Promise.all([waitForStream(first.page), waitForStream(second.page)]);

  await openPlan(first.page);
  const flightPlanStartedAt = Date.now();
  await appendRoute(first.page, "KPAE KAPA");
  await waitForCloudState(
    second.page,
    (state) => includesAll(state.flight_plan_rows, ["KPAE", "KAPA"]),
    20_000,
    "second browser did not adopt the flight plan",
  );
  const flightPlanLatencyMs = Date.now() - flightPlanStartedAt;

  const firstPreferences = {
    regions: { nw: "play" },
    products: { terrain: "pause" },
  };
  const preferencesStartedAt = Date.now();
  await setOfflinePackagePreferences(first.page, firstPreferences);
  await waitForCloudState(
    second.page,
    (state) => deepEqual(state.offline_package_preferences, firstPreferences),
    20_000,
    "second browser did not adopt offline-package preferences",
  );
  const preferencesLatencyMs = Date.now() - preferencesStartedAt;

  const streamBeforeDrop = (await cloudState(second.page)).event_stream_id;
  await second.page.evaluate("window.__aerobagE2e.cloud.dropEventStream()");
  const reconnected = await waitForCloudState(
    second.page,
    (state) => state.event_stream_id && state.event_stream_id !== streamBeforeDrop,
    20_000,
    "dropped Aerobag Cloud stream did not reconnect",
  );

  const secondPreferences = {
    regions: { nw: "play", sw: "pause" },
    products: { terrain: "play" },
  };
  const recoveryStartedAt = Date.now();
  await setOfflinePackagePreferences(first.page, secondPreferences);
  await waitForCloudState(
    second.page,
    (state) => deepEqual(state.offline_package_preferences, secondPreferences),
    20_000,
    "reconnected browser lost the subsequent package-preference update",
  );
  const recoveryLatencyMs = Date.now() - recoveryStartedAt;

  process.stdout.write(`${JSON.stringify({
    result: "passed",
    flight_plan_latency_ms: flightPlanLatencyMs,
    package_preferences_latency_ms: preferencesLatencyMs,
    post_reconnect_latency_ms: recoveryLatencyMs,
    stream_before_drop: streamBeforeDrop,
    stream_after_reconnect: reconnected.event_stream_id,
  })}\n`);
} finally {
  for (const resource of resources.reverse()) {
    await resource.browser?.close();
    await stopProcess(resource.chrome?.process ?? resource.process);
  }
  await rm(profileRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}

async function launchDevStackFixture() {
  const repoRoot = process.env.AEROBAG_REPO_ROOT
    ? path.resolve(process.env.AEROBAG_REPO_ROOT)
    : path.resolve(process.cwd(), "../..");
  const [frontDoorPort, cloudPort] = await Promise.all([allocatePort(), allocatePort()]);
  const stackRoot = path.join(profileRoot, "stack");
  const secretPath = path.join(profileRoot, "server-secret.bin");
  const configuredPublicationRoot = path.resolve(
    repoRoot,
    (await readFile(path.join(repoRoot, ".aerobag-artifact-read-path"), "utf8")).trim(),
  );
  const artifactRoot = path.basename(configuredPublicationRoot) === "published"
    ? path.dirname(configuredPublicationRoot)
    : configuredPublicationRoot;
  const uiTargetRoot = process.env.AEROBAG_UI_TARGET_ROOT
    ? path.resolve(process.env.AEROBAG_UI_TARGET_ROOT)
    : path.resolve(repoRoot, (await readFile(path.join(repoRoot, "ui", "target-root.txt"), "utf8")).trim());
  await writeFile(secretPath, randomBytes(32), { mode: 0o600 });
  const args = [
    path.join(repoRoot, "tools", "run_dev_stack.py"),
    "--artifact-root", artifactRoot,
    "--stack-root", stackRoot,
    "--listen", `127.0.0.1:${frontDoorPort}`,
    "--cloud-server-listen", `127.0.0.1:${cloudPort}`,
    "--cloud-server-secret", secretPath,
    "--web-dist", path.join(uiTargetRoot, "web", "dist"),
    "--skip-binary-build",
    "--disable-live-feeds",
    "--disable-build-watch",
    "--disable-pipeline-health",
  ];
  const child = spawn("python3", args, {
    cwd: repoRoot,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let diagnostics = "";
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (chunk) => {
      diagnostics = `${diagnostics}${chunk.toString("utf8")}`.slice(-16_000);
    });
  }
  resources.push({ process: child });
  const baseUrl = `http://127.0.0.1:${frontDoorPort}/`;
  await waitFor(async () => {
    if (child.exitCode !== null) {
      throw new Error(`dev-stack fixture exited ${child.exitCode}: ${diagnostics}`);
    }
    try {
      const response = await fetch(`${baseUrl}health.json`);
      return response.ok;
    } catch {
      return false;
    }
  }, 15_000, "temporary ACS dev stack did not start");
  return baseUrl;
}

function allocatePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        reject(new Error("could not allocate a local test port"));
        return;
      }
      server.close((error) => error ? reject(error) : resolve(address.port));
    });
  });
}

async function launchIsolatedPage(name) {
  const userDataDir = path.join(profileRoot, name);
  const chrome = await launchChrome({ userDataDir });
  const browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  resources.push({ chrome, browser });
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.navigate(url);
  await page.waitForLoad();
  await waitFor(async () => {
    const state = await page.evaluate(`(() => ({
      disclaimer: Boolean(document.querySelector('[data-testid="parity:disclaimer-accept-button"]')),
      map: Boolean(document.querySelector('[data-testid="map-surface"]')),
      error: document.querySelector('.startupErrorModal')?.textContent ?? null,
    }))()`);
    if (state.error) throw new Error(state.error);
    if (state.disclaimer) {
      await page.evaluate(`document.querySelector('[data-testid="parity:disclaimer-accept-button"]').click()`);
      return false;
    }
    return state.map;
  }, 60_000, `${name} browser did not reach the map`);
  return { chrome, browser, page };
}

async function openCloud(page) {
  await click(page, '.pageLayer.isActive [data-testid="page-button-home"]');
  await click(page, '.pageLayer.isActive [data-testid="home-button-cloud"]');
  await waitFor(
    () => page.evaluate(`Boolean(document.querySelector('.pageLayer.isActive [data-testid="cloud-page"]'))`),
    10_000,
    "Cloud page did not open",
  );
}

async function createAerobagCloudAccount(page) {
  await click(page, '[data-testid="cloud-action-begin_create"]');
  await click(page, '[data-testid="cloud-action-select_provider_aerobag_cloud"]');
  await click(page, '[data-testid="cloud-action-create_account"]');
  await waitFor(
    () => page.evaluate(`document.querySelector('[data-testid="cloud-overall-status"] h2')?.textContent === 'Cloud active'`),
    10_000,
    "Aerobag Cloud account was not created",
  );
}

async function revealSetupCode(page) {
  await click(page, '[data-testid="cloud-action-backup_setup_code"]');
  return await waitFor(
    () => page.evaluate(`document.querySelector('[data-testid="cloud-setup-code-output"]')?.value ?? null`),
    10_000,
    "Device Setup Code did not appear",
  );
}

async function receiveSetupCode(page, setupCode) {
  await click(page, '[data-testid="cloud-action-begin_setup"]');
  await setTextarea(page, '[data-testid="cloud-setup-code-input"]', setupCode);
  await click(page, '[data-testid="cloud-action-accept_setup_code"]');
  await waitFor(
    () => page.evaluate(`document.querySelector('[data-testid="cloud-overall-status"] h2')?.textContent === 'Cloud active'`),
    10_000,
    "second browser did not link the Sync Account",
  );
}

async function openPlan(page) {
  await click(page, '.pageLayer.isActive [data-testid="nav-cdi"]');
  await waitFor(
    () => page.evaluate(`Boolean(document.querySelector('.pageLayer.isActive [data-testid="plan-append-route-input"]'))`),
    10_000,
    "flight-plan page did not open",
  );
}

async function appendRoute(page, route) {
  await setTextarea(page, '[data-testid="plan-append-route-input"]', route);
  await waitFor(
    () => page.evaluate(`Boolean(document.querySelector('.pageLayer.isActive .planEntryInputShell.isReady'))`),
    20_000,
    `route ${route} did not become committable`,
  );
  await page.evaluate(`document.querySelector('.pageLayer.isActive .planEntryForm').requestSubmit()`);
  await waitForCloudState(
    page,
    (state) => includesAll(state.flight_plan_rows, route.split(/\s+/)),
    20_000,
    `route ${route} was not committed`,
  );
}

async function waitForStream(page) {
  return await waitForCloudState(
    page,
    (state) => Boolean(state.event_stream_id),
    20_000,
    "Aerobag Cloud event stream did not connect",
  );
}

async function setOfflinePackagePreferences(page, preferences) {
  await page.evaluate(
    `window.__aerobagE2e.cloud.setOfflinePackagePreferences(${JSON.stringify(preferences)})`,
  );
}

async function cloudState(page) {
  return await page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
}

async function waitForCloudState(page, predicate, timeoutMs, message) {
  return await waitFor(async () => {
    const state = await cloudState(page);
    return state && predicate(state) ? state : false;
  }, timeoutMs, message);
}

async function setTextarea(page, selector, value) {
  await waitFor(async () => page.evaluate(`(() => {
      const input = document.querySelector(${JSON.stringify(selector)});
      if (!(input instanceof HTMLTextAreaElement)) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setter.call(input, ${JSON.stringify(value)});
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    })()`), 10_000, `could not fill ${selector}`);
}

async function click(page, selector) {
  await waitFor(async () => page.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!(element instanceof HTMLElement) || element.hasAttribute('disabled')) return false;
    element.click();
    return true;
  })()`), 10_000, `could not click ${selector}`);
}

function includesAll(values, expected) {
  return expected.every((value) => values.includes(value));
}

function deepEqual(first, second) {
  return JSON.stringify(first) === JSON.stringify(second);
}
