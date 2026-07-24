#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";

const require = createRequire(import.meta.url);
const WebSocket = require("ws");

const args = parseArgs(process.argv.slice(2));
const url = args.url ?? process.env.AEROBAG_PERF_URL ?? "http://127.0.0.1:8083/";
const browserKind = args.browser ?? process.env.AEROBAG_PERF_BROWSER ?? "chrome";
const chromeBin = args.chrome ?? process.env.CHROME_BIN ?? "google-chrome-stable";
const firefoxBin = args.firefox
  ?? process.env.FIREFOX_BIN
  ?? path.resolve(process.cwd(), "../../../.tools/firefox-test/firefox/firefox");
const geckodriverBin = args.geckodriver
  ?? process.env.GECKODRIVER_BIN
  ?? path.resolve(process.cwd(), "../../../.tools/firefox-test/geckodriver");
const logPath = args.log ?? process.env.AEROBAG_WEB_DEBUG_LOG ?? "/tmp/aerobag-web-debug.log";
const runId = args.runId ?? args["run-id"] ?? `terrain-replay-${Date.now()}`;
const durationMs = Number(args.durationMs ?? args["duration-ms"] ?? 16000);
const dragDelayMs = Number(args.dragDelayMs ?? args["drag-delay-ms"] ?? 5000);
const tracePath = args.trace ?? "/adsb-traces/n550ar/n550ar-2024-09-29.json";
const playbackRate = Number(args.rate ?? 8);
const headed = args.headed === "true";
const failOnThreshold = args.fail === "true" || process.env.AEROBAG_PERF_FAIL === "1";
const firstTerrainLimitMs = Number(args.firstTerrainLimitMs ?? args["first-terrain-limit-ms"] ?? 5000);
const paintedIntervalP90LimitMs = Number(args.paintedIntervalP90LimitMs ?? args["painted-interval-p90-limit-ms"] ?? 180);
const paintedIntervalMaxLimitMs = Number(args.paintedIntervalMaxLimitMs ?? args["painted-interval-max-limit-ms"] ?? 1000);
const minPaintedFrames = Number(args.minPaintedFrames ?? args["min-painted-frames"] ?? 4);
const rafGapLimitMs = Number(args.rafGapLimitMs ?? args["raf-gap-limit-ms"] ?? 250);
const eventLoopLagLimitMs = Number(args.eventLoopLagLimitMs ?? args["event-loop-lag-limit-ms"] ?? 250);
const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), `aerobag-terrain-perf-${browserKind}-`));

async function main() {
  let browser;
  let launched;
  try {
    console.log(`run_id: ${runId}`);
    launched = browserKind === "firefox"
      ? await launchFirefox()
      : await launchChrome();
    browser = launched.browser;
    const page = launched.page ?? await createPage(browser);
    if (page.prepareForNavigation) {
      await page.prepareForNavigation(runId);
    }
    await page.navigate(url);
    if (page.bringToFront) {
      await page.bringToFront();
    }
    await page.waitForLoad();
    await waitForMapReady(page);
    await waitForQuiet(page, 1000);

    await ensureTerrainLayerVisible(page);
    await selectReplaySource(page);
    await loadReplayTrace(page, tracePath, playbackRate);

    await emitToken(page, "terrain-replay-perf-begin", {
      run_id: runId,
      url,
      trace: tracePath,
      rate: playbackRate,
      duration_ms: durationMs,
      drag_delay_ms: dragDelayMs,
    });
    await clickByTestId(page, "playback-play-toggle");
    await emitToken(page, "terrain-replay-perf-play", { run_id: runId });

    const rect = await mapSurfaceRect(page);
    if (dragDelayMs >= 0 && dragDelayMs < durationMs) {
      await sleep(dragDelayMs);
      await dragAcrossMap(page, rect);
      await emitToken(page, "terrain-replay-perf-drag", { run_id: runId });
      await sleep(durationMs - dragDelayMs);
    } else {
      await sleep(durationMs);
    }

    await emitToken(page, "terrain-replay-perf-end", { run_id: runId });
    await sleep(500);

    const summary = await summarizeRun(logPath, runId);
    printSummary(summary);
    if (failOnThreshold) {
      applyThresholds(summary);
    }
  } finally {
    await browser?.close();
    if (launched?.process) {
      await terminateProcess(launched.process);
    }
    await removeDirectoryEventually(userDataDir);
  }
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--")) {
      continue;
    }
    const withoutPrefix = value.slice(2);
    const equals = withoutPrefix.indexOf("=");
    if (equals >= 0) {
      parsed[withoutPrefix.slice(0, equals)] = withoutPrefix.slice(equals + 1);
    } else {
      parsed[withoutPrefix] = values[index + 1] && !values[index + 1].startsWith("--")
        ? values[++index]
        : "true";
    }
  }
  return parsed;
}

