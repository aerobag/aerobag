// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AltitudePlannerPage, NavigationPageOptionsContext } from "./App";
import type { FlightPlanUiState } from "./domain/types";

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

function plan(): FlightPlanUiState {
  return {
    plan_id: "test", plan_version: 1, display_rows: [], data_columns: [], controls: [],
    airway_picker: null, airway_routing: null, guidance: null,
    altitude_planner: {
      title: "Altitude Planner", estimate_kind: "modeled",
      estimate_summary: {label: "Model", estimate_kind: "modeled"}, controls: [],
      departure: {
        title: "Depart:", time_label: "", time_value: "0400", basis_label: "Z",
        time_display_action_id: "opaque-basis", when_label: "=", when_value: "-8h",
        when_suffix: "from now", when_is_past: true, enabled: true,
        now_label: "NOW", now_action_uid: "opaque-reset-departure",
      },
    },
  };
}

async function render(planUiState: FlightPlanUiState, action = vi.fn(), input = vi.fn(), basis = vi.fn()) {
  await act(async () => root.render(
    <NavigationPageOptionsContext.Provider value={{
      options: [], maxHistoryDepth: 2, chartOrPlateReturnPages: new Set(["map", "charts"]), defaultChartOrPlateReturnPage: "map",
    }}>
      <AltitudePlannerPage page="altitude" planUiState={planUiState} mostRecentChartOrPlatePage="map"
        onOpenRecentChartOrPlate={() => {}} onSelectPage={() => {}}
        onQueryAltitudeComparisons={async () => ({columns: [], rows: []})}
        onPerformAltitudePlannerAction={action} onSetDepartureInput={input} onToggleDepartureTimeBasis={basis} />
    </NavigationPageOptionsContext.Provider>,
  ));
}

function field(name: string) {
  return host.querySelector<HTMLInputElement>(`[data-testid="altitude-planner-departure-${name}"]`)!;
}

it("dispatches NOW through the core action and renders the returned time and offset", async () => {
  const state = plan(), action = vi.fn(), input = vi.fn();
  await render(state, action, input);
  const when = field("when");
  await act(async () => {
    when.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(when, "unfinished");
    when.dispatchEvent(new Event("input", {bubbles: true}));
  });
  expect(when.value).toBe("unfinished");
  await act(async () => {
    const now = field("now");
    const down = new Event("pointerdown", {bubbles: true, cancelable: true});
    now.dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
    now.click();
  });
  expect(action).toHaveBeenCalledExactlyOnceWith("opaque-reset-departure");
  expect(input).not.toHaveBeenCalled();
  state.altitude_planner.departure = {...state.altitude_planner.departure,
    time_value: "1200", when_value: "now", when_is_past: false};
  await render(state, action, input);
  expect(field("time").value).toBe("1200");
  expect(field("when").value).toBe("now");
  expect(field("when").classList.contains("isWarning")).toBe(false);
});

it("explains locked fields and NOW during navigation, while time-zone display remains usable", async () => {
  const state = plan(), action = vi.fn(), input = vi.fn(), basis = vi.fn();
  const reason = "Core says: active navigation uses now; stop navigation to edit.";
  state.altitude_planner.departure = {...state.altitude_planner.departure,
    time_value: "1200", when_value: "NOW", when_suffix: "", when_is_past: false,
    enabled: false, disabled_reason: reason};
  await render(state, action, input, basis);
  for (const name of ["time", "when", "now"]) {
    expect(field(name).getAttribute("aria-disabled")).toBe("true");
    await act(async () => field(name).click());
    expect(host.querySelector('[data-testid="disabled-action-toast"]')!.textContent).toBe(reason);
  }
  expect(field("time").readOnly).toBe(true);
  expect(field("when").readOnly).toBe(true);
  expect(action).not.toHaveBeenCalled();
  expect(input).not.toHaveBeenCalled();
  await act(async () => field("basis").click());
  expect(basis).toHaveBeenCalledExactlyOnceWith();
});

it("replaces a focused draft when core locks the editor after leg activation", async () => {
  const state = plan(), input = vi.fn();
  await render(state, vi.fn(), input);
  await act(async () => {
    const when = field("when"); when.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(when, "3h");
    when.dispatchEvent(new Event("input", {bubbles: true}));
  });
  state.altitude_planner.departure = {...state.altitude_planner.departure,
    time_value: "1200", when_value: "NOW", when_is_past: false, enabled: false};
  await render(state, vi.fn(), input);
  expect(field("when").value).toBe("NOW");
  await act(async () => field("when").blur());
  expect(input).not.toHaveBeenCalled();
});
