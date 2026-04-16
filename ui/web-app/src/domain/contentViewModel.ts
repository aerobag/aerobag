import type { AppState, ContentInventory, ContentPolicy, FlightPlan } from "./types";
import type { AppCoreAdapter } from "./appCoreAdapter";
import { emptyState, sampleCatalog } from "./sampleData";
import type { CatalogJson } from "./types";

type ContentStateAdapter = Pick<
  AppCoreAdapter,
  "replaceFlightPlanState" | "setContentPolicyState" | "refreshContentState"
>;

export class ContentViewModel {
  constructor(
    private readonly adapter: ContentStateAdapter,
    private state: AppState = emptyState,
    private readonly catalog: CatalogJson = sampleCatalog,
  ) {}

  get snapshot(): AppState {
    return this.state;
  }

  async loadPlan(plan: FlightPlan): Promise<AppState> {
    this.state = await this.adapter.replaceFlightPlanState(this.state, this.catalog, plan);
    return this.state;
  }

  async setPolicy(policy: ContentPolicy): Promise<AppState> {
    this.state = await this.adapter.setContentPolicyState(this.state, this.catalog, policy);
    return this.state;
  }

  async refresh(inventory: ContentInventory): Promise<AppState> {
    this.state = await this.adapter.refreshContentState(this.state, this.catalog, inventory);
    return this.state;
  }
}
