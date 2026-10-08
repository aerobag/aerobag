// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MapSelectionTray } from "./App";
import type { MapSelectionItem } from "./domain/appCoreAdapter";

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

it.each([true, false])("keeps inspector items unwrapped and puts the NOTAM reader in details (padded: %s)", async (padded) => {
  const items: MapSelectionItem[] = ["UBG", "KONAH", "AYURU", "SPOT"].map((label) => ({
    id: label, label, sublabel: "", description: null, distance: null,
    highlight: { kind: "spot", lat: 45, lon: -123 },
    actions: [{ id: "direct", label: "DIRECT", enabled: true, action_uid: "direct", display_only: false, placeholder: false },
      ...(padded ? Array.from({ length: 5 }, (_, i) => ({
        id: `placeholder-${i}`, label: "", enabled: false, placeholder: true, display_only: true,
      })) : [])],
  }));
  items[0].notam_badge = {
    label: "N", count: 1, action_id: "subject_notams:navaid:UBG", accessibility_label: "UBG: 1 NOTAM",
    detail: { title: "Navaid UBG NOTAMs", advisory_text: "Check official sources.", empty_text: "None",
      notams: [{ id: "one", label: "NAV", text: "UBG NAV VOR U/S" }] },
  };
  const select = vi.fn(), action = vi.fn(), reader = vi.fn();
  const render = async (selected = items[0]) => act(async () => root.render(
    <MapSelectionTray point={{ x: 0, y: 0 }} selectedItem={selected}
      result={{ click_lat: 45, click_lon: -123, categories: [{ id: "points", label: "Points", items }] }}
      onSelectItem={select} onSelectAction={action} onDisabledAction={vi.fn()} onNotamOpenChange={reader} />,
  ));
  await render();
  const row = host.querySelector(".mapSelectionRow")!;
  expect([...row.children].every(el => el.matches("button.mapSelectionItem"))).toBe(true);
  expect(row.querySelectorAll("button")).toHaveLength(items.length);
  const indicator = row.querySelector<HTMLElement>(".mapSelectionNotamIndicator")!;
  await act(async () => indicator.click());
  expect(select).toHaveBeenLastCalledWith(items[0]);
  expect(document.querySelector(".notamReaderOverlay")).toBeNull();
  const badgeButtons = host.querySelectorAll<HTMLButtonElement>(".plateProcedureNotamBadge-action");
  expect(badgeButtons).toHaveLength(1);
  expect(badgeButtons[0].closest(".mapSelectionActionGrid")).not.toBeNull();
  await act(async () => badgeButtons[0].click());
  expect(document.querySelector(".notamReaderOverlay")?.textContent).toContain("UBG NAV VOR U/S");
  expect(action).not.toHaveBeenCalled();
  expect(reader).toHaveBeenLastCalledWith(true);
  await render(items[1]);
  expect(document.querySelector(".notamReaderOverlay")).toBeNull();
  expect(reader).toHaveBeenLastCalledWith(false);
});
