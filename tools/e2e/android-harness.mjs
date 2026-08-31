// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  E2E_TIMING, observeChangedValueUntilStable, observeUntil, performTransition,
  TransientObservationError,
} from "./transition-contract.mjs";

export const ANDROID_PACKAGE = "org.aerobag.app";
export const ANDROID_ACTIVITY = `${ANDROID_PACKAGE}/.MainActivity`;
export const DEBUG_ARM_LAYER_NAV_KV_FAULT_EXTRA =
  `${ANDROID_PACKAGE}.extra.DEBUG_ARM_LAYER_NAV_KV_FAULT`;
export const DEBUG_CLEAR_CORE_SETTINGS_EXTRA =
  `${ANDROID_PACKAGE}.extra.DEBUG_CLEAR_CORE_SETTINGS`;
export const DEBUG_CLEAR_UI_PREFS_EXTRA =
  `${ANDROID_PACKAGE}.extra.DEBUG_CLEAR_UI_PREFS`;

export function androidJourneyEpochMs(journeyId, fixtureEpochMs, hostEpochMs = Date.now()) {
  return journeyId === "shared.cloud-crossfill" ? hostEpochMs : fixtureEpochMs;
}

const ADB_TIMEOUT_MS = 20000;
const APP_START_TIMEOUT_MS = 60000;
const CLOCK_SET_TIMEOUT_MS = 15000;
// A transient accessibility stall must leave time for the transition contract
// to observe recovery. Indexed controls normally bypass this fallback entirely.
const SEMANTIC_OBSERVATION_REQUEST_TIMEOUT_SECONDS = 0.9;
const SEMANTIC_ACTION_REQUEST_TIMEOUT_SECONDS = 2.25;
const SEMANTIC_DRIVER_DEVICE_PORT = 19191;
const SEMANTIC_DRIVER_PROTOCOL = "aerobag-semantic-driver/15";
const SEMANTIC_DRIVER_PACKAGE = "org.aerobag.app.test";
const STARTUP_PROJECTION_ID = "org.aerobag.app:id/e2e_startup_state_projection";
const SEMANTIC_DRIVER_SERVICE =
  `${SEMANTIC_DRIVER_PACKAGE}/org.aerobag.app.e2e.SemanticDriverService`;
const SEMANTIC_DRIVER_IME =
  `${SEMANTIC_DRIVER_PACKAGE}/org.aerobag.app.e2e.SemanticDriverInputMethodService`;
const semanticDrivers = new Map();

export function adbArgs(serial, args) {
  return serial ? ["-s", serial, ...args] : args;
}

export function adb(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), {
    encoding: "utf8",
    timeout: ADB_TIMEOUT_MS,
    ...options,
  });
  if (res.status !== 0) {
    const detail = res.error?.message ?? (res.stderr || res.stdout);
    throw new Error(`adb ${args.join(" ")} failed: ${detail}`);
  }
  return res.stdout;
}

export function adbBestEffort(serial, args, options = {}) {
  return spawnSync("adb", adbArgs(serial, args), {
    encoding: "utf8",
    timeout: ADB_TIMEOUT_MS,
    ...options,
  });
}

export function adbBuffer(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), {
    encoding: null,
    timeout: ADB_TIMEOUT_MS,
    ...options,
  });
  if (res.status !== 0) {
    const stderr = Buffer.isBuffer(res.stderr) ? res.stderr.toString("utf8") : (res.stderr || "");
    const stdout = Buffer.isBuffer(res.stdout) ? res.stdout.toString("utf8") : (res.stdout || "");
    const detail = res.error?.message ?? (stderr || stdout);
    throw new Error(`adb ${args.join(" ")} failed: ${detail}`);
  }
  return res.stdout;
}

export function screencapPng(serial) {
  return adbBuffer(serial, ["exec-out", "screencap", "-p"], {
    timeout: 20000,
    maxBuffer: 16 * 1024 * 1024,
  });
}

export function classifyAndroidRendererFailure({ drawingEnabled, maxChannel }) {
  return String(drawingEnabled ?? "").trim() === "0" &&
    Number.isFinite(maxChannel) && maxChannel === 0;
}

export function analyzeAndroidScreenshotBlackness(pngBytes) {
  const script = `
from PIL import Image
from io import BytesIO
import json
import sys

image = Image.open(BytesIO(sys.stdin.buffer.read())).convert("RGB")
extrema = image.getextrema()
print(json.dumps({
    "width": image.width,
    "height": image.height,
    "max_channel": max(channel[1] for channel in extrema),
}))
`;
  const result = spawnSync("python3", ["-c", script], {
    input: pngBytes,
    encoding: "utf8",
    timeout: ADB_TIMEOUT_MS,
    maxBuffer: 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`Android screenshot analysis failed: ${result.stderr || result.stdout}`);
  }
  return JSON.parse(result.stdout);
}

export function captureAndroidRendererFailureEvidence(serial, artifactDir) {
  mkdirSync(artifactDir, { recursive: true });
  const screenshot = screencapPng(serial);
  const drawingEnabled = adb(serial, ["shell", "getprop", "debug.hwui.drawing_enabled"]).trim();
  const screenshotStats = analyzeAndroidScreenshotBlackness(screenshot);
  const evidence = {
    drawing_enabled: drawingEnabled,
    screenshot: screenshotStats,
    entirely_black: screenshotStats.max_channel === 0,
    recoverable: classifyAndroidRendererFailure({
      drawingEnabled,
      maxChannel: screenshotStats.max_channel,
    }),
  };
  writeFileSync(join(artifactDir, "screenshot.png"), screenshot);
  writeFileSync(join(artifactDir, "renderer-state.json"), `${JSON.stringify(evidence, null, 2)}\n`);
  return evidence;
}

export function recoverAndroidRenderer(serial) {
  adb(serial, ["shell", "setprop", "debug.hwui.drawing_enabled", "1"]);
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
}

function requiredSemanticDriver(serial) {
  const state = semanticDrivers.get(serial || "default");
  if (!state) throw new Error("persistent Android semantic driver has not been started");
  return state;
}