async function launchChrome() {
  const chrome = await new Promise((resolve, reject) => {
    const chromeArgs = [
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      "--window-size=1200,1000",
      "about:blank",
    ];
    if (!headed) {
      chromeArgs.unshift("--headless=new", "--disable-gpu");
    }
    const child = spawn(chromeBin, chromeArgs, {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    const timeout = setTimeout(() => {
      reject(new Error(`timed out waiting for Chrome DevTools endpoint; stderr=${stderr}`));
    }, 15000);
    child.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString("utf8");
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timeout);
        resolve({ process: child, wsUrl: match[1] });
      }
    });
    child.on("exit", (code, signal) => {
      clearTimeout(timeout);
      reject(new Error(`Chrome exited before DevTools was ready: code=${code} signal=${signal} stderr=${stderr}`));
    });
  });
  const browser = await connectToBrowser(chrome.wsUrl);
  return { ...chrome, browser };
}

async function launchFirefox() {
  if (!fs.existsSync(firefoxBin)) {
    throw new Error(`Firefox binary not found: ${firefoxBin}`);
  }
  if (!fs.existsSync(geckodriverBin)) {
    throw new Error(`geckodriver binary not found: ${geckodriverBin}`);
  }
  const port = await findLikelyFreePort();
  const driver = await launchGeckodriver(port);
  const client = new WebDriverClient(`http://127.0.0.1:${port}`);
  const page = await client.createFirefoxSession({
    binary: firefoxBin,
    args: headed ? [] : ["-headless"],
  });
  await page.setWindowRect(0, 0, 1200, 1000);
  return {
    process: driver.process,
    browser: client,
    page,
  };
}

function launchGeckodriver(port) {
  return new Promise((resolve, reject) => {
    const child = spawn(geckodriverBin, [
      "--host",
      "127.0.0.1",
      "--port",
      String(port),
      "--binary",
      firefoxBin,
      "--log",
      "warn",
    ], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill("SIGTERM");
      reject(new Error(`timed out waiting for geckodriver on port ${port}; output=${output}`));
    }, 15000);
    const finish = (result) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve(result);
    };
    const fail = (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      reject(error);
    };
    child.stdout.on("data", (chunk) => {
      output += chunk.toString("utf8");
    });
    child.stderr.on("data", (chunk) => {
      output += chunk.toString("utf8");
    });
    child.on("error", (error) => {
      fail(error);
    });
    child.on("exit", (code, signal) => {
      fail(new Error(`geckodriver exited before ready: code=${code} signal=${signal} output=${output}`));
    });
    void (async () => {
      for (;;) {
        try {
          await requestJson(new URL(`http://127.0.0.1:${port}/status`), "GET");
          finish({ process: child });
          return;
        } catch {
          if (settled) {
            return;
          }
          await sleep(100);
        }
      }
    })();
  });
}

async function findLikelyFreePort() {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const port = 29000 + Math.floor(Math.random() * 10000);
    if (await portLooksClosed(port)) {
      return port;
    }
  }
  return 4444;
}

function portLooksClosed(port) {
  return new Promise((resolve) => {
    const request = http.request({
      host: "127.0.0.1",
      port,
      path: "/status",
      method: "GET",
      timeout: 150,
    }, () => resolve(false));
    request.on("timeout", () => {
      request.destroy();
      resolve(true);
    });
    request.on("error", () => resolve(true));
    request.end();
  });
}

function waitForProcessExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true);
  }
  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      child.off("exit", onExit);
      resolve(false);
    }, timeoutMs);
    const onExit = () => {
      clearTimeout(timeout);
      resolve(true);
    };
    child.once("exit", onExit);
  });
}

