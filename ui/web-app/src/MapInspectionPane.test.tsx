// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { MapInspectionBackdrop, MapInspectionPane } from "./MapInspectionPane";

it("routes backdrop drag/pinch pointers to the map but isolates tray pointers and wheel scrolling", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const map = vi.fn(), command = vi.fn(), close = vi.fn(), action = vi.fn();
  const pointer = async (target: Element, type: string, id = 1) => {
    const event = new MouseEvent(type, { bubbles: true, clientX: 20, clientY: 30 });
    Object.defineProperty(event, "pointerId", { value: id });
    await act(async () => { target.dispatchEvent(event); });
  };
  try {
    await act(async () => root.render(<div onPointerDown={map} onPointerMove={map} onPointerUp={map} onWheel={map}>
      <MapInspectionBackdrop onDismiss={close} />
      <MapInspectionPane onCommand={command}>
        <button data-testid="action" onClick={action}>ACTION</button>
      </MapInspectionPane>
    </div>));
    const backdrop = host.querySelector('[aria-label="Close map selection"]')!;
    for (const id of [1, 2]) await pointer(backdrop, "pointerdown", id);
    for (const id of [1, 2]) await pointer(backdrop, "pointermove", id);
    for (const id of [1, 2]) await pointer(backdrop, "pointerup", id);
    expect(map).toHaveBeenCalledTimes(6);
    expect(close).not.toHaveBeenCalled(); // Dismissal belongs to the map/core, not pointer-down.
    map.mockClear();
    const button = host.querySelector<HTMLButtonElement>('[data-testid="action"]')!;
    await pointer(button, "pointerdown");
    await pointer(button, "pointerdown", 2);
    await pointer(button, "pointermove");
    await pointer(button, "pointerup");
    expect(command.mock.calls).toEqual([["touch_started"]]);
    await pointer(button, "pointercancel", 2);
    expect(command.mock.calls).toEqual([["touch_started"], ["touch_ended"]]);
    await act(async () => button.dispatchEvent(new WheelEvent("wheel", { bubbles: true })));
    expect(command).toHaveBeenLastCalledWith("activity");
    await act(async () => button.click());
    expect(action).toHaveBeenCalledOnce();
    expect(map).not.toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
