#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import {
  adb,
  adbBestEffort,
  captureAndroidFailureDiagnostics,
  waitFor,
  wakeAndUnlock,
} from "./android-harness.mjs";
import { E2E_TIMING, observeUntil, TerminalObservationError } from "./transition-contract.mjs";
import { connectChromeStartupTrace } from "./chrome-startup-trace.mjs";
import { createReleaseJourneyFixtureServer } from "./serve-release-journey-fixture.mjs";
import {
  LIVE_FEED_SCHEMA_VERSION,
  liveFeedPath,
  metarVersionFromPath,
} from "./live-feed-contract-paths.mjs";

const REPO_ROOT = process.env.AEROBAG_REPO_ROOT
  ?? path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DEFAULT_WEB_PORT = 18082;
const DEFAULT_LIVE_FEED_PORT = 18083;
const DEFAULT_CDP_PORT = 9222;
const CDP_COMMAND_TIMEOUT_MS = E2E_TIMING.localReadyMs;
// Compilation is setup, not server readiness or an application transition.
const WEB_BUILD_TIMEOUT_MS = 10 * 60_000;
const CHROME_LAUNCH_ATTEMPTS = 3;
const CHROME_PACKAGES = [
  "com.android.chrome",
  "com.chrome.beta",
  "com.chrome.dev",
  "org.chromium.chrome",
];

function progress(message) {
  process.stderr.write(`[android-chrome-livefeed] ${message}\n`);
}

function usage() {
  console.log(`Usage:
  node tools/e2e/run-android-chrome-livefeed-e2e.mjs [--serial emulator-5554] [--web-url URL | --web-dist DIR --release-fixture FILE] [--json]

Starts a scripted live-feed server, serves the immutable bundle or local Vite,
launches Android Chrome through adb/CDP, forces an offline/online transition,
and verifies that live-feed recovery interrupts a pending reconnect backoff.`);
  console.log("AEROBAG_E2E_DISPOSABLE_CHROME_EMULATOR=1 isolates browser dependencies and configures test permissions/rendering; never use it on a personal AVD.");
}

function parseArgs(argv) {
  const args = {
    serial: process.env.ANDROID_SERIAL ?? "",
    webUrl: "",
    webDist: process.env.AEROBAG_RELEASE_JOURNEY_WEB_DIST ?? "",
    fixture: process.env.AEROBAG_RELEASE_JOURNEY_FIXTURE ?? "",
    webPort: Number(process.env.AEROBAG_E2E_WEB_PORT ?? DEFAULT_WEB_PORT),
    liveFeedPort: Number(process.env.AEROBAG_E2E_LIVE_FEED_PORT ?? DEFAULT_LIVE_FEED_PORT),
    cdpPort: Number(process.env.AEROBAG_E2E_CHROME_CDP_PORT ?? DEFAULT_CDP_PORT),
    disposableEmulator: process.env.AEROBAG_E2E_DISPOSABLE_CHROME_EMULATOR === "1",
    json: false,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--serial") {
      args.serial = argv[++i] ?? "";
    } else if (arg === "--web-url") {
      args.webUrl = argv[++i] ?? "";
    } else if (arg === "--web-dist") {
      args.webDist = argv[++i] ?? "";
    } else if (arg === "--release-fixture") {
      args.fixture = argv[++i] ?? "";
    } else if (arg === "--web-port") {
      args.webPort = Number(argv[++i]);
    } else if (arg === "--live-feed-port") {
      args.liveFeedPort = Number(argv[++i]);
    } else if (arg === "--cdp-port") {
      args.cdpPort = Number(argv[++i]);
    } else if (arg === "--json") {
      args.json = true;
    } else if (arg === "-h" || arg === "--help") {
      args.help = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (args.webUrl && args.webDist) throw new Error("choose --web-url or --web-dist, not both");
  if (args.webDist && !args.fixture) throw new Error("--web-dist requires --release-fixture");
  return args;
}

function sha256Hex(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function canonicalizeJson(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalizeJson);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalizeJson(value[key])]),
    );
  }
  return value;
}

function canonicalJsonString(value) {
  return JSON.stringify(canonicalizeJson(value));
}

function utcForVersion(version) {
  const numeric = Number(String(version).replace(/\D/g, "")) || 1;
  return `2026-07-09T12:${String(numeric).padStart(2, "0")}:00Z`;
}

function makeMetarState(version) {
  const timestamp = utcForVersion(version);
  return {
    schema_version: LIVE_FEED_SCHEMA_VERSION,
    version_label: version,
    generated_at_utc: timestamp,
    observed_at_utc: timestamp,
    metar_count: 1,
    metars_by_station: {
      KSEA: {
        raw_text: `METAR KSEA ${version} AUTO 00000KT 10SM CLR 18/09 A3000`,
        observed_at_utc: timestamp,
        station_id: "KSEA",
        flight_category: "vfr",
        clouds: { symbol: "clr" },
        longitude: -122.309306,
        latitude: 47.449889,
      },
    },
    pireps: [],
  };
}

