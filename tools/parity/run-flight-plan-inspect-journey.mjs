#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";

const DEFAULT_WEB_URL = "http://127.0.0.1:8082/";
const JOURNEY_NAME = "flight-plan-inspect-insert";
const ANDROID_PACKAGE = "net.jonh.aerobag.prototype";
const ANDROID_ACTIVITY = `${ANDROID_PACKAGE}/.MainActivity`;

function usage() {
  console.log(`Usage:
  node tools/parity/run-flight-plan-inspect-journey.mjs web [--url http://127.0.0.1:8082/]
  node tools/parity/run-flight-plan-inspect-journey.mjs android [--serial emulator-5554]
  node tools/parity/run-flight-plan-inspect-journey.mjs both [--url http://127.0.0.1:8082/] [--serial emulator-5554]

The web runner launches a temporary headless Chrome through CDP. Set CHROME_BIN if Chrome is not on PATH.
The Android runner expects the app to be installed and uses adb + uiautomator XML dumps.`);
}

function parseArgs(argv) {
  const args = { platform: argv[2], url: DEFAULT_WEB_URL, serial: process.env.ANDROID_SERIAL ?? "" };
  if (argv.includes("-h") || argv.includes("--help")) {
    args.help = true;
    return args;
  }
  for (let i = 3; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--url") {
      args.url = argv[++i];
    } else if (arg === "--serial") {
      args.serial = argv[++i];
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function result(platform) {
  return {
    journey: JOURNEY_NAME,
    platform,
    started_at: new Date().toISOString(),
    steps: [],
    gaps: [],
  };
}

function recordStep(out, name, status = "ok", detail = undefined) {
  out.steps.push({ name, status, ...(detail === undefined ? {} : { detail }) });
}

function recordGap(out, name, detail) {
  out.gaps.push({ name, detail });
  recordStep(out, name, "gap", detail);
}

function findChrome() {
  const candidates = [
    process.env.CHROME_BIN,
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
  ].filter(Boolean);
  for (const candidate of candidates) {
    const found = spawnSync("which", [candidate], { encoding: "utf8" });
    if (found.status === 0) {
      return found.stdout.trim();
    }
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

function httpJson(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => {
        body += chunk;
      });
      res.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    }).on("error", reject);
  });
}

class CdpSocket {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.nextId = 1;
    this.pending = new Map();
    this.events = [];
    socket.on("data", (chunk) => this.onData(chunk));
  }

  static async connect(webSocketDebuggerUrl) {
    const parsed = new URL(webSocketDebuggerUrl);
    const socket = net.connect(Number(parsed.port), parsed.hostname);
    await once(socket, "connect");
    const key = crypto.randomBytes(16).toString("base64");
    socket.write([
      `GET ${parsed.pathname}${parsed.search} HTTP/1.1`,
      `Host: ${parsed.host}`,
      "Upgrade: websocket",
      "Connection: Upgrade",
      `Sec-WebSocket-Key: ${key}`,
      "Sec-WebSocket-Version: 13",
      "\r\n",
    ].join("\r\n"));
    await readUntil(socket, "\r\n\r\n");
    return new CdpSocket(socket);
  }

  send(method, params = {}) {
    const id = this.nextId++;
    this.writeFrame(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`CDP timeout: ${method}`));
        }
      }, 15000).unref();
    });
  }

  writeFrame(text) {
    const payload = Buffer.from(text);
    const mask = crypto.randomBytes(4);
    let header;
    if (payload.length < 126) {
      header = Buffer.from([0x81, 0x80 | payload.length]);
    } else if (payload.length < 65536) {
      header = Buffer.alloc(4);
      header[0] = 0x81;
      header[1] = 0x80 | 126;
      header.writeUInt16BE(payload.length, 2);
    } else {
      throw new Error("CDP frame too large");
    }
    const masked = Buffer.alloc(payload.length);
    for (let i = 0; i < payload.length; i += 1) {
      masked[i] = payload[i] ^ mask[i % 4];
    }
    this.socket.write(Buffer.concat([header, mask, masked]));
  }

  onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (this.buffer.length >= 2) {
      const first = this.buffer[0];
      const second = this.buffer[1];
      let offset = 2;
      let length = second & 0x7f;
      if (length === 126) {
        if (this.buffer.length < 4) return;
        length = this.buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        throw new Error("CDP frame too large");
      }
      const masked = (second & 0x80) !== 0;
      const maskOffset = offset;
      if (masked) offset += 4;
      if (this.buffer.length < offset + length) return;
      let payload = this.buffer.subarray(offset, offset + length);
      if (masked) {
        const mask = this.buffer.subarray(maskOffset, maskOffset + 4);
        payload = Buffer.from(payload.map((byte, index) => byte ^ mask[index % 4]));
      }
      this.buffer = this.buffer.subarray(offset + length);
      if ((first & 0x0f) === 0x8) {
        this.socket.end();
        return;
      }
      const message = JSON.parse(payload.toString("utf8"));
      if (message.id && this.pending.has(message.id)) {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
      } else {
        this.events.push(message);
      }
    }
  }

  close() {
    this.socket.end();
  }
}

