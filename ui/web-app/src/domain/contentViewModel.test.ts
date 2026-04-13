import { describe, expect, it } from "vitest";
import { MockAppCoreAdapter } from "./appCoreAdapter";
import { ContentViewModel } from "./contentViewModel";
import type { CatalogJson, ContentInventory, FlightPlan } from "./types";

const catalog: CatalogJson = {
  schema_version: 1,
  cycle: "2026-04-16",
  catalog_revision: "test",
  families: [],
  regions: [
    { id: "ne", display_name: "Northeast", sort_order: 0 },
  ],
  packages: [
    {
      id: { region: "ne", family: "sec", cycle: "2026-04-16" },
      package_name: "NE_SEC",
      family_id: "sec",
      region_id: "ne",
      cycle: "2026-04-16",
      artifact_kind: "zip",
      relative_url: "/2026-04-16/NE_SEC.zip",
      manifest_name: "NE_SEC",
      size_bytes: 1,
      checksum_sha256: null,
    },
  ],
  charts: [],
  plates: [
    {
      id: {
        airport_id: "BOS",
        procedure_code: "IAP-TEST",
        page: 1,
        cycle: "2026-04-16",
      },
      airport_id: "BOS",
      region_id: "ne",
      cycle: "2026-04-16",
      procedure_code: "IAP-TEST",
      display_name: "Test plate",
      kind: "approach",
      georeferenced: false,
      page_count: 1,
      asset_base_path: "plates/BOS/IAP-TEST",
    },
  ],
  supplements: [],
};

const plan: FlightPlan = {
  id: "plan-1",
  name: "BOS local",
  legs: [
    {
      from: { Airport: "BOS" },
      to: { Airport: "BOS" },
      airway: null,
    },
  ],
  route_components: [
    { kind: "waypoint", waypoint: { Airport: "BOS" } },
    { kind: "waypoint", waypoint: { Airport: "BOS" } },
  ],
  resolved_legs: [
    {
      id: "component-0-1",
      from: { Airport: "BOS" },
      to: { Airport: "BOS" },
      source: { kind: "route_component", component_index: 0 },
      procedure_provenance: null,
    },
  ],
  guidance: null,
  departure: "BOS",
  destination: "BOS",
  alternate: null,
  cruise_altitude_ft: 3000,
  notes: null,
  updated_at_epoch_ms: 0,
  version: 1,
};

const remoteOnlyInventory: ContentInventory = {
  installed_packages: [],
  cached_tilesets: [],
  cached_plates: [],
};

const installedInventory: ContentInventory = {
  installed_packages: [
    {
      package_id: { region: "ne", family: "sec", cycle: "2026-04-16" },
      integrity_ok: true,
    },
  ],
  cached_tilesets: [],
  cached_plates: [],
};

describe("ContentViewModel", () => {
  it("treats remote-only availability as satisfied for web streaming mode", async () => {
    const model = new ContentViewModel(new MockAppCoreAdapter(), undefined, catalog);
    await model.loadPlan(plan);
    await model.setPolicy("StreamAllowed");
    const state = await model.refresh(remoteOnlyInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.availability).toBe("RemoteOnly");
  });

  it("treats installed content as offline-usable for local-first mode", async () => {
    const model = new ContentViewModel(new MockAppCoreAdapter(), undefined, catalog);
    await model.loadPlan(plan);
    await model.setPolicy("OfflineRequired");
    const state = await model.refresh(installedInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.offline_usable).toBe(true);
  });
});
