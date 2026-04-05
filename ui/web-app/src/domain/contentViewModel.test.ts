import { describe, expect, it } from "vitest";
import { MockAppCoreAdapter } from "./appCoreAdapter";
import { ContentViewModel } from "./contentViewModel";
import { installedInventory, remoteOnlyInventory, samplePlan } from "./sampleData";

describe("ContentViewModel", () => {
  it("treats remote-only availability as satisfied for web streaming mode", async () => {
    const model = new ContentViewModel(new MockAppCoreAdapter());
    await model.loadPlan(samplePlan);
    await model.setPolicy("StreamAllowed");
    const state = await model.refresh(remoteOnlyInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.availability).toBe("RemoteOnly");
  });

  it("treats installed content as offline-usable for local-first mode", async () => {
    const model = new ContentViewModel(new MockAppCoreAdapter());
    await model.loadPlan(samplePlan);
    await model.setPolicy("OfflineRequired");
    const state = await model.refresh(installedInventory);

    expect(state.last_content_report?.fully_satisfied).toBe(true);
    expect(state.last_content_report?.items[0]?.availability.offline_usable).toBe(true);
  });
});
