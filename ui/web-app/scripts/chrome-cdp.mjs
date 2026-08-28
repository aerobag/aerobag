// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawn } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const WebSocket = require("ws");

export function launchChrome({
  chromeBin = process.env.CHROME_BIN ?? "google-chrome-stable",
  userDataDir,
  width = 1200,
  height = 1000,
  transport = "pipe",
  netLogPath = process.env.AEROBAG_CHROME_NET_LOG?.replace(
    "{repeat}",
    process.env.AEROBAG_E2E_REPEAT_INDEX ?? "1",
  ) ?? "",
} = {}) {
  if (!userDataDir) {
    throw new Error("launchChrome requires userDataDir");
  }
  if (transport !== "websocket" && transport !== "pipe") {
    throw new Error(`unsupported Chrome DevTools transport: ${transport}`);
  }
  return new Promise((resolve, reject) => {
    const transportArgs = transport === "pipe"
      ? ["--remote-debugging-pipe"]
      : ["--remote-debugging-port=0"];
    const netLogArgs = netLogPath
      ? [`--log-net-log=${netLogPath}`, "--net-log-capture-mode=IncludeSensitive"]
      : [];
    const child = spawn(chromeBin, [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      ...transportArgs,
      ...netLogArgs,
      `--user-data-dir=${userDataDir}`,
      `--window-size=${width},${height}`,
      "about:blank",
    ], {
      // Chrome's pipe transport reserves descriptors 3 and 4 for CDP input
      // and output. It avoids opening a DevTools listener on constrained hosts.
      stdio: transport === "pipe"
        ? ["ignore", "ignore", "pipe", "pipe", "pipe"]
        : ["ignore", "ignore", "pipe"],
    });
    let stderr = "";
    const timeout = setTimeout(() => {
      reject(new Error(`timed out waiting for Chrome DevTools endpoint; stderr=${stderr}`));
    }, 15000);
    child.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    if (transport === "pipe") {
      child.once("spawn", () => {
        clearTimeout(timeout);
        resolve({
          process: child,
          endpoint: {
            pipeWrite: child.stdio[3],
            pipeRead: child.stdio[4],
          },
          pipeWrite: child.stdio[3],
          pipeRead: child.stdio[4],
        });
      });
    }
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString("utf8");
      if (transport === "pipe") return;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timeout);
        resolve({ process: child, endpoint: match[1], wsUrl: match[1] });
      }
    });
    child.on("exit", (code, signal) => {
      clearTimeout(timeout);
      reject(new Error(
        `Chrome exited before DevTools was ready: code=${code} signal=${signal} stderr=${stderr}`,
      ));
    });
  });
}

export async function connectToBrowser(endpoint) {
  const client = new CdpClient(endpoint);
  await client.open();
  // Pipe transport is available as soon as Chrome is spawned, before the
  // browser process has necessarily finished initializing. Make readiness an
  // explicit CDP operation rather than inferring it from stderr or a port.
  await client.send("Browser.getVersion", {}, undefined, 30_000);
  return {
    close: () => client.close(),
    async createPage() {
      const created = await client.send("Target.createTarget", { url: "about:blank" });
      const attached = await client.send("Target.attachToTarget", {
        targetId: created.targetId,
        flatten: true,
      });
      return new CdpPage(client, attached.sessionId);
    },
  };
}

export async function stopProcess(child, timeoutMs = 2000) {
  if (!child || child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  child.kill("SIGTERM");
  if (await waitForProcessExit(child, timeoutMs)) {
    return;
  }
  child.kill("SIGKILL");
  await waitForProcessExit(child, 1000);
}

export async function waitFor(check, timeoutMs, message, intervalMs = 100) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await check();
      if (value) {
        return value;
      }
    } catch (error) {
      lastError = error;
    }
    await sleep(intervalMs);
  }
  throw new Error(`${message}${lastError ? `: ${lastError.message}` : ""}`);
}

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

