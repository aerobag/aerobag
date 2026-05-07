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
const LAYER_OPTION_IDS = ["vectors", "metars", "nexrad", "terrain_warning"];
const PLAN_CONTROL_IDS = ["next-leg", "sequence", "suspend", "unsuspend"];
const CORE_ROW_ACTION_IDS = ["activate_leg", "direct_to", "insert_before", "insert_after", "move_up", "move_down"];
const WEB_PARITY_VIEWPORT = Object.freeze({
  width: 360,
  height: 736,
  deviceScaleFactor: 3,
  mobile: true,
});
const INSPECT_AIRPORT = Object.freeze({
  ident: "KBFI",
  label: "BFI",
  navRefId: "airports:KBFI",
});

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
    checks: {},
    inventories: {},
  };
}

function recordStep(out, name, status = "ok", detail = undefined) {
  out.steps.push({ name, status, ...(detail === undefined ? {} : { detail }) });
}

function recordCheck(out, name, value) {
  out.checks[name] = value;
}

function normalizeInventoryEntry(entry) {
  return {
    id: String(entry.id ?? ""),
    label: String(entry.label ?? "").replace(/\s+/g, " ").trim(),
    enabled: Boolean(entry.enabled),
    active: Boolean(entry.active),
    selected: Boolean(entry.selected),
    on: Boolean(entry.on),
    off: Boolean(entry.off),
    disabled: Boolean(entry.disabled),
  };
}

function normalizeInventory(entries) {
  return entries
    .map(normalizeInventoryEntry)
    .sort((left, right) => left.id.localeCompare(right.id) || left.label.localeCompare(right.label));
}

function recordInventory(out, name, entries) {
  const normalized = normalizeInventory(entries);
  out.inventories[name] = normalized;
  recordStep(out, `inventory: ${name}`, "ok", `${normalized.length} entries`);
  return normalized;
}

function recordGap(out, name, detail) {
  out.gaps.push({ name, detail });
  recordStep(out, name, "gap", detail);
}

