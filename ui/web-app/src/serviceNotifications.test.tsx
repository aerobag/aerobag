// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { NavigationPageOptionsContext, ServiceNotificationsPage } from "./App";
import type { UiServiceNotificationsState } from "./generated/sessionPageWire";

it("renders only core-provided content and forwards the exact displayed read actions", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), action = vi.fn();
  const state: UiServiceNotificationsState = {
    title: "Service Notifications", summary: "1 unread service notifications", source_status: [],
    mark_all_read: { label: "Mark all read", action_id: "service:read-all:displayed-revision" },
    items: [{ id:"notice", title:"Distribution quality", body:"<script>plain text, never markup</script>",
      timing:"Published today", state_label:"Unread", severity:"caution", unread:true, archived:false,
      expanded:false, open_action:{label:"Distribution quality",action_id:"service:read:displayed-revision"}, link:null }],
  };
  async function render() {
    await act(async () => root.render(<NavigationPageOptionsContext.Provider value={{
      options:[{id:"home",label:"Home",launcherLabel:"HOME"},{id:"map",label:"Chart",launcherLabel:"CHART"}],
      maxHistoryDepth:2, chartOrPlateReturnPages:new Set(["map","charts"]), defaultChartOrPlateReturnPage:"map",
    }}><ServiceNotificationsPage page="notices" state={state} navElement={null} mostRecentChartOrPlatePage="map"
      onOpenPlan={() => {}} onOpenRecentChartOrPlate={() => {}} onSelectPage={() => {}} onAction={action} />
    </NavigationPageOptionsContext.Provider>));
  }
  try {
    await render();
    expect(host.textContent).not.toContain(state.items[0].body);
    const title = host.querySelector<HTMLButtonElement>('[data-testid="parity:service:notice:notice"]')!;
    await act(async () => title.click());
    expect(action).toHaveBeenLastCalledWith("service:read:displayed-revision");
    expect(title.getAttribute("aria-expanded")).toBe("false");
    // No optimistic platform receipt: wait for the authoritative core projection.
    state.items[0] = {...state.items[0],expanded:true,unread:false,state_label:"Read"};
    await render();
    expect(host.textContent).toContain(state.items[0].body);
    expect(host.querySelector("script")).toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="parity:service:mark-all-read"]')!.click());
    expect(action).toHaveBeenLastCalledWith("service:read-all:displayed-revision");
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