export class ScriptedLiveFeedServer {
  constructor() {
    this.currentVersion = "v1";
    this.versions = new Map();
    this.clients = new Set();
    this.eventsAvailable = true;
    this.requestCounts = { current: 0, events: 0, versions: 0, states: 0 };
    this.addVersion("v1");
  }

  addVersion(version) {
    if (this.versions.has(version)) {
      return this.versions.get(version);
    }
    const state = makeMetarState(version);
    const json = canonicalJsonString(state);
    const bytes = Buffer.from(json, "utf8");
    const stateSha256 = sha256Hex(bytes);
    const entry = {
      version,
      collectedAtUtc: utcForVersion(version),
      state,
      stateBytes: bytes,
      stateSha256,
      stateBlobSha256: sha256Hex(bytes),
    };
    this.versions.set(version, entry);
    return entry;
  }

  manifestFor(version) {
    const entry = this.addVersion(version);
    return {
      schema_version: LIVE_FEED_SCHEMA_VERSION,
      product: "metars",
      version,
      state: {
        kind: "json",
        url: `states/metars/${version}.json`,
        bytes: entry.stateBytes.length,
        blob_sha256: entry.stateBlobSha256,
        state_sha256: entry.stateSha256,
      },
    };
  }

  currentManifest() {
    const entry = this.addVersion(this.currentVersion);
    return {
      schema_version: LIVE_FEED_SCHEMA_VERSION,
      generated_at_utc: entry.collectedAtUtc,
      products: {
        metars: {
          current: entry.version,
          version_manifest_url: `versions/metars/${entry.version}.json`,
          state_url: `states/metars/${entry.version}.json`,
          state_sha256: entry.stateSha256,
          collected_at_utc: entry.collectedAtUtc,
          published_at_utc: entry.collectedAtUtc,
        },
      },
    };
  }

  publish(version) {
    const entry = this.addVersion(version);
    this.currentVersion = version;
    const payload = JSON.stringify({
      schema_version: LIVE_FEED_SCHEMA_VERSION,
      product: "metars",
      version,
      version_manifest_url: `versions/metars/${version}.json`,
      state_url: `states/metars/${version}.json`,
      state_sha256: entry.stateSha256,
      collected_at_utc: entry.collectedAtUtc,
      published_at_utc: entry.collectedAtUtc,
    });
    for (const client of [...this.clients]) {
      client.write(`id: metars:${version}\n`);
      client.write("event: live-feed-current\n");
      client.write(`data: ${payload}\n\n`);
    }
  }

  dropSseClients() {
    for (const client of [...this.clients]) {
      client.end();
    }
    this.clients.clear();
  }

  setEventsAvailable(available) {
    this.eventsAvailable = available;
  }

  handler(req, res) {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Headers", "content-type");
    res.setHeader("Access-Control-Allow-Methods", "GET, OPTIONS");
    if (req.method === "OPTIONS") {
      res.statusCode = 204;
      res.end();
      return;
    }
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    if (url.pathname === "/live-feeds/status.html") {
      sendText(res, 200, "text/html; charset=utf-8", "<!doctype html><title>test live feeds</title>");
      return;
    }
    if (url.pathname === liveFeedPath("current.json")) {
      this.requestCounts.current += 1;
      sendJson(res, this.currentManifest());
      return;
    }
    if (url.pathname === liveFeedPath("events")) {
      this.requestCounts.events += 1;
      if (!this.eventsAvailable) {
        sendText(res, 503, "text/plain; charset=utf-8", "events unavailable in test fixture");
        return;
      }
      res.writeHead(200, {
        "Content-Type": "text/event-stream; charset=utf-8",
        "Cache-Control": "no-cache",
        "Connection": "keep-alive",
        "Access-Control-Allow-Origin": "*",
      });
      res.write("retry: 60000\n");
      res.write(": ready\n\n");
      const catalog = this.currentManifest();
      res.write(`id: catalog:${catalog.generated_at_utc}\n`);
      res.write("event: live-feed-catalog\n");
      res.write(`data: ${JSON.stringify(catalog)}\n\n`);
      this.clients.add(res);
      req.on("close", () => {
        this.clients.delete(res);
      });
      return;
    }
    const version = metarVersionFromPath(url.pathname, "versions");
    if (version) {
      this.requestCounts.versions += 1;
      sendJson(res, this.manifestFor(version));
      return;
    }
    const stateVersion = metarVersionFromPath(url.pathname, "states");
    if (stateVersion) {
      this.requestCounts.states += 1;
      const entry = this.addVersion(stateVersion);
      sendBuffer(res, 200, "application/json; charset=utf-8", entry.stateBytes);
      return;
    }
    sendText(res, 404, "text/plain; charset=utf-8", `not found: ${url.pathname}`);
  }
}

function sendJson(res, body) {
  sendBuffer(res, 200, "application/json; charset=utf-8", Buffer.from(canonicalJsonString(body), "utf8"));
}

function sendText(res, status, contentType, body) {
  sendBuffer(res, status, contentType, Buffer.from(body, "utf8"));
}

function sendBuffer(res, status, contentType, body) {
  res.writeHead(status, {
    "Content-Type": contentType,
    "Content-Length": String(body.length),
  });
  res.end(body);
}

