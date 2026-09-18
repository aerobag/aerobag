// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, useState, type ComponentProps } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { ChartsPage, NavigationPageOptionsContext } from "./App";
import { SessionRenderStore } from "./domain/sessionRenderStore";
import type { UiSessionSnapshot } from "./domain/appCoreAdapter";
import type { ImageViewportState } from "./domain/imageViewport";

it("publishes an input-ready viewport only after the selected image loads, including replacement and remount", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class {
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) {
      this.callback([{ target, contentRect: { width: 1000, height: 900 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    disconnect() {}
  });
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const store = new SessionRenderStore({
    app_ui_state: {
      ownship: {
        render: { draw_aircraft: false, position: null },
        controls: { sources: [], situation_controls: [], launcher_label: "No GPS", launcher_tone: "unavailable" },
      },
      aircraft_plan_view_path: "",
    },
    playback_panel_state: { visible: false },
  } as unknown as UiSessionSnapshot);
  const viewportChanges = vi.fn();
  const resolveAsset = vi.fn(async (id: string) => `/${id}.png`);
  function Viewer({ chartId }: { chartId: string }) {
    // A persisted viewport is deliberately present before the replacement
    // asset. The launcher/viewport state is not evidence that pixels loaded.
    const [viewport, setViewport] = useState<ImageViewportState | null>({ left: 200, top: 0, zoom: 1 });
    const props = {
      page: "charts", sessionRenderStore: store, appCoreAdapter: null,
      selectedChart: { id: chartId, label: chartId, kind: "reference", folder_category: "other" },
      selectedCollection: null, suggestedChartIds: [], airportMenuEntries: [],
      collectionControl: { launcher_label: "Airport", enabled: true },
      chartControl: { launcher_label: chartId, enabled: true },
      projectedProcedureLoadMenu: { options: [], launcher_label: "Load", enabled: false },
      statusControls: { controls: [] }, folderOpen: false, viewport,
      onViewportChange: (next: ImageViewportState | null) => { viewportChanges(next); setViewport(next); },
      debugState: { plate_flight_plan: false }, navDataEpoch: 0, flightPlanRouteRevision: 0,
      uiSession: { resolveChartAssetUrl: resolveAsset }, onFirstVisualReady: () => {},
    } as unknown as ComponentProps<typeof ChartsPage>;
    return <NavigationPageOptionsContext.Provider value={{
      options: [{ id: "home", label: "Home", launcherLabel: "HOME" }], maxHistoryDepth: 2,
      chartOrPlateReturnPages: new Set(["map", "charts"]), defaultChartOrPlateReturnPage: "map",
    }}><ChartsPage {...props} /></NavigationPageOptionsContext.Provider>;
  }
  const projection = () => host.querySelector('[data-testid^="parity:plate-viewport:"]')?.getAttribute("data-testid");
  async function load(width: number, height: number) {
    const image = host.querySelector<HTMLImageElement>(".chartImage")!;
    Object.defineProperties(image, { naturalWidth: { value: width }, naturalHeight: { value: height } });
    await act(async () => { image.dispatchEvent(new Event("load")); });
  }
  async function wheel() {
    await act(async () => { host.querySelector('[data-testid="plate-surface"]')!
      .dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaY: -360, clientX: 500, clientY: 450 })); });
  }
  try {
    await act(async () => root.render(<Viewer chartId="first" />));
    expect(projection(), "an unloaded image must not authorize gestures").toBeUndefined();
    await load(800, 1200);
    expect(projection()).toContain("chart:first:zoom:1.000");
    await wheel();
    expect(projection()).toContain("chart:first:zoom:2.000");

    await act(async () => root.render(<Viewer chartId="second" />));
    expect(projection(), "old geometry must not masquerade as the new image").toBeUndefined();
    await load(1800, 3000);
    const zoomBefore = Number(projection()!.match(/:zoom:([\d.]+)/)![1]);
    await wheel();
    expect(projection()).toContain(`chart:second:zoom:${(zoomBefore + 1).toFixed(3)}`);

    await act(async () => root.render(null));
    await act(async () => root.render(<Viewer chartId="second" />));
    expect(projection(), "restored view state still needs the remounted image").toBeUndefined();
    await load(1800, 3000);
    await wheel();
    expect(projection()).toContain("chart:second:zoom:2.000");
    expect(viewportChanges).toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
