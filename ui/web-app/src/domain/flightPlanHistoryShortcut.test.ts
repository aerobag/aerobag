// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  flightPlanHistoryAriaKeyShortcuts,
  flightPlanHistoryControlForKey,
  type FlightPlanHistoryKeyInput,
} from "./flightPlanHistoryShortcut";

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");

function keyInput(overrides: Partial<FlightPlanHistoryKeyInput>): FlightPlanHistoryKeyInput {
  return {
    key: "",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    repeat: false,
    ...overrides,
  };
}

describe("flight-plan history keyboard shortcuts", () => {
  it("maps platform undo conventions", () => {
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z", ctrlKey: true }))).toBe("undo");
    expect(flightPlanHistoryControlForKey(keyInput({ key: "Z", metaKey: true }))).toBe("undo");
  });

  it("maps platform redo conventions", () => {
    expect(flightPlanHistoryControlForKey(keyInput({ key: "y", ctrlKey: true }))).toBe("redo");
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z", ctrlKey: true, shiftKey: true }))).toBe("redo");
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z", metaKey: true, shiftKey: true }))).toBe("redo");
  });

  it("does not claim unsupported or repeating chords", () => {
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z" }))).toBeNull();
    expect(flightPlanHistoryControlForKey(keyInput({ key: "y", metaKey: true }))).toBeNull();
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z", ctrlKey: true, altKey: true }))).toBeNull();
    expect(flightPlanHistoryControlForKey(keyInput({ key: "z", ctrlKey: true, repeat: true }))).toBeNull();
  });

  it("publishes the shortcuts on their matching controls", () => {
    expect(flightPlanHistoryAriaKeyShortcuts("undo")).toBe("Control+Z Meta+Z");
    expect(flightPlanHistoryAriaKeyShortcuts("redo")).toBe(
      "Control+Y Control+Shift+Z Meta+Shift+Z",
    );
    expect(flightPlanHistoryAriaKeyShortcuts("stop_navigation")).toBeUndefined();
  });

  it("scopes the window handler to the visible plan page and leaves editors alone", () => {
    const start = appSource.indexOf("function FlightPlanPage(");
    const end = appSource.indexOf("function ChartPlateToggleButton(", start);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const flightPlanPage = appSource.slice(start, end);
    expect(flightPlanPage).toContain('if (props.page !== "plan")');
    expect(flightPlanPage).toContain("event.defaultPrevented || isEditableTarget(event.target)");
    expect(flightPlanPage).toContain('window.addEventListener("keydown", handleHistoryKeyDown)');
  });
});
