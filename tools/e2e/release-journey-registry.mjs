// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

const shared = ["web", "android"];

function journey(id, priority, platforms, assertions, options = {}) {
  return Object.freeze({ id, priority, platforms, assertions, ...options });
}

export const RELEASE_JOURNEYS = Object.freeze([
  journey("shared.startup-navigation", "p0", shared, [
    "disclaimer.accept-persist", "startup.supported-publication", "navigation.map",
    "navigation.charts", "navigation.flight-plan", "navigation.altitude-planner",
    "navigation.data-status", "navigation.settings", "navigation.home",
    "home.chart", "home.plate", "home.flight-plan", "home.altitude-planner",
    "home.data-status", "home.settings", "home.cloud", "home.about",
  ]),
  journey("shared.chart-basic-use", "p0", shared, [
    "chart.search", "chart.pan", "chart.zoom", "chart.raster-repaint",
    "chart.vector-repaint", "chart.inspect", "chart.ctr-on", "chart.ctr-off",
  ]),
  journey("shared.flight-plan-edit-and-navigate", "p0", shared, [
    "plan.route-valid", "plan.route-invalid", "plan.insert-before", "plan.insert-after",
    "plan.move-up", "plan.move-down", "plan.remove", "plan.remove-all-above",
    "plan.activate-leg", "plan.direct-to", "plan.activate-next-leg",
    "plan.stop-navigation", "plan.suspend-sequencing",
    "plan.unsuspend-sequencing", "plan.restore-direct-to", "plan.route-paint",
  ]),
  journey("shared.procedure-departure", "p0", shared, [
    "procedure.sid.select", "procedure.sid.render", "procedure.sid.show-plate",
    "procedure.sid.invariant", "procedure.sid.remove",
  ]),
  journey("shared.procedure-arrival", "p0", shared, [
    "procedure.star.select", "procedure.star.render", "procedure.star.show-plate",
    "procedure.star.invariant", "procedure.star.remove", "plate.multi-page-rotated",
  ]),
  journey("shared.procedure-approach", "p0", shared, [
    "procedure.approach.select", "procedure.approach.load-from-plate",
    "procedure.approach.render", "procedure.approach.replace",
    "procedure.approach.show-plate", "procedure.approach.remove",
  ]),
  journey("shared.plate-operate", "p0", shared, [
    "plate.airport-selector", "plate.chart-selector", "plate.folder",
    "plate.load-procedure", "plate.named-selection", "plate.first-last-page",
    "plate.pan", "plate.zoom", "plate.georeferenced-ownship", "plate.return-folder",
  ]),
  journey("android.offline-cold-start", "p0", ["android"], [
    "home.offline-packages", "navigation.offline-packages", "offline.select",
    "offline.sync", "offline.cold-start", "offline.chart", "offline.plate",
  ], { existing_test: "android.offline-cold-start" }),
  journey("android.rotation-session-retention-regression", "p0", ["android"], [
    "android.rotation-session-retention",
  ], { existing_test: "android.rotation-session-retention-regression" }),
  journey("web.nav-db-rollover", "p0", ["web"], ["web.navdb-advance", "web.navdb-reject"]),

  journey("shared.map-modes-and-overlays", "p1", shared, [
    "raster.none", "raster.sec", "raster.tac", "raster.flyway", "raster.enr-l",
    "raster.enr-h", "raster.shaded-relief", "layer.world-basemap", "layer.vectors",
    "layer.metars", "layer.nexrad", "layer.traffic", "layer.terrain-warning",
    "layer.offline-regions", "map.n-up", "map.trk-up", "map.track-gap",
    "map.warning", "map.chart-reference",
  ], { live_feed_profile: "mixed" }),
  journey("shared.inspector-details", "p1", shared, [
    "inspector.airport-priority", "inspector.spot-fallback", "inspector.distance-live",
    "inspector.terrain-async", "inspector.weather", "inspector.info",
    "inspector.plates", "inspector.disabled-reason", "plan.row-waypoint-info-opens",
    "plan.row-weather-opens", "plan.row-plates-opens",
  ]),
  journey("shared.airport-info", "p1", shared, [
    "airport-info.scroll", "airport-info.time-toggle", "airport-info.tpa-published",
    "airport-info.tpa-derived", "airport-info.runway-complex", "airport-info.runway-fallback",
  ]),
  journey("shared.flight-plan-airway-estimates", "p1", shared, [
    "plan.add-airway", "plan.airway-scroll", "plan.estimates-vectors",
    "plan.ete-scope", "plan.time-mode", "plan.weather-badge", "plan.undo", "plan.redo",
  ]),
  journey("shared.plate-advisories-and-references", "p1", shared, [
    "plate.notam", "plate.geometry-warning", "plate.legend", "plate.inset",
    "plate.composite-scroll",
  ]),
  journey("shared.altitude-planner", "p1", shared, [
    "altitude.open-from-plan", "altitude.aircraft", "altitude.aircraft-profile", "altitude.wind-model",
    "altitude.time", "altitude.altitude", "altitude.changed-estimate",
    "altitude.forecast-fallback", "altitude.unavailable-reason",
  ]),
  journey("shared.status-and-settings", "p1", shared, [
    "status.all-rows", "status.fresh-stale-missing", "settings.flight-data-visibility",
    "settings.debug-folded", "settings.debug-toggle", "settings.debug.tile-labels",
    "settings.debug.nexrad-tile-labels", "settings.debug.fast-tiles",
    "settings.debug.offline-clock", "settings.debug.sequencing-finish-lines",
    "settings.debug.plate-flight-plan", "settings.debug.bad-autopilot",
    "settings.debug.internet-adsb", "settings.debug.gps-capture",
    "settings.debug.developer-log", "status.client", "status.publication",
    "status.contracts", "status.nav-db", "status.cycle-charts",
    "status.cycle-airport-docs", "status.static-base-data", "status.live-feed-connection",
    "status.tfrs", "status.notams", "status.metars", "status.pireps", "status.tafs",
    "status.nexrad", "status.obstacles", "status.winds-aloft",
  ], { live_feed_profile: "mixed" }),
  journey("shared.replay-track-up", "p1", shared, [
    "replay.load", "replay.play-pause", "replay.rate", "replay.seek",
    "replay.ownship", "replay.rotation", "replay.track-gap",
  ]),
  journey("shared.cloud-crossfill", "p1", shared, [
    "cloud.begin-setup", "cloud.begin-create", "cloud.back-setup", "cloud.scan-code",
    "cloud.accept-code", "cloud.create-account", "cloud.backup-code", "cloud.add-device",
    "cloud.close-detail", "cloud.begin-unlink", "cloud.confirm-unlink", "cloud.sync-now",
    "cloud.copy-code", "cloud.crossfill-plan", "cloud.crossfill-packages", "cloud.reconnect",
  ], { android_isolated: true }),
  journey("shared.prepared-live-feeds", "p1", shared, ["livefeed.metar-taf-pirep-notam"]),
  journey("shared.nexrad-frames", "p1", shared, [
    "livefeed.nexrad-frames",
    "livefeed.nexrad-hold",
    "livefeed.nexrad-resume",
  ]),
  journey("shared.obstacles-navkv", "p1", shared, ["livefeed.obstacles-navkv"]),
  journey("shared.winds-aloft-navkv", "p1", shared, ["livefeed.winds-aloft-navkv"]),
  journey("shared.tfr-map-detail", "p1", shared, ["livefeed.tfr-map-detail"]),
  journey("web.raster-load-recovery", "p1", ["web"], ["web.raster-load-recovery"]),

  journey("android.package-maintenance", "p2", ["android"], [
    "settings.display-dim-timeout", "settings.inactivity-sleep-timeout",
    "offline.update", "offline.interrupted-sync",
  ]),
  journey("web.pointer-details", "p2", ["web"], ["web.metar-hover", "web.weather-copy"]),
  journey("shared.other-documents", "p2", shared, ["plate.csup", "plate.other-document"]),
  journey("shared.about-and-saved-state", "p2", shared, [
    "navigation.about", "saved-state.restart",
  ]),
  // Contract failure clears installed/runtime state, so it must run last in grouped suites.
  journey("shared.contract-failures", "p2", shared, ["startup.unsupported-contract"]),
]);

