#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
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

const args = parseArgs(process.argv.slice(2));
const url = args.url ?? process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/";
const viewportWidth = parsePositiveInteger(args.width, 1200);
const viewportHeight = parsePositiveInteger(args.height, 1000);
const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-browser-platform-"));
let chrome;
let browser;

try {
  progress(`launching Chrome for ${url}`);
  chrome = await launchChrome({
    chromeBin: args.chrome ?? process.env.CHROME_BIN,
    userDataDir,
    width: viewportWidth,
    height: viewportHeight,
  });
  progress("connecting to Chrome");
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Log.enable");
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: viewportWidth,
    height: viewportHeight,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await installSyntheticGeolocation(page);
  progress("loading app");
  await page.navigate(url);
  await page.waitForLoad();

  progress("waiting for map");
  await acceptDisclaimerIfPresent(page);
  const mapRect = await waitForMap(page);
  progress("checking shared time-display actions");
  const timeDisplay = await verifyTimeDisplayActions(page);
  progress("checking rotated raster coverage");
  const rotatedRaster = await verifyRotatedRasterCoverage(page, args.screenshot);
  progress("clicking map");
  await clickMap(page, mapRect);
  const selection = await waitForMapSelection(page);

  if (!selection.primary.includes("Elev ")) {
    throw new Error(`selected point does not display elevation: ${JSON.stringify(selection)}`);
  }
  if (!/^-?\d+\.\d{4}, -?\d+\.\d{4}$/.test(selection.secondary)) {
    throw new Error(`selected point does not display four-decimal coordinates: ${JSON.stringify(selection)}`);
  }
  const refreshedSelection = await verifyMapSelectionDistanceRefresh(page, selection);
  if (page.diagnostics.some((entry) => entry.method === "Runtime.exceptionThrown")) {
    throw new Error(`browser exceptions observed: ${JSON.stringify(page.diagnostics.slice(-10))}`);
  }

  process.stdout.write(
    `browser platform smoke passed: ${JSON.stringify({ timeDisplay, rotatedRaster, selection, refreshedSelection })}\n`,
  );
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
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

function parsePositiveInteger(value, fallback) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

async function acceptDisclaimerIfPresent(page) {
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
      if (rect.width < 300 || rect.height < 400) return null;
      return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
    })()`),
    60000,
    `timed out waiting for map surface; diagnostics=${JSON.stringify(page.diagnostics.slice(-10))}`,
    200,
  );
}

async function verifyTimeDisplayActions(page) {
  const bannerLocal = await waitFor(
    async () => await page.evaluate(`(() => {
      const cell = [...document.querySelectorAll('.flightDataCell')]
        .find((candidate) => candidate.querySelector('.flightDataLabel')?.textContent?.startsWith('ETA '));
      if (!(cell instanceof HTMLElement)
        || cell.getAttribute('role') !== 'button'
        || getComputedStyle(cell).pointerEvents !== 'auto') return null;
      return cell.querySelector('.flightDataLabel')?.textContent?.trim() ?? null;
    })()`),
    10000,
    "ETA flight-data cell did not become a clickable core action",
    100,
  );
  if (bannerLocal.includes('LCL')) {
    throw new Error(`ETA flight-data cell exposed generic local label: ${bannerLocal}`);
  }
  await page.evaluate(`(() => {
    const cell = [...document.querySelectorAll('.flightDataCell')]
      .find((candidate) => candidate.querySelector('.flightDataLabel')?.textContent?.startsWith('ETA '));
    cell?.click();
  })()`);
  const bannerZulu = await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.flightDataCell')]
      .find((candidate) => candidate.querySelector('.flightDataLabel')?.textContent?.trim() === 'ETA Z')
      ?.querySelector('.flightDataLabel')?.textContent?.trim() ?? null)()`),
    10000,
    "ETA flight-data action did not switch the shared mode to Zulu",
    100,
  );
  await page.evaluate(`(() => {
    const cell = [...document.querySelectorAll('.flightDataCell')]
      .find((candidate) => candidate.querySelector('.flightDataLabel')?.textContent?.trim() === 'ETA Z');
    cell?.click();
  })()`);
  await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.flightDataCell')]
      .some((candidate) => candidate.querySelector('.flightDataLabel')?.textContent?.trim() === ${JSON.stringify(bannerLocal)}))()`),
    10000,
    "ETA flight-data action did not restore local mode",
    100,
  );

  await page.evaluate(`document.querySelector('[aria-label="Primary navigation"] [data-testid="nav-cdi"]')?.click()`);
  const columnLocal = await waitFor(
    async () => await page.evaluate(`(() => {
      const header = [...document.querySelectorAll('.planHeader.isActionable')]
        .find((candidate) => candidate.textContent?.startsWith('ETA '));
      return header?.textContent?.trim() ?? null;
    })()`),
    10000,
    "flight-plan ETA column did not expose the shared time action",
    100,
  );
  await page.evaluate(`(() => {
    const header = [...document.querySelectorAll('.planHeader.isActionable')]
      .find((candidate) => candidate.textContent?.startsWith('ETA '));
    header?.click();
  })()`);
  const columnZulu = await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.planHeader.isActionable')]
      .find((candidate) => candidate.textContent?.trim() === 'ETA Z')?.textContent?.trim() ?? null)()`),
    10000,
    "flight-plan ETA column action did not switch the shared mode to Zulu",
    100,
  );
  await page.evaluate(`(() => {
    const header = [...document.querySelectorAll('.planHeader.isActionable')]
      .find((candidate) => candidate.textContent?.trim() === 'ETA Z');
    header?.click();
  })()`);
  await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.planHeader.isActionable')]
      .some((candidate) => candidate.textContent?.trim() === ${JSON.stringify(columnLocal)}))()`),
    10000,
    "flight-plan ETA column action did not restore local mode",
    100,
  );
  await page.evaluate(`document.querySelector('[data-testid="page-button-return-chart"]')?.click()`);
  await waitForMap(page);

  return { bannerLocal, bannerZulu, columnLocal, columnZulu };
}

async function installSyntheticGeolocation(page) {
  await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      let heading = 45;
      let latitude = 47.4931;
      let longitude = -122.2157;
      const position = () => ({
        timestamp: Date.now(),
        coords: {
          latitude,
          longitude,
          accuracy: 3,
          altitude: 20,
          altitudeAccuracy: 5,
          heading,
          speed: 45,
        },
      });
      let nextWatchId = 1;
      const watchers = new Map();
      window.__aerobagSetSyntheticTrack = (nextHeading) => {
        heading = nextHeading;
        for (const success of watchers.values()) {
          setTimeout(() => success(position()), 0);
        }
      };
      window.__aerobagSetSyntheticPosition = (nextLatitude, nextLongitude) => {
        latitude = nextLatitude;
        longitude = nextLongitude;
        for (const success of watchers.values()) {
          setTimeout(() => success(position()), 0);
        }
      };
      Object.defineProperty(navigator, 'geolocation', {
        configurable: true,
        value: {
          getCurrentPosition(success) {
            setTimeout(() => success(position()), 0);
          },
          watchPosition(success) {
            const id = nextWatchId++;
            watchers.set(id, success);
            setTimeout(() => success(position()), 0);
            return id;
          },
          clearWatch(id) {
            watchers.delete(id);
          },
        },
      });
    })()`,
  });
}

