// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { FlightDataBanner } from "./App";
import type { FlightDataBannerModel } from "./generated/sessionPageWire";

it("applies core formatting without reselecting text or overwriting newer input", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), command = vi.fn();
  const editor: NonNullable<FlightDataBannerModel["editor"]> = {
    id: "barometer", label: "Altimeter setting", unit: "inHg", input: "29.92", input_revision: 0,
    notice: "BARO ALT from device is cabin alt. Cross-check.", action_rows: [],
    dismiss_action_id: "close", close_label: "CLOSE",
  };
  const render = () => act(async () => root.render(<FlightDataBanner banner={{cells: [{id: "barometer", label: "BARO ALT"}], editor}} edge="right"
    onCellActivated={() => {}} onFlightDataCommand={command} onDisabledAction={() => {}} />));
  try {
    await render();
    const input = host.querySelector<HTMLInputElement>("input")!;
    for (const character of "3006") {
      await act(async () => {
        input.setRangeText(character, input.selectionStart!, input.selectionEnd!, "end");
        input.dispatchEvent(new Event("input", {bubbles: true}));
      });
    }
    expect(command.mock.calls.map(([c]) => c.input)).toEqual(["3", "30", "300", "3006"]);
    editor.input = "30.06";
    editor.input_correction = {source: "3006", start: 2, end: 2, text: "."};
    await render();
    expect(input.value).toBe("30.06");
    expect([input.selectionStart, input.selectionEnd]).toEqual([5, 5]);
    await act(async () => {
      input.setSelectionRange(4, 5);
      input.setRangeText("8", 4, 5, "end");
      input.dispatchEvent(new Event("input", {bubbles: true}));
    });
    editor.input_correction = {...editor.input_correction};
    await render();
    expect(input.value).toBe("30.08");
    await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", {key: "Enter", bubbles: true})));
    expect(command).toHaveBeenLastCalledWith({kind: "editor_action", editor_id: "barometer", action_id: "close"});
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});

it("shows station-only weather on NEAREST without an airport or extra description", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), command = vi.fn();
  const editor: NonNullable<FlightDataBannerModel["editor"]> = {
    id: "barometer", label: "Altimeter setting", unit: "inHg", input: "29.92", input_revision: 0,
    notice: "BARO ALT from device is cabin alt. Cross-check.", dismiss_action_id: "close", close_label: "CLOSE",
    action_rows: [[{id: "nearest", label: "NEAREST", secondary_label: "KSMP 52min old", enabled: true, selected: false,
      symbol_feature: null, weather_badge: {flight_category: "vfr", ceiling_amount: "clear"}}]],
  };
  try {
    await act(async () => root.render(<FlightDataBanner banner={{cells: [{id: "barometer", label: "BARO ALT"}], editor}} edge="right"
      onCellActivated={() => {}} onFlightDataCommand={command} onDisabledAction={() => {}} />));
    const button = host.querySelector<HTMLButtonElement>('[data-testid="barometer-nearest"]')!;
    expect(button.textContent).toBe("NEARESTKSMP 52min old");
    expect(button.querySelector('.planWaypointWeatherBadge')).not.toBeNull();
    expect(button.querySelector('.planWaypointWeatherBadge')!.getAttribute('transform')).toBeNull();
    expect(button.querySelector('.airportMarker')).toBeNull();
    await act(async () => button.click());
    expect(command).toHaveBeenCalledExactlyOnceWith({kind: "editor_action", editor_id: "barometer", action_id: "nearest"});
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});

it("focuses and selects the value on open and reopen without selecting each core echo", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), command = vi.fn();
  const editor = {id: "altitude_target", title: "Target altitude", label: "GPS target ft", input: "2500", input_revision: 0,
    unit: "ft", dismiss_action_id: "close", notice: "Use the aircraft altimeter.", close_label: "CLOSE", action_rows: []};
  const banner: FlightDataBannerModel = {cells: [{id: "altitude_target", label: "TGT GPS", value: "2500"}], editor};
  const render = () => act(async () => root.render(<FlightDataBanner banner={banner} edge="right" onCellActivated={() => {}}
    onFlightDataCommand={command} onDisabledAction={() => {}} />));
  try {
    await render();
    const input = host.querySelector<HTMLInputElement>("input")!;
    expect(document.activeElement).toBe(input);
    expect([input.selectionStart, input.selectionEnd]).toEqual([0, 4]);
    await act(async () => {
      input.setRangeText("2", input.selectionStart!, input.selectionEnd!, "end");
      input.dispatchEvent(new Event("input", {bubbles: true}));
    });
    expect(command).toHaveBeenLastCalledWith({kind: "set_input", editor_id: "altitude_target", input: "2"});
    editor.input = "2";
    await render();
    expect(document.activeElement).toBe(input);
    expect([input.selectionStart, input.selectionEnd]).toEqual([1, 1]);
    await act(async () => {
      input.setRangeText("6", input.selectionStart!, input.selectionEnd!, "end");
      input.dispatchEvent(new Event("input", {bubbles: true}));
    });
    expect(command).toHaveBeenLastCalledWith({kind: "set_input", editor_id: "altitude_target", input: "26"});
    editor.input = "26";
    banner.editor = null;
    await render();
    banner.editor = editor;
    await render();
    const reopened = host.querySelector<HTMLInputElement>("input")!;
    expect(document.activeElement).toBe(reopened);
    expect([reopened.selectionStart, reopened.selectionEnd]).toEqual([0, 2]);
    expect(command).toHaveBeenCalledTimes(2);
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});

