let seq = 0;
let globalErrorLoggingInstalled = false;

declare const __AEROBAG_DEBUG_LOG_ENABLED__: boolean;

export type DebugLogRecord = {
  seq: number;
  ts_ms: number;
  tag: string;
  browser_instance_id: string;
  run_id?: string;
  data?: unknown;
};

const queue: DebugLogRecord[] = [];
const observers = new Set<(record: DebugLogRecord) => void>();
const flushDelayMs = 250;
const maxFlushBatchSize = 1000;
const debugLogDeveloperServerUploadStorageKey = "aerobag.web.debugLogToDeveloperServer.v1";
let flushScheduled = false;
let flushInFlight = false;
let browserInstanceId = resolveBrowserInstanceId();
let developerServerUploadEnabled = readPersistedDebugLogDeveloperServerUploadEnabled();

export const VERBOSE_PERF_DEBUG_LOGS = false;
export const DebugLogDeveloperServerPath = "/__debug_log";

export function setDebugLogDeveloperServerUploadEnabled(enabled: boolean): void {
  const wasEnabled = developerServerUploadEnabled;
  developerServerUploadEnabled = enabled;
  if (!enabled) {
    queue.splice(0, queue.length);
    return;
  }
  if (!wasEnabled) {
    debugLog("LOGGING_ENABLED", pageIdentityForDebugLog());
  }
  scheduleFlush();
}

function scheduleFlush() {
  if (
    flushScheduled
    || typeof fetch !== "function"
    || typeof location === "undefined"
    || !/^https?:$/.test(location.protocol)
    || typeof globalThis.setTimeout !== "function"
  ) {
    return;
  }
  flushScheduled = true;
  globalThis.setTimeout(() => {
    flushScheduled = false;
    void flushQueue();
  }, flushDelayMs);
}

async function flushQueue() {
  if (!developerServerUploadEnabled) {
    queue.splice(0, queue.length);
    return;
  }
  if (queue.length === 0) {
    return;
  }
  if (flushInFlight) {
    scheduleFlush();
    return;
  }
  flushInFlight = true;
  const batch = queue.splice(0, Math.min(queue.length, maxFlushBatchSize));
  try {
    await fetch(DebugLogDeveloperServerPath, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(batch),
    });
  } catch {
    queue.unshift(...batch);
  } finally {
    flushInFlight = false;
    if (queue.length > 0) {
      scheduleFlush();
    }
  }
}

export function debugLog(tag: string, data?: unknown) {
  if (
    !isDebugLogEnabled()
    || typeof fetch !== "function"
    || typeof location === "undefined"
    || !/^https?:$/.test(location.protocol)
    || typeof performance === "undefined"
  ) {
    return;
  }
  const runId = currentDebugRunId();
  const record = {
    seq: ++seq,
    ts_ms: Math.round(performance.now()),
    tag,
    browser_instance_id: browserInstanceId,
    ...(runId ? { run_id: runId } : {}),
    data,
  };
  for (const observer of observers) {
    try {
      observer(record);
    } catch {
      // Diagnostics must never perturb the code path being measured.
    }
  }
  if (developerServerUploadEnabled) {
    queue.push(record);
    scheduleFlush();
  }
}

export function isDebugLogEnabled(): boolean {
  if (developerServerUploadEnabled) {
    return true;
  }
  if (
    typeof __AEROBAG_DEBUG_LOG_ENABLED__ !== "undefined"
    && __AEROBAG_DEBUG_LOG_ENABLED__
  ) {
    return true;
  }
  const global = globalThis as unknown as {
    __aerobagDebugLogEnabled?: unknown;
  };
  return global.__aerobagDebugLogEnabled === true || currentDebugRunId() !== null;
}

export function perfDebugLog(tag: string, data?: () => unknown) {
  if (!VERBOSE_PERF_DEBUG_LOGS) {
    return;
  }
  debugLog(tag, data?.());
}

export function observeDebugLog(observer: (record: DebugLogRecord) => void): () => void {
  observers.add(observer);
  return () => {
    observers.delete(observer);
  };
}

export async function flushDebugLogNow(): Promise<void> {
  await flushQueue();
}

export function readPersistedDebugLogDeveloperServerUploadEnabled(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  try {
    return window.localStorage.getItem(debugLogDeveloperServerUploadStorageKey) === "1";
  } catch {
    return false;
  }
}

export function writePersistedDebugLogDeveloperServerUploadEnabled(enabled: boolean): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(debugLogDeveloperServerUploadStorageKey, enabled ? "1" : "0");
  } catch {
    // A failed persistence write must not affect the debug flag itself.
  }
}

export function getBrowserInstanceId(): string {
  return browserInstanceId;
}

