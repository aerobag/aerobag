// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export const realScheduler = Object.freeze({
  now: () => performance.now(),
  setTimeout: (callback, ms) => setTimeout(callback, ms),
  clearTimeout: (timer) => clearTimeout(timer),
});

export class DeadlineExceededError extends Error {
  constructor(description, deadline) {
    super(`${description} timed out`);
    this.name = "DeadlineExceededError";
    this.deadline = deadline;
  }
}

// Own the timer as well as the operation. A timed-out action is terminal:
// callers must tear down its journey, never retry it or continue to another
// action. AbortSignal lets cooperative work stop; it cannot undo a mutation
// already delivered to a browser/device.
export async function withinDeadline(description, operation, deadline, scheduler = realScheduler) {
  const controller = new AbortController();
  const expired = new DeadlineExceededError(description, deadline);
  const remaining = deadline - scheduler.now();
  if (remaining <= 0) throw expired;
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = scheduler.setTimeout(() => {
      reject(expired);
      controller.abort(expired);
    }, Math.ceil(remaining));
  });
  try {
    const result = await Promise.race([
      Promise.resolve().then(() => operation({ signal: controller.signal })), timeout,
    ]);
    // Synchronous CPU work can delay the timer itself; it still cannot earn
    // success outside the budget.
    if (scheduler.now() >= deadline) {
      controller.abort(expired);
      throw expired;
    }
    return result;
  } finally {
    scheduler.clearTimeout(timer);
  }
}
