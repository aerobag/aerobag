// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { recordChromeStartupTrace } from "./chrome-startup-trace.mjs";

function harness(t, { endEvent = { stream: "trace", dataLossOccurred: false }, chunks, fail } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "chrome-trace-test-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const output = join(dir, "startup.json"), calls = [], handlers = new Map();
  let closed = 0, closeListener;
  const data = chunks ?? [{ data: '{"traceEvents":[]}', eof: true }];
  const client = {
    onEvent(session, method, handler) { assert.equal(session, ""); handlers.set(method, handler); },
    offEvent(session, method) { assert.equal(session, ""); handlers.delete(method); },
    onClose(handler) { closeListener = handler; return () => { closeListener = null; }; },
    close() { closed++; closeListener?.(new Error("browser closed")); },
    async send(method, params, session, timeout) {
      calls.push({ method, params, session, timeout });
      assert.equal(session, undefined, "tracing must target the browser, not the renderer");
      assert.ok(timeout > 0 && timeout <= 3000);
      if (method === fail) throw new Error(`${method} failed`);
      if (method === "Tracing.end" && endEvent) handlers.get("Tracing.tracingComplete")(endEvent);
      if (method === "IO.read") return data.shift() ?? { data: "x", eof: false };
      return {};
    },
  };
  return { client, output, calls, handlers, get closed() { return closed; } };
}

test("startup trace uses a bounded ring and captures the completion event before end acknowledgement", async t => {
  const h = harness(t, { chunks: [
    { data: '{"traceEvents":', eof: false },
    { data: Buffer.from("[]}").toString("base64"), base64Encoded: true, eof: true },
  ] });
  const trace = await recordChromeStartupTrace(h.client, h.output);
  assert.deepEqual(h.calls[0].params.traceConfig, {
    recordMode: "recordContinuously", traceBufferSizeInKb: 4096,
    includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing", "v8"],
  });
  const stopping = trace.stop();
  assert.equal(trace.stop(), stopping, "cleanup must not request a second trace");
  const result = await stopping;
  assert.deepEqual(JSON.parse(readFileSync(h.output, "utf8")), { traceEvents: [] });
  assert.equal(result.bytes, 18);
  assert.equal(result.data_loss_occurred, false);
  assert.equal(h.closed, 1);
  assert.equal(h.handlers.size, 0);
  assert.deepEqual(h.calls.map(call => call.method), ["Tracing.start", "Tracing.end", "IO.read", "IO.read", "IO.close"]);
});

test("a wrapped trace remains useful but reports loss explicitly", async t => {
  const h = harness(t, { endEvent: { stream: "trace", dataLossOccurred: true } });
  const trace = await recordChromeStartupTrace(h.client, h.output);
  assert.equal((await trace.stop()).data_loss_occurred, true);
  assert.ok(existsSync(h.output));
});

for (const fail of ["Tracing.start", "Tracing.end", "IO.read", "IO.close"]) {
  test(`trace ${fail} failure closes only its diagnostic connection and produces no misleading file`, async t => {
    const h = harness(t, { fail });
    await assert.rejects(async () => {
      const trace = await recordChromeStartupTrace(h.client, h.output);
      await trace.stop();
    }, /failed/);
    assert.equal(h.closed, 1);
    assert.equal(h.handlers.size, 0);
    assert.equal(existsSync(h.output), false);
  });
}

test("a missing trace event is bounded even after end has acknowledged", async t => {
  const h = harness(t, { endEvent: null });
  const trace = await recordChromeStartupTrace(h.client, h.output, { timeoutMs: 20 });
  await assert.rejects(trace.stop(), /capture timed out/);
  assert.equal(h.closed, 1);
  assert.equal(h.handlers.size, 0);
  assert.equal(existsSync(h.output), false);
});

test("an endless trace stream is bounded by bytes, not just per-chunk deadlines", async t => {
  const h = harness(t, { chunks: [] });
  const trace = await recordChromeStartupTrace(h.client, h.output, { byteLimit: 4 });
  await assert.rejects(trace.stop(), /exceeds 4 bytes/);
  assert.equal(h.calls.filter(call => call.method === "IO.read").length, 5);
  assert.equal(h.closed, 1);
  assert.equal(existsSync(h.output), false);
});

test("browser death while the trace event is pending fails immediately", async t => {
  const h = harness(t, { endEvent: null });
  const trace = await recordChromeStartupTrace(h.client, h.output);
  const stopped = trace.stop();
  h.client.close();
  await assert.rejects(stopped, /browser closed/);
  assert.equal(existsSync(h.output), false);
});
