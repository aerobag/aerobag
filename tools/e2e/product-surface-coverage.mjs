// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { validateJourneyRegistry } from "./release-journey-registry.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");

function read(relativePath) {
  return readFileSync(join(root, relativePath), "utf8");
}

function snakeCase(value) {
  return value.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

export function rustEnumVariants(source, enumName) {
  const match = new RegExp(`pub enum ${enumName}\\s*\\{([\\s\\S]*?)\\n\\}`, "m").exec(source);
  if (!match) throw new Error(`cannot find Rust enum ${enumName}`);
  return [...match[1].matchAll(/^\s*([A-Z][A-Za-z0-9_]*)\s*(?:[,({]|$)/gm)]
    .map((entry) => snakeCase(entry[1]));
}

export function chartFamilyIds(source) {
  const match = /fn supported_chart_families\(\)[\s\S]*?\{([\s\S]*?)\n\}/m.exec(source);
  if (!match) throw new Error("cannot find supported_chart_families");
  return [...match[1].matchAll(/\(\s*(?:NO_RASTER_FAMILY_ID|"([^"]+)")\s*,/g)]
    .map((entry) => entry[1] ?? "none");
}

export function settingsRowIds(source) {
  return [...source.matchAll(/const [A-Z0-9_]+_ROW_ID: &str = "([^"]+)";/g)]
    .map((entry) => entry[1]);
}

export function statusRowIds(statusSource, policySource) {
  const fixed = [
    "client", "publication:current_artifacts", "contracts:expected", "nav_db",
    "cycle:charts", "cycle:airport_docs", "static:base_data", "live_feed:connection",
  ];
  const products = [...policySource.matchAll(/product_id:\s*"([^"]+)"/g)]
    .map((entry) => `live_feed:${entry[1]}`);
  for (const id of fixed) {
    if (!statusSource.includes(`"${id}"`)) throw new Error(`status row ${id} disappeared from core`);
  }
  return [...fixed, ...products];
}

export function sourceProductSurfaces() {
  const contracts = read("ui/core-rust/crates/app-ui-contracts/src/session.rs");
  return {
    home_destinations: rustEnumVariants(
      read("ui/core-rust/crates/app-ui-contracts/src/home.rs"),
      "UiHomeDestination",
    ),
    navigation_pages: rustEnumVariants(contracts, "UiNavigationPageId"),
    raster_families: chartFamilyIds(read("ui/core-rust/crates/app-core/src/had_ops.rs")),
    map_layers: rustEnumVariants(contracts, "MapLayerId"),
    flight_plan_controls: rustEnumVariants(contracts, "FlightPlanControlId"),
    flight_plan_row_actions: rustEnumVariants(
      read("ui/core-rust/crates/app-core/src/planning.rs"),
      "FlightPlanRowActionId",
    ),
    procedure_kinds: rustEnumVariants(
      read("ui/core-rust/crates/app-core/src/planning.rs"),
      "ProcedureKind",
    ),
    settings_rows: settingsRowIds(
      read("ui/core-rust/crates/app-core/src/settings_controller.rs"),
    ),
    debug_flags: rustEnumVariants(contracts, "DebugFlagId"),
    status_rows: statusRowIds(
      read("ui/core-rust/crates/app-core/src/data_status_controller.rs"),
      read("crates/product-contracts/src/live_feed_policy.rs"),
    ),
    plate_controls: ["airport_selector", "chart_selector", "load_procedure", "folder"],
    altitude_planner_controls: rustEnumVariants(
      read("ui/core-rust/crates/app-core/src/altitude_planner.rs"),
      "AltitudePlannerControlId",
    ),
    altitude_planner_forecast_rows: rustEnumVariants(
      read("ui/core-rust/crates/app-core/src/altitude_planner.rs"),
      "AltitudePlannerForecastRowId",
    ),
    cloud_actions: rustEnumVariants(
      read("ui/core-rust/crates/app-ui-contracts/src/cloud.rs"),
      "CloudUiActionId",
    ),
  };
}

export function loadCoverageManifest(path = join(root, "tools/e2e/product-surface-coverage.json")) {
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  if (manifest.schema_version !== 1 || typeof manifest.surfaces !== "object") {
    throw new Error("product surface coverage schema_version must be 1");
  }
  return manifest;
}

export function verifyProductSurfaceCoverage(manifest = loadCoverageManifest(), actual = sourceProductSurfaces()) {
  const { assertion_owners: assertionOwners } = validateJourneyRegistry();
  const failures = [];
  for (const [category, actualIds] of Object.entries(actual)) {
    const coverage = manifest.surfaces[category];
    if (!coverage) {
      failures.push(`${category}: missing category`);
      continue;
    }
    const expectedIds = Object.keys(coverage).sort();
    const sourceIds = [...new Set(actualIds)].sort();
    const missing = sourceIds.filter((id) => !(id in coverage));
    const stale = expectedIds.filter((id) => !sourceIds.includes(id));
    if (missing.length) failures.push(`${category}: uncovered ${missing.join(", ")}`);
    if (stale.length) failures.push(`${category}: stale ${stale.join(", ")}`);
    for (const [id, assertionId] of Object.entries(coverage)) {
      if (assertionId === "uncovered") failures.push(`${category}.${id}: explicitly uncovered`);
      if (!assertionOwners.has(assertionId)) {
        failures.push(`${category}.${id}: unknown assertion ${assertionId}`);
      }
    }
  }
  const extraCategories = Object.keys(manifest.surfaces).filter((key) => !(key in actual));
  if (extraCategories.length) failures.push(`unknown categories: ${extraCategories.join(", ")}`);
  if (failures.length) throw new Error(`product surface coverage failed:\n${failures.join("\n")}`);
  return { categories: Object.keys(actual).length, branches: Object.values(actual).flat().length };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const summary = verifyProductSurfaceCoverage();
  process.stdout.write(`product surface coverage: ${summary.branches} branches in ${summary.categories} categories\n`);
}
