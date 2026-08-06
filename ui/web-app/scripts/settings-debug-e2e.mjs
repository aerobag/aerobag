#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  connectToBrowser,
  launchChrome,
  stopProcess,
  waitFor,
} from "./chrome-cdp.mjs";

const url = process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/";
const screenshotPath = process.env.AEROBAG_E2E_SCREENSHOT ?? null;
const viewportWidth = Number(process.env.AEROBAG_E2E_WIDTH ?? 1000);
const viewportHeight = Number(process.env.AEROBAG_E2E_HEIGHT ?? 900);
const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-settings-debug-"));
let chrome;
let browser;

try {
  chrome = await launchChrome({ userDataDir, width: viewportWidth, height: viewportHeight });
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: viewportWidth,
    height: viewportHeight,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await page.navigate(url);
  await page.waitForLoad();
  await acceptDisclaimerAndWaitForMap(page);

  const legacyDebugUi = await page.evaluate(`(() => ({
    launcher: Boolean(document.querySelector('.debugLauncher')),
    panel: Boolean(document.querySelector('.debugPanel')),
    button: [...document.querySelectorAll('button')].some((button) => button.textContent?.trim() === 'DBG'),
  }))()`);
  if (legacyDebugUi.launcher || legacyDebugUi.panel || legacyDebugUi.button) {
    throw new Error(`legacy DBG UI remains visible: ${JSON.stringify(legacyDebugUi)}`);
  }

  await page.evaluate("document.querySelector('[data-testid=\"page-button-home\"]')?.click()");
  await waitFor(
    () => page.evaluate("Boolean(document.querySelector('[data-testid=\"home-button-settings\"]'))"),
    10_000,
    "Settings launcher did not appear on Home",
  );
  await page.evaluate("document.querySelector('[data-testid=\"home-button-settings\"]')?.click()");

  const initial = await waitFor(
    () => page.evaluate(`(() => {
      const header = document.querySelector('[data-testid="settings-section-debug_diagnostics"]');
      if (!header) return null;
      return {
        expanded: header.getAttribute('aria-expanded'),
        toggleVisible: Boolean(document.querySelector('[data-testid="settings-toggle-debug_plate_flight_plan"]')?.offsetParent),
      };
    })()`),
    10_000,
    "Debug Diagnostics section did not appear",
  );
  if (initial.expanded !== "false" || initial.toggleVisible) {
    throw new Error(`Debug Diagnostics did not start folded: ${JSON.stringify(initial)}`);
  }

  await page.evaluate("document.querySelector('[data-testid=\"settings-section-debug_diagnostics\"]')?.click()");
  const before = await waitFor(
    () => page.evaluate(`(() => {
      const toggle = document.querySelector('[data-testid="settings-toggle-debug_plate_flight_plan"]');
      return toggle?.offsetParent ? { checked: toggle.checked } : null;
    })()`),
    10_000,
    "Debug Diagnostics toggle did not become visible",
  );
  await page.evaluate("document.querySelector('[data-testid=\"settings-toggle-debug_plate_flight_plan\"]')?.click()");
  await waitFor(
    () => page.evaluate(`document.querySelector('[data-testid="settings-toggle-debug_plate_flight_plan"]')?.checked === ${!before.checked}`),
    10_000,
    "Debug setting did not round-trip through core",
  );

  if (screenshotPath) {
    const screenshot = await page.send("Page.captureScreenshot", { format: "png" });
    await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));
  }
  if (page.diagnostics.some((entry) => entry.method === "Runtime.exceptionThrown")) {
    throw new Error(`browser exceptions observed: ${JSON.stringify(page.diagnostics.slice(-10))}`);
  }
  process.stdout.write("settings debug diagnostics e2e passed\n");
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}

async function acceptDisclaimerAndWaitForMap(page) {
  await waitFor(
    async () => {
      const state = await page.evaluate(`(() => ({
        accept: Boolean(document.querySelector('.disclaimerAcceptButton')),
        map: Boolean(document.querySelector('[data-testid="map-surface"]')),
        startupError: document.querySelector('.startupErrorModal')?.textContent ?? null,
      }))()`);
      if (state.startupError) {
        throw new Error(state.startupError);
      }
      if (state.accept) {
        await page.evaluate("document.querySelector('.disclaimerAcceptButton').click()");
        return false;
      }
      return state.map;
    },
    60_000,
    "timed out waiting for disclaimer or map",
    200,
  );
}
