import { loadWasmAdapterOnThisThread, type AppCoreAdapter, type UiInvalidation, type UiSession } from "./appCoreAdapter";
import { debugLog } from "./debugLog";

type WorkerCallTarget =
  | { kind: "adapter" }
  | { kind: "session"; sessionId: number };

type WorkerCallRequest = {
  kind: "call";
  id: number;
  target: WorkerCallTarget;
  method: string;
  args: unknown[];
};

type WorkerCallResponse =
  | { kind: "response"; id: number; ok: true; result: unknown }
  | { kind: "response"; id: number; ok: false; error: WorkerErrorPayload };

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
  try {
    const result = await dispatchCall(message);
    postResponse({ kind: "response", id: message.id, ok: true, result });
  } catch (error) {
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
    });
  }
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
  if (response.ok && response.result instanceof Uint8Array) {
    ctx.postMessage(response, [response.result.buffer]);
    return;
  }
  ctx.postMessage(response);
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
