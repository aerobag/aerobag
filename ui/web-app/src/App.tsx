import { useEffect, useMemo, useRef, useState } from "react";
import { mapViews } from "./domain/sampleData";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  dragViewport,
  renderTiles,
  viewportCenterLatLon,
  zoomAroundPoint,
  type MapViewportState,
  type ScreenPoint,
} from "./domain/mapViewport";

type SurfaceSize = {
  width: number;
  height: number;
};

export default function App() {
  const locationSearch = typeof window !== "undefined" ? window.location.search : "";
  const debugTileLabels = new URLSearchParams(locationSearch).has("debugTiles");
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [selectedMapId, setSelectedMapId] = useState<string>(mapViews[0].id);
  const selectedMap = useMemo(
    () => mapViews.find((view) => view.id === selectedMapId) ?? mapViews[0],
    [selectedMapId],
  );
  const viewportRef = useRef<MapViewportState>(createInitialViewport(selectedMap.map_view));
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [viewport, setViewport] = useState<MapViewportState>(() => createInitialViewport(selectedMap.map_view));

  useEffect(() => {
    viewportRef.current = viewport;
  }, [viewport]);

  useEffect(() => {
    const nextViewport = createInitialViewport(selectedMap.map_view);
    viewportRef.current = nextViewport;
    setViewport(nextViewport);
    activePointersRef.current.clear();
    dragRef.current = null;
    pinchRef.current = null;
  }, [selectedMap]);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      setSurfaceSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const center = useMemo(() => viewportCenterLatLon(viewport), [viewport]);
  const tiles = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return renderTiles(selectedMap.map_view, viewport, surfaceSize.width, surfaceSize.height);
  }, [selectedMap, surfaceSize, viewport]);

  function updateViewport(next: MapViewportState) {
    viewportRef.current = next;
    setViewport(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    activePointersRef.current.set(event.pointerId, point);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2 && surfaceSize.width > 0 && surfaceSize.height > 0) {
      const [first, second] = Array.from(activePointersRef.current.values());
          pinchRef.current = createPinchSnapshot(
            viewportRef.current,
            first,
            second,
            surfaceSize.width,
        surfaceSize.height,
      );
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    const pointers = Array.from(activePointersRef.current.entries());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      updateViewport(dragViewport(viewportRef.current, dx, dy));
      dragRef.current = { id: event.pointerId, last: point };
      return;
    }
    if (pointers.length >= 2) {
      const [first, second] = pointers;
      if (!pinchRef.current) {
          pinchRef.current = createPinchSnapshot(
            viewportRef.current,
            first[1],
            second[1],
            surfaceSize.width,
          surfaceSize.height,
        );
      }
      updateViewport(
        applyPinchGesture(
          pinchRef.current,
          first[1],
          second[1],
          selectedMap.map_view,
          surfaceSize.width,
          surfaceSize.height,
        ),
      );
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    event.preventDefault();
    const nextZoom = viewport.zoom - event.deltaY / 360;
    updateViewport(
      zoomAroundPoint(
        viewport,
        selectedMap.map_view,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        nextZoom,
      ),
    );
  }

  return (
    <main className="mapShell">
      <div
        ref={containerRef}
        className="mapSurface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
      >
        <div className="mapBackdrop" />
        {tiles.map((tile) => (
          <div
            key={`${tile.zoom}-${tile.x}-${tile.yTms}`}
            className="mapTile"
            style={{
              left: `${tile.left}px`,
              top: `${tile.top}px`,
              width: `${tile.size}px`,
              height: `${tile.size}px`,
            }}
          >
            <img className="mapTileImage" src={tile.src} alt="" draggable={false} />
            {debugTileLabels ? (
              <div className="tileLabel">
                z{tile.zoom} x{tile.x} y{tile.yTms}
              </div>
            ) : null}
          </div>
        ))}

        <section className="hud hudTop">
          <p className="eyebrow">Aerobag</p>
          <h1>{selectedMap.map_view.chart_name}</h1>
          <p className="lede">
            Drag to pan. Wheel or pinch to zoom. Web streams the unpacked sectional tiles on demand; Android installs the package locally.
          </p>
          <p className="debugFlag">
            search={locationSearch || "(empty)"} debugTiles={debugTileLabels ? "on" : "off"}
          </p>
          <div className="selectorRow" role="tablist" aria-label="Chart package">
            {mapViews.map((mapOption) => (
              <button
                key={mapOption.id}
                type="button"
                className={`selectorButton${mapOption.id === selectedMap.id ? " isActive" : ""}`}
                onPointerDown={(event) => event.stopPropagation()}
                onPointerUp={(event) => event.stopPropagation()}
                onClick={() => setSelectedMapId(mapOption.id)}
              >
                {mapOption.label}
              </button>
            ))}
          </div>
        </section>

        <section className="hud hudBottom">
          <dl className="facts">
            <div>
              <dt>Latitude</dt>
              <dd>{center.lat.toFixed(4)}</dd>
            </div>
            <div>
              <dt>Longitude</dt>
              <dd>{center.lon.toFixed(4)}</dd>
            </div>
            <div>
              <dt>Zoom</dt>
              <dd>{viewport.zoom.toFixed(2)}</dd>
            </div>
          </dl>
        </section>
      </div>
    </main>
  );
}
