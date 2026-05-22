let seq = 0;
let globalErrorLoggingInstalled = false;

type DebugLogRecord = {
  seq: number;
  ts_ms: number;
  tag: string;
  data?: unknown;
};

const queue: DebugLogRecord[] = [];
let flushScheduled = false;

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
  if (
    typeof fetch !== "function"
    || typeof location === "undefined"
    || !/^https?:$/.test(location.protocol)
    || typeof performance === "undefined"
  ) {
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
