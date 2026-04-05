import type { AppState, CatalogJson, ContentInventory, FlightPlan } from "./types";

export const sampleCatalog: CatalogJson = {
  schema_version: 1,
  cycle: "2026-04-16",
  catalog_revision: "2026-04-05T22:00:00Z",
  families: [
    {
      id: "sectional",
      display_name: "VFR Sectional Charts",
      kind: "tiled_raster",
      max_zoom: 10,
      tile_size: 512,
    },
  ],
  regions: [
    {
      id: "ne",
      display_name: "Northeast",
      sort_order: 0,
    },
  ],
  packages: [
    {
      id: {
        region: "ne",
        family: "sectional",
        cycle: "2026-04-16",
      },
      package_name: "NE_SEC",
      family_id: "sectional",
      region_id: "ne",
      cycle: "2026-04-16",
      artifact_kind: "zip",
      relative_url: "/2026-04-16/NE_SEC.zip",
      manifest_name: "NE_SEC",
      size_bytes: null,
      checksum_sha256: null,
    },
  ],
  charts: [],
  plates: [
    {
      id: {
        airport_id: "KBOS",
        procedure_code: "IAP-ILS-RWY-04R",
        page: 1,
        cycle: "2026-04-16",
      },
      airport_id: "KBOS",
      region_id: "ne",
      cycle: "2026-04-16",
      procedure_code: "IAP-ILS-RWY-04R",
      display_name: "ILS OR LOC RWY 04R",
      kind: "approach",
      georeferenced: true,
      page_count: 1,
      asset_base_path: "plates/KBOS/IAP-ILS-RWY-04R",
    },
  ],
  supplements: [],
};

export const samplePlan: FlightPlan = {
  id: "plan-1",
  name: "KBOS local",
  legs: [
    {
      from: { Airport: "KBOS" },
      to: { Airport: "KBOS" },
      airway: null,
    },
  ],
  departure: "KBOS",
  destination: "KBOS",
  alternate: null,
  cruise_altitude_ft: 3000,
  notes: "Prototype content sync scenario",
  updated_at_epoch_ms: 0,
  version: 1,
};

export const emptyState: AppState = {
  active_plan: null,
  content_policy: "PreferLocal",
  last_content_requirements: [],
  last_content_report: null,
};

export const remoteOnlyInventory: ContentInventory = {
  installed_packages: [],
  cached_tilesets: [],
  cached_plates: [],
};

export const installedInventory: ContentInventory = {
  installed_packages: [
    {
      package_id: {
        region: "ne",
        family: "sectional",
        cycle: "2026-04-16",
      },
      integrity_ok: true,
    },
  ],
  cached_tilesets: [],
  cached_plates: [],
};
