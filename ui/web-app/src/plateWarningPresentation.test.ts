// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

describe("plate procedure geometry warnings", () => {
  it("uses the standard status tray on the viewer and its compact face in the folder", () => {
    const chartsPage = sourceBetween("function ChartsPage(", "function HomePage(");
    const dataStatusDock = sourceBetween("function DataStatusDock(", "function SituationTransportRow(");

    expect(chartsPage).toContain("dataStatusState={procedureGeometryStatus}");
    expect(chartsPage).toContain("statusOpen={trayGroup.isOpen(\"procedureWarning\")}");
    expect(chartsPage).toContain("<DataStatusWarningFace");
    expect(chartsPage).toContain("count={chart.procedure_geometry_warning_count.toString()}");
    expect(dataStatusDock).toContain("<DataStatusWarningFace");
    expect(styles).toContain(".plateProcedureWarningMini");
    expect(styles).not.toContain(".plateProcedureWarning.isViewer");
  });
});
