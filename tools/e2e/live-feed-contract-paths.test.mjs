// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import {
  LIVE_FEED_ROOT,
  liveFeedPath,
  metarVersionFromPath,
} from "./live-feed-contract-paths.mjs";

test("relative manifest URLs resolve to paths served by the scripted server", () => {
  const currentUrl = new URL(`http://127.0.0.1${liveFeedPath("current.json")}`);

  for (const [kind, relativePath] of [
    ["versions", "versions/metars/v1.json"],
    ["states", "states/metars/v1.json"],
  ]) {
    const pathname = new URL(relativePath, currentUrl).pathname;
    assert.equal(pathname, `${LIVE_FEED_ROOT}/${relativePath}`);
    assert.equal(metarVersionFromPath(pathname, kind), "v1");
  }
});

test("version path parsing rejects unrelated and nested paths", () => {
  assert.equal(metarVersionFromPath("/live-feeds/v2/versions/metars/v1.json", "versions"), null);
  assert.equal(metarVersionFromPath(liveFeedPath("versions/metars/nested/v1.json"), "versions"), null);
  assert.equal(metarVersionFromPath(liveFeedPath("states/metars/v1.zip"), "states"), null);
});
