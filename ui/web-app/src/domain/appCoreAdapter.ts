import type {
  AppState,
  CatalogJson,
  ContentAvailability,
  ContentInventory,
  ContentPolicy,
  FlightPlan,
} from "./types";

export interface AppCoreAdapter {
  replaceFlightPlanState(state: AppState, catalog: CatalogJson, plan: FlightPlan): Promise<AppState>;
  setContentPolicyState(state: AppState, catalog: CatalogJson, policy: ContentPolicy): Promise<AppState>;
  refreshContentState(state: AppState, catalog: CatalogJson, inventory: ContentInventory): Promise<AppState>;
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

function airportCode(ref: FlightPlan["legs"][number]["from"]): string | null {
  if ("Airport" in ref) {
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
}

type WasmModule = {
  replace_flight_plan_state(stateJson: string, catalogJson: string, planJson: string): Promise<string> | string;
  set_content_policy_state(stateJson: string, catalogJson: string, policyJson: string): Promise<string> | string;
  refresh_content_state(stateJson: string, catalogJson: string, inventoryJson: string): Promise<string> | string;
};

export class WasmAppCoreAdapter implements AppCoreAdapter {
  constructor(private readonly module: WasmModule) {}

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
}

export async function loadBestAvailableAdapter(
  importer: (path: string) => Promise<unknown> = (path) => import(/* @vite-ignore */ path),
): Promise<LoadedAdapter> {
  try {
    const mod = (await importer("/generated/app_wasm.js")) as Partial<WasmModule>;
    if (
      typeof mod.replace_flight_plan_state !== "function" ||
      typeof mod.set_content_policy_state !== "function" ||
      typeof mod.refresh_content_state !== "function"
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
