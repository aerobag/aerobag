import { describe, expect, it } from "vitest";
import { mapView } from "./sampleData";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  renderTiles,
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

  it("initial viewport renders real package tiles and round-trips its center lat/lon", () => {
    const viewport = createInitialViewport(mapView);
    const tiles = renderTiles(mapView, viewport, 1200, 900);
    const center = viewportCenterLatLon(viewport);

    expect(tiles.length).toBeGreaterThan(0);
    expect(tiles.every((tile) => tile.src.startsWith(mapView.tile_url_root))).toBe(true);
    expect(center.lat).toBeCloseTo(mapView.initial_viewport.lat, 3);
    expect(center.lon).toBeCloseTo(mapView.initial_viewport.lon, 3);
  });
});
