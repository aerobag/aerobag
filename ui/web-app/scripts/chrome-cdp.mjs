// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { spawn } from "node:child_process";
import { requireWebDependency } from "./web-workspace-require.mjs";

const WebSocket = requireWebDependency("ws");

export class CdpProtocolError extends Error {
  constructor(method, error) {
    super(`${method}: ${JSON.stringify(error)}`);
    this.name = "CdpProtocolError";
    this.method = method;
    this.code = error.code;
    this.detail = error.message;
  }
}

export function chromeProcessDiagnostics(chrome) {
  return {
    pid: chrome?.process?.pid ?? null,
    exit_code: chrome?.process?.exitCode ?? null,
    signal_code: chrome?.process?.signalCode ?? null,
    spawn_arguments: chrome?.process?.spawnargs ?? [],
    stderr: chrome?.getStderr?.() ?? "",
  };
}

export function launchChrome({
  chromeBin = process.env.CHROME_BIN ?? "google-chrome-stable",
  userDataDir,
  width = 1200,
  height = 1000,
  headless = true,
  transport = "pipe",
  env = process.env,
  onSpawn = null,
  startupTimeoutMs = 15_000,
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
      ? [`--log-net-log=${netLogPath}`, "--net-log-capture-mode=Default"]
      : [];
    const headlessArgs = headless ? ["--headless=new"] : [];
    const child = spawn(chromeBin, [
      ...headlessArgs,
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
      env,
    });
    let stderr = "";
    let settled = false;
    const handle = { process: child, getStderr: () => stderr };
    const failed = async (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      error.chrome = handle;
      await stopProcess(child);
      reject(error);
    };
    const timeout = setTimeout(() => {
      void failed(new Error(`timed out waiting for Chrome DevTools endpoint; stderr=${stderr}`));
    }, startupTimeoutMs);
    child.on("error", (error) => {
      void failed(error);
    });
    if (transport === "pipe") {
      child.once("spawn", () => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        resolve({
          process: child,
          endpoint: {
            pipeWrite: child.stdio[3],
            pipeRead: child.stdio[4],
            process: child,
          },
          pipeWrite: child.stdio[3],
          pipeRead: child.stdio[4],
          getStderr: () => stderr,
        });
      });
    }
    child.stderr.on("data", (chunk) => {
      stderr = (stderr + chunk.toString("utf8")).slice(-65_536);
      if (transport === "pipe") return;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match && !settled) {
        settled = true;
        clearTimeout(timeout);
        resolve({
          process: child,
          endpoint: match[1],
          wsUrl: match[1],
          getStderr: () => stderr,
        });
      }
    });
    child.on("exit", (code, signal) => {
      void failed(new Error(
        `Chrome exited before DevTools was ready: code=${code} signal=${signal} stderr=${stderr}`,
      ));
    });
    try { onSpawn?.(child); } catch (error) { void failed(error); }
  });
}

export async function connectToBrowser(endpoint) {
  const client = new CdpClient(endpoint);
  try {
    await client.open();
    // Pipe transport is available as soon as Chrome is spawned, before the
    // browser process has necessarily finished initializing. Make readiness an
    // explicit CDP operation rather than inferring it from stderr or a port.
    await client.send("Browser.getVersion", {}, undefined, 30_000);
    return new CdpBrowser(client);
  } catch (error) {
    client.close(error);
    throw error;
  }
}

export class CdpBrowser {
  constructor(client) {
    this.client = client;
  }

  close() {
    return this.client.close();
  }

  async createBrowserContext() {
    const { browserContextId } = await this.client.send("Target.createBrowserContext", {
      disposeOnDetach: true,
    });
    return browserContextId;
  }

  async disposeBrowserContext(browserContextId) {
    await this.client.send("Target.disposeBrowserContext", { browserContextId });
  }

