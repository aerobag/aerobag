#!/usr/bin/env node
import { spawn } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";

const require = createRequire(import.meta.url);
const WebSocket = require("ws");

const args = parseArgs(process.argv.slice(2));
const url = args.url ?? process.env.AEROBAG_PERF_URL ?? "http://127.0.0.1:8085/";
const chromeBin = args.chrome ?? process.env.CHROME_BIN ?? "google-chrome-stable";
const logPath = args.log ?? process.env.AEROBAG_WEB_DEBUG_LOG ?? "/tmp/aerobag-web-debug.log";
const dragCount = Number(args.drags ?? 20);
const dragIntervalMs = Number(args.intervalMs ?? args["interval-ms"] ?? 500);
const settleMs = Number(args.settleMs ?? args["settle-ms"] ?? 4000);
const runId = args.runId ?? args["run-id"] ?? `vector-drag-${Date.now()}`;
const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), "aerobag-vector-perf-chrome-"));

async function main() {
  let chrome;
  let browser;
  try {
    console.log(`run_id: ${runId}`);
    chrome = await launchChrome();
    browser = await connectToBrowser(chrome.wsUrl);
    const page = await createPage(browser);
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `globalThis.__aerobagPerfRunId = ${JSON.stringify(runId)};`,
    });
    await page.navigate(url);
    await page.send("Page.bringToFront");
    await page.waitForLoad();
    await waitForMapReady(page);
    await waitForQuiet(page, 1500);

    await emitToken(page, "automated-test-begin", {
      run_id: runId,
      url,
      drag_count: dragCount,
      interval_ms: dragIntervalMs,
      direction: "northwest",
    });

    const rect = await mapSurfaceRect(page);
    for (let index = 0; index < dragCount; index += 1) {
      await dragNorthwest(page, rect, index);
      await sleep(dragIntervalMs);
    }

    await sleep(settleMs);
    await emitToken(page, "automated-test-end", {
      run_id: runId,
      settle_ms: settleMs,
    });
    await sleep(500);

    const summary = await summarizeRun(logPath, runId);
    printSummary(summary);
  } finally {
    await browser?.close();
    if (chrome?.process && !chrome.process.killed) {
      chrome.process.kill("SIGTERM");
      const exited = await waitForProcessExit(chrome.process, 2000);
      if (!exited && !chrome.process.killed) {
        chrome.process.kill("SIGKILL");
        await waitForProcessExit(chrome.process, 1000);
      }
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

function launchChrome() {
  return new Promise((resolve, reject) => {
    const child = spawn(chromeBin, [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      "--window-size=1200,1000",
      "about:blank",
    ], {
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
    this.loadPromise = new Promise((resolve) => {
      this.resolveLoad = resolve;
    });
    this.client.onEvent((message) => {
      if (message.sessionId === this.sessionId && message.method === "Page.loadEventFired") {
        this.resolveLoad();
      }
    });
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
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

async function waitForMapReady(page) {
  const deadline = Date.now() + 60000;
  for (;;) {
    const ready = await page.evaluate(`(() => {
      const surface = document.querySelector('[data-testid="map-surface"]');
      if (!surface) return false;
      const rect = surface.getBoundingClientRect();
      const images = Array.from(document.querySelectorAll('.mapTileImage'));
      return rect.width >= 600
        && rect.height >= 500
        && images.length > 0
        && images.every((image) => image.complete && image.naturalWidth > 0);
    })()`);
    if (ready) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error("timed out waiting for map surface and initial raster images");
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

async function emitToken(page, tag, data) {
  await page.evaluate(`fetch('/__debug_log', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify([{
      seq: 0,
      ts_ms: Math.round(performance.now()),
      tag: ${JSON.stringify(tag)},
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

async function dragNorthwest(page, rect, index) {
  const start = {
    x: rect.left + rect.width * 0.17,
    y: rect.top + rect.height * 0.17,
  };
  const end = {
    x: rect.left + rect.width * 0.83,
    y: rect.top + rect.height * 0.83,
  };
  const pointerId = 1;
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: start.x,
    y: start.y,
    button: "none",
    pointerType: "mouse",
  });
  await page.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: start.x,
    y: start.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
    pointerType: "mouse",
    pointerId,
  });
  const steps = 8;
  for (let step = 1; step <= steps; step += 1) {
    const t = step / steps;
    await page.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: start.x + (end.x - start.x) * t,
      y: start.y + (end.y - start.y) * t,
      button: "left",
      buttons: 1,
      pointerType: "mouse",
      pointerId,
    });
    await sleep(12);
  }
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: end.x,
    y: end.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
    pointerType: "mouse",
    pointerId,
  });
  await emitToken(page, "automated-test-drag", {
    run_id: runId,
    index,
  });
}

async function summarizeRun(debugLogPath, selectedRunId) {
  let runRows = [];
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
    if (entry.tag === "automated-test-begin" && entry.data?.run_id === selectedRunId) {
      currentRows = [entry];
      capturing = true;
      continue;
    }
    if (!capturing) {
      continue;
    }
    currentRows.push(entry);
    if (entry.tag === "automated-test-end" && entry.data?.run_id === selectedRunId) {
      runRows = currentRows;
      currentRows = [];
      capturing = false;
    }
  }
  if (runRows.length === 0) {
    throw new Error(`could not find automated-test-begin/end for ${selectedRunId}`);
  }
  const selectedRows = runRows.filter((entry) => entry.run_id === selectedRunId || entry.data?.run_id === selectedRunId);
  const dragTokens = selectedRows.filter((entry) => entry.tag === "automated-test-drag");
  const rasterImages = selectedRows.filter((entry) => entry.tag === "map.raster.images.done");
  const overlayStarts = selectedRows.filter((entry) => entry.tag === "map.overlay.query.start");
  const overlayDone = selectedRows.filter((entry) =>
    entry.tag === "map.overlay.query.done" || entry.tag === "map.overlay.query.superseded_result",
  );
  const overlayFreshDone = selectedRows.filter((entry) => entry.tag === "map.overlay.query.done");
  const overlaySupersededDone = selectedRows.filter((entry) => entry.tag === "map.overlay.query.superseded_result");
  const overlayStale = selectedRows.filter((entry) => entry.tag === "map.overlay.query.stale_result");
  const overlayDeferredForRaster = selectedRows.filter((entry) => entry.tag === "map.overlay.query.deferred_for_raster");
  const workerCalls = selectedRows.filter((entry) => entry.tag === "app_core.worker.call.done");
  const workerResponses = selectedRows.filter((entry) => entry.tag === "app_core.worker.response.received");
  const workerResponsePosts = selectedRows.filter((entry) => entry.tag === "app_core.worker.response.posted");
  const coreSteps = selectedRows.filter((entry) => entry.tag === "map.overlay.core_had.step");
  const coreOverlay = selectedRows.filter((entry) => entry.tag === "map.overlay.core");
  const sessionOverlay = selectedRows.filter((entry) => entry.tag === "map.overlay.session");
  const wasmOverlay = selectedRows.filter((entry) => entry.tag === "map.overlay.wasm");
  const dragResults = dragTokens.map((drag, index) => {
    const endTs = drag.ts_ms;
    const nextGroupTs = dragTokens[index + 1]?.ts_ms ?? Infinity;
    const image = rasterImages.find((entry) => entry.ts_ms >= endTs);
    const overlay = overlayDone.find((entry) => entry.ts_ms >= endTs);
    const overlayBeforeNextDrag = overlayDone.find((entry) => entry.ts_ms >= endTs && entry.ts_ms < nextGroupTs);
    return {
      index,
      drag_ts: endTs,
      moves: null,
      raster_ms: image ? image.ts_ms - endTs : null,
      vector_ms: overlay ? overlay.ts_ms - endTs : null,
      vector_before_next_drag_ms: overlayBeforeNextDrag ? overlayBeforeNextDrag.ts_ms - endTs : null,
    };
  });
  return {
    runId: selectedRunId,
    lineCount: selectedRows.length,
    dragResults,
    rasterLatencies: dragResults.map((row) => row.raster_ms).filter(Number.isFinite),
    vectorLatencies: dragResults.map((row) => row.vector_ms).filter(Number.isFinite),
    vectorBeforeNextDragLatencies: dragResults.map((row) => row.vector_before_next_drag_ms).filter(Number.isFinite),
    overlayStarts,
    overlayDone,
    overlayFreshDone,
    overlaySupersededDone,
    overlayStaleCount: overlayStale.length,
    overlayStale,
    overlayDeferredForRaster,
    workerCalls,
    workerResponses,
    workerResponsePosts,
    coreSteps,
    coreOverlay,
    sessionOverlay,
    wasmOverlay,
  };
}

function groupByGap(entries, gapMs) {
  const groups = [];
  let current = [];
  for (const entry of entries) {
    if (current.length > 0 && entry.ts_ms - current[current.length - 1].ts_ms > gapMs) {
      groups.push(current);
      current = [];
    }
    current.push(entry);
  }
  if (current.length > 0) {
    groups.push(current);
  }
  return groups;
}

function printSummary(summary) {
  console.log(`run_id: ${summary.runId}`);
  console.log(`log_rows: ${summary.lineCount}`);
  console.log(`drag_groups: ${summary.dragResults.length}`);
  console.log("");
  printStats("raster_images_ms", summary.rasterLatencies);
  printStats("vector_first_done_ms", summary.vectorLatencies);
  printStats("vector_before_next_drag_ms", summary.vectorBeforeNextDragLatencies);
  console.log(`overlay_stale_results: ${summary.overlayStaleCount}`);
  console.log(`overlay_fresh_done: ${summary.overlayFreshDone.length}`);
  console.log(`overlay_superseded_landed: ${summary.overlaySupersededDone.length}`);
  console.log(`overlay_deferred_for_raster: ${summary.overlayDeferredForRaster.length}`);
  console.log("");
  printOverlayQueryPumpStats(summary.overlayStarts, summary.overlayDone, summary.overlayStale);
  printTopCalls("top_app_core_worker_elapsed", summary.workerCalls, "elapsed_ms");
  printTopCalls("top_app_core_worker_queue_wait", summary.workerCalls, "queue_wait_ms");
  printWorkerOverlayStats(summary.workerCalls);
  printTopResponses(summary.workerResponses);
  printWorkerPostStats(summary.workerResponsePosts);
  printOverlayStepStats(summary.coreSteps);
  printOverlaySteps(summary.coreSteps);
  printOverlayPhaseStats(summary.sessionOverlay, "map.overlay.session");
  printOverlayPhaseStats(summary.wasmOverlay, "map.overlay.wasm");
  printOverlayCoreStats(summary.coreOverlay);
  printOverlayPhaseRows(summary.sessionOverlay, "map.overlay.session");
  printOverlayPhaseRows(summary.wasmOverlay, "map.overlay.wasm");
  printOverlayCoreRows(summary.coreOverlay);
}

function printStats(label, values) {
  const sorted = values.slice().sort((a, b) => a - b);
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

function printTopCalls(label, rows, metric) {
  console.log(label);
  for (const entry of rows
    .slice()
    .sort((left, right) => (right.data?.[metric] ?? -1) - (left.data?.[metric] ?? -1))
    .slice(0, 8)) {
    console.log(`  ${metric}=${entry.data?.[metric]} elapsed_ms=${entry.data?.elapsed_ms} queue_wait_ms=${entry.data?.queue_wait_ms} method=${entry.data?.method} id=${entry.data?.id}`);
  }
  console.log("");
}

function printTopResponses(rows) {
  console.log("worker_response_received");
  for (const entry of rows
    .filter((row) => row.data?.method === "queryMapOverlay" || row.data?.method === "snapshot")
    .slice()
    .sort((left, right) => (right.data?.round_trip_ms ?? -1) - (left.data?.round_trip_ms ?? -1))
    .slice(0, 8)) {
    console.log(`  round_trip_ms=${entry.data?.round_trip_ms} post_to_receive_ms=${entry.data?.post_to_receive_ms} method=${entry.data?.method} kind=${entry.data?.result_kind} features=${entry.data?.visible_features ?? ""}`);
  }
  console.log("");
}

function printOverlayQueryPumpStats(starts, done, stale) {
  console.log("map.overlay.query pump stats");
  printStats("  start_queue_wait_ms", starts.map((entry) => entry.data?.queue_wait_ms).filter(Number.isFinite));
  printStats("  done_elapsed_ms", done.map((entry) => entry.data?.elapsed_ms).filter(Number.isFinite));
  printStats("  stale_elapsed_ms", stale.map((entry) => entry.data?.elapsed_ms).filter(Number.isFinite));
  printStats("  done_visible_features", done.map((entry) => entry.data?.visible_features).filter(Number.isFinite));
  console.log("");
}

function printWorkerOverlayStats(rows) {
  const overlays = rows.filter((entry) => entry.data?.method === "queryMapOverlay");
  console.log("queryMapOverlay worker stats");
  printStats("  elapsed_ms", overlays.map((entry) => entry.data?.elapsed_ms).filter(Number.isFinite));
  printStats("  queue_wait_ms", overlays.map((entry) => entry.data?.queue_wait_ms).filter(Number.isFinite));
  console.log("");
}

function printWorkerPostStats(rows) {
  console.log("worker_response_posted map_overlay");
  const overlays = rows.filter((entry) => entry.data?.result_kind === "map_overlay");
  printStats("  post_ms", overlays.map((entry) => entry.data?.post_ms).filter(Number.isFinite));
  printStats("  features", overlays.map((entry) => entry.data?.visible_features).filter(Number.isFinite));
  console.log("");
}

function printOverlayStepStats(rows) {
  console.log("map.overlay.core_had.step stats");
  printStats("  op_ms", rows.map((entry) => entry.data?.operation_ms).filter(Number.isFinite));
  printStats("  parse_ms", rows.map((entry) => entry.data?.parse_ms).filter(Number.isFinite));
  printStats("  json_bytes", rows.map((entry) => entry.data?.json_bytes).filter(Number.isFinite));
  console.log("");
}

function printOverlaySteps(rows) {
  console.log("map.overlay.core_had.step");
  for (const entry of rows.slice(-12)) {
    console.log(`  iter=${entry.data?.iteration} state=${entry.data?.state} op_ms=${entry.data?.operation_ms} parse_ms=${entry.data?.parse_ms} json_bytes=${entry.data?.json_bytes} resources=${entry.data?.resource_count}`);
  }
  console.log("");
}

function printOverlayPhaseStats(rows, label) {
  console.log(`${label} stats`);
  for (const metric of ["total_ms", "overlay_ms", "to_value_ms", "core_ms", "serialize_ms", "json_bytes"]) {
    printStats(`  ${metric}`, rows.map((entry) => entry.data?.[metric]).filter(Number.isFinite));
  }
  console.log("");
}

function printOverlayCoreStats(rows) {
  console.log("map.overlay.core phase stats");
  for (const metric of ["total_ms", "point_vector_ms", "obstacle_ms", "airspace_ms", "tfr_ms", "metar_ms", "labels_ms"]) {
    printStats(`  ${metric}`, rows.map((entry) => entry.data?.timing?.[metric]).filter(Number.isFinite));
  }
  printStats("  visible_features", rows.map((entry) => entry.data?.visible_features).filter(Number.isFinite));
  printStats("  visible_metars", rows.map((entry) => entry.data?.visible_metars).filter(Number.isFinite));
  console.log("");
}

function printOverlayPhaseRows(rows, label) {
  console.log(label);
  for (const entry of rows
    .slice()
    .sort((left, right) => (right.data?.total_ms ?? -1) - (left.data?.total_ms ?? -1))
    .slice(0, 8)) {
    const data = entry.data ?? {};
    console.log(`  total=${data.total_ms} overlay=${data.overlay_ms ?? ""} to_value=${data.to_value_ms ?? ""} core=${data.core_ms ?? ""} serialize=${data.serialize_ms ?? ""} json_bytes=${data.json_bytes ?? ""}`);
  }
  console.log("");
}

function printOverlayCoreRows(rows) {
  console.log("map.overlay.core phases");
  for (const entry of rows
    .slice()
    .sort((left, right) => (right.data?.timing?.total_ms ?? -1) - (left.data?.timing?.total_ms ?? -1))
    .slice(0, 8)) {
    const timing = entry.data?.timing ?? {};
    console.log(`  total=${timing.total_ms} point=${timing.point_vector_ms} airspace=${timing.airspace_ms} labels=${timing.labels_ms} metar=${timing.metar_ms} features=${entry.data?.visible_features} metars=${entry.data?.visible_metars}`);
  }
  console.log("");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

await main();
