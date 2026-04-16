import { describe, expect, it } from "vitest";
import { ContentViewModel } from "./contentViewModel";
import type { AppState, CatalogJson, ContentAvailability, ContentInventory, ContentPolicy, FlightPlan } from "./types";

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
      procedure_airport_id: null,
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

class FakeContentAdapter {
  async replaceFlightPlanState(state: AppState, catalog: CatalogJson, nextPlan: FlightPlan): Promise<AppState> {
    if (nextPlan.legs.length === 0) {
      throw new Error("InvalidFlightPlan: flight plan must contain at least one leg");
    }
    const packageIds = new Map<string, CatalogJson["packages"][number]["id"]>();
    for (const leg of nextPlan.legs) {
      for (const ref of [leg.from, leg.to]) {
        if (!ref || !("Airport" in ref)) continue;
        const airportCode = ref.Airport.toUpperCase();
        for (const plate of catalog.plates) {
          if (plate.airport_id.toUpperCase() !== airportCode) continue;
          const pkg = catalog.packages.find((entry) => entry.region_id === plate.region_id);
          if (pkg) {
            packageIds.set(JSON.stringify(pkg.id), pkg.id);
          }
        }
      }
    }
    return {
      ...state,
      active_plan: nextPlan,
      last_content_requirements: [
        {
          package_ids: [...packageIds.values()],
          chart_ids: [],
          plate_ids: [],
        },
      ],
      last_content_report: null,
    };
  }

  async setContentPolicyState(state: AppState, _catalog: CatalogJson, policy: ContentPolicy): Promise<AppState> {
    return {
      ...state,
      content_policy: policy,
    };
  }

  async refreshContentState(state: AppState, _catalog: CatalogJson, inventory: ContentInventory): Promise<AppState> {
    const items = state.last_content_requirements.flatMap((requirement) =>
      requirement.package_ids.map((pkg) => {
        const installed = inventory.installed_packages.some(
          (entry) =>
            entry.integrity_ok &&
            entry.package_id.region === pkg.region &&
            entry.package_id.family === pkg.family &&
            entry.package_id.cycle === pkg.cycle,
        );

        const availability: ContentAvailability =
          installed
            ? state.content_policy === "StreamAllowed"
              ? "LocalAndRemote"
              : "LocalOnly"
            : state.content_policy === "StreamAllowed"
              ? "RemoteOnly"
              : "Unavailable";

        return {
          label: `${pkg.region.toUpperCase()}_${pkg.family === "sec" ? "SEC" : pkg.family.toUpperCase()}`,
          availability: {
            availability,
            cycle_current: true,
            integrity_ok: installed,
            cached: installed,
            offline_usable: installed,
          },
        };
      }),
    );

    const fullySatisfied = items.every((item) =>
      state.content_policy === "StreamAllowed"
        ? item.availability.availability !== "Unavailable"
        : item.availability.availability === "LocalOnly" || item.availability.availability === "LocalAndRemote",
    );

    return {
      ...state,
      last_content_report: {
        fully_satisfied: fullySatisfied,
        items,
      },
    };
  }
}

describe("ContentViewModel", () => {
  it("treats remote-only availability as satisfied for web streaming mode", async () => {
    const model = new ContentViewModel(new FakeContentAdapter(), undefined, catalog);
    await model.loadPlan(plan);
    await model.setPolicy("StreamAllowed");
    const state = await model.refresh(remoteOnlyInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.availability).toBe("RemoteOnly");
  });

  it("treats installed content as offline-usable for local-first mode", async () => {
    const model = new ContentViewModel(new FakeContentAdapter(), undefined, catalog);
    await model.loadPlan(plan);
    await model.setPolicy("OfflineRequired");
    const state = await model.refresh(installedInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.offline_usable).toBe(true);
  });
});
