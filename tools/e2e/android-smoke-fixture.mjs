// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";

export const ANDROID_SMOKE_FIXTURE_SCHEMA_VERSION = 1;

export function validateAndroidSmokeFixture(manifest) {
  if (manifest?.schema_version !== ANDROID_SMOKE_FIXTURE_SCHEMA_VERSION) {
    throw new Error(
      `Android smoke fixture schema must be ${ANDROID_SMOKE_FIXTURE_SCHEMA_VERSION}`,
    );
  }
  if (manifest.fixture !== "android-smoke-publication") {
    throw new Error("Android smoke fixture has the wrong fixture id");
  }
  const plate = manifest.capabilities?.plate?.georeferenced;
  if (typeof plate?.airport_id !== "string" || !plate.airport_id) {
    throw new Error("Android smoke fixture has no georeferenced plate airport");
  }
  if (typeof plate.label_contains !== "string" || !plate.label_contains) {
    throw new Error("Android smoke fixture has no georeferenced plate label");
  }
  return manifest;
}

export function loadAndroidSmokeFixture(path) {
  return validateAndroidSmokeFixture(JSON.parse(readFileSync(path, "utf8")));
}