  async createPage({ browserContextId } = {}) {
    const created = await this.client.send("Target.createTarget", {
      url: "about:blank",
      ...(browserContextId ? { browserContextId } : {}),
    });
    const attached = await this.client.send("Target.attachToTarget", {
      targetId: created.targetId,
      flatten: true,
    });
    return new CdpPage(this.client, attached.sessionId, created.targetId, browserContextId);
  }
}

export async function stopProcess(child, timeoutMs = 2000) {
  if (!child?.pid || child.exitCode !== null || child.signalCode !== null) {
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

export class CdpPage {
  constructor(client, sessionId, targetId, browserContextId = undefined) {
    this.client = client;
    this.sessionId = sessionId;
    this.targetId = targetId;
    this.browserContextId = browserContextId;
    this.diagnostics = [];
    this.networkRequests = new Map();
    this.loadPromise = null;
    this.ownedEvents = [];
    this.installDiagnosticListeners(sessionId, "page");
    this.onEvent(sessionId, "Target.attachedToTarget", (params) => {
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

  onEvent(sessionId, method, handler) {
    this.client.onEvent(sessionId, method, handler);
    this.ownedEvents.push([sessionId, method, handler]);
  }

  dispose() {
    this.cancelLoad?.(new Error("page disposed"));
    for (const [sessionId, method, handler] of this.ownedEvents) {
      this.client.offEvent(sessionId, method, handler);
    }
    this.ownedEvents = [];
    this.networkRequests.clear();
  }

  installDiagnosticListeners(sessionId, target) {
    this.onEvent(sessionId, "Log.entryAdded", (params) => {
      if (["error", "warning"].includes(params.entry?.level)) {
        this.diagnostics.push({ method: "Log.entryAdded", target, entry: params.entry });
      }
    });
    this.onEvent(sessionId, "Runtime.exceptionThrown", (params) => {
      this.diagnostics.push({
        method: "Runtime.exceptionThrown",
        target,
        exception: params.exceptionDetails,
      });
    });
    this.onEvent(sessionId, "Runtime.consoleAPICalled", (params) => {
      if (params.type === "error" || params.type === "warning") {
        this.diagnostics.push({
          method: "Runtime.consoleAPICalled",
          target,
          type: params.type,
          args: params.args,
        });
      }
    });
    this.onEvent(sessionId, "Network.requestWillBeSent", (params) => {
      this.networkRequests.set(`${sessionId}:${params.requestId}`, {
        url: params.request?.url,
        timestamp: params.timestamp,
      });
    });
    this.onEvent(sessionId, "Network.loadingFailed", (params) => {
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
    this.onEvent(sessionId, "Network.responseReceived", (params) => {
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

  async closeForReset(timeoutMs) {
    const targetId = this.targetId;
    const deadline = performance.now() + timeoutMs;
    await this.client.send("Target.closeTarget", { targetId }, undefined, timeoutMs);
    while (performance.now() < deadline) {
      const { targetInfos } = await this.client.send("Target.getTargets", {}, undefined,
        Math.max(1, deadline - performance.now()));
      if (performance.now() >= deadline) break;
      const pageExists = targetInfos.some((target) => target.targetId === targetId);
      const dedicatedWorkersExist = targetInfos.some((target) => target.type === "worker" &&
        (!this.browserContextId || target.browserContextId === this.browserContextId));
      if (!pageExists && !dedicatedWorkersExist) {
        this.dispose();
        return;
      }
      await sleep(Math.min(10, Math.max(0, deadline - performance.now())));
    }
    throw new Error("old page or dedicated worker survived browser reset");
  }

  send(method, params = {}, timeoutMs = 15_000) {
    return this.client.send(method, params, this.sessionId, timeoutMs);
  }

  async routeOrigin(sourceOrigin, targetOrigin) {
    const source = new URL(sourceOrigin).origin;
    const target = new URL(targetOrigin).origin;
    this.onEvent(this.sessionId, "Fetch.requestPaused", (params) => {
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

  async navigate(url, { timeoutMs = 30_000 } = {}) {
    this.cancelLoad?.(new Error("navigation superseded"));
    this.loadPromise = null;
    const deadline = performance.now() + timeoutMs;
    await this.send("Page.setLifecycleEventsEnabled", { enabled: true }, timeoutMs);
    let navigation = null;
    const earlyLoads = [];
    let finish, unsubscribe = () => {};
    const remaining = Math.max(1, Math.ceil(deadline - performance.now()));
    const timer = setTimeout(() => finish(new Error(`Page load timed out for ${url}`)), remaining);
    const loaded = (event) => {
      if (event.name !== "load") return;
      if (!navigation) { earlyLoads.push(event); return; }
      if (event.loaderId === navigation.loaderId && event.frameId === navigation.frameId) finish();
    };
    this.loadPromise = new Promise((resolve, reject) => {
      let settled = false;
      finish = (error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.client.offEvent(this.sessionId, "Page.lifecycleEvent", loaded);
        unsubscribe();
        this.cancelLoad = null;
        if (error) reject(error); else resolve();
      };
      this.cancelLoad = finish;
    });
    // Events/connection failure can arrive before the navigate RPC returns.
    // Keep the original rejecting promise for waitForLoad without an unhandled
    // rejection in that gap.
    this.loadPromise.catch(() => {});
    this.client.onEvent(this.sessionId, "Page.lifecycleEvent", loaded);
    unsubscribe = this.client.onClose?.((error) => finish(error)) ?? (() => {});
    try {
      navigation = await this.send("Page.navigate", { url }, remaining);
      if (navigation.errorText || navigation.isDownload) {
        throw new Error(`Page.navigate failed for ${url}: ${navigation.errorText || "navigation became a download"}`);
      }
      // Same-document navigation has no new loader and emits no load event.
      if (!navigation.loaderId) finish();
      else for (const event of earlyLoads) loaded(event);
    } catch (error) {
      finish(error);
      this.loadPromise = null;
      throw error;
    }
  }

  async waitForLoad() {
    if (!this.loadPromise) throw new Error("waitForLoad called without a successful navigation");
    await this.loadPromise;
  }

  async evaluate(expression, { userGesture = false } = {}) {
    const response = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture,
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
    this.closeListeners = new Set();
  }

  open({ timeoutMs = 15_000 } = {}) {
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
      if (this.endpoint.process) {
        // Descendants may still hold pipe descriptors after Chrome exits.
        // Process liveness is independent of receiving an EOF on the pipe.
        this.processExitHandler = (code, signal) => this.close(new Error(
          `Chrome exited: code=${code} signal=${signal}`,
        ));
        this.endpoint.process.once("exit", this.processExitHandler);
        const { exitCode, signalCode } = this.endpoint.process;
        if (exitCode !== null || signalCode !== null) {
          this.processExitHandler(exitCode, signalCode);
        }
      }
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.endpoint, { handshakeTimeout: timeoutMs });
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
    for (const listener of this.closeListeners) listener(this.closedError);
    this.closeListeners.clear();
    if (this.processExitHandler) {
      this.endpoint.process.off("exit", this.processExitHandler);
    }
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
      this.pending.set(id, { resolve, reject, timeout, method });
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

  onClose(handler) {
    if (this.closedError) handler(this.closedError);
    else this.closeListeners.add(handler);
    return () => this.closeListeners.delete(handler);
  }

  onEvent(sessionId, method, handler) {
    const key = `${sessionId}:${method}`;
    const handlers = this.listeners.get(key) ?? new Set();
    handlers.add(handler);
    this.listeners.set(key, handlers);
  }

  offEvent(sessionId, method, handler) {
    const key = `${sessionId}:${method}`;
    const handlers = this.listeners.get(key);
    handlers?.delete(handler);
    if (handlers?.size === 0) this.listeners.delete(key);
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
        pending.reject(new CdpProtocolError(pending.method, message.error));
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
