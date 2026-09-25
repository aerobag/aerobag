// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Shared indexed modifiers own ordinary app targets, including their absence.
// These remaining legacy projections are deliberately read through accessibility,
// never tried in the index first. Keep the producer named when migrating one.
export const ANDROID_ACCESSIBILITY_TARGETS = Object.freeze([
  { owner: "AltitudePlannerPage.kt", exact: [
    "altitude-planner-forecast", "altitude-planner-status", "altitude-comparison-panel",
    "altitude-comparison-loading", "altitude-planner-departure-time",
    "altitude-planner-departure-when",
  ], prefixes: ["altitude-planner-wind-row:"] },
  { owner: "ChartsPage.kt", exact: ["plate-surface", "plate-ownship-overlay"],
    prefixes: ["plate-flight-plan-overlay:"] },
  { owner: "CloudPage.kt", exact: ["cloud-setup-code-output", "cloud-copy-status"],
    prefixes: ["cloud-action-revision:", "cloud-panel:"] },
  { owner: "FlightPlanPage.kt", exact: [
    "plan-list", "plan-controls", "plan-row-tray-scrim", "plan-procedure-picker",
    "plan-append-route-feedback",
  ], prefixes: [] },
  { owner: "MainActivity.kt", exact: [], prefixes: ["app-process:"] },
  { owner: "MapExplorerPage.kt", exact: ["weather-detail-modal"], prefixes: [
    "map-feature:", "map-selection-detail-modal:", "airport-info-fact:", "airport-info-runways:", "airport-info-runway:",
  ] },
  { owner: "NotamWidgets.kt", exact: ["procedure-notam-modal"], prefixes: [] },
  { owner: "OfflinePackagesPage.kt", exact: ["offline-library-panel", "offline-packages-panel"],
    prefixes: ["offline-preferences:", "offline-core:", "offline-region:", "offline-product:", "offline-zoom-level:"] },
  { owner: "PlanWidgets.kt", exact: [], prefixes: ["plan-procedure-row:", "plan-weather-badge:", "plan-data:"] },
  { owner: "PlaybackWidget.kt", exact: ["playback-widget", "playback-overview"], prefixes: [] },
]);

export function androidObservationBackend(tag, { prefix = false } = {}) {
  for (const { exact, prefixes } of ANDROID_ACCESSIBILITY_TARGETS) {
    if (exact.some(name => tag === `parity:${name}`) ||
        prefixes.some(name => tag.startsWith(`parity:${name}`))) return "accessibility";
  }
  if (prefix && tag !== "" && ANDROID_ACCESSIBILITY_TARGETS.some(({ exact, prefixes }) =>
    [...exact, ...prefixes].some(name => `parity:${name}`.startsWith(tag)))) {
    throw new Error(`Android collection ${tag} spans observation backends; use a specific target family`);
  }
  if (tag === "" || tag.startsWith("parity:") || tag.startsWith("flight-data-cell:") ||
      tag.startsWith("org.aerobag.app:id/e2e_")) return "indexed";
  throw new Error(`No Android observation contract for ${tag}`);
}

export function rejectObservationOverrides(options) {
  if (Object.keys(options).length !== 0) {
    throw new Error(`Observation options are not target-owned: ${Object.keys(options).join(", ")}`);
  }
}
