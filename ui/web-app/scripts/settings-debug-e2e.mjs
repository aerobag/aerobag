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
const homeScreenshotPath = process.env.AEROBAG_E2E_HOME_SCREENSHOT ?? null;
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
  const altitudePlannerIcon = await page.evaluate(`(() => {
    const icon = document.querySelector('[data-testid="home-button-altitude_planner"] img');
    return icon ? { src: icon.getAttribute('src'), loaded: icon.complete && icon.naturalWidth > 0 } : null;
  })()`);
  if (!altitudePlannerIcon?.loaded || !altitudePlannerIcon.src?.includes("home-altitude-planner-icon.png")) {
    throw new Error(`Altitude Planner Home icon did not render: ${JSON.stringify(altitudePlannerIcon)}`);
  }
  if (homeScreenshotPath) {
    const screenshot = await page.send("Page.captureScreenshot", { format: "png" });
    await writeFile(homeScreenshotPath, Buffer.from(screenshot.data, "base64"));
  }
  await page.evaluate("document.querySelector('[data-testid=\"home-button-settings\"]')?.click()");

  const aircraftLibrary = await waitFor(
    () => page.evaluate(`(() => {
      const library = document.querySelector('[data-testid="settings-aircraft-library"]');
      if (!library) return null;
      return {
        systemEntries: library.querySelectorAll('[data-testid="settings-aircraft-entry-system"]').length,
        icons: library.querySelectorAll('.aircraftSymbolIcon').length,
      };
    })()`),
    10_000,
    "Aircraft library did not appear",
  );
  if (aircraftLibrary.systemEntries < 1 || aircraftLibrary.icons < aircraftLibrary.systemEntries) {
    throw new Error(`Aircraft library did not render bundled models and symbols: ${JSON.stringify(aircraftLibrary)}`);
  }
  await page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-add\"]')?.click()");
  const validAircraftSource = await waitFor(
    () => page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-source\"]')?.value ?? null"),
    10_000,
    "Aircraft editor did not open",
  );
  await page.evaluate(`(() => {
    const editor = document.querySelector('[data-testid="settings-aircraft-source"]');
    if (!editor) return;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    setter.call(editor, '{');
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('[data-testid="settings-aircraft-save"]')?.click();
  })()`);
  await waitFor(
    () => page.evaluate("document.querySelector('.settingsAircraftError')?.textContent?.includes('Invalid JSON') ?? false"),
    10_000,
    "Invalid aircraft definition did not remain in the editor with feedback",
  );
  await page.evaluate(`(() => {
    const editor = document.querySelector('[data-testid="settings-aircraft-source"]');
    if (!editor) return;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    setter.call(editor, ${JSON.stringify(validAircraftSource)});
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('[data-testid="settings-aircraft-save"]')?.click();
  })()`);
  await waitFor(
    () => page.evaluate("Boolean(document.querySelector('[data-testid=\"settings-aircraft-entry-user\"]'))"),
    10_000,
    "Valid private aircraft did not enter the library",
  );
  const orderedAircraftSources = await page.evaluate(`[
    ...document.querySelectorAll('.settingsAircraftEntry .settingsAircraftIdentity small'),
  ].map((element) => element.textContent?.trim())`);
  if (orderedAircraftSources[0] !== "USER") {
    throw new Error(`Private aircraft did not sort before system aircraft: ${JSON.stringify(orderedAircraftSources)}`);
  }

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
  const diagnosticsOrder = await page.evaluate(`(() => {
    const library = document.querySelector('[data-testid="settings-aircraft-library"]');
    const diagnostics = document
      .querySelector('[data-testid="settings-section-debug_diagnostics"]')
      ?.closest('.settingsPageSection');
    return {
      libraryBeforeDiagnostics: Boolean(
        library && diagnostics
          && (library.compareDocumentPosition(diagnostics) & Node.DOCUMENT_POSITION_FOLLOWING),
      ),
      diagnosticsIsLast: diagnostics?.parentElement?.lastElementChild === diagnostics,
    };
  })()`);
  if (!diagnosticsOrder.libraryBeforeDiagnostics || !diagnosticsOrder.diagnosticsIsLast) {
    throw new Error(`Debug Diagnostics was not the final settings block: ${JSON.stringify(diagnosticsOrder)}`);
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
    await page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-library\"]')?.scrollIntoView({ block: 'start' })");
    const screenshot = await page.send("Page.captureScreenshot", { format: "png" });
    await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));
  }
  await page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-toggle-user\"]')?.click()");
  await waitFor(
    () => page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-entry-user\"]')?.classList.contains('isHidden') ?? false"),
    10_000,
    "Hidden private aircraft did not remain available in the library",
  );
  await page.evaluate("document.querySelector('[data-testid=\"settings-aircraft-toggle-user\"]')?.click()");
  await waitFor(
    () => page.evaluate("!(document.querySelector('[data-testid=\"settings-aircraft-entry-user\"]')?.classList.contains('isHidden') ?? true)"),
    10_000,
    "Hidden private aircraft could not be shown again",
  );
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
