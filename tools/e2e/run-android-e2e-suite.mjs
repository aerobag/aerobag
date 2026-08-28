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
  androidJourneyEpochMs,
  androidOfflinePackagesVisible,
  androidRuntimeReadyForJourney,
  androidRuntimeUiVisible,
  androidTag,
  assertRuntimeIsAvailable,
  captureAndroidFailureDiagnostics,
  clearAerobagPersistedLiveFeeds,
  currentAerobagPid,
  destinationCenterEvidence,
  dumpAndroid,
  findNode,
  findNodeByScrolling,
  findNodes,
  hasAndroidTag,
  hasAndroidText,
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
  shutdownAndroidSemanticDrivers,
  tapNode,
  tapTag,
  waitFor,
  waitForAndroidOrientation,
  waitForNode,
} from "./android-harness.mjs";
import { loadReleaseJourneyFixture } from "./release-journey-fixture.mjs";
import {
  offlineSyncButtonIsIdle,
  releaseJourneyImplementation,
} from "./release-journey-implementations.mjs";
import { RELEASE_JOURNEYS } from "./release-journey-registry.mjs";
import { executeReleaseJourney } from "./release-journey-runtime.mjs";
import { AndroidSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import {
  assertConditionRemains,
  E2E_TIMING,
  observeUntil,
  performTransition,
} from "./transition-contract.mjs";

const DEFAULT_ROUTE = "KRNT KPWT";
const CTR_STRESS_ROUTE = "KRNT KPDX";
const ROTATION_ROUTE = "KRNT KPWT KPLU";
const DEFAULT_PACKAGE_SOURCE_PORT = process.env.PACKAGE_SOURCE_PORT ?? "8083";
const OFFLINE_REGION_IDS = ["ak", "ec", "nc", "ne", "nw", "pac", "sc", "se", "sw"];
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

function nativeSemanticDriver(serial) {
  return new AndroidSemanticJourneyDriver(serial, {
    resetApp: async () => {
      throw new Error("native journey reset must be performed by its explicit bootstrap");
    },
  });
}

async function nativeTransition(result, description, contract) {
  const completed = await performTransition(description, {
    ...contract,
    onTiming(timing) {
      result.diagnostics.userTransitions ??= [];
      result.diagnostics.userTransitions.push(timing);
      contract.onTiming?.(timing);
    },
  });
  recordStep(
    result,
    description,
    `${completed.timing.response_ms}ms response, ${completed.timing.total_ms}ms total`,
  );
  return completed.value;
}

function usage() {
  console.log(`Usage:
  node tools/e2e/run-android-e2e-suite.mjs [--serial emulator-5554] [--route "KRNT KPWT"] [--package-source-port 8083] [--release-fixture fixture.json] [--no-sync-offline-packages] [--sync-all-available-packages] [--test TEST_ID] [--json]

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
    releaseFixture: process.env.AEROBAG_RELEASE_JOURNEY_FIXTURE ?? "",
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
    } else if (arg === "--release-fixture") {
      args.releaseFixture = argv[++i] ?? "";
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
  return androidOfflinePackagesVisible(xml);
}

function disclaimerVisible(xml) {
  return findNode(xml, (node) => hasAndroidTag(node, "parity:disclaimer-accept-button")) !== null;
}

function runtimeUiVisible(xml) {
  return androidRuntimeUiVisible(xml);
}

function offlineSyncIsIdle(xml) {
  const node = findNode(xml, (candidate) => hasAndroidTag(candidate, "parity:offline-sync-button"));
  if (!node) return false;
  return offlineSyncButtonIsIdle({
    enabled: node.enabled === "true",
    text: androidNodeLabel(xml, node),
  });
}

async function tapTagIfPresent(serial, tag, timeoutMs = E2E_TIMING.localReadyMs) {
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
  }, E2E_TIMING.startupMs, "runtime or offline package UI ready");

  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  if (androidRuntimeReadyForJourney(dumpAndroid(serial))) {
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
      if (await tapTagIfPresent(serial, "parity:offline-refresh-button", E2E_TIMING.localReadyMs)) {
        recordStep(result, "offline package library refresh requested");
        await waitFor(() => {
          const nextXml = dumpAndroid(serial);
          return findNode(nextXml, (node) =>
            hasAndroidTag(node, "parity:offline-packages-panel") ||
            hasAndroidTag(node, "parity:offline-sync-button")
          );
        }, E2E_TIMING.bulkOperationMs, "offline package planner after refresh", E2E_TIMING.resourcePollIntervalMs);
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
        const before = findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag));
        const selected = before?.selected === "true" || before?.checked === "true";
        if (selected && await tapTagIfPresent(serial, tag, 1200)) {
          recordStep(result, "offline region deselected", regionId);
          await waitFor(() => {
            const after = findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag));
            return after && after.selected !== "true" && after.checked !== "true";
          }, E2E_TIMING.userResponseMs, `offline region ${regionId} deselection`, E2E_TIMING.pollIntervalMs);
        }
      }
    }

    if (offlineSyncIsIdle(dumpAndroid(serial)) && await tapTagIfPresent(
      serial,
      "parity:offline-sync-button",
      E2E_TIMING.localReadyMs,
    )) {
      recordStep(result, "offline package sync requested", "region nw");
      await waitFor(() => {
        const nextXml = dumpAndroid(serial);
        return runtimeUiVisible(nextXml) || disclaimerVisible(nextXml);
      }, E2E_TIMING.offlineSyncMs, "runtime loaded after offline package sync", E2E_TIMING.resourcePollIntervalMs);
      recordStep(result, "offline package sync completed", "runtime loaded");
    }
  }

  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  xml = dumpAndroid(serial);
  if (offlinePackagesVisible(xml) && runtimeUiVisible(xml)) {
    await tapTag(serial, "parity:button:HOME", E2E_TIMING.localReadyMs);
    await waitFor(
      () => androidRuntimeReadyForJourney(dumpAndroid(serial)),
      E2E_TIMING.localReadyMs,
      "Home page after offline package bootstrap",
      50,
    );
    recordStep(result, "offline package page dismissed", "Home page visible");
  }
  await waitFor(
    () => androidRuntimeReadyForJourney(dumpAndroid(serial)),
    E2E_TIMING.startupMs,
    "runtime available after offline package sync",
  );
  recordStep(result, "offline packages ready", "runtime available");
}

async function waitForRuntime(serial, result) {
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return runtimeUiVisible(xml);
  }, E2E_TIMING.startupMs, "runtime UI ready");
  assertRuntimeIsAvailable(serial);
  recordStep(result, "runtime UI ready");
}

async function ensurePlanPage(serial, result) {
  assertRuntimeIsAvailable(serial);
  await nativeSemanticDriver(serial).openPage("flight_plan");
  recordStep(result, "plan page visible");
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
  await nativeSemanticDriver(serial).enterText("plan-append-route-input", route);
}

async function appendRoute(serial, result, route, assertionId = "flightPlan.routeCommitted") {
  await fillRouteInput(serial, route);
  const destination = routeDestination(route);
  const shortDestination = destination.replace(/^K(?=[A-Z]{3}$)/, "");
  await nativeTransition(result, `route ${route} committed to flight plan`, {
    ready: async () => routeInputText(dumpAndroid(serial)) === route,
    act: async () => pressKey(serial, "KEYCODE_ENTER"),
    complete: async () => {
      const xml = dumpAndroid(serial);
      const planRows = findNodes(xml, (node) => androidTag(node).startsWith("parity:plan-row:"));
      return planRows.length >= 2 &&
        (
          hasAndroidText(xml, destination) ||
          hasAndroidText(xml, shortDestination) ||
          hasAndroidTextContaining(xml, destination) ||
          hasAndroidTextContaining(xml, shortDestination)
        );
    },
  });
  if (assertionId) recordCheck(result, assertionId, true, route);
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
  }, E2E_TIMING.localReadyMs, `destination plan row visible for ${destination}`);
  await tapNode(serial, rowNode);
  const activate = await waitForNode(
    serial,
    (node) => androidTag(node).startsWith("parity:plan-row-action:activate_leg:enabled:true"),
    E2E_TIMING.localReadyMs,
    "enabled activate-leg action",
  );
  await nativeTransition(result, `destination leg activated ${destination}`, {
    ready: async () => findNode(
      dumpAndroid(serial),
      (node) => androidTag(node).startsWith("parity:plan-row-action:activate_leg:enabled:true"),
    ),
    act: async () => tapNode(serial, activate),
    complete: async () => findNode(
      dumpAndroid(serial),
      (node) => androidTag(node).startsWith("parity:plan-state:") &&
        !androidTag(node).includes(":from:none:to:") &&
        !androidTag(node).endsWith(":to:none"),
    ),
  });
}

async function ensureChartPage(serial, result) {
  assertRuntimeIsAvailable(serial);
  await nativeSemanticDriver(serial).openPage("map");
  recordStep(result, "chart page visible");
}

function chartSearchInputText(xml) {
  const input = findNode(xml, (node) => hasAndroidTag(node, CHART_SEARCH_INPUT_TAG));
  if (!input) return "";
  return ((input.text ?? "") || androidNodeLabel(xml, input)).replace(/\s+/g, " ").trim();
}

async function centerChartOnDestination(serial, result, route) {
  const destination = routeDestination(route);
  if (!destination) return;
  const driver = nativeSemanticDriver(serial);
  await driver.enterText("chart-search-input", destination);
  let evidence = null;
  await nativeTransition(result, `chart centered on destination ${destination}`, {
    ready: async () => {
      const xml = dumpAndroid(serial);
      return chartSearchInputText(xml) === destination &&
        findNode(xml, (node) => hasAndroidTag(node, `parity:chart-search-suggestion:${destination}`));
    },
    act: async () => driver.performAction(`chart-search-suggestion-${destination}`),
    complete: async () => {
      evidence = destinationCenterEvidence(dumpAndroid(serial), destination);
      return evidence.matched ? evidence : null;
    },
    responseTimeoutMs: E2E_TIMING.observationMs,
  });
  await dismissMapSelection(serial);
  await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-surface"),
    E2E_TIMING.localReadyMs,
    "map semantics visible after destination search",
  );
  recordStep(result, "chart centered on destination", `${destination}, ${evidence.probeTag}`);
}

async function inspectAirportFromChartSearch(serial, result, airportId) {
  const driver = nativeSemanticDriver(serial);
  await driver.enterText("chart-search-input", airportId);
  let evidence = null;
  await nativeTransition(result, `airport inspector opened for ${airportId}`, {
    ready: async () => {
      const xml = dumpAndroid(serial);
      return chartSearchInputText(xml) === airportId &&
        findNode(xml, (node) => hasAndroidTag(node, `parity:chart-search-suggestion:${airportId}`));
    },
    act: async () => driver.performAction(`chart-search-suggestion-${airportId}`),
    complete: async () => {
      evidence = destinationCenterEvidence(dumpAndroid(serial), airportId);
      return evidence.matched ? evidence : null;
    },
    responseTimeoutMs: E2E_TIMING.observationMs,
  });
  recordStep(result, "airport inspector opened", `${airportId}, ${evidence.probeTag}`);
}

async function dismissMapSelection(serial) {
  pressKey(serial, "KEYCODE_BACK");
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    return findNode(xml, (node) => hasAndroidTag(node, "parity:map-selection-tray")) === null;
  }, E2E_TIMING.localReadyMs, "map inspector dismissed");
}

async function inspectRawTerrainSpot(serial, result) {
  const surface = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-surface"),
    E2E_TIMING.localReadyMs,
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
  }, E2E_TIMING.observationMs, "raw SPOT inspector with terrain elevation");
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
  }, E2E_TIMING.resourceMs, "visible flight-plan route overlay");
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
  return (await observeUntil(message, async () => {
    const probe = findMapFollowProbe(dumpAndroid(serial));
    return probe !== null && predicate(probe) ? probe : null;
  }, { timeoutMs, intervalMs: E2E_TIMING.pollIntervalMs })).value;
}

async function ensureBadAutopilotDebugFlag(serial, result) {
  const driver = nativeSemanticDriver(serial);
  await driver.openPage("settings");
  await scrollUntilTag(serial, DEBUG_DIAGNOSTICS_SECTION_TAG, 12);
  let xml = dumpAndroid(serial);
  if (!findNode(xml, (node) => hasAndroidTag(node, DEBUG_DIAGNOSTICS_SECTION_TAG))) {
    throwWithUi(serial, "Debug Diagnostics settings section is not visible");
  }
  await nativeTransition(result, "Debug Diagnostics section opened", {
    ready: async () => findNode(
      dumpAndroid(serial),
      (node) => hasAndroidTag(node, DEBUG_DIAGNOSTICS_SECTION_TAG),
    ),
    act: async () => tapTag(serial, DEBUG_DIAGNOSTICS_SECTION_TAG, E2E_TIMING.localReadyMs),
    complete: async () => {
      const section = findNode(
        dumpAndroid(serial),
        (node) => hasAndroidTag(node, DEBUG_DIAGNOSTICS_SECTION_TAG),
      );
      return section?.checked === "true" || section?.selected === "true" ? section : null;
    },
  });
  await scrollUntilTag(serial, BAD_AUTOPILOT_DEBUG_TAG, 6);
  xml = dumpAndroid(serial);
  let checkbox = findNode(xml, (node) => hasAndroidTag(node, BAD_AUTOPILOT_DEBUG_TAG));
  if (!checkbox) {
    throwWithUi(serial, "Bad Autopilot debug flag is not visible");
  }
  if (checkbox.checked !== "true") {
    await nativeTransition(result, "Bad Autopilot debug flag enabled", {
      ready: async () => findNode(
        dumpAndroid(serial),
        (node) => hasAndroidTag(node, BAD_AUTOPILOT_DEBUG_TAG) && node.checked !== "true",
      ),
      act: async () => tapTag(serial, BAD_AUTOPILOT_DEBUG_TAG, E2E_TIMING.localReadyMs),
      complete: async () => findNode(
        dumpAndroid(serial),
        (node) => hasAndroidTag(node, BAD_AUTOPILOT_DEBUG_TAG) && node.checked === "true",
      ),
    });
  }
  await driver.openPage("map");
  recordStep(result, "Bad Autopilot debug source enabled");
}

async function ensureBadAutopilotAvailable(serial, result) {
  await nativeTransition(result, "ownship source tray opened", {
    ready: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, "parity:ownship-launcher")),
    act: async () => tapTag(serial, "parity:ownship-launcher", E2E_TIMING.localReadyMs),
    complete: async () => findNode(
      dumpAndroid(serial),
      (node) => androidTag(node).startsWith("parity:ownship-source:"),
    ),
  });
  if (findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG))) {
    pressKey(serial, "KEYCODE_BACK");
    recordStep(result, "Bad Autopilot source available");
    return;
  }
  pressKey(serial, "KEYCODE_BACK");
  await ensureBadAutopilotDebugFlag(serial, result);
  await nativeTransition(result, "Bad Autopilot source available after enabling debug flag", {
    ready: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, "parity:ownship-launcher")),
    act: async () => tapTag(serial, "parity:ownship-launcher", E2E_TIMING.localReadyMs),
    complete: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG)),
  });
  pressKey(serial, "KEYCODE_BACK");
  recordStep(result, "Bad Autopilot source available");
}

async function selectBadAutopilotSource(serial, result) {
  await tapTag(serial, "parity:ownship-launcher", E2E_TIMING.localReadyMs);
  const sourceNode = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG),
    E2E_TIMING.localReadyMs,
    "Bad Autopilot ownship source",
  );
  if (sourceNode.enabled !== "true") {
    throwWithUi(serial, "Bad Autopilot source is visible but disabled");
  }
  await nativeTransition(result, "Bad Autopilot ownship selected", {
    ready: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, BAD_AUTOPILOT_SOURCE_TAG)),
    act: async () => tapTag(serial, BAD_AUTOPILOT_SOURCE_TAG, E2E_TIMING.localReadyMs),
    complete: async () => findMapFollowProbe(dumpAndroid(serial)),
    responseTimeoutMs: E2E_TIMING.observationMs,
  });
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
  }, E2E_TIMING.resourceMs, "plate image visibly painted on first open", E2E_TIMING.resourcePollIntervalMs).catch((error) => {
    throw new Error(`${error.message}; lastCrop=${JSON.stringify(lastCrop)}; lastStats=${JSON.stringify(lastStats)}`);
  });
  recordStep(
    result,
    "plate image visibly painted",
    `mean=${lastStats.lumaMean.toFixed(1)} stdev=${lastStats.lumaStdDev.toFixed(1)} bright=${lastStats.brightRatio.toFixed(3)}`,
  );
  recordCheck(result, "plate.firstOpenVisiblyPainted", true, JSON.stringify(lastStats));
}

function labelContainsWords(label, expected) {
  const haystack = label.toUpperCase();
  return expected.toUpperCase().split(/[^A-Z0-9]+/).filter(Boolean)
    .every((word) => haystack.includes(word));
}

async function openPlateFromAirportInspector(serial, result, airportId, expectedLabel) {
  await inspectAirportFromChartSearch(serial, result, airportId);

  const airportItemTag = `parity:map-selection-item:airport-${airportId}`;
  if (findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, airportItemTag))) {
    await nativeTransition(result, `airport ${airportId} selected in inspector`, {
      ready: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, airportItemTag)),
      act: async () => tapTag(serial, airportItemTag, E2E_TIMING.localReadyMs),
      complete: async () => findNode(
        dumpAndroid(serial),
        (node) => hasAndroidTag(node, "parity:map-selection-action:plates"),
      ),
    });
  }
  const platesAction = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-selection-action:plates"),
    E2E_TIMING.localReadyMs,
    "airport plates action",
  );
  if (platesAction.enabled !== "true") {
    throwWithUi(serial, `airport plates action is disabled for ${airportId}`);
  }
  await tapTag(serial, "parity:map-selection-action:plates", E2E_TIMING.localReadyMs);
  await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:plate-folder-button"),
    E2E_TIMING.observationMs,
    "plate folder page opened",
  );
  recordStep(result, "plate folder opened", airportId);

  const tile = await findNodeByScrolling(
    serial,
    (node) => androidTag(node).startsWith(PLATE_FOLDER_TILE_PREFIX) &&
      labelContainsWords(androidTag(node), expectedLabel),
    12,
  );
  if (!tile) throwWithUi(serial, `plate folder does not contain ${expectedLabel}`);
  const tileTag = androidTag(tile);
  await tapTag(serial, tileTag, E2E_TIMING.localReadyMs);
  await waitForNode(
    serial,
    (node) => hasAndroidTag(node, PLATE_SURFACE_TAG),
    E2E_TIMING.observationMs,
    "plate surface after tile selection",
  );
  recordStep(result, "fixture plate opened", tileTag.slice(PLATE_FOLDER_TILE_PREFIX.length));
}

async function ensureMapFollowEngaged(serial, result) {
  let probe = await waitForMapFollowProbe(
    serial,
    () => true,
    E2E_TIMING.observationMs,
    "map-follow probe visible",
  );
  if (!probe.following) {
    await tapTag(serial, "parity:center-here-button", E2E_TIMING.localReadyMs);
    probe = await waitForMapFollowProbe(
      serial,
      (nextProbe) => nextProbe.following,
      E2E_TIMING.userResponseMs,
      "CTR follow engaged",
    );
  }
  await waitForMapFollowProbe(
    serial,
    (nextProbe) => nextProbe.following && mapFollowOffsetPx(nextProbe) <= 120,
    E2E_TIMING.observationMs,
    "CTR follow centered on ownship",
  );
  recordStep(result, "CTR follow engaged");
}

async function disengageMapFollowForRouteVisibility(serial, result) {
  const initialProbe = findMapFollowProbe(dumpAndroid(serial));
  if (!initialProbe) {
    recordStep(result, "CTR follow unavailable; already disengaged");
    return;
  }
  const probe = initialProbe;
  if (!probe.following) return;
  await tapTag(serial, "parity:center-here-button", E2E_TIMING.localReadyMs);
  await waitForMapFollowProbe(
    serial,
    (nextProbe) => !nextProbe.following,
    E2E_TIMING.observationMs,
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
  await waitForPlanSignature(serial, expectedSignature, E2E_TIMING.observationMs);
}

async function dragMapWhileFollowing(serial, result) {
  const surfaceNode = await waitForNode(
    serial,
    (node) => hasAndroidTag(node, "parity:map-surface"),
    E2E_TIMING.localReadyMs,
    "map surface bounds",
  );
  const rect = rectOfBounds(surfaceNode.bounds);
  const startX = rect.left + rect.width * 0.50;
  const startY = rect.top + rect.height * 0.54;
  const endX = rect.left + rect.width * 0.72;
  const endY = startY;
  let probe = null;
  try {
    probe = await nativeTransition(result, "map drag keeps CTR engaged with an offset ownship", {
      ready: async () => findMapFollowProbe(dumpAndroid(serial)),
      act: async () => swipe(serial, startX, startY, endX, endY, 650),
      complete: async () => {
        const nextProbe = findMapFollowProbe(dumpAndroid(serial));
        return nextProbe?.following && mapFollowOffsetPx(nextProbe) >= 80 ? nextProbe : null;
      },
      responseTimeoutMs: E2E_TIMING.observationMs,
    });
  } catch (error) {
    probe = findMapFollowProbe(dumpAndroid(serial));
    throw new Error(`${error.message}; lastProbe=${describeMapFollowProbe(probe)}`);
  }
  const stable = await assertConditionRemains(
    "CTR offset remains stable after drag",
    async () => findMapFollowProbe(dumpAndroid(serial)),
    (nextProbe) => Boolean(nextProbe?.following && mapFollowOffsetPx(nextProbe) >= 80),
    {
      durationMs: E2E_TIMING.stabilityMs,
      intervalMs: E2E_TIMING.stabilityPollIntervalMs,
    },
  );
  const settled = findMapFollowProbe(dumpAndroid(serial));
  const settledOffset = mapFollowOffsetPx(settled);
  recordCheck(
    result,
    "chart.ctrDragKeepsOwnshipOffset",
    settledOffset >= 80,
    `offset=${settledOffset.toFixed(0)}px samples=${stable.samples} tag=${settled.tag}`,
  );
}

async function zoomMapOneStepWhileFollowing(serial, result, direction) {
  const before = await waitForMapFollowProbe(
    serial,
    (probe) => probe.following && mapFollowOffsetPx(probe) >= 80,
    E2E_TIMING.localReadyMs,
    `map-follow offset before ${direction} zoom`,
  );
  const key = direction === "in" ? "KEYCODE_PLUS" : "KEYCODE_MINUS";
  const changed = await nativeTransition(result, `map zoom ${direction} keeps CTR engaged`, {
    ready: async () => findMapFollowProbe(dumpAndroid(serial)),
    act: async () => pressKey(serial, key),
    complete: async () => {
      const probe = findMapFollowProbe(dumpAndroid(serial));
      const changedInDirection = direction === "in"
        ? probe?.zoomCenti > before.zoomCenti
        : probe?.zoomCenti < before.zoomCenti;
      return probe?.following && changedInDirection && mapFollowOffsetPx(probe) >= 80 ? probe : null;
    },
    responseTimeoutMs: E2E_TIMING.observationMs,
  });
  return changed;
}

async function zoomMapWhileFollowing(serial, result, { assertStable = false } = {}) {
  const before = findMapFollowProbe(dumpAndroid(serial));
  await zoomMapOneStepWhileFollowing(serial, result, "in");
  const settled = await zoomMapOneStepWhileFollowing(serial, result, "in");
  if (assertStable) {
    await assertConditionRemains(
      "CTR offset remains stable after zoom",
      async () => findMapFollowProbe(dumpAndroid(serial)),
      (probe) => Boolean(
        probe?.following &&
        mapFollowOffsetPx(probe) >= 80 &&
        probe.zoomCenti >= settled.zoomCenti
      ),
      {
        durationMs: E2E_TIMING.stabilityMs,
        intervalMs: E2E_TIMING.stabilityPollIntervalMs,
      },
    );
  }
  const settledOffset = mapFollowOffsetPx(settled);
  recordCheck(result, "chart.ctrZoomKeepsFollowing", settled.following, settled.tag);
  recordCheck(
    result,
    "chart.ctrZoomKeepsOwnshipOffset",
    settledOffset >= 80,
    `offset=${settledOffset.toFixed(0)}px tag=${settled.tag}`,
  );
  recordStep(result, "map zoom preserved CTR offset", `${before.zoomCenti} -> ${settled.zoomCenti}`);
}

function layerToggleIsOn(xml, layerId) {
  return layerToggleNode(xml, layerId)?.checked === "true";
}

async function openLayersTray(serial, result) {
  let xml = dumpAndroid(serial);
  if (!layerToggleNode(xml, "terrain_warning")) {
    await nativeTransition(result, "Layers tray opened", {
      ready: async () => findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, "parity:layers-button")),
      act: async () => tapTag(serial, "parity:layers-button", E2E_TIMING.localReadyMs),
      complete: async () => layerToggleNode(dumpAndroid(serial), "terrain_warning"),
    });
    xml = dumpAndroid(serial);
  } else {
    recordStep(result, "Layers tray opened");
  }
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

  await nativeTransition(result, "Terrain disabled", {
    ready: async () => layerToggleIsOn(dumpAndroid(serial), "terrain_warning"),
    act: async () => tapTag(serial, layerToggleTag("terrain_warning"), E2E_TIMING.localReadyMs),
    complete: async () => !layerToggleIsOn(dumpAndroid(serial), "terrain_warning"),
  });
  recordCheck(result, "layers.terrainCommandAccepted", rejectedLayerCommandCount(serial) === 0);
  recordStep(result, "Terrain disabled without a session-command warning");

  xml = await openLayersTray(serial, result);
  recordCheck(result, "layers.terrainTurnedOff", !layerToggleIsOn(xml, "terrain_warning"));
  await nativeTransition(result, "NEXRAD enabled", {
    ready: async () => !layerToggleIsOn(dumpAndroid(serial), "nexrad"),
    act: async () => tapTag(serial, layerToggleTag("nexrad"), E2E_TIMING.localReadyMs),
    complete: async () => layerToggleIsOn(dumpAndroid(serial), "nexrad"),
  });
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

async function waitForPlanSignature(serial, expected = null, timeoutMs = E2E_TIMING.observationMs) {
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
      const bounds = await waitForAndroidOrientation(serial, orientation, E2E_TIMING.observationMs);
      const signature = await waitForPlanSignature(serial, baselineSignature, E2E_TIMING.observationMs);
      const pid = currentAerobagPid(serial);
      recordCheck(result, `rotation.${transition + 1}.pidStable`, pid === initialPid, `${initialPid} -> ${pid}`);
      await ensureChartPage(serial, result);
      await waitForRouteOverlay(serial, result);
      await ensurePlanPage(serial, result);
      finalSignature = await waitForPlanSignature(serial, baselineSignature, E2E_TIMING.observationMs);
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

async function waitForLogcatMarker(serial, marker, timeoutMs = E2E_TIMING.resourceMs) {
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
  await nativeSemanticDriver(serial).openPage("data_status");
  await waitFor(
    () => findNodes(
      dumpAndroid(serial),
      (node) => androidTag(node).startsWith("parity:data-status-row:"),
    ).length > 0,
    E2E_TIMING.localReadyMs,
    "Data Status rows",
    E2E_TIMING.pollIntervalMs,
  );

  const anyNotamNode = await findNodeByScrolling(
    serial,
    (node) => androidTag(node).startsWith("parity:data-status-row:live_feed:notams:"),
    12,
  );
  if (!anyNotamNode) {
    throwWithUi(serial, "persisted NOTAM row is absent from Data Status");
  }

  let loadedNode = null;
  try {
    await waitFor(() => {
      const xml = dumpAndroid(serial);
      loadedNode = findNode(xml, (node) => {
        const tag = androidTag(node);
        return tag.startsWith("parity:data-status-row:live_feed:notams:") && !tag.endsWith(":MISSING");
      });
      return loadedNode !== null;
    }, E2E_TIMING.resourceMs, "persisted NOTAM loaded row", E2E_TIMING.resourcePollIntervalMs);
  } catch (_error) {
    throwWithUi(serial, "persisted NOTAM did not appear loaded in Data Status");
  }
  recordStep(result, "persisted NOTAM visible in core status UI", androidTag(loadedNode));
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
  let initialPipelineStartCount = 0;
  try {
    await launchFreshAndroidApp(serial, { clearUiPrefs: false, clearCoreSettings: false });
    await waitForLogcatMarker(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER);
    initialPipelineStartCount = logcatMarkerCount(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER);
    adb(serial, ["logcat", "-c"]);
    recordStep(result, "persisted live-feed promotion paused at deterministic gate");
    await ensurePlanPage(serial, result);
    const restoredSignature = await waitForPlanSignature(serial, null, E2E_TIMING.observationMs);
    recordCheck(
      result,
      "rotation.liveFeedPhaseRouteRestored",
      planContentsEqual(restoredSignature, baselineSignature),
      JSON.stringify(restoredSignature),
    );
    await activateDestinationLeg(serial, result, ROTATION_ROUTE);
    const liveFeedBaselineSignature = await waitForPlanSignature(serial, null, E2E_TIMING.observationMs);
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
    await waitForPlanSignature(serial, liveFeedBaselineSignature, E2E_TIMING.observationMs);
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
    initialPipelineStartCount === 1 && logcatMarkerCount(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER) === 0,
    `initial marker count=${initialPipelineStartCount}; subsequent marker count=${logcatMarkerCount(serial, LIVE_FEED_PROMOTION_PAUSE_MARKER)}`,
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
  const { serial } = args;
  const route = CTR_STRESS_ROUTE;
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
    await zoomMapWhileFollowing(serial, result, { assertStable: cycle === 1 || cycle === 8 });
    recordStep(result, "CTR zoom stress cycle", String(cycle));
    await zoomMapOneStepWhileFollowing(serial, result, "out");
    await zoomMapOneStepWhileFollowing(serial, result, "out");
  }
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function runPlateFirstRenderSmoke(args) {
  const { serial } = args;
  if (!args.releaseFixture) {
    throw new Error("android.plate-first-render-smoke requires --release-fixture");
  }
  const fixture = loadReleaseJourneyFixture(args.releaseFixture);
  const plateCapability = fixture.capabilities.plate.georeferenced;
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
  await openPlateFromAirportInspector(
    serial,
    result,
    plateCapability.airport_id,
    plateCapability.label_contains,
  );
  await waitForPlateImagePainted(serial, result);
  result.status = "pass";
  result.finished_at = new Date().toISOString();
  return result;
}

async function launchReleaseJourneyAndroidApp(args, fixture, options) {
  adbBestEffort(args.serial, ["shell", "am", "force-stop", "com.android.chrome"]);
  await launchFreshAndroidApp(args.serial, options);
  const initialViewport = fixture.capabilities.initial_viewport;
  if (args.serial?.startsWith("emulator-") && initialViewport) {
    adbBestEffort(args.serial, [
      "emu", "geo", "fix",
      String(initialViewport.longitude ?? initialViewport.lon),
      String(initialViewport.latitude ?? initialViewport.lat),
    ]);
  }
}

function useAndroidFixtureClock(serial, epochMs) {
  const previousAutoTime = adb(serial, ["shell", "settings", "get", "global", "auto_time"]).trim();
  adb(serial, ["shell", "settings", "put", "global", "auto_time", "0"]);
  adb(serial, ["shell", "cmd", "alarm", "set-time", String(epochMs)]);
  return () => {
    adbBestEffort(serial, [
      "shell", "settings", "put", "global", "auto_time",
      previousAutoTime === "0" ? "0" : "1",
    ]);
  };
}

async function runSharedReleaseJourney(args, journey) {
  if (!args.releaseFixture) {
    throw new Error(`${journey.id} requires --release-fixture`);
  }
  const fixture = loadReleaseJourneyFixture(args.releaseFixture);
  const restoreClock = useAndroidFixtureClock(
    args.serial,
    androidJourneyEpochMs(journey.id, fixture.capabilities.reference_epoch_ms),
  );
  try {
    const bootstrap = createTestResult(`${journey.id}.bootstrap`);
    await launchReleaseJourneyAndroidApp(
      args,
      fixture,
      { clearUiPrefs: true, clearCoreSettings: true },
    );
    await ensureOfflinePackagesReady(args.serial, bootstrap, args);
    await waitForRuntime(args.serial, bootstrap);

    const driver = new AndroidSemanticJourneyDriver(args.serial, {
      resetApp: () => launchReleaseJourneyAndroidApp(
        args,
        fixture,
        { clearUiPrefs: true, clearCoreSettings: true },
      ),
      reloadApp: () => launchReleaseJourneyAndroidApp(
        args,
        fixture,
        { clearUiPrefs: false, clearCoreSettings: false },
      ),
      resetApplicationData: async () => {
        adb(args.serial, ["shell", "pm", "clear", "org.aerobag.app"]);
        await launchReleaseJourneyAndroidApp(
          args,
          fixture,
          { clearUiPrefs: false, clearCoreSettings: false },
        );
      },
    });
    const implementation = releaseJourneyImplementation(journey.id);
    if (!implementation) throw new Error(`${journey.id} has no implemented release journey`);
    return await executeReleaseJourney({
      journey,
      platform: "android",
      driver,
      fixture,
      fixtureOrigin: `http://127.0.0.1:${args.packageSourcePort}`,
      artifactDir: join(E2E_ARTIFACT_DIR, journey.id),
    }, implementation);
  } finally {
    restoreClock();
  }
}