function semanticDriverRequest(port, path, timeoutSeconds = 5, method = "GET") {
  const methodArgs = method === "GET" ? [] : ["--request", method];
  return spawnSync("curl", [
    "--fail-with-body",
    "--silent",
    "--show-error",
    "--max-time",
    String(timeoutSeconds),
    ...methodArgs,
    `http://127.0.0.1:${port}${path}`,
  ], {
    encoding: "utf8",
    timeout: (timeoutSeconds + 1) * 1000,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function semanticDriverRequestTimedOut(response) {
  return response.status === 28 || response.stderr.includes("Operation timed out");
}

function semanticDriverRequestBusy(response) {
  return response.stdout.includes("semantic request busy") ||
    response.stderr.includes("503");
}

function semanticDriverObservationUnavailable(response) {
  return semanticDriverRequestTimedOut(response) || semanticDriverRequestBusy(response);
}

function semanticDriverIdleRequest(port, timeoutMs, request = semanticDriverRequest) {
  const query = new URLSearchParams({ timeout_ms: String(timeoutMs) });
  return request(
    port,
    `/await-idle?${query}`,
    Math.max(0.1, (timeoutMs + 100) / 1_000),
  );
}

export function semanticDriverObservationRequest(
  port,
  path,
  request = semanticDriverRequest,
) {
  return request(
    port, path, SEMANTIC_OBSERVATION_REQUEST_TIMEOUT_SECONDS,
  );
}

export function semanticDriverActionRequest(
  port,
  path,
  method = "POST",
  request = semanticDriverRequest,
) {
  const deadline = performance.now() + SEMANTIC_ACTION_REQUEST_TIMEOUT_SECONDS * 1_000;
  let response = request(
    port, path, SEMANTIC_ACTION_REQUEST_TIMEOUT_SECONDS, method,
  );
  while (semanticDriverRequestBusy(response)) {
    const remainingMs = deadline - performance.now();
    if (remainingMs <= 250) return response;
    const idleTimeoutMs = Math.max(1, Math.min(2_000, Math.floor(remainingMs - 250)));
    const idle = semanticDriverIdleRequest(port, idleTimeoutMs, request);
    if (idle.status !== 0 || idle.stdout.trim() !== "idle") return response;
    const retryMs = deadline - performance.now();
    if (retryMs <= 50) return response;
    response = request(port, path, retryMs / 1_000, method);
  }
  return response;
}

export function setAndroidSemanticText(serial, tag, value) {
  const state = requiredSemanticDriver(serial);
  const query = new URLSearchParams({ tag, value });
  const response = semanticDriverActionRequest(state.port, `/set-text?${query}`);
  if (response.status === 0 && response.stdout.trim() === "ok") return true;
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  throw new Error(`persistent Android IME text action failed for ${tag}: ${detail}`);
}

export function androidSemanticTextReady(serial) {
  const state = requiredSemanticDriver(serial);
  const response = semanticDriverRequest(state.port, "/ime-ready", 1);
  if (response.status !== 0) {
    const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
    throw new Error(`persistent Android IME readiness probe failed: ${detail}`);
  }
  return response.stdout.trim() === "ready";
}

export function setAndroidSemanticProgress(serial, tag, value, expectedBounds, semanticPath) {
  const state = requiredSemanticDriver(serial);
  if (!expectedBounds || !semanticPath) {
    throw new Error(
      `persistent Android semantic progress action for ${tag} has no readiness path and bounds`,
    );
  }
  const query = new URLSearchParams({
    tag,
    value: String(value),
    bounds: expectedBounds,
    path: semanticPath,
  });
  const response = semanticDriverActionRequest(state.port, `/set-progress?${query}`);
  if (response.status === 0 && response.stdout.trim() === "ok") return true;
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  throw new Error(`persistent Android semantic progress action failed for ${tag}: ${detail}`);
}

export function clickAndroidSemanticNode(
  serial,
  tag,
  expectedBounds,
  semanticPath,
  expectedState = {},
) {
  const state = requiredSemanticDriver(serial);
  if (!expectedBounds || !semanticPath) {
    throw new Error(`persistent Android semantic click for ${tag} has no readiness path and bounds`);
  }
  let currentBounds = expectedBounds;
  let currentPath = semanticPath;
  let target = null;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    target = androidPhysicalTapTarget(state.port, tag, currentBounds, currentPath);
    if (target) break;
    const refreshed = queryAndroidExactProjection(
      serial, tag, { providerOnly: true },
    )[0] ?? null;
    if (!androidSemanticReadinessStateMatches(tag, refreshed, expectedState)) {
      throw new Error(`persistent Android semantic click target changed for ${tag}`);
    }
    currentBounds = refreshed.bounds;
    currentPath = refreshed["semantic-path"];
  }
  if (!target) {
    throw new Error(`persistent Android physical tap target stayed stale for ${tag}`);
  }
  adb(serial, [
    "shell", "input", "tap",
    String(Math.round(target.bounds.left + target.bounds.width / 2)),
    String(Math.round(target.bounds.top + target.bounds.height / 2)),
  ]);
  if (awaitAndroidPhysicalTouch(
    state.port, target.bounds, target.touchTag, target.touchSequence,
  )) return true;
  throw new Error(`persistent Android physical tap was not received by ${tag}`);
}

export function androidSemanticReadinessStateMatches(tag, node, expectedState = {}) {
  const matches = (actual, expected) => expected == null || String(actual) === String(expected);
  return node?.["resource-id"] === tag &&
    Boolean(node.bounds) && Boolean(node["semantic-path"]) &&
    node.visible === "true" && node["center-reachable"] === "true" &&
    matches(node.enabled, expectedState.enabled) &&
    matches(node.selected, expectedState.selected) &&
    matches(node.checked, expectedState.checked) &&
    matches(node["state-description"], expectedState.stateDescription);
}

function androidPhysicalTapTarget(port, tag, expectedBounds, semanticPath) {
  const query = new URLSearchParams({
    tag,
    bounds: expectedBounds,
    path: semanticPath,
  });
  const response = semanticDriverActionRequest(port, `/tap-target?${query}`);
  if (response.status !== 0) {
    if (response.stdout.trim() === "physical tap target rejected") return null;
    const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
    throw new Error(`persistent Android physical tap failed for ${tag}: ${detail}`);
  }
  const target = JSON.parse(response.stdout);
  return {
    bounds: rectOfBounds(target.bounds),
    touchTag: target.touch_tag ?? "",
    touchSequence: Number(target.touch_sequence),
  };
}

function awaitAndroidPhysicalTouch(port, bounds, tag, after) {
  const query = new URLSearchParams({
    bounds: `[${bounds.left},${bounds.top}][${bounds.right},${bounds.bottom}]`,
    tag,
    after: String(after),
    timeout_ms: "500",
  });
  const response = semanticDriverRequest(port, `/await-touch?${query}`, 0.75);
  return response.status === 0 && response.stdout.trim() === "received";
}

export function focusAndroidSemanticNode(
  serial,
  tag,
  expectedBounds,
  semanticPath,
  expectedState = {},
) {
  return clickAndroidSemanticNode(
    serial, tag, expectedBounds, semanticPath, expectedState,
  );
}

export function scrollAndroidSemanticNode(serial, bounds, direction) {
  const state = requiredSemanticDriver(serial);
  const semanticDirection = direction === "down" || direction === "forward"
    ? "forward"
    : "backward";
  const query = new URLSearchParams({ bounds, direction: semanticDirection });
  const response = semanticDriverActionRequest(state.port, `/scroll?${query}`);
  if (response.status === 0 && response.stdout.trim() === "ok") return true;
  if (response.stderr.includes("409") || response.stdout.includes("scroll action rejected")) {
    return false;
  }
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  throw new Error(`persistent Android semantic scroll failed for ${bounds}: ${detail}`);
}

export function scrollAndroidSemanticSurface(serial, orientation, direction) {
  const state = requiredSemanticDriver(serial);
  const semanticDirection = direction === "down" || direction === "forward"
    ? "forward"
    : "backward";
  const query = new URLSearchParams({ orientation, direction: semanticDirection });
  const response = semanticDriverActionRequest(state.port, `/scroll?${query}`);
  if (response.status === 0 && response.stdout.trim() === "ok") return true;
  if (response.stderr.includes("409") || response.stdout.includes("scroll action rejected")) {
    return false;
  }
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  throw new Error(`persistent Android ${orientation} semantic scroll failed: ${detail}`);
}

export function waitForAndroidSemanticEvent(serial, timeoutMs) {
  const state = requiredSemanticDriver(serial);
  const boundedTimeoutMs = Math.max(1, Math.min(1_000, Math.ceil(timeoutMs)));
  const query = new URLSearchParams({ timeout_ms: String(boundedTimeoutMs) });
  const response = semanticDriverRequest(
    state.port,
    `/await-event?${query}`,
    Math.max(2, Math.ceil(boundedTimeoutMs / 1_000) + 1),
  );
  if (response.status === 0) return response.stdout.trim() === "changed";
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  throw new Error(`persistent Android semantic event wait failed: ${detail}`);
}

export function queryAndroidSemanticNodes(
  serial,
  tag,
  { prefix = false, first = false, includeDescendantText = true } = {},
) {
  const state = requiredSemanticDriver(serial);
  const query = new URLSearchParams({
    tag,
    prefix: String(prefix),
    first: String(first),
    descendant_text: String(includeDescendantText),
  });
  const response = semanticDriverObservationRequest(state.port, `/query?${query}`);
  if (response.status === 0) return JSON.parse(response.stdout);
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  if (semanticDriverObservationUnavailable(response)) {
    throw new TransientObservationError(
      `Android semantic tree was busy while querying ${tag}: ${detail}`,
    );
  }
  throw new Error(`persistent Android semantic query failed for ${tag}: ${detail}`);
}

export function queryAndroidExactProjection(
  serial,
  tag,
  {
    includeDescendantText = false,
    indexedOnly = false,
    boundedOnly = false,
    providerOnly = false,
    renderedOnly = false,
  } = {},
) {
  const state = requiredSemanticDriver(serial);
  const query = new URLSearchParams({
    tag,
    descendant_text: String(includeDescendantText),
    indexed_only: String(indexedOnly),
    bounded_only: String(boundedOnly),
    provider_only: String(providerOnly),
    rendered_only: String(renderedOnly),
  });
  const response = semanticDriverObservationRequest(state.port, `/exact-projection?${query}`);
  if (response.status === 0) return JSON.parse(response.stdout);
  const detail = response.error?.message || response.stdout.trim() || response.stderr.trim();
  if (semanticDriverObservationUnavailable(response)) {
    throw new TransientObservationError(
      `Android exact semantic projection was busy while querying ${tag}: ${detail}`,
    );
  }
  throw new Error(`persistent Android exact semantic projection failed for ${tag}: ${detail}`);
}

function semanticDriverDump(serial) {
  const state = requiredSemanticDriver(serial);
  // A blocked accessibility traversal is a failed transition, not a reason to
  // hide an emulator stall behind a journey-scale timeout.
  const response = semanticDriverObservationRequest(state.port, "/dump");
  if (response.status === 0 && response.stdout.includes("<hierarchy")) {
    return response.stdout;
  }
  const detail = response.error?.message || response.stderr.trim() || "request failed";
  if (semanticDriverObservationUnavailable(response)) {
    throw new TransientObservationError(
      `Android semantic tree was busy while rendering the hierarchy: ${detail}`,
    );
  }
  throw new Error(`persistent Android semantic driver failed: ${detail}`);
}

export async function ensureAndroidSemanticDriver(serial) {
  const key = serial || "default";
  const current = semanticDrivers.get(key);
  if (current) {
    const health = semanticDriverRequest(current.port, "/health", 1);
    if (health.status === 0 && health.stdout.trim() === SEMANTIC_DRIVER_PROTOCOL) {
      return current.port;
    }
  }

  const testPackage = adbBestEffort(serial, ["shell", "pm", "path", SEMANTIC_DRIVER_PACKAGE]);
  if (testPackage.status !== 0 || !testPackage.stdout.includes("package:")) {
    throw new Error(
      `persistent Android semantic driver package is not installed: ${SEMANTIC_DRIVER_PACKAGE}`,
    );
  }
  await observeUntil("Android semantic driver IME registration", () => {
    // `ime list -s` only reports enabled methods. Include disabled methods so
    // registration can be observed before the following `ime enable` call.
    const listed = adbBestEffort(serial, ["shell", "ime", "list", "-a", "-s"]);
    return listed.status === 0 && listed.stdout.split(/\r?\n/).includes(SEMANTIC_DRIVER_IME)
      ? true
      : null;
  }, {
    timeoutMs: E2E_TIMING.localResourceMs,
    intervalMs: E2E_TIMING.resourcePollIntervalMs,
  });
  adb(serial, ["shell", "ime", "enable", SEMANTIC_DRIVER_IME]);
  adb(serial, ["shell", "ime", "set", SEMANTIC_DRIVER_IME]);
  const selectedIme = adb(serial, ["shell", "settings", "get", "secure", "default_input_method"])
    .trim();
  if (selectedIme !== SEMANTIC_DRIVER_IME) {
    throw new Error(`Android semantic driver IME was not selected: ${selectedIme || "<empty>"}`);
  }

  if (current) {
    adbBestEffort(serial, ["forward", "--remove", `tcp:${current.port}`]);
    semanticDrivers.delete(key);
  }
  const forwarded = adb(serial, [
    "forward", "tcp:0", `tcp:${SEMANTIC_DRIVER_DEVICE_PORT}`,
  ]).trim();
  const port = Number(forwarded);
  if (!Number.isInteger(port) || port <= 0) {
    throw new Error(`adb did not allocate a semantic-driver port: ${JSON.stringify(forwarded)}`);
  }

  adb(serial, [
    "shell", "settings", "put", "secure", "enabled_accessibility_services",
    SEMANTIC_DRIVER_SERVICE,
  ]);
  adb(serial, ["shell", "settings", "put", "secure", "accessibility_enabled", "1"]);
  const state = { port };
  semanticDrivers.set(key, state);

  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const health = semanticDriverRequest(port, "/health", 1);
    if (health.status === 0) {
      const actualProtocol = health.stdout.trim();
      if (actualProtocol !== SEMANTIC_DRIVER_PROTOCOL) {
        throw new Error(
          `persistent Android semantic driver protocol mismatch: expected ` +
            `${SEMANTIC_DRIVER_PROTOCOL}, got ${actualProtocol || "<empty>"}`,
        );
      }
      return port;
    }
    await delay(100);
  }
  adbBestEffort(serial, ["forward", "--remove", `tcp:${port}`]);
  semanticDrivers.delete(key);
  throw new Error(
    "persistent Android semantic accessibility service did not start",
  );
}

export function shutdownAndroidSemanticDriver(serial) {
  const key = serial || "default";
  const state = semanticDrivers.get(key);
  if (!state) return;
  adbBestEffort(serial, ["forward", "--remove", `tcp:${state.port}`]);
  semanticDrivers.delete(key);
}

export function shutdownAndroidSemanticDrivers() {
  for (const key of [...semanticDrivers.keys()]) {
    shutdownAndroidSemanticDriver(key === "default" ? "" : key);
  }
}

export function dumpAndroid(serial) {
  const persistentDump = semanticDriverDump(serial);
  if (persistentDump !== null) return persistentDump;
  throw new Error("persistent Android semantic driver has not been started");
}

export function captureAndroidFailureDiagnostics(serial, artifactDir, label) {
  mkdirSync(artifactDir, { recursive: true });
  const captures = [
    ["screenshot.png", () => screencapPng(serial)],
    ["ui.xml", () => dumpAndroid(serial)],
    ["logcat.txt", () => adb(serial, ["logcat", "-d", "-v", "threadtime"], {
      maxBuffer: 16 * 1024 * 1024,
    })],
    ["activity.txt", () => adb(serial, ["shell", "dumpsys", "activity", "activities"])],
    ["window.txt", () => adb(serial, ["shell", "dumpsys", "window", "windows"])],
  ];
  const artifacts = [];
  const failures = [];
  for (const [name, capture] of captures) {
    const path = join(artifactDir, name);
    try {
      writeFileSync(path, capture());
      artifacts.push(path);
    } catch (error) {
      failures.push(`${name}: ${error.message}`);
    }
  }
  if (failures.length > 0) {
    const path = join(artifactDir, "diagnostic-errors.txt");
    writeFileSync(path, `${label}\n${failures.join("\n")}\n`);
    artifacts.push(path);
  }
  return artifacts;
}

export function decodeXml(text) {
  return text
    .replaceAll("&#10;", "\n")
    .replaceAll("&#xA;", "\n")
    .replaceAll("&quot;", "\"")
    .replaceAll("&apos;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

export function findNodes(xml, predicate) {
  const nodes = [];
  const nodeRegex = /<node\b[^>]*>/g;
  let match;
  while ((match = nodeRegex.exec(xml))) {
    const attrs = Object.fromEntries(
      [...match[0].matchAll(/([a-zA-Z0-9_-]+)=(?:"([^"]*)"|'([^']*)')/g)]
        .map((entry) => [entry[1], decodeXml(entry[2] ?? entry[3])]),
    );
    if (predicate(attrs)) nodes.push(attrs);
  }
  return nodes;
}

export function findNode(xml, predicate) {
  return findNodes(xml, predicate)[0] ?? null;
}

export function findAerobagAnrDialog(xml) {
  return findNode(xml, (node) =>
    node.package === "android" &&
    node["resource-id"] === "android:id/alertTitle" &&
    /^(?:Aerobag|org\.aerobag\.app) (?:isn't|isn’t) responding$/.test(node.text ?? "")
  );
}

export function assertNoAerobagAnr(xml) {
  const anr = findAerobagAnrDialog(xml);
  if (anr) throw new Error(`Aerobag ANR detected: ${anr.text}`);
}

export function androidImeVisible(xml) {
  return findNode(xml, (node) => {
    const identity = `${node.package ?? ""} ${node.class ?? ""}`;
    return node.package !== ANDROID_PACKAGE && /(?:inputmethod|keyboard|latinime)/i.test(identity);
  }) !== null;
}

export function androidImeShownFromDumpsys(output) {
  return /^\s*mInputShown=true\s*$/m.test(output);
}

export function androidImeShown(serial) {
  return androidImeShownFromDumpsys(adb(serial, ["shell", "dumpsys", "input_method"]));
}

export function androidTag(node) {
  const contentDescription = node["content-desc"] ?? "";
  if (contentDescription.startsWith("parity:")) return contentDescription;
  const resourceId = node["resource-id"] ?? "";
  const marker = "parity:";
  const offset = resourceId.indexOf(marker);
  return offset >= 0 ? resourceId.slice(offset) : "";
}

export function hasAndroidTag(node, tag) {
  const contentDescription = node["content-desc"] ?? "";
  const resourceId = node["resource-id"] ?? "";
  return (
    contentDescription === tag ||
    resourceId === tag ||
    resourceId.endsWith(`:id/${tag}`) ||
    resourceId.endsWith(`/id/${tag}`) ||
    androidTag(node) === tag
  );
}

export function androidRuntimeUiVisible(xml) {
  return findNode(xml, (node) =>
    hasAndroidTag(node, "parity:primary-navigation") ||
    hasAndroidTag(node, "parity:map-surface") ||
    hasAndroidTag(node, "parity:plan-append-route-input") ||
    hasAndroidTag(node, "parity:button:FLIGHT\nPLAN") ||
    hasAndroidTag(node, "parity:button:CHART")
  ) !== null;
}

function startupProjectionFromValue(value) {
  const fields = {};
  const components = value.split(":");
  for (let index = 0; index + 1 < components.length; index += 2) {
    fields[components[index]] = components[index + 1];
  }
  return fields;
}

function startupProjectionFromNode(node) {
  if (!node) return null;
  return startupProjectionFromValue(
    androidTag(node).slice("parity:startup-state:".length),
  );
}

function startupProjectionFromExactNode(node) {
  if (!node) return null;
  return startupProjectionFromValue(node["state-description"] ?? "");
}

function startupStateFromNode(node) {
  const fields = startupProjectionFromNode(node);
  return fields?.ready === "true" ? fields : null;
}

export function androidStartupProjection(xml) {
  return startupProjectionFromNode(findNode(xml, (candidate) =>
    androidTag(candidate).startsWith("parity:startup-state:")));
}

export function androidStartupState(xml) {
  return startupStateFromNode(findNode(xml, (candidate) =>
    androidTag(candidate).startsWith("parity:startup-state:")));
}

export function queryAndroidStartupState(serial) {
  const fields = queryAndroidStartupProjection(serial);
  return fields?.ready === "true" ? fields : null;
}

export function queryAndroidStartupProjection(serial) {
  return startupProjectionFromExactNode(
    queryAndroidExactProjection(serial, STARTUP_PROJECTION_ID)?.[0] ?? null,
  );
}

export function queryAndroidRuntimeReadyForJourney(serial) {
  const state = queryAndroidStartupState(serial);
  return state?.disclaimer_required === "false" && state?.page !== "OfflinePackages"
    ? state
    : null;
}

export function androidInteractiveRuntime(state, home) {
  return state?.ready === "true" &&
    state?.disclaimer_required === "false" &&
    androidSemanticNodeIsActionable(home)
    ? { state, home }
    : null;
}

export async function waitForAndroidInteractiveRuntime(
  serial,
  timeoutMs = E2E_TIMING.startupMs,
) {
  const observed = await observeUntil("painted interactive Android runtime", () => {
    const state = queryAndroidStartupState(serial);
    if (state?.disclaimer_required !== "false") return null;
    const home = queryAndroidSemanticNodes(
      serial,
      "parity:button:HOME",
      { first: true },
    )[0] ?? null;
    return androidInteractiveRuntime(state, home);
  }, {
    timeoutMs,
    intervalMs: E2E_TIMING.pollIntervalMs,
    consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
    waitForNextProbe: (intervalMs) => waitForAndroidSemanticEvent(serial, intervalMs),
  });
  return observed.value;
}

export function queryAndroidOfflinePackagesVisible(serial) {
  const startup = queryAndroidStartupProjection(serial);
  if (startup?.page === "OfflinePackages") return true;
  for (const tag of [
    "parity:offline-library-panel",
    "parity:offline-packages-panel",
    "parity:offline-refresh-button",
    "parity:offline-sync-button",
  ]) {
    if (queryAndroidSemanticNodes(serial, tag, { first: true })?.[0]) return true;
  }
  return false;
}

const MAP_LAYER_PARITY_IDS = Object.freeze({
  nexrad: "Nexrad",
  terrain_warning: "TerrainWarning",
});

export function layerToggleTag(layerId) {
  const parityId = MAP_LAYER_PARITY_IDS[layerId];
  if (!parityId) throw new Error(`unsupported E2E map layer: ${layerId}`);
  return `parity:tray-option:${parityId}`;
}

export function layerToggleNode(xml, layerId) {
  const tag = layerToggleTag(layerId);
  return findNode(xml, (node) => hasAndroidTag(node, tag));
}

export function destinationCenterEvidence(xml, destination, maxOffsetPx = 8) {
  const tray = findNode(xml, (node) => hasAndroidTag(node, "parity:map-selection-tray"));
  const airportItemTag = `parity:map-selection-item:airport-${destination}`;
  const selectedTag = `parity:map-selection-selected:${destination}`;
  const probePrefix = `parity:map-selection-center:${destination}:offset-px:`;
  const airportItem = findNode(xml, (node) => hasAndroidTag(node, airportItemTag));
  const selected = findNode(xml, (node) => hasAndroidTag(node, selectedTag));
  const probe = findNode(xml, (node) => androidTag(node).startsWith(probePrefix));
  const offsetPx = probe
    ? Number(androidTag(probe).slice(probePrefix.length))
    : Number.NaN;
  return {
    matched: tray !== null && airportItem !== null && selected !== null &&
      Number.isFinite(offsetPx) && offsetPx <= maxOffsetPx,
    airportItemTag,
    selectedTag,
    probeTag: probe ? androidTag(probe) : null,
    offsetPx: Number.isFinite(offsetPx) ? offsetPx : null,
  };
}

export function semanticProjectionFields(state) {
  const components = String(state ?? "").split(":");
  const fields = {};
  for (let index = 0; index + 1 < components.length; index += 2) {
    fields[components[index]] = components[index + 1];
  }
  return fields;
}

export function destinationCenterProjectionEvidence(entries, destination, maxOffsetPx = 8) {
  const state = entries?.[0]?.state ?? "";
  const fields = semanticProjectionFields(state);
  const offsetToken = fields["offset-px"] ?? "";
  const offsetPx = /^\d+$/.test(offsetToken) ? Number(offsetToken) : Number.NaN;
  return {
    matched: fields.selected === destination && fields.category === "airport" &&
      fields.centered === destination && Number.isFinite(offsetPx) && offsetPx <= maxOffsetPx,
    selected: fields.selected ?? null,
    category: fields.category ?? null,
    centered: fields.centered ?? null,
    probeTag: fields.centered && fields["offset-px"]
      ? `parity:map-selection-center:${fields.centered}:offset-px:${fields["offset-px"]}`
      : null,
    offsetPx: Number.isFinite(offsetPx) ? offsetPx : null,
  };
}

export function hasAndroidText(xml, text) {
  return findNode(xml, (node) => node.text === text) !== null;
}

export function rectOfBounds(bounds) {
  const match = /^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$/.exec(bounds ?? "");
  if (!match) throw new Error(`invalid Android bounds: ${bounds}`);
  const [, left, top, right, bottom] = match.map(Number);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

export function displayBoundsFromXml(xml) {
  const rects = findNodes(xml, (node) => Boolean(node.bounds)).map((node) => {
    try {
      return rectOfBounds(node.bounds);
    } catch (_error) {
      return null;
    }
  }).filter(Boolean);
  if (rects.length === 0) throw new Error("Android UI dump contains no display bounds");
  return {
    left: Math.min(...rects.map((rect) => rect.left)),
    top: Math.min(...rects.map((rect) => rect.top)),
    right: Math.max(...rects.map((rect) => rect.right)),
    bottom: Math.max(...rects.map((rect) => rect.bottom)),
    width: Math.max(...rects.map((rect) => rect.right)) - Math.min(...rects.map((rect) => rect.left)),
    height: Math.max(...rects.map((rect) => rect.bottom)) - Math.min(...rects.map((rect) => rect.top)),
  };
}

export function renderedFlightPlanSignature(xml) {
  const stateNode = findNode(xml, (node) => androidTag(node).startsWith("parity:plan-state:"));
  if (!stateNode) throw new Error("rendered flight-plan state semantics are unavailable");
  const stateTag = androidTag(stateNode);
  const countMatch = /^parity:plan-state:rows:(\d+):/.exec(stateTag);
  if (!countMatch) throw new Error(`invalid flight-plan state tag: ${stateTag}`);
  const rows = findNodes(xml, (node) => androidTag(node).startsWith("parity:plan-row:"))
    .map((node) => ({ tag: androidTag(node), label: androidNodeLabel(xml, node) }));
  return {
    rowCount: Number(countMatch[1]),
    stateTag,
    rows,
  };
}

export function classifyAerobagLogcat(logcat) {
  const lines = logcat.split(/\r?\n/);
  const evidence = [];
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/ANR in org\.aerobag\.app\b/.test(line)) evidence.push(line.trim());
    if (/Force finishing .*org\.aerobag\.app\/\.MainActivity/.test(line)) evidence.push(line.trim());
    if (/(?:Process org\.aerobag\.app .* has died|Killing \d+:org\.aerobag\.app\b)/.test(line)) {
      evidence.push(line.trim());
    }
    if (/prepared .* projection is unavailable/i.test(line)) evidence.push(line.trim());
    if (/FATAL EXCEPTION/.test(line)) {
      const block = lines.slice(index, index + 40);
      const processLine = block.find((candidate) => /Process: org\.aerobag\.app\b/.test(candidate));
      if (processLine) evidence.push(`${line.trim()} | ${processLine.trim()}`);
    }
  }
  return [...new Set(evidence)];
}

export function currentAerobagPid(serial) {
  const output = adbBestEffort(serial, ["shell", "pidof", ANDROID_PACKAGE]);
  if (output.status !== 0) return null;
  const pid = output.stdout.trim().split(/\s+/)[0];
  return /^\d+$/.test(pid) ? Number(pid) : null;
}

export function androidAppLifecyclePresent(serial) {
  if (currentAerobagPid(serial) !== null) return true;
  const activities = adb(serial, ["shell", "dumpsys", "activity", "activities"]);
  if (/^\s*\*?\s*Task\{[^\n]*\bA=\d+:org\.aerobag\.app\b/m.test(activities)) return true;
  if (/^\s*\*?\s*Hist\s+#\d+: ActivityRecord\{[^\n]*\borg\.aerobag\.app\/\.MainActivity\b/m.test(activities)) {
    return true;
  }
  const windows = adb(serial, ["shell", "dumpsys", "window", "windows"]);
  return /^\s*Window #\d+ Window\{[^\n]*\borg\.aerobag\.app\/org\.aerobag\.app\.MainActivity\b/m
    .test(windows);
}

export function saveAndroidRotationState(serial) {
  const get = (key) => adb(serial, ["shell", "settings", "get", "system", key]).trim();
  return {
    accelerometerRotation: get("accelerometer_rotation"),
    userRotation: get("user_rotation"),
  };
}

export function lockAndroidRotation(serial) {
  adb(serial, ["shell", "settings", "put", "system", "accelerometer_rotation", "0"]);
}

export function setAndroidRotation(serial, orientation) {
  const rotation = orientation === "portrait" ? "0" : orientation === "landscape" ? "1" : null;
  if (rotation === null) throw new Error(`unsupported Android orientation: ${orientation}`);
  lockAndroidRotation(serial);
  adb(serial, ["shell", "settings", "put", "system", "user_rotation", rotation]);
}

export function restoreAndroidRotationState(serial, state) {
  adbBestEffort(serial, [
    "shell", "settings", "put", "system", "user_rotation", state.userRotation,
  ]);
  adbBestEffort(serial, [
    "shell", "settings", "put", "system", "accelerometer_rotation", state.accelerometerRotation,
  ]);
}

export async function waitForAndroidOrientation(serial, orientation, timeoutMs = 15000) {
  let observed = null;
  await waitFor(() => {
    const xml = dumpAndroid(serial);
    assertNoAerobagAnr(xml);
    if (!findNode(xml, (node) => node.package === ANDROID_PACKAGE)) return false;
    observed = displayBoundsFromXml(xml);
    return orientation === "portrait"
      ? observed.height > observed.width
      : observed.width > observed.height;
  }, timeoutMs, `actual Android ${orientation} display bounds`);
  return observed;
}

export function scanAerobagLogcat(serial) {
  const logcat = adb(serial, ["logcat", "-d", "-v", "threadtime"], {
    maxBuffer: 16 * 1024 * 1024,
  });
  return { logcat, evidence: classifyAerobagLogcat(logcat) };
}

export function seedAerobagPrivateFiles(serial, fixtureFilesRoot) {
  const staging = "/data/local/tmp/aerobag-e2e-private-files";
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
  adbBestEffort(serial, ["shell", "rm", "-rf", staging]);
  adb(serial, ["push", `${fixtureFilesRoot}/.`, staging]);
  adb(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "-rf", "files/live-feeds"]);
  adb(serial, ["shell", "run-as", ANDROID_PACKAGE, "mkdir", "-p", "files"]);
  adb(serial, ["shell", "run-as", ANDROID_PACKAGE, "cp", "-R", `${staging}/live-feeds`, "files/"]);
  adbBestEffort(serial, ["shell", "rm", "-rf", staging]);
}

export function clearAerobagPersistedLiveFeeds(serial) {
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
  adbBestEffort(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "-rf", "files/live-feeds"]);
  adbBestEffort(serial, [
    "shell", "run-as", ANDROID_PACKAGE, "rm", "-f", "files/e2e-live-feed-promotion.pause",
  ]);
}

export function setAerobagPrivateSentinel(serial, relativePath, present) {
  if (!/^[a-zA-Z0-9._-]+$/.test(relativePath)) {
    throw new Error(`unsafe Aerobag sentinel path: ${relativePath}`);
  }
  const path = `files/${relativePath}`;
  if (present) {
    adb(serial, ["shell", "run-as", ANDROID_PACKAGE, "touch", path]);
  } else {
    adb(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "-f", path]);
  }
}

export function androidNodeLabel(xml, node) {
  const rect = rectOfBounds(node.bounds);
  return findNodes(xml, (candidate) => {
    const text = (candidate.text ?? "").trim();
    if (!text) return false;
    if (candidate.bounds === node.bounds) return true;
    try {
      const child = rectOfBounds(candidate.bounds);
      return child.left >= rect.left && child.right <= rect.right && child.top >= rect.top && child.bottom <= rect.bottom;
    } catch (_error) {
      return false;
    }
  })
    .map((candidate) => candidate.text.trim())
    .filter(Boolean)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

export function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitFor(fn, timeoutMs, message, intervalMs = 150) {
  await observeUntil(message, async () => await fn() ? true : null, {
    timeoutMs,
    intervalMs,
  });
}

export async function setAndroidWallClockAndWait(
  serial,
  epochMs,
  {
    adbCommand = adb,
    now = Date.now,
    wait = waitFor,
  } = {},
) {
  const requestedAtHostMs = now();
  adbCommand(serial, ["shell", "cmd", "alarm", "set-time", String(epochMs)]);
  await wait(async () => {
    const deviceEpochMs = Number(adbCommand(serial, ["shell", "date", "+%s%3N"]).trim());
    const expectedEpochMs = epochMs + (now() - requestedAtHostMs);
    return Number.isFinite(deviceEpochMs) && Math.abs(deviceEpochMs - expectedEpochMs) <= 1_500;
  }, CLOCK_SET_TIMEOUT_MS, "Android fixture clock did not reach the requested epoch", 100);
}

export async function waitForNode(serial, predicate, timeoutMs, message) {
  let found = null;
  await waitFor(async () => {
    found = findNode(dumpAndroid(serial), predicate);
    return found !== null;
  }, timeoutMs, message);
  return found;
}

export function tagExists(serial, tag) {
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

export async function activateAndroidNode(serial, node) {
  const tag = androidTag(node);
  if (!tag || !node?.bounds || !node?.["semantic-path"]) {
    throw new Error("Android semantic action requires tagged readiness evidence");
  }
  if (!clickAndroidSemanticNode(serial, tag, node.bounds, node["semantic-path"], {
    enabled: node.enabled,
    selected: node.selected,
    checked: node.checked,
    stateDescription: node["state-description"],
  })) {
    throw new Error(`Android semantic action ${tag} was rejected`);
  }
  return node;
}

export function androidSemanticNodeIsActionable(node) {
  return node?.enabled === "true" &&
    node?.clickable === "true" &&
    node?.visible === "true" &&
    node?.["center-reachable"] === "true";
}

export async function scrollUntilTag(serial, tag, maxSwipes = 8, requireReachable = false) {
  if (await scrollUntilTagInDirection(serial, tag, "down", maxSwipes, requireReachable)) {
    return true;
  }
  return scrollUntilTagInDirection(serial, tag, "up", maxSwipes, requireReachable);
}

export async function scrollUntilTagPrefix(serial, tagPrefix, maxSwipes = 8, requireReachable = false) {
  if (await scrollUntilTagPrefixInDirection(
    serial, tagPrefix, "down", maxSwipes, requireReachable,
  )) return true;
  return scrollUntilTagPrefixInDirection(serial, tagPrefix, "up", maxSwipes, requireReachable);
}

export function findVerticalScrollSurface(xml) {
  return findNode(xml, (node) =>
    node.scrollable === "true" &&
    node.package === ANDROID_PACKAGE &&
    node.class !== "android.widget.HorizontalScrollView");
}

export function findHorizontalScrollSurface(xml) {
  return findNode(xml, (node) => hasAndroidTag(node, "parity:plan-controls")) ??
    findNode(xml, (node) =>
      node.scrollable === "true" &&
      node.package === ANDROID_PACKAGE &&
      node.class === "android.widget.HorizontalScrollView") ??
    findNode(xml, (node) => {
      if (node.scrollable !== "true" || node.package !== ANDROID_PACKAGE || !node.bounds) return false;
      const bounds = rectOfBounds(node.bounds);
      return bounds.width > bounds.height * 2 && bounds.top > 1200;
    });
}

export function verticalScrollTargetIsReachable(xml, tag, { prefix = false } = {}) {
  const target = findNode(xml, (node) => {
    const nodeTag = androidTag(node);
    return prefix ? nodeTag.startsWith(tag) : nodeTag === tag;
  });
  if (!target) return false;
  const scrollSurface = findVerticalScrollSurface(xml);
  if (!scrollSurface) return true;
  const targetBounds = rectOfBounds(target.bounds);
  const surfaceBounds = rectOfBounds(scrollSurface.bounds);
  const centerX = targetBounds.left + targetBounds.width / 2;
  const centerY = targetBounds.top + targetBounds.height / 2;
  return centerX >= surfaceBounds.left && centerX <= surfaceBounds.right &&
    centerY >= surfaceBounds.top && centerY <= surfaceBounds.bottom;
}

export async function scrollAndroidAndAwait(serial, bounds, direction) {
  const before = dumpAndroid(serial);
  if (!scrollAndroidSemanticNode(serial, bounds, direction)) return false;
  try {
    await observeChangedValueUntilStable(
      "Android semantic scroll projection settled",
      () => dumpAndroid(serial),
      {
        initialValue: before,
        timeoutMs: E2E_TIMING.userResponseMs,
        intervalMs: E2E_TIMING.pollIntervalMs,
      },
    );
    return true;
  } catch (_error) {
    return false;
  }
}

export async function findNodeByScrolling(serial, predicate, maxSwipes = 8) {
  let xml = dumpAndroid(serial);
  let found = findNode(xml, predicate);
  if (found) return found;
  for (const direction of ["down", "up"]) {
    for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
      const scrollSurface =
        findVerticalScrollSurface(xml) ??
        findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"));
      if (!scrollSurface?.bounds || !await scrollAndroidAndAwait(serial, scrollSurface.bounds, direction)) break;
      xml = dumpAndroid(serial);
      found = findNode(xml, predicate);
      if (found) return found;
    }
  }
  return null;
}

async function scrollUntilTagPrefixInDirection(
  serial, tagPrefix, direction, maxSwipes, requireReachable,
) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (requireReachable
      ? verticalScrollTargetIsReachable(xml, tagPrefix, { prefix: true })
      : findNode(xml, (node) => androidTag(node).startsWith(tagPrefix))) return true;
    const scrollSurface =
      findVerticalScrollSurface(xml) ??
      findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"));
    if (!scrollSurface?.bounds || !await scrollAndroidAndAwait(serial, scrollSurface.bounds, direction)) break;
  }
  const xml = dumpAndroid(serial);
  return requireReachable
    ? verticalScrollTargetIsReachable(xml, tagPrefix, { prefix: true })
    : findNode(xml, (node) => androidTag(node).startsWith(tagPrefix)) !== null;
}

export async function scrollHorizontallyUntilTag(serial, tag, maxSwipes = 8) {
  for (const direction of ["forward", "backward"]) {
    for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
      const xml = dumpAndroid(serial);
      if (findNode(xml, (node) => hasAndroidTag(node, tag))) return true;
      const horizontalSurface = findHorizontalScrollSurface(xml);
      if (!horizontalSurface) return false;
      if (!await scrollAndroidAndAwait(serial, horizontalSurface.bounds, direction)) break;
    }
  }
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

async function scrollUntilTagInDirection(serial, tag, direction, maxSwipes, requireReachable) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const target = queryAndroidExactProjection(serial, tag, { includeDescendantText: false })[0];
    if (target && (!requireReachable || target["center-reachable"] === "true")) {
      return true;
    }
    if (!scrollAndroidSemanticSurface(serial, "vertical", direction)) break;
  }
  const target = queryAndroidExactProjection(serial, tag, { includeDescendantText: false })[0];
  return Boolean(target && (!requireReachable || target["center-reachable"] === "true"));
}

