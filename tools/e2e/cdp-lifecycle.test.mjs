// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { CdpClient, CdpPage, launchChrome } from "../../ui/web-app/scripts/chrome-cdp.mjs";

function pageWith(send) {
  const client = new CdpClient({});
  client.send = async (method, ...args) => {
    if (method === "Page.setLifecycleEventsEnabled") return {};
    return send(method, ...args);
  };
  const page = new CdpPage(client, "session", "target");
  const event = (loaderId, frameId = "main") => client.handleMessage(JSON.stringify({
    sessionId: "session", method: "Page.lifecycleEvent", params: { name: "load", loaderId, frameId },
  }));
  const loadListeners = () => client.listeners.get("session:Page.lifecycleEvent")?.size ?? 0;
  return { client, page, event, loadListeners };
}

test("navigation retains an exact load notification arriving before its RPC response", async () => {
  const harness = pageWith(() => {
    harness.event("requested");
    return { loaderId: "requested", frameId: "main" };
  });
  await harness.page.navigate("http://fixture.test/");
  await harness.page.waitForLoad();
  assert.equal(harness.loadListeners(), 0);
  assert.equal(harness.client.closeListeners.size, 0);
});

test("old loaders and other frames cannot satisfy a new navigation", async () => {
  const harness = pageWith(() => ({ loaderId: "new", frameId: "main" }));
  await harness.page.navigate("http://fixture.test/");
  harness.event("old");
  harness.event("new", "iframe");
  assert.equal(harness.loadListeners(), 1);
  harness.event("new");
  await harness.page.waitForLoad();
  assert.equal(harness.loadListeners(), 0);
});

test("same-document navigation does not wait for a nonexistent load event", async () => {
  const { page, loadListeners } = pageWith(() => ({ frameId: "main" }));
  await page.navigate("http://fixture.test/#anchor");
  await page.waitForLoad();
  assert.equal(loadListeners(), 0);
});

test("a rejected navigation RPC cleans its listener and pending load", async () => {
  const { page, client, loadListeners } = pageWith(() => { throw new Error("target crashed"); });
  await assert.rejects(page.navigate("http://fixture.test/"), /target crashed/);
  await assert.rejects(page.waitForLoad(), /without a successful navigation/);
  assert.equal(loadListeners(), 0);
  assert.equal(client.closeListeners.size, 0);
});

test("a missing load notification expires and removes its observers", async () => {
  const { page, client, loadListeners } = pageWith(() => ({ loaderId: "new", frameId: "main" }));
  await page.navigate("http://fixture.test/", { timeoutMs: 10 });
  await assert.rejects(page.waitForLoad(), /Page load timed out/);
  assert.equal(loadListeners(), 0);
  assert.equal(client.closeListeners.size, 0);
});

test("connection loss rejects pending page load immediately", async () => {
  const { page, client, loadListeners } = pageWith(() => ({ loaderId: "new", frameId: "main" }));
  await page.navigate("http://fixture.test/");
  const rejected = assert.rejects(page.waitForLoad(), /browser exited/);
  client.close(new Error("browser exited"));
  await rejected;
  assert.equal(loadListeners(), 0);
});

test("replacing a navigation rejects its old wait without poisoning the new one", async () => {
  let index = 0;
  const { page, event } = pageWith(() => ({ loaderId: String(++index), frameId: "main" }));
  await page.navigate("http://fixture.test/one");
  const superseded = assert.rejects(page.waitForLoad(), /superseded/);
  await page.navigate("http://fixture.test/two");
  await superseded;
  event("1");
  event("2");
  await page.waitForLoad();
});

test("a websocket that never upgrades has a bounded handshake", async () => {
  const server = createServer();
  const sockets = new Set();
  server.on("connection", (socket) => { sockets.add(socket); socket.on("close", () => sockets.delete(socket)); });
  server.on("upgrade", () => {});
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const client = new CdpClient(`ws://127.0.0.1:${server.address().port}`);
  try {
    await assert.rejects(client.open({ timeoutMs: 20 }), /timed out/);
  } finally {
    client.close();
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve) => server.close(resolve));
  }
});

test("launcher reports early process death with retained process evidence", async () => {
  let child;
  await assert.rejects(launchChrome({
    chromeBin: process.execPath, userDataDir: "/unused-test-profile", transport: "websocket",
    // Node rejects Chrome's flags and exits: exercise early process death,
    // not a browser or user profile, without a fake shell executable.
    onSpawn: (process) => { child = process; }, startupTimeoutMs: 100,
  }), (error) => {
    assert.equal(error.chrome.process, child);
    assert.match(error.message, /exited before DevTools/);
    return true;
  });
  assert.notEqual(child.exitCode, null);
});

test("launcher tears down a live process on endpoint timeout and bounds stderr", async () => {
  const directory = await mkdtemp(join(tmpdir(), "aerobag-launcher-test-"));
  const executable = join(directory, "browser");
  await writeFile(executable, `#!${process.execPath}\nprocess.stderr.write('x'.repeat(70000));\nsetInterval(() => {}, 1000);\n`, { mode: 0o755 });
  let child;
  try {
    await assert.rejects(launchChrome({
      chromeBin: executable, userDataDir: join(directory, "profile"),
      transport: "websocket", startupTimeoutMs: 100,
      onSpawn: (process) => { child = process; },
    }), (error) => {
      assert.match(error.message, /timed out waiting for Chrome DevTools endpoint/);
      assert.equal(error.chrome.process, child);
      assert.ok(error.chrome.getStderr().length <= 65_536);
      assert.ok(child.exitCode !== null || child.signalCode !== null);
      return true;
    });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("disposing a page unregisters its page and dedicated-worker observers", () => {
  const { page, client } = pageWith(() => ({}));
  page.installDiagnosticListeners("worker-session", "worker");
  assert.ok(client.listeners.size > 10);
  page.dispose();
  assert.equal(client.listeners.size, 0);
  page.dispose();
  assert.equal(client.listeners.size, 0);
});
