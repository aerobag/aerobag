#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  assertNoAerobagAnr,
  ANDROID_PACKAGE,
  acceptDisclaimerIfPresent,
  adb,
  adbBestEffort,
  androidNodeLabel,
  androidTag,
  assertRuntimeIsAvailable,
  captureAndroidFailureDiagnostics,
  clearAerobagPersistedLiveFeeds,
  clearFocusedText,
  currentAerobagPid,
  delay,
  destinationCenterEvidence,
  dumpAndroid,
  findNode,
  findNodes,
  hasAndroidTag,
  hasAndroidText,
  inputText,
  launchFreshAndroidApp,
  layerToggleNode,
  layerToggleTag,
  lockAndroidRotation,
  pressKey,
  rectOfBounds,
  renderedFlightPlanSignature,
  restoreAndroidRotationState,
  saveAndroidRotationState,
  scanAerobagLogcat,
  screencapPng,
  seedAerobagPrivateFiles,
  setAerobagPrivateSentinel,
  scrollUntilTag,
  swipe,
  setAndroidRotation,
  tapFirstPresentTag,
  tapNode,
  tapTag,
  waitFor,
  waitForAndroidOrientation,
  waitForNode,
} from "./android-harness.mjs";

const DEFAULT_ROUTE = "KRNT KPWT";
const ROTATION_ROUTE = "KRNT KPWT KPLU";
const DEFAULT_PACKAGE_SOURCE_PORT = process.env.PACKAGE_SOURCE_PORT ?? "8083";
const OFFLINE_REGION_IDS = ["ak", "ec", "nc", "ne", "nw", "pac", "sc", "se", "sw"];
const PLAN_PAGE_TAGS = ["parity:plan-append-route-input"];
const CHART_PAGE_TAGS = ["parity:map-surface"];
const CHART_SEARCH_INPUT_TAG = "parity:chart-search-input";
const ROUTE_OVERLAY_PREFIX = "parity:flight-plan-route-overlay:";
const MAP_FOLLOW_PREFIX = "parity:map-follow-state:";
const BAD_AUTOPILOT_SOURCE_TAG = "parity:ownship-source:__bad_autopilot__";
const BAD_AUTOPILOT_DEBUG_TAG = "parity:settings-toggle:debug_bad_autopilot";
const DEBUG_DIAGNOSTICS_SECTION_TAG = "parity:settings-section:debug_diagnostics";
const PLATE_SURFACE_TAG = "parity:plate-surface";
const PLATE_FOLDER_TILE_PREFIX = "parity:plate-folder-tile:";
const E2E_ARTIFACT_DIR = process.env.AEROBAG_E2E_ARTIFACT_DIR ?? join(tmpdir(), "aerobag-e2e-artifacts");
const ROTATION_LIVE_FEED_FIXTURE = process.env.AEROBAG_ROTATION_LIVE_FEED_FIXTURE ??
  (process.env.AEROBAG_TEST_ARTIFACTS_ROOT
    ? join(process.env.AEROBAG_TEST_ARTIFACTS_ROOT, "e2e/android-rotation-live-feed")
    : "");
const LIVE_FEED_PROMOTION_SENTINEL = "e2e-live-feed-promotion.pause";
const LIVE_FEED_PROMOTION_PAUSE_MARKER = "E2E live-feed promotion paused";

function usage() {
  console.log(`Usage:
  node tools/e2e/run-android-e2e-suite.mjs [--serial emulator-5554] [--route "KRNT KPWT"] [--package-source-port 8083] [--no-sync-offline-packages] [--sync-all-available-packages] [--test TEST_ID] [--json]

Runs Android end-to-end UI tests against an installed Aerobag app.
When a clean emulator starts on Offline Packages, the runner syncs the NW
package set through the app UI before running the route smoke. Compact
publications may use --sync-all-available-packages to skip searches for region
toggles that are not present in that publication.`);
}

