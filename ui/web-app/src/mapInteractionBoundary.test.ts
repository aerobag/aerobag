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
  it("keeps map-selection inspector interactions from bubbling into map gestures", () => {
    const trayShell = sourceBetween(
      'className="mapSelectionTray"',
      "{result.categories.map",
    );
    const detailShell = sourceBetween(
      'className="mapSelectionDetailModal"',
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

  it("treats an open map-selection inspector as a hard map gesture boundary", () => {
    expect(functionSource("handlePointerRelease")).toContain("trayGroup.scrimOpen || mapSelection");
    expect(functionSource("handleWheel")).toContain("trayGroup.scrimOpen || mapSelection");
    expect(functionSource("handleDoubleClick")).toContain("trayGroup.scrimOpen || mapSelection");
  });

  it("uses lightweight hover weather for METAR symbols without opening the map inspector", () => {
    const hoverSource = functionSource("handleMetarHoverEnter");
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
});
