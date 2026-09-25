// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { androidObservationBackend, rejectObservationOverrides } from "./android-observation-contract.mjs";
import { queryAndroidExactProjection, queryAndroidSemanticNodes, scrollHorizontallyUntilTag } from "./android-harness.mjs";
import { TransientObservationError } from "./transition-contract.mjs";

function transport() {
  let response = { status: 0, stdout: "[]" };
  const requests = [];
  const context = {
    URLSearchParams, androidObservationBackend, rejectObservationOverrides, TransientObservationError,
    requiredSemanticDriver: () => ({ port: 19191 }),
    semanticDriverObservationUnavailable: value => value.status === 28,
    semanticDriverObservationRequest(_port, path) {
      requests.push(new URL(path, "http://device"));
      return response;
    },
  };
  return {
    exact: runInNewContext(`(${queryAndroidExactProjection})`, context),
    collection: runInNewContext(`(${queryAndroidSemanticNodes})`, context),
    respond(value) { response = value; }, requests,
  };
}

test("all app namespaces use the publisher for exact, prefix and absent lifecycle reads", () => {
  const targets = ["parity:cloud-", "parity:cloud-panel:test", "parity:plan-list", "parity:scroll:1"];
  for (const tag of [...targets, "parity:page:plate", "parity:service:notice:test", "parity:plan-row:1",
    "flight-data-cell:clock", "org.aerobag.app:id/e2e_startup_state_projection"]) {
    const io = transport();
    for (const read of [() => io.exact("test", tag), () => io.collection("test", tag, { prefix: true })]) {
      for (const state of [[], [{ "resource-id": tag, text: "first" }], [{ "resource-id": tag, text: "changed" }], []]) {
        io.respond({ status: 0, stdout: JSON.stringify(state) });
        assert.equal(JSON.stringify(read()), JSON.stringify(state));
        const params = io.requests.at(-1).searchParams;
        assert.equal(params.get("provider_only"), "true", tag);
        assert.equal(params.get("rendered_only"), "false", tag);
      }
      io.respond({ status: 28, stdout: "", stderr: "busy" });
      assert.throws(read, TransientObservationError);
      io.respond({ status: 7, stdout: "", stderr: "unavailable" });
      assert.throws(read, /persistent Android/);
    }
    assert.equal(io.requests.length, 12, "no retries or source switches inside an observation");
  }
});

test("callers cannot override backend ownership or request a fallback", () => {
  const io = transport();
  for (const name of ["providerOnly", "renderedOnly", "indexedOnly", "boundedOnly", "indexed"]) {
    for (const query of [io.exact, io.collection]) {
      assert.throws(() => query("test", "parity:page:home", { [name]: true }), /not target-owned/);
    }
  }
  assert.equal(io.requests.length, 0, "invalid requests fail before device I/O");
  assert.throws(() => io.exact("test", "unknown-namespace:target"), /No Android observation contract/);
  assert.equal(io.requests.length, 0);
});

test("horizontal discovery checks the same indexed target before and after physical scrolling", async () => {
  const io = transport();
  const tag = "parity:plan-control:Undo";
  let scrolls = 0;
  const reveal = runInNewContext(`(${scrollHorizontallyUntilTag})`, {
    queryAndroidExactProjection: io.exact,
    dumpAndroid: () => { throw new Error("tree access forbidden"); },
    scrollAndroidSemanticSurfaceAndAwait: async (_serial, orientation, direction) => {
      assert.equal(orientation, "horizontal");
      assert.equal(direction, "forward");
      scrolls++;
      io.respond({ status: 0, stdout: JSON.stringify([{
        "resource-id": tag, visible: "true", "center-reachable": "true",
      }]) });
      return true;
    },
  });
  assert.equal(await reveal("test", tag), true);
  assert.equal(scrolls, 1);
  assert.equal(await reveal("test", tag), true);
  assert.equal(scrolls, 1, "already positioned controls never scroll");
  assert.ok(io.requests.every(request => request.searchParams.get("provider_only") === "true"));
  assert.ok(io.requests.every(request => request.searchParams.get("verify_reachable") === "true"));
});
