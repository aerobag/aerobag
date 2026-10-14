// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FlightPlanWorkspaceBand, FlightPlanSlotLabel } from "./App";
import { FlightPlanWorkspaceLayout } from "./FlightPlanWorkspaceLayout";
import type { FlightPlanControlId, FlightPlanControlUiView, FlightPlanWorkspaceUi } from "./domain/types";

let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const control = (id: FlightPlanControlId, symbol_id: string): FlightPlanControlUiView => ({ id, symbol_id, label: id, enabled: true, selected: false });
const workspace = (): FlightPlanWorkspaceUi => ({
  open_slot: "active", active_label: "ACTIVE PLAN", standby_label: "STANDBY PLAN",
  active_summary: "KRNT SEA KUAO", standby_summary: "KPAE KPLU",
  standby_plan: null, entry_focus_revision: 0, notice: null,
  controls: [control("toggle_open_plan", "open_plan"), control("swap_plans", "swap_plans"), control("copy_active_to_standby", "copy_plan")],
  clear_standby: control("clear_standby", "remove"),
});

it("renders both collapsed slots from core, sending distinct Open, Swap, Copy and Clear commands", async () => {
  const state = workspace(), action = vi.fn(), disabled = vi.fn();
  const render = () => act(async () => root.render(<FlightPlanWorkspaceBand workspace={state} onControl={action} onDisabled={disabled} />));
  await render();
  expect(host.textContent).toContain("STANDBY PLAN");
  expect(host.querySelector<HTMLInputElement>('[data-testid="plan-collapsed-summary"]')!.value).toBe("KPAE KPLU");
  for (const id of ["toggle_open_plan", "swap_plans", "copy_active_to_standby"]) {
    const button = host.querySelector<HTMLButtonElement>(`[data-testid="plan-control-${id}"]`)!;
    expect(button.querySelector("svg")).not.toBeNull();
    await act(async () => button.click());
    expect(action).toHaveBeenLastCalledWith(id);
  }
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="plan-collapsed-open"]')!.click());
  expect(action).toHaveBeenLastCalledWith("open_standby_plan");
  expect(state.open_slot).toBe("active");
  state.open_slot = "standby";
  await render();
  expect(host.textContent).toContain("ACTIVE PLAN");
  expect(host.querySelector<HTMLInputElement>('[data-testid="plan-collapsed-summary"]')!.value).toBe("KRNT SEA KUAO");
  expect(host.querySelector('[data-testid="plan-control-clear_standby"]')).toBeNull();
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="plan-collapsed-open"]')!.click());
  expect(action).toHaveBeenLastCalledWith("open_active_plan");
});

it("explains core-disabled swap without dispatching or changing either plan", async () => {
  const state = workspace(), action = vi.fn(), disabled = vi.fn();
  state.controls[1] = { ...state.controls[1], enabled: false, disabled_reason: "Enter a Standby route before swapping flight plans." };
  await act(async () => root.render(<FlightPlanWorkspaceBand workspace={state} onControl={action} onDisabled={disabled} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="plan-control-swap_plans"]')!.click());
  expect(action).not.toHaveBeenCalled();
  expect(disabled).toHaveBeenCalledWith(state.controls[1].disabled_reason);
});

it("allows selecting the collapsed route without opening that plan, and keeps Clear out of the divider", async () => {
  const action = vi.fn();
  await act(async () => root.render(<FlightPlanWorkspaceBand workspace={workspace()} onControl={action} onDisabled={vi.fn()} />));
  const summary = host.querySelector<HTMLElement>('[data-testid="plan-collapsed-summary"]')!;
  await act(async () => summary.click());
  expect(action).not.toHaveBeenCalled();
  expect(host.querySelector('[data-testid="plan-control-clear_standby"]')).toBeNull();
  const input = summary as HTMLInputElement;
  await act(async () => input.focus());
  expect(input.readOnly).toBe(true);
  expect(input.selectionEnd! - input.selectionStart!).toBe(input.value.length);
});

it("uses the same core label and theme class for page headings and planner subtitles", async () => {
  for (const slot of ["active", "standby"] as const) {
    await act(async () => root.render(<FlightPlanSlotLabel workspace={workspace()} slot={slot} testId="altitude-plan-heading" />));
    expect(host.textContent).toBe(`${slot.toUpperCase()} PLAN`);
    expect(host.firstElementChild?.classList.contains(`is${slot}`)).toBe(true);
  }
});

it("expands the live grid and clips the non-interactive closing grid with the divider", async () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  const reduce = { matches: false };
  vi.stubGlobal("matchMedia", () => reduce);
  Object.defineProperty(document, "timeline", { configurable: true, value: { currentTime: 0 } });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function(this: HTMLElement) {
    return this.classList.contains("planWorkspaceLayout") ? 800 : this.classList.contains("planWorkspaceBandPosition") ? 76 : 716;
  });
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(76);
  const animations: Array<{ element: HTMLElement; frames: Keyframe[]; cancel: ReturnType<typeof vi.fn>; resolve: () => void }> = [];
  const animate = function(this: HTMLElement, frames: Keyframe[]) {
    let resolve!: () => void;
    const finished = new Promise<void>(done => { resolve = done; });
    const cancel = vi.fn();
    animations.push({ element: this, frames, cancel, resolve });
    return { cancel, finished } as unknown as Animation;
  };
  Object.defineProperty(HTMLElement.prototype, "animate", { configurable: true, value: animate });
  const state = workspace();
  const render = () => act(async () => root.render(<FlightPlanWorkspaceLayout slot={state.open_slot}
    band={<FlightPlanWorkspaceBand workspace={state} onControl={vi.fn()} onDisabled={vi.fn()} />}>
    <div data-testid="test-grid">{state.open_slot}</div>
  </FlightPlanWorkspaceLayout>));
  await render();
  expect(animations).toHaveLength(0);
  state.open_slot = "standby";
  await render();
  expect(animations).toHaveLength(4);
  expect(animations[0].frames.map(frame => frame.height)).toEqual(["76px", "716px"]);
  expect(animations[1].frames.filter(frame => frame.height).map(frame => frame.height)).toEqual(["716px", "76px"]);
  expect(animations[3].frames.map(frame => frame.top)).toEqual(["716px", "76px"]);
  const closing = host.querySelector<HTMLElement>(".planWorkspaceClosingEditor")!;
  expect(closing.textContent).toBe("active");
  expect(closing.inert).toBe(true);
  expect(closing.getAttribute("aria-hidden")).toBe("true");
  expect(closing.querySelector("[data-testid]")).toBeNull();
  expect(host.querySelector('[data-testid="test-grid"]')?.textContent).toBe("standby");
  await act(async () => animations.forEach(animation => animation.resolve()));
  expect(host.querySelector(".planWorkspaceClosingEditor")).toBeNull();
  expect(host.querySelector<HTMLElement>('[data-testid="plan-workspace-layout"]')?.dataset.transition).toBe("idle");
  reduce.matches = true;
  state.open_slot = "active";
  await render();
  expect(animations).toHaveLength(4);
  expect(host.querySelector(".planWorkspaceClosingEditor")).toBeNull();
  delete (HTMLElement.prototype as Partial<HTMLElement>).animate;
});