function parseArgs(argv) {
  const args = {
    serial: process.env.ANDROID_SERIAL ?? "",
    route: DEFAULT_ROUTE,
    packageSourcePort: DEFAULT_PACKAGE_SOURCE_PORT,
    syncOfflinePackages: true,
    syncAllAvailablePackages: false,
    test: "",
    json: false,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "-h" || arg === "--help") {
      args.help = true;
    } else if (arg === "--serial") {
      args.serial = argv[++i] ?? "";
    } else if (arg === "--route") {
      args.route = argv[++i] ?? "";
    } else if (arg === "--package-source-port") {
      args.packageSourcePort = argv[++i] ?? "";
    } else if (arg === "--no-sync-offline-packages") {
      args.syncOfflinePackages = false;
    } else if (arg === "--sync-all-available-packages") {
      args.syncAllAvailablePackages = true;
    } else if (arg === "--test") {
      args.test = argv[++i] ?? "";
    } else if (arg === "--json") {
      args.json = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function createTestResult(id) {
  return {
    id,
    platform: "android",
    started_at: new Date().toISOString(),
    steps: [],
    checks: {},
    diagnostics: {},
  };
}

function recordStep(result, name, detail = undefined) {
  result.steps.push({
    name,
    status: "ok",
    ...(detail === undefined ? {} : { detail }),
  });
  console.log(`ok - ${name}${detail === undefined ? "" : `: ${detail}`}`);
}

function recordCheck(result, name, value, detail = undefined) {
  result.checks[name] = { pass: Boolean(value), ...(detail === undefined ? {} : { detail }) };
  if (!value) {
    throw new Error(`${name} failed${detail === undefined ? "" : `: ${detail}`}`);
  }
}

function summarizeUi(xml) {
  const tags = findNodes(xml, (node) => androidTag(node))
    .map((node) => androidTag(node))
    .slice(0, 80);
  const texts = findNodes(xml, (node) => (node.text ?? "").trim().length > 0)
    .map((node) => node.text.trim())
    .filter(Boolean)
    .slice(0, 80);
  return { tags, texts };
}

function throwWithUi(serial, message) {
  const xml = dumpAndroid(serial);
  const summary = summarizeUi(xml);
  throw new Error(`${message}; visibleTags=${JSON.stringify(summary.tags)}; visibleTexts=${JSON.stringify(summary.texts)}`);
}

function hasAndroidTextContaining(xml, text) {
  return findNode(xml, (node) => (node.text ?? "").includes(text)) !== null;
}

function offlinePackagesVisible(xml) {
  return findNode(xml, (node) =>
    hasAndroidTag(node, "parity:offline-library-panel") ||
    hasAndroidTag(node, "parity:offline-packages-panel") ||
    hasAndroidTag(node, "parity:offline-refresh-button") ||
    hasAndroidTag(node, "parity:offline-sync-button")
  ) !== null;
}

function disclaimerVisible(xml) {
  return findNode(xml, (node) => hasAndroidTag(node, "parity:disclaimer-accept-button")) !== null;
}

function runtimeUiVisible(xml) {
  return !offlinePackagesVisible(xml) && findNode(xml, (node) =>
    hasAndroidTag(node, "parity:map-surface") ||
    hasAndroidTag(node, "parity:plan-append-route-input") ||
    hasAndroidTag(node, "parity:button:FLIGHT\nPLAN") ||
    hasAndroidTag(node, "parity:button:CHART")
  ) !== null;
}

function offlineSyncIsIdle(xml) {
  const node = findNode(xml, (candidate) => hasAndroidTag(candidate, "parity:offline-sync-button"));
  if (!node) return false;
  const label = androidNodeLabel(xml, node).toUpperCase();
  return node.enabled === "true" && !label.includes("SYNCING") && !label.includes("CANCELING");
}

async function tapTagIfPresent(serial, tag, timeoutMs = 3000) {
  try {
    await tapTag(serial, tag, timeoutMs);
    return true;
  } catch (_error) {
    return false;
  }
}

async function ensureOfflinePackagesReady(
  serial,
  result,
  { packageSourcePort, syncOfflinePackages, syncAllAvailablePackages },
) {
  if (packageSourcePort) {
    adbBestEffort(serial, ["reverse", `tcp:${packageSourcePort}`, `tcp:${packageSourcePort}`]);
  }

  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return runtimeUiVisible(xml) || offlinePackagesVisible(xml) || disclaimerVisible(xml);
  }, 45000, "runtime or offline package UI ready");

  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  if (runtimeUiVisible(dumpAndroid(serial))) {
    recordStep(result, "offline packages ready", "runtime already available");
    return;
  }
  if (!syncOfflinePackages) {
    throwWithUi(serial, "offline packages are required but sync is disabled");
  }

  let xml = dumpAndroid(serial);
  if (
    findNode(xml, (node) => hasAndroidTag(node, "parity:offline-library-panel")) ||
    findNode(xml, (node) => hasAndroidTag(node, "parity:offline-refresh-button"))
  ) {
    if (!findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"))) {
      if (await tapTagIfPresent(serial, "parity:offline-refresh-button", 10000)) {
        recordStep(result, "offline package library refresh requested");
        await waitFor(() => {
          const nextXml = dumpAndroid(serial);
          return findNode(nextXml, (node) =>
            hasAndroidTag(node, "parity:offline-packages-panel") ||
            hasAndroidTag(node, "parity:offline-sync-button")
          );
        }, 120000, "offline package planner after refresh", 500);
        recordStep(result, "offline package planner visible after refresh");
      }
    }
  }

  xml = dumpAndroid(serial);
  if (findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"))) {
    if (!syncAllAvailablePackages) {
      for (const regionId of OFFLINE_REGION_IDS) {
        if (regionId === "nw") continue;
        const tag = `parity:offline-region:${regionId}:toggle`;
        await scrollUntilTag(serial, tag, 6);
        if (await tapTagIfPresent(serial, tag, 1200)) {
          recordStep(result, "offline region deselected", regionId);
          await delay(150);
        }
      }
    }

    if (offlineSyncIsIdle(dumpAndroid(serial)) && await tapTagIfPresent(serial, "parity:offline-sync-button", 10000)) {
      recordStep(result, "offline package sync requested", "region nw");
      await delay(1000);
      await waitFor(() => {
        const nextXml = dumpAndroid(serial);
        return runtimeUiVisible(nextXml) || offlineSyncIsIdle(nextXml);
      }, 600000, "offline package sync completion", 1000);
      recordStep(result, "offline package sync completed");
    }
  }

  for (let attempt = 0; attempt < 4; attempt += 1) {
    if (await acceptDisclaimerIfPresent(serial)) {
      recordStep(result, "disclaimer accepted");
    }
    if (runtimeUiVisible(dumpAndroid(serial))) {
      recordStep(result, "offline packages ready", "runtime available");
      return;
    }
    await tapFirstPresentTag(
      serial,
      ["parity:button:FLIGHT\nPLAN", "parity:button:PLAN", "parity:button:CHART", "parity:button:HOME"],
      10000,
    ).catch(() => {});
    await delay(1000);
  }

  await waitFor(async () => {
    if (await acceptDisclaimerIfPresent(serial)) {
      recordStep(result, "disclaimer accepted");
    }
    return runtimeUiVisible(dumpAndroid(serial));
  }, 45000, "runtime available after offline package sync");
  recordStep(result, "offline packages ready", "runtime available");
}

async function waitForRuntime(serial, result) {
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return runtimeUiVisible(xml);
  }, 45000, "runtime UI ready");
  assertRuntimeIsAvailable(serial);
  recordStep(result, "runtime UI ready");
}

async function ensurePlanPage(serial, result) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (
      PLAN_PAGE_TAGS.some((tag) => findNode(xml, (node) => hasAndroidTag(node, tag))) ||
      findNode(xml, (node) => androidTag(node).startsWith("parity:plan-state:"))
    ) {
      recordStep(result, "plan page visible");
      return;
    }
    assertRuntimeIsAvailable(serial);
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:map-surface") || hasAndroidTag(node, "parity:plate-airport-button"))) {
      await tapTag(serial, "parity:nav-cdi", 10000);
      await delay(500);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:FLIGHT\nPLAN"))) {
      await tapTag(serial, "parity:button:FLIGHT\nPLAN", 10000);
      await delay(500);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:home-button:FlightPlan"))) {
      await tapTag(serial, "parity:home-button:FlightPlan", 10000);
      await delay(500);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:HOME"))) {
      await tapTag(serial, "parity:button:HOME", 10000);
      await delay(400);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:PLAN"))) {
      await tapTag(serial, "parity:button:PLAN", 10000);
      await delay(500);
      continue;
    }
    pressKey(serial, "KEYCODE_BACK");
    await delay(400);
  }
  throwWithUi(serial, "could not navigate to flight plan page");
}

function routeDestination(route) {
  const tokens = route.trim().split(/\s+/).filter(Boolean);
  return tokens[tokens.length - 1] ?? "";
}

function routeInputText(xml) {
  const input = findNode(xml, (node) => hasAndroidTag(node, "parity:plan-append-route-input"));
  if (!input) return "";
  return ((input.text ?? "") || androidNodeLabel(xml, input)).replace(/\s+/g, " ").trim();
}

async function fillRouteInput(serial, route) {
  let lastObserved = "";
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await tapTag(serial, "parity:plan-append-route-input", 10000);
    await delay(300);
    clearFocusedText(serial);
    inputText(serial, route);
    const matched = await waitFor(() => {
      const xml = dumpAndroid(serial);
      lastObserved = routeInputText(xml);
      return lastObserved === route;
    }, 3500, `route input contains ${route}`).then(
      () => true,
      () => false,
    );
    if (matched) return;
  }
  throwWithUi(serial, `route input did not contain ${route}; last observed=${JSON.stringify(lastObserved)}`);
}

