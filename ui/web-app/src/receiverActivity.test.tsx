// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { ReceiverActivityView, ReceiverPanelView } from "./App";
import type { UiReceiverPanel, UiReceiverAction } from "./generated/sessionPageWire";
import type { UiReceiverActivity } from "./generated/sessionPageWire";
import themeJson from "@shared-ui-theme";
const theme = themeJson as {receiver_activity: Record<string, string>};

it("renders the typed chooser and forwards opaque actions without inventing device policy", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div");
  const root = createRoot(host);
  const click = vi.fn();
  const action = (id: string, label: string): UiReceiverAction => ({ action_id: id, label, enabled: true, disabled_reason: null, symbol: null });
  const state: UiReceiverPanel = { title: "ADS-B receiver", status: "Disconnected", detail: "Connect or pair a new Bluetooth transponder.", activity: null, actions: [],
    chooser: { devices: [action("opaque-device", "GTX345  AA:BB")], empty_text: null,
      show_all: action("opaque-filter", "Show all paired devices"), show_all_checked: false,
      refresh: {...action("opaque-refresh", "Refresh paired devices"), symbol: "\u21bb"}, pair: action("opaque-pair", "Pair a new device") },
    diagnostics: { title: "Diagnostics", detail: "Counters", actions: [] } };
  try {
    await act(async () => root.render(<ReceiverPanelView state={state} onAction={click} onDisabledAction={vi.fn()} />));
    expect(host.querySelector("header")?.textContent).toBe("ADS-B receiverDisconnected");
    expect(host.querySelector(".receiverDeviceList")?.textContent).toBe("GTX345  AA:BB");
    expect(host.querySelector("details")?.open).toBe(false);
    expect(host.querySelector<HTMLInputElement>("input")?.checked).toBe(false);
    for (const id of ["opaque-device", "opaque-refresh", "opaque-pair"]) {
      await act(async () => host.querySelector<HTMLButtonElement>(`[data-testid="receiver:${id}"]`)!.click());
    }
    await act(async () => host.querySelector<HTMLInputElement>("input")!.click());
    expect(click.mock.calls.map(([id]) => id)).toEqual(["opaque-device", "opaque-refresh", "opaque-pair", "opaque-filter"]);
  } finally { await act(async () => root.unmount()); vi.unstubAllGlobals(); }
});

it("paints core geometry and replaces it on rewind without owning time or counts", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div");
  const root = createRoot(host);
  const state: UiReceiverActivity = {title: "Reception activity", axis_start: "60 minutes ago", axis_end: "Now",
    strips: [{title: "Weather", scale_label: "0-4 messages/min", legend: [{color_key: "metar", text: "METAR: last received 3s ago"}],
      rects: [{x: 59100, y: 500, width: 800, height: 500, color_key: "metar"}]}]};
  try {
    await act(async () => root.render(<ReceiverActivityView state={state} />));
    const rect = host.querySelector("rect")!;
    expect([rect.getAttribute("x"), rect.getAttribute("height"), rect.getAttribute("fill")]).toEqual(["59100", "500", theme.receiver_activity.metar]);
    expect(host.textContent).toContain("METAR: last received 3s ago");
    state.strips[0] = {...state.strips[0], rects: [], legend: [{color_key: "metar", text: "METAR: not received"}]};
    await act(async () => root.render(<ReceiverActivityView state={state} />));
    expect(host.querySelector("rect")).toBeNull();
    expect(host.textContent).toContain("METAR: not received");
    expect(host.textContent).not.toContain("3s ago");
  } finally { await act(async () => root.unmount()); vi.unstubAllGlobals(); }
});
