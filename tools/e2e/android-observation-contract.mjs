// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// App observations have exactly one owner: the rendering publisher. Android
// accessibility is reserved for external UI, input delivery, and diagnostics.
export function androidObservationBackend(tag) {
  if (tag === "" || tag.startsWith("parity:") || tag.startsWith("flight-data-cell:") ||
      tag.startsWith("org.aerobag.app:id/e2e_")) return "indexed";
  throw new Error(`No Android observation contract for ${tag}`);
}

export function rejectObservationOverrides(options) {
  if (Object.keys(options).length !== 0) {
    throw new Error(`Observation options are not target-owned: ${Object.keys(options).join(", ")}`);
  }
}