async function appendRoute(serial, result, route) {
  await fillRouteInput(serial, route);
  await tapTag(serial, "parity:plan-append-route-input", 10000);
  pressKey(serial, "KEYCODE_ENTER");
  const destination = routeDestination(route);
  const shortDestination = destination.replace(/^K(?=[A-Z]{3}$)/, "");
  await waitFor(async () => {
    const xml = dumpAndroid(serial);
    const planRows = findNodes(xml, (node) => androidTag(node).startsWith("parity:plan-row:"));
    return planRows.length >= 2 &&
      (
        hasAndroidText(xml, destination) ||
        hasAndroidText(xml, shortDestination) ||
        hasAndroidTextContaining(xml, destination) ||
        hasAndroidTextContaining(xml, shortDestination)
      );
  }, 45000, `route ${route} committed to flight plan`);
  recordStep(result, "route committed to flight plan", route);
  recordCheck(result, "flightPlan.routeCommitted", true, route);
  pressKey(serial, "KEYCODE_BACK");
  await delay(600);
}

async function activateDestinationLeg(serial, result, route) {
  const destination = routeDestination(route);
  const shortDestination = destination.replace(/^K(?=[A-Z]{3}$)/, "");
  let rowNode = null;
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    rowNode = findNodes(xml, (node) => androidTag(node).startsWith("parity:plan-row:"))
      .find((node) => {
        const label = androidNodeLabel(xml, node);
        return label.includes(destination) || label.includes(shortDestination);
      }) ?? null;
    return rowNode !== null;
  }, 10000, `destination plan row visible for ${destination}`);
  await tapNode(serial, rowNode);
  await tapTag(serial, "parity:plan-row-action:activate_leg", 10000);
  await delay(1000);
  recordStep(result, "destination leg activated", destination);
}

async function ensureChartPage(serial, result) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (CHART_PAGE_TAGS.some((tag) => findNode(xml, (node) => hasAndroidTag(node, tag)))) {
      recordStep(result, "chart page visible");
      return;
    }
    assertRuntimeIsAvailable(serial);
    const chartButton = findNode(xml, (node) => hasAndroidTag(node, "parity:button:CHART"));
    if (chartButton) {
      await tapNode(serial, chartButton);
      await delay(600);
      continue;
    }
    const homeButton = findNode(xml, (node) => hasAndroidTag(node, "parity:button:HOME"));
    if (homeButton) {
      await tapNode(serial, homeButton);
      await delay(400);
      continue;
    }
    const planButton = findNode(xml, (node) =>
      hasAndroidTag(node, "parity:button:FLIGHT\nPLAN") || hasAndroidTag(node, "parity:button:PLAN")
    );
    if (planButton) {
      await tapNode(serial, planButton);
    }
    await delay(400);
  }
  throwWithUi(serial, "could not navigate to chart page");
}

function chartSearchInputText(xml) {
  const input = findNode(xml, (node) => hasAndroidTag(node, CHART_SEARCH_INPUT_TAG));
  if (!input) return "";
  return ((input.text ?? "") || androidNodeLabel(xml, input)).replace(/\s+/g, " ").trim();
}

async function centerChartOnDestination(serial, result, route) {
  const destination = routeDestination(route);
  if (!destination) return;
  let lastObserved = "";
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await tapTag(serial, CHART_SEARCH_INPUT_TAG, 10000);
    await delay(300);
    clearFocusedText(serial);
    inputText(serial, destination);
    const matched = await waitFor(() => {
      const xml = dumpAndroid(serial);
      lastObserved = chartSearchInputText(xml);
      return lastObserved === destination &&
        findNode(xml, (node) => hasAndroidTag(node, `parity:chart-search-suggestion:${destination}`)) !== null;
    }, 3500, `chart search contains ${destination}`).then(
      () => true,
      () => false,
    );
    if (matched) {
      await tapTag(serial, `parity:chart-search-suggestion:${destination}`, 10000);
      let evidence = null;
      await waitFor(() => {
        evidence = destinationCenterEvidence(dumpAndroid(serial), destination);
        return evidence.matched;
      }, 15000, `destination-specific centered inspector for ${destination}`);
      await dismissMapSelection(serial);
      await waitForNode(
        serial,
        (node) => hasAndroidTag(node, "parity:map-surface"),
        10000,
        "map semantics visible after destination search",
      );
      recordStep(result, "chart centered on destination", `${destination}, ${evidence.probeTag}`);
      return;
    }
  }
  throwWithUi(serial, `chart search did not contain ${destination}; last observed=${JSON.stringify(lastObserved)}`);
}

async function inspectAirportFromChartSearch(serial, result, airportId) {
  let lastObserved = "";
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await tapTag(serial, CHART_SEARCH_INPUT_TAG, 10000);
    await delay(300);
    clearFocusedText(serial);
    inputText(serial, airportId);
    const matched = await waitFor(() => {
      const xml = dumpAndroid(serial);
      lastObserved = chartSearchInputText(xml);
      return lastObserved === airportId &&
        findNode(xml, (node) => hasAndroidTag(node, `parity:chart-search-suggestion:${airportId}`)) !== null;
    }, 7000, `chart search suggestion visible for ${airportId}`).then(
      () => true,
      () => false,
    );
    if (matched) {
      await tapTag(serial, `parity:chart-search-suggestion:${airportId}`, 10000);
      let evidence = null;
      await waitFor(() => {
        evidence = destinationCenterEvidence(dumpAndroid(serial), airportId);
        return evidence.matched;
      }, 15000, `destination-specific centered inspector for ${airportId}`);
      recordStep(result, "airport inspector opened", `${airportId}, ${evidence.probeTag}`);
      return;
    }
  }
  throwWithUi(serial, `chart search did not show ${airportId} suggestion; last observed=${JSON.stringify(lastObserved)}`);
}

async function dismissMapSelection(serial) {
  pressKey(serial, "KEYCODE_BACK");
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return findNode(xml, (node) => hasAndroidTag(node, "parity:map-selection-tray")) === null;
  }, 5000, "map inspector dismissed");
}

async function inspectRawTerrainSpot(serial, result) {
  const surface = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-surface"),
    10000,
    "map surface for raw inspection",
  );
  const rect = rectOfBounds(surface.bounds);
  const x = Math.round(rect.left + rect.width * 0.72);
  const y = Math.round(rect.top + rect.height * 0.72);
  adb(serial, ["shell", "input", "tap", String(x), String(y)]);

  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return findNode(xml, (node) => hasAndroidTag(node, "parity:map-selection-tray")) !== null &&
      findNode(xml, (node) => hasAndroidTag(node, "parity:map-selection-selected:SPOT")) !== null &&
      findNode(xml, (node) => /(?:^| · )Elev -?\d+(?:$| · )/.test(node.text ?? "")) !== null;
  }, 15000, "raw SPOT inspector with terrain elevation");
  recordStep(result, "raw map SPOT inspector opened", `screen=${x},${y}`);
  recordCheck(result, "inspector.rawSpotTerrainElevation", true, "numeric terrain elevation");
}

