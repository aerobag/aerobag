// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Explicit real-browser check, not part of the emulator-free unit-test gate.
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { connectToBrowser, launchChrome, stopProcess } from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { recreateWebJourneyPage, WebSemanticTransport } from "../e2e/web-semantic-transport.mjs";
import { withinDeadline } from "../e2e/deadline.mjs";
import { launchCloudJourneyPeer } from "../e2e/cloud-journey-peer.mjs";

test("real browser reset isolates a dirty predecessor and delayed worker; reload retains storage", { timeout: 30_000 }, async () => {
  let lateResponse;
  let workerRequested;
  const requestSeen = new Promise((resolve) => { workerRequested = resolve; });
  const server = createServer((request, response) => {
    if (request.url === "/late") { lateResponse = response; workerRequested(); return; }
    response.setHeader("Content-Type", "text/html");
    response.end("<!doctype html><title>Lifecycle fixture</title><body><p id='ready'>ready</p></body>");
  });
  const sockets = new Set();
  server.on("connection", (socket) => { sockets.add(socket); socket.on("close", () => sockets.delete(socket)); });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const origin = `http://127.0.0.1:${server.address().port}`;
  const profile = await mkdtemp(join(tmpdir(), "aerobag-browser-lifecycle-"));
  let chrome, browser;
  try {
    chrome = await launchChrome({ userDataDir: profile });
    browser = await connectToBrowser(chrome.endpoint);
    const configure = async (page) => { await page.send("Page.enable"); await page.send("Runtime.enable"); return page; };
    const initial = await configure(await browser.createPage({ browserContextId: await browser.createBrowserContext() }));
    const transport = new WebSemanticTransport(initial, {
      url: origin,
      recreatePage: (previous, options) => recreateWebJourneyPage(browser, previous, configure, options),
    });
    await transport.reset();
    assert.equal(await transport.page.evaluate("document.querySelector('#ready').textContent"), "ready");
    await transport.page.evaluate(`(async () => {
      localStorage.setItem('predecessor', 'dirty');
      document.cookie = 'predecessor=dirty; path=/';
      await new Promise((resolve, reject) => {
        const request = indexedDB.open('predecessor', 1);
        request.onsuccess = () => { request.result.close(); resolve(); };
        request.onerror = () => reject(request.error);
      });
    })()`);
    const dirtyContext = transport.page.browserContextId;
    await transport.reload();
    assert.equal(transport.page.browserContextId, dirtyContext);
    assert.equal(await transport.page.evaluate("localStorage.getItem('predecessor')"), "dirty");
    assert.match(await transport.page.evaluate("document.cookie"), /predecessor=dirty/);
    assert.deepEqual(await transport.page.evaluate("indexedDB.databases().then(items => items.map(item => item.name))"), ["predecessor"]);
    await transport.page.evaluate(`(() => {
      const source = ${JSON.stringify(`fetch('${origin}/late').then(response => response.text()).then(value => postMessage(value));`)};
      window.predecessorWorker = new Worker(URL.createObjectURL(new Blob([source], { type: 'text/javascript' })));
      predecessorWorker.onmessage = (event) => { localStorage.setItem('late', event.data); document.body.dataset.late = event.data; };
    })()`);
    await withinDeadline("old worker reaches controlled response", () => requestSeen, performance.now() + 3_000);
    await transport.reset();
    assert.notEqual(transport.page.browserContextId, dirtyContext);
    const { targetInfos } = await browser.client.send("Target.getTargets");
    assert.equal(targetInfos.some((target) => target.browserContextId === dirtyContext), false);
    // Deliberately release old work only after the next context is ready.
    lateResponse.end("old-worker-result");
    await transport.page.evaluate(`fetch('${origin}/barrier').then(response => response.text())`);
    const cleanState = () => transport.page.evaluate(`(async () => ({
      storage: Object.keys(localStorage), cookies: document.cookie,
      databases: (await indexedDB.databases()).map(item => item.name),
      late: document.body.dataset.late ?? null,
      ready: document.querySelector('#ready').textContent,
    }))()`);
    const expected = { storage: [], cookies: "", databases: [], late: null, ready: "ready" };
    assert.deepEqual(await cleanState(), expected);
    // The same clean journey alone and after the deliberately dirty predecessor.
    await transport.reset();
    assert.deepEqual(await cleanState(), expected);
  } finally {
    browser?.close();
    await stopProcess(chrome?.process);
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve) => server.close(resolve));
    await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
});

test("a failed cloud peer setup tears down its browser before rejecting", { timeout: 30_000 }, async () => {
  await assert.rejects(launchCloudJourneyPeer({ url: "http://127.0.0.1:9/" }), (error) => {
    assert.match(error.message, /Page.navigate failed.*ERR_UNSAFE_PORT/);
    const process = error.chrome?.process;
    assert.ok(process?.pid);
    assert.ok(process.exitCode !== null || process.signalCode !== null);
    assert.equal(error.cleanup_error, undefined);
    return true;
  });
});