export function journeyById(id) {
  return RELEASE_JOURNEYS.find((entry) => entry.id === id) ?? null;
}

export function assertionOwner(assertionId) {
  return RELEASE_JOURNEYS.find((entry) => entry.assertions.includes(assertionId)) ?? null;
}

export function validateJourneyRegistry(journeys = RELEASE_JOURNEYS) {
  const ids = new Set();
  const assertions = new Map();
  for (const entry of journeys) {
    if (!/^(shared|web|android)\.[a-z0-9.-]+$/.test(entry.id)) {
      throw new Error(`invalid journey id ${entry.id}`);
    }
    if (ids.has(entry.id)) throw new Error(`duplicate journey id ${entry.id}`);
    ids.add(entry.id);
    if (![["p0"], ["p1"], ["p2"]].flat().includes(entry.priority)) {
      throw new Error(`invalid priority for ${entry.id}`);
    }
    if (!entry.platforms.length || entry.platforms.some((value) => !["web", "android"].includes(value))) {
      throw new Error(`invalid platforms for ${entry.id}`);
    }
    for (const assertionId of entry.assertions) {
      const previous = assertions.get(assertionId);
      if (previous) throw new Error(`assertion ${assertionId} is owned by ${previous} and ${entry.id}`);
      assertions.set(assertionId, entry.id);
    }
  }
  return { journey_ids: ids, assertion_owners: assertions };
}
