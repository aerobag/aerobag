// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const E2E_TIMING = Object.freeze({
  userResponseMs: 3_000,
  localReadyMs: 3_000,
  stabilityMs: 1_500,
  syntheticOwnshipProgressMs: 5_000,
  stabilityPollIntervalMs: 100,
  transitionReadinessSamples: 2,
  transitionCompletionSamples: 2,
  stableObservationSamples: 3,
  localRenderMs: 3_000,
  localResourceMs: 15_000,
  replayProgressMs: 15_000,
  animationCycleMs: 15_000,
  androidRecreationMs: 15_000,
  startupMs: 60_000,
  resourceMs: 60_000,
  cloudConsistencyMs: 30_000,
  externalConsistencyMs: 90_000,
  bulkOperationMs: 120_000,
  offlineSyncMs: 300_000,
  pollIntervalMs: 50,
  resourcePollIntervalMs: 250,
});

function diagnosticValue(value) {
  if (value == null || typeof value === "string" || typeof value === "number" ||
      typeof value === "boolean") return value;
  try {
    const encoded = JSON.stringify(value);
    return encoded.length <= 1_000 ? JSON.parse(encoded) : `${encoded.slice(0, 997)}...`;
  } catch {
    return String(value);
  }
}

export class ObservationTimeoutError extends Error {
  constructor(description, elapsedMs, diagnostics, lastError = null) {
    super(
      `${description} timed out after ${elapsedMs}ms` +
        (lastError ? `: ${lastError.message}` : ""),
    );
    this.name = "ObservationTimeoutError";
    this.diagnostics = diagnostics;
    if (lastError) this.cause = lastError;
  }
}

export class TerminalObservationError extends Error {
  constructor(description, detail, options = undefined) {
    super(`${description}: ${detail}`, options);
    this.name = "TerminalObservationError";
  }
}

export class TransientObservationError extends Error {
  constructor(detail, cause = undefined) {
    super(detail, cause ? { cause } : undefined);
    this.name = "TransientObservationError";
  }
}

export async function observeUntil(
  description,
  probe,
  {
    timeoutMs = E2E_TIMING.localReadyMs,
    intervalMs = E2E_TIMING.pollIntervalMs,
    consecutiveSuccesses = 1,
    consecutiveValueKey = null,
    waitForNextProbe = null,
  } = {},
) {
  if (!Number.isInteger(consecutiveSuccesses) || consecutiveSuccesses < 1) {
    throw new Error("consecutiveSuccesses must be a positive integer");
  }
  const startedAt = performance.now();
  const deadline = startedAt + timeoutMs;
  let lastError = null;
  let successfulSamples = 0;
  let successfulValue = null;
  let successfulKey = null;
  let lastValue = null;
  let attempts = 0;
  while (performance.now() < deadline) {
    try {
      attempts += 1;
      const value = await probe();
      lastValue = diagnosticValue(value);
      const probeFinishedAt = performance.now();
      if (probeFinishedAt >= deadline) break;
      if (value) {
        const currentKey = consecutiveValueKey ? consecutiveValueKey(value) : null;
        if (successfulSamples > 0 && consecutiveValueKey && currentKey !== successfulKey) {
          successfulSamples = 1;
        } else {
          successfulSamples += 1;
        }
        successfulValue = value;
        successfulKey = currentKey;
        if (successfulSamples >= consecutiveSuccesses) {
          return { value: successfulValue, durationMs: Math.round(probeFinishedAt - startedAt) };
        }
      } else {
        successfulSamples = 0;
        successfulValue = null;
        successfulKey = null;
      }
    } catch (error) {
      if (error instanceof TerminalObservationError) throw error;
      if (!(error instanceof TransientObservationError)) {
        throw new TerminalObservationError(description, error.message, { cause: error });
      }
      lastError = error;
      successfulSamples = 0;
      successfulValue = null;
      successfulKey = null;
    }
    const remainingMs = Math.max(0, deadline - performance.now());
    if (remainingMs <= 0) break;
    if (waitForNextProbe) {
      await waitForNextProbe(Math.min(intervalMs, remainingMs));
    } else {
      await new Promise((resolve) => setTimeout(resolve, Math.min(intervalMs, remainingMs)));
    }
  }
  const elapsedMs = Math.round(performance.now() - startedAt);
  throw new ObservationTimeoutError(
    description,
    elapsedMs,
    {
      description,
      elapsed_ms: elapsedMs,
      timeout_ms: timeoutMs,
      attempts,
      successful_samples: successfulSamples,
      required_successful_samples: consecutiveSuccesses,
      last_value: lastValue,
      last_error: lastError?.message ?? null,
    },
    lastError,
  );
}

