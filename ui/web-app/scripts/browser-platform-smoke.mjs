#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  connectToBrowser,
  launchChrome,
  stopProcess,
  waitFor,
} from "./chrome-cdp.mjs";

const args = parseArgs(process.argv.slice(2));
const url = args.url ?? process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/";
const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-browser-platform-"));
let chrome;
let browser;

try {
  progress(`launching Chrome for ${url}`);
  chrome = await launchChrome({
    chromeBin: args.chrome ?? process.env.CHROME_BIN,
    userDataDir,
  });
  progress("connecting to Chrome");
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Log.enable");
  progress("loading app");
  await page.navigate(url);
  await page.waitForLoad();

  progress("waiting for map");
  await acceptDisclaimerIfPresent(page);
  const mapRect = await waitForMap(page);
  progress("clicking map");
  await clickMap(page, mapRect);
  const selection = await waitForMapSelection(page);

  if (!selection.primary.includes("Elev ")) {
    throw new Error(`selected point does not display elevation: ${JSON.stringify(selection)}`);
  }
  if (!/^-?\d+\.\d{4}, -?\d+\.\d{4}$/.test(selection.secondary)) {
    throw new Error(`selected point does not display four-decimal coordinates: ${JSON.stringify(selection)}`);
  }
  if (page.diagnostics.some((entry) => entry.method === "Runtime.exceptionThrown")) {
    throw new Error(`browser exceptions observed: ${JSON.stringify(page.diagnostics.slice(-10))}`);
  }

  process.stdout.write(`browser map-selection smoke passed: ${JSON.stringify(selection)}\n`);
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true });
}

function progress(message) {
  process.stderr.write(`[browser-platform-smoke] ${message}\n`);
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--")) {
      continue;
    }
    const withoutPrefix = value.slice(2);
    const equals = withoutPrefix.indexOf("=");
    if (equals >= 0) {
      parsed[withoutPrefix.slice(0, equals)] = withoutPrefix.slice(equals + 1);
    } else {
      parsed[withoutPrefix] = values[index + 1] && !values[index + 1].startsWith("--")
        ? values[++index]
        : "true";
    }
  }
  return parsed;
}

async function acceptDisclaimerIfPresent(page) {
  await waitFor(
    async () => {
      const state = await page.evaluate(`(() => ({
        accept: Boolean(document.querySelector('.disclaimerAcceptButton')),
        map: Boolean(document.querySelector('[data-testid="map-surface"]')),
        startupError: document.querySelector('.startupFailure')?.textContent ?? null,
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
    60000,
    "timed out waiting for disclaimer or map",
    200,
  );
}

async function waitForMap(page) {
  return await waitFor(
    async () => await page.evaluate(`(() => {
      const surface = document.querySelector('[data-testid="map-surface"]');
      if (!surface) return null;
      const rect = surface.getBoundingClientRect();
      if (rect.width < 600 || rect.height < 500) return null;
      return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
    })()`),
    60000,
    `timed out waiting for map surface; diagnostics=${JSON.stringify(page.diagnostics.slice(-10))}`,
    200,
  );
}

async function clickMap(page, rect) {
  const x = rect.left + rect.width * 0.5;
  const y = rect.top + rect.height * 0.5;
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x,
    y,
    button: "none",
    pointerType: "mouse",
  });
  await page.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x,
    y,
    button: "left",
    buttons: 1,
    clickCount: 1,
    pointerType: "mouse",
  });
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button: "left",
    buttons: 0,
    clickCount: 1,
    pointerType: "mouse",
  });
}

async function waitForMapSelection(page) {
  return await waitFor(
    async () => await page.evaluate(`(() => {
      const tray = document.querySelector('[data-testid="map-selection-tray"]');
      const selected = tray?.querySelector('.mapSelectionItem.isSelected');
      if (!tray || !selected) return null;
      return {
        selected: selected.textContent?.trim() ?? "",
        primary: tray.querySelector('.mapSelectionActionTitlePrimary')?.textContent?.trim() ?? "",
        secondary: tray.querySelector('.mapSelectionActionTitleSecondary')?.textContent?.trim() ?? "",
      };
    })()`),
    20000,
    `map click did not open a preselected inspector; diagnostics=${JSON.stringify(page.diagnostics.slice(-10))}`,
    100,
  );
}
