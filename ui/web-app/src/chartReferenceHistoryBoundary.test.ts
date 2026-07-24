// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const styleSource = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

function styleBetween(start: string, end: string): string {
  const startIndex = styleSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = styleSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return styleSource.slice(startIndex, endIndex);
}

describe("chart-reference view history boundary", () => {
  it("records a reference chart click without replaying stale core state", () => {
    const handler = sourceBetween(
      "onSelectChart={(chartId) => {",
      "ownship={appUiState.ownship.render}",
    );

    expect(handler).toContain("uiSession.selectChart(chartId)");
    expect(handler).toContain("selectedReferenceFamilyId,");
    expect(handler).toContain("suggestedChartIds: derivedChartPageState.suggested_chart_ids");
    expect(handler).not.toContain("restoreChartPageState");
  });

  it("keeps ordinary history pushes local unless restoring an older view", () => {
    const helper = sourceBetween(
      "function pushViewSnapshot(",
      "function navigateToMostRecentChartOrPlate()",
    );

    expect(helper).toContain("restoreCore = false");
    expect(helper).toContain("applySnapshotLocally(nextCurrent, nextHistory)");
    expect(helper).toContain("restoreSnapshot(nextCurrent, nextHistory)");
  });

  it("puts the reference action on its chart-family tray row", () => {
    const familyDock = sourceBetween(
      'testId="chart-family-button"',
      'launcherLabel="LAYERS"',
    );

    expect(familyDock).toContain("accessory: chartReferenceAction?.family_id === family.id");
    expect(familyDock).toContain("iconSrc: CHART_REFERENCE_ICON_SRC");
    expect(familyDock).not.toContain('label: "REF"');
    expect(familyDock).toContain("dismissTrayOnSelect: true");
    expect(familyDock).not.toContain('trayGroup.close("family")');
    expect(familyDock).not.toContain("accessoryLabel=");

    const trayDock = sourceBetween("function TrayDock(", "function ChartSearchBox(");
    expect(trayDock).toContain("option.accessory?.onSelect()");
    expect(trayDock).toContain("if (option.dismissTrayOnSelect)");
    expect(trayDock).toContain("onToggle()");

    const accessoryStyle = styleBetween(".trayButtonAccessory {", ".trayButtonAccessoryIcon {");
    expect(accessoryStyle).toContain("width: var(--thumb)");
    expect(accessoryStyle).toContain("height: var(--thumb)");
  });
});
