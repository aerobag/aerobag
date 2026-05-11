import { describe, expect, it } from "vitest";
import { mapView } from "./sampleData";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  preserveViewportForMap,
  rasterPlanViewportForDevice,
  screenToWorld,
  viewportCenterLatLon,
  zoomAroundPoint,
} from "./mapViewport";

describe("mapViewport", () => {
  it("zoomAroundPoint keeps the anchored chart point under the cursor", () => {
    const viewport = createInitialViewport(mapView);
    const width = 1200;
    const height = 900;
    const anchor = { x: 320, y: 280 };
    const anchoredWorld = screenToWorld(viewport, anchor, width, height);

    const zoomed = zoomAroundPoint(viewport, mapView, anchor, width, height, viewport.zoom + 0.8);
    const anchoredWorldAfter = screenToWorld(zoomed, anchor, width, height);

    expect(anchoredWorldAfter.x).toBeCloseTo(anchoredWorld.x, 8);
    expect(anchoredWorldAfter.y).toBeCloseTo(anchoredWorld.y, 8);
  });

  it("pinch math preserves both initial touch anchors for collinear motion", () => {
    const viewport = createInitialViewport(mapView);
    const width = 1200;
    const height = 900;
    const startFirst = { x: 320, y: 450 };
    const startSecond = { x: 880, y: 450 };
    const snapshot = createPinchSnapshot(viewport, startFirst, startSecond, width, height);
    const movedFirst = { x: 260, y: 450 };
    const movedSecond = { x: 940, y: 450 };

    const pinched = applyPinchGesture(snapshot, movedFirst, movedSecond, mapView, width, height);
    const firstWorldAfter = screenToWorld(pinched, movedFirst, width, height);
    const secondWorldAfter = screenToWorld(pinched, movedSecond, width, height);

    expect(firstWorldAfter.x).toBeCloseTo(snapshot.anchorOneWorld.x, 8);
    expect(firstWorldAfter.y).toBeCloseTo(snapshot.anchorOneWorld.y, 8);
    expect(secondWorldAfter.x).toBeCloseTo(snapshot.anchorTwoWorld.x, 8);
    expect(secondWorldAfter.y).toBeCloseTo(snapshot.anchorTwoWorld.y, 8);
  });

  it("double-click style zoom-in still preserves the clicked anchor", () => {
    const viewport = createInitialViewport(mapView);
    const width = 1280;
    const height = 900;
    const anchor = { x: 640, y: 360 };
    const anchoredWorld = screenToWorld(viewport, anchor, width, height);

    const zoomed = zoomAroundPoint(viewport, mapView, anchor, width, height, viewport.zoom + 0.75);
    const anchoredWorldAfter = screenToWorld(zoomed, anchor, width, height);

    expect(zoomed.zoom).toBeCloseTo(viewport.zoom + 0.75, 8);
    expect(anchoredWorldAfter.x).toBeCloseTo(anchoredWorld.x, 8);
    expect(anchoredWorldAfter.y).toBeCloseTo(anchoredWorld.y, 8);
  });

  it("switching layers preserves map center while clamping zoom only if needed", () => {
    const viewport = createInitialViewport(mapView);
    const moved = {
      centerWorldX: viewport.centerWorldX + 12.5,
      centerWorldY: viewport.centerWorldY - 8.75,
      zoom: 10.4,
    };
    const otherMapView = {
      ...mapView,
      min_zoom: 4.2,
      max_zoom: 9.8,
    };

    const preserved = preserveViewportForMap(moved, otherMapView);

    expect(preserved.centerWorldX).toBeCloseTo(moved.centerWorldX, 8);
    expect(preserved.centerWorldY).toBeCloseTo(moved.centerWorldY, 8);
    expect(preserved.zoom).toBeCloseTo(moved.zoom, 8);
  });

  it("clamps zoom to the published display max exactly", () => {
    const capped = zoomAroundPoint(
      createInitialViewport(mapView),
      { ...mapView, max_zoom: 12.5 },
      { x: 500, y: 400 },
      1200,
      900,
      13.2,
    );

    expect(capped.zoom).toBe(12.5);
  });

  it("keeps clamped high-DPI raster plans in their effective CSS viewport", () => {
    const viewport = {
      centerWorldX: 42,
      centerWorldY: 99,
      zoom: 11,
    };

    const planned = rasterPlanViewportForDevice(viewport, 3, 12.5);

    expect(planned.deviceViewport.centerWorldX).toBe(viewport.centerWorldX);
    expect(planned.deviceViewport.centerWorldY).toBe(viewport.centerWorldY);
    expect(planned.deviceViewport.zoom).toBeCloseTo(12.5, 8);
    expect(planned.cssViewport.centerWorldX).toBe(viewport.centerWorldX);
    expect(planned.cssViewport.centerWorldY).toBe(viewport.centerWorldY);
    expect(planned.cssViewport.zoom).toBeCloseTo(12.5 - Math.log2(3), 8);
    expect(planned.cssViewport.zoom).toBeLessThan(viewport.zoom);
  });
});
