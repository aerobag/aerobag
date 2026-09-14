// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DataStatusDock, PageLayer, ServiceNotificationsSection, useModalTrayGroup } from "./App";
import type { UiDataStatusState, UiServiceNotificationsState } from "./generated/sessionPageWire";
const styles = readFileSync("src/styles.css", "utf8");

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

function projection(): UiServiceNotificationsState {
  return {
    title: "Service Notifications", summary: "1 unread service notifications", source_status: [],
    expanded: true,
    enter_action: {label: "Open Status", action_id: "opaque-enter"},
    toggle_action: {label: "Collapse service notifications", action_id: "opaque-fold"},
    mark_all_read: {label: "Mark all read", action_id: "opaque-read-all"},
    items: [{id: "notice", title: "Distribution quality", body: "<script>plain text, never markup</script>",
      timing: "Published today", state_label: "Unread", severity: "warning", tone: "warning",
      unread: true, archived: false, expanded: false,
      open_action: {label: "Distribution quality", action_id: "opaque-read-revision"}, link: null}],
  };
}

it("renders the core projection and forwards actions without optimistic reading or folding", async () => {
  const state = projection(), action = vi.fn();
  const render = () => act(async () => root.render(<ServiceNotificationsSection state={state} onAction={action} />));
  await render();
  expect(action).toHaveBeenCalledExactlyOnceWith("opaque-enter");
  expect(host.textContent).not.toContain(state.items[0].body);
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="parity:service:notice:notice"]')!.click());
  expect(action).toHaveBeenLastCalledWith("opaque-read-revision");
  expect(host.querySelector("article")!.classList.contains("serviceNoticeTone-warning")).toBe(true);
  expect(host.textContent).not.toContain(state.items[0].body);

  state.items[0] = {...state.items[0], expanded: true, unread: false, state_label: "Read", tone: "history"};
  await render();
  expect(host.textContent).toContain(state.items[0].body);
  expect(host.querySelector("script")).toBeNull();
  expect(host.querySelector("article")!.classList.contains("serviceNoticeTone-history")).toBe(true);
  expect(host.querySelector("article")!.className).not.toContain("warning");
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="parity:service:mark-all-read"]')!.click());
  expect(action).toHaveBeenLastCalledWith("opaque-read-all");
  await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="parity:service:toggle"]')!.click());
  expect(action).toHaveBeenLastCalledWith("opaque-fold");
  expect(host.textContent).toContain(state.items[0].body);
  state.expanded = false;
  await render();
  expect(host.textContent).not.toContain(state.items[0].body);
  expect(action.mock.calls.filter(([id]) => id === "opaque-enter")).toHaveLength(1);
});

it("asks core for entry policy on page reentry and remount, not on incidental updates", async () => {
  const state = projection(), action = vi.fn();
  const render = (active: boolean) => act(async () => root.render(<PageLayer active={active}>
    <ServiceNotificationsSection state={state} onAction={action} />
  </PageLayer>));
  await render(false);
  expect(action).not.toHaveBeenCalled();
  await render(true);
  await render(true);
  expect(action).toHaveBeenCalledExactlyOnceWith("opaque-enter");
  await render(false);
  await render(true);
  expect(action.mock.calls).toEqual([["opaque-enter"], ["opaque-enter"]]);
  await act(async () => root.render(null));
  await render(true);
  expect(action.mock.calls).toEqual([["opaque-enter"], ["opaque-enter"], ["opaque-enter"]]);
});

it("uses the existing light gray theme background and no border or halo for history", () => {
  const sheet = new CSSStyleSheet();
  sheet.insertRule(styles.slice(styles.indexOf(".serviceNoticeTone-history {"),
    styles.indexOf("}", styles.indexOf(".serviceNoticeTone-history {")) + 1));
  const rule = sheet.cssRules[0] as CSSStyleRule;
  expect(rule.style.getPropertyValue("background")).toBe("var(--theme-data-status-quiet-bg)");
  expect(rule.style.getPropertyValue("border")).toBe("0");
  expect(rule.style.getPropertyValue("box-shadow")).toBe("none");
});

it("keeps mark-all-read compact and right-aligned without changing its action", async () => {
  const action = vi.fn();
  await act(async () => root.render(<ServiceNotificationsSection state={projection()} onAction={action} />));
  const button = host.querySelector<HTMLButtonElement>('[data-testid="parity:service:mark-all-read"]')!;
  expect(button.classList.contains("serviceNotificationsMarkAll")).toBe(true);
  const sheet = new CSSStyleSheet();
  const start = styles.indexOf(".serviceNotificationsMarkAll {");
  sheet.insertRule(styles.slice(start, styles.indexOf("}", start) + 1));
  const rule = sheet.cssRules[0] as CSSStyleRule;
  expect(rule.style.getPropertyValue("align-self")).toBe("flex-end");
  expect(rule.style.getPropertyValue("width")).toBe("min(100%, calc(var(--thumb) * 3.5))");
  expect(rule.style.getPropertyValue("min-width")).toBe("0");
  await act(async () => button.click());
  expect(action).toHaveBeenLastCalledWith("opaque-read-all");
});

it("closes body-portaled status trays when their mounted page becomes inactive", async () => {
  const status: UiDataStatusState = {launcher_severity: "caution", launcher_count: "1", boxes: [{
    id: "service:unread", label: "Service", severity: "caution", drives_caution: true, hushed: false,
    value: "1 unread", detail: "Read service notifications", actions: [],
  }]};
  function MapTray() {
    const tray = useModalTrayGroup(["status"]);
    return <DataStatusDock dataStatusState={status} open={tray.isOpen("status")}
      onToggle={() => tray.toggle("status")} onAction={() => {}} />;
  }
  const render = (active: boolean) => act(async () => root.render(<PageLayer active={active}><MapTray /></PageLayer>));
  const toggle = () => act(async () => host.querySelector<HTMLButtonElement>('[data-testid="data-status-launcher"]')!.click());
  const panel = () => document.querySelector('[data-testid="data-status-panel"]');
  await render(true);
  await toggle();
  expect(panel()).not.toBeNull();
  expect(host.contains(panel())).toBe(false);
  await render(false);
  expect(panel()).toBeNull();
  await render(true);
  expect(panel()).toBeNull();
  await toggle();
  expect(panel()).not.toBeNull();
  await toggle();
  expect(panel()).toBeNull();
});
