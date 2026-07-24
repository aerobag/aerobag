#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawn } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";

const require = createRequire(import.meta.url);
const WebSocket = require("ws");

const networkProfiles = {
  none: null,
  "wifi-okay": { latencyMs: 40, downKbps: 12000, upKbps: 3000 },
  "wifi-bad": { latencyMs: 100, downKbps: 3000, upKbps: 750 },
  "cell-4g": { latencyMs: 150, downKbps: 1500, upKbps: 500 },
  "cell-bad": { latencyMs: 300, downKbps: 600, upKbps: 200 },
};

const args = parseArgs(process.argv.slice(2));
const url = args.url ?? process.env.AEROBAG_PERF_URL ?? "http://127.0.0.1:8085/";
const chromeBin = args.chrome ?? process.env.CHROME_BIN ?? "google-chrome-stable";
const logPath = args.log ?? process.env.AEROBAG_WEB_DEBUG_LOG ?? "/tmp/aerobag-web-debug.log";
const samples = positiveNumber(args.samples, 6);
const startupTimeoutMs = positiveNumber(args.timeoutMs ?? args["timeout-ms"], 30000);
const waitTimeoutMs = positiveNumber(args.waitMs ?? args["wait-ms"], Math.max(120000, samples * (startupTimeoutMs + 5000)));
const settleMs = positiveNumber(args.settleMs ?? args["settle-ms"], 1000);
const cacheMode = args.cache ?? "enabled";
const profileName = args.profile ?? "cell-4g";
const runId = args.runId ?? args["run-id"] ?? `startup-net-${profileName}-${Date.now()}`;
const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), "aerobag-startup-network-chrome-"));
const networkProfile = resolveNetworkProfile(profileName, args);

async function main() {
  let chrome;
  let browser;
  try {
    const startOffset = fileSize(logPath);
    console.log(`run_id: ${runId}`);
    console.log(`url: ${url}`);
    console.log(`samples: ${samples}`);
    console.log(`profile: ${describeProfile(profileName, networkProfile)}`);
    console.log(`cache: ${cacheMode}`);

    chrome = await launchChrome();
    browser = await connectToBrowser(chrome.wsUrl);
    const page = await browser.createTarget();
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Log.enable");
    await page.send("Network.enable");
    if (cacheMode === "disabled") {
      await page.send("Network.setCacheDisabled", { cacheDisabled: true });
    } else if (cacheMode === "enabled" || cacheMode === "cold") {
      await page.send("Network.setCacheDisabled", { cacheDisabled: false });
    } else {
      throw new Error(`unknown --cache mode ${JSON.stringify(cacheMode)}; use enabled, cold, or disabled`);
    }
    if (networkProfile) {
      await page.send("Network.emulateNetworkConditions", {
        offline: false,
        latency: networkProfile.latencyMs,
        downloadThroughput: kbpsToBytesPerSecond(networkProfile.downKbps),
        uploadThroughput: kbpsToBytesPerSecond(networkProfile.upKbps),
        connectionType: "cellular4g",
      });
    }
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `globalThis.__aerobagPerfRunId = ${JSON.stringify(runId)};`,
    });

    await runSamples(page, logPath, startOffset);
    await sleep(settleMs);

    const rows = await collectRun(logPath, runId);
    printSummary(rows);
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

async function runSamples(page, debugLogPath, startOffset) {
  let offset = startOffset;
  const deadline = Date.now() + waitTimeoutMs;
  for (let sampleIndex = 0; sampleIndex < samples; sampleIndex += 1) {
    const remainingMs = Math.max(1000, deadline - Date.now());
    if (cacheMode === "cold") {
      await page.send("Network.clearBrowserCache");
    }
    await page.navigate(startupHarnessUrl(url, sampleIndex));
    const result = await waitForSampleDone(
      debugLogPath,
      offset,
      runId,
      sampleIndex + 1,
      Math.min(remainingMs, startupTimeoutMs + 30000),
    );
    offset = result.offset;
    console.log(`sample ${sampleIndex + 1}/${samples}: ${result.reason}`);
  }
}

