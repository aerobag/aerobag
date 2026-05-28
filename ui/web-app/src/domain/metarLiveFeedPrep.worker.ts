import init, {
  install_rust_debug_logger,
  prepare_metar_live_feed_resource,
  reset_metar_live_feed_preparer,
} from "@generated/app_wasm.js";
import { debugLog, installRustDebugLogBridge } from "./debugLog";

type MetarLiveFeedPrepRequest =
  | {
      kind: "prepare";
      id: number;
      resourceId: string;
      resourceBytes: Uint8Array;
    }
  | {
      kind: "reset";
      id: number;
    };

type MetarLiveFeedPrepResponse =
  | {
      kind: "prepared";
      id: number;
      ok: true;
      resourceId?: string;
      preparedBytes?: Uint8Array;
    }
  | {
      kind: "prepared";
      id: number;
      ok: false;
      error: string;
    };

type MetarLiveFeedPrepRuntime = {
  addEventListener(type: "message", listener: (event: MessageEvent<MetarLiveFeedPrepRequest>) => void): void;
  postMessage(message: MetarLiveFeedPrepResponse, transfer?: Transferable[]): void;
};

const ctx = self as unknown as MetarLiveFeedPrepRuntime;
let wasmReady: Promise<void> | null = null;

ctx.addEventListener("message", (event) => {
  const message = event.data;
  void handleMessage(message);
});

async function handleMessage(message: MetarLiveFeedPrepRequest): Promise<void> {
  const startedAt = performance.now();
  try {
    await ensureWasmReady();
    if (message.kind === "reset") {
      reset_metar_live_feed_preparer();
      ctx.postMessage({ kind: "prepared", id: message.id, ok: true });
      return;
    }
    const preparedBytes = new Uint8Array(
      prepare_metar_live_feed_resource(message.resourceId, message.resourceBytes),
    );
    debugLog("metar.live_feed.prep.done", {
      resource_id: message.resourceId,
      input_bytes: message.resourceBytes.byteLength,
      prepared_bytes: preparedBytes.byteLength,
      elapsed_ms: Math.round(performance.now() - startedAt),
    });
    ctx.postMessage({
      kind: "prepared",
      id: message.id,
      ok: true,
      resourceId: message.resourceId,
      preparedBytes,
    }, [preparedBytes.buffer]);
  } catch (error) {
    const messageText = error instanceof Error ? error.message : String(error);
    debugLog("metar.live_feed.prep.error", {
      kind: message.kind,
      resource_id: message.kind === "prepare" ? message.resourceId : null,
      elapsed_ms: Math.round(performance.now() - startedAt),
      error: messageText,
    });
    ctx.postMessage({
      kind: "prepared",
      id: message.id,
      ok: false,
      error: messageText,
    });
  }
}

async function ensureWasmReady(): Promise<void> {
  if (!wasmReady) {
    installRustDebugLogBridge();
    wasmReady = init().then(() => {
      install_rust_debug_logger();
    });
  }
  return wasmReady;
}
