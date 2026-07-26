// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export type WorkerCreateUiSessionRequest = {
  recentAirportIds: string[];
  selectedAirportId?: string;
  selectedChartId?: string;
  settingsJson: string | null;
};