function listen(server, port) {
  return new Promise((resolve, reject) => {
    const onError = (error) => reject(error);
    server.once("error", onError);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", onError);
      const address = server.address();
      resolve(typeof address === "object" && address ? address.port : port);
    });
  });
}

function closeServer(server) {
  return new Promise((resolve) => server.close(() => resolve()));
}

function httpJson(url) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => {
        body += chunk;
      });
      res.on("end", () => {
        if ((res.statusCode ?? 500) >= 400) {
          reject(new Error(`${url} returned ${res.statusCode}: ${body.slice(0, 200)}`));
          return;
        }
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    });
    req.on("error", reject);
    req.setTimeout(E2E_TIMING.localReadyMs, () => {
      req.destroy(new Error(`timeout fetching ${url}`));
    });
  });
}

function httpText(url) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => {
        body += chunk;
      });
      res.on("end", () => {
        if ((res.statusCode ?? 500) >= 400) {
          reject(new Error(`${url} returned ${res.statusCode}: ${body.slice(0, 200)}`));
          return;
        }
        resolve(body);
      });
    });
    req.on("error", reject);
    req.setTimeout(E2E_TIMING.localReadyMs, () => {
      req.destroy(new Error(`timeout fetching ${url}`));
    });
  });
}

function httpReady(url) {
  return new Promise((resolve) => {
    const req = http.get(url, (res) => {
      res.resume();
      resolve((res.statusCode ?? 500) < 500);
    });
    req.on("error", () => resolve(false));
    req.setTimeout(E2E_TIMING.stabilityMs, () => {
      req.destroy();
      resolve(false);
    });
  });
}

export class CdpSocket {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.nextId = 1;
    this.pending = new Map();
    this.commandHistory = [];
    socket.on("data", (chunk) => this.onData(chunk));
    socket.on("error", (error) => this.rejectPending(error));
    socket.on("close", () => this.rejectPending(new Error("CDP socket closed")));
  }

  static async connect(webSocketDebuggerUrl) {
    const parsed = new URL(webSocketDebuggerUrl);
    const socket = net.connect(Number(parsed.port), parsed.hostname);
    await withTimeout(
      once(socket, "connect"),
      E2E_TIMING.localReadyMs,
      `CDP socket did not connect to ${webSocketDebuggerUrl}`,
    );
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
    await withTimeout(
      readUntil(socket, "\r\n\r\n"),
      E2E_TIMING.localReadyMs,
      `CDP websocket handshake timed out for ${webSocketDebuggerUrl}`,
    );
    return new CdpSocket(socket);
  }

  send(method, params = {}) {
    const id = this.nextId++;
    const record = { id, method, startedAt: performance.now(), status: "pending" };
    this.commandHistory.push(record);
    if (this.commandHistory.length > 20) this.commandHistory.shift();
    const finish = (status) => { record.status = status; record.finishedAt = performance.now(); };
    try {
      this.writeFrame(JSON.stringify({ id, method, params }));
    } catch (error) {
      finish("failed");
      return Promise.reject(error);
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) {
          finish("timed_out");
          reject(new Error(`CDP timeout: ${method}`));
        }
      }, CDP_COMMAND_TIMEOUT_MS);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          finish("completed");
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          finish("failed");
          reject(error);
        },
      });
    });
  }

  diagnostics() {
    return {
      command_timeout_ms: CDP_COMMAND_TIMEOUT_MS,
      pending_ids: [...this.pending.keys()],
      commands: this.commandHistory.map(record => ({
        id: record.id, method: record.method, status: record.status,
        elapsed_ms: Math.round((record.finishedAt ?? performance.now()) - record.startedAt),
      })),
    };
  }

  writeFrame(text) {
    if (this.socket.destroyed) {
      throw new Error("CDP socket is closed");
    }
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
        if (this.buffer.length < 10) return;
        const high = this.buffer.readUInt32BE(2);
        const low = this.buffer.readUInt32BE(6);
        if (high !== 0 || low > Number.MAX_SAFE_INTEGER) {
          throw new Error("CDP frame too large");
        }
        length = low;
        offset = 10;
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
      }
    }
  }

  close() {
    this.socket.end();
  }

  rejectPending(error) {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

function once(emitter, event) {
  return new Promise((resolve, reject) => {
    emitter.once(event, resolve);
    emitter.once("error", reject);
  });
}

function withTimeout(promise, timeoutMs, message) {
  let timer;
  return Promise.race([
    promise.finally(() => clearTimeout(timer)),
    new Promise((_resolve, reject) => {
      timer = setTimeout(() => reject(new Error(message)), timeoutMs);
    }),
  ]);
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

async function cdpEval(cdp, expression) {
  const result = await cdp.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`CDP evaluation failed: ${JSON.stringify(result.exceptionDetails)}`);
  }
  return result.result?.value;
}

async function liveFeedStatus(cdp) {
  return cdpEval(cdp, "window.__aerobagE2e?.liveFeeds?.() ?? null");
}

