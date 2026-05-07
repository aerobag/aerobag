let seq = 0;
let globalErrorLoggingInstalled = false;
// Enable verbose bug-hunt logs with ?aerobagVerboseDebug=1 or
// localStorage.setItem("aerobag.verboseDebug", "1").
const verboseDebugQueryKeys = ["aerobagVerboseDebug", "debugVerbose", "verboseDebug"];
const verboseDebugStorageKey = "aerobag.verboseDebug";
let verboseDebugLoggingEnabledCache: boolean | null = null;

type DebugLogRecord = {
  seq: number;
  ts_ms: number;
  tag: string;
  data?: unknown;
};

const queue: DebugLogRecord[] = [];
let flushScheduled = false;

function scheduleFlush() {
  if (flushScheduled || typeof window === "undefined") {
    return;
  }
  flushScheduled = true;
  window.setTimeout(() => {
    flushScheduled = false;
    void flushQueue();
  }, 0);
}

async function flushQueue() {
  if (queue.length === 0) {
    return;
  }
  const batch = queue.splice(0, queue.length);
  try {
    await fetch("/__debug_log", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(batch),
    });
  } catch {
    queue.unshift(...batch);
  }
}

export function debugLog(tag: string, data?: unknown) {
  if (typeof window === "undefined") {
    return;
  }
  queue.push({
    seq: ++seq,
    ts_ms: Math.round(performance.now()),
    tag,
    data,
  });
  scheduleFlush();
}

export function verboseDebugLog(tag: string, data?: unknown) {
  if (!verboseDebugLoggingEnabled()) {
    return;
  }
  debugLog(tag, data);
}

export function verboseDebugLoggingEnabled() {
  if (verboseDebugLoggingEnabledCache !== null) {
    return verboseDebugLoggingEnabledCache;
  }
  if (typeof window === "undefined") {
    return false;
  }
  const params = new URLSearchParams(window.location.search);
  for (const key of verboseDebugQueryKeys) {
    const value = params.get(key);
    if (value === null) {
      continue;
    }
    verboseDebugLoggingEnabledCache = value !== "0" && value !== "false" && value !== "off";
    return verboseDebugLoggingEnabledCache;
  }
  try {
    const value = window.localStorage.getItem(verboseDebugStorageKey);
    verboseDebugLoggingEnabledCache = value === "1" || value === "true" || value === "on";
    return verboseDebugLoggingEnabledCache;
  } catch {
    verboseDebugLoggingEnabledCache = false;
    return false;
  }
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
  if (typeof performance === "undefined") {
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