function startupHarnessUrl(baseUrl, sampleIndex) {
  const target = new URL(baseUrl);
  target.searchParams.set("startupReloadHarness", "1");
  target.searchParams.set("startupSamples", String(samples));
  target.searchParams.set("startupRunId", runId);
  target.searchParams.set("startupSampleIndex", String(sampleIndex));
  target.searchParams.set("startupTimeoutMs", String(startupTimeoutMs));
  target.searchParams.set("startupNoAutoReload", "1");
  target.searchParams.set("startupCacheBust", `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
  return target.href;
}

function resolveNetworkProfile(name, parsedArgs) {
  const customLatency = numberArg(parsedArgs.latencyMs ?? parsedArgs["latency-ms"]);
  const customDown = numberArg(parsedArgs.downKbps ?? parsedArgs["down-kbps"]);
  const customUp = numberArg(parsedArgs.upKbps ?? parsedArgs["up-kbps"]);
  if (customLatency !== null || customDown !== null || customUp !== null) {
    return {
      latencyMs: customLatency ?? 0,
      downKbps: customDown ?? 10000,
      upKbps: customUp ?? 1000,
    };
  }
  if (!(name in networkProfiles)) {
    throw new Error(`unknown --profile ${JSON.stringify(name)}; use one of ${Object.keys(networkProfiles).join(", ")}`);
  }
  return networkProfiles[name];
}

function describeProfile(name, profile) {
  if (!profile) {
    return `${name} (unthrottled)`;
  }
  return `${name} (${profile.latencyMs}ms, down ${profile.downKbps} kbps, up ${profile.upKbps} kbps)`;
}

function kbpsToBytesPerSecond(kbps) {
  return Math.max(1, Math.round((kbps * 1000) / 8));
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--")) {
      continue;
    }
    const keyValue = value.slice(2);
    const equals = keyValue.indexOf("=");
    if (equals >= 0) {
      parsed[keyValue.slice(0, equals)] = keyValue.slice(equals + 1);
    } else {
      parsed[keyValue] = values[index + 1] && !values[index + 1].startsWith("--")
        ? values[++index]
        : "true";
    }
  }
  return parsed;
}

function positiveNumber(value, fallback) {
  if (value === undefined || value === null) {
    return fallback;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function numberArg(value) {
  if (value === undefined || value === null) {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
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
    ], { stdio: ["ignore", "pipe", "pipe"] });

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

class CdpPage {
  constructor(client, sessionId) {
    this.client = client;
    this.sessionId = sessionId;
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
  }

  async navigate(targetUrl) {
    await this.send("Page.navigate", { url: targetUrl });
  }
}

class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.nextId = 1;
    this.pending = new Map();
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
    if (!message.id || !this.pending.has(message.id)) {
      return;
    }
    const pending = this.pending.get(message.id);
    this.pending.delete(message.id);
    if (message.error) {
      pending.reject(new Error(JSON.stringify(message.error)));
    } else {
      pending.resolve(message.result ?? {});
    }
  }
}

async function waitForSampleDone(debugLogPath, startOffset, selectedRunId, sampleIndex, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let offset = startOffset;
  let buffered = "";
  while (Date.now() < deadline) {
    const size = fileSize(debugLogPath);
    if (size > offset) {
      const chunk = await readFileRange(debugLogPath, offset, size);
      offset = size;
      buffered += chunk;
      const lines = buffered.split(/\n/);
      buffered = lines.pop() ?? "";
      for (const line of lines) {
        if (!line.includes(selectedRunId)) {
          continue;
        }
        let entry;
        try {
          entry = JSON.parse(line);
        } catch {
          continue;
        }
        if (
          entry.tag === "startup.reload_harness.sample.done"
          && entry.data?.run_id === selectedRunId
          && entry.data?.sample_index === sampleIndex
        ) {
          return { offset, reason: entry.data?.reason ?? "unknown" };
        }
      }
    }
    await sleep(250);
  }
  throw new Error(`timed out waiting for startup.reload_harness.sample.done for ${selectedRunId} sample ${sampleIndex}`);
}

function readFileRange(filePath, start, end) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    const stream = fs.createReadStream(filePath, { encoding: "utf8", start, end: end - 1 });
    stream.on("data", (chunk) => chunks.push(chunk));
    stream.on("error", reject);
    stream.on("end", () => resolve(chunks.join("")));
  });
}

async function collectRun(debugLogPath, selectedRunId) {
  const rows = {
    begins: [],
    dones: [],
    fatals: [],
    cacheWarm: [],
    longTasks: [],
    lag: [],
  };
  const stream = fs.createReadStream(debugLogPath, { encoding: "utf8" });
  let buffered = "";
  for await (const chunk of stream) {
    buffered += chunk;
    const lines = buffered.split(/\n/);
    buffered = lines.pop() ?? "";
    for (const line of lines) {
      collectRunLine(rows, line, selectedRunId);
    }
  }
  if (buffered) {
    collectRunLine(rows, buffered, selectedRunId);
  }
  return rows;
}

function collectRunLine(rows, line, selectedRunId) {
    if (!line.includes(selectedRunId)) {
    return;
    }
    let entry;
    try {
      entry = JSON.parse(line);
    } catch {
    return;
    }
    switch (entry.tag) {
      case "startup.reload_harness.sample.begin":
        rows.begins.push(entry);
        break;
      case "startup.reload_harness.sample.done":
        rows.dones.push(entry);
        break;
      case "startup.fatal":
        rows.fatals.push(entry);
        break;
      case "startup.cache_warm.done":
        rows.cacheWarm.push(entry);
        break;
      case "browser.long_task":
        rows.longTasks.push(entry);
        break;
      case "browser.event_loop_lag":
        rows.lag.push(entry);
        break;
    }
}

function printSummary(rows) {
  const samplesDone = rows.dones.map((entry) => entry.data).sort((left, right) => left.sample_index - right.sample_index);
  const doneIndexes = new Set(samplesDone.map((sample) => sample.sample_index));
  const beginIndexes = rows.begins.map((entry) => entry.data?.sample_index).filter(Number.isFinite).sort((left, right) => left - right);
  const missing = beginIndexes.filter((index) => !doneIndexes.has(index));
  console.log("");
  console.log(`sample_begin: ${rows.begins.length}`);
  console.log(`sample_done: ${rows.dones.length}`);
  console.log(`sample_missing_done: ${missing.length ? missing.join(",") : "none"}`);
  console.log(`startup_fatals: ${rows.fatals.length}`);
  console.log(`cache_warm_done: ${rows.cacheWarm.length}`);
  console.log("");
  console.log("metric                 min    p50    p90    p95    max   mean");
  printMetric("start", samplesDone, (milestones) => milestones.startMs);
  printMetric("adapter", samplesDone, (milestones) => milestones.adapterReadyMs);
  printMetric("session", samplesDone, (milestones) => milestones.sessionReadyMs);
  printMetric("shell", samplesDone, (milestones) => milestones.shellHideMs);
  printMetric("raster", samplesDone, (milestones) => milestones.firstRasterMs);
  printMetric("no_metar", samplesDone, (milestones) => milestones.airspaceVectorsMs ?? milestones.firstOverlayMs);
  printMetric("full", samplesDone, (milestones) => milestones.fullVectorsMs);
  console.log("");
  console.log("phase                 min    p50    p90    p95    max   mean");
  printMetric("adapter-start", samplesDone, (milestones) => diff(milestones.adapterReadyMs, milestones.startMs));
  printMetric("session-adapter", samplesDone, (milestones) => diff(milestones.sessionReadyMs, milestones.adapterReadyMs));
  printMetric("raster-session", samplesDone, (milestones) => diff(milestones.firstRasterMs, milestones.sessionReadyMs));
  printMetric("no_metar-raster", samplesDone, (milestones) => diff(milestones.airspaceVectorsMs ?? milestones.firstOverlayMs, milestones.firstRasterMs));
  printMetric("full-no_metar", samplesDone, (milestones) => diff(milestones.fullVectorsMs, milestones.airspaceVectorsMs ?? milestones.firstOverlayMs));
  if (rows.cacheWarm.length > 0) {
    console.log("");
    console.log("cache_warm");
    printStats("elapsed", rows.cacheWarm.map((entry) => entry.data?.elapsed_ms).filter(Number.isFinite));
    const counts = new Set(rows.cacheWarm.map((entry) => `${entry.data?.resource_count}/${entry.data?.fetched_count}`));
    console.log(`resources/fetched: ${Array.from(counts).sort().join(", ")}`);
  }
}

function printMetric(label, samplesDone, getter) {
  const values = samplesDone
    .map((sample) => getter(sample.milestones ?? {}))
    .filter(Number.isFinite);
  printStats(label, values);
}

function printStats(label, values) {
  const stats = summarize(values);
  if (!stats) {
    console.log(`${label.padEnd(20)}      -      -      -      -      -      -`);
    return;
  }
  console.log(`${label.padEnd(20)} ${formatNumber(stats.min)} ${formatNumber(stats.p50)} ${formatNumber(stats.p90)} ${formatNumber(stats.p95)} ${formatNumber(stats.max)} ${formatNumber(stats.mean)}`);
}

function summarize(values) {
  const sorted = values.slice().sort((left, right) => left - right);
  if (sorted.length === 0) {
    return null;
  }
  return {
    min: sorted[0],
    p50: median(sorted),
    p90: percentile(sorted, 0.90),
    p95: percentile(sorted, 0.95),
    max: sorted[sorted.length - 1],
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
  };
}

function median(sorted) {
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) {
    return sorted[middle];
  }
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

function percentile(sorted, p) {
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(sorted.length * p) - 1));
  return sorted[index];
}

function formatNumber(value) {
  return String(Math.round(value)).padStart(6);
}

function diff(later, earlier) {
  return Number.isFinite(later) && Number.isFinite(earlier) ? later - earlier : null;
}

function fileSize(filePath) {
  try {
    return fs.statSync(filePath).size;
  } catch {
    return 0;
  }
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

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

await main();