export async function waitForLiveFeedStatus(
  cdp,
  predicate,
  timeoutMs,
  message,
  intervalMs = E2E_TIMING.resourcePollIntervalMs,
) {
  return (await observeUntil(message, () => liveFeedStatus(cdp), {
    timeoutMs,
    intervalMs,
    accept: predicate,
  })).value;
}

function defaultWebAppDir() {
  const packagePath = path.join(process.cwd(), "package.json");
  try {
    const parsed = JSON.parse(fs.readFileSync(packagePath, "utf8"));
    if (parsed.name === "aerobag-web") {
      return process.cwd();
    }
  } catch (_error) {
    // Fall through to the source tree. The wrapper normally runs this from the
    // staged web workspace, where node_modules already exists.
  }
  return path.join(REPO_ROOT, "ui/web-app");
}

export function prepareWebApp({ webDist, runCommand = spawnSync, cwd = defaultWebAppDir() } = {}) {
  if (webDist) {
    if (!fs.existsSync(path.join(webDist, "index.html"))) {
      throw new Error(`web dist has no index.html: ${webDist}`);
    }
    return;
  }
  const result = runCommand("npm", ["run", "inner:prepare:dev"], {
    cwd,
    stdio: "inherit",
    timeout: WEB_BUILD_TIMEOUT_MS,
  });
  if (result.error) {
    throw new Error(`Web build preparation failed: ${result.error.message}`, { cause: result.error });
  }
  if (result.status !== 0) {
    throw new Error(`Web build preparation failed: code=${result.status} signal=${result.signal}`);
  }
}

export function createPrebuiltWebServer({ webDist, fixture }, liveFeed) {
  return createReleaseJourneyFixtureServer({ fixture, webDist, liveFeedProfile: "empty" }, {
    liveFeedHandler: (request, response) => liveFeed.handler(request, response),
  });
}

function startVite(webPort, liveFeedPort) {
  const cwd = defaultWebAppDir();
  const proc = spawn("npm", [
    "run",
    "inner:serve:dev",
    "--",
    "--host",
    "127.0.0.1",
    "--port",
    String(webPort),
    "--strictPort",
  ], {
    cwd,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      AEROBAG_LIVE_FEEDS_ORIGIN: `http://127.0.0.1:${liveFeedPort}`,
    },
  });
  proc.stdout.on("data", (chunk) => {
    if (/error|ready|local/i.test(chunk.toString("utf8"))) {
      process.stderr.write(chunk);
    }
  });
  proc.stderr.on("data", (chunk) => process.stderr.write(chunk));
  return proc;
}

async function stopProcess(proc) {
  if (!proc || proc.exitCode !== null) {
    return;
  }
  try {
    process.kill(-proc.pid, "SIGTERM");
  } catch (_error) {
    proc.kill("SIGTERM");
  }
  await withTimeout(
    once(proc, "exit").catch(() => {}),
    E2E_TIMING.localReadyMs,
    `process ${proc.pid} did not stop after SIGTERM`,
  ).catch(() => {});
  if (proc.exitCode === null) {
    try {
      process.kill(-proc.pid, "SIGKILL");
    } catch (_error) {
      proc.kill("SIGKILL");
    }
  }
}

function portFromUrl(url, fallback) {
  try {
    const parsed = new URL(url);
    if (parsed.port) return Number(parsed.port);
    return parsed.protocol === "https:" ? 443 : 80;
  } catch (_error) {
    return fallback;
  }
}

function withCacheBust(url, value) {
  const parsed = new URL(url);
  parsed.searchParams.set("aerobag_e2e_run", value);
  return parsed.toString();
}

function findChromePackage(serial) {
  for (const packageName of CHROME_PACKAGES) {
    const result = adbBestEffort(serial, ["shell", "pm", "path", packageName]);
    if (result.status === 0 && result.stdout.trim()) {
      return packageName;
    }
  }
  throw new Error(`Android Chrome/Chromium package not found. Tried: ${CHROME_PACKAGES.join(", ")}`);
}

function chromeRuntimeState(serial, packageName) {
  const processes = adbBestEffort(
    serial,
    ["shell", "pidof", packageName],
    { timeout: E2E_TIMING.localReadyMs },
  );
  const sockets = adbBestEffort(
    serial,
    ["shell", "cat", "/proc/net/unix"],
    { timeout: E2E_TIMING.localReadyMs },
  );
  return {
    process_running: processes.status === 0 && Boolean(processes.stdout.trim()),
    process_ids: processes.status === 0 ? processes.stdout.trim() : "",
    devtools_socket: sockets.status === 0 && sockets.stdout.includes("@chrome_devtools_remote"),
  };
}

async function waitForChromeDevtoolsSocket(serial, packageName) {
  let sawBrowserProcess = false;
  await observeUntil(
    "Android Chrome did not create chrome_devtools_remote",
    () => {
      const state = chromeRuntimeState(serial, packageName);
      sawBrowserProcess ||= state.process_running;
      if (sawBrowserProcess && !state.process_running) {
        throw new TerminalObservationError(
          "Android Chrome launch",
          "browser process exited before its DevTools socket became ready",
        );
      }
      return state.process_running && state.devtools_socket ? state : null;
    },
    {
      timeoutMs: E2E_TIMING.localResourceMs,
      intervalMs: E2E_TIMING.resourcePollIntervalMs,
    },
  );
}

