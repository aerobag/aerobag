// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { NavigationPageOptionsContext, SettingsPage } from "./App";
import type { UiSettingsPageState, UiSettingsPageSection } from "./generated/sessionPageWire";

let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
});

it("renders the supplied block order, waits for core expansion, and updates cloud help", async () => {
  const action = vi.fn(), aircraftAction = vi.fn();
  const section: UiSettingsPageSection = {
    id: "diagnostics", title: "Diagnostics", expanded: false,
    toggle_action: { action_id: "opaque-expand", value_id: "next" },
    rows: [{ kind: "toggle", id: "debug", title: "Debug flag", action_id: "opaque-debug",
      value_id: "off", indent_level: 0, stops: [], items: [] }],
  };
  const library = {
    title: "Aircraft library", summary: "Your aircraft", column_min_width_thumbs: 4.2, column_gap_thumbs: 0.1,
    entries: [], add_action: { action_id: "opaque-add", label: "Add aircraft", enabled: true },
    sync_indicator: { symbol: "cloud", tone: "caution" as const, help_text: "Cloud Account Sync not working; see Status page." },
  };
  const state: UiSettingsPageState = { title: "Settings", summary: "", blocks: [
    { kind: "controls", rows: [] },
    { kind: "section", section },
    { kind: "aircraft_library", library },
  ] };
  const render = () => act(async () => root.render(<NavigationPageOptionsContext.Provider value={{
    options: [], maxHistoryDepth: 2, chartOrPlateReturnPages: new Set(["map", "charts"]), defaultChartOrPlateReturnPage: "map",
  }}><SettingsPage page="settings" state={state}
    navElement={null} mostRecentChartOrPlatePage="map" onOpenPlan={() => {}}
    onOpenRecentChartOrPlate={() => {}} onSelectPage={() => {}}
    onSettingsAction={action} onAircraftLibraryAction={aircraftAction} /></NavigationPageOptionsContext.Provider>));
  await render();
  expect(Array.from(host.querySelector(".settingsPageRows")!.children).map(el => el.className))
    .toEqual(["settingsPageSection", "settingsAircraftLibrary"]);
  const toggle = () => host.querySelector<HTMLButtonElement>('[data-testid="settings-section-diagnostics"]')!;
  await act(async () => toggle().click());
  expect(action).toHaveBeenLastCalledWith("opaque-expand", "next");
  expect(toggle().getAttribute("aria-expanded")).toBe("false");
  section.expanded = true;
  await render();
  expect(toggle().getAttribute("aria-expanded")).toBe("true");
  const indicator = () => host.querySelector<HTMLButtonElement>('[data-testid="settings-sync-aircraft-library"]')!;
  expect(indicator().dataset.tone).toBe("caution");
  await act(async () => indicator().click());
  expect(host.querySelector('[role="status"]')!.textContent).toBe(library.sync_indicator.help_text);
  state.blocks[2] = { kind: "aircraft_library", library: { ...library,
    sync_indicator: { symbol: "cloud", tone: "ok", help_text: "Synchronized through your Sync Account." },
  } };
  await render();
  expect(indicator().dataset.tone).toBe("ok");
  expect(indicator().title).toBe("Synchronized through your Sync Account.");
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="settings-aircraft-add"]')!.click());
  expect(aircraftAction).toHaveBeenLastCalledWith("opaque-add", undefined);
});