export function pressKey(serial, keyCode) {
  adb(serial, ["shell", "input", "keyevent", keyCode]);
}

export function swipe(serial, startX, startY, endX, endY, durationMs = 450) {
  adb(serial, [
    "shell",
    "input",
    "touchscreen",
    "swipe",
    String(Math.round(startX)),
    String(Math.round(startY)),
    String(Math.round(endX)),
    String(Math.round(endY)),
    String(Math.round(durationMs)),
  ]);
}

export function wakeAndUnlock(serial) {
  adbBestEffort(serial, ["shell", "input", "keyevent", "KEYCODE_WAKEUP"]);
  adbBestEffort(serial, ["shell", "wm", "dismiss-keyguard"]);
  adbBestEffort(serial, ["shell", "input", "keyevent", "KEYCODE_MENU"]);
  adbBestEffort(serial, ["shell", "cmd", "statusbar", "collapse"]);
}

export function grantAerobagRuntimePermissions(serial) {
  for (const permission of [
    "android.permission.ACCESS_FINE_LOCATION",
    "android.permission.ACCESS_COARSE_LOCATION",
    "android.permission.POST_NOTIFICATIONS",
  ]) {
    adbBestEffort(serial, ["shell", "pm", "grant", ANDROID_PACKAGE, permission]);
  }
}