export async function establishChromeRuntime({
  attempts = CHROME_LAUNCH_ATTEMPTS,
  launchAttempt,
  onAttemptFailure = () => {},
}) {
  const failures = [];
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await launchAttempt(attempt);
    } catch (error) {
      failures.push(error);
      if (attempt >= attempts) {
        const details = failures
          .map((failure, index) => `attempt ${index + 1}: ${failure.message}`)
          .join("; ");
        throw new Error(
          `Android Chrome failed to become ready after ${attempts} attempts (${details})`,
          { cause: error },
        );
      }
      await onAttemptFailure(error, attempt);
    }
  }
  throw new Error("Android Chrome launch attempts must be positive");
}

export function prepareChromeDevice(serial, {
  disposableEmulator = false, packageName = "com.android.chrome", runAdb = adb,
} = {}) {
  runAdb(serial, ["wait-for-device"]);
  if (!disposableEmulator) return;
  if (runAdb(serial, ["shell", "getprop", "ro.kernel.qemu"]).trim() !== "1") {
    throw new Error("Chrome test setup requires a disposable emulator, not a physical device");
  }
  // Chrome is bundled in this image, but the fixture journey needs no Google
  // services. Their boot-time ANRs kill Chrome through its font-provider bind.
  runAdb(serial, ["shell", "pm", "disable-user", "--user", "0", "com.google.android.gms"]);
  const disabled = runAdb(serial, ["shell", "pm", "list", "packages", "-d", "--user", "0", "com.google.android.gms"]);
  if (!disabled.split(/\r?\n/).includes("package:com.google.android.gms")) {
    throw new Error("Google Play Services is still enabled on the isolated Chrome emulator");
  }
  progress("isolated disposable emulator from Google Play Services");
  // Chrome's Android 13+ notification rationale is separate from --no-first-run.
  runAdb(serial, ["shell", "pm", "grant", packageName, "android.permission.POST_NOTIFICATIONS"]);
  const permissions = runAdb(serial, ["shell", "dumpsys", "package", packageName]);
  if (!/^\s*android\.permission\.POST_NOTIFICATIONS: granted=true(?:,|\s|$)/m.test(permissions)) {
    throw new Error(`Chrome notification permission was not granted for ${packageName}`);
  }
  progress("verified browser notification permission on disposable emulator");
}

export function chromeCommandLine({ disposableEmulator = false } = {}) {
  const flags = ["chrome", "--disable-fre", "--no-first-run", "--no-default-browser-check"];
  // Chrome 113's SurfaceControl compositor crashes on the API 34 software GPU.
  // Keep ordinary compositing enabled; this is only an emulator presentation path.
  if (disposableEmulator) flags.push("--disable-features=AndroidSurfaceControl");
  return flags.join(" ");
}

export async function prepareChromeLaunch(serial, options = {}) {
  prepareChromeDevice(serial, options);
  (options.wake ?? wakeAndUnlock)(serial);
  if (options.disposableEmulator) {
    await (options.waitForTheme ?? waitForChromeEmulatorTheme)(serial);
  }
}

export async function waitForChromeEmulatorTheme(serial, {
  runAdb = adb, timeoutMs = E2E_TIMING.startupMs, intervalMs = E2E_TIMING.resourcePollIntervalMs,
} = {}) {
  const startedAt = performance.now();
  const remainingMs = () => Math.max(1, Math.floor(timeoutMs - (performance.now() - startedAt)));
  progress("waiting for disposable emulator's initial theme publication");
  const { value: theme } = await observeUntil("Android initial theme publication", () => {
    const controller = runAdb(serial, ["shell", "dumpsys", "activity", "service",
      "com.android.systemui/.SystemUIService", "ThemeOverlayController"],
    { timeout: Math.min(E2E_TIMING.localResourceMs, remainingMs()) });
    const overlays = runAdb(serial, ["shell", "cmd", "overlay", "list", "--user", "0", "android"],
      { timeout: Math.min(E2E_TIMING.localResourceMs, remainingMs()) });
    const fields = Object.fromEntries([...controller.matchAll(/^\s*(m\w+)=(true|false)\s*$/gm)]
      .map(([, name, value]) => [name, value === "true"]));
    const registered = [...overlays.matchAll(/^\[([ x])\] com\.android\.systemui:(neutral|accent|dynamic)\s*$/gm)]
      .map(([, , name]) => name);
    return {
      monet: fields.mIsMonetEnabled ?? null,
      needs_creation: fields.mNeedsOverlayCreation ?? null,
      deferred: fields.mDeferredThemeEvaluation ?? null,
      registered,
    };
  }, {
    timeoutMs, intervalMs,
    accept: state => state.monet === true && state.needs_creation === false && state.deferred === false
      && ["neutral", "accent", "dynamic"].every(name => state.registered.includes(name)),
  });
  // Registry publication precedes queued ASSETS_PATHS notifications, which
  // recreate Chrome's activity and destroy its page target. Drain those messages
  // before launch, not all future broadcasts or an arbitrary quiet interval.
  const barrier = runAdb(serial, ["shell", "am", "wait-for-broadcast-barrier",
    "--flush-broadcast-loopers", "--flush-application-threads"], { timeout: remainingMs() });
  if (!/^Test barrier passed\s*$/m.test(barrier) || !/^Finished application barriers!\s*$/m.test(barrier)) {
    throw new Error(`Android theme configuration barrier did not complete: ${barrier}`);
  }
  progress(`initial theme published and configuration notifications drained: ${JSON.stringify(theme)}`);
}

