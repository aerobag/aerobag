// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawnSync } from "node:child_process";

export const ANDROID_PACKAGE = "org.aerobag.app";
export const ANDROID_ACTIVITY = `${ANDROID_PACKAGE}/.MainActivity`;

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

export function hasAndroidText(xml, text) {
  return findNode(xml, (node) => node.text === text) !== null;
}

export function rectOfBounds(bounds) {
  const match = /^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$/.exec(bounds ?? "");
  if (!match) throw new Error(`invalid Android bounds: ${bounds}`);
  const [, left, top, right, bottom] = match.map(Number);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
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

export async function launchFreshAndroidApp(serial, { clearUiPrefs = true, clearCoreSettings = false } = {}) {
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
  adb(serial, ["shell", "am", "start", "-W", "-n", ANDROID_ACTIVITY]);
  wakeAndUnlock(serial);
  await waitForNode(serial, (node) => node.package === ANDROID_PACKAGE, 90000, "Aerobag app visible");
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
