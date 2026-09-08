// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { DeadlineExceededError, withinDeadline } from "./deadline.mjs";
import {
  assertConditionRemains, observeUntil, ObservationTimeoutError, performTransition,
  TerminalObservationError, TransientObservationError,
} from "./transition-contract.mjs";

function manualScheduler() {
  let time = 0;
  let next = 0;
  const timers = new Map();
  return {
    now: () => time,
    setTimeout(callback, ms) { const id = ++next; timers.set(id, { at: time + ms, callback }); return id; },
    clearTimeout(id) { timers.delete(id); },
    advance(ms) {
      time += ms;
      for (const [id, timer] of [...timers]) {
        if (timer.at <= time) { timers.delete(id); timer.callback(); }
      }
    },
    pending: () => timers.size,
  };
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("an abandoned probe has a real deadline, abort signal, and no late success", async () => {
  const scheduler = manualScheduler();
  const entered = deferred(), late = deferred();
  let calls = 0;
  const observation = observeUntil("hung probe", ({ signal }) => {
    calls += 1;
    entered.resolve(signal);
    return late.promise;
  }, { timeoutMs: 10, scheduler });
  const signal = await entered.promise;
  const rejected = assert.rejects(observation, (error) => {
    assert.ok(error instanceof ObservationTimeoutError);
    assert.equal(error.diagnostics.elapsed_ms, 10);
    assert.equal(error.diagnostics.last_value, null);
    return true;
  });
  scheduler.advance(10);
  await rejected;
  assert.equal(signal.aborted, true);
  late.resolve(true);
  assert.equal(calls, 1);
  assert.equal(scheduler.pending(), 0);
});

test("an event wait cannot hide a missing notification beyond the observation deadline", async () => {
  const scheduler = manualScheduler();
  const waiting = deferred();
  const observation = observeUntil("lost event", () => false, {
    timeoutMs: 10, scheduler,
    waitForNextProbe: () => { waiting.resolve(); return new Promise(() => {}); },
  });
  await waiting.promise;
  const rejected = assert.rejects(observation, ObservationTimeoutError);
  scheduler.advance(10);
  await rejected;
  assert.equal(scheduler.pending(), 0);
});

test("deadline ownership removes timers on early success and rejection", async () => {
  const scheduler = manualScheduler();
  assert.equal(await withinDeadline("success", () => 42, 10, scheduler), 42);
  await assert.rejects(withinDeadline("failure", () => { throw new Error("broken"); }, 10, scheduler), /broken/);
  assert.equal(scheduler.pending(), 0);
  let calls = 0;
  scheduler.advance(10);
  await assert.rejects(withinDeadline("expired", () => { calls += 1; }, 10, scheduler), DeadlineExceededError);
  assert.equal(calls, 0);
});

test("temporal sampling cannot pass while its last probe is hung", async () => {
  const scheduler = manualScheduler();
  const entered = deferred();
  const observation = assertConditionRemains("stays visible", () => {
    entered.resolve(); return new Promise(() => {});
  }, Boolean, { durationMs: 10, probeTimeoutMs: 10, scheduler });
  await entered.promise;
  const rejected = assert.rejects(observation, DeadlineExceededError);
  scheduler.advance(10);
  await rejected;
  assert.equal(scheduler.pending(), 0);
});

test("the final temporal sample retains its read budget after the sampling window ends", async () => {
  const scheduler = manualScheduler();
  const entered = deferred(), sample = deferred();
  const observation = assertConditionRemains("stays visible", () => {
    entered.resolve(); return sample.promise;
  }, Boolean, { durationMs: 10, probeTimeoutMs: 20, scheduler });
  await entered.promise;
  scheduler.advance(11);
  sample.resolve(true);
  assert.deepEqual(await observation, { durationMs: 11, samples: 1 });
  assert.equal(scheduler.pending(), 0);
});

for (const error of [new TerminalObservationError("driver", "disconnected"), new Error("protocol bug")]) {
  test(`a failed pre-action probe forbids mutation: ${error.name}`, async () => {
    let actions = 0;
    const timings = [];
    await assert.rejects(performTransition("button", {
      ready: () => true, readinessSamples: 1,
      complete: () => { throw error; }, act: () => { actions += 1; },
      onTiming: (timing) => timings.push(timing),
    }), (caught) => caught === error || caught.cause === error);
    assert.equal(actions, 0);
    assert.equal(timings[0].failure_phase, "precondition");
  });
}

test("transient pre-action reads retry the read, not the action", async () => {
  let probes = 0, actions = 0;
  const result = await performTransition("button", {
    ready: () => true, readinessSamples: 1, completionSamples: 1,
    waitForObservation: async () => {},
    complete: () => {
      if (++probes === 1) throw new TransientObservationError("context replaced");
      return actions > 0;
    },
    act: () => { actions += 1; },
  });
  assert.equal(result.value, true);
  assert.equal(actions, 1);
  assert.equal(probes, 3);
});

test("a hung mutation fails once and never starts completion or another action", async () => {
  const scheduler = manualScheduler();
  const entered = deferred(), late = deferred();
  let actions = 0, completions = 0;
  const timings = [];
  const transition = performTransition("hung delivery", {
    scheduler, responseTimeoutMs: 10, readinessSamples: 1,
    ready: () => true,
    complete: () => { completions += 1; return false; },
    act: (_evidence, { signal }) => { actions += 1; entered.resolve(signal); return late.promise; },
    onTiming: (timing) => timings.push(timing),
  });
  const signal = await entered.promise;
  const rejected = assert.rejects(transition, /hung delivery action timed out/);
  scheduler.advance(10);
  await rejected;
  late.resolve("late accepted delivery");
  assert.equal(actions, 1);
  assert.equal(completions, 1); // precondition only
  assert.equal(signal.aborted, true);
  assert.equal(timings[0].failure_phase, "action");
  assert.equal(scheduler.pending(), 0);
});

test("failure diagnostics cannot hang or replace the original terminal failure", async () => {
  const scheduler = manualScheduler();
  const entered = deferred();
  const original = new TerminalObservationError("ready", "crashed");
  const timings = [];
  const transition = performTransition("broken", {
    scheduler, diagnosticTimeoutMs: 10,
    ready: () => { throw original; }, act: () => assert.fail("must not act"), complete: () => false,
    diagnose: () => { entered.resolve(); return new Promise(() => {}); },
    onTiming: (timing) => timings.push(timing),
  });
  await entered.promise;
  const rejected = assert.rejects(transition, (error) => error === original);
  scheduler.advance(10);
  await rejected;
  assert.match(timings[0].diagnostic_state.diagnostic_error, /diagnostics timed out/);
  assert.equal(scheduler.pending(), 0);
});