async function launchAndroidChrome(serial, url, cdpPort, { disposableEmulator = false } = {}) {
  adb(serial, ["wait-for-device"]);
  progress("locating Android Chrome package");
  const packageName = findChromePackage(serial);
  await prepareChromeLaunch(serial, { disposableEmulator, packageName });
  adbBestEffort(serial, ["shell", "am", "force-stop", packageName]);
  adbBestEffort(serial, ["forward", "--remove", `tcp:${cdpPort}`]);
  adbBestEffort(serial, ["shell", "rm", "-f", "/data/local/tmp/chrome-command-line"]);
  adb(serial, [
    "shell",
    `printf '%s\\n' '${chromeCommandLine({ disposableEmulator })}' > /data/local/tmp/chrome-command-line`,
  ]);
  await establishChromeRuntime({
    launchAttempt: async (attempt) => {
      progress(`launching ${packageName} (attempt ${attempt}/${CHROME_LAUNCH_ATTEMPTS})`);
      adbBestEffort(serial, ["shell", "am", "force-stop", packageName]);
      adbBestEffort(serial, ["forward", "--remove", `tcp:${cdpPort}`]);
      adb(serial, ["shell", "am", "start", "-a", "android.intent.action.VIEW", "-d", url, packageName]);
      progress("waiting for Chrome browser process and DevTools socket");
      await waitForChromeDevtoolsSocket(serial, packageName);
      progress(`forwarding Chrome CDP to tcp:${cdpPort}`);
      adb(serial, ["forward", `tcp:${cdpPort}`, "localabstract:chrome_devtools_remote"]);
      progress("waiting for Chrome CDP HTTP endpoint");
      await observeUntil(
        `Android Chrome did not expose CDP on forwarded port ${cdpPort}`,
        async () => {
          const state = chromeRuntimeState(serial, packageName);
          if (!state.process_running || !state.devtools_socket) {
            throw new TerminalObservationError(
              "Android Chrome launch",
              "browser process or DevTools socket disappeared before CDP became ready",
            );
          }
          try {
            await httpJson(`http://127.0.0.1:${cdpPort}/json/version`);
            return state;
          } catch (_error) {
            return null;
          }
        },
        {
          timeoutMs: E2E_TIMING.localReadyMs,
          intervalMs: E2E_TIMING.resourcePollIntervalMs,
        },
      );
    },
    onAttemptFailure: async (error, attempt) => {
      progress(`Chrome launch attempt ${attempt} failed; relaunching: ${error.message}`);
    },
  });
  return { packageName };
}

async function connectChromePage(cdpPort, url) {
  progress("reading Chrome page targets");
  const tabs = await httpJson(`http://127.0.0.1:${cdpPort}/json`);
  const tab = tabs.find((entry) => entry.type === "page" && entry.url === url && entry.webSocketDebuggerUrl);
  if (!tab) {
    throw new Error(`Chrome did not expose a debuggable page target: ${JSON.stringify(tabs)}`);
  }
  await httpText(`http://127.0.0.1:${cdpPort}/json/activate/${tab.id}`);
  progress("connecting to Chrome page target");
  const cdp = await CdpSocket.connect(tab.webSocketDebuggerUrl);
  try {
    await cdp.send("Page.enable");
    await cdp.send("Runtime.enable");
    await cdp.send("Page.bringToFront");
    return cdp;
  } catch (error) {
    cdp.close();
    throw error;
  }
}

export async function openChromeJourneyPage({
  serial, url, cdpPort, disposableEmulator = false,
  launch = launchAndroidChrome, connect = connectChromePage,
  beforeNavigate = async () => {},
}) {
  const { packageName } = await launch(serial, "about:blank", cdpPort, { disposableEmulator });
  const cdp = await connect(cdpPort, "about:blank");
  cdp.androidPackageName = packageName;
  try {
    const version = await cdp.send("Browser.getVersion");
    progress(`browser ${version.product}; waiting for blank-page render before loading Aerobag`);
    const ready = await cdpEval(cdp, `new Promise(resolve => requestAnimationFrame(() => resolve({
      url: location.href, visibility: document.visibilityState
    })))`);
    if (ready?.url !== "about:blank" || ready?.visibility !== "visible") {
      throw new Error(`Unexpected browser readiness: ${JSON.stringify(ready)}`);
    }
    await cdp.send("Network.enable");
    await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
    // This network journey needs no GPS. Resolve the site's permission before
    // startup rather than letting an unattended native prompt become test input.
    if (disposableEmulator) {
      await cdp.send("Browser.setPermission", {
        origin: new URL(url).origin, permission: { name: "geolocation" }, setting: "denied",
      });
    }
    await beforeNavigate();
    const navigation = await cdp.send("Page.navigate", { url });
    if (navigation.errorText || navigation.isDownload || !navigation.frameId) {
      throw new Error(`Chrome navigation failed: ${JSON.stringify(navigation)}`);
    }
    progress(`navigated Chrome to ${url}`);
    return cdp;
  } catch (error) {
    cdp.close();
    throw error;
  }
}

