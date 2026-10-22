// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { ReceiverActivityView } from "./App";
import type { UiReceiverActivity } from "./generated/sessionPageWire";
import themeJson from "@shared-ui-theme";
const theme = themeJson as {receiver_activity: Record<string, string>};

it("paints core geometry and replaces it on rewind without owning time or counts", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div");
  const root = createRoot(host);
  const state: UiReceiverActivity = {title: "Reception activity", detail: "Partial minute", axis_start: "60 minutes ago", axis_end: "Now",
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
