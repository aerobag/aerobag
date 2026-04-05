import type { AppState, ContentInventory, ContentPolicy, FlightPlan } from "./types";
import type { AppCoreAdapter } from "./appCoreAdapter";
import { emptyState, sampleCatalog } from "./sampleData";

export class ContentViewModel {
  constructor(
    private readonly adapter: AppCoreAdapter,
    private state: AppState = emptyState,
  ) {}

  get snapshot(): AppState {
    return this.state;
  }

  async loadPlan(plan: FlightPlan): Promise<AppState> {
    this.state = await this.adapter.replaceFlightPlanState(this.state, sampleCatalog, plan);
    return this.state;
  }

  async setPolicy(policy: ContentPolicy): Promise<AppState> {
    this.state = await this.adapter.setContentPolicyState(this.state, sampleCatalog, policy);
    return this.state;
  }

  async refresh(inventory: ContentInventory): Promise<AppState> {
    this.state = await this.adapter.refreshContentState(this.state, sampleCatalog, inventory);
    return this.state;
  }
}