function recordActionClass(out, name, value, detail = undefined) {
  recordCheck(out, `actionClass.${name}`, value);
  if (value) recordStep(out, `action class: ${name}`, "ok", detail);
  else recordGap(out, `action class: ${name}`, detail ?? "not reachable");
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
    "--disable-extensions",
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
    const tab = tabs.find((entry) => entry.type === "page" && entry.webSocketDebuggerUrl);
    if (!tab) {
      throw new Error(`Chrome did not expose a debuggable page target: ${JSON.stringify(tabs)}`);
    }
    cdp = await CdpSocket.connect(tab.webSocketDebuggerUrl);
    await cdp.send("Page.enable");
    await cdp.send("Network.enable");
    await cdp.send("Runtime.enable");
    await cdp.send("Console.enable");
    await cdp.send("Emulation.setDeviceMetricsOverride", WEB_PARITY_VIEWPORT);
    await cdp.send("Page.navigate", { url });
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "map surface");
    recordStep(out, "app started");

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]') !== null", "plan append input");
    recordStep(out, "opened plan page");
    recordCheck(out, "openedPlanFromCdi", true);

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "chart after plan CDI");
    recordStep(out, "plan CDI returned to chart");
    recordCheck(out, "planCdiReturnsToChart", true);

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]') !== null", "plan after chart CDI");
    recordStep(out, "chart CDI returned to plan");
    recordCheck(out, "chartCdiReturnsToPlan", true);
    recordCheck(out, "appendRoutePresent", true);
    await webCheckPlanActionClasses(cdp, out);

    await webSetInput(cdp, "[data-testid=\"plan-append-route-input\"]", "KRNT V2 ZZZZZ ");
    await waitForWeb(
      cdp,
      "document.body.innerText.includes('unknown route element ZZZZZ')",
      "append route feedback",
    );
    recordStep(out, "append route feedback visible");
    recordCheck(out, "appendRouteFeedbackVisible", true);

    await webSetInput(cdp, "[data-testid=\"plan-append-route-input\"]", "KAWO");
    await waitForWeb(cdp, "document.querySelector('.planEntryInputShell.isReady') !== null", "append route ready");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"plan-append-route-input\"]')?.closest('form') !== null", "plan append form");
    await cdp.send("Runtime.evaluate", {
      expression: "document.querySelector('[data-testid=\"plan-append-route-input\"]').closest('form').requestSubmit()",
      awaitPromise: true,
    });
    await waitForWeb(cdp, "document.body.innerText.includes('KAWO')", "appended KAWO");
    await delay(1000);
    recordStep(out, "appended KAWO to flight plan");
    recordCheck(out, "appendRouteCommitsKAWO", true);

    await webClick(cdp, "[data-testid=\"nav-cdi\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "returned chart");
    recordStep(out, "returned to chart");
    await webCheckChartActionClasses(cdp, out);
    await webCheckPlateActionClasses(cdp, out);

    await webDrag(cdp, "[data-testid=\"map-surface\"]", 40, 20);
    recordStep(out, "dragged map surface");

    await webSetInput(cdp, "[data-testid=\"chart-search-input\"]", INSPECT_AIRPORT.ident);
    await waitForWeb(cdp, `document.querySelector('[data-testid="chart-search-suggestion-${INSPECT_AIRPORT.ident}"]') !== null`, `${INSPECT_AIRPORT.ident} search suggestion`);
    await webClick(cdp, `[data-testid="chart-search-suggestion-${INSPECT_AIRPORT.ident}"]`);
    await delay(500);
    recordStep(out, `recentered on ${INSPECT_AIRPORT.ident} via chart search`);
    recordCheck(out, "chartSearchRecentersInspectAirport", true);

    await webClick(cdp, "[data-testid=\"map-surface\"]");
    await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-selection-tray\"]') !== null", "map selection tray");
    recordStep(out, "opened map inspection tray");
    recordCheck(out, "inspectTrayAppears", true);
    recordInventory(out, "chart.inspect.items", await webCollectTestIds(cdp, "map-selection-item-"));

    const selectedInspectLabel = await webEval(cdp, `
      (() => {
        const items = [...document.querySelectorAll('[data-testid^="map-selection-item-airport-"]')];
        const item = items.find((entry) => entry.dataset.testid === ${JSON.stringify(`map-selection-item-airport-${INSPECT_AIRPORT.ident}`)});
        item?.click();
        return (item?.textContent ?? "").replace(/\\s+/g, " ").trim();
      })()
    `);
    const inspectItemSelected = selectedInspectLabel.length > 0;
    recordCheck(out, "inspectItemSelected", inspectItemSelected);
    if (!inspectItemSelected) {
      recordGap(out, "selected inspected airport", `${INSPECT_AIRPORT.ident} airport was not visible in the inspect tray`);
    }
    recordInventory(out, "chart.inspect.selected-actions", await webCollectTestIds(cdp, "map-selection-action-"));
    const insertAvailable = await webExists(cdp, "[data-testid=\"map-selection-action-insert\"]:not(:disabled)");
    recordCheck(out, "inspectInsertActionPresent", insertAvailable);
    if (insertAvailable) {
      await webClick(cdp, "[data-testid=\"map-selection-action-insert\"]");
      recordStep(out, "inserted inspected airport into flight plan");

      await webClick(cdp, "[data-testid=\"nav-cdi\"]");
      await waitForWeb(cdp, `document.body.innerText.includes(${JSON.stringify(selectedInspectLabel)})`, "inspected airport visible in plan");
      recordStep(out, "verified inspected airport in flight plan");
      recordCheck(out, "inspectInsertAddsSelectedItem", true);
    } else {
      recordGap(out, "inspect insert action present", "selected inspected airport was already in the flight plan, so core did not expose Insert");
      recordCheck(out, "inspectInsertAddsSelectedItem", false);
    }
    out.finished_at = new Date().toISOString();
    out.status = out.gaps.length === 0 ? "pass" : "gaps";
    return out;
  } finally {
    cdp?.close();
    if (chrome.proc.exitCode === null && chrome.proc.signalCode === null) {
      chrome.proc.kill("SIGTERM");
    }
    await waitForProcessExit(chrome.proc);
    await removeChromeProfile(chrome.profile);
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
  try {
    await waitFor(() => webEval(cdp, `Boolean(${expression})`), 30000, `Timed out waiting for ${label}`);
  } catch (error) {
    const diagnostics = await webEval(cdp, `
      (() => ({
        href: location.href,
        title: document.title,
        bodyText: document.body?.innerText?.slice(0, 500) ?? "",
        rootHtml: document.getElementById("root")?.innerHTML?.slice(0, 500) ?? "",
        startupHidden: document.getElementById("startup-shell")?.className ?? null,
        parityLastVectorClick: window.__parityLastVectorClick ?? null,
        resources: performance.getEntriesByType("resource").slice(-20).map((entry) => ({
          name: entry.name,
          transferSize: entry.transferSize,
          encodedBodySize: entry.encodedBodySize,
        })),
        testIds: [...document.querySelectorAll("[data-testid]")].slice(0, 40).map((el) => el.getAttribute("data-testid")),
      }))()
    `).catch((diagnosticError) => ({ diagnosticError: diagnosticError.message }));
    const events = cdp.events
      .filter((event) => [
        "Runtime.exceptionThrown",
        "Runtime.consoleAPICalled",
        "Console.messageAdded",
        "Log.entryAdded",
        "Network.loadingFailed",
        "Network.responseReceived",
      ].includes(event.method))
      .slice(-20)
      .map((event) => event.params);
    throw new Error(`${error.message}; diagnostics=${JSON.stringify(diagnostics)}; events=${JSON.stringify(events)}`);
  }
}

async function webClick(cdp, selector) {
  const box = await webElementBox(cdp, selector);
  await dispatchClick(cdp, box.x + box.width / 2, box.y + box.height / 2);
}