async function terminateProcess(child) {
  if (!child.killed) {
    child.kill("SIGTERM");
  }
  const exited = await waitForProcessExit(child, 2000);
  if (!exited && !child.killed) {
    child.kill("SIGKILL");
    await waitForProcessExit(child, 1000);
  }
}

async function removeDirectoryEventually(directory) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    try {
      fs.rmSync(directory, { recursive: true, force: true });
      return;
    } catch (error) {
      if (attempt === 9) {
        console.warn(`warning: failed to remove ${directory}: ${error instanceof Error ? error.message : String(error)}`);
        return;
      }
      await sleep(100);
    }
  }
}

async function connectToBrowser(wsUrl) {
  const client = new CdpClient(wsUrl);
  await client.open();
  return {
    async close() {
      await client.close();
    },
    async createTarget() {
      const created = await client.send("Target.createTarget", { url: "about:blank" });
      const attached = await client.send("Target.attachToTarget", {
        targetId: created.targetId,
        flatten: true,
      });
      return new CdpPage(client, attached.sessionId);
    },
  };
}

async function createPage(browser) {
  return await browser.createTarget();
}

class CdpPage {
  constructor(client, sessionId) {
    this.client = client;
    this.sessionId = sessionId;
    this.diagnostics = [];
    this.loadPromise = new Promise((resolve) => {
      this.resolveLoad = resolve;
    });
    this.client.onEvent((message) => {
      if (message.sessionId === this.sessionId && message.method === "Page.loadEventFired") {
        this.resolveLoad();
      }
      if (message.sessionId !== this.sessionId) {
        return;
      }
      if (message.method === "Runtime.exceptionThrown") {
        this.diagnostics.push({
          method: message.method,
          exception: message.params?.exceptionDetails,
        });
      } else if (message.method === "Runtime.consoleAPICalled") {
        this.diagnostics.push({
          method: message.method,
          type: message.params?.type,
          args: (message.params?.args ?? []).map((arg) => arg.value ?? arg.description ?? arg.unserializableValue ?? ""),
        });
      } else if (message.method === "Log.entryAdded") {
        this.diagnostics.push({
          method: message.method,
          entry: message.params?.entry,
        });
      }
    });
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
  }

  async prepareForNavigation(selectedRunId) {
    await this.send("Page.enable");
    await this.send("Runtime.enable");
    await this.send("Log.enable");
    await this.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `globalThis.__aerobagPerfRunId = ${JSON.stringify(selectedRunId)};`,
    });
  }

  async bringToFront() {
    await this.send("Page.bringToFront");
  }

  async navigate(targetUrl) {
    this.loadPromise = new Promise((resolve) => {
      this.resolveLoad = resolve;
    });
    await this.send("Page.navigate", { url: targetUrl });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(`Runtime.evaluate failed: ${JSON.stringify(result.exceptionDetails)}`);
    }
    return result.result?.value;
  }

  async clickAt(x, y) {
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "none", pointerType: "mouse" });
    await this.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1, pointerType: "mouse" });
    await this.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1, pointerType: "mouse" });
  }

  async drag(start, end, steps, stepDelayMs) {
    const pointerId = 1;
    await this.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: start.x,
      y: start.y,
      button: "none",
      pointerType: "mouse",
    });
    await this.send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: start.x,
      y: start.y,
      button: "left",
      buttons: 1,
      clickCount: 1,
      pointerType: "mouse",
      pointerId,
    });
    for (let step = 1; step <= steps; step += 1) {
      const t = step / steps;
      await this.send("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: start.x + (end.x - start.x) * t,
        y: start.y + (end.y - start.y) * t,
        button: "left",
        buttons: 1,
        pointerType: "mouse",
        pointerId,
      });
      await sleep(stepDelayMs);
    }
    await this.send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: end.x,
      y: end.y,
      button: "left",
      buttons: 0,
      clickCount: 1,
      pointerType: "mouse",
      pointerId,
    });
  }

  waitForLoad() {
    return this.loadPromise;
  }
}

class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.nextId = 1;
    this.pending = new Map();
    this.eventListeners = [];
  }

  open() {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.wsUrl);
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
      this.ws.addEventListener("message", (event) => this.handleMessage(event.data));
    });
  }

  close() {
    this.ws?.close();
  }

  onEvent(listener) {
    this.eventListeners.push(listener);
  }

  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(message));
    });
  }

  handleMessage(data) {
    const message = JSON.parse(data);
    if (message.id && this.pending.has(message.id)) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(JSON.stringify(message.error)));
      } else {
        pending.resolve(message.result ?? {});
      }
      return;
    }
    for (const listener of this.eventListeners) {
      listener(message);
    }
  }
}

