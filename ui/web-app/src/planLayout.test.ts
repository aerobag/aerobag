// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { flightPlanWaypointUsesFullWidthLabel } from "./domain/flightPlanLayout";
import { actionSymbol } from "./generated/navSymbols";

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

  it("renders core-selected flight-plan controls with the checked theme", () => {
    expect(appSource).toContain('control.selected ? " selectedControlHighlight" : ""');
    expect(appSource).toContain("aria-pressed={control.selected}");

    const selectedControlStyles = [...styles.matchAll(/\.selectedControlHighlight\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(selectedControlStyles).toContain("var(--theme-button-checked)");
    expect(selectedControlStyles).toContain("var(--theme-button-fg)");
  });

  it("renders every flight-plan control through its core-projected shared vector symbol", () => {
    expect(appSource).toContain("actionSymbol(control.symbol_id)");
    expect(appSource).toContain("<ActionIcon layers={symbol} />");

    const controlSymbols = [
      "undo",
      "redo",
      "activate_next_leg",
      "stop_navigation",
      "toggle_sequencing_suspension",
      "restore_direct_to",
    ];
    for (const symbolId of controlSymbols) {
      expect(actionSymbol(symbolId)).toBeTruthy();
    }

    const activate = actionSymbol("activate_leg")!;
    const stop = actionSymbol("stop_navigation")!;
    expect(stop.slice(0, activate.length)).toEqual(activate);
    expect(stop.at(-1)).toMatchObject({ stroke: "white" });

    const directTo = actionSymbol("direct_to")!;
    const restore = actionSymbol("restore_direct_to")!;
    expect(restore.slice(0, directTo.length)).toEqual(directTo);
    expect(restore.at(-1)).toMatchObject({ stroke: "white" });
  });

  it("overlays core-projected weather on the existing waypoint symbol cell", () => {
    expect(appSource).toContain("weatherBadge: row.weather_badge ?? null");
    expect(appSource).toContain("weatherBadge={row.weatherBadge}");
    expect(appSource).toContain('<g className="planWaypointWeatherBadge" transform="translate(10 10) scale(1)">');

    const symbolBlocks = [...styles.matchAll(/\.planWaypointSymbol\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(symbolBlocks).toContain("width: calc(var(--thumb) * 0.78)");
    expect(symbolBlocks).toContain("height: calc(var(--thumb) * 0.78)");
  });

  it("moves altitude planning to a standalone core-driven page", () => {
    const plannerPage = appSource.slice(
      appSource.indexOf("function AltitudePlannerPage("),
      appSource.indexOf("function FlightPlanPage("),
    );
    expect(appSource).toContain('onClick={() => props.onSelectPage("altitude")}');
    expect(plannerPage).toContain("props.onQueryAltitudeComparisons()");
    expect(plannerPage).toContain("props.onPerformAltitudePlannerAction(actionUid)");
    expect(plannerPage).toContain('props.onSetDepartureInput(field, input)');
    expect(plannerPage).toContain('submitDepartureInput("time", departureTimeInput)');
    expect(plannerPage).toContain('submitDepartureInput("when", departureWhenInput)');
    expect(plannerPage).toContain("setDepartureWhenInput(planner.departure.when_value)");
    expect(plannerPage).toContain("props.onToggleDepartureTimeBasis()");
    expect(plannerPage).toContain("comparisonControlKey");
    expect(plannerPage).toContain("<TrayDock");
    expect(plannerPage).toContain("<TrayScrim");
    expect(plannerPage).not.toContain("altitudePlannerControlOptions");
    expect(plannerPage).not.toContain('type="datetime-local"');
    expect(plannerPage).not.toContain("new Date(");
    expect(plannerPage).toContain("planner.forecast.rows.map");
    expect(plannerPage).toContain("row.selected");
    expect(plannerPage).toContain("altitude-planner-wind-action-${row.id}");
    expect(plannerPage).toContain("performAction(action.action_uid)");
    expect(plannerPage).toContain('" isActive selectedControlHighlight"');
    const selectedWindActionStyles = [...styles.matchAll(/\.selectedControlHighlight\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(selectedWindActionStyles).toContain("0 0 0 2px var(--theme-button-fg)");
    expect(selectedWindActionStyles).toContain("0 0 0 4px var(--theme-button-checked)");
    expect(plannerPage).toContain("panel?.advisories");
    expect(plannerPage).not.toMatch(/onPerformAltitudePlannerAction\(actionUid\)[\s\S]*?\.then\(reload\)/);
    expect(plannerPage).toContain("panel.columns.map");
    expect(plannerPage).toContain("row.cells.map");
    expect(plannerPage).toContain('className="altitudeComparisonRegion"');
    expect(plannerPage).toContain('className="altitudeComparisonLoading"');
    expect(plannerPage).toContain("{loading && showUserActionSpinner ? (");
    expect(plannerPage).toContain("comparisonRequestGeneration.current += 1");
    expect(plannerPage).toContain("if (enteredAltitudePlanner) setShowUserActionSpinner(true)");
    const controlTray = plannerPage.slice(
      plannerPage.indexOf('data-testid="altitude-planner-control-tray"'),
      plannerPage.indexOf("</header>"),
    );
    expect(controlTray).toContain("planner.controls.map");
    expect(controlTray).toContain("altitudePlannerDeparture");
    const departureStyles = [...styles.matchAll(/\.altitudePlannerDeparture\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(departureStyles).toContain("display: flex");
    expect(departureStyles).toContain("height: var(--thumb)");
    expect(departureStyles).toContain("white-space: nowrap");
    expect(departureStyles).toContain("background: var(--theme-control-group-bg)");
    const departureInputStyles = [...styles.matchAll(/\.altitudePlannerDeparture input\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(departureInputStyles).toContain("background: var(--theme-text-input-bg)");
    expect(plannerPage).not.toContain("departureWhenInput.length + 4");
    const departureWhenInputStyles = [
      ...styles.matchAll(/\.altitudePlannerDeparture input\.altitudePlannerDepartureWhen\s*\{([^}]*)\}/g),
    ]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(departureWhenInputStyles).toContain("width: calc(var(--thumb) * 1.25)");
    expect(departureWhenInputStyles).toContain("min-width: calc(var(--thumb) * 1.25)");
    expect(departureWhenInputStyles).toContain("max-width: calc(var(--thumb) * 1.25)");
    const departureBasisStyles = [...styles.matchAll(/\.altitudePlannerDepartureBasis\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(departureBasisStyles).toContain("min-width: calc(var(--thumb) * 1.45)");
    expect(departureBasisStyles).toContain("height: calc(var(--thumb) * 0.58)");
    expect(plannerPage).toContain('planner.departure.when_is_past ? " isWarning" : ""');
    const departureWarningStyles = [...styles.matchAll(/\.altitudePlannerDeparture input\.isWarning\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(departureWarningStyles).toContain("var(--theme-data-status-warning-stroke)");
    expect(plannerPage).not.toContain('{loading ? <p>Calculating…</p> : null}');
    expect(plannerPage).toContain('comparisonRefreshRevision');
    expect(plannerPage).toContain('setComparisonRefreshRevision((revision) => revision + 1)');
    expect(plannerPage).toContain('if (userActionPendingRefresh.current) return;');
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