function parseRouteOverlayTag(tag) {
  const match = /^parity:flight-plan-route-overlay:segments:(\d+):visible:(\d+)$/.exec(tag);
  if (!match) return null;
  return {
    segments: Number(match[1]),
    visible: Number(match[2]),
  };
}

async function waitForRouteOverlay(serial, result) {
  let overlay = null;
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    const node = findNode(xml, (candidate) => androidTag(candidate).startsWith(ROUTE_OVERLAY_PREFIX));
    if (!node) return false;
    const parsed = parseRouteOverlayTag(androidTag(node));
    if (!parsed) return false;
    overlay = {
      ...parsed,
      label: androidNodeLabel(xml, node),
      tag: androidTag(node),
    };
    return parsed.segments > 0 && parsed.visible > 0;
  }, 45000, "visible flight-plan route overlay");
  recordStep(result, "flight-plan route overlay visible", `${overlay.segments} segment(s), ${overlay.visible} visible`);
  recordCheck(result, "chart.flightPlanRouteRendered", true, overlay.tag);
}

function parseMapFollowTag(tag) {
  const match = /^parity:map-follow-state:following:(0|1):ownship-x:(-?\d+):ownship-y:(-?\d+):center-x:(-?\d+):center-y:(-?\d+):zoom-centi:(-?\d+)$/.exec(tag);
  if (!match) return null;
  return {
    following: match[1] === "1",
    ownshipX: Number(match[2]),
    ownshipY: Number(match[3]),
    centerX: Number(match[4]),
    centerY: Number(match[5]),
    zoomCenti: Number(match[6]),
    tag,
  };
}

function mapFollowOffsetPx(probe) {
  return Math.hypot(probe.ownshipX - probe.centerX, probe.ownshipY - probe.centerY);
}

function describeMapFollowProbe(probe) {
  if (!probe) return "<none>";
  return `following=${probe.following} offset=${mapFollowOffsetPx(probe).toFixed(0)}px zoom=${probe.zoomCenti} tag=${probe.tag}`;
}

function findMapFollowProbe(xml) {
  const node = findNode(xml, (candidate) => androidTag(candidate).startsWith(MAP_FOLLOW_PREFIX));
  if (!node) return null;
  return parseMapFollowTag(androidTag(node));
}

async function waitForMapFollowProbe(serial, predicate, timeoutMs, message) {
  let probe = null;
  let lastError = null;
  let deadline = Date.now() + timeoutMs;
  let adbTimeoutRetryAvailable = true;
  while (Date.now() < deadline) {
    const attemptStarted = Date.now();
    try {
      probe = findMapFollowProbe(dumpAndroid(serial));
      if (probe !== null && predicate(probe)) return probe;
    } catch (error) {
      lastError = error;
      if (adbTimeoutRetryAvailable && /spawnSync adb ETIMEDOUT/.test(error.message)) {
        // A single uiautomator dump can consume adb's entire 20-second timeout.
        // Preserve the caller's probe budget after that hosted-runner stall.
        deadline += Date.now() - attemptStarted;
        adbTimeoutRetryAvailable = false;
      }
    }
    await delay(250);
  }
  throw new Error(`${message}${lastError ? `: ${lastError.message}` : ""}`);
}

async function ensureBadAutopilotDebugFlag(serial, result) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (findNode(xml, (node) => hasAndroidTag(node, DEBUG_DIAGNOSTICS_SECTION_TAG))) {
      break;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:home-button:Settings"))) {
      await tapTag(serial, "parity:home-button:Settings", 10000);
    } else if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:HOME"))) {
      await tapTag(serial, "parity:button:HOME", 10000);
    } else {
      pressKey(serial, "KEYCODE_BACK");
    }
    await delay(400);
  }
  let xml = dumpAndroid(serial);
  if (!findNode(xml, (node) => hasAndroidTag(node, DEBUG_DIAGNOSTICS_SECTION_TAG))) {
    throwWithUi(serial, "Debug Diagnostics settings section is not visible");
  }
  await tapTag(serial, DEBUG_DIAGNOSTICS_SECTION_TAG, 5000);
  await delay(300);
  xml = dumpAndroid(serial);
  let checkbox = findNode(xml, (node) => hasAndroidTag(node, BAD_AUTOPILOT_DEBUG_TAG));
  if (!checkbox) {
    throwWithUi(serial, "Bad Autopilot debug flag is not visible");
  }
  if (checkbox.checked !== "true") {
    await tapTag(serial, BAD_AUTOPILOT_DEBUG_TAG, 5000);
    await delay(700);
  }
  await tapTag(serial, "parity:button:CHART", 5000);
  await delay(500);
  recordStep(result, "Bad Autopilot debug source enabled");
}

async function ensureBadAutopilotAvailable(serial, result) {
  await tapTag(serial, "parity:ownship-launcher", 10000);
  await delay(300);
  if (findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG))) {
    pressKey(serial, "KEYCODE_BACK");
    await delay(300);
    recordStep(result, "Bad Autopilot source available");
    return;
  }
  pressKey(serial, "KEYCODE_BACK");
  await delay(300);
  await ensureBadAutopilotDebugFlag(serial, result);
  await tapTag(serial, "parity:ownship-launcher", 10000);
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return findNode(xml, (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG)) !== null;
  }, 10000, "Bad Autopilot source available after enabling debug flag");
  pressKey(serial, "KEYCODE_BACK");
  await delay(300);
  recordStep(result, "Bad Autopilot source available");
}

async function selectBadAutopilotSource(serial, result) {
  await tapTag(serial, "parity:ownship-launcher", 10000);
  const sourceNode = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG),
    10000,
    "Bad Autopilot ownship source",
  );
  if (sourceNode.enabled !== "true") {
    throwWithUi(serial, "Bad Autopilot source is visible but disabled");
  }
  await tapTag(serial, BAD_AUTOPILOT_SOURCE_TAG, 5000);
  await waitForMapFollowProbe(serial, () => true, 45000, "Bad Autopilot ownship probe visible");
  recordStep(result, "Bad Autopilot ownship selected");
}

function cropPlateSurface(rect) {
  return {
    left: Math.round(rect.left + rect.width * 0.22),
    top: Math.round(rect.top + rect.height * 0.20),
    right: Math.round(rect.right - rect.width * 0.22),
    bottom: Math.round(rect.bottom - rect.height * 0.12),
  };
}

