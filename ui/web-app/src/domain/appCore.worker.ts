import { loadWasmAdapterOnThisThread, type AppCoreAdapter, type UiInvalidation, type UiSession } from "./appCoreAdapter";
import { debugLog, setBrowserInstanceId } from "./debugLog";

type WorkerCallTarget =
  | { kind: "adapter" }
  | { kind: "session"; sessionId: number };

type WorkerCallRequest = {
  kind: "call";
  id: number;
  sentAtEpochMs: number;
  browserInstanceId: string;
  debugRunId?: string;
  target: WorkerCallTarget;
  method: string;
  args: unknown[];
};

type WorkerCallResponse =
  | { kind: "response"; id: number; ok: true; result: unknown; workerPostedAtEpochMs: number }
  | { kind: "response"; id: number; ok: false; error: WorkerErrorPayload; workerPostedAtEpochMs: number };

type WorkerSessionInvalidation = {
  kind: "sessionInvalidation";
  sessionId: number;
  invalidations: UiInvalidation[];
};

type WorkerErrorPayload = {
  name?: string;
  message: string;
  stack?: string;
};

type WorkerSessionMarker = {
  __aerobagWorkerSessionId: number;
};

type WorkerRuntime = {
  addEventListener(type: "message", listener: (event: MessageEvent<WorkerCallRequest>) => void): void;
  postMessage(message: WorkerCallResponse | WorkerSessionInvalidation, transfer?: Transferable[]): void;
};

const ctx = self as unknown as WorkerRuntime;
let adapterPromise: Promise<AppCoreAdapter> | null = null;
let nextSessionId = 1;
const sessions = new Map<number, UiSession>();

ctx.addEventListener("message", (event: MessageEvent<WorkerCallRequest>) => {
  const message = event.data;
  if (message.kind !== "call") {
    return;
  }
  void handleCall(message);
});

async function handleCall(message: WorkerCallRequest): Promise<void> {
  setBrowserInstanceId(message.browserInstanceId);
  if (message.debugRunId) {
    (globalThis as unknown as { __aerobagPerfRunId?: string }).__aerobagPerfRunId = message.debugRunId;
  }
  const startedAt = performance.now();
  try {
    const result = await dispatchCall(message);
    logWorkerCallDone(message, startedAt);
    postResponse({ kind: "response", id: message.id, ok: true, result, workerPostedAtEpochMs: Date.now() });
  } catch (error) {
    logWorkerCallDone(message, startedAt, error);
    debugLog("app_core.worker.call.error", {
      target: message.target,
      method: message.method,
      error: error instanceof Error ? error.message : String(error),
    });
    postResponse({
      kind: "response",
      id: message.id,
      ok: false,
      error: serializeError(error),
      workerPostedAtEpochMs: Date.now(),
    });
  }
}

function logWorkerCallDone(message: WorkerCallRequest, startedAt: number, error?: unknown): void {
  const elapsedMs = performance.now() - startedAt;
  const queueWaitMs = Date.now() - message.sentAtEpochMs - elapsedMs;
  const important =
    message.method === "queryRasterTilePlan"
    || message.method === "queryMapOverlay"
    || message.method === "ingestLiveFeedSseEvents"
    || message.method === "ingestLiveFeedSseEvent"
    || elapsedMs >= 100
    || queueWaitMs >= 100
    || error !== undefined;
  if (!important) {
    return;
  }
  debugLog("app_core.worker.call.done", {
    id: message.id,
    target: message.target.kind,
    method: message.method,
    queue_wait_ms: Math.round(queueWaitMs),
    elapsed_ms: Math.round(elapsedMs),
    error: error instanceof Error ? error.message : error === undefined ? null : String(error),
  });
}

async function dispatchCall(message: WorkerCallRequest): Promise<unknown> {
  if (message.target.kind === "adapter") {
    return callAdapterMethod(message.method, message.args);
  }
  const session = sessions.get(message.target.sessionId);
  if (!session) {
    throw new Error(`unknown worker session ${message.target.sessionId}`);
  }
  return callSessionMethod(message.target.sessionId, session, message.method, message.args);
}

