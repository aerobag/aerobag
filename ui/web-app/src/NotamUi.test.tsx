// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { NotamBadgedControl } from "./NotamUi";
import type { NotamBadgeUiView } from "./domain/types";

it("opens the shared reader without activating the row, follows updates, and closes on removal/navigation", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host), rowClick = vi.fn(), mapPointer = vi.fn();
  const badge: NotamBadgeUiView = {
    label: "N", count: 1, action_id: "subject_notams:airway:V23", accessibility_label: "V23: 1 NOTAM",
    detail: { title: "Airway V23 NOTAMs", advisory_text: "Check affected segments.", empty_text: "None", notams: [{ id: "one", label: "FDC 4/0462", text: "V23 MEA 5400 NORTHBOUND" }] },
  };
  const render = async (value: NotamBadgeUiView | null, active = true) => act(async () => root.render(
    <div onClick={rowClick} onPointerDown={mapPointer}>
      <NotamBadgedControl badge={value} active={active}><button>V23</button></NotamBadgedControl>
    </div>,
  ));
  const open = async () => {
    const button = host.querySelector<HTMLButtonElement>("[data-action-id]")!;
    await act(async () => {
      button.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      button.click();
    });
  };
  try {
    await render(badge); await open();
    expect(rowClick).not.toHaveBeenCalled(); expect(mapPointer).not.toHaveBeenCalled();
    expect(document.querySelector(".notamReaderOverlay")?.textContent).toContain("V23 MEA 5400 NORTHBOUND");
    await render({ ...badge, detail: { ...badge.detail, notams: [{ id: "one", label: "Updated", text: "MEA 6000" }] } });
    expect(document.querySelector(".notamReaderOverlay")?.textContent).toContain("MEA 6000");
    expect(document.querySelector(".notamReaderOverlay")?.textContent).not.toContain("5400");
    await render(null);
    expect(document.querySelector(".notamReaderOverlay")).toBeNull();
    expect(host.querySelector("[data-action-id]")).toBeNull();
    await render(badge); await open(); await render(badge, false); await render(badge);
    expect(document.querySelector(".notamReaderOverlay")).toBeNull();
    await open();
    await act(async () => document.querySelector<HTMLButtonElement>('[aria-label="Close NOTAMs"]')!.click());
    expect(document.querySelector(".notamReaderOverlay")).toBeNull();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