class CdpPage {
  constructor(client, sessionId) {
    this.client = client;
    this.sessionId = sessionId;
    this.diagnostics = [];
    this.networkRequests = new Map();
    this.loadPromise = null;
    this.installDiagnosticListeners(sessionId, "page");
    this.client.onEvent(sessionId, "Target.attachedToTarget", (params) => {
      const childSessionId = params.sessionId;
      const target = params.targetInfo?.type ?? "child";
      this.installDiagnosticListeners(childSessionId, target);
      // Enabling CDP Network after a worker starts can cancel that worker's
      // active Fetch batch. Worker failures are captured through Runtime; use
      // Chrome's optional netlog for transport-level diagnostics.
      this.client.send("Runtime.enable", {}, childSessionId).catch((error) => {
        this.diagnostics.push({ method: "Target.diagnosticsFailed", target, error: error.message });
      });
    });
  }

  installDiagnosticListeners(sessionId, target) {
    this.client.onEvent(sessionId, "Runtime.exceptionThrown", (params) => {
      this.diagnostics.push({
        method: "Runtime.exceptionThrown",
        target,
        exception: params.exceptionDetails,
      });
    });
    this.client.onEvent(sessionId, "Runtime.consoleAPICalled", (params) => {
      if (params.type === "error" || params.type === "warning") {
        this.diagnostics.push({
          method: "Runtime.consoleAPICalled",
          target,
          type: params.type,
          args: params.args,
        });
      }
    });
    this.client.onEvent(sessionId, "Network.requestWillBeSent", (params) => {
      this.networkRequests.set(`${sessionId}:${params.requestId}`, {
        url: params.request?.url,
        timestamp: params.timestamp,
      });
    });
    this.client.onEvent(sessionId, "Network.loadingFailed", (params) => {
      const requestKey = `${sessionId}:${params.requestId}`;
      const request = this.networkRequests.get(requestKey);
      this.diagnostics.push({
        method: "Network.loadingFailed",
        target,
        url: request?.url,
        requestTimestamp: request?.timestamp,
        ...params,
      });
      this.networkRequests.delete(requestKey);
    });
    this.client.onEvent(sessionId, "Network.responseReceived", (params) => {
      if ((params.response?.status ?? 0) >= 400) {
        this.diagnostics.push({
          method: "Network.responseReceived",
          target,
          status: params.response.status,
          url: params.response.url,
        });
      }
      this.networkRequests.delete(`${sessionId}:${params.requestId}`);
    });
  }

  async enableChildTargetDiagnostics() {
    await this.send("Target.setAutoAttach", {
      autoAttach: true,
      waitForDebuggerOnStart: false,
      flatten: true,
    });
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
  }

  async routeOrigin(sourceOrigin, targetOrigin) {
    const source = new URL(sourceOrigin).origin;
    const target = new URL(targetOrigin).origin;
    this.client.onEvent(this.sessionId, "Fetch.requestPaused", (params) => {
      const original = new URL(params.request.url);
      const replacement = original.origin === source
        ? `${target}${original.pathname}${original.search}${original.hash}`
        : original.toString();
      this.send("Fetch.continueRequest", {
        requestId: params.requestId,
        url: replacement,
      }).catch((error) => {
        this.diagnostics.push({
          method: "Fetch.continueRequestFailed",
          source,
          target,
          url: original.toString(),
          error: error.message,
        });
      });
    });
    await this.send("Fetch.enable", {
      patterns: [{ urlPattern: `${source}/*`, requestStage: "Request" }],
    });
  }

  async navigate(url) {
    this.loadPromise = new Promise((resolve) => {
      const finish = () => {
        this.client.offEvent(this.sessionId, "Page.loadEventFired", finish);
        resolve();
      };
      this.client.onEvent(this.sessionId, "Page.loadEventFired", finish);
    });
    await this.send("Page.navigate", { url });
  }

  async waitForLoad() {
    await this.loadPromise;
  }

  async evaluate(expression) {
    const response = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (response.exceptionDetails) {
      throw new Error(
        response.exceptionDetails.exception?.description
          ?? response.exceptionDetails.text
          ?? "browser evaluation failed",
      );
    }
    return response.result?.value;
  }
}

export class CdpClient {
  constructor(endpoint) {
    this.endpoint = endpoint;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.closedError = null;
  }

