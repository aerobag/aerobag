// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

"use strict";

// One transport request plus one replaceable pending edit. Responses from an
// older edit/chart must never replace the current rendered boundary.
function latestPreview(request, receive, fail) {
  let generation = 0;
  let pending = null;
  let running = false;
  async function pump() {
    if (running) return;
    running = true;
    try {
      while (pending) {
        const current = pending;
        pending = null;
        try {
          const result = await request(current.value);
          if (current.generation === generation) receive(result);
        } catch (error) {
          if (current.generation === generation) fail(error);
        }
      }
    } finally {
      running = false;
    }
  }
  return {
    submit(value) {
      pending = { value, generation: ++generation };
      void pump();
    },
    reset() {
      ++generation;
      pending = null;
    },
  };
}

if (typeof module !== "undefined") module.exports = latestPreview;
