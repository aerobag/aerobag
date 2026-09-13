// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer, request } from "node:http";
import test from "node:test";
import { proxyCloudRequest } from "./serve-release-journey-fixture.mjs";

async function listen(t, handler) {
  const server = createServer(handler);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(async () => {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

async function cloudProxy(t, handler) {
  const upstream = await listen(t, handler);
  const proxy = await listen(t, (req, res) => proxyCloudRequest(req, res, upstream.origin));
  return { upstream: upstream.server, origin: proxy.origin };
}

// Event handshakes control ordering; this is only a failure watchdog. Nothing
// depends on a heartbeat, a browser retry, or elapsed sleep before an assertion.
const event = (emitter, name) => once(emitter, name, { signal: AbortSignal.timeout(2_000) });

function connect(t, origin, options = {}) {
  const req = request(`${origin}/cloud/v1/events`, { agent: false, ...options });
  t.after(() => req.destroy());
  return req;
}

test("cloud proxy releases quiet SSE connections on every browser reload", { timeout: 5_000 }, async (t) => {
  let active = 0;
  let upstreamClosed;
  const { origin } = await cloudProxy(t, (_req, res) => {
    active += 1;
    upstreamClosed = event(res, "close").then(() => { active -= 1; });
    res.writeHead(active > 1 ? 429 : 200, { "content-type": "text/event-stream" });
    res.write("event: ready\ndata: {}\n\n");
    // Deliberately no heartbeat: disconnect must release the slot itself.
  });
  // More reloads than the real server's four-slot account limit. There is still
  // only one live browser here, so none may consume a second upstream slot.
  for (let reload = 0; reload < 8; reload += 1) {
    const req = connect(t, origin);
    const response = event(req, "response");
    req.end();
    const [res] = await response;
    assert.equal(res.statusCode, 200, `reload ${reload} leaked a connection slot`);
    await event(res, "data");
    assert.equal(active, 1, "a completed GET must not close its ongoing SSE response");
    res.destroy();
    await upstreamClosed;
    assert.equal(active, 0, "browser disconnect must release upstream without a heartbeat");
  }
});

test("cloud proxy cancels upstream when the browser leaves before response headers", { timeout: 5_000 }, async (t) => {
  const { origin, upstream } = await cloudProxy(t, () => {});
  const arrived = event(upstream, "request");
  const req = connect(t, origin);
  req.on("error", () => {}); // Intentional client abort before any response.
  req.end();
  const [, upstreamResponse] = await arrived;
  const closed = event(upstreamResponse, "close");
  req.destroy();
  await closed;
});

test("cloud proxy cancels an incomplete upload when the browser disconnects", { timeout: 5_000 }, async (t) => {
  const { origin, upstream } = await cloudProxy(t, () => {});
  const arrived = event(upstream, "request");
  const req = connect(t, origin, { method: "POST" });
  req.on("error", () => {});
  req.write("partial encrypted request");
  const [, upstreamResponse] = await arrived;
  const closed = event(upstreamResponse, "close");
  req.destroy();
  await closed;
});

test("cloud proxy preserves completed request and response bodies", { timeout: 5_000 }, async (t) => {
  const { origin } = await cloudProxy(t, async (req, res) => {
    let body = "";
    for await (const chunk of req) body += chunk;
    res.writeHead(201, { "content-type": "application/json" });
    res.end(body);
  });
  const req = connect(t, origin, { method: "POST", headers: { "content-type": "application/json" } });
  const response = event(req, "response");
  const body = JSON.stringify({ encrypted: "body stays intact" });
  req.end(body);
  const [res] = await response;
  let received = "";
  for await (const chunk of res) received += chunk;
  assert.equal(res.statusCode, 201);
  assert.equal(res.headers["content-type"], "application/json");
  assert.equal(received, body);
});

test("cloud proxy reports an upstream failure before headers as 502", { timeout: 5_000 }, async (t) => {
  const { origin } = await cloudProxy(t, (req) => req.socket.destroy());
  const req = connect(t, origin);
  const response = event(req, "response");
  req.end();
  const [res] = await response;
  let body = "";
  for await (const chunk of res) body += chunk;
  assert.equal(res.statusCode, 502);
  assert.match(body, /cloud proxy failed/);
});

test("cloud proxy closes the downstream response if upstream fails mid-stream", { timeout: 5_000 }, async (t) => {
  let upstreamResponse;
  const { origin } = await cloudProxy(t, (_req, res) => {
    upstreamResponse = res;
    res.writeHead(200, { "content-type": "text/event-stream" });
    res.write("event: ready\ndata: {}\n\n");
  });
  const req = connect(t, origin);
  const response = event(req, "response");
  req.end();
  const [res] = await response;
  await event(res, "data");
  const aborted = event(res, "aborted");
  res.on("error", () => {}); // Expected truncated upstream response.
  upstreamResponse.destroy();
  await aborted;
});