function analyzePngCrop(pngBytes, crop) {
  const tmpDir = mkdtempSync(join(tmpdir(), "aerobag-e2e-plate-"));
  const pngPath = join(tmpDir, "screen.png");
  try {
    writeFileSync(pngPath, pngBytes);
    const script = `
from PIL import Image
import json
import math
import sys

path = sys.argv[1]
crop = json.loads(sys.argv[2])
image = Image.open(path).convert("RGB")
left = max(0, min(image.width, int(crop["left"])))
top = max(0, min(image.height, int(crop["top"])))
right = max(left + 1, min(image.width, int(crop["right"])))
bottom = max(top + 1, min(image.height, int(crop["bottom"])))
region = image.crop((left, top, right, bottom))
pixels = list(region.getdata())
step = max(1, len(pixels) // 50000)
sample = pixels[::step]
lumas = [0.2126 * r + 0.7152 * g + 0.0722 * b for (r, g, b) in sample]
mean = sum(lumas) / len(lumas)
variance = sum((value - mean) ** 2 for value in lumas) / len(lumas)
bright = sum(1 for value in lumas if value >= 220.0) / len(lumas)
dark = sum(1 for value in lumas if value <= 80.0) / len(lumas)
quantized = len({(r // 16, g // 16, b // 16) for (r, g, b) in sample})
print(json.dumps({
    "crop": {"left": left, "top": top, "right": right, "bottom": bottom},
    "sampleCount": len(sample),
    "lumaMean": mean,
    "lumaStdDev": math.sqrt(variance),
    "brightRatio": bright,
    "darkRatio": dark,
    "quantizedColorCount": quantized,
}))
`;
    const res = spawnSync("python3", ["-c", script, pngPath, JSON.stringify(crop)], {
      encoding: "utf8",
      timeout: 20000,
    });
    if (res.status !== 0) {
      throw new Error(`screenshot analysis failed: ${res.stderr || res.stdout}`);
    }
    return JSON.parse(res.stdout);
  } finally {
    rmSync(tmpDir, { recursive: true, force: true });
  }
}

function plateImageIsVisiblyPainted(stats) {
  return stats.brightRatio >= 0.10 && stats.lumaMean >= 170 && stats.lumaStdDev >= 18;
}

async function waitForPlateImagePainted(serial, result) {
  let lastStats = null;
  let lastCrop = null;
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    const surface = findNode(xml, (node) => hasAndroidTag(node, PLATE_SURFACE_TAG));
    if (!surface) return false;
    lastCrop = cropPlateSurface(rectOfBounds(surface.bounds));
    lastStats = analyzePngCrop(screencapPng(serial), lastCrop);
    return plateImageIsVisiblyPainted(lastStats);
  }, 45000, "plate image visibly painted on first open", 1000).catch((error) => {
    throw new Error(`${error.message}; lastCrop=${JSON.stringify(lastCrop)}; lastStats=${JSON.stringify(lastStats)}`);
  });
  recordStep(
    result,
    "plate image visibly painted",
    `mean=${lastStats.lumaMean.toFixed(1)} stdev=${lastStats.lumaStdDev.toFixed(1)} bright=${lastStats.brightRatio.toFixed(3)}`,
  );
  recordCheck(result, "plate.firstOpenVisiblyPainted", true, JSON.stringify(lastStats));
}

async function openFirstPlateFromAirportInspector(serial, result, airportId) {
  await inspectAirportFromChartSearch(serial, result, airportId);

  const airportItemTag = `parity:map-selection-item:airport-${airportId}`;
  if (findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, airportItemTag))) {
    await tapTag(serial, airportItemTag, 5000);
    await delay(300);
  }
  const platesAction = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-selection-action:plates"),
    10000,
    "airport plates action",
  );
  if (platesAction.enabled !== "true") {
    throwWithUi(serial, `airport plates action is disabled for ${airportId}`);
  }
  await tapTag(serial, "parity:map-selection-action:plates", 10000);
  await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:plate-folder-button"),
    15000,
    "plate folder page opened",
  );
  recordStep(result, "plate folder opened", airportId);

  const firstTile = await waitForNode(
    serial,
    (node) => androidTag(node).startsWith(PLATE_FOLDER_TILE_PREFIX),
    45000,
    "first plate folder tile",
  );
  const firstTileTag = androidTag(firstTile);
  await tapTag(serial, firstTileTag, 10000);
  await waitForNode(serial, (node) => hasAndroidTag(node, PLATE_SURFACE_TAG), 15000, "plate surface after tile selection");
  recordStep(result, "first plate opened", firstTileTag.slice(PLATE_FOLDER_TILE_PREFIX.length));
}

async function ensureMapFollowEngaged(serial, result) {
  let probe = await waitForMapFollowProbe(serial, () => true, 10000, "map-follow probe visible");
  if (!probe.following) {
    await tapTag(serial, "parity:button:CTR", 10000);
    probe = await waitForMapFollowProbe(serial, (nextProbe) => nextProbe.following, 10000, "CTR follow engaged");
  }
  await waitForMapFollowProbe(
    serial,
    (nextProbe) => nextProbe.following && mapFollowOffsetPx(nextProbe) <= 120,
    15000,
    "CTR follow centered on ownship",
  );
  recordStep(result, "CTR follow engaged");
}

async function disengageMapFollowForRouteVisibility(serial, result) {
  const probe = await waitForMapFollowProbe(serial, () => true, 30000, "map-follow probe visible");
  if (!probe.following) return;
  await tapTag(serial, "parity:button:CTR", 10000);
  await waitForMapFollowProbe(
    serial,
    (nextProbe) => !nextProbe.following,
    30000,
    "CTR follow disengaged",
  );
  recordStep(result, "CTR follow disengaged for route visibility");
}

async function prepareRouteViewportForRotations(serial, result, route, expectedSignature) {
  await ensureChartPage(serial, result);
  await disengageMapFollowForRouteVisibility(serial, result);
  await centerChartOnDestination(serial, result, route);
  await waitForRouteOverlay(serial, result);
  await ensurePlanPage(serial, result);
  await waitForPlanSignature(serial, expectedSignature, 15000);
}

async function dragMapWhileFollowing(serial, result) {
  const surfaceNode = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-surface"),
    10000,
    "map surface bounds",
  );
  const rect = rectOfBounds(surfaceNode.bounds);
  const startX = rect.left + rect.width * 0.50;
  const startY = rect.top + rect.height * 0.54;
  const endX = rect.left + rect.width * 0.72;
  const endY = startY;
  swipe(serial, startX, startY, endX, endY, 650);
  let probe = null;
  try {
    probe = await waitForMapFollowProbe(
      serial,
      (nextProbe) => nextProbe.following && mapFollowOffsetPx(nextProbe) >= 80,
      8000,
      "ownship remains off-center after map drag with CTR engaged",
    );
  } catch (error) {
    probe = findMapFollowProbe(dumpAndroid(serial));
    throw new Error(`${error.message}; lastProbe=${describeMapFollowProbe(probe)}`);
  }
  await delay(1200);
  const settled = await waitForMapFollowProbe(
    serial,
    (nextProbe) => nextProbe.following,
    3000,
    "map-follow probe after drag settle",
  );
  const settledOffset = mapFollowOffsetPx(settled);
  recordCheck(result, "chart.ctrDragKeepsOwnshipOffset", settledOffset >= 80, `offset=${settledOffset.toFixed(0)}px tag=${settled.tag}`);
  recordStep(result, "map drag preserved CTR offset", `offset=${mapFollowOffsetPx(probe).toFixed(0)}px`);
}