async function setChromeOffline(cdp, offline) {
  await cdp.send("Network.emulateNetworkConditions", {
    offline,
    latency: offline ? 0 : 20,
    downloadThroughput: offline ? 0 : 5_000_000,
    uploadThroughput: offline ? 0 : 2_000_000,
    connectionType: offline ? "none" : "wifi",
  });
}

async function run(args) {
  if (!args.serial) {
    throw new Error("missing --serial or ANDROID_SERIAL");
  }

  if (!args.webUrl) {
    progress(args.webDist ? "using immutable web bundle" : "preparing generated web sources and WASM before server readiness");
    prepareWebApp({ webDist: args.webDist });
  }

  const liveFeed = new ScriptedLiveFeedServer();
  const liveFeedHttp = args.webDist
    ? createPrebuiltWebServer(args, liveFeed)
    : http.createServer((req, res) => liveFeed.handler(req, res));
  const liveFeedPort = await listen(liveFeedHttp, args.webDist ? args.webPort : args.liveFeedPort);
  let vite = null;
  let cdp = null;
  let startupTrace = null;
  const artifactDir = process.env.AEROBAG_E2E_ARTIFACT_DIR;
  let traceStatus = { state: "not_started" };
  const finishStartupTrace = async () => {
    if (startupTrace) {
      const trace = startupTrace;
      startupTrace = null;
      try { traceStatus = { state: "captured", ...await trace.stop() }; }
      catch (error) { traceStatus = { state: "failed", error: error.message }; }
    }
    if (artifactDir) {
      try {
        fs.mkdirSync(artifactDir, { recursive: true });
        fs.writeFileSync(path.join(artifactDir, "chrome-startup-trace-status.json"), `${JSON.stringify(traceStatus, null, 2)}\n`);
      } catch (error) { progress(`startup trace status could not be saved: ${error.message}`); }
    }
    if (traceStatus.state === "failed") progress(`startup trace unavailable: ${traceStatus.error}`);
  };
  const baseWebUrl = args.webUrl || `http://127.0.0.1:${args.webDist ? liveFeedPort : args.webPort}/`;
  const webUrl = withCacheBust(baseWebUrl, `${Date.now()}-${process.pid}`);
  const webPort = portFromUrl(baseWebUrl, args.webPort);
  const result = {
    test: "android.chrome.live-feed-network-recovery",
    web_url: webUrl,
    live_feed_origin: `http://127.0.0.1:${liveFeedPort}`,
    serial: args.serial,
    checks: {},
  };

  try {
    if (!args.webUrl && !args.webDist) {
      progress(`starting Vite on tcp:${webPort}`);
      vite = startVite(webPort, liveFeedPort);
      await waitFor(
        () => {
          if (vite.exitCode !== null || vite.signalCode !== null) {
            throw new TerminalObservationError(
              "Vite readiness", `Vite exited early: code=${vite.exitCode} signal=${vite.signalCode}`,
            );
          }
          return httpReady(webUrl);
        },
        E2E_TIMING.bulkOperationMs,
        `Vite did not become ready at ${webUrl}`,
        E2E_TIMING.resourcePollIntervalMs,
      );
    }

    progress(`preparing adb device ${args.serial}`);
    adb(args.serial, ["wait-for-device"]);
    adbBestEffort(args.serial, ["reverse", `tcp:${webPort}`, `tcp:${webPort}`]);
    adbBestEffort(args.serial, ["reverse", `tcp:${liveFeedPort}`, `tcp:${liveFeedPort}`]);

    cdp = await openChromeJourneyPage({
      serial: args.serial, url: webUrl, cdpPort: args.cdpPort, disposableEmulator: args.disposableEmulator,
      beforeNavigate: async () => {
        if (!artifactDir) return;
        try {
          startupTrace = await connectChromeStartupTrace(args.cdpPort, path.join(artifactDir, "chrome-startup-trace.json"));
          traceStatus = { state: "recording" };
        } catch (error) { traceStatus = { state: "failed", error: error.message }; }
      },
    });
    progress("waiting for web live-feed hook");
    await waitFor(
      async () => Boolean(await cdpEval(cdp, "typeof window.__aerobagE2e?.liveFeeds === 'function'")),
      E2E_TIMING.externalConsistencyMs,
      "web live-feed E2E hook did not become available in Android Chrome",
      E2E_TIMING.resourcePollIntervalMs,
    );

    progress("waiting for initial METAR v1");
    await waitForLiveFeedStatus(
      cdp,
      (status) => status?.connection?.value === "CONNECTED" && status?.product_versions?.metars === "v1",
      E2E_TIMING.externalConsistencyMs,
      "initial live-feed connection did not reach METAR v1",
      E2E_TIMING.resourcePollIntervalMs,
    );
    result.checks.initial_metar_version = "v1";
    await finishStartupTrace();

    progress("publishing METAR v2 over SSE");
    liveFeed.publish("v2");
    await waitForLiveFeedStatus(
      cdp,
      (status) => status?.product_versions?.metars === "v2",
      E2E_TIMING.cloudConsistencyMs,
      "SSE update did not advance METARs to v2",
      E2E_TIMING.resourcePollIntervalMs,
    );
    result.checks.sse_metar_version = "v2";

    progress("forcing Chrome offline");
    liveFeed.setEventsAvailable(false);
    await setChromeOffline(cdp, true);
    liveFeed.dropSseClients();
    await waitForLiveFeedStatus(
      cdp,
      (status) => status?.navigator_online === false
        && status?.connection?.facts?.some((fact) => fact.label === "Last error"),
      E2E_TIMING.userTransitionDeadlineMs,
      "offline transition did not schedule live-feed reconnect",
      E2E_TIMING.resourcePollIntervalMs,
    );

    progress("publishing METAR v3 while offline");
    liveFeed.publish("v3");
    liveFeed.setEventsAvailable(true);
    const beforeOnlineEventStreams = liveFeed.requestCounts.events;
    await setChromeOffline(cdp, false);
    await cdpEval(cdp, "window.dispatchEvent(new Event('online')); true");
    progress("waiting for online to interrupt reconnect backoff");
    await waitFor(
      () => liveFeed.requestCounts.events > beforeOnlineEventStreams,
      E2E_TIMING.userTransitionDeadlineMs,
      "online event did not interrupt pending live-feed reconnect backoff",
      E2E_TIMING.stabilityPollIntervalMs,
    );
    result.checks.online_interrupted_backoff = true;

    progress("waiting for METAR v3 recovery");
    await waitForLiveFeedStatus(
      cdp,
      (status) => status?.product_versions?.metars === "v3",
      E2E_TIMING.cloudConsistencyMs,
      "live-feed SSE catalog after online did not advance METARs to v3",
      E2E_TIMING.resourcePollIntervalMs,
    );
    const finalStatus = await liveFeedStatus(cdp);
    result.checks.recovered_metar_version = "v3";
    result.checks.active_event_sources = finalStatus?.adapter?.active_event_sources ?? null;
    result.checks.current_manifest_requests = liveFeed.requestCounts.current;
    result.checks.event_stream_requests = liveFeed.requestCounts.events;
    if (liveFeed.requestCounts.current !== 0) {
      throw new Error(`application client unexpectedly fetched current.json ${liveFeed.requestCounts.current} time(s)`);
    }
    if (finalStatus?.adapter && (finalStatus.adapter.active_event_sources ?? 0) > 1) {
      throw new Error(`expected at most one active EventSource, got ${finalStatus.adapter.active_event_sources}`);
    }
    return result;
  } catch (error) {
    await finishStartupTrace();
    if (artifactDir) {
      try {
        fs.mkdirSync(artifactDir, { recursive: true });
        fs.writeFileSync(path.join(artifactDir, "failure.json"), `${JSON.stringify({
          test: result.test, error: error.stack ?? String(error),
          cause: error.cause?.stack ?? null, observation: error.diagnostics ?? null,
          cdp: cdp?.diagnostics() ?? null, live_feed_requests: liveFeed.requestCounts, startup_trace: traceStatus,
        }, null, 2)}\n`);
        captureAndroidFailureDiagnostics(
          args.serial, artifactDir, result.test,
          { packageName: cdp?.androidPackageName ?? findChromePackage(args.serial) },
        );
      } catch (diagnosticError) {
        progress(`failure diagnostics could not complete: ${diagnosticError.message}`);
      }
    }
    const counts = JSON.stringify(liveFeed.requestCounts);
    if (error instanceof Error) {
      error.message = `${error.message}; live feed request counts: ${counts}`;
    }
    throw error;
  } finally {
    progress("cleaning up Android Chrome live-feed E2E");
    cdp?.close();
    adbBestEffort(args.serial, ["forward", "--remove", `tcp:${args.cdpPort}`]);
    await stopProcess(vite);
    liveFeed.dropSseClients();
    await withTimeout(
      closeServer(liveFeedHttp),
      E2E_TIMING.localReadyMs,
      "live-feed fixture server did not close",
    ).catch(() => {});
  }
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  const args = parseArgs(process.argv);
  if (args.help) {
    usage();
    process.exit(0);
  }

  run(args).then((result) => {
    if (args.json) {
      console.log(JSON.stringify(result, null, 2));
    } else {
      console.log(`PASS ${result.test}`);
      console.log(`  METAR versions: ${result.checks.initial_metar_version} -> ${result.checks.sse_metar_version} -> ${result.checks.recovered_metar_version}`);
      console.log(`  client current.json requests: ${result.checks.current_manifest_requests}`);
      console.log(`  event streams opened: ${result.checks.event_stream_requests}`);
    }
  }).catch((error) => {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
    if (error?.diagnostics) console.error(JSON.stringify(error.diagnostics, null, 2));
    process.exit(1);
  });
}