function firstAerobagProcessNode(serial) {
  try {
    return queryAndroidSemanticNodes(
      serial,
      "parity:app-process:",
      { prefix: true, first: true },
    )[0] ?? null;
  } catch (error) {
    if (/curl: \(28\) Operation timed out/.test(error.message)) {
      throw new TransientObservationError(
        "Android semantic process query timed out",
        error,
      );
    }
    throw error;
  }
}

function semanticNodeIdentity(node) {
  return JSON.stringify({
    tag: node?.["resource-id"] ?? null,
    path: node?.["semantic-path"] ?? null,
    bounds: node?.bounds ?? null,
  });
}

export async function restartAndroidAppAcrossSemanticLifecycle({
  stopApp,
  prepareSemanticDriver,
  startApp,
  readProcessNode,
  readStoppedState = async () => (await readProcessNode()) === null,
  timeoutMs = E2E_TIMING.startupMs,
  intervalMs = E2E_TIMING.pollIntervalMs,
}) {
  await stopApp();
  await prepareSemanticDriver();
  await observeUntil(
    "previous Aerobag process, task, window, and semantic UI removed",
    async () => (await readStoppedState()) ? true : null,
    {
      timeoutMs,
      intervalMs,
      consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
    },
  );
  await startApp();
  const observed = await observeUntil(
    "new Aerobag semantic UI visible",
    () => readProcessNode(),
    {
      timeoutMs,
      intervalMs,
      consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
      consecutiveValueKey: semanticNodeIdentity,
    },
  );
  return observed.value;
}