async function zoomMapWhileFollowing(serial, result) {
  const before = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && mapFollowOffsetPx(probe) >= 80,
    5000,
    "map-follow offset before zoom",
  );

  pressKey(serial, "KEYCODE_PLUS");
  const firstZoomed = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && probe.zoomCenti > before.zoomCenti,
    8000,
    "first map zoom changed while CTR was engaged",
  );
  await delay(1200);
  const firstSettled = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && probe.zoomCenti >= firstZoomed.zoomCenti,
    3000,
    "map-follow probe after first zoom settled",
  );
  const firstSettledOffset = mapFollowOffsetPx(firstSettled);
  recordCheck(
    result,
    "chart.ctrFirstZoomKeepsOwnshipOffset",
    firstSettledOffset >= 80,
    `offset=${firstSettledOffset.toFixed(0)}px tag=${firstSettled.tag}`,
  );

  pressKey(serial, "KEYCODE_PLUS");
  const secondZoomed = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && probe.zoomCenti > firstSettled.zoomCenti,
    8000,
    "second map zoom changed while CTR was engaged",
  );
  await delay(1200);
  const settled = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && probe.zoomCenti >= secondZoomed.zoomCenti,
    3000,
    "map-follow probe after second zoom settled",
  );
  const settledOffset = mapFollowOffsetPx(settled);
  recordCheck(result, "chart.ctrZoomKeepsFollowing", settled.following, settled.tag);
  recordCheck(result, "chart.ctrZoomKeepsOwnshipOffset", settledOffset >= 80, `offset=${settledOffset.toFixed(0)}px tag=${settled.tag}`);
  recordStep(result, "map zoom preserved CTR offset", `${before.zoomCenti} -> ${settled.zoomCenti}`);
}

function layerToggleIsOn(xml, layerId) {
  return layerToggleNode(xml, layerId)?.checked === "true";
}

async function openLayersTray(serial, result) {
  let xml = dumpAndroid(serial);
  if (!layerToggleNode(xml, "terrain_warning")) {
    await tapTag(serial, "parity:layers-button", 10000);
    await waitFor(() => layerToggleNode(dumpAndroid(serial), "terrain_warning") !== null, 10000, "Layers tray");
    xml = dumpAndroid(serial);
  }
  recordStep(result, "Layers tray opened");
  return xml;
}

function rejectedLayerCommandCount(serial) {
  const logcat = adb(serial, ["logcat", "-d", "-v", "brief", "-s", "AerobagSessionCommand"]);
  if (!logcat.includes("session snapshot requires nav-kv resources in non-paged API")) return 0;
  return (logcat.match(/recovered rejected session command command=setMapLayerVisibility/g) ?? []).length;
}

function captureLayerToggleRegressionScreenshot(serial, result) {
  mkdirSync(E2E_ARTIFACT_DIR, { recursive: true });
  const screenshotPath = join(E2E_ARTIFACT_DIR, "android-layer-toggle-navdb-regression.png");
  writeFileSync(screenshotPath, screencapPng(serial));
  result.diagnostics.screenshot = screenshotPath;
  recordStep(result, "layer-toggle regression screenshot captured", screenshotPath);
  return screenshotPath;
}

async function runLayerToggleNavDbRegression(args) {
  const { serial, route } = args;
  const result = createTestResult("android.layer-toggle-navdb-regression");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, {
    clearUiPrefs: true,
    clearCoreSettings: false,
    armLayerNavKvFault: true,
  });
  recordStep(result, "app launched", serial || "default adb device");
  recordStep(result, "nav-db faults armed for the next two layer commands");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensurePlanPage(serial, result);
  await appendRoute(serial, result, route);
  await ensurePlanPage(serial, result);
  await activateDestinationLeg(serial, result, route);
  await ensureChartPage(serial, result);
  await centerChartOnDestination(serial, result, route);
  await waitForRouteOverlay(serial, result);
  await ensureBadAutopilotAvailable(serial, result);
  await selectBadAutopilotSource(serial, result);

  let xml = await openLayersTray(serial, result);
  recordCheck(result, "layers.terrainInitiallyOn", layerToggleIsOn(xml, "terrain_warning"));
  recordCheck(result, "layers.nexradInitiallyOff", !layerToggleIsOn(xml, "nexrad"));

  await tapTag(serial, layerToggleTag("terrain_warning"), 10000);
  await delay(500);
  recordCheck(result, "layers.terrainCommandAccepted", rejectedLayerCommandCount(serial) === 0);
  recordStep(result, "Terrain disabled without a session-command warning");

  xml = await openLayersTray(serial, result);
  recordCheck(result, "layers.terrainTurnedOff", !layerToggleIsOn(xml, "terrain_warning"));
  await tapTag(serial, layerToggleTag("nexrad"), 10000);
  await delay(500);
  const screenshotPath = captureLayerToggleRegressionScreenshot(serial, result);
  recordCheck(result, "layers.nexradCommandAccepted", rejectedLayerCommandCount(serial) === 0, screenshotPath);
  recordStep(result, "NEXRAD enabled without a session-command warning");

  xml = dumpAndroid(serial);
  recordCheck(result, "layers.nexradTurnedOn", layerToggleIsOn(xml, "nexrad"));
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function runFlightPlanRouteSmoke(args) {
  const { serial, route } = args;
  const result = createTestResult("android.flight-plan-route-smoke");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: false });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensurePlanPage(serial, result);
  await appendRoute(serial, result, route);
  await ensureChartPage(serial, result);
  await centerChartOnDestination(serial, result, route);
  await waitForRouteOverlay(serial, result);
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

function signaturesEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function planContentsEqual(left, right) {
  return left.rowCount === right.rowCount && JSON.stringify(left.rows) === JSON.stringify(right.rows);
}

async function waitForPlanSignature(serial, expected = null, timeoutMs = 15000) {
  let signature = null;
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    assertNoAerobagAnr(xml);
    try {
      signature = renderedFlightPlanSignature(xml);
    } catch (_error) {
      return false;
    }
    return expected === null || signaturesEqual(signature, expected);
  }, timeoutMs, expected === null ? "rendered flight-plan signature" : "preserved flight-plan signature");
  return signature;
}

function writeRotationArtifacts(result, transcript, beforeSignature, afterSignature, label) {
  mkdirSync(E2E_ARTIFACT_DIR, { recursive: true });
  const transcriptPath = join(E2E_ARTIFACT_DIR, `android-rotation-${label}-transcript.json`);
  const signaturesPath = join(E2E_ARTIFACT_DIR, `android-rotation-${label}-plan-signatures.json`);
  writeFileSync(transcriptPath, `${JSON.stringify(transcript, null, 2)}\n`);
  writeFileSync(signaturesPath, `${JSON.stringify({ before: beforeSignature, after: afterSignature }, null, 2)}\n`);
  result.diagnostics[`${label}RotationTranscript`] = transcriptPath;
  result.diagnostics[`${label}PlanSignatures`] = signaturesPath;
}

