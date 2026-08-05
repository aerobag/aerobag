// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { compassSymbol } from "./generated/navSymbols";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

describe("primary navigation layout", () => {
  it("uses one shared bottom dock on every top-level product page", () => {
    expect(appSource.match(/<PrimaryNavigationDock/g)).toHaveLength(8);
    expect(appSource).not.toContain("primaryNavigationDockStatic");
    expect(styles).not.toContain(".primaryNavigationDockStatic");
    expect(styles).not.toContain(".planFooter");

    const dock = sourceBetween("function PrimaryNavigationDock(", "function TrayDock(");
    expect(dock).toContain("<HomeNavButton");
    expect(dock).toContain("<NavElementButton");
    expect(dock).toContain("<ChartPlateToggleButton");
    expect(dock).toContain("<ChartPlateReturnButton");
  });

  it("keeps page navigation out of the map and plate top control rows", () => {
    const mapPage = sourceBetween("function MapPage(", "function NavElementButton(");
    const chartsPage = sourceBetween("function ChartsPage(", "function HomePage(");
    const settingsPage = sourceBetween("function SettingsPage(", "function DataStatusPage(");
    const dataStatusPage = sourceBetween("function DataStatusPage(", "function DataStatusPageRowArticle(");

    for (const page of [mapPage, chartsPage, settingsPage, dataStatusPage]) {
      expect(page).not.toContain("<HomeNavButton");
      expect(page).not.toContain("<ChartPlateToggleButton");
      expect(page).toContain("<PrimaryNavigationDock");
    }
  });

  it("places the map-only CTR and orientation controls after Search instead of beside DBG", () => {
    const mapPage = sourceBetween("function MapPage(", "function NavElementButton(");
    const searchIndex = mapPage.indexOf("<ChartSearchBox");
    const centerHereIndex = mapPage.indexOf('className={`centerHereButton');
    const orientationIndex = mapPage.indexOf("<MapOrientationButton");
    const bottomRightIndex = mapPage.indexOf("mapBottomRightDock");

    expect(searchIndex).toBeGreaterThanOrEqual(0);
    expect(centerHereIndex).toBeGreaterThan(searchIndex);
    expect(orientationIndex).toBeGreaterThan(centerHereIndex);
    expect(orientationIndex).toBeLessThan(bottomRightIndex);
    expect(mapPage.slice(bottomRightIndex)).not.toContain("centerHereButton");
    expect(mapPage).not.toContain("DebugMapUpSlider");
  });

  it("renders the shared compass symbol with triangular needle halves", () => {
    const orientationButton = sourceBetween("function MapOrientationButton(", "function NavElementButton(");
    expect(orientationButton).toContain("<RenderNavSymbolLayers layers={compassSymbol} />");
    expect(orientationButton).not.toContain("mapOrientationNeedleNorth");
    expect(styles).not.toContain(".mapOrientationNeedle");
    expect(compassSymbol.find((layer) => layer.paint === "compass_north_needle")?.path)
      .toBe("M 0 -15 L 3.2 0 L -3.2 0 Z");
    expect(compassSymbol.find((layer) => layer.paint === "compass_south_needle")?.path)
      .toBe("M 0 15 L -3.2 0 L 3.2 0 Z");
  });

  it("makes the CDI and both square buttons one thumb high", () => {
    const navElementCss = styles.match(/\.navElement\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(navElementCss).toMatch(/height:\s*var\(--thumb\)/);
    expect(styles).toContain(".primaryNavigationDock");
    expect(styles).toContain("gap: var(--thumb-gap)");
  });

  it("raises both corner-control docks when they would collide with primary navigation", () => {
    const mapPage = sourceBetween("function MapPage(", "function NavElementButton(");
    expect(mapPage).toContain("shouldRaiseBottomCornerControls(surfaceSize.width)");
    expect(mapPage).toContain('raisedForPrimaryNavigation={bottomCornerControlsRaised}');
    expect(mapPage).toContain('bottomCornerControlsRaised ? " isRaisedForPrimaryNavigation"');
    expect(styles).toContain(".mapBottomRightDock.isRaisedForPrimaryNavigation");
    expect(styles).toContain(".zoomControl.isRaisedForPrimaryNavigation");
    expect(styles).toContain("bottom: calc(var(--thumb) + (var(--thumb-gap) * 2) + var(--safe-bottom))");
  });
});