async function webClickVectorFeature(cdp, target) {
  const point = await webEval(cdp, `
    (() => {
      const target = ${JSON.stringify(target)};
      const features = [...document.querySelectorAll('[data-testid^="parity:map-feature:"]')];
      const feature = features.find((entry) => {
        const tag = entry.getAttribute("data-testid") ?? "";
        const parts = tag.split(":");
        const kind = parts[2] ?? "";
        const label = parts[3] ?? "";
        const id = parts.slice(4).join(":");
        return kind.toLowerCase() === target.kind.toLowerCase() &&
          (!target.idContains || id.includes(target.idContains)) &&
          (!target.label || label === target.label || id.includes(target.label));
      });
      if (!feature) return null;
      const rect = feature.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return null;
      const point = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      window.__parityLastVectorClick = {
        target,
        tag: feature.getAttribute("data-testid"),
        x: point.x,
        y: point.y,
        hit: document.elementFromPoint(point.x, point.y)?.className?.toString?.() ?? document.elementFromPoint(point.x, point.y)?.tagName ?? null,
      };
      return { x: point.x, y: point.y };
    })()
  `);
  if (!point) {
    const labels = await webEval(cdp, `
      [...document.querySelectorAll('[data-testid^="parity:map-feature:"]')]
        .map((entry) => entry.getAttribute("data-testid") ?? "")
        .filter(Boolean)
        .slice(0, 80)
    `).catch(() => []);
    throw new Error(`missing vector feature ${JSON.stringify(target)}; visible features=${JSON.stringify(labels)}`);
  }
  await dispatchClick(cdp, point.x, point.y);
}

