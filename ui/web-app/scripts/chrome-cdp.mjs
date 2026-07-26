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
} = {}) {
  if (!userDataDir) {
    throw new Error("launchChrome requires userDataDir");
  }
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
      `--window-size=${width},${height}`,
      "about:blank",
    ], {
      stdio: ["ignore", "ignore", "pipe"],
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
      reject(new Error(
        `Chrome exited before DevTools was ready: code=${code} signal=${signal} stderr=${stderr}`,
      ));
    });
  });
}

export async function connectToBrowser(wsUrl) {
  const client = new CdpClient(wsUrl);
  await client.open();
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
    this.loadPromise = null;
    this.client.onEvent(sessionId, "Runtime.exceptionThrown", (params) => {
      this.diagnostics.push({
        method: "Runtime.exceptionThrown",
        exception: params.exceptionDetails,
      });
    });
    this.client.onEvent(sessionId, "Runtime.consoleAPICalled", (params) => {
      if (params.type === "error" || params.type === "warning") {
        this.diagnostics.push({
          method: "Runtime.consoleAPICalled",
          type: params.type,
          args: params.args,
        });
      }
    });
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
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

class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
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
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timeout);
      pending.reject(new Error(`CDP connection closed while request ${id} was pending`));
    }
    this.pending.clear();
  }

  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP request timed out: ${method}`));
      }, 15000);
      this.pending.set(id, { resolve, reject, timeout });
      this.ws.send(JSON.stringify(message));
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

  handleMessage(data) {
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