function transitionReadinessKey(value) {
  if (!value || typeof value !== "object") return JSON.stringify(value);
  return JSON.stringify({
    semantic_id: value.test_id ?? value.id ?? null,
    enabled: value.enabled ?? null,
    actionable: value.actionable ?? null,
    bounds: value.bounds ?? null,
  });
}

export async function observeChangedValueUntilStable(
  description,
  probe,
  {
    initialValue,
    timeoutMs = E2E_TIMING.userResponseMs,
    intervalMs = E2E_TIMING.pollIntervalMs,
    stableSamples = E2E_TIMING.transitionCompletionSamples + 1,
    valueKey = (value) => value,
    waitForNextProbe = null,
  } = {},
) {
  if (!Number.isInteger(stableSamples) || stableSamples < 2) {
    throw new Error("stableSamples must be at least two");
  }
  const initialKey = valueKey(initialValue);
  let previousKey = initialKey;
  let unchangedSamples = 0;
  return observeUntil(description, async () => {
    const value = await probe();
    const key = valueKey(value);
    if (key === initialKey) {
      previousKey = key;
      unchangedSamples = 0;
      return null;
    }
    if (key !== previousKey) {
      previousKey = key;
      unchangedSamples = 1;
      return null;
    }
    unchangedSamples += 1;
    return unchangedSamples >= stableSamples ? value : null;
  }, {
    timeoutMs,
    intervalMs,
    waitForNextProbe,
  });
}

export async function observeValueUntilStable(
  description,
  probe,
  {
    timeoutMs = E2E_TIMING.localReadyMs,
    intervalMs = E2E_TIMING.pollIntervalMs,
    stableSamples = E2E_TIMING.stableObservationSamples,
    valueKey = (value) => JSON.stringify(value),
    waitForNextProbe = null,
  } = {},
) {
  return observeUntil(description, probe, {
    timeoutMs,
    intervalMs,
    consecutiveSuccesses: stableSamples,
    consecutiveValueKey: valueKey,
    waitForNextProbe,
  });
}

