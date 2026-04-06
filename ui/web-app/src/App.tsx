import { useEffect, useMemo, useRef, useState } from "react";
import { mapView } from "./domain/sampleData";
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
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewportRef = useRef<MapViewportState>(createInitialViewport(mapView));
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [viewport, setViewport] = useState<MapViewportState>(() => createInitialViewport(mapView));

  useEffect(() => {
    viewportRef.current = viewport;
  }, [viewport]);

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
    return renderTiles(mapView, viewport, surfaceSize.width, surfaceSize.height);
  }, [surfaceSize, viewport]);

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
          mapView,
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
        mapView,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        nextZoom,
      ),
    );
  }

  function nudgeZoom(delta: number) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    updateViewport(
      zoomAroundPoint(
        viewport,
        mapView,
        { x: surfaceSize.width / 2, y: surfaceSize.height / 2 },
        surfaceSize.width,
        surfaceSize.height,
        viewport.zoom + delta,
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
          <img
            key={`${tile.zoom}-${tile.x}-${tile.yTms}`}
            className="mapTile"
            src={tile.src}
            alt=""
            draggable={false}
            style={{
              left: `${tile.left}px`,
              top: `${tile.top}px`,
              width: `${tile.size}px`,
              height: `${tile.size}px`,
            }}
          />
        ))}

        <section className="hud hudTop">
          <p className="eyebrow">Avare Web Prototype</p>
          <h1>{mapView.chart_name}</h1>
          <p className="lede">
            Drag to pan. Wheel or pinch to zoom. The surface is driven by lat, lon, and continuous zoom over the tiled chart.
          </p>
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

        <div className="zoomControls">
          <button type="button" onClick={() => nudgeZoom(0.35)}>+</button>
          <button type="button" onClick={() => nudgeZoom(-0.35)}>-</button>
        </div>
      </div>
    </main>
  );
}
