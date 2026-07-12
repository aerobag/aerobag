import { loadWasmAdapterOnThisThread, type AppCoreAdapter, type UiInvalidation, type UiSession } from "./appCoreAdapter";
import { debugLog, observeDebugLog, setBrowserInstanceId, type DebugLogRecord } from "./debugLog";

type WorkerCallTarget =
  | { kind: "adapter" }
  | { kind: "session"; sessionId: number };

type WorkerCallRequest = {
  kind: "call";
  id: number;
  sentAtEpochMs: number;
  browserInstanceId: string;
  debugLogEnabled: boolean;
  debugRunId?: string;
  target: WorkerCallTarget;
  method: string;
  args: unknown[];
};

type WorkerCallResponse =
  | { kind: "response"; id: number; ok: true; result: unknown; workerPostedAtEpochMs: number; workerPostedAtMs: number }
  | { kind: "response"; id: number; ok: false; error: WorkerErrorPayload; workerPostedAtEpochMs: number; workerPostedAtMs: number };

type WorkerResponseReady = {
  kind: "responseReady";
  id: number;
  target: WorkerCallTarget;
  method: string;
  workerReadyAtMs: number;
};

type WorkerSessionInvalidation = {
  kind: "sessionInvalidation";
  sessionId: number;
  invalidations: UiInvalidation[];
};

type WorkerCoreSettingsChanged = {
  kind: "coreSettingsChanged";
  settingsJson: string | null;
};

type WorkerDebugLog = {
  kind: "workerDebugLog";
  record: DebugLogRecord;
};

type WorkerErrorPayload = {
  name?: string;
  message: string;
  stack?: string;
};

type WorkerSessionMarker = {
  __aerobagWorkerSessionId: number;
  initialSnapshot: ReturnType<UiSession["initialSnapshot"]>;
};

type WorkerRuntime = {
  addEventListener(type: "message", listener: (event: MessageEvent<WorkerCallRequest>) => void): void;
  postMessage(message: WorkerCallResponse | WorkerResponseReady | WorkerSessionInvalidation | WorkerCoreSettingsChanged | WorkerDebugLog, transfer?: Transferable[]): void;
};

const ctx = self as unknown as WorkerRuntime;
let adapterPromise: Promise<AppCoreAdapter> | null = null;
let nextSessionId = 1;
const sessions = new Map<number, UiSession>();
const pendingCalls = new Map<number, PendingWorkerCall>();
let workerDebugForwardingEnabled = false;

ctx.addEventListener("message", (event: MessageEvent<WorkerCallRequest>) => {
  const message = event.data;
  if (message.kind !== "call") {
    return;
  }
  void handleCall(message);
});

async function handleCall(message: WorkerCallRequest): Promise<void> {
  setBrowserInstanceId(message.browserInstanceId);
  if (message.debugLogEnabled) {
    enableWorkerDebugForwarding();
    (globalThis as unknown as { __aerobagDebugLogEnabled?: boolean }).__aerobagDebugLogEnabled = true;
  }
  if (message.debugRunId) {
    (globalThis as unknown as { __aerobagPerfRunId?: string }).__aerobagPerfRunId = message.debugRunId;
  }
  debugLog("app_core.worker.message.received", {
    id: message.id,
    target: message.target.kind,
    method: message.method,
  });
  pendingCalls.set(message.id, { target: message.target, method: message.method });
  const startedAt = performance.now();
  try {
    const result = await dispatchCall(message);
    logWorkerCallDone(message, startedAt);
    postResponse({
      kind: "response",
      id: message.id,
      ok: true,
      result,
      workerPostedAtEpochMs: Date.now(),
      workerPostedAtMs: nowEpochishMs(),
    });
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
      workerPostedAtMs: nowEpochishMs(),
    });
  } finally {
    pendingCalls.delete(message.id);
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
  debugLog("app_core.worker.adapter_call.start", { method });
  const adapter = await ensureAdapter();
  debugLog("app_core.worker.adapter_ready", { method });
  if (method === "createUiSession") {
    const sessionArgs = args.slice(0, 4);
    setWorkerCoreSettingsJson(typeof args[4] === "string" ? args[4] : null);
    const session = await callMethod<UiSession>(adapter, method, sessionArgs);
    const sessionId = nextSessionId++;
    sessions.set(sessionId, session);
    session.setInvalidationListener((invalidations) => {
      postMessage({ kind: "sessionInvalidation", sessionId, invalidations });
    });
    return {
      __aerobagWorkerSessionId: sessionId,
      initialSnapshot: session.initialSnapshot(),
    } satisfies WorkerSessionMarker;
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
    const result = await callMethod(session, method, args);
    if (method === "acceptDisclaimer" || method === "performSettingsAction") {
      ctx.postMessage({
        kind: "coreSettingsChanged",
        settingsJson: workerCoreSettingsJson(),
      });
    }
    return result;
  } finally {
    if (method === "destroy") {
      sessions.delete(sessionId);
    }
  }
}

async function ensureAdapter(): Promise<AppCoreAdapter> {
  if (!adapterPromise) {
    debugLog("app_core.worker.adapter_load.start");
    adapterPromise = loadWasmAdapterOnThisThread().then(
      (loaded) => {
        debugLog("app_core.worker.adapter_load.done", { backend: loaded.backend });
        return loaded.adapter;
      },
      (error) => {
        debugLog("app_core.worker.adapter_load.error", {
          error: error instanceof Error ? error.message : String(error),
        });
        throw error;
      },
    );
  }
  return adapterPromise;
}

function enableWorkerDebugForwarding(): void {
  if (workerDebugForwardingEnabled) {
    return;
  }
  workerDebugForwardingEnabled = true;
  observeDebugLog((record) => {
    ctx.postMessage({ kind: "workerDebugLog", record });
  });
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
  const pending = pendingCalls.get(response.id);
  if (pending) {
    ctx.postMessage({
      kind: "responseReady",
      id: response.id,
      target: pending.target,
      method: pending.method,
      workerReadyAtMs: nowEpochishMs(),
    });
  }
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

function setWorkerCoreSettingsJson(settingsJson: string | null): void {
  (globalThis as unknown as { __aerobagCoreSettingsJson?: string | null }).__aerobagCoreSettingsJson = settingsJson;
}

function workerCoreSettingsJson(): string | null {
  const value = (globalThis as unknown as { __aerobagCoreSettingsJson?: unknown }).__aerobagCoreSettingsJson;
  return typeof value === "string" && value.length > 0 ? value : null;
}

type PendingWorkerCall = {
  target: WorkerCallTarget;
  method: string;
};

function nowEpochishMs(): number {
  return performance.timeOrigin + performance.now();
}
