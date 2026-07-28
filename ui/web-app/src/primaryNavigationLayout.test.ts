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

describe("primary navigation layout", () => {
  it("uses one shared bottom dock on every top-level product page", () => {
    expect(appSource.match(/<PrimaryNavigationDock/g)).toHaveLength(6);

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

  it("places the map-only CTR control after Search instead of beside DBG", () => {
    const mapPage = sourceBetween("function MapPage(", "function NavElementButton(");
    const searchIndex = mapPage.indexOf("<ChartSearchBox");
    const centerHereIndex = mapPage.indexOf('className={`centerHereButton');
    const bottomRightIndex = mapPage.indexOf('<div className="mapBottomRightDock">');

    expect(searchIndex).toBeGreaterThanOrEqual(0);
    expect(centerHereIndex).toBeGreaterThan(searchIndex);
    expect(centerHereIndex).toBeLessThan(bottomRightIndex);
    expect(mapPage.slice(bottomRightIndex)).not.toContain("centerHereButton");
  });

  it("makes the CDI and both square buttons one thumb high", () => {
    const navElementCss = styles.match(/\.navElement\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(navElementCss).toMatch(/height:\s*var\(--thumb\)/);
    expect(styles).toContain(".primaryNavigationDock");
    expect(styles).toContain("gap: var(--thumb-gap)");
  });
});
