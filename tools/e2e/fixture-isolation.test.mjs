// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { once } from "node:events";
import { request } from "node:http";
import test from "node:test";
import { fixtureServer } from "./fixture-test-server.mjs";

function getOnce(url) {
  return new Promise((resolve, reject) => {
    // Unlike fetch's transport recovery, one request here really means one.
    const req = request(url, { agent: false }, (response) => {
      let text = "";
      response.on("data", (chunk) => { text += chunk; });
      response.on("end", () => resolve({ status: response.statusCode, text }));
      response.on("error", reject);
    });
    req.setTimeout(2_000, () => req.destroy(new Error("request timed out")));
    req.on("error", reject);
    req.end();
  });
}

test("fixture reset rearms one-shot transport faults for the next journey", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  const url = `${origin}/release-journey/payload.txt?aerobag_e2e_abort_once=same-operation`;
  await assert.rejects(getOnce(url), { code: "ECONNRESET" });
  assert.deepEqual(await getOnce(url), { status: 200, text: "payload" });
  const reset = await fetch(`${origin}/__control`, { method: "POST", body: JSON.stringify({ reset: true }), signal: AbortSignal.timeout(2_000) });
  assert.equal(reset.status, 200);
  await reset.text();
  await assert.rejects(getOnce(url), { code: "ECONNRESET" });
});

test("a delayed control mutation cannot cross a fixture reset generation", { timeout: 5_000 }, async (t) => {
  const { server, origin } = await fixtureServer(t);
  let finishResponse, rejectResponse;
  const oldResponse = new Promise((resolve, reject) => { finishResponse = resolve; rejectResponse = reject; });
  oldResponse.catch(() => {});
  const oldRequest = request(`${origin}/__control`, { method: "POST", agent: false }, (response) => {
    response.resume();
    response.on("end", () => finishResponse(response.statusCode));
  });
  oldRequest.on("error", rejectResponse);
  oldRequest.setTimeout(2_000, () => oldRequest.destroy(new Error("control request timed out")));
  t.after(() => oldRequest.destroy());
  const arrived = once(server, "request");
  oldRequest.write('{"publication":');
  await arrived;
  const reset = await fetch(`${origin}/__control`, { method: "POST", body: JSON.stringify({ reset: true }), signal: AbortSignal.timeout(2_000) });
  assert.equal(reset.status, 200);
  await reset.text();
  oldRequest.end('"unsupported"}');
  assert.equal(await oldResponse, 409);
  const health = await fetch(`${origin}/__health`, { signal: AbortSignal.timeout(2_000) }).then((response) => response.json());
  assert.equal(health.control.publication, "primary");
});

test("raster delay preserves successful response bytes and resets between journeys", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  const control = async (value) => {
    const response = await fetch(`${origin}/__control`, {
      method: "POST", body: JSON.stringify(value), signal: AbortSignal.timeout(2_000),
    });
    assert.equal(response.status, 200);
    return response.json();
  };
  assert.equal((await control({ raster_delay_ms: 20 })).raster_delay_ms, 20);
  assert.deepEqual(await getOnce(`${origin}/packages/map/tiles/test.webp`), { status: 200, text: "tile bytes" });
  const requests = await fetch(`${origin}/__requests`, { signal: AbortSignal.timeout(2_000) }).then((response) => response.json());
  assert.equal(requests.find((entry) => entry.url === "/packages/map/tiles/test.webp").raster_delay_ms, 20);
  assert.equal((await control({ reset: true })).raster_delay_ms, 0);
  assert.deepEqual(await getOnce(`${origin}/packages/map/tiles/test.webp`), { status: 200, text: "tile bytes" });
});

test("raster delay rejects unbounded and malformed values", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  for (const raster_delay_ms of [-1, 30_001, 1.5, "20"]) {
    const response = await fetch(`${origin}/__control`, {
      method: "POST", body: JSON.stringify({ raster_delay_ms }), signal: AbortSignal.timeout(2_000),
    });
    assert.equal(response.status, 400);
    await response.text();
  }
  const health = await fetch(`${origin}/__health`, { signal: AbortSignal.timeout(2_000) }).then((response) => response.json());
  assert.equal(health.control.raster_delay_ms, 0);
});