async function webDispatchPointerClick(cdp, selector, x, y) {
  await cdp.send("Runtime.evaluate", {
    expression: `
      (() => {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error('missing pointer target ${selector}');
        const eventInit = {
          bubbles: true,
          cancelable: true,
          composed: true,
          clientX: ${JSON.stringify(x)},
          clientY: ${JSON.stringify(y)},
          pointerId: 1,
          pointerType: 'mouse',
          isPrimary: true,
          button: 0,
          buttons: 1,
        };
        el.dispatchEvent(new PointerEvent('pointerdown', eventInit));
        el.dispatchEvent(new PointerEvent('pointerup', { ...eventInit, buttons: 0 }));
        el.dispatchEvent(new MouseEvent('click', eventInit));
      })()
    `,
    awaitPromise: true,
  });
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

async function webExists(cdp, selector) {
  return webEval(cdp, `document.querySelector(${JSON.stringify(selector)}) !== null`);
}

async function webCollectTestIds(cdp, prefix) {
  return webEval(cdp, `
    (() => {
      const prefix = ${JSON.stringify(prefix)};
      return [...document.querySelectorAll('[data-testid]')]
        .map((el) => {
          const testId = el.getAttribute("data-testid") ?? "";
          if (!testId.startsWith(prefix)) return null;
          const disabled = Boolean(el.disabled) || el.getAttribute("aria-disabled") === "true" || el.classList.contains("isDisabled");
          return {
            id: testId.slice(prefix.length),
            label: (el.innerText ?? el.textContent ?? "").replace(/\\s+/g, " ").trim(),
            enabled: !disabled,
            disabled,
            active: el.classList.contains("isActive") || el.classList.contains("isOpen"),
            selected: el.classList.contains("isSelected"),
            on: el.classList.contains("isOn"),
            off: el.classList.contains("isOff"),
          };
        })
        .filter(Boolean);
    })()
  `);
}

async function webRecordTrayInventory(cdp, out, name, launcherSelector, optionPrefix = "tray-option-") {
  await webClick(cdp, launcherSelector);
  await delay(250);
  const entries = await webCollectTestIds(cdp, optionPrefix);
  recordInventory(out, name, entries);
  await webClick(cdp, launcherSelector);
  return entries;
}

async function webRecordPlanRowInventory(cdp, out, name, rowIndex) {
  const rowSelector = await webEval(cdp, `
    (() => {
      const rows = [...document.querySelectorAll('[data-testid^="plan-row-"]')];
      const row = rows[${rowIndex} < 0 ? rows.length + ${rowIndex} : ${rowIndex}];
      return row ? \`[data-testid="\${row.getAttribute("data-testid")}"]\` : null;
    })()
  `);
  if (!rowSelector) {
    recordGap(out, `inventory: ${name}`, `no flight-plan row at index ${rowIndex}`);
    return [];
  }
  await webClick(cdp, rowSelector);
  await waitForWeb(cdp, "document.querySelector('[data-testid^=\"plan-row-action-\"]') !== null", `${name} action tray`);
  const entries = await webCollectTestIds(cdp, "plan-row-action-");
  recordInventory(out, name, entries);
  await cdp.send("Runtime.evaluate", {
    expression: `document.querySelector('[aria-label="Close waypoint actions"]')?.click()`,
    awaitPromise: true,
  });
  return entries;
}

async function webCheckChartActionClasses(cdp, out) {
  recordActionClass(out, "chart.drag-pan", await webExists(cdp, "[data-testid=\"map-surface\"]"));
  recordActionClass(out, "chart.search", await webExists(cdp, "[data-testid=\"chart-search-input\"]"));

  await webClick(cdp, "[data-testid=\"layers-button\"]");
  await waitForWeb(cdp, "document.querySelector('[data-testid=\"tray-option-vectors\"]') !== null", "layers tray");
  recordInventory(out, "chart.layers", await webCollectTestIds(cdp, "tray-option-"));
  const layerOptions = await webEval(cdp, `
    ${JSON.stringify(LAYER_OPTION_IDS)}.filter((id) => document.querySelector(\`[data-testid="tray-option-\${id}"]\`) !== null)
  `);
  recordActionClass(out, "chart.layers", layerOptions.length === LAYER_OPTION_IDS.length, layerOptions.join(","));
  await webClick(cdp, "[data-testid=\"layers-button\"]");

  await webClick(cdp, "[data-testid=\"chart-family-button\"]");
  await waitForWeb(cdp, "document.querySelector('[data-testid^=\"tray-option-\"]') !== null", "chart family tray");
  recordInventory(out, "chart.map-family", await webCollectTestIds(cdp, "tray-option-"));
  const familyCount = await webEval(cdp, `document.querySelectorAll('[data-testid^="tray-option-"]').length`);
  recordActionClass(out, "chart.map-family", familyCount > 0, `${familyCount} options`);
  await webClick(cdp, "[data-testid=\"chart-family-button\"]");
}

async function webCheckPlateActionClasses(cdp, out) {
  await webClick(cdp, "[data-testid=\"page-button-plate\"]");
  await waitForWeb(cdp, "document.querySelector('[data-testid=\"plate-airport-button\"]') !== null", "plate page controls");
  recordInventory(out, "plate.controls", await webCollectTestIds(cdp, "plate-"));
  recordActionClass(out, "plate.airport-selector", await webExists(cdp, "[data-testid=\"plate-airport-button\"]"));
  recordActionClass(out, "plate.chart-selector", await webExists(cdp, "[data-testid=\"plate-chart-button\"]"));
  recordActionClass(out, "plate.load-procedure", await webExists(cdp, "[data-testid=\"plate-load-button\"]"));
  recordActionClass(out, "plate.folder", await webExists(cdp, "[data-testid=\"plate-folder-button\"]"));
  await webRecordTrayInventory(cdp, out, "plate.airports", "[data-testid=\"plate-airport-button\"]");
  await webRecordTrayInventory(cdp, out, "plate.charts", "[data-testid=\"plate-chart-button\"]");
  await webRecordTrayInventory(cdp, out, "plate.loads", "[data-testid=\"plate-load-button\"]");
  await webClick(cdp, "[data-testid=\"page-button-chart\"]");
  await waitForWeb(cdp, "document.querySelector('[data-testid=\"map-surface\"]') !== null", "return chart from plate");
}

async function webCheckPlanActionClasses(cdp, out) {
  for (const id of PLAN_CONTROL_IDS) {
    recordActionClass(out, `plan.global.${id}`, await webExists(cdp, `[data-testid="plan-control-${id}"]`));
  }
  const firstRowSelector = await webEval(cdp, `
    (() => {
      const row = [...document.querySelectorAll('[data-testid^="plan-row-"]')][0];
      return row ? \`[data-testid="\${row.getAttribute("data-testid")}"]\` : null;
    })()
  `);
  if (!firstRowSelector) {
    recordActionClass(out, "plan.row-actions", false, "no flight-plan row found");
    return;
  }
  await webClick(cdp, firstRowSelector);
  await waitForWeb(cdp, "document.querySelector('[data-testid^=\"plan-row-action-\"]') !== null", "plan row action tray");
  const rowActionEntries = await webCollectTestIds(cdp, "plan-row-action-");
  recordInventory(out, "plan.row.first.actions", rowActionEntries);
  const rowActions = rowActionEntries.map((entry) => entry.id);
  recordActionClass(out, "plan.row-actions", rowActions.length > 0, rowActions.join(","));
  for (const id of CORE_ROW_ACTION_IDS) {
    recordActionClass(out, `plan.row.${id}`, rowActions.includes(id), rowActions.join(","));
  }
  await cdp.send("Runtime.evaluate", {
    expression: `document.querySelector('[aria-label="Close waypoint actions"]')?.click()`,
    awaitPromise: true,
  });
  await webRecordPlanRowInventory(cdp, out, "plan.row.last.actions", -1);
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

async function waitForProcessExit(proc, timeoutMs = 3000) {
  if (proc.exitCode !== null || proc.signalCode !== null) return;
  await Promise.race([
    once(proc, "exit").catch(() => {}),
    delay(timeoutMs),
  ]);
}

async function removeChromeProfile(profile) {
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      fs.rmSync(profile, { recursive: true, force: true });
      return;
    } catch (error) {
      if (error?.code !== "ENOTEMPTY" && error?.code !== "EBUSY") throw error;
      await delay(100 * (attempt + 1));
    }
  }
  fs.rmSync(profile, { recursive: true, force: true });
}

function adbArgs(serial, args) {
  return serial ? ["-s", serial, ...args] : args;
}

