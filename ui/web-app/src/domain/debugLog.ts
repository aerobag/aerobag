let seq = 0;

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

