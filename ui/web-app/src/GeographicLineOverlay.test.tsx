// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, useRef } from "react";
import { readFileSync } from "node:fs";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { GeographicLineOverlay } from "./GeographicLineOverlay";
import { GlideRingGeometry } from "./GlideRingOverlay";
import { useMapGeometryBinding } from "./MapGeometryLayer";
import type { MapGeometryBinding } from "./MapGeometryLayer";
import { latLonToWorld, type MapViewportState } from "./domain/mapViewport";

it("keeps the intercept arc above glide reach even when glide geometry arrives later", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const style = document.createElement("style");
  style.textContent = readFileSync("src/styles.css", "utf8");
  document.head.append(style);
  const host = document.createElement("div"), content = document.createElement("div");
  document.body.append(host, content);
  const root = createRoot(host);
  const binding = {
    host: content, frame: {width: 320, height: 320},
    screen: ({lat, lon}: {lat: number; lon: number}) => ({x: lon, y: lat}),
  } as MapGeometryBinding;
  const points = [{lat: 160, lon: 40}, {lat: 160, lon: 280}];
  try {
    for (const visible of [false, true, false, true]) {
      await act(async () => root.render(<>
        <GeographicLineOverlay binding={binding} annotation={{
          points, label_position: points[1], label: "1500 GPS", label_bearing_deg: 0,
        }} />
        {visible ? <GlideRingGeometry geometry={binding} ring={{
          paths: [points], label_position: null, wind_label: "", wind_direction_deg_true: null,
          speed_label: "", message: null, recheck_after_ms: 1000,
        }} /> : null}
      </>));
      const arc = content.querySelector('[data-testid="altitude-intercept-arc"]')!;
      expect(getComputedStyle(arc).pointerEvents).toBe("none");
      if (visible) {
        const glide = content.querySelector('[data-testid="glide-ring"]')!;
        const arcLayer = Number(getComputedStyle(arc).zIndex) || 0;
        const glideLayer = Number(getComputedStyle(glide).zIndex) || 0;
        expect(arcLayer).toBeGreaterThan(glideLayer);
        expect(getComputedStyle(glide).pointerEvents).toBe("none");
      }
    }
  } finally {
    await act(async () => root.unmount());
    host.remove(); content.remove(); style.remove(); vi.unstubAllGlobals();
  }
});

it("keeps arc geometry inside the map's immediate transform and reprojects on zoom", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const world = latLonToWorld(47, -122);
  const viewport: MapViewportState = { centerWorldX: world.x, centerWorldY: world.y, zoom: 8, rotationDeg: 0 };
  function Map({ zoom, rotation = 0, visible = true }: { zoom: number; rotation?: number; visible?: boolean }) {
    const surface = useRef<HTMLDivElement>(null), live = useRef({...viewport, zoom}); live.current = {...viewport, zoom};
    const up = useRef(rotation), content = useRef<HTMLDivElement>(null); up.current = rotation;
    const { binding, bindContent } = useMapGeometryBinding({...viewport, zoom}, 600, 800, surface, live, up, content);
    return <div ref={surface}><div data-testid="map-content" ref={bindContent} />
      <GeographicLineOverlay binding={binding} annotation={visible ? {
        points: [{lat: 47, lon: -122}, {lat: 47, lon: -121.9}], label_position: {lat: 47, lon: -121.9}, label: "1500 GPS", label_bearing_deg: 90,
      } : null} />
    </div>;
  }
  try {
    await act(async () => root.render(<Map zoom={8} />));
    const content = host.querySelector<HTMLElement>('[data-testid="map-content"]')!;
    const arc = content.querySelector('[data-testid="altitude-intercept-arc"]')!;
    expect(arc.textContent).toBe("1500 GPS");
    expect(arc.querySelector("text")!.getAttribute("transform")).toMatch(/^rotate\(90,/);
    const initial = arc.querySelector("polyline")!.getAttribute("points");
    content.style.transform = "translate(90px, 30px) scale(1.2)";
    expect(content.contains(arc)).toBe(true);
    expect(arc.querySelector("polyline")!.getAttribute("points")).toBe(initial);
    await act(async () => root.render(<Map zoom={9} rotation={90} />));
    expect(arc.querySelector("polyline")!.getAttribute("points")).not.toBe(initial);
    // The shared map transform supplies -90 degrees in track-up; the label
    // retains its geographic bearing instead of counter-rotating the map.
    expect(arc.querySelector("text")!.getAttribute("transform")).toMatch(/^rotate\(90,/);
    await act(async () => root.render(<Map zoom={9} visible={false} />));
    expect(host.querySelector('[data-testid="altitude-intercept-arc"]')).toBeNull();
  } finally { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); }
});
