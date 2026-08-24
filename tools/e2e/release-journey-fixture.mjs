// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

export const RELEASE_JOURNEY_FIXTURE_SCHEMA_VERSION = 1;

const REQUIRED_CAPABILITIES = Object.freeze([
  "reference_epoch_ms", "initial_viewport", "replay_trace", "second_publication",
  "airport.runway_complex", "airport.runway_fallback", "airport.published_tpa",
  "airport.derived_tpa", "airway", "procedure.sid", "procedure.star",
  "procedure.approach", "plate.georeferenced", "plate.multi_page_rotated",
  "plate.notam", "plate.geometry_warning", "plate.legend", "plate.inset",
  "document.csup", "document.other", "live_feeds.fresh", "live_feeds.mixed",
  "live_feeds.stale",
]);

export function nestedCapability(manifest, capability) {
  return capability.split(".").reduce((value, component) => value?.[component], manifest.capabilities);
}

export function validateReleaseJourneyFixture(manifest, manifestPath = "fixture.json") {
  if (manifest?.schema_version !== RELEASE_JOURNEY_FIXTURE_SCHEMA_VERSION) {
    throw new Error(`release journey fixture schema must be ${RELEASE_JOURNEY_FIXTURE_SCHEMA_VERSION}`);
  }
  if (manifest.fixture !== "release-journey-publication") {
    throw new Error("release journey fixture has the wrong fixture id");
  }
  if (!manifest.publication_root || !manifest.capabilities) {
    throw new Error("release journey fixture requires publication_root and capabilities");
  }
  for (const capability of REQUIRED_CAPABILITIES) {
    const value = nestedCapability(manifest, capability);
    if (value === undefined || value === null || value === "") {
      throw new Error(`release journey fixture is missing capability ${capability}`);
    }
  }
  for (const family of ["none", "sec", "tac", "flyway", "enr-l", "enr-h", "shaded-relief"]) {
    if (!manifest.capabilities.raster_families?.includes(family)) {
      throw new Error(`release journey fixture is missing raster family ${family}`);
    }
  }
  return {
    ...manifest,
    root: resolve(dirname(manifestPath)),
    publication_path: resolve(dirname(manifestPath), manifest.publication_root),
  };
}

export function loadReleaseJourneyFixture(path) {
  return validateReleaseJourneyFixture(JSON.parse(readFileSync(path, "utf8")), path);
}

export function releaseJourneyCapability(fixture, capability) {
  const value = nestedCapability(fixture, capability);
  if (value === undefined) throw new Error(`fixture capability ${capability} is unavailable`);
  return value;
}