function once(emitter, event) {
  return new Promise((resolve, reject) => {
    emitter.once(event, resolve);
    emitter.once("error", reject);
  });
}

function readUntil(socket, marker) {
  return new Promise((resolve, reject) => {
    let body = "";
    function onData(chunk) {
      body += chunk.toString("utf8");
      if (body.includes(marker)) {
        socket.off("data", onData);
        resolve(body);
      }
    }
    socket.on("data", onData);
    socket.once("error", reject);
  });
}

async function launchChrome() {
  const chrome = findChrome();
  if (!chrome) {
    throw new Error("No Chrome/Chromium binary found. Install chromium or set CHROME_BIN.");
  }
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "aerobag-parity-chrome-"));
  const port = 9222 + Math.floor(Math.random() * 1000);
  const proc = spawn(chrome, [
    "--headless=new",
    "--disable-gpu",
    "--no-sandbox",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  await waitFor(async () => {
    await httpJson(`http://127.0.0.1:${port}/json/version`);
    return true;
  }, 10000, `Chrome did not open CDP port ${port}`);
  return { proc, port, profile };
}

async function webJourney(url) {
  const out = result("web");
  const chrome = await launchChrome();
  let cdp;
  try {
    const tabs = await httpJson(`http://127.0.0.1:${chrome.port}/json`);
    cdp = await CdpSocket.connect(tabs[0].webSocketDebuggerUrl);
    await cdp.send("Page.enable");
    await cdp.send("Runtime.enable");
    await cdp.send("Page.navigate", { url });
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "map surface");
    recordStep(out, "app started");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]') !== null", "plan append input");
    recordStep(out, "opened plan page");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "chart after plan CDI");
    recordStep(out, "plan CDI returned to chart");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]') !== null", "plan after chart CDI");
    recordStep(out, "chart CDI returned to plan");

    await webSetInput(cdp, "[data-testid=\"plan-append-route-input\"]", "KBFI");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]')?.closest('form') !== null", "plan append form");
    await cdp.send("Runtime.evaluate", {
      expression: "document.querySelector('[data-testid=\"plan-append-route-input\"]').closest('form').requestSubmit()",
      awaitPromise: true,
    });
    await waitForWeb(cdp, "document.body.innerText.includes('KBFI')", "appended KBFI");
    recordStep(out, "appended KBFI to flight plan");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "returned chart");
    recordStep(out, "returned to chart");

    await webSetInput(cdp, "[data-testid=\"chart-search-input\"]", "KOLM");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"chart-search-suggestion-KOLM\"]') !== null", "KOLM search suggestion");
    await webClick(cdp, "[data-testid=\"chart-search-suggestion-KOLM\"]");
    await delay(500);
    recordStep(out, "recentered on KOLM via chart search");

    await webDrag(cdp, "[data-testid=\"map-surface\"]", 40, 20);
    recordStep(out, "dragged map surface");

    await webClickAtCenter(cdp, "[data-testid=\"map-surface\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-selection-tray\"]') !== null", "map selection tray");
    recordStep(out, "opened map inspection tray");

    await cdp.send("Runtime.evaluate", {
      expression: `
        [...document.querySelectorAll('[data-testid^="map-selection-item-airport-"]')]
          .find((el) => el.textContent.includes('KOLM'))?.click()
      `,
      awaitPromise: true,
    });
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-selection-action-insert\"]:not(:disabled)') !== null", "insert action enabled");
    await webClick(cdp, "[data-testid=\"map-selection-action-insert\"]");
    recordStep(out, "inserted inspected airport into flight plan");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.body.innerText.includes('KOLM')", "KOLM visible in plan");
    recordStep(out, "verified KOLM in flight plan");
    out.finished_at = new Date().toISOString();
    out.status = out.gaps.length === 0 ? "pass" : "gaps";
    return out;
  } finally {
    cdp?.close();
    chrome.proc.kill("SIGTERM");
    fs.rmSync(chrome.profile, { recursive: true, force: true });
  }
}

