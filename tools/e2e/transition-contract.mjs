// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const E2E_TIMING = Object.freeze({
  userResponseMs: 3_000,
  localReadyMs: 3_000,
  stabilityMs: 1_500,
  stabilityPollIntervalMs: 100,
  transitionCompletionSamples: 2,
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

export async function observeUntil(
  description,
  probe,
  {
    timeoutMs = E2E_TIMING.localReadyMs,
    intervalMs = E2E_TIMING.pollIntervalMs,
    consecutiveSuccesses = 1,
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
  while (performance.now() < deadline) {
    try {
      const value = await probe();
      const probeFinishedAt = performance.now();
      if (probeFinishedAt >= deadline) break;
      if (value) {
        successfulSamples += 1;
        successfulValue = value;
        if (successfulSamples >= consecutiveSuccesses) {
          return { value: successfulValue, durationMs: Math.round(probeFinishedAt - startedAt) };
        }
      } else {
        successfulSamples = 0;
        successfulValue = null;
      }
    } catch (error) {
      lastError = error;
      successfulSamples = 0;
      successfulValue = null;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  const elapsedMs = Math.round(performance.now() - startedAt);
  throw new Error(
    `${description} timed out after ${elapsedMs}ms` +
      (lastError ? `: ${lastError.message}` : ""),
  );
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
  readyTimeoutMs = E2E_TIMING.localReadyMs,
  responseTimeoutMs = E2E_TIMING.userResponseMs,
  intervalMs = E2E_TIMING.pollIntervalMs,
  onTiming = null,
}) {
  if (responseTimeoutMs > E2E_TIMING.userResponseMs) {
    throw new Error(
      `${description} requests a ${responseTimeoutMs}ms user-response budget; ` +
        `user transitions are capped at ${E2E_TIMING.userResponseMs}ms. ` +
        "Observe longer resource or temporal work as a separate named phase.",
    );
  }
  const transitionStartedAt = performance.now();
  const readyResult = await observeUntil(`${description} ready`, ready, {
    timeoutMs: readyTimeoutMs,
    intervalMs,
  });
  const actionStartedAt = performance.now();
  await act(readyResult.value);
  const actionDurationMs = Math.round(performance.now() - actionStartedAt);
  const completion = await observeUntil(`${description} completed`, complete, {
    timeoutMs: responseTimeoutMs,
    intervalMs,
    consecutiveSuccesses: E2E_TIMING.transitionCompletionSamples,
  });
  const timing = {
    description,
    ready_ms: readyResult.durationMs,
    action_ms: actionDurationMs,
    response_ms: completion.durationMs,
    total_ms: Math.round(performance.now() - transitionStartedAt),
    response_budget_ms: responseTimeoutMs,
  };
  onTiming?.(timing);
  return { value: completion.value, timing };
}
