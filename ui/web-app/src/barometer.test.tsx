// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { FlightDataBanner } from "./App";
import type { FlightDataBannerModel } from "./generated/sessionPageWire";

it("renders core barometer state and sends unparsed setting input back to core", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), action = vi.fn(), command = vi.fn();
  const banner: FlightDataBannerModel = { cells: [{
    id: "barometer", label: "BARO ft", value: null,
    action: { action_id: "barometer", accessibility_label: "Set barometric altimeter" },
  }] };
  const render = async () => {
    await act(async () => root.render(<FlightDataBanner banner={banner} edge="right" onCellActivated={action} onBarometerCommand={command} />));
  };
  try {
    await render();
    expect(host.textContent).toContain("—");
    await act(async () => host.querySelector<HTMLElement>('[data-testid="flight-data-cell:barometer"]')!.click());
    expect(action).toHaveBeenCalledWith("barometer");
    expect(host.querySelector('[role="dialog"]')).toBeNull();
    banner.barometer_editor = { title: "BARO", label: "Altimeter inHg", input: "29.92", input_revision: 0, error: null,
      nearest_label: "NEAREST", nearest_enabled: false, nearest_detail: null, close_label: "CLOSE" };
    await render();
    const input = host.querySelector<HTMLInputElement>('[data-testid="barometer-setting"]')!;
    expect(input.value).toBe("29.92");
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "29.");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(command).toHaveBeenLastCalledWith({ kind: "set_setting", input: "29." });
    expect(input.value).toBe("29."); // A pending core response must not erase typed characters.
    banner.barometer_editor.error = "Enter an altimeter setting from 25.00 to 35.00 inHg.";
    await render();
    expect(host.querySelector('[role="alert"]')!.textContent).toBe(banner.barometer_editor.error);
    const nearestButton = host.querySelector<HTMLButtonElement>('[data-testid="barometer-nearest"]')!;
    expect(nearestButton.disabled).toBe(true);
    await act(async () => nearestButton.click());
    expect(command).not.toHaveBeenCalledWith({ kind: "use_nearest" });
    banner.barometer_editor.nearest_enabled = true;
    banner.barometer_editor.nearest_detail = "KBFI: 29.97 inHg, 8.1nm, 20 min old";
    await render();
    await act(async () => nearestButton.click());
    expect(command).toHaveBeenLastCalledWith({ kind: "use_nearest" });
    banner.barometer_editor.input = "29.97";
    banner.barometer_editor.input_revision = 1;
    banner.barometer_editor.error = null;
    await render();
    expect(input.value).toBe("29.97");
    expect(host.textContent).toContain("KBFI: 29.97 inHg");
    await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="barometer-close"]')!.click());
    expect(command).toHaveBeenLastCalledWith({ kind: "close_editor" });
    banner.barometer_editor = null;
    await render();
    expect(host.querySelector('[role="dialog"]')).toBeNull();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