async function exerciseRetainedPlanAcrossRotations(
  serial,
  result,
  baselineSignature,
  transitionCount,
  transcript,
  label,
) {
  const originalRotation = saveAndroidRotationState(serial);
  const initialPid = currentAerobagPid(serial);
  recordCheck(result, "rotation.initialPidAvailable", initialPid !== null, String(initialPid));
  let finalSignature = baselineSignature;
  try {
    lockAndroidRotation(serial);
    for (let transition = 0; transition < transitionCount; transition += 1) {
      const orientation = transition % 2 === 0 ? "landscape" : "portrait";
      setAndroidRotation(serial, orientation);
      const bounds = await waitForAndroidOrientation(serial, orientation, 15000);
      const signature = await waitForPlanSignature(serial, baselineSignature, 15000);
      const pid = currentAerobagPid(serial);
      recordCheck(result, `rotation.${transition + 1}.pidStable`, pid === initialPid, `${initialPid} -> ${pid}`);
      await ensureChartPage(serial, result);
      await waitForRouteOverlay(serial, result);
      await ensurePlanPage(serial, result);
      finalSignature = await waitForPlanSignature(serial, baselineSignature, 15000);
      transcript.push({
        transition: transition + 1,
        orientation,
        bounds,
        pid,
        signature,
        navigationResponsive: true,
      });
      recordStep(result, "rotation retained active plan", `${transition + 1}/${transitionCount} ${orientation} ${bounds.width}x${bounds.height}`);
    }
  } finally {
    restoreAndroidRotationState(serial, originalRotation);
    writeRotationArtifacts(result, transcript, baselineSignature, finalSignature, label);
  }
  return finalSignature;
}

async function waitForLogcatMarker(serial, marker, timeoutMs = 30000) {
  await waitFor(() => {
    const logcat = adb(serial, ["logcat", "-d", "-v", "brief"], { maxBuffer: 8 * 1024 * 1024 });
    return logcat.includes(marker);
  }, timeoutMs, `logcat marker ${marker}`);
}

function logcatMarkerCount(serial, marker) {
  const logcat = adb(serial, ["logcat", "-d", "-v", "brief"], { maxBuffer: 8 * 1024 * 1024 });
  return logcat.split(marker).length - 1;
}

async function verifyNotamsLoadedInUi(serial, result) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const xml = dumpAndroid(serial);
    const loadedNode = findNode(xml, (node) => {
      const tag = androidTag(node);
      return tag.startsWith("parity:data-status-row:live_feed:notams:") && !tag.endsWith(":MISSING");
    });
    if (loadedNode) {
      recordStep(result, "persisted NOTAM visible in core status UI", androidTag(loadedNode));
      return;
    }
    const statusRows = findNodes(xml, (node) => androidTag(node).startsWith("parity:data-status-row:"));
    if (statusRows.length > 0) {
      const scrollSurface = findNode(xml, (node) => node.scrollable === "true" && node.package === ANDROID_PACKAGE);
      const bounds = rectOfBounds(scrollSurface?.bounds ?? "[20,180][1060,2100]");
      swipe(
        serial,
        Math.round((bounds.left + bounds.right) / 2),
        Math.round(bounds.bottom - 100),
        Math.round((bounds.left + bounds.right) / 2),
        Math.round(bounds.top + 100),
        450,
      );
      await delay(350);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:home-button:DataStatus"))) {
      await tapTag(serial, "parity:home-button:DataStatus", 10000);
      await delay(500);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:HOME"))) {
      await tapTag(serial, "parity:button:HOME", 10000);
      await delay(500);
      continue;
    }
    pressKey(serial, "KEYCODE_BACK");
    await delay(400);
  }
  throwWithUi(serial, "persisted NOTAM did not appear loaded in Data Status");
}

async function runPersistedLiveFeedRotationPhase(args, result, baselineSignature) {
  const { serial } = args;
  if (!ROTATION_LIVE_FEED_FIXTURE) {
    throw new Error("AEROBAG_ROTATION_LIVE_FEED_FIXTURE or AEROBAG_TEST_ARTIFACTS_ROOT is required");
  }
  seedAerobagPrivateFiles(serial, join(ROTATION_LIVE_FEED_FIXTURE, "files"));
  setAerobagPrivateSentinel(serial, LIVE_FEED_PROMOTION_SENTINEL, true);
  adb(serial, ["logcat", "-c"]);
  const pausedTranscript = [];
  const promotedTranscript = [];
  try {
    await launchFreshAndroidApp(serial, { clearUiPrefs: false, clearCoreSettings: false });
    await waitForLogcatMarker(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER);
    recordStep(result, "persisted live-feed promotion paused at deterministic gate");
    await ensurePlanPage(serial, result);
    const restoredSignature = await waitForPlanSignature(serial, null, 15000);
    recordCheck(
      result,
      "rotation.liveFeedPhaseRouteRestored",
      planContentsEqual(restoredSignature, baselineSignature),
      JSON.stringify(restoredSignature),
    );
    await activateDestinationLeg(serial, result, ROTATION_ROUTE);
    const liveFeedBaselineSignature = await waitForPlanSignature(serial, null, 15000);
    recordCheck(
      result,
      "rotation.liveFeedPhaseActiveLegProjected",
      !liveFeedBaselineSignature.stateTag.includes(":from:none:to:") &&
        !liveFeedBaselineSignature.stateTag.endsWith(":to:none"),
      liveFeedBaselineSignature.stateTag,
    );
    await prepareRouteViewportForRotations(
      serial,
      result,
      ROTATION_ROUTE,
      liveFeedBaselineSignature,
    );
    await exerciseRetainedPlanAcrossRotations(
      serial,
      result,
      liveFeedBaselineSignature,
      2,
      pausedTranscript,
      "live-feed-paused",
    );
    setAerobagPrivateSentinel(serial, LIVE_FEED_PROMOTION_SENTINEL, false);
    await verifyNotamsLoadedInUi(serial, result);
    await ensurePlanPage(serial, result);
    await waitForPlanSignature(serial, liveFeedBaselineSignature, 15000);
    await prepareRouteViewportForRotations(
      serial,
      result,
      ROTATION_ROUTE,
      liveFeedBaselineSignature,
    );
    await exerciseRetainedPlanAcrossRotations(
      serial,
      result,
      liveFeedBaselineSignature,
      2,
      promotedTranscript,
      "live-feed-promoted",
    );
  } finally {
    setAerobagPrivateSentinel(serial, LIVE_FEED_PROMOTION_SENTINEL, false);
  }
  recordCheck(
    result,
    "rotation.liveFeedPipelineStartedOnce",
    logcatMarkerCount(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER) === 1,
    `marker count=${logcatMarkerCount(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER)}`,
  );
  await ensureChartPage(serial, result);
  await disengageMapFollowForRouteVisibility(serial, result);
  await centerChartOnDestination(serial, result, ROTATION_ROUTE);
  await waitForRouteOverlay(serial, result);
  const { logcat, evidence } = scanAerobagLogcat(serial);
  const logcatPath = join(E2E_ARTIFACT_DIR, "android-rotation-live-feed-logcat.txt");
  writeFileSync(logcatPath, logcat);
  result.diagnostics.liveFeedRotationLogcat = logcatPath;
  recordCheck(result, "rotation.liveFeedNoAerobagFatalEvidence", evidence.length === 0, evidence.join("\n"));
}

