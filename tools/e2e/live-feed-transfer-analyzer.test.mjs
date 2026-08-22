// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  annotateSseAccounting,
  loadCapture,
  selectWarmPayload,
  simulateNexradPolicy,
  sseWireBytes,
} from "../analyze-live-feed-transfer.mjs";

function payload(product, version, history = []) {
  const result = {
    schema_version: 3,
    product,
    version,
    version_manifest_url: `versions/${product}/${version}.json`,
    state_url: `states/${product}/${version}.json`,
    state_sha256: version.padEnd(64, "0"),
  };
  if (history.length > 0) result.history = history;
  return result;
}

function event(product, version, epochMs, history = []) {
  const body = payload(product, version, history);
  return {
    product,
    version,
    epochMs,
    payload: body,
    sse_wire_bytes: sseWireBytes(product, body),
  };
}

test("SSE accounting removes unused history but retains the NEXRAD animation window", () => {
  const history = Array.from({ length: 12 }, (_, index) => ({
    version: `v${index}`,
    version_manifest_url: `versions/metars/v${index}.json`,
    state_url: `states/metars/v${index}.json`,
    state_sha256: String(index).repeat(64).slice(0, 64),
  }));
  const metars = annotateSseAccounting([
    event("metars", "v12", 0, history),
    event("metars", "v13", 1),
  ]);
  assert.ok(metars[1].accounting.sseBaselineBytes > metars[1].accounting.sseCurrentBytes);
  assert.equal(
    metars[1].accounting.sseCurrentBytes,
    sseWireBytes("metars", payload("metars", "v13")),
  );

  const nexradHistory = history.map((entry) => ({
    ...entry,
    version_manifest_url: entry.version_manifest_url.replace("metars", "nexrad"),
    state_url: entry.state_url.replace("metars", "nexrad"),
  }));
  const nexrad = annotateSseAccounting([
    event("nexrad", "v12", 0, nexradHistory),
    event("nexrad", "v13", 1),
  ]);
  const sixFramePayload = payload("nexrad", "v13", nexradHistory.slice(-5).concat({
    version: "v12",
    version_manifest_url: "versions/nexrad/v12.json",
    state_url: "states/nexrad/v12.json",
    state_sha256: "v12".padEnd(64, "0"),
  }));
  assert.equal(
    nexrad[1].accounting.sseCurrentBytes,
    sseWireBytes("nexrad", sixFramePayload),
  );
});

test("warm payload selection takes an applicable smaller delta and rejects a larger one", () => {
  const manifest = {
    product: "metars",
    version: "v2",
    state: { bytes: 1_000 },
    delta_from_previous: { from_version: "v1", to_version: "v2", bytes: 100 },
  };
  assert.deepEqual(selectWarmPayload(manifest, "v1"), {
    kind: "delta",
    bytes: 100,
    refs: [manifest.delta_from_previous],
  });
  manifest.delta_from_previous.bytes = 1_001;
  assert.deepEqual(selectWarmPayload(manifest, "v1"), {
    kind: "full",
    bytes: 1_000,
    refs: [manifest.state],
  });
  assert.equal(
    selectWarmPayload(
      { product: "winds-aloft", version: "w2", install_state: { bytes: 12_000 } },
      "w1",
      { windsOnDemand: true },
    ),
    null,
  );
});

test("NEXRAD cadence accounting selects only schedule-allowed frames", () => {
  const events = Array.from({ length: 7 }, (_, index) => ({
    epochMs: index * 5 * 60_000,
    version: `n${index}`,
    manifest: {
      install_profiles: {
        offline_0: { bytes: 100 },
        offline_low1: { bytes: 25 },
      },
    },
  }));
  for (const [cadence, frames, bytes] of [
    ["every", 7, 700],
    ["10m", 4, 400],
    ["30m", 2, 200],
    ["never", 0, 0],
  ]) {
    const result = simulateNexradPolicy(events, {
      profile: "offline_0",
      cadenceForEpochMs: () => cadence,
    });
    assert.equal(result.frames, frames);
    assert.equal(result.bytes, bytes);
    assert.equal(result.missingVersions.length, 0);
  }
});

test("capture loading recovers a compacted NOTAM manifest under its alternate name", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "aerobag-transfer-analysis-"));
  try {
    const liveRoot = path.join(root, "live-feeds", "v3");
    fs.mkdirSync(path.join(liveRoot, "versions", "notams"), { recursive: true });
    fs.writeFileSync(path.join(root, "capture.json"), JSON.stringify({
      state: "complete",
      started_at_utc: "2026-08-20T00:00:00Z",
      stop_at_utc: "2026-08-21T00:00:00Z",
    }));
    const body = payload("notams", "n1");
    body.version_manifest_url = "versions/notams/n1.checkpoint.json";
    fs.writeFileSync(path.join(root, "events.jsonl"), `${JSON.stringify({
      captured_at_utc: "2026-08-20T00:00:01Z",
      kind: "initial_snapshot",
      product: "notams",
      version: "n1",
      sse_wire_bytes: sseWireBytes("notams", body),
      payload: body,
    })}\n`);
    const manifest = JSON.stringify({
      schema_version: 3,
      product: "notams",
      version: "n1",
      state: { bytes: 1_000 },
      repetitive_test_padding: "x".repeat(1_000),
    });
    fs.writeFileSync(
      path.join(liveRoot, "versions", "notams", "n1.json"),
      manifest,
    );

    const capture = loadCapture(root);
    assert.equal(capture.events[0].manifestMissing, false);
    assert.equal(capture.events[0].manifestAliased, true);
    assert.equal(capture.events[0].manifest.version, "n1");
    assert.equal(capture.events[0].manifestRawBytes, Buffer.byteLength(manifest));
    assert.ok(capture.events[0].manifestGzipBytes < capture.events[0].manifestRawBytes);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
