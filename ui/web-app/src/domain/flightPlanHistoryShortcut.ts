// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { FlightPlanControlId } from "./types";

export type FlightPlanHistoryControlId = Extract<FlightPlanControlId, "undo" | "redo">;

export type FlightPlanHistoryKeyInput = {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
};

export function flightPlanHistoryControlForKey(
  input: FlightPlanHistoryKeyInput,
): FlightPlanHistoryControlId | null {
  if (input.repeat || input.altKey || input.ctrlKey === input.metaKey) {
    return null;
  }
  const key = input.key.toLowerCase();
  if (key === "z") {
    return input.shiftKey ? "redo" : "undo";
  }
  if (key === "y" && input.ctrlKey && !input.shiftKey) {
    return "redo";
  }
  return null;
}

export function flightPlanHistoryAriaKeyShortcuts(
  controlId: FlightPlanControlId,
): string | undefined {
  switch (controlId) {
    case "undo":
      return "Control+Z Meta+Z";
    case "redo":
      return "Control+Y Control+Shift+Z Meta+Shift+Z";
    default:
      return undefined;
  }
}