export function setBrowserInstanceId(nextId: string): void {
  if (nextId.length === 0) {
    return;
  }
  browserInstanceId = nextId;
  (globalThis as unknown as { __aerobagBrowserInstanceId?: string }).__aerobagBrowserInstanceId = nextId;
}

function currentDebugRunId(): string | null {
  const candidate = (globalThis as unknown as { __aerobagPerfRunId?: unknown }).__aerobagPerfRunId;
  return typeof candidate === "string" && candidate.length > 0 ? candidate : null;
}

function pageIdentityForDebugLog() {
  const navigationEntry =
    typeof performance !== "undefined"
      ? performance.getEntriesByType("navigation")[0]
      : null;
  return {
    href: typeof location !== "undefined" ? location.href : "",
    userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
    html_start_ms: (globalThis as unknown as { __aerobag_html_start?: number }).__aerobag_html_start ?? null,
    enabled_at_ms: typeof performance !== "undefined" ? Math.round(performance.now()) : null,
    navigation: navigationEntry && "toJSON" in navigationEntry
      ? (navigationEntry as PerformanceNavigationTiming).toJSON()
      : null,
  };
}

function resolveBrowserInstanceId(): string {
  const global = globalThis as unknown as { __aerobagBrowserInstanceId?: unknown };
  if (typeof global.__aerobagBrowserInstanceId === "string" && global.__aerobagBrowserInstanceId.length > 0) {
    return global.__aerobagBrowserInstanceId;
  }
  const generated = createBrowserInstanceId();
  global.__aerobagBrowserInstanceId = generated;
  return generated;
}

function createBrowserInstanceId(): string {
  const cryptoApi = (globalThis as unknown as {
    crypto?: { randomUUID?: () => string };
  }).crypto;
  if (cryptoApi?.randomUUID) {
    return cryptoApi.randomUUID();
  }
  return `browser-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function installRustDebugLogBridge() {
  if (typeof globalThis === "undefined") {
    return;
  }
  (globalThis as unknown as {
    __aerobagRustDebugLog?: (tag: string, dataJson: string) => void;
  }).__aerobagRustDebugLog = (tag, dataJson) => {
    let data: unknown = dataJson;
    try {
      data = JSON.parse(dataJson);
    } catch {
      // Keep malformed Rust payloads visible instead of dropping the diagnostic.
    }
    debugLog(tag, data);
  };
}

export function installGlobalErrorLogging() {
  if (typeof window === "undefined" || globalErrorLoggingInstalled) {
    return;
  }
  globalErrorLoggingInstalled = true;

  window.addEventListener("error", (event) => {
    debugLog("window.error", {
      message: event.message,
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      error: serializeUnknown(event.error),
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    debugLog("window.unhandledrejection", {
      reason: serializeUnknown(event.reason),
    });
  });
}

export function debugTiming<T>(
  tag: string,
  work: () => T,
  data?: unknown,
): T {
  if (!isDebugLogEnabled() || typeof performance === "undefined") {
    return work();
  }
  const start = performance.now();
  debugLog(`${tag}.start`, data);
  try {
    const result = work();
    if (isPromiseLike(result)) {
      return result.then(
        (value) => {
          debugLog(`${tag}.done`, { ...objectData(data), elapsed_ms: Math.round(performance.now() - start) });
          return value;
        },
        (error) => {
          debugLog(`${tag}.error`, { ...objectData(data), elapsed_ms: Math.round(performance.now() - start), message: error instanceof Error ? error.message : String(error) });
          throw error;
        },
      ) as T;
    }
    debugLog(`${tag}.done`, { ...objectData(data), elapsed_ms: Math.round(performance.now() - start) });
    return result;
  } catch (error) {
    debugLog(`${tag}.error`, { ...objectData(data), elapsed_ms: Math.round(performance.now() - start), message: error instanceof Error ? error.message : String(error) });
    throw error;
  }
}

export function perfDebugTiming<T>(
  tag: string,
  work: () => T,
  data?: () => unknown,
): T {
  if (!VERBOSE_PERF_DEBUG_LOGS) {
    return work();
  }
  return debugTiming(tag, work, data?.());
}

function objectData(data: unknown): Record<string, unknown> {
  return data && typeof data === "object" && !Array.isArray(data) ? data as Record<string, unknown> : {};
}

function isPromiseLike<T>(value: T | PromiseLike<T>): value is PromiseLike<T> {
  return value !== null && typeof value === "object" && "then" in value && typeof value.then === "function";
}

function serializeUnknown(value: unknown): unknown {
  if (value instanceof Error) {
    return {
      name: value.name,
      message: value.message,
      stack: value.stack,
      cause: serializeUnknown(value.cause),
    };
  }
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>).map(([key, entry]) => [key, serializeUnknown(entry)]);
    return Object.fromEntries(entries);
  }
  return value;
}
