// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import * as chrome from "./run-android-chrome-livefeed-e2e.mjs";
import { LIVE_FEED_SCHEMA_VERSION, liveFeedPath } from "./live-feed-contract-paths.mjs";

async function inputs(t) {
  const root = await mkdtemp(join(tmpdir(), "aerobag-chrome-inputs-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const directory of ["web", "publication", "live"]) await mkdir(join(root, directory));
  const json = (file, value) => writeFile(join(root, file), JSON.stringify(value));
  await writeFile(join(root, "web/index.html"), "<!doctype html><title>immutable app</title>");
  await writeFile(join(root, "web/app.wasm"), Buffer.from([0, 97, 115, 109]));
  await json("fixture.json", { fixture: "chrome-inputs", publication_root: "publication", capabilities: { live_feeds: { empty: "live" } } });
  await json("publication/current_artifacts.json", [{ artifact_roots: { packaged: "" }, bundles: [{ relative_path: "bundle.json" }], contracts: {} }]);
  await json("publication/bundle.json", { bundle_id: "test", packages: [{ id: "csup", family_id: "csup", region_id: "nw", filename: "test.zip", relative_path: "test.zip" }] });
  await json("live/current.json", { schema_version: LIVE_FEED_SCHEMA_VERSION, generated_at_utc: "2026-11-04T12:00:00Z", products: {} });
  return { webDist: join(root, "web"), fixture: join(root, "fixture.json") };
}

test("immutable Chrome web inputs never compile, including missing input errors", async (t) => {
  const args = await inputs(t);
  const runCommand = () => assert.fail("immutable consumer attempted to compile");
  chrome.prepareWebApp({ ...args, runCommand });
  assert.throws(() => chrome.prepareWebApp({ webDist: join(args.webDist, "absent"), runCommand }), /web dist has no index.html/);
});

test("Chrome immutable origin serves app, packages and the controlled reconnect stream", { timeout: 5_000 }, async (t) => {
  const args = await inputs(t);
  const liveFeed = new chrome.ScriptedLiveFeedServer();
  const server = chrome.createPrebuiltWebServer(args, liveFeed);
  const abort = new AbortController();
  t.after(async () => {
    abort.abort();
    liveFeed.dropSseClients();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  const listening = once(server, "listening");
  server.listen(0, "127.0.0.1");
  await listening;
  const origin = `http://127.0.0.1:${server.address().port}`;
  const get = (url) => fetch(new URL(url, origin), { signal: AbortSignal.any([abort.signal, AbortSignal.timeout(3_000)]) });
  assert.match(await (await get("/")).text(), /immutable app/);
  const wasm = await get("/app.wasm");
  assert.equal(wasm.headers.get("content-type"), "application/wasm");
  assert.deepEqual(Buffer.from(await wasm.arrayBuffer()), Buffer.from([0, 97, 115, 109]));
  assert.equal((await (await get("/packages/current_artifacts.json")).json())[0].bundles[0].relative_path, "bundle.json");
  const stream = await get(liveFeedPath("events"));
  const reader = stream.body.getReader();
  assert.match(Buffer.from((await reader.read()).value).toString(), /"current":"v1"/);
  liveFeed.publish("v2");
  assert.match(Buffer.from((await reader.read()).value).toString(), /"version":"v2"/);
  const manifest = await (await get(liveFeedPath("versions/metars/v2.json"))).json();
  const state = await (await get(liveFeedPath(manifest.state.url))).json();
  assert.equal(state.version_label, "v2");
  liveFeed.setEventsAvailable(false);
  liveFeed.dropSseClients();
  assert.equal((await reader.read()).done, true);
  assert.equal((await get(liveFeedPath("events"))).status, 503);
  liveFeed.publish("v3");
  liveFeed.setEventsAvailable(true);
  const recovered = (await get(liveFeedPath("events"))).body.getReader();
  assert.match(Buffer.from((await recovered.read()).value).toString(), /"current":"v3"/);
  assert.equal(liveFeed.requestCounts.current, 0);
});
