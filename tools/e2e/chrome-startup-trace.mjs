// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { CdpClient } from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { E2E_TIMING } from "./transition-contract.mjs";

export async function connectChromeStartupTrace(port, outputPath) {
  const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
    signal: AbortSignal.timeout(E2E_TIMING.localReadyMs),
  });
  if (!response.ok) throw new Error(`Chrome trace endpoint: HTTP ${response.status}`);
  const { webSocketDebuggerUrl } = await response.json();
  const client = new CdpClient(webSocketDebuggerUrl);
  try {
    await client.open({ timeoutMs: E2E_TIMING.localReadyMs });
    return await recordChromeStartupTrace(client, outputPath);
  } catch (error) {
    client.close();
    throw error;
  }
}

// Use the browser connection, not Runtime on the potentially blocked renderer.
// No debugger/worker attachment, CPU sampling, app probes or network interception.
export async function recordChromeStartupTrace(client, outputPath, {
  timeoutMs = E2E_TIMING.localResourceMs,
  byteLimit = 16 * 1024 * 1024,
} = {}) {
  let completed;
  const completion = new Promise(resolve => { completed = resolve; });
  client.onEvent("", "Tracing.tracingComplete", completed);
  try {
    await client.send("Tracing.start", {
      transferMode: "ReturnAsStream",
      traceConfig: {
        recordMode: "recordContinuously",
        traceBufferSizeInKb: 4096,
        includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing", "v8"],
      },
    }, undefined, E2E_TIMING.localReadyMs);
  } catch (error) {
    client.offEvent("", "Tracing.tracingComplete", completed);
    client.close();
    throw error;
  }

  let stopping;
  return { stop() {
    if (stopping) return stopping;
    stopping = (async () => {
      const deadline = performance.now() + timeoutMs;
      let timer, unsubscribe = () => {};
      const terminal = new Promise((_resolve, reject) => {
        timer = setTimeout(() => reject(new Error("Chrome startup trace capture timed out")), timeoutMs);
        unsubscribe = client.onClose(reject);
      });
      // The same budget covers ending the recording, waiting for its event and
      // reading every chunk. A missing event or endless stream cannot hang CI.
      const bounded = promise => Promise.race([promise, terminal]);
      const send = (method, params = {}) => bounded(client.send(method, params, undefined,
        Math.max(1, Math.min(E2E_TIMING.localReadyMs, deadline - performance.now()))));
      try {
        await send("Tracing.end");
        const result = await bounded(completion);
        if (!result.stream) throw new Error("Chrome startup trace returned no stream");
        const chunks = [];
        let bytes = 0;
        for (;;) {
          const chunk = await send("IO.read", { handle: result.stream, size: 64 * 1024 });
          const buffer = Buffer.from(chunk.data, chunk.base64Encoded ? "base64" : "utf8");
          bytes += buffer.length;
          if (bytes > byteLimit) throw new Error(`Chrome startup trace exceeds ${byteLimit} bytes`);
          chunks.push(buffer);
          if (chunk.eof) break;
        }
        await send("IO.close", { handle: result.stream });
        mkdirSync(dirname(outputPath), { recursive: true });
        writeFileSync(outputPath, Buffer.concat(chunks));
        return { path: outputPath, bytes, data_loss_occurred: result.dataLossOccurred ?? false };
      } finally {
        clearTimeout(timer);
        unsubscribe();
        client.offEvent("", "Tracing.tracingComplete", completed);
        client.close();
      }
    })();
    return stopping;
  } };
}