  open() {
    if (typeof this.endpoint !== "string") {
      this.pipeWrite = this.endpoint.pipeWrite;
      this.pipeBuffer = Buffer.alloc(0);
      this.pipeDataHandler = (chunk) => this.handlePipeData(chunk);
      this.pipeErrorHandler = (error) => this.close(error);
      this.pipeClosedHandler = () => this.close(new Error("CDP pipe closed"));
      this.endpoint.pipeRead.on("data", this.pipeDataHandler);
      this.endpoint.pipeRead.on("error", this.pipeErrorHandler);
      this.endpoint.pipeRead.on("end", this.pipeClosedHandler);
      this.pipeWrite.on("error", this.pipeErrorHandler);
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.endpoint);
      const startupError = (event) => reject(event.error ?? new Error("CDP websocket failed to open"));
      this.ws.addEventListener("error", startupError, { once: true });
      this.ws.addEventListener("open", () => {
        this.ws.removeEventListener("error", startupError);
        this.ws.addEventListener("error", (event) => {
          this.close(event.error ?? new Error("CDP websocket failed"));
        });
        this.ws.addEventListener("close", () => {
          this.close(new Error("CDP websocket closed"));
        });
        resolve();
      }, { once: true });
      this.ws.addEventListener("message", (event) => this.handleMessage(event.data));
    });
  }

  close(error = undefined) {
    if (this.closedError) return;
    this.closedError = error ?? new Error("CDP connection closed");
    this.listeners.clear();
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timeout);
      pending.reject(new Error(
        `${this.closedError.message} while request ${id} was pending`,
        { cause: this.closedError },
      ));
    }
    this.pending.clear();
    this.ws?.close();
    if (this.pipeWrite && !this.pipeWrite.destroyed && !this.pipeWrite.writableEnded) {
      this.pipeWrite.end();
    }
  }

  send(method, params = {}, sessionId = undefined, timeoutMs = 15_000) {
    if (this.closedError) {
      return Promise.reject(new Error(
        `CDP request ${method} rejected: ${this.closedError.message}`,
        { cause: this.closedError },
      ));
    }
    const id = this.nextId++;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP request timed out: ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
      const encoded = JSON.stringify(message);
      try {
        if (this.pipeWrite) {
          if (this.pipeWrite.destroyed || this.pipeWrite.writableEnded) {
            throw new Error("CDP pipe is not writable");
          }
          this.pipeWrite.write(`${encoded}\0`, (error) => {
            if (!error || !this.pending.has(id)) return;
            clearTimeout(timeout);
            this.pending.delete(id);
            reject(error);
          });
        } else {
          if (this.ws.readyState !== WebSocket.OPEN) {
            throw new Error("CDP websocket is not open");
          }
          this.ws.send(encoded);
        }
      } catch (error) {
        clearTimeout(timeout);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  onEvent(sessionId, method, handler) {
    const key = `${sessionId}:${method}`;
    const handlers = this.listeners.get(key) ?? new Set();
    handlers.add(handler);
    this.listeners.set(key, handlers);
  }

  offEvent(sessionId, method, handler) {
    this.listeners.get(`${sessionId}:${method}`)?.delete(handler);
  }

  handlePipeData(chunk) {
    if (this.closedError) return;
    this.pipeBuffer = Buffer.concat([this.pipeBuffer, chunk]);
    for (;;) {
      const delimiter = this.pipeBuffer.indexOf(0);
      if (delimiter < 0) return;
      const message = this.pipeBuffer.subarray(0, delimiter).toString("utf8");
      this.pipeBuffer = this.pipeBuffer.subarray(delimiter + 1);
      if (message) this.handleMessage(message);
    }
  }

  handleMessage(data) {
    if (this.closedError) return;
    const message = JSON.parse(data);
    if (message.id && this.pending.has(message.id)) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      clearTimeout(pending.timeout);
      if (message.error) {
        pending.reject(new Error(JSON.stringify(message.error)));
      } else {
        pending.resolve(message.result ?? {});
      }
      return;
    }
    const key = `${message.sessionId ?? ""}:${message.method ?? ""}`;
    for (const handler of this.listeners.get(key) ?? []) {
      handler(message.params ?? {});
    }
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
