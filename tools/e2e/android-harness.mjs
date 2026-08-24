// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

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

export function adbArgs(serial, args) {
  return serial ? ["-s", serial, ...args] : args;
}

export function adb(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), {
    encoding: "utf8",
    timeout: 20000,
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
    timeout: 20000,
    ...options,
  });
}

export function adbBuffer(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), {
    encoding: null,
    timeout: 20000,
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

export function dumpAndroid(serial, dumpPath = `/sdcard/aerobag-e2e-${process.pid}.xml`) {
  // Compose's outlined text creates many accessibility-only descendants.
  // Compressed dumps retain the tagged semantic controls and omit that noise.
  let lastError = null;
  const modes = ["compressed", "compressed", "compressed", "full"];
  for (let attempt = 0; attempt < modes.length; attempt += 1) {
    try {
      adbBestEffort(serial, ["shell", "rm", "-f", dumpPath], { timeout: 3000 });
      const modeArgs = modes[attempt] === "compressed" ? ["--compressed"] : [];
      adb(serial, ["shell", "uiautomator", "dump", ...modeArgs, dumpPath], { timeout: 10000 });
      return adb(serial, ["exec-out", "cat", dumpPath]);
    } catch (error) {
      lastError = error;
      adbBestEffort(serial, ["shell", "input", "keyevent", "KEYCODE_WAKEUP"], { timeout: 3000 });
      spawnSync("sleep", [String(0.4 + attempt * 0.3)]);
    }
  }
  throw lastError;
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

export function findSystemUiAnrWaitButton(xml) {
  const systemUiTitle = findNode(xml, (node) =>
    node.package === "android" &&
    node["resource-id"] === "android:id/alertTitle" &&
    node.text === "System UI isn't responding"
  );
  if (!systemUiTitle) return null;
  return findNode(xml, (node) =>
    node.package === "android" &&
    node["resource-id"] === "android:id/aerr_wait" &&
    node.enabled === "true"
  );
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

export function centerOfBounds(bounds) {
  const rect = rectOfBounds(bounds);
  return {
    x: Math.round((rect.left + rect.right) / 2),
    y: Math.round((rect.top + rect.bottom) / 2),
  };
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
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      if (await fn()) return;
    } catch (error) {
      lastError = error;
    }
    await delay(intervalMs);
  }
  throw new Error(`${message}${lastError ? `: ${lastError.message}` : ""}`);
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

export async function tapNode(serial, node) {
  const { x, y } = centerOfBounds(node.bounds);
  adb(serial, ["shell", "input", "tap", String(x), String(y)]);
}

export async function tapTag(serial, tag, timeoutMs = 5000) {
  const node = await waitForNode(serial, (candidate) => hasAndroidTag(candidate, tag), timeoutMs, tag);
  await tapNode(serial, node);
  return node;
}

export async function tapFirstPresentTag(serial, tags, timeoutMs = 5000) {
  const node = await waitForNode(
    serial,
    (candidate) => tags.some((tag) => hasAndroidTag(candidate, tag)),
    timeoutMs,
    `one of ${tags.join(", ")}`,
  );
  await tapNode(serial, node);
  return androidTag(node);
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

export function verticalScrollGesture(bounds, direction) {
  const inset = Math.min(80, bounds.height / 5);
  // Starting in Android's status or navigation gesture zones opens the shade
  // or leaves the app instead of scrolling Compose content.
  const systemGestureInset = Math.max(inset, 180);
  const safeTop = Math.min(bounds.bottom - 1, bounds.top + systemGestureInset);
  const safeBottom = Math.max(safeTop + 1, bounds.bottom - systemGestureInset);
  return {
    x: Math.round((bounds.left + bounds.right) / 2),
    startY: Math.round(direction === "down" ? safeBottom : safeTop),
    endY: Math.round(direction === "down" ? safeTop : safeBottom),
  };
}

async function recoverObscuredAndroidApp(serial, xml) {
  const aerobagVisible = findNode(xml, (node) => node.package === ANDROID_PACKAGE);
  const systemUiVisible = findNode(xml, (node) => node.package === "com.android.systemui");
  if (aerobagVisible || !systemUiVisible) return xml;
  adbBestEffort(serial, ["shell", "cmd", "statusbar", "collapse"], { timeout: 3000 });
  await delay(250);
  return dumpAndroid(serial);
}

async function scrollUntilTagPrefixInDirection(
  serial, tagPrefix, direction, maxSwipes, requireReachable,
) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = await recoverObscuredAndroidApp(serial, dumpAndroid(serial));
    if (requireReachable
      ? verticalScrollTargetIsReachable(xml, tagPrefix, { prefix: true })
      : findNode(xml, (node) => androidTag(node).startsWith(tagPrefix))) return true;
    const scrollSurface =
      findVerticalScrollSurface(xml) ??
      findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"));
    const bounds = rectOfBounds(scrollSurface?.bounds ?? "[90,383][1065,2021]");
    const { x, startY, endY } = verticalScrollGesture(bounds, direction);
    swipe(serial, x, startY, x, endY, 450);
    await delay(250);
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
      const bounds = rectOfBounds(horizontalSurface.bounds);
      const inset = Math.min(80, bounds.width / 5);
      const startX = direction === "forward" ? bounds.right - inset : bounds.left + inset;
      const endX = direction === "forward" ? bounds.left + inset : bounds.right - inset;
      const y = Math.round((bounds.top + bounds.bottom) / 2);
      swipe(serial, startX, y, endX, y, 450);
      await delay(250);
    }
  }
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

async function scrollUntilTagInDirection(serial, tag, direction, maxSwipes, requireReachable) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = await recoverObscuredAndroidApp(serial, dumpAndroid(serial));
    if (requireReachable
      ? verticalScrollTargetIsReachable(xml, tag)
      : findNode(xml, (node) => hasAndroidTag(node, tag))) {
      return true;
    }
    const scrollSurface =
      findVerticalScrollSurface(xml) ??
      findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"));
    const bounds = rectOfBounds(scrollSurface?.bounds ?? "[90,383][1065,2021]");
    const { x, startY, endY } = verticalScrollGesture(bounds, direction);
    adb(serial, ["shell", "input", "touchscreen", "swipe", String(x), String(startY), String(x), String(endY), "450"]);
    await delay(250);
  }
  const xml = dumpAndroid(serial);
  return requireReachable
    ? verticalScrollTargetIsReachable(xml, tag)
    : findNode(xml, (node) => hasAndroidTag(node, tag)) !== null;
}