it.each(["Enter", "outside", "help", "no close button"])("target editor uses standard tray interaction: %s", async (interaction) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), command = vi.fn(), help = vi.fn();
  const reason = "This device does not provide a barometric pressure sensor.";
  const banner: FlightDataBannerModel = {
    cells: [{id: "altitude_target", label: "TGT GPS", value: "2500"}],
    editor: {id: "altitude_target", title: "Target altitude", label: "GPS target ft", input: "2500", input_revision: 0,
      unit: "ft", dismiss_action_id: "close",
      notice: "Use the aircraft altimeter.", close_label: "CLOSE",
      action_rows: [[{id: "baro", label: "BARO", enabled: false, selected: false, disabled_reason: reason}]]},
  };
  try {
    await act(async () => root.render(<FlightDataBanner banner={banner} edge="right" onCellActivated={() => {}}
      onFlightDataCommand={command} onDisabledAction={help} />));
    if (interaction === "Enter") {
      await act(async () => {
        const input = host.querySelector<HTMLInputElement>("input")!; input.focus();
        input.dispatchEvent(new KeyboardEvent("keydown", {key: "Enter", bubbles: true, cancelable: true}));
      });
      expect(command).toHaveBeenCalledExactlyOnceWith({kind: "editor_action", editor_id: "altitude_target", action_id: "close"});
    } else if (interaction === "outside") {
      await act(async () => host.querySelector('[aria-label="CLOSE"]')!.dispatchEvent(new Event("pointerdown", {bubbles: true})));
      expect(command).toHaveBeenCalledExactlyOnceWith({kind: "editor_action", editor_id: "altitude_target", action_id: "close"});
    } else if (interaction === "help") {
      const button = host.querySelector<HTMLButtonElement>('[data-testid="altitude_target-baro"]')!;
      expect(button.title).toBe(reason);
      expect(button.getAttribute("aria-disabled")).toBe("true");
      expect(button.classList.contains("isDisabled")).toBe(true);
      await act(async () => button.click());
      expect(help).toHaveBeenCalledExactlyOnceWith(reason);
      expect(command).not.toHaveBeenCalled();
    } else {
      expect(host.querySelector('[data-testid="altitude_target-close"]')).toBeNull();
    }
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});

it("renders target reference and step actions from core and does not invent a blink timer", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), command = vi.fn(), mapGesture = vi.fn();
  const banner: FlightDataBannerModel = {
    cells: [{ id: "altitude_target", label: "TGT BARO", value: "1500",
      action: { action_id: "altitude_target", accessibility_label: "Set target altitude" },
      attention: { message: "Approaching target altitude.", highlighted: true } }],
    editor: { id: "altitude_target", title: "Target altitude", label: "BARO target ft", input: "1500", input_revision: 0,
      unit: "ft", dismiss_action_id: "close",
      notice: "Cross-check with aircraft altimeter.", close_label: "CLOSE", action_rows: [[
        { id: "baro", label: "BARO", enabled: true, selected: true },
        { id: "gps", label: "GPS", enabled: true, selected: false },
      ], [
        { id: "decrease", label: "-100", enabled: true, selected: false },
        { id: "increase", label: "+100", enabled: true, selected: false },
      ], [
        { id: "off", label: "OFF", enabled: true, selected: false },
      ]] },
  };
  const render = () => act(async () => root.render(
    <div onPointerDown={mapGesture} onPointerMove={mapGesture} onPointerUp={mapGesture} onPointerCancel={mapGesture}
      onClick={mapGesture} onDoubleClick={mapGesture} onWheel={mapGesture}>
      <FlightDataBanner banner={banner} edge="right" onCellActivated={() => {}} onFlightDataCommand={command} onDisabledAction={() => {}} />
    </div>));
  try {
    await render();
    expect(host.querySelector('[data-testid="altitude_target-baro"]')!.getAttribute("aria-pressed")).toBe("true");
    await act(async () => {
      const input = host.querySelector('[data-testid="altitude_target-setting"]')!;
      for (const type of ["pointerdown", "pointermove", "pointerup", "pointercancel", "click", "dblclick", "wheel"]) {
        input.dispatchEvent(new Event(type, {bubbles: true, cancelable: true}));
      }
    });
    expect(mapGesture).not.toHaveBeenCalled();
    for (const id of ["gps", "increase", "decrease", "off"]) {
      await act(async () => host.querySelector<HTMLButtonElement>(`[data-testid="altitude_target-${id}"]`)!.click());
      expect(command).toHaveBeenLastCalledWith({kind: "editor_action", editor_id: "altitude_target", action_id: id});
    }
    expect(host.querySelector(".flightDataCell.isAttention")).not.toBeNull();
    banner.cells[0].attention!.highlighted = false;
    await render();
    expect(host.querySelector(".flightDataCell.isAttention")).toBeNull();
    expect(host.querySelector(".flightDataValue")!.textContent).toBe("1500");
    banner.editor!.input = "1600"; banner.editor!.input_revision = 1;
    await render();
    expect(host.querySelector<HTMLInputElement>('[data-testid="altitude_target-setting"]')!.value).toBe("1600");
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});

