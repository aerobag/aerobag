// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { flightPlanWaypointUsesFullWidthLabel } from "./domain/flightPlanLayout";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

describe("flight plan layout CSS", () => {
  it("keeps the table header sticky to the vertical scroll viewport", () => {
    const headerBlocks = [...styles.matchAll(/\.planHeader\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(headerBlocks).toContain("position: sticky");
    expect(headerBlocks).toContain("top: 0");

    const viewportBlocks = [...styles.matchAll(/\.planScrollViewport\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(viewportBlocks).toMatch(/overflow:\s*auto/);

    const surfaceBlocks = [...styles.matchAll(/\.planScrollSurface\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "");
    expect(surfaceBlocks.length).toBeGreaterThan(0);
    for (const block of surfaceBlocks) {
      expect(block).not.toMatch(/overflow(?:-[xy])?:\s*auto/);
      expect(block).not.toMatch(/overflow(?:-[xy])?:\s*scroll/);
    }
  });

  it("uses a top-aligned textarea for the route entry caret layer", () => {
    expect(appSource).toMatch(/<textarea\s+className="planEntryInput"[\s\S]*data-testid="plan-append-route-input"/);

    const inputBlocks = [...styles.matchAll(/\.planEntryInput\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(inputBlocks).toContain("display: block");
    expect(inputBlocks).toContain("height: var(--thumb)");
    expect(inputBlocks).toContain("resize: none");
    expect(inputBlocks).toContain("overflow: hidden");
  });

  it("gives symbol-free flight-plan labels the full waypoint cell", () => {
    expect(flightPlanWaypointUsesFullWidthLabel(false, false)).toBe(true);
    expect(flightPlanWaypointUsesFullWidthLabel(true, true)).toBe(true);
    expect(flightPlanWaypointUsesFullWidthLabel(false, true)).toBe(false);
  });

  it("moves altitude planning to a standalone core-driven page", () => {
    const plannerPage = appSource.slice(
      appSource.indexOf("function AltitudePlannerPage("),
      appSource.indexOf("function FlightPlanPage("),
    );
    expect(appSource).toContain('onClick={() => props.onSelectPage("altitude")}');
    expect(plannerPage).toContain("props.onQueryAltitudeComparisons()");
    expect(plannerPage).toContain("props.onPerformAltitudePlannerAction(actionUid)");
    expect(plannerPage).not.toMatch(/onPerformAltitudePlannerAction\(actionUid\)[\s\S]*?\.then\(reload\)/);
    expect(plannerPage).toContain("panel.columns.map");
    expect(plannerPage).toContain("row.cells.map");
    expect(plannerPage).toContain('className="altitudeComparisonRegion"');
    expect(plannerPage).toContain('className="altitudeComparisonLoading"');
    expect(plannerPage).not.toContain('{loading ? <p>Calculating…</p> : null}');
    expect(plannerPage).not.toMatch(/tailwind|headwind|average_wind|toFixed/);

    const comparisonRegion = [...styles.matchAll(/\.altitudeComparisonRegion\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(comparisonRegion).toContain("position: relative");
    const loadingOverlay = [...styles.matchAll(/\.altitudeComparisonLoading\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(loadingOverlay).toContain("position: absolute");
    expect(loadingOverlay).toContain("inset: 0");
  });
});
