// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { once } from "node:events";
import { request } from "node:http";
import test from "node:test";
import { fixtureServer, liveFeedStream } from "./fixture-test-server.mjs";
import {
  SERVICE_BULLETIN_CONTROL_PATH, SERVICE_BULLETIN_PATH, SERVICE_NOTICE_FIXTURES,
} from "./service-notifications-fixture.mjs";

async function json(origin, path, body) {
  const response = await fetch(new URL(path, origin), {
    ...(body === undefined ? {} : { method: "POST", body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(2_000),
  });
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}

test("service publication updates an already-open real SSE stream and reconnect advertises the latest revision", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  const initial = await json(origin, SERVICE_BULLETIN_PATH);
  assert.equal(initial.revision, 1);
  assert.deepEqual(initial.notices, []);
  const stream = await liveFeedStream(t, origin);
  assert.deepEqual(await stream.waitForHint(1), { publisher: `${origin}${SERVICE_BULLETIN_PATH}`, revision: 1 });
  const state = await json(origin, SERVICE_BULLETIN_CONTROL_PATH, { publication: "published" });
  assert.equal(state.subscribers, 1);
  assert.equal(state.live_announcements, 1);
  const hint = await stream.waitForHint(2);
  const document = await json(origin, SERVICE_BULLETIN_PATH);
  assert.equal(hint.publisher, document.publisher);
  assert.equal(hint.revision, document.revision);
  assert.deepEqual(document.notices.map((notice) => notice.title), SERVICE_NOTICE_FIXTURES.map((notice) => notice.title));
  assert.deepEqual(document.notices.map((notice) => notice.severity), ["info", "caution", "warning", "warning"]);
  assert.equal(document.notices.at(-1).resolved, true);
  assert.ok(document.notices.every((notice) => notice.attention_revision === 1));
  const requests = await json(origin, "/__requests");
  assert.deepEqual(requests.filter((entry) => entry.url === SERVICE_BULLETIN_PATH).map((entry) => entry.service_bulletin_revision), [1, 2]);
  const reconnected = await liveFeedStream(t, origin);
  await reconnected.waitForHint(2);
  await json(origin, SERVICE_BULLETIN_CONTROL_PATH, { publication: "archived" });
  await stream.waitForHint(3);
  await reconnected.waitForHint(3);
  assert.ok((await json(origin, SERVICE_BULLETIN_PATH)).notices.every((notice) => notice.resolved));
});

test("SSE publishers retain each client's Host across Android adb reverse and web origins", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  const androidHost = "127.0.0.1:19093";
  const android = await liveFeedStream(t, origin, androidHost);
  const web = await liveFeedStream(t, origin);
  await android.waitForHint(1);
  await web.waitForHint(1);
  await json(origin, SERVICE_BULLETIN_CONTROL_PATH, { publication: "published" });
  assert.equal((await android.waitForHint(2)).publisher, `http://${androidHost}${SERVICE_BULLETIN_PATH}`);
  assert.equal((await web.waitForHint(2)).publisher, `${origin}${SERVICE_BULLETIN_PATH}`);
});

test("reset empties bulletins, disconnects old SSE subscribers, and isolates the next journey", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  const stream = await liveFeedStream(t, origin);
  await stream.waitForHint(1);
  await json(origin, SERVICE_BULLETIN_CONTROL_PATH, { publication: "published" });
  await stream.waitForHint(2);
  const closed = new Promise((resolve) => stream.response.once("close", resolve));
  await json(origin, "/__control", { reset: true });
  await closed;
  assert.deepEqual((await json(origin, "/__health")).service_notifications, {
    revision: 1, publication: "empty", subscribers: 0, live_announcements: 0,
  });
  assert.deepEqual((await json(origin, SERVICE_BULLETIN_PATH)).notices, []);
  await json(origin, SERVICE_BULLETIN_CONTROL_PATH, { publication: "published" });
  assert.deepEqual(stream.hints.map((hint) => hint.revision), [1, 2]);
  assert.equal((await json(origin, "/__health")).service_notifications.live_announcements, 0);
  const next = await liveFeedStream(t, origin);
  await next.waitForHint(2);
});

test("a disconnected service SSE subscriber is removed without waiting for a heartbeat", { timeout: 5_000 }, async (t) => {
  const { server, origin } = await fixtureServer(t);
  const arrived = once(server, "request");
  const stream = await liveFeedStream(t, origin);
  const [, serverResponse] = await arrived;
  await stream.waitForHint(1);
  const closed = new Promise((resolve) => serverResponse.once("close", resolve));
  stream.close();
  await closed;
  assert.equal((await json(origin, "/__health")).service_notifications.subscribers, 0);
});

test("service publication rejects invalid input without mutation", { timeout: 5_000 }, async (t) => {
  const { origin } = await fixtureServer(t);
  for (const body of [null, {}, { publication: "production" }, { publication: "published", reset: true }]) {
    const response = await fetch(new URL(SERVICE_BULLETIN_CONTROL_PATH, origin), {
      method: "POST", body: JSON.stringify(body), signal: AbortSignal.timeout(2_000),
    });
    assert.equal(response.status, 400);
    await response.text();
  }
  assert.equal((await json(origin, "/__health")).service_notifications.revision, 1);
});

test("a delayed bulletin publication cannot cross a fixture reset", { timeout: 5_000 }, async (t) => {
  const { server, origin } = await fixtureServer(t);
  const req = request(new URL(SERVICE_BULLETIN_CONTROL_PATH, origin), { method: "POST", agent: false });
  t.after(() => req.destroy());
  const arrived = once(server, "request");
  req.write('{"publication":');
  await arrived;
  await json(origin, "/__control", { reset: true });
  const responseReceived = once(req, "response", { signal: AbortSignal.timeout(2_000) });
  req.end('"published"}');
  const [response] = await responseReceived;
  assert.equal(response.statusCode, 409);
  response.resume();
  assert.equal((await json(origin, "/__health")).service_notifications.revision, 1);
});
