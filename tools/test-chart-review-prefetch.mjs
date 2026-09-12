// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import {test} from "node:test";
import {SheetPrefetchQueue, cacheImageBytes, sheetsAhead} from "./chart_visual_review/prefetch.mjs";

test("next ten follows the cursor; all covers the filtered queue without duplicates", () => {
  const rows = Array.from({length:20}, (_,id) => ({id}));
  assert.deepEqual(sheetsAhead(rows,3,"10"),rows.slice(3,14));
  assert.deepEqual(sheetsAhead(rows,18,"10"),rows.slice(18));
  assert.deepEqual(sheetsAhead(rows,3,"all"),[...rows.slice(3),...rows.slice(0,3)]);
  assert.deepEqual(sheetsAhead(rows,3,"0"),[]);
  assert.deepEqual(sheetsAhead(rows,null,"10"),[]);
});

const request = (key, priority = 2) => ({key, priority, value:key});
const flush = () => new Promise(resolve => setImmediate(resolve));
function harness() {
  const calls = [], pending = new Map();
  const queue = new SheetPrefetchQueue({load:key => {
    calls.push(key);
    return new Promise((resolve, reject) => pending.set(key, {resolve, reject}));
  }});
  return {queue, calls, pending};
}

test("bounded background prefetch leaves a foreground slot and follows new priorities", async () => {
  const {queue,calls,pending} = harness();
  queue.setRequests([request("a"),request("b"),request("c")]);
  await flush(); assert.deepEqual(calls,["a"]);
  queue.setRequests([request("z",0),request("a"),request("b"),request("c")]);
  await flush(); assert.deepEqual(calls,["a","z"]);
  pending.get("a").resolve("pixels a"); await flush();
  assert.deepEqual(calls,["a","z","b"]);
  assert.equal(queue.get("a").status,"ready");
  queue.setRequests([request("y",0),request("z",1)]);
  pending.get("b").resolve("pixels b"); await flush();
  assert.deepEqual(calls,["a","z","b","y"],"discard outdated pending work when cursor or mode changes");
  pending.get("z").resolve("pixels z"); pending.get("y").resolve("pixels y"); await flush();
});

test("deduplicates observers and lookahead, retains errors without a retry storm, and keys revisions separately", async () => {
  const {queue,calls,pending} = harness();
  queue.setRequests([request("v1",2),request("v1",0)]); await flush();
  queue.setRequests([request("v1",0)]); await flush();
  assert.deepEqual(calls,["v1"]);
  pending.get("v1").reject(Error("wifi down")); await flush();
  queue.setRequests([request("v1",0)]); await flush();
  assert.deepEqual(calls,["v1"]); assert.equal(queue.get("v1").status,"error");
  queue.retryFailed(); await flush(); assert.deepEqual(calls,["v1","v1"]);
  pending.get("v1").resolve("ready"); await flush();
  queue.setRequests([request("v1",0)]); await flush(); assert.equal(calls.length,2);
  queue.setRequests([request("v2",0)]); await flush(); assert.equal(calls.at(-1),"v2");
  pending.get("v2").resolve("new image"); await flush();
});

test("cache warmer drains bytes without decoding images or retaining the body", async () => {
  const data = new ReadableStream({start(controller) {
    controller.enqueue(new Uint8Array(40)); controller.enqueue(new Uint8Array(60)); controller.close();
  }});
  assert.equal(await cacheImageBytes("/image", async () => new Response(data)),100);
  await assert.rejects(cacheImageBytes("/image", async () => new Response("failure",{status:503})),/503/);
});