async function runOfflineColdStart(args) {
  if (!args.releaseFixture) throw new Error("android.offline-cold-start requires --release-fixture");
  const fixture = loadReleaseJourneyFixture(args.releaseFixture);
  const restoreClock = useAndroidFixtureClock(args.serial, fixture.capabilities.reference_epoch_ms);
  const result = createTestResult("android.offline-cold-start");
  try {
    await launchFreshAndroidApp(args.serial, { clearUiPrefs: true, clearCoreSettings: true });
    await ensureOfflinePackagesReady(args.serial, result, args);
    await waitForRuntime(args.serial, result);
    recordCheck(result, "offline.select", result.steps.some((step) =>
      step.name === "offline package sync requested" || step.name === "offline packages ready"));
    recordCheck(result, "offline.sync", runtimeUiVisible(dumpAndroid(args.serial)));

    const driver = new AndroidSemanticJourneyDriver(args.serial, {
      resetApp: () => launchFreshAndroidApp(args.serial, { clearUiPrefs: true, clearCoreSettings: false }),
      reloadApp: () => launchFreshAndroidApp(args.serial, { clearUiPrefs: false, clearCoreSettings: false }),
    });
    const georef = fixture.capabilities.plate.georeferenced;
    await driver.openPage("flight_plan");
    await appendRoute(args.serial, result, `KRNT ${georef.airport_id}`, null);
    await driver.openPage("home");
    const offlineDestination = await driver.readElement("home-button:OfflinePackages");
    recordCheck(result, "home.offline-packages", Boolean(offlineDestination?.enabled));
    await driver.openPage("offline_packages");
    recordCheck(result, "navigation.offline-packages", Boolean(
      await driver.readElement("page:offline_packages"),
    ));

    adbBestEffort(args.serial, ["reverse", "--remove", `tcp:${args.packageSourcePort}`]);
    adbBestEffort(args.serial, ["shell", "svc", "wifi", "disable"]);
    adbBestEffort(args.serial, ["shell", "svc", "data", "disable"]);
    try {
      await driver.reload();
      await waitFor(
        () => runtimeUiVisible(dumpAndroid(args.serial)),
        E2E_TIMING.startupMs,
        "offline cold-start runtime",
      );
      recordCheck(result, "offline.cold-start", true);

      await driver.openPage("map");
      let raster = null;
      await waitFor(() => {
        const entry = findNode(dumpAndroid(args.serial), (node) =>
          androidTag(node).startsWith("parity:raster-state:"));
        raster = entry ? androidTag(entry) : null;
        return raster && /planned:[1-9][0-9]*:loaded:[1-9][0-9]*/.test(raster);
      }, E2E_TIMING.resourceMs, "offline chart raster");
      recordCheck(result, "offline.chart", Boolean(raster), raster);

      await driver.openPage("charts");
      await driver.chooseOption("plate-airport-button", georef.airport_id);
      const label = georef.label_contains.replace(/\s*\(GPS\)\s*/i, " ").replace(/\bRWY\s+/i, " ").replace(/\s+/g, " ").trim();
      const choices = await driver.readProjection("parity:tray-option:");
      if (!choices.length) await driver.performAction("plate-chart-button");
      const option = (await driver.readProjection("parity:tray-option:"))
        .find((entry) => entry.text.toUpperCase().includes(label.toUpperCase()));
      if (!option) throw new Error(`offline plate ${label} is unavailable`);
      await driver.performAction(`tray-option:${option.id.replace(/^parity:tray-option:/, "")}`);
      const plate = await waitForNode(
        args.serial,
        (node) => androidTag(node).startsWith("parity:plate-viewport:chart:"),
        E2E_TIMING.resourceMs,
        "offline plate image",
      );
      recordCheck(result, "offline.plate", Boolean(plate), plate ? androidTag(plate) : undefined);
    } finally {
      adbBestEffort(args.serial, ["shell", "svc", "wifi", "enable"]);
      adbBestEffort(args.serial, ["shell", "svc", "data", "enable"]);
      adbBestEffort(args.serial, ["reverse", `tcp:${args.packageSourcePort}`, `tcp:${args.packageSourcePort}`]);
    }
    result.status = "pass";
    result.finished_at = new Date().toISOString();
    return result;
  } finally {
    restoreClock();
  }
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
  {
    id: "android.offline-cold-start",
    run: runOfflineColdStart,
  },
  ...RELEASE_JOURNEYS
    .filter((journey) => journey.platforms.includes("android") && releaseJourneyImplementation(journey.id))
    .map((journey) => ({
      id: journey.id,
      run: (args) => runSharedReleaseJourney(args, journey),
    })),
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

main().then(
  () => shutdownAndroidSemanticDrivers(),
  (error) => {
    shutdownAndroidSemanticDrivers();
    console.error(`E2E FAILED: ${error.message}`);
    process.exit(1);
  },
);