async function callAdapterMethod(method: string, args: unknown[]): Promise<unknown> {
  const adapter = await ensureAdapter();
  if (method === "createUiSession") {
    const session = await callMethod<UiSession>(adapter, method, args);
    const sessionId = nextSessionId++;
    sessions.set(sessionId, session);
    session.setInvalidationListener((invalidations) => {
      postMessage({ kind: "sessionInvalidation", sessionId, invalidations });
    });
    return { __aerobagWorkerSessionId: sessionId } satisfies WorkerSessionMarker;
  }
  return callMethod(adapter, method, args);
}

async function callSessionMethod(
  sessionId: number,
  session: UiSession,
  method: string,
  args: unknown[],
): Promise<unknown> {
  if (method === "setInvalidationListener") {
    throw new Error("session invalidation listener is controlled by the worker proxy");
  }
  try {
    return await callMethod(session, method, args);
  } finally {
    if (method === "destroy") {
      sessions.delete(sessionId);
    }
  }
}

async function ensureAdapter(): Promise<AppCoreAdapter> {
  adapterPromise ??= loadWasmAdapterOnThisThread().then((loaded) => loaded.adapter);
  return adapterPromise;
}

async function callMethod<T>(target: unknown, method: string, args: unknown[]): Promise<T> {
  if (!target || typeof target !== "object") {
    throw new Error(`cannot call ${method} on non-object target`);
  }
  const fn = (target as Record<string, unknown>)[method];
  if (typeof fn !== "function") {
    throw new Error(`app-core worker target does not implement ${method}`);
  }
  return await fn.apply(target, args) as T;
}

function postResponse(response: WorkerCallResponse): void {
  const startedAt = performance.now();
  if (response.ok && response.result instanceof Uint8Array) {
    ctx.postMessage(response, [response.result.buffer]);
    logWorkerResponsePosted(response, startedAt);
    return;
  }
  ctx.postMessage(response);
  logWorkerResponsePosted(response, startedAt);
}

function logWorkerResponsePosted(response: WorkerCallResponse, startedAt: number): void {
  const elapsedMs = performance.now() - startedAt;
  const summary = response.ok
    ? summarizeWorkerResult(response.result)
    : { important: true, data: { result_kind: "error" } };
  if (elapsedMs < 10 && !summary.important) {
    return;
  }
  debugLog("app_core.worker.response.posted", {
    id: response.id,
    post_ms: Math.round(elapsedMs),
    ...summary.data,
  });
}

function summarizeWorkerResult(result: unknown): { important: boolean; data: Record<string, unknown> } {
  if (result instanceof Uint8Array) {
    return { important: result.byteLength >= 65536, data: { result_kind: "bytes", byte_length: result.byteLength } };
  }
  if (!result || typeof result !== "object") {
    return { important: false, data: { result_kind: typeof result } };
  }
  const record = result as Record<string, unknown>;
  if (Array.isArray(record.visible_features) || Array.isArray(record.visible_metars)) {
    return {
      important: true,
      data: {
        result_kind: "map_overlay",
        visible_features: Array.isArray(record.visible_features) ? record.visible_features.length : null,
        visible_metars: Array.isArray(record.visible_metars) ? record.visible_metars.length : null,
        visible_pireps: Array.isArray(record.visible_pireps) ? record.visible_pireps.length : null,
        airspace_paths: Array.isArray(record.airspace_paths) ? record.airspace_paths.length : null,
        airspace_labels: Array.isArray(record.airspace_labels) ? record.airspace_labels.length : null,
      },
    };
  }
  if ("app_state" in record && "debug_state" in record) {
    return { important: true, data: { result_kind: "session_snapshot" } };
  }
  return { important: false, data: { result_kind: "object" } };
}

function postMessage(message: WorkerSessionInvalidation): void {
  ctx.postMessage(message);
}

function serializeError(error: unknown): WorkerErrorPayload {
  if (error instanceof Error) {
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
    };
  }
  return { message: String(error) };
}