export async function assertConditionRemains(
  description,
  probe,
  accept,
  {
    durationMs,
    intervalMs = E2E_TIMING.pollIntervalMs,
  },
) {
  const startedAt = performance.now();
  let samples = 0;
  while (performance.now() - startedAt < durationMs) {
    const value = await probe();
    samples += 1;
    if (!accept(value)) {
      throw new Error(`${description} failed after ${Math.round(performance.now() - startedAt)}ms`);
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return { durationMs: Math.round(performance.now() - startedAt), samples };
}

export async function performTransition(description, {
  ready,
  act,
  complete,
  diagnose = null,
  waitForObservation = null,
  readyTimeoutMs = E2E_TIMING.localReadyMs,
  responseTimeoutMs = E2E_TIMING.userResponseMs,
  intervalMs = E2E_TIMING.pollIntervalMs,
  readinessSamples = E2E_TIMING.transitionReadinessSamples,
  onTiming = null,
}) {
  if (!Number.isInteger(readinessSamples) || readinessSamples < 1) {
    throw new Error(`${description} readinessSamples must be a positive integer`);
  }
  if (responseTimeoutMs > E2E_TIMING.userResponseMs) {
    throw new Error(
      `${description} requests a ${responseTimeoutMs}ms user-response budget; ` +
        `user transitions are capped at ${E2E_TIMING.userResponseMs}ms. ` +
        "Observe longer resource or temporal work as a separate named phase.",
    );
  }
  const transitionStartedAt = performance.now();
  const timing = {
    description,
    outcome: "fail",
    failure_phase: "ready",
    ready_ms: null,
    action_ms: null,
    completion_ms: null,
    response_ms: null,
    total_ms: null,
    response_budget_ms: responseTimeoutMs,
    ready_state: null,
    action_result: null,
    observation: null,
    diagnostic_state: null,
  };
  const recordFailure = async (phase, error = null) => {
    timing.failure_phase = phase;
    if (diagnose) {
      try {
        timing.diagnostic_state = diagnosticValue(await diagnose());
      } catch (diagnosticError) {
        timing.diagnostic_state = { diagnostic_error: diagnosticError.message };
      }
    }
    onTiming?.(timing);
    if (error) throw error;
  };
  let readyResult;
  try {
    readyResult = await observeUntil(`${description} ready`, ready, {
      timeoutMs: readyTimeoutMs,
      intervalMs,
      consecutiveSuccesses: readinessSamples,
      consecutiveValueKey: transitionReadinessKey,
      waitForNextProbe: waitForObservation,
    });
    timing.ready_ms = readyResult.durationMs;
    timing.ready_state = diagnosticValue(readyResult.value);
  } catch (error) {
    timing.total_ms = Math.round(performance.now() - transitionStartedAt);
    timing.observation = error.diagnostics ?? null;
    await recordFailure("ready", error);
  }
  let completionBeforeAction = null;
  try {
    completionBeforeAction = await complete();
  } catch {
    // Completion probes use the same absent-state convention as observeUntil:
    // a missing postcondition may either return null or fail to read.
  }
  if (completionBeforeAction) {
    timing.total_ms = Math.round(performance.now() - transitionStartedAt);
    timing.observation = diagnosticValue(completionBeforeAction);
    await recordFailure(
      "precondition",
      new Error(`${description} completion was already satisfied before the action`),
    );
  }
  const actionStartedAt = performance.now();
  let actionResult;
  try {
    actionResult = await act(readyResult.value);
    timing.action_result = diagnosticValue(actionResult);
  } catch (error) {
    timing.action_ms = Math.round(performance.now() - actionStartedAt);
    timing.response_ms = timing.action_ms;
    timing.total_ms = Math.round(performance.now() - transitionStartedAt);
    timing.observation = { error: error.message };
    await recordFailure("action", error);
  }
  const actionDurationMs = Math.round(performance.now() - actionStartedAt);
  timing.action_ms = actionDurationMs;
  const remainingResponseMs = responseTimeoutMs - (performance.now() - actionStartedAt);
  if (remainingResponseMs <= 0) {
    timing.response_ms = actionDurationMs;
    timing.total_ms = Math.round(performance.now() - transitionStartedAt);
    await recordFailure(
      "action",
      new Error(`${description} action exceeded the ${responseTimeoutMs}ms user-response budget`),
    );
  }
  let completion;
  try {
    completion = await observeUntil(`${description} completed`, complete, {
      timeoutMs: remainingResponseMs,
      intervalMs,
      consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
      waitForNextProbe: waitForObservation,
    });
  } catch (error) {
    timing.completion_ms = Math.round(performance.now() - actionStartedAt - actionDurationMs);
    timing.response_ms = Math.round(performance.now() - actionStartedAt);
    timing.total_ms = Math.round(performance.now() - transitionStartedAt);
    timing.observation = error.diagnostics ?? null;
    await recordFailure("completion", error);
  }
  const responseDurationMs = Math.round(performance.now() - actionStartedAt);
  Object.assign(timing, {
    outcome: "pass",
    failure_phase: null,
    completion_ms: completion.durationMs,
    response_ms: responseDurationMs,
    total_ms: Math.round(performance.now() - transitionStartedAt),
    observation: diagnosticValue(completion.value),
  });
  onTiming?.(timing);
  return { value: completion.value, timing };
}