it("renders core barometer state and sends unparsed setting input back to core", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), action = vi.fn(), command = vi.fn();
  const banner: FlightDataBannerModel = { cells: [{
    id: "barometer", label: "BARO ft", value: null,
    action: { action_id: "barometer", accessibility_label: "Set barometric altimeter" },
  }] };
  const render = async () => {
    await act(async () => root.render(<FlightDataBanner banner={banner} edge="right" onCellActivated={action} onFlightDataCommand={command} onDisabledAction={() => {}} />));
  };
  try {
    await render();
    expect(host.textContent).toContain("—");
    await act(async () => host.querySelector<HTMLElement>('[data-testid="flight-data-cell:barometer"]')!.click());
    expect(action).toHaveBeenCalledWith("barometer");
    expect(host.querySelector('[role="dialog"]')).toBeNull();
    banner.editor = { id: "barometer", title: null, label: "Altimeter setting", unit: "inHg", dismiss_action_id: "close", input: "29.92", input_revision: 0, error: null,
      notice: "BARO ALT from device is cabin alt. Cross-check.",
      action_rows: [[{ id: "nearest", label: "NEAREST", enabled: false, selected: false }]], close_label: "CLOSE" };
    await render();
    const input = host.querySelector<HTMLInputElement>('[data-testid="barometer-setting"]')!;
    expect(input.value).toBe("29.92");
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "29.");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(command).toHaveBeenLastCalledWith({ kind: "set_input", editor_id: "barometer", input: "29." });
    expect(input.value).toBe("29."); // A pending core response must not erase typed characters.
    banner.editor.error = "Enter an altimeter setting from 25.00 to 35.00 inHg.";
    await render();
    expect(host.querySelector('[role="alert"]')!.textContent).toBe(banner.editor.error);
    const nearestButton = host.querySelector<HTMLButtonElement>('[data-testid="barometer-nearest"]')!;
    expect(nearestButton.disabled).toBe(true);
    await act(async () => nearestButton.click());
    expect(command).not.toHaveBeenCalledWith({ kind: "editor_action", editor_id: "barometer", action_id: "nearest" });
    banner.editor.action_rows[0][0].enabled = true;
    banner.editor.action_rows[0][0].secondary_label = "KBFI 20min old";
    banner.editor.warning = "Check baro setting:\nKBFI 29.97 20 min ago";
    banner.cells[0].attention = { message: banner.editor.warning, highlighted: true };
    await render();
    await act(async () => nearestButton.click());
    expect(command).toHaveBeenLastCalledWith({ kind: "editor_action", editor_id: "barometer", action_id: "nearest" });
    expect(host.textContent).toContain("BARO ALT from device is cabin alt. Cross-check.");
    expect(host.querySelector('.flightDataCell.isAttention')).not.toBeNull();
    expect(host.querySelector('[role="status"]')!.textContent).toBe(banner.editor.warning);
    banner.editor.input = "29.97";
    banner.editor.input_revision = 1;
    banner.editor.error = null;
    await render();
    expect(input.value).toBe("29.97");
    expect(nearestButton.textContent).toBe("NEARESTKBFI 20min old");
    expect(nearestButton.textContent).not.toContain("29.97");
    expect(host.querySelector('[role="dialog"] strong')).toBeNull();
    expect(host.querySelector('.flightDataSettingInput')!.textContent).toBe("inHg");
    expect(nearestButton.closest('.flightDataSettingActions')!.nextElementSibling!.textContent).toBe(banner.editor.notice);
    await act(async () => host.querySelector('[aria-label="CLOSE"]')!.dispatchEvent(new Event("pointerdown", {bubbles: true})));
    expect(command).toHaveBeenLastCalledWith({ kind: "editor_action", editor_id: "barometer", action_id: "close" });
    banner.editor = null;
    await render();
    expect(host.querySelector('[role="dialog"]')).toBeNull();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
