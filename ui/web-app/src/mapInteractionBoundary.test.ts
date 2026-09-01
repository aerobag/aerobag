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

function functionSource(name: string): string {
  const start = appSource.indexOf(`function ${name}`);
  expect(start, name).toBeGreaterThanOrEqual(0);
  const nextFunction = appSource.indexOf("\n  function ", start + 1);
  expect(nextFunction, `${name} end`).toBeGreaterThan(start);
  return appSource.slice(start, nextFunction);
}

describe("map interaction boundaries", () => {
  it("routes production map orientation through the planned map-up value", () => {
    expect(appSource).toContain("const plannedMapUpDeg = resolveMapUpDegrees(");
    expect(appSource).toContain("mapUpDeg={plannedMapUpDeg}");
    expect(appSource).toContain("rotationDeg: plannedMapUpDeg");
    expect(appSource).not.toContain('aria-label="Debug map-up rotation"');
  });

  it("clips rotated rasters at the map surface rather than before rotation", () => {
    const mapSurfaceBlocks = [...styles.matchAll(/\.mapSurface\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(mapSurfaceBlocks).toContain("overflow: hidden");

    const rasterLayerBlocks = [...styles.matchAll(/\.rasterTileLayer\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(rasterLayerBlocks).toContain("overflow: visible");
    expect(rasterLayerBlocks).not.toContain("overflow: hidden");
  });

  it("keeps map-selection inspector interactions from bubbling into map gestures", () => {
    const trayShell = sourceBetween(
      'className="mapSelectionTray"',
      "{result.categories.map",
    );
    const detailShell = sourceBetween(
      'className="mapSelectionDetailModal weatherDetailModal"',
      '<div className="mapSelectionDetailTitle">',
    );
    const requiredHandlers = [
      "onPointerDown={stopPointer}",
      "onPointerMove={stopPointer}",
      "onPointerUp={stopPointer}",
      "onPointerCancel={stopPointer}",
      "onWheel={stopWheel}",
      "onClick={stopClick}",
      "onDoubleClick={stopDoubleClick}",
    ];

    for (const handler of requiredHandlers) {
      expect(trayShell).toContain(handler);
      expect(detailShell).toContain(handler);
    }
  });

  it("allows map-selection detail text to be selected inside the non-selectable map surface", () => {
    const detailModalBlocks = [...styles.matchAll(/\.mapSelectionDetailModal\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(detailModalBlocks).toContain("user-select: text");
  });

  it("does not shrink airport identity text on either inspector surface", () => {
    const selectionSecondaryBlocks = [...styles.matchAll(/\.mapSelectionActionTitleSecondary\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(selectionSecondaryBlocks).toContain("font-size: inherit");

    const airportIdentityBlocks = [...styles.matchAll(/\.airportInfoName,\s*\n\.airportInfoLocation\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(airportIdentityBlocks).toContain("font-size: calc(var(--thumb) * 0.16)");
    expect(airportIdentityBlocks).toContain("font-weight: 800");
  });

  it("draws the selected inspector item outside its existing shape", () => {
    expect(appSource).toContain('" isSelected selectedControlHighlight"');
    const selectedItemBlocks = [...styles.matchAll(/\.selectedControlHighlight\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(selectedItemBlocks).toContain("0 0 0 2px var(--theme-button-fg)");
    expect(selectedItemBlocks).toContain("0 0 0 4px var(--theme-button-checked)");
    expect(appSource).toContain('mapSelectionItem${selectedItem?.id === item.id ? " isSelected selectedControlHighlight" : ""}');

    const itemBlocks = [...styles.matchAll(/\.mapSelectionItem\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(itemBlocks).toContain("width: var(--thumb)");
    expect(itemBlocks).toContain("height: var(--thumb)");
  });

  it("uses the weather modal as the only weather-detail scroll viewport", () => {
    const modalBlocks = [...styles.matchAll(/\.mapSelectionDetailModal\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(modalBlocks).toContain("overflow-y: auto");

    const textOverrideBlocks = [...styles.matchAll(
      /\.weatherDetailModal:not\(\.hoverWeatherDetailModal\) \.weatherDetailText\s*\{([^}]*)\}/g,
    )].map((match) => match[1] ?? "").join("\n");
    expect(textOverrideBlocks).toContain("max-height: none");
    expect(textOverrideBlocks).toContain("overflow: visible");

    const notamListOverrideBlocks = [...styles.matchAll(
      /\.weatherDetailModal:not\(\.hoverWeatherDetailModal\) \.airportNotamList\s*\{([^}]*)\}/g,
    )].map((match) => match[1] ?? "").join("\n");
    expect(notamListOverrideBlocks).toContain("max-height: none");
    expect(notamListOverrideBlocks).toContain("overflow: visible");
  });

  it("treats an open map-selection inspector as a hard map gesture boundary", () => {
    expect(functionSource("handlePointerRelease")).toContain("trayGroup.scrimOpen || mapSelection");
    expect(functionSource("handleWheel")).toContain("trayGroup.scrimOpen || mapSelection");
    expect(functionSource("handleDoubleClick")).toContain("trayGroup.scrimOpen || mapSelection");
  });

  it("accepts the first plate gesture before the viewport synchronization effect runs", () => {
    const chartsPage = sourceBetween(
      "function ChartsPage(props:",
      "function HomePage(props:",
    );
    for (const handler of ["handlePointerDown", "handleWheel", "handleDoubleClick"]) {
      const start = chartsPage.indexOf(`function ${handler}`);
      expect(start, handler).toBeGreaterThanOrEqual(0);
      const end = chartsPage.indexOf("\n  function ", start + 1);
      const source = chartsPage.slice(start, end);
      expect(source).toContain("viewportRef.current ?? effectiveViewport");
    }
  });

  it("opens a raw map click with core's preferred point already selected", () => {
    const releaseSource = functionSource("handlePointerRelease");
    expect(releaseSource).toContain(
      "mapSelectionItemById(result, result.initial_selected_item_id ?? null)",
    );
    expect(releaseSource).not.toContain("selectedItem: null");
  });

  it("rejects a raw map-click result after a newer click or viewport", () => {
    const releaseSource = functionSource("handlePointerRelease");
    const viewportSource = functionSource("updateViewport");

    expect(releaseSource).toContain("mapSelectionRequestGenerationRef.current");
    expect(releaseSource).toContain("selectionGeneration !== mapSelectionRequestGenerationRef.current");
    expect(viewportSource).toContain("mapSelectionRequestGenerationRef.current += 1");
  });

  it("refreshes NEXRAD for viewport changes before the first frame lands", () => {
    expect(appSource).not.toContain("nexradHasPaintableFrameRef");
    expect(appSource).toContain("if (!nexradOverlayFrame && !lastRequest)");
    expect(appSource).toContain("requestThrottledNexradViewportRefresh()");
  });

  it("continues core-directed NEXRAD cache GC while the layer is hidden", () => {
    const queryEffect = sourceBetween(
      "if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {\n      nexradQueryRequestRef.current = null;",
      "if (!mapIsVisible || !mapLayerState.nexrad.visible || !uiSession) {",
    );
    expect(queryEffect).not.toContain("!mapLayerState.nexrad.visible");
    expect(queryEffect).toContain("pumpNexradQueryQueue()");

    const imageElement = sourceBetween("<image\n                        href={nexradFrameCacheRef.current", "preserveAspectRatio=\"none\"");
    expect(imageElement).not.toContain("href={resolveLiveFeedResourceUrl(tile.src)}");
  });

  it("uses lightweight hover weather for METAR symbols without opening the map inspector", () => {
    const hoverSource = sourceBetween(
      "const handleMetarHoverEnter",
      "const handleMetarHoverLeave",
    );
    expect(hoverSource).toContain('event.pointerType !== "mouse"');
    expect(hoverSource).toContain("queryMapSelection");
    expect(hoverSource).toContain("weatherDetailForMetarSelection");

    const metarOverlay = sourceBetween(
      'className="metarOverlay"',
      "</svg>",
    );
    expect(metarOverlay).toContain('className="metarHoverTarget"');
    expect(metarOverlay).toContain("onPointerEnter={(event) => handleMetarHoverEnter(event, feature)}");
    expect(metarOverlay).toContain("onPointerLeave={() => handleMetarHoverLeave(feature)}");
    expect(appSource).toContain('className="hoverWeatherDetailModal"');
  });

  it("keeps METAR hover hit targets active while leaving the hover weather panel pointer-transparent", () => {
    const metarHitBlocks = [...styles.matchAll(/\.(?:metarHoverTarget|metarHoverHitTarget)(?:,\s*\n\.(?:metarHoverTarget|metarHoverHitTarget))*\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(metarHitBlocks).toContain("pointer-events: all");

    const hoverPanelBlocks = [...styles.matchAll(/\.hoverWeatherDetailModal\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(hoverPanelBlocks).toContain("transform: none");
    expect(hoverPanelBlocks).toContain("pointer-events: none");
  });

  it("keeps core-actionable time cells and the ETA column header clickable", () => {
    const actionableFlightDataBlocks = [...styles.matchAll(/\.flightDataCell\.isActionable\s*\{([^}]*)\}/g)]
      .map((match) => match[1] ?? "")
      .join("\n");
    expect(actionableFlightDataBlocks).toContain("pointer-events: auto");

    const banner = sourceBetween("function FlightDataBanner", "function FlightDataCellContents");
    expect(banner).toContain("cell.action_id");
    expect(banner).toContain("props.onAction(cell.action_id!)");

    const planHeaders = sourceBetween("planDataColumns.map((column)", "displayRows.map((row)");
    expect(planHeaders).toContain("column.action_id");
    expect(planHeaders).toContain("props.onFlightPlanColumnAction(column.action_id!)");
  });

  it("routes raster, vector, and terrain work through the shared landing policy", () => {
    expect(functionSource("pumpTerrainRenderQueue")).toContain("shouldLandCompletedCoalescedWork");
    expect(functionSource("pumpMapOverlayQueryQueue")).toContain("shouldLandCompletedCoalescedWork");
    expect(functionSource("pumpRasterTilePlanQueue")).toContain("shouldLandCompletedCoalescedWork");
    expect(appSource).not.toContain("supersededTerrainRequestCanLand");
    expect(appSource).not.toContain("supersededMapOverlayCanLand");
    expect(appSource).not.toContain("supersededRasterPlanCanLand");
  });
});
