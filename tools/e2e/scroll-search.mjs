// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export async function findScrollTarget({ read, scroll, maxSteps = 16 }) {
  let steps = 0;
  // Start with the beginning, then sweep the list. A separate allowance per
  // direction can spend the entire return allowance retracing visited rows.
  for (const direction of ["up", "down"]) {
    for (;;) {
      const target = await read();
      if (target) return target;
      if (steps >= maxSteps) {
        throw new Error(`scroll search exhausted its ${maxSteps} gestures while searching ${direction}; list not fully traversed`);
      }
      if (!await scroll(direction)) break;
      steps++;
    }
  }
  return null;
}