async function webEval(cdp, expression) {
  const res = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (res.exceptionDetails) {
    throw new Error(res.exceptionDetails.text);
  }
  return res.result.value;
}

async function waitForWeb(cdp, expression, label) {
  await waitFor(() => webEval(cdp, `Boolean(${expression})`), 12000, `Timed out waiting for ${label}`);
}

async function webClick(cdp, selector) {
  const box = await webElementBox(cdp, selector);
  await dispatchClick(cdp, box.x + box.width / 2, box.y + box.height / 2);
}

async function webClickAtCenter(cdp, selector) {
  const box = await webElementBox(cdp, selector);
  await dispatchClick(cdp, box.x + box.width / 2, box.y + box.height / 2);
}

async function webDrag(cdp, selector, dx, dy) {
  const box = await webElementBox(cdp, selector);
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x + dx, y: y + dy, button: "left", buttons: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x + dx, y: y + dy, button: "left", buttons: 0, clickCount: 1 });
}

async function dispatchClick(cdp, x, y) {
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 });
}

async function webElementBox(cdp, selector) {
  const box = await webEval(cdp, `
    (() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    })()
  `);
  if (!box) throw new Error(`missing web selector ${selector}`);
  return box;
}

async function webSetInput(cdp, selector, value) {
  await cdp.send("Runtime.evaluate", {
    expression: `
      (() => {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error('missing input ${selector}');
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
        setter.call(el, ${JSON.stringify(value)});
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.focus();
      })()
    `,
    awaitPromise: true,
  });
}

async function waitFor(fn, timeoutMs, message) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      if (await fn()) return;
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  throw new Error(`${message}${lastError ? `: ${lastError.message}` : ""}`);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function adbArgs(serial, args) {
  return serial ? ["-s", serial, ...args] : args;
}

function adb(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), { encoding: "utf8", ...options });
  if (res.status !== 0) {
    throw new Error(`adb ${args.join(" ")} failed: ${res.stderr || res.stdout}`);
  }
  return res.stdout;
}

function dumpAndroid(serial) {
  adb(serial, ["shell", "uiautomator", "dump", "/sdcard/aerobag-parity.xml"]);
  return adb(serial, ["exec-out", "cat", "/sdcard/aerobag-parity.xml"]);
}

