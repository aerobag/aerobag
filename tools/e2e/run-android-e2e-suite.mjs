#!/usr/bin/env node
import {
  ANDROID_PACKAGE,
  acceptDisclaimerIfPresent,
  adb,
  adbBestEffort,
  androidNodeLabel,
  androidTag,
  assertRuntimeIsAvailable,
  clearFocusedText,
  delay,
  dumpAndroid,
  findNode,
  findNodes,
  hasAndroidTag,
  hasAndroidText,
  inputText,
  launchFreshAndroidApp,
  pressKey,
  scrollUntilTag,
  tapFirstPresentTag,
  tapTag,
  waitFor,
} from "./android-harness.mjs";

const DEFAULT_ROUTE = "KRNT KPWT";
const DEFAULT_PACKAGE_SOURCE_PORT = process.env.PACKAGE_SOURCE_PORT ?? "8083";
const OFFLINE_REGION_IDS = ["ak", "ec", "nc", "ne", "nw", "pac", "sc", "se", "sw"];
const PLAN_PAGE_TAGS = ["parity:plan-append-route-input"];
const CHART_PAGE_TAGS = ["parity:map-surface"];
const CHART_SEARCH_INPUT_TAG = "parity:chart-search-input";
const ROUTE_OVERLAY_PREFIX = "parity:flight-plan-route-overlay:";

function usage() {
  console.log(`Usage:
  node tools/e2e/run-android-e2e-suite.mjs [--serial emulator-5554] [--route "KRNT KPWT"] [--package-source-port 8083] [--no-sync-offline-packages] [--json]

Runs Android end-to-end UI tests against an installed Aerobag app.
When a clean emulator starts on Offline Packages, the runner syncs the NW
package set through the app UI before running the route smoke.`);
}

function parseArgs(argv) {
  const args = {
    serial: process.env.ANDROID_SERIAL ?? "",
    route: DEFAULT_ROUTE,
    packageSourcePort: DEFAULT_PACKAGE_SOURCE_PORT,
    syncOfflinePackages: true,
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

async function ensureOfflinePackagesReady(serial, result, { packageSourcePort, syncOfflinePackages }) {
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
    for (const regionId of OFFLINE_REGION_IDS) {
      if (regionId === "nw") continue;
      const tag = `parity:offline-region:${regionId}:toggle`;
      await scrollUntilTag(serial, tag, 6);
      if (await tapTagIfPresent(serial, tag, 1200)) {
        recordStep(result, "offline region deselected", regionId);
        await delay(150);
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
    if (PLAN_PAGE_TAGS.some((tag) => findNode(xml, (node) => hasAndroidTag(node, tag)))) {
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

async function ensureChartPage(serial, result) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (CHART_PAGE_TAGS.some((tag) => findNode(xml, (node) => hasAndroidTag(node, tag)))) {
      recordStep(result, "chart page visible");
      return;
    }
    assertRuntimeIsAvailable(serial);
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:nav-cdi"))) {
      await tapTag(serial, "parity:nav-cdi", 10000);
      await delay(600);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:CHART"))) {
      await tapTag(serial, "parity:button:CHART", 10000);
      await delay(600);
      continue;
    }
    if (findNode(xml, (node) => hasAndroidTag(node, "parity:button:HOME"))) {
      await tapTag(serial, "parity:button:HOME", 10000);
      await delay(400);
      continue;
    }
    await tapFirstPresentTag(serial, ["parity:button:FLIGHT\nPLAN", "parity:button:PLAN"], 2000).catch(() => {
      pressKey(serial, "KEYCODE_BACK");
    });
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
      return lastObserved === destination;
    }, 3500, `chart search contains ${destination}`).then(
      () => true,
      () => false,
    );
    if (matched) {
      pressKey(serial, "KEYCODE_ENTER");
      await delay(800);
      pressKey(serial, "KEYCODE_BACK");
      await waitFor(() => {
        const xml = dumpAndroid(serial);
        return findNode(xml, (node) => hasAndroidTag(node, "parity:map-surface")) !== null;
      }, 10000, "map semantics visible after destination search");
      recordStep(result, "chart centered on destination", destination);
      return;
    }
  }
  throwWithUi(serial, `chart search did not contain ${destination}; last observed=${JSON.stringify(lastObserved)}`);
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

async function runFlightPlanRouteSmoke({ serial, route, packageSourcePort, syncOfflinePackages }) {
  const result = createTestResult("android.flight-plan-route-smoke");
  adb(serial, ["logcat", "-c"]);
  await launchFreshAndroidApp(serial, { clearUiPrefs: true, clearCoreSettings: false });
  recordStep(result, "app launched", serial || "default adb device");
  if (await acceptDisclaimerIfPresent(serial)) {
    recordStep(result, "disclaimer accepted");
  }
  await ensureOfflinePackagesReady(serial, result, { packageSourcePort, syncOfflinePackages });
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

const tests = [
  {
    id: "android.flight-plan-route-smoke",
    run: runFlightPlanRouteSmoke,
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
  const suite = {
    suite: "android-e2e",
    package: ANDROID_PACKAGE,
    serial: args.serial || null,
    started_at: new Date().toISOString(),
    results: [],
  };
  for (const test of tests) {
    console.log(`# ${test.id}`);
    try {
      suite.results.push(await test.run(args));
    } catch (error) {
      const failed = createTestResult(test.id);
      failed.status = "fail";
      failed.finished_at = new Date().toISOString();
      failed.error = error.message;
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
