// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { linkCloudJourneyPeer } from "./cloud-journey-peer.mjs";
import { E2E_TIMING } from "./transition-contract.mjs";

function controlledLink({ feedback = true, verifyAfterMs = 8_000, failure = false } = {}) {
  let now = 0, clickedAt = null, clicks = 0;
  const timers = new Map();
  let timerId = 0, ticking = false;
  function tick() {
    if (ticking) return;
    ticking = true;
    setImmediate(() => {
      ticking = false;
      if (!timers.size) return;
      now = Math.min(...Array.from(timers.values(), (timer) => timer.at));
      for (const [id, timer] of [...timers]) {
        if (timer.at <= now) { timers.delete(id); timer.callback(); }
      }
      tick();
    });
  }
  const scheduler = {
    now: () => now,
    setTimeout(callback, ms) {
      const id = ++timerId;
      timers.set(id, { at: now + ms, callback });
      tick();
      return id;
    },
    clearTimeout: (id) => timers.delete(id),
  };
  const driver = {
    readElement(id) {
      if (id === "cloud-action-accept_setup_code") return { enabled: true };
      if (clickedAt === null) return null;
      if (id === "cloud-panel-linked") {
        return now - clickedAt >= verifyAfterMs ? { state: "active", text: "Sync Account linked" } : null;
      }
      if (id === "cloud-panel-link_account" && feedback) {
        return failure
          ? { state: "error", text: "Could not link Sync Account: permission denied" }
          : { state: "working", text: "Linking Sync Account..." };
      }
      return null;
    },
    performAction(id) {
      assert.equal(id, "accept_setup_code");
      clicks++;
      clickedAt = now;
    },
  };
  return { scheduler, driver, clicks: () => clicks, elapsed: () => now - clickedAt };
}

test("cloud linking acknowledges input promptly while provider verification is pending", async () => {
  const h = controlledLink();
  const timings = [];
  await linkCloudJourneyPeer(h.driver, { scheduler: h.scheduler, onTiming: timing => timings.push(timing) });
  assert.equal(h.clicks(), 1);
  assert.ok(timings[0].response_ms < E2E_TIMING.userResponseTargetMs);
  assert.ok(h.elapsed() >= 8_000, "pending feedback cannot substitute for verified linkage");
});

test("missing visible linking feedback still fails the short UI deadline", async () => {
  const h = controlledLink({ feedback: false });
  await assert.rejects(linkCloudJourneyPeer(h.driver, { scheduler: h.scheduler }), /completed timed out/);
  assert.equal(h.clicks(), 1);
  assert.ok(h.elapsed() <= E2E_TIMING.userTransitionDeadlineMs);
});

test("permanently pending provider verification fails with the rendered state retained", async () => {
  const h = controlledLink({ verifyAfterMs: Infinity });
  await assert.rejects(linkCloudJourneyPeer(h.driver, { scheduler: h.scheduler }), error => {
    assert.match(error.message, /provider verification.*timed out/);
    assert.equal(error.diagnostics.last_value.linking.text, "Linking Sync Account...");
    return true;
  });
  assert.equal(h.clicks(), 1);
});

test("provider rejection is terminal, not pending until a deadline", async () => {
  const h = controlledLink({ failure: true });
  await assert.rejects(linkCloudJourneyPeer(h.driver, { scheduler: h.scheduler }), /permission denied/);
  assert.equal(h.clicks(), 1);
  assert.ok(h.elapsed() < E2E_TIMING.userResponseTargetMs);
});
