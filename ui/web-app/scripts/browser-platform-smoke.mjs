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
  progress("checking rotated raster coverage");
  const rotatedRaster = await verifyRotatedRasterCoverage(page);
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

  process.stdout.write(
    `browser platform smoke passed: ${JSON.stringify({ rotatedRaster, selection })}\n`,
  );
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

async function verifyRotatedRasterCoverage(page) {
  await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.mapTileImage')]
      .some((image) => image.complete && image.naturalWidth > 0))()`),
    30000,
    "timed out waiting for initial raster images",
    200,
  );

  await page.evaluate(`(() => {
    document.querySelector('.debugLauncher')?.click();
    const slider = document.querySelector('input[aria-label="Debug map-up rotation"]');
    if (!(slider instanceof HTMLInputElement)) {
      throw new Error('debug map-up rotation control is unavailable');
    }
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(slider, '45');
    slider.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);

  const plan = await waitFor(
    async () => await page.evaluate(`(() => {
      const layer = document.querySelector('.rasterTileLayer');
      if (!(layer instanceof HTMLElement)) return null;
      const loadedOffBoundaryTiles = [...layer.querySelectorAll('.mapTile')].filter((tile) => {
        if (!(tile instanceof HTMLElement)) return false;
        const image = tile.querySelector('.mapTileImage');
        const loaded = image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0;
        const outside = tile.offsetLeft < 0
          || tile.offsetTop < 0
          || tile.offsetLeft + tile.offsetWidth > layer.offsetWidth
          || tile.offsetTop + tile.offsetHeight > layer.offsetHeight;
        return loaded && outside;
      });
      return loadedOffBoundaryTiles.length > 0 ? {
        loadedTiles: [...layer.querySelectorAll('.mapTileImage')]
          .filter((image) => image.complete && image.naturalWidth > 0).length,
        loadedOffBoundaryTiles: loadedOffBoundaryTiles.length,
      } : null;
    })()`),
    30000,
    `rotated raster plan did not load off-boundary tiles; diagnostics=${JSON.stringify(page.diagnostics.slice(-10))}`,
    100,
  );

  const paintProbe = await page.evaluate(`(() => {
    const map = document.querySelector('[data-testid="map-surface"]');
    const bearing = document.querySelector('.mapBearingTransform');
    const content = document.querySelector('.mapContentTransform');
    const layer = document.querySelector('.rasterTileLayer');
    if (!(map instanceof HTMLElement)
      || !(bearing instanceof HTMLElement)
      || !(content instanceof HTMLElement)
      || !(layer instanceof HTMLElement)) {
      throw new Error('rotated raster transform stack is unavailable');
    }

    const marker = document.createElement('div');
    marker.dataset.rasterOverflowProbe = 'true';
    Object.assign(marker.style, {
      position: 'absolute',
      left: Math.round(layer.offsetWidth / 3) + 'px',
      top: '-40px',
      width: '20px',
      height: '20px',
      pointerEvents: 'auto',
      zIndex: '2147483647',
    });
    const saved = {
      bearingPointerEvents: bearing.style.pointerEvents,
      contentPointerEvents: content.style.pointerEvents,
      layerPointerEvents: layer.style.pointerEvents,
      layerZIndex: layer.style.zIndex,
    };
    bearing.style.pointerEvents = 'auto';
    content.style.pointerEvents = 'auto';
    layer.style.pointerEvents = 'auto';
    layer.style.zIndex = '2147483647';
    layer.append(marker);

    const mapRect = map.getBoundingClientRect();
    const markerRect = marker.getBoundingClientRect();
    const x = markerRect.left + markerRect.width / 2;
    const y = markerRect.top + markerRect.height / 2;
    const transformedInsideMap = x >= mapRect.left && x < mapRect.right
      && y >= mapRect.top && y < mapRect.bottom;
    const painted = transformedInsideMap && document.elementsFromPoint(x, y).includes(marker);
    const overflow = getComputedStyle(layer).overflow;

    marker.remove();
    bearing.style.pointerEvents = saved.bearingPointerEvents;
    content.style.pointerEvents = saved.contentPointerEvents;
    layer.style.pointerEvents = saved.layerPointerEvents;
    layer.style.zIndex = saved.layerZIndex;
    return { painted, transformedInsideMap, overflow };
  })()`);
  if (!paintProbe.transformedInsideMap || !paintProbe.painted) {
    throw new Error(`rotated off-boundary raster pixels were clipped: ${JSON.stringify(paintProbe)}`);
  }

  await page.evaluate(`(() => {
    const slider = document.querySelector('input[aria-label="Debug map-up rotation"]');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(slider, '0');
    slider.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('.debugLauncher')?.click();
  })()`);
  return { ...plan, ...paintProbe };
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