class WebDriverClient {
  constructor(baseUrl) {
    this.baseUrl = baseUrl;
    this.sessionId = null;
  }

  async createFirefoxSession(options) {
    const response = await this.request("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "firefox",
          acceptInsecureCerts: true,
          pageLoadStrategy: "normal",
          "moz:firefoxOptions": {
            binary: options.binary,
            args: options.args,
            prefs: {
              "dom.disable_beforeunload": true,
              "browser.shell.checkDefaultBrowser": false,
            },
          },
        },
      },
    });
    this.sessionId = response.sessionId ?? response.value?.sessionId;
    if (!this.sessionId) {
      throw new Error(`geckodriver did not return a session id: ${JSON.stringify(response)}`);
    }
    await this.request("POST", `/session/${this.sessionId}/timeouts`, {
      script: 60000,
      pageLoad: 60000,
      implicit: 0,
    });
    return new WebDriverPage(this);
  }

  async close() {
    if (!this.sessionId) {
      return;
    }
    const id = this.sessionId;
    this.sessionId = null;
    try {
      await this.request("DELETE", `/session/${id}`);
    } catch {
      // The browser may already be gone; process cleanup follows.
    }
  }

  request(method, pathname, body = undefined) {
    return requestJson(new URL(pathname, this.baseUrl), method, body);
  }
}

class WebDriverPage {
  constructor(client) {
    this.client = client;
    this.diagnostics = [];
  }

  async prepareForNavigation(selectedRunId) {
    this.preloadScript = `globalThis.__aerobagPerfRunId = ${JSON.stringify(selectedRunId)}`;
  }

  async navigate(targetUrl) {
    await this.client.request("POST", `/session/${this.client.sessionId}/url`, { url: targetUrl });
    if (this.preloadScript) {
      await this.evaluate(this.preloadScript);
    }
  }

  async waitForLoad() {
    await waitForCondition(this, `document.readyState === "complete"`, 60000);
  }

  async setWindowRect(x, y, width, height) {
    await this.client.request("POST", `/session/${this.client.sessionId}/window/rect`, {
      x,
      y,
      width,
      height,
    });
  }

  async evaluate(expression) {
    const script = `
      const done = arguments[arguments.length - 1];
      Promise.resolve()
        .then(async () => (${expression}))
        .then((value) => done({ ok: true, value }))
        .catch((error) => done({
          ok: false,
          message: error && error.message ? String(error.message) : String(error),
          stack: error && error.stack ? String(error.stack) : null,
        }));
    `;
    const response = await this.client.request("POST", `/session/${this.client.sessionId}/execute/async`, {
      script,
      args: [],
    });
    const result = response.value ?? response;
    if (!result?.ok) {
      throw new Error(`WebDriver evaluate failed: ${JSON.stringify(result)}`);
    }
    return result.value;
  }

  async clickAt(x, y) {
    await this.performPointerActions([
      { type: "pointerMove", duration: 0, x: Math.round(x), y: Math.round(y), origin: "viewport" },
      { type: "pointerDown", button: 0 },
      { type: "pointerUp", button: 0 },
    ]);
  }

  async drag(start, end, steps, stepDelayMs) {
    const actions = [
      { type: "pointerMove", duration: 0, x: Math.round(start.x), y: Math.round(start.y), origin: "viewport" },
      { type: "pointerDown", button: 0 },
    ];
    for (let step = 1; step <= steps; step += 1) {
      const t = step / steps;
      actions.push({
        type: "pointerMove",
        duration: stepDelayMs,
        x: Math.round(start.x + (end.x - start.x) * t),
        y: Math.round(start.y + (end.y - start.y) * t),
        origin: "viewport",
      });
    }
    actions.push({ type: "pointerUp", button: 0 });
    await this.performPointerActions(actions);
  }

