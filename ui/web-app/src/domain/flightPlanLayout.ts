// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export function flightPlanWaypointUsesFullWidthLabel(
  procedureGroupCell: boolean,
  hasWaypointSymbol: boolean,
): boolean {
  return procedureGroupCell || !hasWaypointSymbol;
}
