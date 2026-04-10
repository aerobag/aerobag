import type {
  AppState,
  CatalogJson,
  ContentInventory,
  ContentPolicy,
  FlightPlan,
  ChartFamilyId,
  ContentAvailability,
} from "./types";

export interface AppCoreAdapter {
  replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState>;
  removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan>;
  setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState>;
  refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState>;
  chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null>;
}

export type AdapterBackendKind = "mock" | "wasm";

export type LoadedAdapter = {
  adapter: AppCoreAdapter;
  backend: AdapterBackendKind;
  detail: string;
};

function packageName(region: string, family: string): string {
  const regionCode = region.toUpperCase();
  const familyCode =
    family === "sectional"
      ? "SEC"
      : family === "ifr_low"
        ? "ENR_L"
        : family === "ifr_high"
          ? "ENR_H"
          : family === "ifr_area"
            ? "ENR_A"
            : family.toUpperCase();
  return `${regionCode}_${familyCode}`;
}

function airportCode(ref: FlightPlan["legs"][number]["from"] | null | undefined): string | null {
  if (ref && "Airport" in ref) {
    return ref.Airport;
  }
  return null;
}

export class MockAppCoreAdapter implements AppCoreAdapter {
  async replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState> {
    if (plan.legs.length === 0) {
      throw new Error("InvalidFlightPlan: flight plan must contain at least one leg");
    }

    const packageMap = new Map<
      string,
      CatalogJson["packages"][number]["id"]
    >();

    for (const leg of plan.legs) {
      for (const ref of [leg.from, leg.to]) {
        const code = airportCode(ref);
        if (!code) continue;
        for (const plate of catalog.plates) {
          if (plate.airport_id.toUpperCase() !== code.toUpperCase()) continue;
          const pkg = catalog.packages.find((entry) => entry.region_id === plate.region_id);
          if (pkg) {
            packageMap.set(JSON.stringify(pkg.id), pkg.id);
          }
        }
      }
    }

    return {
      ...state,
      active_plan: plan,
      last_content_requirements: [
        {
          package_ids: [...packageMap.values()],
          chart_ids: [],
          plate_ids: [],
        },
      ],
      last_content_report: null,
    };
  }

  async removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan> {
    if (index < 0 || index >= plan.legs.length) {
      throw new Error(`InvalidFlightPlan: flight plan leg index out of range: ${index}`);
    }
    const legs = plan.legs.filter((_, legIndex) => legIndex !== index);
    if (legs.length === 0) {
      throw new Error("InvalidFlightPlan: flight plan must contain at least one leg");
    }
    return {
      ...plan,
      legs,
      departure: airportCode(legs[0]?.from ?? null),
      destination: airportCode(legs[legs.length - 1]?.to ?? null),
      updated_at_epoch_ms: plan.updated_at_epoch_ms + 1,
      version: plan.version + 1,
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
          label: packageName(pkg.region, pkg.family),
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

  async chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null> {
    for (const chart of catalog.charts) {
      if (chart.family_id !== family) {
        continue;
      }

      const coverage = chart.coverage as { kind?: string; value?: { polygon_id?: string } };
      const polygonId = coverage.value?.polygon_id;
      if (coverage.kind !== "polygon_ref" || !polygonId) {
        continue;
      }

      const polygon = geometry.polygons.find((entry) => entry.id === polygonId);
      if (polygon && pointInPolygon(lat, lon, polygon.points)) {
        return chart;
      }
    }

    return null;
  }
}

type WasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  remove_flight_plan_leg(planJson: string, index: number): Promise<string> | string;
  replace_flight_plan_state(stateJson: string, catalogJson: string, planJson: string): Promise<string> | string;
  set_content_policy_state(stateJson: string, catalogJson: string, policyJson: string): Promise<string> | string;
  refresh_content_state(stateJson: string, catalogJson: string, inventoryJson: string): Promise<string> | string;
  chart_for_position(
    catalogJson: string,
    geometryJson: string,
    familyJson: string,
    lat: number,
    lon: number,
  ): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(private readonly module: WasmModule) {}

  async removeFlightPlanLeg(plan: FlightPlan, index: number): Promise<FlightPlan> {
    return JSON.parse(
      await this.module.remove_flight_plan_leg(
        JSON.stringify(plan),
        index,
      ),
    ) as FlightPlan;
  }

  async replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState> {
    return JSON.parse(
      await this.module.replace_flight_plan_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(plan),
      ),
    ) as AppState;
  }

  async setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState> {
    return JSON.parse(
      await this.module.set_content_policy_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(policy),
      ),
    ) as AppState;
  }

  async refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState> {
    return JSON.parse(
      await this.module.refresh_content_state(
        JSON.stringify(state),
        JSON.stringify(catalog),
        JSON.stringify(inventory),
      ),
    ) as AppState;
  }

  async chartForPosition(
    catalog: CatalogJson,
    geometry: { polygons: Array<{ id: string; points: number[][] }> },
    family: ChartFamilyId,
    lat: number,
    lon: number,
  ): Promise<CatalogJson["charts"][number] | null> {
    return JSON.parse(
      await this.module.chart_for_position(
        JSON.stringify(catalog),
        JSON.stringify(geometry),
        JSON.stringify(family),
        lat,
        lon,
      ),
    ) as CatalogJson["charts"][number] | null;
  }
}

export async function loadBestAvailableAdapter(
  importer: () => Promise<unknown> = () => import("@generated/app_wasm.js"),
): Promise<LoadedAdapter> {
  try {
    const mod = (await importer()) as Partial<WasmModule>;
    if (typeof mod.default === "function") {
      await mod.default();
    }
    if (
      typeof mod.replace_flight_plan_state !== "function" ||
      typeof mod.remove_flight_plan_leg !== "function" ||
      typeof mod.set_content_policy_state !== "function" ||
      typeof mod.refresh_content_state !== "function" ||
      typeof mod.chart_for_position !== "function"
    ) {
      throw new Error("generated wasm module is missing required exports");
    }

    return {
      adapter: new WasmAppCoreAdapter(mod as WasmModule),
      backend: "wasm",
      detail: "Using generated Rust WASM bindings.",
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      adapter: new MockAppCoreAdapter(),
      backend: "mock",
      detail: `Falling back to mock adapter: ${message}`,
    };
  }
}

function pointInPolygon(lat: number, lon: number, points: number[][]): boolean {
  let inside = false;
  let previousIndex = points.length - 1;

  for (let currentIndex = 0; currentIndex < points.length; currentIndex += 1) {
    const [currentLon, currentLat] = points[currentIndex];
    const [previousLon, previousLat] = points[previousIndex];
    const crossesLatitude = (currentLat > lat) !== (previousLat > lat);

    if (crossesLatitude) {
      const interpolatedLon =
        previousLon + ((currentLon - previousLon) * (lat - previousLat)) / (currentLat - previousLat);
      if (lon < interpolatedLon) {
        inside = !inside;
      }
    }

    previousIndex = currentIndex;
  }

  return inside;
}