async function verifyRotatedRasterCoverage(page, screenshotPath) {
  await waitFor(
    async () => await page.evaluate(`(() => [...document.querySelectorAll('.mapTileImage')]
      .some((image) => image.complete && image.naturalWidth > 0))()`),
    30000,
    "timed out waiting for initial raster images",
    200,
  );

  await waitFor(
    async () => await page.evaluate(`(() => {
      const trackCell = [...document.querySelectorAll('.flightDataCell')]
        .find((cell) => cell.textContent?.includes('TRK °M'));
      return trackCell && !trackCell.textContent?.includes('—') ? trackCell.textContent : null;
    })()`),
    20000,
    "synthetic browser GPS track did not reach the UI",
    100,
  );
  await page.evaluate(`(() => {
    const button = document.querySelector('[data-testid="map-orientation-button"]');
    if (!(button instanceof HTMLButtonElement)) {
      throw new Error('map orientation button is unavailable');
    }
    button.click();
  })()`);

  const plan = await waitFor(
    async () => await page.evaluate(`(() => {
      const bearing = document.querySelector('.mapBearingTransform');
      const button = document.querySelector('[data-testid="map-orientation-button"]');
      const layer = document.querySelector('.rasterTileLayer');
      if (!(bearing instanceof HTMLElement)
        || !(button instanceof HTMLButtonElement)
        || !(layer instanceof HTMLElement)
        || button.getAttribute('aria-pressed') !== 'true'
        || !button.textContent?.includes('TRK')) return null;
      const bearingTransform = getComputedStyle(bearing).transform;
      if (!bearingTransform.startsWith('matrix(')
        || Math.abs(Number(bearingTransform.split('(')[1].split(',')[0]) - Math.SQRT1_2) > 0.01) {
        return null;
      }
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
        mode: button.textContent.trim(),
        bearingTransform,
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
    const width = layer.offsetWidth;
    const height = layer.offsetHeight;
    const fractions = [0.1, 0.25, 0.5, 0.75, 0.9];
    const candidates = [
      ...fractions.map((fraction) => ({ left: width * fraction, top: -30 })),
      ...fractions.map((fraction) => ({ left: width * fraction, top: height + 10 })),
      ...fractions.map((fraction) => ({ left: -30, top: height * fraction })),
      ...fractions.map((fraction) => ({ left: width + 10, top: height * fraction })),
    ];
    let transformedInsideMap = false;
    let painted = false;
    for (const candidate of candidates) {
      marker.style.left = Math.round(candidate.left) + 'px';
      marker.style.top = Math.round(candidate.top) + 'px';
      const markerRect = marker.getBoundingClientRect();
      const x = markerRect.left + markerRect.width / 2;
      const y = markerRect.top + markerRect.height / 2;
      const inside = x >= mapRect.left + 2 && x < mapRect.right - 2
        && y >= mapRect.top + 2 && y < mapRect.bottom - 2;
      transformedInsideMap ||= inside;
      painted ||= inside && document.elementsFromPoint(x, y).includes(marker);
      if (painted) break;
    }
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

  await page.evaluate(`window.__aerobagSetSyntheticTrack?.(null)`);
  const retainedTrackUp = await waitFor(
    async () => await page.evaluate(`(() => {
      const trackCell = [...document.querySelectorAll('.flightDataCell')]
        .find((cell) => cell.textContent?.includes('TRK °M'));
      const bearing = document.querySelector('.mapBearingTransform');
      const button = document.querySelector('[data-testid="map-orientation-button"]');
      if (!trackCell?.textContent?.includes('—')
        || !(bearing instanceof HTMLElement)
        || !(button instanceof HTMLButtonElement)
        || button.getAttribute('aria-pressed') !== 'true') return null;
      const bearingTransform = getComputedStyle(bearing).transform;
      const firstMatrixValue = bearingTransform.startsWith('matrix(')
        ? Number(bearingTransform.split('(')[1].split(',')[0])
        : Number.NaN;
      return Math.abs(firstMatrixValue - Math.SQRT1_2) <= 0.01
        ? { bearingTransform, trackCell: trackCell.textContent.trim() }
        : null;
    })()`),
    10000,
    "track-up map snapped north when synthetic GPS track disappeared",
    100,
  );

  if (screenshotPath) {
    const screenshot = await page.send("Page.captureScreenshot", { format: "png", fromSurface: true });
    await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));
  }

  await page.evaluate(`document.querySelector('[data-testid="map-orientation-button"]')?.click()`);
  return { ...plan, ...paintProbe, retainedTrackUp };
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

async function verifyMapSelectionDistanceRefresh(page, initialSelection) {
  const initialDistance = initialSelection.primary.match(/\b\d+(?:\.\d+)?nm\b/)?.[0];
  if (!initialDistance) {
    throw new Error(`selected point does not display ownship distance: ${JSON.stringify(initialSelection)}`);
  }
  await page.evaluate(`window.__aerobagSetSyntheticPosition?.(47.4931, -121.9157)`);
  return await waitFor(
    async () => await page.evaluate(`(() => {
      const primary = document.querySelector('[data-testid="map-selection-tray"] .mapSelectionActionTitlePrimary')
        ?.textContent?.trim() ?? "";
      const distance = primary.match(/\\b\\d+(?:\\.\\d+)?nm\\b/)?.[0] ?? null;
      return distance && distance !== ${JSON.stringify(initialDistance)} ? { primary, distance } : null;
    })()`),
    10000,
    `map selection distance did not follow ownship movement from ${initialDistance}`,
    100,
  );
}