const ANDROID_DIRECT_TEXT_KEYS = Object.freeze({
  "/": "KEYCODE_SLASH",
  "-": "KEYCODE_MINUS",
  ".": "KEYCODE_PERIOD",
  ",": "KEYCODE_COMMA",
  "@": "KEYCODE_AT",
  "=": "KEYCODE_EQUALS",
  "[": "KEYCODE_LEFT_BRACKET",
  "]": "KEYCODE_RIGHT_BRACKET",
  "\\": "KEYCODE_BACKSLASH",
  ";": "KEYCODE_SEMICOLON",
  "'": "KEYCODE_APOSTROPHE",
  "`": "KEYCODE_GRAVE",
});

const ANDROID_SHIFTED_TEXT_KEYS = Object.freeze({
  ":": "KEYCODE_SEMICOLON",
  "?": "KEYCODE_SLASH",
  "_": "KEYCODE_MINUS",
  "+": "KEYCODE_EQUALS",
  "!": "KEYCODE_1",
  "#": "KEYCODE_3",
  "$": "KEYCODE_4",
  "%": "KEYCODE_5",
  "^": "KEYCODE_6",
  "&": "KEYCODE_7",
  "*": "KEYCODE_8",
  "(": "KEYCODE_9",
  ")": "KEYCODE_0",
  "<": "KEYCODE_COMMA",
  ">": "KEYCODE_PERIOD",
  "\"": "KEYCODE_APOSTROPHE",
  "{": "KEYCODE_LEFT_BRACKET",
  "}": "KEYCODE_RIGHT_BRACKET",
  "|": "KEYCODE_BACKSLASH",
  "~": "KEYCODE_GRAVE",
});

export function androidTextInputCommands(text) {
  const commands = [];
  let plain = "";
  const flushPlain = () => {
    if (!plain) return;
    commands.push(["shell", "input", "text", plain.replaceAll(" ", "%s")]);
    plain = "";
  };
  for (const character of String(text)) {
    if (/^[A-Za-z0-9 ]$/.test(character)) {
      plain += character;
      continue;
    }
    flushPlain();
    const directKey = ANDROID_DIRECT_TEXT_KEYS[character];
    if (directKey) {
      commands.push(["shell", "input", "keyevent", directKey]);
      continue;
    }
    const shiftedKey = ANDROID_SHIFTED_TEXT_KEYS[character];
    if (shiftedKey) {
      commands.push([
        "shell", "input", "keycombination", "KEYCODE_SHIFT_LEFT", shiftedKey,
      ]);
      continue;
    }
    throw new Error(`Android E2E text injection does not support ${JSON.stringify(character)}`);
  }
  flushPlain();
  return commands;
}

export function inputText(serial, text) {
  for (const command of androidTextInputCommands(text)) {
    adb(serial, command);
    spawnSync("sleep", ["0.075"]);
  }
  spawnSync("sleep", ["0.25"]);
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

export function androidClearTextCommand(maxChars = 160) {
  return [
    "shell", "input", "keyevent", "KEYCODE_MOVE_END",
    ...Array.from({ length: maxChars }, () => "KEYCODE_DEL"),
  ];
}

export function clearFocusedText(serial, maxChars = 160) {
  adb(serial, androidClearTextCommand(maxChars));
  spawnSync("sleep", ["0.25"]);
  // Gboard can re-commit one composing character after the bulk key stream.
  adb(serial, androidClearTextCommand(16));
  spawnSync("sleep", ["0.25"]);
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

export async function launchFreshAndroidApp(
  serial,
  { clearUiPrefs = true, clearCoreSettings = false, armLayerNavKvFault = false } = {},
) {
  adb(serial, ["wait-for-device"]);
  wakeAndUnlock(serial);
  grantAerobagRuntimePermissions(serial);
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
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
  adb(serial, startArgs);
  let dismissedSystemUiAnr = false;
  await waitFor(async () => {
    const xml = dumpAndroid(serial);
    if (findNode(xml, (node) => node.package === ANDROID_PACKAGE)) {
      return true;
    }
    const waitButton = findSystemUiAnrWaitButton(xml);
    if (waitButton && !dismissedSystemUiAnr) {
      dismissedSystemUiAnr = true;
      console.warn("Android System UI ANR obscured Aerobag during startup; selecting Wait once");
      await tapNode(serial, waitButton);
    }
    return false;
  }, 90000, "Aerobag app visible");
}

export async function acceptDisclaimerIfPresent(serial) {
  const xml = dumpAndroid(serial);
  if (!findNode(xml, (node) => hasAndroidTag(node, "parity:disclaimer-accept-button"))) {
    return false;
  }
  await tapTag(serial, "parity:disclaimer-accept-button", 3000);
  await waitFor(
    () => !findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, "parity:disclaimer-accept-button")),
    10000,
    "disclaimer dismissed",
  );
  return true;
}

export function assertRuntimeIsAvailable(serial) {
  const xml = dumpAndroid(serial);
  if (androidOfflinePackagesVisible(xml)) {
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
