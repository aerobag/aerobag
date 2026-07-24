// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import init, {
  install_rust_debug_logger,
  prepare_live_feed_resource,
  reset_live_feed_preparer,
} from "@generated/app_wasm.js";
import { debugLog, installRustDebugLogBridge } from "./debugLog";

type LiveFeedPrepRequest =
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

type LiveFeedPrepResponse =
  | {
      kind: "prepared";
      id: number;
      ok: true;
      preparedBytes?: Uint8Array;
      elapsedMs?: number;
    }
  | {
      kind: "prepared";
      id: number;
      ok: false;
      error: string;
    };

type LiveFeedPrepRuntime = {
  addEventListener(type: "message", listener: (event: MessageEvent<LiveFeedPrepRequest>) => void): void;
  postMessage(message: LiveFeedPrepResponse, transfer?: Transferable[]): void;
};

const ctx = self as unknown as LiveFeedPrepRuntime;
let wasmReady: Promise<void> | null = null;

ctx.addEventListener("message", (event) => {
  void handleMessage(event.data);
});

async function handleMessage(message: LiveFeedPrepRequest): Promise<void> {
  const startedAt = performance.now();
  try {
    await ensureWasmReady();
    if (message.kind === "reset") {
      reset_live_feed_preparer();
      ctx.postMessage({ kind: "prepared", id: message.id, ok: true });
      return;
    }
    const preparedBytes = new Uint8Array(
      prepare_live_feed_resource(message.resourceId, message.resourceBytes),
    );
    debugLog("live_feed.prep.done", {
      resource_id: message.resourceId,
      input_bytes: message.resourceBytes.byteLength,
      prepared_bytes: preparedBytes.byteLength,
      elapsed_ms: Math.round(performance.now() - startedAt),
    });
    ctx.postMessage({
      kind: "prepared",
      id: message.id,
      ok: true,
      preparedBytes,
      elapsedMs: Math.round(performance.now() - startedAt),
    }, [preparedBytes.buffer]);
  } catch (error) {
    const messageText = error instanceof Error ? error.message : String(error);
    debugLog("live_feed.prep.error", {
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