export async function launchFreshAndroidApp(
  serial,
  { clearUiPrefs = true, clearCoreSettings = false, armLayerNavKvFault = false } = {},
) {
  adb(serial, ["wait-for-device"]);
  wakeAndUnlock(serial);
  grantAerobagRuntimePermissions(serial);
  const startArgs = ["shell", "am", "start", "-W", "-n", ANDROID_ACTIVITY];
  if (clearUiPrefs) {
    startArgs.push("--ez", DEBUG_CLEAR_UI_PREFS_EXTRA, "true");
  }
  if (clearCoreSettings) {
    startArgs.push("--ez", DEBUG_CLEAR_CORE_SETTINGS_EXTRA, "true");
  }
  if (armLayerNavKvFault) {
    startArgs.push("--ez", DEBUG_ARM_LAYER_NAV_KV_FAULT_EXTRA, "true");
  }
  await restartAndroidAppAcrossSemanticLifecycle({
    // `force-stop` removes the task asynchronously. A delayed remove-task
    // timeout can then kill the replacement process after `am start` returns.
    // `stop-app` provides the process reset the journeys need without that race.
    stopApp: () => adb(serial, ["shell", "am", "stop-app", ANDROID_PACKAGE]),
    prepareSemanticDriver: () => ensureAndroidSemanticDriver(serial),
    startApp: () => {
      // `am start -W` includes Android process startup and can exceed the generic
      // command timeout on cold CI emulators. Semantic lifecycle is checked here.
      adb(serial, startArgs, { timeout: APP_START_TIMEOUT_MS });
    },
    readProcessNode: () => firstAerobagProcessNode(serial),
    readStoppedState: async () =>
      firstAerobagProcessNode(serial) === null && !androidAppLifecyclePresent(serial),
  });
}

