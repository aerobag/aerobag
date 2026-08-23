// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import {
  LIVE_FEED_ROOT,
  LIVE_FEED_SCHEMA_VERSION,
  liveFeedPath,
  metarVersionFromPath,
} from "./live-feed-contract-paths.mjs";
import { ScriptedLiveFeedServer } from "./run-android-chrome-livefeed-e2e.mjs";

test("scripted SSE catalog supplies the complete v3 envelope", () => {
  const manifest = new ScriptedLiveFeedServer().currentManifest();

  assert.equal(manifest.schema_version, LIVE_FEED_SCHEMA_VERSION);
  assert.equal(manifest.generated_at_utc, "2026-07-09T12:01:00Z");
  assert.deepEqual(Object.keys(manifest.products), ["metars"]);
  assert.deepEqual(
    Object.keys(manifest.products.metars).sort(),
    [
      "collected_at_utc",
      "current",
      "published_at_utc",
      "state_sha256",
      "state_url",
      "version_manifest_url",
    ],
  );
});

test("relative manifest URLs resolve to paths served by the scripted server", () => {
  const eventsUrl = new URL(`http://127.0.0.1${liveFeedPath("events")}`);

  for (const [kind, relativePath] of [
    ["versions", "versions/metars/v1.json"],
    ["states", "states/metars/v1.json"],
  ]) {
    const pathname = new URL(relativePath, eventsUrl).pathname;
    assert.equal(pathname, `${LIVE_FEED_ROOT}/${relativePath}`);
    assert.equal(metarVersionFromPath(pathname, kind), "v1");
  }
});

test("version path parsing rejects unrelated and nested paths", () => {
  assert.equal(metarVersionFromPath("/live-feeds/v2/versions/metars/v1.json", "versions"), null);
  assert.equal(metarVersionFromPath(liveFeedPath("versions/metars/nested/v1.json"), "versions"), null);
  assert.equal(metarVersionFromPath(liveFeedPath("states/metars/v1.zip"), "states"), null);
});