function findNode(xml, predicate) {
  const nodeRegex = /<node\b[^>]*>/g;
  let match;
  while ((match = nodeRegex.exec(xml))) {
    const attrs = Object.fromEntries([...match[0].matchAll(/([a-zA-Z0-9_-]+)="([^"]*)"/g)].map((entry) => [entry[1], decodeXml(entry[2])]));
    if (predicate(attrs)) return attrs;
  }
  return null;
}

function hasAndroidTag(node, tag) {
  const contentDescription = node["content-desc"] ?? "";
  const resourceId = node["resource-id"] ?? "";
  return (
    contentDescription === tag ||
    resourceId === tag ||
    resourceId.endsWith(`:id/${tag}`) ||
    resourceId.endsWith(`/id/${tag}`)
  );
}

function androidTag(node) {
  const contentDescription = node["content-desc"] ?? "";
  if (contentDescription.startsWith("parity:")) return contentDescription;
  const resourceId = node["resource-id"] ?? "";
  const marker = "parity:";
  const offset = resourceId.indexOf(marker);
  return offset >= 0 ? resourceId.slice(offset) : "";
}

function hasAndroidText(xml, text) {
  return findNode(xml, (node) => node.text === text) !== null;
}

function decodeXml(text) {
  return text
    .replaceAll("&quot;", "\"")
    .replaceAll("&apos;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

function centerOfBounds(bounds) {
  const match = /^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$/.exec(bounds ?? "");
  if (!match) throw new Error(`invalid Android bounds: ${bounds}`);
  const [, x1, y1, x2, y2] = match.map(Number);
  return { x: Math.round((x1 + x2) / 2), y: Math.round((y1 + y2) / 2) };
}

function rectOfBounds(bounds) {
  const match = /^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$/.exec(bounds ?? "");
  if (!match) throw new Error(`invalid Android bounds: ${bounds}`);
  const [, left, top, right, bottom] = match.map(Number);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

function findNodes(xml, predicate) {
  const nodes = [];
  const nodeRegex = /<node\b[^>]*>/g;
  let match;
  while ((match = nodeRegex.exec(xml))) {
    const attrs = Object.fromEntries([...match[0].matchAll(/([a-zA-Z0-9_-]+)="([^"]*)"/g)].map((entry) => [entry[1], decodeXml(entry[2])]));
    if (predicate(attrs)) nodes.push(attrs);
  }
  return nodes;
}

function androidAssertFeedbackHasBottomControlClearance(serial, out, feedbackNode) {
  const feedback = rectOfBounds(feedbackNode.bounds);
  const xml = dumpAndroid(serial);
  const bottomControlTags = new Set([
    "parity:button:Next Leg",
    "parity:button:Sequence",
    "parity:button:Suspend",
    "parity:button:Unsusp",
    "parity:nav-cdi",
    "parity:button:DBG",
  ]);
  const controls = findNodes(xml, (node) => bottomControlTags.has(androidTag(node)));
  if (controls.length === 0) {
    recordGap(out, "append route feedback bottom-control clearance", "no bottom controls found while checking IME layout");
    return;
  }
  const nearestTop = Math.min(...controls.map((node) => rectOfBounds(node.bounds).top).filter((top) => top >= feedback.bottom));
  const clearancePx = nearestTop - feedback.bottom;
  const minimumClearancePx = 64;
  if (!Number.isFinite(clearancePx) || clearancePx < minimumClearancePx) {
    recordGap(
      out,
      "append route feedback bottom-control clearance",
      `feedback bottom ${feedback.bottom}px is only ${Number.isFinite(clearancePx) ? clearancePx : "no"}px above bottom controls; expected >= ${minimumClearancePx}px`,
    );
    return;
  }
  recordStep(out, "append route feedback bottom-control clearance", "ok", `${clearancePx}px`);
}

function androidImeTop(serial) {
  const dump = adb(serial, ["shell", "dumpsys", "window", "windows"]);
  const inputMethodStart = dump.search(/Window #\d+ Window\{[^}]+ InputMethod\}/);
  if (inputMethodStart < 0) return null;
  const inputMethodBlock = dump.slice(inputMethodStart, inputMethodStart + 5000);
  const regionMatch = /touchable region=SkRegion\(\((\d+),(\d+),(\d+),(\d+)\)\)/.exec(inputMethodBlock);
  if (regionMatch) {
    return Number(regionMatch[2]);
  }
  const insetMatch = /mGivenVisibleInsets=\[0,(\d+)\]\[0,0\]/.exec(inputMethodBlock);
  if (insetMatch) {
    return Number(insetMatch[1]) + 128;
  }
  return null;
}

function androidAssertNodeAboveIme(serial, out, label, node, minimumClearancePx = 24) {
  const imeTop = androidImeTop(serial);
  if (imeTop === null) {
    recordGap(out, label, "IME top could not be determined from dumpsys window");
    return;
  }
  const rect = rectOfBounds(node.bounds);
  const clearancePx = imeTop - rect.bottom;
  if (clearancePx < minimumClearancePx) {
    recordGap(
      out,
      label,
      `node bottom ${rect.bottom}px is ${clearancePx}px above IME top ${imeTop}px; expected >= ${minimumClearancePx}px`,
    );
    return;
  }
  recordStep(out, label, "ok", `${clearancePx}px`);
}

function androidTypeRouteTokens(serial, tokens) {
  for (const token of tokens) {
    adb(serial, ["shell", "input", "text", token]);
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_SPACE"]);
  }
}

function androidDeleteChars(serial, count) {
  for (let i = 0; i < count; i += 1) {
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_DEL"]);
  }
}

async function androidScrollUntilTag(serial, tag, maxSwipes = 8) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (findNode(xml, (node) => hasAndroidTag(node, tag))) {
      return true;
    }
    const scrollSurface =
      findNode(xml, (node) => hasAndroidTag(node, "parity:plan-list")) ??
      findNode(xml, (node) => node.scrollable === "true" && node.package === ANDROID_PACKAGE);
    const bounds = rectOfBounds(scrollSurface?.bounds ?? "[90,383][1065,2021]");
    const x = Math.round((bounds.left + bounds.right) / 2);
    const startY = Math.round(bounds.bottom - Math.min(80, bounds.height / 5));
    const endY = Math.round(bounds.top + Math.min(80, bounds.height / 5));
    adb(serial, ["shell", "input", "swipe", String(x), String(startY), String(x), String(endY), "450"]);
    await delay(250);
  }
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

async function androidScrollUntilText(serial, text, maxSwipes = 8) {
  for (let attempt = 0; attempt < maxSwipes; attempt += 1) {
    const xml = dumpAndroid(serial);
    if (hasAndroidText(xml, text)) {
      return true;
    }
    const scrollSurface =
      findNode(xml, (node) => hasAndroidTag(node, "parity:plan-list")) ??
      findNode(xml, (node) => node.scrollable === "true" && node.package === ANDROID_PACKAGE);
    const bounds = rectOfBounds(scrollSurface?.bounds ?? "[90,383][1065,2021]");
    const x = Math.round((bounds.left + bounds.right) / 2);
    const startY = Math.round(bounds.top + Math.min(80, bounds.height / 5));
    const endY = Math.round(bounds.bottom - Math.min(80, bounds.height / 5));
    adb(serial, ["shell", "input", "swipe", String(x), String(startY), String(x), String(endY), "450"]);
    await delay(250);
  }
  return hasAndroidText(dumpAndroid(serial), text);
}

function androidTapNode(serial, out, label, predicate) {
  const xml = dumpAndroid(serial);
  const node = findNode(xml, predicate);
  if (!node) {
    recordGap(out, label, "control not found in UIAutomator dump");
    return false;
  }
  const { x, y } = centerOfBounds(node.bounds);
  adb(serial, ["shell", "input", "tap", String(x), String(y)]);
  recordStep(out, label);
  return true;
}

function androidTapResolvedNode(serial, out, label, node) {
  const { x, y } = centerOfBounds(node.bounds);
  adb(serial, ["shell", "input", "tap", String(x), String(y)]);
  recordStep(out, label);
}

async function androidWaitForNode(serial, predicate, timeoutMs, message) {
  let found = null;
  await waitFor(async () => {
    found = findNode(dumpAndroid(serial), predicate);
    return found !== null;
  }, timeoutMs, message);
  return found;
}

async function androidTapTag(serial, out, label, tag, timeoutMs = 5000) {
  let node;
  try {
    node = await androidWaitForNode(serial, (candidate) => hasAndroidTag(candidate, tag), timeoutMs, label);
  } catch (_error) {
    recordGap(out, label, "control not found in UIAutomator dump");
    return false;
  }
  const { x, y } = centerOfBounds(node.bounds);
  adb(serial, ["shell", "input", "tap", String(x), String(y)]);
  recordStep(out, label);
  return true;
}

async function androidWaitForFocusedTag(serial, out, label, tag, timeoutMs = 5000) {
  try {
    const node = await androidWaitForNode(
      serial,
      (candidate) => hasAndroidTag(candidate, tag) && candidate.focused === "true",
      timeoutMs,
      label,
    );
    recordStep(out, label);
    return node;
  } catch (_error) {
    recordGap(out, label, "control did not become focused in UIAutomator dump");
    return null;
  }
}

async function androidJourney(serial) {
  const out = result("android");
  adb(serial, ["wait-for-device"]);
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
  adb(serial, ["shell", "am", "start", "-n", ANDROID_ACTIVITY]);
  await androidWaitForNode(
    serial,
    (node) => node.package === ANDROID_PACKAGE,
    12000,
    "Aerobag app visible",
  );
  const xml = dumpAndroid(serial);
  if (!xml.includes(`package="${ANDROID_PACKAGE}"`) && !xml.includes("parity:")) {
    recordGap(out, "app visible", "Aerobag app is not visible; install it before running this journey");
    out.status = "gaps";
    return out;
  }
  recordStep(out, "app visible");

  if (hasAndroidText(xml, "Waypoint")) {
    recordStep(out, "opened plan page", "already on plan page");
  } else {
    await androidTapTag(serial, out, "opened plan page", "parity:nav-cdi", 12000);
  }
  await delay(500);

  if (await androidTapTag(serial, out, "plan CDI returned to chart", "parity:nav-cdi", 12000)) {
    try {
      await androidWaitForNode(serial, (node) => hasAndroidTag(node, "parity:map-surface"), 7000, "chart after plan CDI");
      recordStep(out, "chart visible after plan CDI");
    } catch (_error) {
      recordGap(out, "chart visible after plan CDI", "map surface was not visible after tapping CDI from PLAN");
    }
    if (await androidTapTag(serial, out, "chart CDI returned to plan", "parity:nav-cdi", 7000)) {
      try {
        await androidWaitForNode(
          serial,
          (node) => hasAndroidTag(node, "parity:plan-append-route-input") || node.text === "Waypoint",
          7000,
          "plan after chart CDI",
        );
        recordStep(out, "plan visible after chart CDI");
      } catch (_error) {
        recordGap(out, "plan visible after chart CDI", "plan page was not visible after tapping CDI from CHART");
      }
    }
  }

  const planXml = dumpAndroid(serial);
  if (planXml.includes("Append route") || planXml.includes("parity:plan-append-route-input")) {
    recordStep(out, "free-form append route present");
    if (await androidTapTag(serial, out, "focused free-form append route", "parity:plan-append-route-input")) {
      androidTypeRouteTokens(serial, ["KRNT", "V2", "ZZZZZ"]);
      try {
        const feedback = await androidWaitForNode(
          serial,
          (node) => hasAndroidTag(node, "parity:plan-append-route-feedback") && (node.text ?? "").trim() !== "",
          7000,
          "append route feedback visible",
        );
        recordStep(out, "append route feedback visible", "ok", feedback.text);
        androidAssertFeedbackHasBottomControlClearance(serial, out, feedback);
        androidAssertNodeAboveIme(serial, out, "append route feedback above IME", feedback);
      } catch (_error) {
        recordGap(out, "append route feedback visible", "no non-empty feedback appeared after typing KRNT V2 ZZZZZ");
      }
      androidDeleteChars(serial, 24);
      await delay(300);
      adb(serial, ["shell", "input", "text", "KBFI"]);
      await delay(300);
      adb(serial, ["shell", "input", "keyevent", "ENTER"]);
      await delay(1200);
      const appendedXml = dumpAndroid(serial);
      if (hasAndroidText(appendedXml, "KBFI")) {
        recordStep(out, "appended KBFI to flight plan");
        if (await androidTapTag(serial, out, "focused append route for long plan", "parity:plan-append-route-input")) {
          androidTypeRouteTokens(serial, ["KRNT", "SEA", "KPAE", "KBFI", "KRNT", "SEA", "KPAE", "KBFI", "KRNT", "SEA"]);
          await delay(300);
          adb(serial, ["shell", "input", "keyevent", "ENTER"]);
          await delay(1200);
          recordStep(out, "expanded flight plan for long-route feedback");
          if (await androidScrollUntilTag(serial, "parity:plan-append-route-input")) {
            if (await androidTapTag(serial, out, "focused long-plan append route", "parity:plan-append-route-input")) {
              const longInput = await androidWaitForFocusedTag(serial, out, "long-plan append route input focused", "parity:plan-append-route-input");
              if (longInput) {
                androidAssertNodeAboveIme(serial, out, "long-plan append route input above IME", longInput);
              }
              await delay(700);
              androidTypeRouteTokens(serial, ["ZZZZZ"]);
              try {
                const longFeedback = await androidWaitForNode(
                  serial,
                  (node) => hasAndroidTag(node, "parity:plan-append-route-feedback") && (node.text ?? "").trim() !== "",
                  7000,
                  "long-plan append route feedback",
                );
                recordStep(out, "long-plan append route feedback visible", "ok", longFeedback.text);
                androidAssertNodeAboveIme(serial, out, "long-plan append route feedback above IME", longFeedback);
                androidDeleteChars(serial, 8);
              } catch (_error) {
                recordGap(out, "long-plan append route feedback visible", "no non-empty feedback appeared after extending the plan");
              }
            }
          } else {
            recordGap(out, "focused long-plan append route", "append route field was not reachable after extending the plan");
          }
        }
      } else {
        recordGap(out, "appended KBFI to flight plan", "KBFI was not visible after submitting the append route field");
      }
    }
  } else {
    recordGap(out, "free-form append route present", "Android currently has no parity-tagged free-form flight-plan entry field");
  }

  await androidTapTag(serial, out, "opened home page", "parity:button:HOME");
  await delay(500);
  await androidTapTag(serial, out, "returned chart page", "parity:button:CHART");
  await androidTapTag(serial, out, "drag/click map surface", "parity:map-surface");
  await delay(500);

  const selectionXml = dumpAndroid(serial);
  if (findNode(selectionXml, (node) => hasAndroidTag(node, "parity:map-selection-tray"))) {
    recordStep(out, "map inspection tray appeared");
  } else {
    recordGap(out, "map inspection tray appeared", "tap did not open an inspect tray at the tapped map location");
  }

  let selectedLabel = "";
  try {
    const firstInspectableItem = await androidWaitForNode(
      serial,
      (node) => androidTag(node).startsWith("parity:map-selection-item:"),
      5000,
      "first inspect item",
    );
    const tag = androidTag(firstInspectableItem);
    selectedLabel = tag.slice("parity:map-selection-item:".length);
    androidTapResolvedNode(serial, out, "selected first inspect item", firstInspectableItem);
    await delay(500);
  } catch (_error) {
    recordGap(out, "selected first inspect item", "no inspect item was visible");
  }

  const selectedXml = dumpAndroid(serial);
  const insertAction = findNode(selectedXml, (node) => hasAndroidTag(node, "parity:map-selection-action:insert"));
  if (insertAction) {
    androidTapResolvedNode(serial, out, "inspect insert action present", insertAction);
    await delay(500);
    if (selectedLabel) {
      await androidTapTag(serial, out, "opened plan page after insert", "parity:nav-cdi");
      await delay(500);
      if (hasAndroidText(dumpAndroid(serial), selectedLabel) || await androidScrollUntilText(serial, selectedLabel)) {
        recordStep(out, "verified inspected item in flight plan", selectedLabel);
      } else {
        recordGap(out, "verified inspected item in flight plan", `${selectedLabel} was not visible in the plan after insert`);
      }
    }
  } else {
    recordGap(out, "inspect insert action present", "no insert action was visible in the current inspect tray");
  }

  out.finished_at = new Date().toISOString();
  out.status = out.gaps.length === 0 ? "pass" : "gaps";
  return out;
}

async function main() {
  const args = parseArgs(process.argv);
  if (args.help || !["web", "android", "both"].includes(args.platform)) {
    usage();
    process.exit(args.help ? 0 : 2);
  }
  const outputs = [];
  if (args.platform === "web" || args.platform === "both") {
    outputs.push(await webJourney(args.url));
  }
  if (args.platform === "android" || args.platform === "both") {
    outputs.push(await androidJourney(args.serial));
  }
  console.log(JSON.stringify(outputs.length === 1 ? outputs[0] : outputs, null, 2));
  if (outputs.some((entry) => entry.status !== "pass")) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
