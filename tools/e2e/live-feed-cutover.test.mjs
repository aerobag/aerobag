// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createReleaseJourneyFixtureServer } from "./serve-release-journey-fixture.mjs";
import { CUTOVER_MARKERS } from "./live-feed-cutover-fixture.mjs";
import { liveFeedPath } from "./live-feed-contract-paths.mjs";
import { liveFeedProviderCutover } from "./live-feed-cutover-journey.mjs";
import { journeyById } from "./release-journey-registry.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "aerobag-cutover-"));
  await mkdir(join(root, "publication"));
  await mkdir(join(root, "live"));
  const json = (path, value) => writeFile(join(root, path), JSON.stringify(value));
  await json("fixture.json", { fixture: "cutover-wire-test", publication_root: "publication", capabilities: { live_feeds: { fresh: "live" } } });
  await json("publication/current_artifacts.json", [{ artifact_roots: { packaged: "" }, bundles: [{ relative_path: "bundle.json" }], contracts: {} }]);
  await json("publication/bundle.json", { bundle_id: "test", packages: [{ id: "test", family_id: "csup", region_id: "nw", filename: "test.zip", relative_path: "test.zip" }] });
  await json("live/current.json", { schema_version: 3, generated_at_utc: "2026-08-04T12:00:00Z", products: {} });
  const server = createReleaseJourneyFixtureServer({ fixture: join(root, "fixture.json"), liveFeedProfile: "cutover" });
  const listening = once(server, "listening");
  server.listen(0, "127.0.0.1");
  await listening;
  const origin = `http://127.0.0.1:${server.address().port}`;
  const abort = new AbortController();
  const get = (path) => fetch(new URL(path, origin), { signal: AbortSignal.any([abort.signal, AbortSignal.timeout(3_000)]), redirect: "error" });
  const control = async (update) => {
    const response = await fetch(`${origin}/__control`, { method: "POST", body: JSON.stringify(update), signal: AbortSignal.timeout(3_000) });
    const text = await response.text();
    assert.equal(response.status, 200, text);
    return JSON.parse(text).live_feed_cutover;
  };
  t.after(async () => {
    abort.abort();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(root, { recursive: true, force: true });
  });
  return {
    get, control, server,
    act: (action, options = {}) => control({ live_feed_cutover: { action, ...options } }),
    status: async () => (await (await get("/__health")).json()).live_feed_cutover,
  };
}

async function stream(f) {
  const response = await f.get(liveFeedPath("events"));
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type"), /text\/event-stream/);
  const reader = response.body.getReader();
  const initial = await reader.read();
  return { reader, initial: Buffer.from(initial.value).toString() };
}

async function arm(f, suppress = false) {
  await f.act("arm", { suppress_resync: suppress });
  const arrived = once(f.server, "request");
  const pending = f.get(liveFeedPath("states/metars/old-late.json"));
  await arrived;
  assert.equal((await f.status()).pending, 1);
  return { pending };
}

test("cutover profile uses independent immutable resources and verified full-state hashes", { timeout: 5_000 }, async (t) => {
  const f = await fixture(t);
  const catalog = await (await f.get(liveFeedPath("current.json"))).json();
  assert.equal(catalog.products.metars.current, "old-1");
  const manifest = await (await f.get(liveFeedPath(catalog.products.metars.version_manifest_url))).json();
  const bytes = Buffer.from(await (await f.get(liveFeedPath(manifest.state.url))).arrayBuffer());
  assert.equal(createHash("sha256").update(bytes).digest("hex"), manifest.state.state_sha256);
  assert.match(bytes.toString(), /CUTOVEROLD/);
  assert.equal((await f.get(liveFeedPath("states/metars/new-1.json"))).status, 404);
  assert.equal((await f.get(liveFeedPath("versions/metars/never-seeded.json"))).status, 404);
});

test("same-URL cutover retains old SSE and completes its held snapshot across the binding switch", { timeout: 5_000 }, async (t) => {
  const f = await fixture(t);
  const old = await stream(f);
  assert.match(old.initial, /live-feed-catalog/);
  const { pending } = await arm(f);
  const switched = await f.act("switch");
  assert.equal(switched.streams.old, 1);
  assert.equal(switched.streams.new, 0);
  assert.equal(switched.pending, 1);
  await f.act("release-old");
  assert.match(await (await pending).text(), /CUTOVEROBSOLETE/);
  const oldEvents = Buffer.from((await old.reader.read()).value).toString();
  assert.match(oldEvents, /old-missing/);
  for (const path of ["versions/metars/old-missing.json", "states/metars/old-late.json", "states/metars/old-1.json"]) {
    assert.equal((await f.get(liveFeedPath(path))).status, 404, path);
  }
  await f.act("drain");
  assert.equal((await old.reader.read()).done, true);
  const next = await stream(f);
  assert.match(next.initial, /new-1/);
  const manifest = await (await f.get(liveFeedPath("versions/metars/new-1.json"))).json();
  assert.equal(manifest.delta_from_previous.from_version, "new-retired");
  assert.equal((await f.get(liveFeedPath(manifest.delta_from_previous.url))).status, 404);
  assert.equal((await f.get(liveFeedPath("states/metars/new-retired.json"))).status, 404);
  assert.match(await (await f.get(liveFeedPath(manifest.state.url))).text(), /CUTOVERNEW/);
  await f.act("next");
  assert.match(Buffer.from((await next.reader.read()).value).toString(), /new-2/);
  assert.match(await (await f.get(liveFeedPath("states/metars/new-2.json"))).text(), /CUTOVERNEXT/);
});

