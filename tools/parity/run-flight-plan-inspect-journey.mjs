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

function usage() {
  console.log(`Usage:
  node tools/parity/run-flight-plan-inspect-journey.mjs web [--url http://127.0.0.1:8082/]
  node tools/parity/run-flight-plan-inspect-journey.mjs android [--serial emulator-5554]
  node tools/parity/run-flight-plan-inspect-journey.mjs both [--url http://127.0.0.1:8082/] [--serial emulator-5554]

The web runner launches a temporary headless Chrome through CDP. Set CHROME_BIN if Chrome is not on PATH.
The Android runner expects the app to already be installed/launched and uses adb + uiautomator XML dumps.`);
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

async function androidJourney(serial) {
  const out = result("android");
  adb(serial, ["wait-for-device"]);
  const xml = dumpAndroid(serial);
  if (!xml.includes("Aerobag") && !xml.includes("parity:")) {
    recordGap(out, "app visible", "Aerobag app is not visible; install/launch it before running this journey");
    out.status = "gaps";
    return out;
  }
  recordStep(out, "app visible");

  androidTapNode(serial, out, "opened plan page", (node) => node["content-desc"] === "parity:nav-cdi");
  await delay(500);

  const planXml = dumpAndroid(serial);
  if (planXml.includes("Append route") || planXml.includes("parity:plan-append-route-input")) {
    recordStep(out, "free-form append route present");
  } else {
    recordGap(out, "free-form append route present", "Android currently has no parity-tagged free-form flight-plan entry field");
  }

  androidTapNode(serial, out, "returned chart page", (node) => node["content-desc"] === "parity:nav-cdi");
  await delay(500);
  androidTapNode(serial, out, "drag/click map surface", (node) => node["content-desc"] === "parity:map-surface");
  await delay(500);

  const selectionXml = dumpAndroid(serial);
  if (selectionXml.includes("parity:map-selection-tray")) {
    recordStep(out, "map inspection tray appeared");
  } else {
    recordGap(out, "map inspection tray appeared", "tap did not open an inspect tray at the tapped map location");
  }
  if (selectionXml.includes("parity:map-selection-action:insert")) {
    recordStep(out, "inspect insert action present");
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