function adb(serial, args, options = {}) {
  const res = spawnSync("adb", adbArgs(serial, args), { encoding: "utf8", timeout: 15000, ...options });
  if (res.status !== 0) {
    const detail = res.error?.message ?? (res.stderr || res.stdout);
    throw new Error(`adb ${args.join(" ")} failed: ${detail}`);
  }
  return res.stdout;
}

function adbBestEffort(serial, args, options = {}) {
  return spawnSync("adb", adbArgs(serial, args), { encoding: "utf8", timeout: 15000, ...options });
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
    .replaceAll("&#10;", "\n")
    .replaceAll("&#xA;", "\n")
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

function androidNodeLabel(xml, node) {
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

function androidCollectTags(xml, prefix) {
  return findNodes(xml, (node) => androidTag(node).startsWith(prefix))
    .map((node) => {
      const tag = androidTag(node);
      const selected = node.selected === "true" || node.checked === "true";
      const enabled = node.enabled !== "false";
      return {
        id: tag.slice(prefix.length),
        label: androidNodeLabel(xml, node),
        enabled,
        disabled: !enabled,
        active: selected,
        selected,
        on: false,
        off: false,
      };
    });
}

function androidRecordInventory(serial, out, name, prefix) {
  const entries = androidCollectTags(dumpAndroid(serial), prefix);
  recordInventory(out, name, entries);
  return entries;
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
  adb(serial, ["shell", "input", "keyevent", "KEYCODE_MOVE_END"]);
  for (let i = 0; i < count; i += 1) {
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_DEL"]);
  }
}

async function androidScrollUntilTag(serial, tag, maxSwipes = 8) {
  if (await androidScrollUntilTagInDirection(serial, tag, "down", maxSwipes)) {
    return true;
  }
  return androidScrollUntilTagInDirection(serial, tag, "up", maxSwipes);
}

async function androidScrollUntilTagInDirection(serial, tag, direction, maxSwipes) {
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
    const inset = Math.min(80, bounds.height / 5);
    const startY = direction === "down"
      ? Math.round(bounds.bottom - inset)
      : Math.round(bounds.top + inset);
    const endY = direction === "down"
      ? Math.round(bounds.top + inset)
      : Math.round(bounds.bottom - inset);
    adb(serial, ["shell", "input", "swipe", String(x), String(startY), String(x), String(endY), "450"]);
    await delay(250);
  }
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

async function androidScrollUntilText(serial, text, maxSwipes = 8) {
  if (await androidScrollUntilTextInDirection(serial, text, "down", maxSwipes)) {
    return true;
  }
  return androidScrollUntilTextInDirection(serial, text, "up", maxSwipes);
}

async function androidScrollUntilTextInDirection(serial, text, direction, maxSwipes) {
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
    const inset = Math.min(80, bounds.height / 5);
    const startY = direction === "down"
      ? Math.round(bounds.bottom - inset)
      : Math.round(bounds.top + inset);
    const endY = direction === "down"
      ? Math.round(bounds.top + inset)
      : Math.round(bounds.bottom - inset);
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

function androidTagExists(serial, tag) {
  return findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, tag)) !== null;
}

async function androidCheckChartActionClasses(serial, out) {
  recordActionClass(out, "chart.drag-pan", androidTagExists(serial, "parity:map-surface"));
  recordActionClass(out, "chart.search", androidTagExists(serial, "chart-search-input"));

  if (await androidTapTag(serial, out, "opened layers tray for action audit", "parity:layers-button", 5000)) {
    await delay(300);
    const layerXml = dumpAndroid(serial);
    const layerInventory = androidCollectTags(layerXml, "parity:tray-option:").map((entry) => (
      LAYER_OPTION_IDS.includes(entry.id)
        ? { ...entry, on: entry.selected, off: !entry.selected }
        : entry
    ));
    recordInventory(out, "chart.layers", layerInventory);
    const layerOptions = LAYER_OPTION_IDS.filter((id) => findNode(layerXml, (node) => hasAndroidTag(node, `parity:tray-option:${id}`)));
    recordActionClass(out, "chart.layers", layerOptions.length === LAYER_OPTION_IDS.length, layerOptions.join(","));
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
    recordStep(out, "closed layers tray after action audit");
    await delay(200);
  } else {
    recordActionClass(out, "chart.layers", false, "layers launcher not found");
  }

  if (await androidTapTag(serial, out, "opened map-family tray for action audit", "parity:chart-family-button", 5000)) {
    await delay(300);
    const familyXml = dumpAndroid(serial);
    recordInventory(out, "chart.map-family", androidCollectTags(familyXml, "parity:tray-option:"));
    const familyOptions = findNodes(familyXml, (node) => androidTag(node).startsWith("parity:tray-option:"));
    recordActionClass(out, "chart.map-family", familyOptions.length > 0, `${familyOptions.length} options`);
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
    recordStep(out, "closed map-family tray after action audit");
    await delay(200);
  } else {
    recordActionClass(out, "chart.map-family", false, "chart-family launcher not found");
  }
}

async function androidCheckPlateActionClasses(serial, out) {
  if (!await androidTapTag(serial, out, "opened plate page for action audit", "parity:button:CHART", 5000)) {
    recordActionClass(out, "plate.airport-selector", false, "page toggle not found");
    recordActionClass(out, "plate.chart-selector", false, "page toggle not found");
    recordActionClass(out, "plate.load-procedure", false, "page toggle not found");
    recordActionClass(out, "plate.folder", false, "page toggle not found");
    return;
  }
  await delay(500);
  recordInventory(out, "plate.controls", androidCollectTags(dumpAndroid(serial), "parity:plate-"));
  recordActionClass(out, "plate.airport-selector", androidTagExists(serial, "parity:plate-airport-button"));
  recordActionClass(out, "plate.chart-selector", androidTagExists(serial, "parity:plate-chart-button"));
  recordActionClass(out, "plate.load-procedure", androidTagExists(serial, "parity:plate-load-button"));
  recordActionClass(out, "plate.folder", androidTagExists(serial, "parity:plate-folder-button"));
  if (await androidTapTag(serial, out, "opened plate airport tray inventory", "parity:plate-airport-button", 5000)) {
    await delay(300);
    androidRecordInventory(serial, out, "plate.airports", "parity:tray-option:");
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
    await delay(200);
  }
  if (await androidTapTag(serial, out, "opened plate chart tray inventory", "parity:plate-chart-button", 5000)) {
    await delay(300);
    androidRecordInventory(serial, out, "plate.charts", "parity:tray-option:");
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
    await delay(200);
  }
  const loadButton = findNode(dumpAndroid(serial), (node) => hasAndroidTag(node, "parity:plate-load-button"));
  if (loadButton?.enabled === "true") {
    androidTapResolvedNode(serial, out, "opened plate load tray inventory", loadButton);
    await delay(300);
    androidRecordInventory(serial, out, "plate.loads", "parity:tray-option:");
    adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
    await delay(200);
  } else if (loadButton) {
    recordInventory(out, "plate.loads", []);
    recordStep(out, "skipped disabled plate load tray inventory");
  }
  if (androidTagExists(serial, "parity:map-surface")) {
    recordStep(out, "returned chart page after plate action audit", "already on chart");
  } else {
    await androidTapTag(serial, out, "returned chart page after plate action audit", "parity:button:PLATE", 5000);
  }
  await delay(500);
}

async function androidCheckPlanActionClasses(serial, out) {
  const planControls = new Map([
    ["next-leg", "parity:button:Next Leg"],
    ["sequence", "parity:button:Sequence"],
    ["suspend", "parity:button:Suspend"],
    ["unsuspend", "parity:button:Unsusp"],
  ]);
  for (const [id, tag] of planControls) {
    recordActionClass(out, `plan.global.${id}`, androidTagExists(serial, tag));
  }
  const row = findNode(dumpAndroid(serial), (node) => androidTag(node).startsWith("parity:plan-row:"));
  if (!row) {
    recordActionClass(out, "plan.row-actions", false, "no flight-plan row found");
    return;
  }
  androidTapResolvedNode(serial, out, "opened first plan row for action audit", row);
  await delay(300);
  const actionXml = dumpAndroid(serial);
  const rowActionEntries = androidCollectTags(actionXml, "parity:plan-row-action:");
  recordInventory(out, "plan.row.first.actions", rowActionEntries);
  const rowActions = rowActionEntries.map((entry) => entry.id);
  recordActionClass(out, "plan.row-actions", rowActions.length > 0, rowActions.join(","));
  for (const id of CORE_ROW_ACTION_IDS) {
    recordActionClass(out, `plan.row.${id}`, rowActions.includes(id), rowActions.join(","));
  }
  adb(serial, ["shell", "input", "tap", "1040", "1200"]);
  await delay(300);
  const rows = findNodes(dumpAndroid(serial), (node) => androidTag(node).startsWith("parity:plan-row:"));
  if (rows.length > 1) {
    androidTapResolvedNode(serial, out, "opened last plan row for action inventory", rows[rows.length - 1]);
    await delay(300);
    androidRecordInventory(serial, out, "plan.row.last.actions", "parity:plan-row-action:");
    adb(serial, ["shell", "input", "tap", "1040", "1200"]);
    await delay(300);
  } else {
    recordGap(out, "inventory: plan.row.last.actions", "no distinct last flight-plan row found");
  }
}

async function androidJourney(serial) {
  const out = result("android");
  adb(serial, ["wait-for-device"]);
  adb(serial, ["shell", "am", "force-stop", ANDROID_PACKAGE]);
  adbBestEffort(serial, ["shell", "run-as", ANDROID_PACKAGE, "rm", "shared_prefs/aerobag_ui.xml"]);
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
    recordCheck(out, "openedPlanFromCdi", true);
  } else {
    recordCheck(out, "openedPlanFromCdi", await androidTapTag(serial, out, "opened plan page", "parity:nav-cdi", 12000));
  }
  await delay(500);

  if (await androidTapTag(serial, out, "plan CDI returned to chart", "parity:nav-cdi", 12000)) {
    try {
      await androidWaitForNode(
        serial,
        (node) => hasAndroidTag(node, "parity:map-surface") || hasAndroidTag(node, "parity:plate-airport-button"),
        7000,
        "chart or plate after plan CDI",
      );
      recordStep(out, "chart or plate visible after plan CDI");
      recordCheck(out, "planCdiReturnsToChart", true);
    } catch (_error) {
      recordGap(out, "chart or plate visible after plan CDI", "neither chart nor plate surface was visible after tapping CDI from PLAN");
      recordCheck(out, "planCdiReturnsToChart", false);
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
        recordCheck(out, "chartCdiReturnsToPlan", true);
        await androidCheckPlanActionClasses(serial, out);
      } catch (_error) {
        recordGap(out, "plan visible after chart CDI", "plan page was not visible after tapping CDI from CHART");
        recordCheck(out, "chartCdiReturnsToPlan", false);
      }
    }
  }

  const planXml = dumpAndroid(serial);
  if (planXml.includes("Append route") || planXml.includes("parity:plan-append-route-input")) {
    recordStep(out, "free-form append route present");
    recordCheck(out, "appendRoutePresent", true);
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
        recordCheck(out, "appendRouteFeedbackVisible", true);
        androidAssertFeedbackHasBottomControlClearance(serial, out, feedback);
        androidAssertNodeAboveIme(serial, out, "append route feedback above IME", feedback);
      } catch (_error) {
        recordGap(out, "append route feedback visible", "no non-empty feedback appeared after typing KRNT V2 ZZZZZ");
        recordCheck(out, "appendRouteFeedbackVisible", false);
      }
      androidDeleteChars(serial, 80);
      await delay(300);
      adb(serial, ["shell", "input", "text", "KAWO"]);
      await delay(300);
      adb(serial, ["shell", "input", "keyevent", "KEYCODE_ENTER"]);
      await delay(1200);
      adb(serial, ["shell", "input", "keyevent", "KEYCODE_BACK"]);
      await delay(700);
      if (hasAndroidText(dumpAndroid(serial), "KAWO") || await androidScrollUntilText(serial, "KAWO")) {
        recordStep(out, "appended KAWO to flight plan");
        recordCheck(out, "appendRouteCommitsKAWO", true);
      } else {
        recordGap(out, "appended KAWO to flight plan", "KAWO was not visible after submitting the append route field");
        recordCheck(out, "appendRouteCommitsKAWO", false);
      }
    }
  } else {
    recordGap(out, "free-form append route present", "Android currently has no parity-tagged free-form flight-plan entry field");
    recordCheck(out, "appendRoutePresent", false);
  }

  await androidTapTag(serial, out, "opened home page", "parity:button:HOME");
  await delay(500);
  await androidTapTag(serial, out, "returned chart page", "parity:button:CHART");
  await androidCheckChartActionClasses(serial, out);
  await androidCheckPlateActionClasses(serial, out);

  if (await androidTapTag(serial, out, "focused chart search", "chart-search-input", 5000)) {
    adb(serial, ["shell", "input", "text", INSPECT_AIRPORT.ident]);
    try {
      await androidTapTag(serial, out, `selected ${INSPECT_AIRPORT.ident} chart search suggestion`, `chart-search-suggestion-${INSPECT_AIRPORT.ident}`, 12000);
      adb(serial, ["shell", "input", "keyevent", "ENTER"]);
      await delay(1000);
      recordStep(out, `recentered on ${INSPECT_AIRPORT.ident} via chart search`);
      recordCheck(out, "chartSearchRecentersInspectAirport", true);
    } catch (_error) {
      recordGap(out, `recentered on ${INSPECT_AIRPORT.ident} via chart search`, `${INSPECT_AIRPORT.ident} airport feature was not visible after submitting chart search`);
      recordCheck(out, "chartSearchRecentersInspectAirport", false);
    }
  } else {
    recordCheck(out, "chartSearchRecentersInspectAirport", false);
  }

  if (out.checks.chartSearchRecentersInspectAirport === true) {
    await androidTapTag(serial, out, "clicked recentered map surface", "parity:map-surface", 7000);
    await delay(500);
  } else {
    recordGap(out, "clicked recentered map surface", "skipped because chart search did not prove recenter");
  }

  const selectionXml = dumpAndroid(serial);
  if (findNode(selectionXml, (node) => hasAndroidTag(node, "parity:map-selection-tray"))) {
    recordStep(out, "map inspection tray appeared");
    recordCheck(out, "inspectTrayAppears", true);
    recordInventory(out, "chart.inspect.items", androidCollectTags(selectionXml, "parity:map-selection-item:"));
  } else {
    recordGap(out, "map inspection tray appeared", "tap did not open an inspect tray at the tapped map location");
    recordCheck(out, "inspectTrayAppears", false);
  }

  let selectedLabel = "";
  try {
    const firstInspectableItem = await androidWaitForNode(
      serial,
      (node) => {
        const tag = androidTag(node);
        return tag === `parity:map-selection-item:airport-${INSPECT_AIRPORT.ident}`;
      },
      5000,
      `${INSPECT_AIRPORT.ident} inspect item`,
    );
    const tag = androidTag(firstInspectableItem);
    selectedLabel = tag.slice("parity:map-selection-item:".length).split("-").at(-1) ?? "";
    androidTapResolvedNode(serial, out, "selected first inspect item", firstInspectableItem);
    recordCheck(out, "inspectItemSelected", true);
    await delay(500);
  } catch (_error) {
    recordGap(out, "selected first inspect item", "no inspect item was visible");
    recordCheck(out, "inspectItemSelected", false);
  }

  const selectedXml = dumpAndroid(serial);
  recordInventory(out, "chart.inspect.selected-actions", androidCollectTags(selectedXml, "parity:map-selection-action:"));
  const insertAction = findNode(selectedXml, (node) => hasAndroidTag(node, "parity:map-selection-action:insert"));
  if (insertAction) {
    recordCheck(out, "inspectInsertActionPresent", true);
    androidTapResolvedNode(serial, out, "inspect insert action present", insertAction);
    await delay(500);
    if (selectedLabel) {
      await androidTapTag(serial, out, "opened plan page after insert", "parity:nav-cdi");
      await delay(500);
      if (hasAndroidText(dumpAndroid(serial), selectedLabel) || await androidScrollUntilText(serial, selectedLabel)) {
        recordStep(out, "verified inspected item in flight plan", "ok", selectedLabel);
        recordCheck(out, "inspectInsertAddsSelectedItem", true);
      } else {
        recordGap(out, "verified inspected item in flight plan", `${selectedLabel} was not visible in the plan after insert`);
        recordCheck(out, "inspectInsertAddsSelectedItem", false);
      }
    }
  } else {
    recordGap(out, "inspect insert action present", "no insert action was visible in the current inspect tray");
    recordCheck(out, "inspectInsertActionPresent", false);
  }

  out.finished_at = new Date().toISOString();
  out.status = out.gaps.length === 0 ? "pass" : "gaps";
  return out;
}