export async function acceptDisclaimerIfPresent(serial) {
  const initial = (await observeUntil("initial mandatory disclaimer state", () =>
    queryAndroidStartupProjection(serial), {
    timeoutMs: E2E_TIMING.startupMs,
    intervalMs: E2E_TIMING.resourcePollIntervalMs,
  })).value;
  if (initial?.disclaimer_required !== "true") return false;
  await performTransition("accept mandatory disclaimer", {
    readinessSamples: 1,
    ready: async () => {
      const readyState = queryAndroidStartupProjection(serial);
      if (readyState?.disclaimer_required !== "true") return null;
      const button = queryAndroidSemanticNodes(
        serial,
        "parity:disclaimer-accept-button",
      )?.[0] ?? null;
      return androidSemanticNodeIsActionable(button) ? button : null;
    },
    act: async (readyButton) => activateAndroidNode(serial, readyButton),
    complete: async () => {
      const nextState = queryAndroidStartupProjection(serial);
      return nextState?.disclaimer_required === "false" ? nextState : null;
    },
    responseTimeoutMs: E2E_TIMING.userResponseMs,
    readyTimeoutMs: E2E_TIMING.startupMs,
  });
  await observeUntil("application startup after accepting mandatory disclaimer", () => {
    const nextState = queryAndroidStartupState(serial);
    return nextState?.disclaimer_required === "false" ? nextState : null;
  }, {
    timeoutMs: E2E_TIMING.startupMs,
    intervalMs: E2E_TIMING.resourcePollIntervalMs,
    consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
  });
  return true;
}

export function assertRuntimeIsAvailable(serial) {
  if (!queryAndroidRuntimeReadyForJourney(serial)) {
    throw new Error("offline packages page is visible; install a usable nav-db package before running Android E2E");
  }
}

export function androidOfflinePackagesVisible(xml) {
  return findNode(xml, (node) =>
    hasAndroidTag(node, "parity:offline-library-panel") ||
    hasAndroidTag(node, "parity:offline-packages-panel") ||
    hasAndroidTag(node, "parity:offline-refresh-button") ||
    hasAndroidTag(node, "parity:offline-sync-button")
  ) !== null;
}

export function androidRuntimeReadyForJourney(xml) {
  return androidRuntimeUiVisible(xml) && !androidOfflinePackagesVisible(xml);
}
