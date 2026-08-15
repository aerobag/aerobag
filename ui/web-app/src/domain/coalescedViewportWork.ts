// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export type SequencedWork = {
  id: number;
};

// Coalescing replaces pending work. It must not starve the display by invalidating
// useful work that has already completed while the viewport continues to move.
export function shouldLandCompletedCoalescedWork<T extends SequencedWork>(
  completed: T,
  latest: T | null,
  lastLandedId: number,
  compatible: (completed: T, latest: T) => boolean,
): boolean {
  if (latest === null || completed.id <= lastLandedId) {
    return false;
  }
  return completed.id === latest.id || compatible(completed, latest);
}
