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

export function dumpAndroid(serial, dumpPath = "/sdcard/aerobag-e2e.xml") {
  adb(serial, ["shell", "uiautomator", "dump", dumpPath]);
  return adb(serial, ["exec-out", "cat", dumpPath]);
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
      [...match[0].matchAll(/([a-zA-Z0-9_-]+)="([^"]*)"/g)]
        .map((entry) => [entry[1], decodeXml(entry[2])]),
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

export async function scrollUntilTag(serial, tag, maxSwipes = 8) {
  if (await scrollUntilTagInDirection(serial, tag, "down", maxSwipes)) {
    return true;
  }
  return scrollUntilTagInDirection(serial, tag, "up", maxSwipes);
}

async function scrollUntilTagInDirection(serial, tag, direction, maxSwipes) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (findNode(xml, (node) => hasAndroidTag(node, tag))) {
      return true;
    }
    const scrollSurface =
      findNode(xml, (node) => node.scrollable === "true" && node.package === ANDROID_PACKAGE) ??
      findNode(xml, (node) => hasAndroidTag(node, "parity:offline-packages-panel"));
    const bounds = rectOfBounds(scrollSurface?.bounds ?? "[90,383][1065,2021]");
    const x = Math.round((bounds.left + bounds.right) / 2);
    const inset = Math.min(80, bounds.height / 5);
    const startY = direction === "down"
      ? Math.round(bounds.bottom - inset)
      : Math.round(bounds.top + inset);
    const endY = direction === "down"
      ? Math.round(bounds.top + inset)
      : Math.round(bounds.bottom - inset);
    adb(serial, ["shell", "input", "touchscreen", "swipe", String(x), String(startY), String(x), String(endY), "450"]);
    await delay(250);
  }
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

export function inputText(serial, text) {
  adb(serial, ["shell", "input", "text", text.replaceAll(" ", "%s")]);
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

export function clearFocusedText(serial, maxChars = 160) {
  pressKey(serial, "KEYCODE_MOVE_END");
  for (let i = 0; i < maxChars; i += 1) {
    pressKey(serial, "KEYCODE_DEL");
  }
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
  if (clearUiPrefs) {
    adbBestEffort(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "shared_prefs/aerobag_ui.xml"]);
  }
  if (clearCoreSettings) {
    adbBestEffort(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "files/core-settings-v1.json"]);
  }
  const startArgs = ["shell", "am", "start", "-W", "-n", ANDROID_ACTIVITY];
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
  const offlineVisible = findNode(xml, (node) =>
    hasAndroidTag(node, "parity:offline-library-panel") ||
    hasAndroidTag(node, "parity:offline-packages-panel") ||
    hasAndroidTag(node, "parity:offline-refresh-button") ||
    hasAndroidTag(node, "parity:offline-sync-button")
  );
  if (offlineVisible) {
    throw new Error("offline packages page is visible; install a usable nav-db package before running Android E2E");
  }
}