  async performPointerActions(actions) {
    await this.client.request("POST", `/session/${this.client.sessionId}/actions`, {
      actions: [{
        type: "pointer",
        id: "mouse",
        parameters: { pointerType: "mouse" },
        actions,
      }],
    });
    await this.client.request("DELETE", `/session/${this.client.sessionId}/actions`);
  }
}

function requestJson(url, method, body) {
  return new Promise((resolve, reject) => {
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body), "utf8");
    const request = http.request(url, {
      method,
      headers: payload
        ? {
          "Content-Type": "application/json",
          "Content-Length": String(payload.byteLength),
        }
        : undefined,
    }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
      response.on("end", () => {
        const text = Buffer.concat(chunks).toString("utf8");
        let parsed = null;
        if (text.trim()) {
          try {
            parsed = JSON.parse(text);
          } catch (error) {
            reject(new Error(`invalid JSON from ${method} ${url}: ${text.slice(0, 500)}; ${error instanceof Error ? error.message : String(error)}`));
            return;
          }
        }
        if (response.statusCode < 200 || response.statusCode >= 300) {
          reject(new Error(`HTTP ${response.statusCode} from ${method} ${url}: ${text.slice(0, 1000)}`));
          return;
        }
        resolve(parsed ?? {});
      });
    });
    request.on("error", reject);
    if (payload) {
      request.write(payload);
    }
    request.end();
  });
}

async function waitForMapReady(page) {
  const deadline = Date.now() + 60000;
  for (;;) {
    const ready = await page.evaluate(`(() => {
      const surface = document.querySelector('[data-testid="map-surface"]');
      if (!surface) return false;
      const rect = surface.getBoundingClientRect();
      const images = Array.from(document.querySelectorAll('.mapTileImage'));
      const loadedImages = images.filter((image) => image.complete && image.naturalWidth > 0);
      return rect.width >= 600
        && rect.height >= 500
        && images.length > 0
        && loadedImages.length > 0;
    })()`);
    if (ready) {
      return;
    }
    if (Date.now() > deadline) {
      const state = await page.evaluate(`(() => {
        const surface = document.querySelector('[data-testid="map-surface"]');
        const rect = surface ? surface.getBoundingClientRect() : null;
        const viteError = document.querySelector('vite-error-overlay')?.shadowRoot?.textContent
          ?? document.querySelector('vite-error-overlay')?.textContent
          ?? null;
        return {
          url: location.href,
          title: document.title,
          bodyText: document.body?.innerText?.slice(0, 500) ?? "",
          surface: surface ? { width: rect.width, height: rect.height, className: surface.className } : null,
          imageCount: document.querySelectorAll('.mapTileImage').length,
          viteError: viteError ? viteError.slice(0, 1000) : null,
        };
      })()`);
      throw new Error(`timed out waiting for map surface and initial raster images: ${JSON.stringify({
        state,
        diagnostics: page.diagnostics.slice(-20),
      })}`);
    }
    await sleep(250);
  }
}

async function waitForQuiet(page, quietMs) {
  let lastCount = -1;
  let stableSince = Date.now();
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const count = await page.evaluate("performance.getEntriesByType('resource').length");
    if (count !== lastCount) {
      lastCount = count;
      stableSince = Date.now();
    }
    if (Date.now() - stableSince >= quietMs) {
      return;
    }
    await sleep(250);
  }
}

async function ensureTerrainLayerVisible(page) {
  await clickByTestId(page, "layers-button");
  await waitForSelector(page, '[data-testid="tray-option-terrain_warning"]');
  const terrainIsOn = await page.evaluate(`(() => {
    const button = document.querySelector('[data-testid="tray-option-terrain_warning"]');
    return Boolean(button && button.classList.contains('isOn'));
  })()`);
  if (!terrainIsOn) {
    await clickByTestId(page, "tray-option-terrain_warning");
    await waitForCondition(page, `(() => {
      const button = document.querySelector('[data-testid="tray-option-terrain_warning"]');
      return !button || button.classList.contains('isOn');
    })()`, 10000);
  } else {
    await clickByTestId(page, "layers-button");
  }
}