function comparePlatformOutputs(outputs) {
  const web = outputs.find((entry) => entry.platform === "web");
  const android = outputs.find((entry) => entry.platform === "android");
  if (!web || !android) return null;
  const comparison = {
    journey: JOURNEY_NAME,
    platform: "parity",
    status: "pass",
    divergences: [],
  };
  const baseCheckNames = [
    "openedPlanFromCdi",
    "planCdiReturnsToChart",
    "chartCdiReturnsToPlan",
    "chartSearchRecentersInspectAirport",
    "appendRoutePresent",
    "appendRouteFeedbackVisible",
    "appendRouteCommitsKAWO",
    "inspectTrayAppears",
    "inspectItemSelected",
    "inspectInsertActionPresent",
    "inspectInsertAddsSelectedItem",
  ];
  const actionClassNames = new Set([
    ...Object.keys(web.checks).filter((name) => name.startsWith("actionClass.")),
    ...Object.keys(android.checks).filter((name) => name.startsWith("actionClass.")),
  ]);
  const checkNames = [...baseCheckNames, ...[...actionClassNames].sort()];
  for (const name of checkNames) {
    const webValue = web.checks[name] ?? null;
    const androidValue = android.checks[name] ?? null;
    if (webValue !== androidValue) {
      comparison.divergences.push({ name, web: webValue, android: androidValue });
    }
  }
  const inventoryNames = new Set([
    ...Object.keys(web.inventories ?? {}),
    ...Object.keys(android.inventories ?? {}),
  ]);
  for (const name of [...inventoryNames].sort()) {
    const webInventory = web.inventories?.[name] ?? [];
    const androidInventory = android.inventories?.[name] ?? [];
    const webComparable = comparableInventory(webInventory, androidInventory);
    const androidComparable = comparableInventory(androidInventory, webInventory);
    if (JSON.stringify(webComparable) !== JSON.stringify(androidComparable)) {
      comparison.divergences.push({ name: `inventory.${name}`, web: webComparable, android: androidComparable });
    }
  }
  if (comparison.divergences.length > 0) {
    comparison.status = "diverged";
  }
  return comparison;
}

function comparableInventory(entries, counterpartEntries) {
  const counterpartById = new Map(counterpartEntries.map((entry) => [entry.id, entry]));
  return entries.map((entry) => {
    const counterpart = counterpartById.get(entry.id);
    const compareAsToggle = Boolean(entry.on || entry.off || counterpart?.on || counterpart?.off);
    return {
      id: entry.id,
      ...(entry.label && counterpart?.label ? { label: entry.label } : {}),
      enabled: entry.enabled,
      disabled: entry.disabled,
      ...(compareAsToggle
        ? { on: entry.on, off: entry.off }
        : { active: entry.active || entry.selected }),
    };
  });
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
  const comparison = args.platform === "both" ? comparePlatformOutputs(outputs) : null;
  const payload = outputs.length === 1 ? outputs[0] : { journeys: outputs, comparison };
  console.log(JSON.stringify(payload, null, 2));
  if (outputs.some((entry) => entry.status !== "pass") || (comparison !== null && comparison.status !== "pass")) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
