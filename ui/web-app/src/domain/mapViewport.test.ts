import { describe, expect, it } from "vitest";
import { mapView } from "./sampleData";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  dragViewport,
  isStaleMapFollowTargetViewport,
  latLonToWorld,
  preserveViewportForMap,
  sameMapViewport,
  screenToWorld,
  transformScreenPointBetweenFrames,
  viewportCenterLatLon,
  worldToScreen,
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

  it("initial viewport round-trips its center lat/lon", () => {
    const viewport = createInitialViewport(mapView);
    const center = viewportCenterLatLon(viewport);

    expect(center.lat).toBeCloseTo(mapView.initial_viewport.lat, 3);
    expect(center.lon).toBeCloseTo(mapView.initial_viewport.lon, 3);
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

  it("treats tiny viewport differences as the same viewport", () => {
    const viewport = createInitialViewport(mapView);
    const nearlySame = {
      centerWorldX: viewport.centerWorldX + 1e-10,
      centerWorldY: viewport.centerWorldY - 1e-10,
      zoom: viewport.zoom + 1e-10,
    };

    expect(sameMapViewport(viewport, nearlySame)).toBe(true);
  });

  it("rejects stale map-follow targets while waiting for a follow-sync acknowledgement", () => {
    const awaited = createInitialViewport(mapView);
    const stale = {
      ...awaited,
      centerWorldX: awaited.centerWorldX + 0.25,
    };

    expect(isStaleMapFollowTargetViewport(stale, awaited)).toBe(true);
    expect(isStaleMapFollowTargetViewport(awaited, awaited)).toBe(false);
    expect(isStaleMapFollowTargetViewport(stale, null)).toBe(false);
  });

  it("carries screen-space overlay points across resize", () => {
    const viewport = createInitialViewport(mapView);
    const world = latLonToWorld(42.1, -70.8);
    const oldFrame = { viewport, width: 1200, height: 900 };
    const newFrame = { viewport, width: 900, height: 1200 };
    const oldScreen = worldToScreen(viewport, world, oldFrame.width, oldFrame.height);

    const carried = transformScreenPointBetweenFrames(oldFrame, newFrame, oldScreen);
    const direct = worldToScreen(viewport, world, newFrame.width, newFrame.height);

    expect(carried.x).toBeCloseTo(direct.x, 8);
    expect(carried.y).toBeCloseTo(direct.y, 8);
  });

  it("carries screen-space overlay points across pan", () => {
    const viewport = createInitialViewport(mapView);
    const nextViewport = dragViewport(viewport, 173, -91);
    const world = latLonToWorld(42.1, -70.8);
    const oldFrame = { viewport, width: 1200, height: 900 };
    const newFrame = { viewport: nextViewport, width: 1200, height: 900 };
    const oldScreen = worldToScreen(viewport, world, oldFrame.width, oldFrame.height);

    const carried = transformScreenPointBetweenFrames(oldFrame, newFrame, oldScreen);
    const direct = worldToScreen(nextViewport, world, newFrame.width, newFrame.height);

    expect(carried.x).toBeCloseTo(direct.x, 8);
    expect(carried.y).toBeCloseTo(direct.y, 8);
  });
});
