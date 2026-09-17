// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, useRef } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { GeographicLineOverlay } from "./GeographicLineOverlay";
import { useMapGeometryBinding } from "./MapGeometryLayer";
import { latLonToWorld, type MapViewportState } from "./domain/mapViewport";

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
