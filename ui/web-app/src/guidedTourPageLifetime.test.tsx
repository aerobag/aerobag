// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PageLayer } from "./App";
import { GuidedTourContext, useGuidedTour } from "./GuidedTour";
import type { UiGuidedTour } from "./generated/sessionPageWire";

const scene: UiGuidedTour = {
  generation: 1, step_id: "aircraft-model", chapter: "Flight Planning", title: "Choose an airplane",
  body: "", placement: "top_right", presentation: "callout", restart_label: "Start over",
  page: "altitude_planner", surface: "aircraft_models", subject: "", row_uid: null, option_uid: null,
  targets: [], position: 29, total: 53, back_enabled: true, next_label: "Next", close_label: "Close tour",
  shortcuts: [], map_point: null, viewport: {lat: 47, lon: -122, zoom: 7, centered: false, track_up: false},
};

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// Deliberately knows no tour/page cleanup rules and uses a raw body portal:
// a future page or tray must inherit disposal from the production page owner.
function FuturePage({name, pending}: {name: string; pending?: Promise<void>}) {
  const tour = useGuidedTour();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(0);
  useEffect(() => {
    if (tour?.surface === "aircraft_models") {
      if (pending) void pending.then(() => setOpen(true));
      else setOpen(true);
    }
  }, [tour?.generation]);
  return <>
    <button data-testid={`${name}-toggle`} onClick={() => setOpen(value => !value)}>Menu</button>
    <button data-testid={`${name}-draft`} onClick={() => setDraft(value => value + 1)}>{draft}</button>
    {open ? createPortal(<aside data-testid={`${name}-menu`}>Future menu {draft}</aside>, document.body) : null}
  </>;
}

function render(tour: UiGuidedTour | null, page: string, pending?: Promise<void>) {
  return act(async () => root.render(<GuidedTourContext.Provider value={tour}>
    <PageLayer active={page === "future"}><FuturePage name="future" pending={pending} /></PageLayer>
    <PageLayer active={page === "another"}><FuturePage name="another" /></PageLayer>
    <PageLayer active={page === "home"}><div>Home</div></PageLayer>
  </GuidedTourContext.Provider>));
}
const menu = (name = "future") => document.querySelector(`[data-testid=${name}-menu]`);
const click = (id: string) => act(async () => host.querySelector<HTMLButtonElement>(`[data-testid=${id}]`)!.click());

it.each(["home", "future"])("disposes every demo portal when closing to %s, including retained hidden pages", async page => {
  await render(scene, "future");
  expect(menu()).not.toBeNull();
  expect(menu("another")).not.toBeNull();
  expect(host.contains(menu())).toBe(false);
  await render(null, page);
  expect(menu()).toBeNull();
  expect(menu("another")).toBeNull();
  for (const next of ["another", "home", "future"]) {
    await render(null, next);
    expect(menu()).toBeNull();
    expect(menu("another")).toBeNull();
  }
  await click("future-toggle");
  expect(menu()).not.toBeNull();
  await click("future-toggle");
  expect(menu()).toBeNull();
});

it("starts with fresh presentation state but preserves it between tour steps and normal page switches", async () => {
  await render(null, "future");
  await click("future-draft");
  await click("future-toggle");
  await render(null, "home");
  await render(null, "future");
  expect(menu()?.textContent).toBe("Future menu 1");
  await render(scene, "future");
  expect(menu()?.textContent).toBe("Future menu 0");
  await click("future-draft");
  await render({...scene, generation: 2}, "future");
  expect(menu()?.textContent).toBe("Future menu 1");
  await render(null, "home");
  await render(scene, "future");
  expect(menu()?.textContent).toBe("Future menu 0");
});

it("a pending demo menu cannot reopen after Close or leak into a resumed tour", async () => {
  let finish!: () => void;
  const pending = new Promise<void>(resolve => { finish = resolve; });
  await render(scene, "future", pending);
  expect(menu()).toBeNull();
  await render(null, "home");
  await act(async () => finish());
  expect(menu()).toBeNull();
  await render(null, "future");
  expect(menu()).toBeNull();
  await render(scene, "future");
  expect(menu()).not.toBeNull();
});
