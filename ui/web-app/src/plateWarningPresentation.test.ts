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

    expect(chartsPage).toContain("statusControls.controls.map((control)");
    expect(chartsPage).toContain("statusControlTrayId(control.id)");
    expect(appSource).toContain('case "procedure_geometry":');
    expect(appSource).toContain('return "procedureWarning";');
    expect(appSource).toContain('case "global":');
    expect(appSource).toContain('return "status";');
    expect(chartsPage.match(/onAction: onStatusAction/g)).toHaveLength(1);
    expect(appSource.match(/onStatusAction={performStatusAction}/g)).toHaveLength(2);
    expect(appSource).toContain("uiSession.performStatusAction(actionId)");
    expect(chartsPage).toContain("<DataStatusWarningFace");
    expect(chartsPage).toContain("count={chart.procedure_geometry_warning_count.toString()}");
    expect(dataStatusDock).toContain("<DataStatusWarningFace");
    expect(styles).toContain(".plateProcedureWarningMini");
    expect(styles).not.toContain(".plateProcedureWarning.isViewer");
  });

  it("renders procedure NOTAMs as an independent core-modeled badge and modal", () => {
    const chartsPage = sourceBetween("function ChartsPage(", "function HomePage(");

    expect(chartsPage).toContain('className="plateThumbShell"');
    expect(chartsPage).toContain('className="plateThumbStickerRow"');
    expect(chartsPage).toContain('placement="folder"');
    expect(chartsPage).toContain('placement="dock"');
    expect(chartsPage).toContain("!folderOpen && selectedChart?.procedure_notam_badge");
    expect(chartsPage).toContain("setProcedureNotamDetail(chart.procedure_notam_badge!.detail)");
    expect(chartsPage).toContain("selectedCollection?.unmatched_procedure_notam_badge");
    expect(chartsPage).toContain('className="plateFolderUnmatchedNotamBadge"');
    expect(chartsPage).toContain("<ProcedureNotamModal detail={procedureNotamDetail} />");
    expect(appSource).toContain("props.badge.accessibility_label");
    expect(appSource).toContain("data-action-id={props.badge.action_id}");
    expect(styles).toContain(".plateThumbStickerRow");
    expect(styles).toContain(".plateFolderUnmatchedNotamBadge");
    expect(styles).toContain(".plateProcedureNotamBadge-dock");
    expect(styles).toMatch(/\.plateProcedureNotamBadge\s*\{[^}]*border-radius:\s*0;/s);
    expect(styles).toContain("--theme-plate-notam-badge-bg");
    expect(styles).toContain("--theme-plate-notam-badge-stroke");
  });
});