async function selectReplaySource(page) {
  await clickByTestId(page, "ownship-source-button");
  await waitForCondition(page, `(() => {
    return Array.from(document.querySelectorAll('[data-testid^="tray-option-"]'))
      .some((button) => /replay/i.test(button.textContent ?? ''));
  })()`, 10000);
  const replayTestId = await page.evaluate(`(() => {
    const button = Array.from(document.querySelectorAll('[data-testid^="tray-option-"]'))
      .find((candidate) => /replay/i.test(candidate.textContent ?? ''));
    return button?.getAttribute('data-testid') ?? null;
  })()`);
  if (!replayTestId) {
    throw new Error("replay ownship source option is not visible");
  }
  await clickByTestId(page, replayTestId.replace(/^tray-option-/, "tray-option-"));
  try {
    await waitForSelector(page, ".playbackWidgetInput");
  } catch (error) {
    const state = await page.evaluate(`(() => ({
      bodyText: document.body?.innerText?.slice(0, 1000) ?? "",
      sourceButton: document.querySelector('[data-testid="ownship-source-button"]')?.textContent ?? null,
      options: Array.from(document.querySelectorAll('[data-testid^="tray-option-"]')).map((button) => ({
        testId: button.getAttribute('data-testid'),
        text: button.textContent,
        className: button.className,
        disabled: button.disabled,
      })),
      playbackPresent: Boolean(document.querySelector('.playbackWidget')),
    }))()`);
    throw new Error(`replay source selected but playback widget did not appear: ${JSON.stringify(state)}; ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function loadReplayTrace(page, selectedTracePath, rate) {
  await setInputValue(page, ".playbackWidgetInput", selectedTracePath);
  await setInputValue(page, ".playbackWidgetRate", String(rate));
  await clickByTestId(page, "playback-load-button");
  await waitForCondition(page, `(() => {
    const button = document.querySelector('[data-testid="playback-play-toggle"]');
    return Boolean(button && !button.disabled);
  })()`, 30000);
}

async function clickByTestId(page, testId) {
  await clickSelector(page, `[data-testid="${cssString(testId)}"]`);
}

async function clickSelector(page, selector) {
  const rect = await page.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  })()`);
  if (!rect) {
    throw new Error(`missing element: ${selector}`);
  }
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;
  await page.clickAt(x, y);
}

function cssString(value) {
  return String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

async function setInputValue(page, selector, value) {
  await page.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) throw new Error('missing input: ${selector}');
    const prototype = Object.getPrototypeOf(element);
    const descriptor = Object.getOwnPropertyDescriptor(prototype, 'value');
    descriptor?.set?.call(element, ${JSON.stringify(value)});
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
}

async function waitForSelector(page, selector, timeoutMs = 10000) {
  await waitForCondition(page, `Boolean(document.querySelector(${JSON.stringify(selector)}))`, timeoutMs);
}

async function waitForCondition(page, expression, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await page.evaluate(expression)) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for condition: ${expression}`);
    }
    await sleep(100);
  }
}

async function emitToken(page, tag, data) {
  await page.evaluate(`fetch('/__debug_log', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify([{
      seq: 0,
      ts_ms: Math.round(performance.now()),
      tag: ${JSON.stringify(tag)},
      browser_instance_id: String(globalThis.__aerobagBrowserInstanceId ?? ''),
      run_id: ${JSON.stringify(runId)},
      data: ${JSON.stringify(data)}
    }])
  })`);
}

async function mapSurfaceRect(page) {
  return await page.evaluate(`(() => {
    const rect = document.querySelector('[data-testid="map-surface"]').getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  })()`);
}

async function dragAcrossMap(page, rect) {
  const start = {
    x: rect.left + rect.width * 0.32,
    y: rect.top + rect.height * 0.28,
  };
  const end = {
    x: rect.left + rect.width * 0.70,
    y: rect.top + rect.height * 0.68,
  };
  await page.drag(start, end, 16, 16);
}

async function summarizeRun(debugLogPath, selectedRunId) {
  let rows = [];
  let currentRows = [];
  let capturing = false;
  const input = fs.createReadStream(debugLogPath, { encoding: "utf8" });
  const lines = readline.createInterface({ input, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line) {
      continue;
    }
    let entry;
    try {
      entry = JSON.parse(line);
    } catch {
      continue;
    }
    if (entry.tag === "terrain-replay-perf-begin" && entry.data?.run_id === selectedRunId) {
      currentRows = [entry];
      capturing = true;
      continue;
    }
    if (!capturing) {
      continue;
    }
    currentRows.push(entry);
    if (entry.tag === "terrain-replay-perf-end" && entry.data?.run_id === selectedRunId) {
      rows = currentRows;
      capturing = false;
    }
  }
  if (rows.length === 0) {
    throw new Error(`could not find terrain-replay-perf-begin/end for ${selectedRunId}`);
  }
  const browserInstanceId = rows[0].browser_instance_id ?? "";
  rows = rows.filter((entry) =>
    !browserInstanceId
    || entry.browser_instance_id === browserInstanceId
    || entry.data?.run_id === selectedRunId,
  );
  const begin = rows.find((entry) => entry.tag === "terrain-replay-perf-begin");
  const play = rows.find((entry) => entry.tag === "terrain-replay-perf-play") ?? begin;
  const frameReady = rows.filter((entry) => entry.tag === "terrain.overlay.frame.ready");
  const framePainted = rows.filter((entry) => entry.tag === "terrain.overlay.frame.painted");
  const terrainBatches = rows.filter((entry) => entry.tag === "terrain.overlay.batch.done");
  const workerBatches = rows.filter((entry) => entry.tag === "terrain.worker.batch.done");
  const workerTiles = rows.filter((entry) => entry.tag === "terrain.worker.tile.done");
  const renderPlans = rows.filter((entry) => entry.tag === "terrain.overlay.render.plan");
  const rafGaps = rows.filter((entry) => entry.tag === "main_thread.raf_gap");
  const eventLoopLags = rows.filter((entry) => entry.tag === "main_thread.event_loop_lag");
  const longTasks = rows.filter((entry) => entry.tag === "main_thread.longtask");
  const firstTerrain = frameReady.find((entry) => entry.ts_ms >= play.ts_ms) ?? null;
  return {
    runId: selectedRunId,
    lineCount: rows.length,
    browserInstanceId,
    durationMs: (rows.at(-1)?.ts_ms ?? 0) - (begin?.ts_ms ?? 0),
    firstTerrainMs: firstTerrain ? firstTerrain.ts_ms - play.ts_ms : null,
    frameReady,
    framePainted,
    terrainBatches,
    workerBatches,
    workerTiles,
    renderPlans,
    rafGaps,
    eventLoopLags,
    longTasks,
  };
}

function printSummary(summary) {
  console.log(`run_id: ${summary.runId}`);
  console.log(`browser_instance_id: ${summary.browserInstanceId}`);
  console.log(`log_rows: ${summary.lineCount}`);
  console.log(`duration_ms: ${summary.durationMs}`);
  console.log(`first_terrain_ms: ${summary.firstTerrainMs ?? "missing"}`);
  console.log("");
  printStats("terrain_frame_ready_elapsed_ms", values(summary.frameReady, "elapsed_ms"));
  printStats("terrain_frame_ready_interval_ms", intervals(summary.frameReady));
  printStats("terrain_frame_painted_elapsed_ms", values(summary.framePainted, "elapsed_ms"));
  printStats("terrain_frame_painted_interval_ms", intervals(summary.framePainted));
  printStats("terrain_overlay_batch_elapsed_ms", values(summary.terrainBatches, "elapsed_ms"));
  printStats("terrain_worker_batch_elapsed_ms", values(summary.workerBatches, "elapsed_ms"));
  printStats("terrain_worker_tile_elapsed_ms", values(summary.workerTiles, "elapsed_ms"));
  printStats("main_thread_raf_gap_ms", values(summary.rafGaps, "gap_ms"));
  printStats("main_thread_event_loop_lag_ms", values(summary.eventLoopLags, "lag_ms"));
  printStats("main_thread_longtask_ms", values(summary.longTasks, "duration_ms"));
  console.log("");
  console.log(`terrain_frames: ${summary.frameReady.length}`);
  console.log(`terrain_frames_painted: ${summary.framePainted.length}`);
  console.log(`terrain_batches: ${summary.terrainBatches.length}`);
  console.log(`terrain_worker_batches: ${summary.workerBatches.length}`);
  console.log(`terrain_worker_tiles: ${summary.workerTiles.length}`);
  console.log("");
  printTopRows("top_render_plans", summary.renderPlans, (entry) => [
    `requests=${entry.data?.request_count}`,
    `cached=${entry.data?.cached_count}`,
    `in_flight=${entry.data?.in_flight_count}`,
    `missing=${entry.data?.missing_count}`,
    `batch=${entry.data?.work_batch_count}`,
    `zooms=${JSON.stringify(entry.data?.request_zooms ?? [])}`,
  ].join(" "));
  printTopRows("top_terrain_batches", summary.terrainBatches, (entry) => [
    `elapsed_ms=${entry.data?.elapsed_ms}`,
    `tile_count=${entry.data?.tile_count}`,
    `raw_bytes=${entry.data?.raw_bytes}`,
    `altitude_bucket=${entry.data?.altitude_bucket}`,
  ].join(" "), "elapsed_ms");
  printTopRows("top_raf_gaps", summary.rafGaps, (entry) => [
    `gap_ms=${entry.data?.gap_ms}`,
    `since_start_ms=${entry.ts_ms}`,
  ].join(" "), "gap_ms");
}

function values(rows, key) {
  return rows
    .map((entry) => entry.data?.[key])
    .filter((value) => Number.isFinite(value));
}

function intervals(rows) {
  const result = [];
  for (let index = 1; index < rows.length; index += 1) {
    result.push(rows[index].ts_ms - rows[index - 1].ts_ms);
  }
  return result;
}

function printStats(label, inputValues) {
  const sorted = inputValues.slice().sort((a, b) => a - b);
  if (sorted.length === 0) {
    console.log(`${label}: no samples`);
    return;
  }
  console.log(`${label}: n=${sorted.length} min=${sorted[0]} p50=${percentile(sorted, 0.50)} p90=${percentile(sorted, 0.90)} max=${sorted[sorted.length - 1]}`);
}

function percentile(sorted, p) {
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(sorted.length * p) - 1));
  return sorted[index];
}

function printTopRows(label, rows, formatter, sortKey = null) {
  console.log(label);
  const sorted = sortKey
    ? rows.slice().sort((left, right) => (right.data?.[sortKey] ?? -1) - (left.data?.[sortKey] ?? -1))
    : rows.slice();
  for (const entry of sorted.slice(0, 8)) {
    console.log(`  ${formatter(entry)}`);
  }
}

function applyThresholds(summary) {
  const failures = [];
  if (summary.firstTerrainMs == null || summary.firstTerrainMs > firstTerrainLimitMs) {
    failures.push(`first terrain ${summary.firstTerrainMs ?? "missing"} > ${firstTerrainLimitMs}ms`);
  }
  const paintedIntervals = intervals(summary.framePainted).sort((left, right) => left - right);
  const paintedIntervalP90 = paintedIntervals.length > 0 ? percentile(paintedIntervals, 0.90) : null;
  const paintedIntervalMax = max(paintedIntervals);
  if (summary.framePainted.length < minPaintedFrames) {
    failures.push(`terrain painted frames ${summary.framePainted.length} < ${minPaintedFrames}`);
  }
  if (paintedIntervalP90 == null || paintedIntervalP90 > paintedIntervalP90LimitMs) {
    failures.push(`p90 terrain painted interval ${paintedIntervalP90 ?? "missing"} > ${paintedIntervalP90LimitMs}ms`);
  }
  if (paintedIntervalMax == null || paintedIntervalMax > paintedIntervalMaxLimitMs) {
    failures.push(`max terrain painted interval ${paintedIntervalMax ?? "missing"} > ${paintedIntervalMaxLimitMs}ms`);
  }
  const maxRafGap = max(values(summary.rafGaps, "gap_ms"));
  if (maxRafGap != null && maxRafGap > rafGapLimitMs) {
    failures.push(`max RAF gap ${maxRafGap} > ${rafGapLimitMs}ms`);
  }
  const maxEventLoopLag = max(values(summary.eventLoopLags, "lag_ms"));
  if (maxEventLoopLag != null && maxEventLoopLag > eventLoopLagLimitMs) {
    failures.push(`max event-loop lag ${maxEventLoopLag} > ${eventLoopLagLimitMs}ms`);
  }
  if (failures.length > 0) {
    console.error("");
    console.error("terrain replay perf thresholds failed:");
    for (const failure of failures) {
      console.error(`  ${failure}`);
    }
    process.exitCode = 1;
  }
}

function max(inputValues) {
  if (inputValues.length === 0) {
    return null;
  }
  return Math.max(...inputValues);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