async function runRotationSessionRetentionRegression(args) {
  const { serial } = args;
  const route = ROTATION_ROUTE;
  const result = createTestResult("android.rotation-session-retention-regression");
  const transcript = [];
  clearAerobagPersistedLiveFeeds(serial);
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: true });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) recordStep(result, "disclaimer accepted");
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensurePlanPage(serial, result);
  await appendRoute(serial, result, route);
  await ensurePlanPage(serial, result);
  await activateDestinationLeg(serial, result, route);
  await ensureChartPage(serial, result);
  await centerChartOnDestination(serial, result, route);
  await waitForRouteOverlay(serial, result);
  await ensurePlanPage(serial, result);
  const baselineSignature = await waitForPlanSignature(serial);
  recordCheck(
    result,
    "rotation.planHasExactRoute",
    baselineSignature.rowCount === 4 &&
      baselineSignature.rows.map((row) => row.label).join(" ") === ROTATION_ROUTE,
    JSON.stringify(baselineSignature),
  );
  recordCheck(
    result,
    "rotation.activeLegProjected",
    !baselineSignature.stateTag.includes(":from:none:to:") &&
      !baselineSignature.stateTag.endsWith(":to:none"),
    baselineSignature.stateTag,
  );

  adb(serial, ["logcat", "-c"]);
  await exerciseRetainedPlanAcrossRotations(
    serial,
    result,
    baselineSignature,
    6,
    transcript,
    "plan",
  );
  await ensureChartPage(serial, result);
  await waitForRouteOverlay(serial, result);
  const { logcat, evidence } = scanAerobagLogcat(serial);
  const logcatPath = join(E2E_ARTIFACT_DIR, "android-rotation-logcat.txt");
  writeFileSync(logcatPath, logcat);
  result.diagnostics.rotationLogcat = logcatPath;
  recordCheck(result, "rotation.noAerobagFatalEvidence", evidence.length === 0, evidence.join("\n"));
  await runPersistedLiveFeedRotationPhase(args, result, baselineSignature);
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function runMapFollowCtrGestureSmoke(args) {
  const { serial, route } = args;
  const result = createTestResult("android.map-follow-ctr-gesture-smoke");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: false });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensurePlanPage(serial, result);
  await appendRoute(serial, result, route);
  await ensurePlanPage(serial, result);
  await activateDestinationLeg(serial, result, route);
  await ensureChartPage(serial, result);
  await centerChartOnDestination(serial, result, route);
  await waitForRouteOverlay(serial, result);
  await ensureBadAutopilotAvailable(serial, result);
  await selectBadAutopilotSource(serial, result);
  await ensureMapFollowEngaged(serial, result);
  await dragMapWhileFollowing(serial, result);
  for (let cycle = 1; cycle <= 8; cycle += 1) {
    await zoomMapWhileFollowing(serial, result);
    recordStep(result, "CTR zoom stress cycle", String(cycle));
    pressKey(serial, "KEYCODE_MINUS");
    pressKey(serial, "KEYCODE_MINUS");
    await delay(1200);
  }
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function runPlateFirstRenderSmoke(args) {
  const { serial } = args;
  const result = createTestResult("android.plate-first-render-smoke");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: false });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensureChartPage(serial, result);
  await openFirstPlateFromAirportInspector(serial, result, "KPLU");
  await waitForPlateImagePainted(serial, result);
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function runRawMapInspectorTerrainSmoke(args) {
  const { serial } = args;
  const result = createTestResult("android.raw-map-inspector-terrain-smoke");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: false });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, args);
  await waitForRuntime(serial, result);
  await ensureChartPage(serial, result);
  await inspectAirportFromChartSearch(serial, result, "KPLU");
  await dismissMapSelection(serial);
  await inspectRawTerrainSpot(serial, result);
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

const tests = [
  {
    id: "android.flight-plan-route-smoke",
    run: runFlightPlanRouteSmoke,
  },
  {
    id: "android.plate-first-render-smoke",
    run: runPlateFirstRenderSmoke,
  },
  {
    id: "android.raw-map-inspector-terrain-smoke",
    run: runRawMapInspectorTerrainSmoke,
  },
  {
    id: "android.map-follow-ctr-gesture-smoke",
    run: runMapFollowCtrGestureSmoke,
  },
  {
    id: "android.layer-toggle-navdb-regression",
    run: runLayerToggleNavDbRegression,
  },
  {
    id: "android.rotation-session-retention-regression",
    run: runRotationSessionRetentionRegression,
  },
];

async function main() {
  const args = parseArgs(process.argv);
  if (args.help) {
    usage();
    return;
  }
  if (!args.route.trim()) {
    throw new Error("--route must not be empty");
  }
  const selectedTests = args.test
    ? tests.filter((test) => test.id === args.test)
    : tests;
  if (selectedTests.length === 0) {
    throw new Error(`unknown test ${JSON.stringify(args.test)}; available=${tests.map((test) => test.id).join(", ")}`);
  }
  const suite = {
    suite: "android-e2e",
    package: ANDROID_PACKAGE,
    serial: args.serial || null,
    started_at: new Date().toISOString(),
    results: [],
  };
  for (const test of selectedTests) {
    console.log(`# ${test.id}`);
    try {
      suite.results.push(await test.run(args));
    } catch (error) {
      const failed = createTestResult(test.id);
      failed.status = "fail";
      failed.finished_at = new Date().toISOString();
      failed.error = error.message;
      failed.artifacts = captureAndroidFailureDiagnostics(args.serial, E2E_ARTIFACT_DIR, test.id);
      suite.results.push(failed);
      if (args.json) {
        console.log(JSON.stringify(suite, null, 2));
      }
      throw error;
    }
  }
  suite.finished_at = new Date().toISOString();
  suite.status = suite.results.every((entry) => entry.status === "pass") ? "pass" : "fail";
  if (args.json) {
    console.log(JSON.stringify(suite, null, 2));
  } else {
    console.log(`# RESULT ${suite.status}: ${suite.results.length} test(s)`);
  }
}

main().catch((error) => {
  console.error(`E2E FAILED: ${error.message}`);
  process.exit(1);
});