test("cutover negative control suppresses new catalogs and events but still accepts reconnects", { timeout: 5_000 }, async (t) => {
  const f = await fixture(t);
  await stream(f);
  const { pending } = await arm(f, true);
  await f.act("switch");
  await f.act("release-old");
  await (await pending).text();
  await f.act("drain");
  const next = await stream(f);
  assert.match(next.initial, /connected/);
  assert.doesNotMatch(next.initial, /live-feed-catalog/);
  assert.equal((await f.get(liveFeedPath("current.json"))).status, 503);
  await f.act("next");
  assert.equal((await f.status()).suppress_resync, true);
  // Full bytes still exist. This fault targets discovery/resync, not the weather encoder.
  assert.equal((await f.get(liveFeedPath("states/metars/new-1.json"))).status, 200);
});

test("cutover controls reject unordered transitions and reset provider state between journeys", { timeout: 5_000 }, async (t) => {
  const f = await fixture(t);
  await assert.rejects(f.act("switch"), /old response barrier/);
  await stream(f);
  const { pending } = await arm(f);
  await f.act("switch");
  await f.act("release-old");
  await (await pending).text();
  await f.act("drain");
  await f.control({ reset: true });
  const reset = await f.status();
  assert.equal(reset.active, "old");
  assert.equal(reset.phase, "old");
  assert.equal(reset.pending, 0);
  assert.deepEqual(reset.streams, { old: 0, new: 0 });
  assert.match(await (await f.get(liveFeedPath("current.json"))).text(), /old-1/);
});

function modeledJourney({ suppressed = false, losePlan = false, reorderPlan = false, loseViewport = false, loseLayers = false, loseNext = false, platform = "web" } = {}) {
  let phase = "old";
  const marker = () => phase === "next" && !loseNext ? CUTOVER_MARKERS.next :
    phase === "drained" || phase === "next" ? CUTOVER_MARKERS.new : CUTOVER_MARKERS.old;
  const trace = [];
  const status = () => ({ streams: { old: 1, new: phase === "drained" ? 1 : 0 }, pending: phase === "armed" ? 1 : 0, trace });
  const runtime = {
    platform, artifactDir: "/unused", result: { diagnostics: {} },
    reset: async () => { assert.equal(phase, "old", "no mid-cutover reset"); },
    reload: async () => assert.fail("cutover must not reload"), openPage: async () => {},
    action: async (_label, _id, options) => options.complete(),
    check: (_id, pass, detail) => assert.ok(pass, detail),
    eventually: async (description, probe) => { const value = await probe(); assert.ok(value, description); return value; },
    driver: {
      back: async () => {}, captureFrame: async () => {},
      readModal: async () => ({}), readElement: async () => ({ text: marker() }),
      readProjection: async (prefix) => {
        if (prefix === "parity:live-overlay:") {
          const count = suppressed ? 1 : phase === "next" && !loseNext ? 3 : ["drained", "next"].includes(phase) ? 2 : 1;
          return [`parity:live-overlay:metars:${count}:pireps:0`];
        }
        const changed = phase === "drained" && ((loseViewport && prefix.includes("viewport")) || (loseLayers && prefix.includes("layer")));
        return [`${prefix}${changed ? "lost" : "preserved"}`];
      },
    },
  };
  const helpers = {
    acceptDisclaimer: async () => {}, appendRoute: async () => {}, setLayerVisible: async () => {},
    selectAirportFromMapSearch: async () => {}, closeMapDetail: async () => {},
    planRows: async () => {
      const labels = phase === "next" && losePlan ? ["KSEA", "KBFI", "KPAE"] :
        phase === "next" && reorderPlan ? ["KBFI", "KSEA", "KRNT"] : ["KSEA", "KBFI", "KRNT"];
      return platform === "android" ? labels.map((text) => ({ text })) : labels;
    },
    fixtureHealth: async () => ({ live_feed_cutover: status() }),
    setFixtureControl: async (_runtime, update) => {
      const action = update.live_feed_cutover.action;
      if (action === "arm") phase = "armed";
      if (action === "switch") {
        phase = "switched";
        trace.push({ kind: "switch", old_streams: 1, pending: 1 });
        trace.push({ kind: "resource", provider: "new", status: 404, path: "versions/metars/old-missing.json" });
      }
      if (action === "drain") {
        phase = "drained";
        trace.push({ kind: "connect", provider: "new" });
        trace.push({ kind: "resource", provider: "new", status: 200, path: "states/metars/new-1.json" });
      }
      if (action === "next") phase = "next";
      return { live_feed_cutover: status() };
    },
  };
  return liveFeedProviderCutover(runtime, helpers, { suppressResync: suppressed });
}

test("cutover journey runs on native Android and web in main/PR and release p0 selection", () => {
  const journey = journeyById("shared.live-feed-provider-cutover");
  assert.equal(journey.priority, "p0");
  assert.equal(journey.live_feed_profile, "cutover");
  assert.deepEqual(journey.platforms, ["web", "android"]);
});

for (const platform of ["web", "android"]) {
  test(`cutover ${platform} journey model accepts recovery and continued rendering`, () => modeledJourney({ platform }));
  for (const [fault, message] of [
    ["suppressed", /rendered METAR count 2/], ["loseNext", /rendered METAR count 3/],
    ["losePlan", /planBefore/], ["reorderPlan", /planBefore/],
    ["loseViewport", /before/], ["loseLayers", /before/],
  ]) {
    test(`cutover ${platform} journey model rejects ${fault}`, () =>
      assert.rejects(modeledJourney({ platform, [fault]: true }), message));
  }
}
