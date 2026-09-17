// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, it, expect, vi } from "vitest";
import { GlideRingOverlay } from "./GlideRingOverlay";
import type { UiSession } from "./domain/appCoreAdapter";
import type { UiGlideRing } from "./generated/sessionPageWire";
import type { MapGeometryBinding } from "./MapGeometryLayer";

describe("GlideRingOverlay", () => {
  it("binds the path to map content and ignores completion after unmount", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    vi.useFakeTimers();
    const controls = document.createElement("div");
    const mapContent = document.createElement("div");
    document.body.append(controls, mapContent);
    const geometry = {
      host: mapContent, frame: {width: 300, height: 500},
      screen: ({lat, lon}: {lat: number; lon: number}) => ({x: lon, y: lat}),
      displayViewport: () => ({rotationDeg: 90}),
    } as MapGeometryBinding;
    let resolve!: (ring: UiGlideRing) => void;
    const queryGlideRing = vi.fn(() => new Promise<UiGlideRing>(r => { resolve = r; }));
    const session = {queryGlideRing} as unknown as UiSession;
    const root = createRoot(controls);
    const ring: UiGlideRing = {paths: [[{lat: 1, lon: 2}, {lat: 3, lon: 4}]],
      label_position: {lat: 1, lon: 2}, wind_label: "+12", wind_direction_deg_true: 90, speed_label: "62 kt IAS",
      message: null, recheck_after_ms: 1000};
    try {
      await act(async () => root.render(<GlideRingOverlay session={session} geometry={geometry} />));
      await act(async () => vi.advanceTimersByTime(10000));
      expect(queryGlideRing).toHaveBeenCalledTimes(1);
      await act(async () => resolve(ring));
      expect(controls.querySelector("svg")).toBeNull();
      expect(mapContent.querySelector("path")?.getAttribute("d")).toBe("M2,1 L4,3");
      expect(mapContent.querySelector("svg")?.style.pointerEvents).toBe("none");
      expect(mapContent.textContent).toContain("62 kt IAS");
      expect(mapContent.querySelector("rect")).toBeNull();
      expect(mapContent.querySelector(".mapUpright")?.textContent).toContain("62 kt IAS");
      expect((mapContent.querySelector('[data-testid="glide-wind-arrow"]') as SVGElement).style.transform)
        .toBe("rotate(calc(90deg - var(--map-up-deg, 0deg)))");
      await act(async () => vi.advanceTimersByTime(1000));
      expect(queryGlideRing).toHaveBeenCalledTimes(2);
      await act(async () => root.unmount());
      await act(async () => resolve(ring));
      await act(async () => vi.advanceTimersByTime(10000));
      expect(mapContent.childElementCount).toBe(0);
      expect(queryGlideRing).toHaveBeenCalledTimes(2);
    } finally {
      controls.remove(); mapContent.remove(); vi.useRealTimers(); vi.unstubAllGlobals();
    }
  });
});
