import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { chartPage, mapViews, samplePlan } from "./domain/sampleData";
import {
  applyPinchGesture,
  createInitialViewport,
  createPinchSnapshot,
  dragViewport,
  preserveViewportForMap,
  renderTiles,
  viewportCenterLatLon,
  zoomAroundPoint,
  type MapViewportState,
  type ScreenPoint,
} from "./domain/mapViewport";
import {
  clampImageZoom,
  createInitialImageViewport,
  dragImageViewport,
  zoomImageAroundPoint,
  type ImageViewportState,
} from "./domain/imageViewport";

type SurfaceSize = {
  width: number;
  height: number;
};

type AppPage = "map" | "plan" | "charts";

type ChartFamilyId = "sectional" | "tac" | "ifr_low" | "ifr_high";

type ChartAsset = NonNullable<(typeof chartPage)["airports"][number]>["charts"][number];

const chartFamilies: Array<{ id: ChartFamilyId; label: string; launcherLabel: string }> = [
  { id: "sectional", label: "SECTIONAL", launcherLabel: "SEC" },
  { id: "tac", label: "TAC", launcherLabel: "TAC" },
  { id: "ifr_low", label: "IFR-LOW", launcherLabel: "IFR L" },
  { id: "ifr_high", label: "IFR-HIGH", launcherLabel: "IFR H" },
];

const waypointActions = ["Remove", "Insert", "Reorder", "Waypoint Info", "Add Airway", "Select Procedure", "Charts"];

export default function App() {
  const locationSearch = typeof window !== "undefined" ? window.location.search : "";
  const debugTileLabels = new URLSearchParams(locationSearch).has("debugTiles");
  const [page, setPage] = useState<AppPage>("map");
  const [selectedMapId, setSelectedMapId] = useState<string>(mapViews[0].id);
  const [selectedAirportId, setSelectedAirportId] = useState(chartPage.initial_airport_id || chartPage.airports[0]?.id || "");
  const [selectedChartId, setSelectedChartId] = useState(chartPage.initial_chart_id || chartPage.airports[0]?.charts[0]?.id || "");

  const selectedMap = useMemo(
    () => mapViews.find((view) => view.id === selectedMapId) ?? mapViews[0],
    [selectedMapId],
  );
  const [mapViewport, setMapViewport] = useState<MapViewportState>(() => createInitialViewport(selectedMap.map_view));
  const selectedFamily = useMemo(
    () => chartFamilies.find((family) => family.id === selectedMap.map_view.chart_family) ?? chartFamilies[0],
    [selectedMap],
  );
  const availableFamilies = useMemo(
    () => new Set(mapViews.map((view) => view.map_view.chart_family)),
    [],
  );
  const selectedFamilyMapViews = useMemo(
    () => mapViews.filter((view) => view.map_view.chart_family === selectedMap.map_view.chart_family),
    [selectedMap],
  );
  const selectedAirport = useMemo(
    () => chartPage.airports.find((airport) => airport.id === selectedAirportId) ?? chartPage.airports[0] ?? null,
    [selectedAirportId],
  );
  const selectedChart = useMemo(
    () => selectedAirport?.charts.find((chart) => chart.id === selectedChartId) ?? selectedAirport?.charts[0] ?? null,
    [selectedAirport, selectedChartId],
  );
  const legSummary = useMemo(() => {
    const firstLeg = samplePlan.legs[0];
    if (!firstLeg) {
      return "NO LEG";
    }
    const from = navRefLabel(firstLeg.from);
    const to = navRefLabel(firstLeg.to);
    return `${from} -> ${to} CRS 342`;
  }, []);

  useEffect(() => {
    if (!selectedAirport && chartPage.airports[0]) {
      setSelectedAirportId(chartPage.airports[0].id);
      setSelectedChartId(chartPage.airports[0].charts[0]?.id ?? "");
    }
  }, [selectedAirport]);

  useEffect(() => {
    setMapViewport((current) => preserveViewportForMap(current, selectedMap.map_view));
  }, [selectedMap]);

  return (
    <main className="appShell">
      {page === "map" ? (
        <MapPage
          debugTileLabels={debugTileLabels}
          selectedMapId={selectedMapId}
          selectedMap={selectedMap}
          selectedFamilyMapViews={selectedFamilyMapViews}
          selectedFamily={selectedFamily}
          availableFamilies={availableFamilies}
          viewport={mapViewport}
          onViewportChange={setMapViewport}
          onSelectMapId={setSelectedMapId}
          onOpenPlan={() => setPage("plan")}
          legSummary={legSummary}
          locationSearch={locationSearch}
        />
      ) : null}

      {page === "plan" ? (
        <FlightPlanPage
          legSummary={legSummary}
          onBack={() => setPage("map")}
          onOpenCharts={() => setPage("charts")}
        />
      ) : null}

      {page === "charts" ? (
        <ChartsPage
          selectedAirport={selectedAirport}
          selectedChart={selectedChart}
          onBack={() => setPage("plan")}
          onSelectAirport={(airportId) => {
            setSelectedAirportId(airportId);
            const airport = chartPage.airports.find((entry) => entry.id === airportId);
            setSelectedChartId(airport?.charts[0]?.id ?? "");
          }}
          onSelectChart={setSelectedChartId}
        />
      ) : null}
    </main>
  );
}

function MapPage(props: {
  debugTileLabels: boolean;
  selectedMapId: string;
  selectedMap: (typeof mapViews)[number];
  selectedFamilyMapViews: (typeof mapViews);
  selectedFamily: (typeof chartFamilies)[number];
  availableFamilies: Set<string>;
  viewport: MapViewportState;
  onViewportChange: (next: MapViewportState) => void;
  onSelectMapId: (mapId: string) => void;
  onOpenPlan: () => void;
  legSummary: string;
  locationSearch: string;
}) {
  const {
    debugTileLabels,
    selectedMap,
    selectedFamilyMapViews,
    selectedFamily,
    availableFamilies,
    viewport,
    onViewportChange,
    onSelectMapId,
    onOpenPlan,
    legSummary,
    locationSearch,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [familyTrayOpen, setFamilyTrayOpen] = useState(false);
  const viewportRef = useRef<MapViewportState>(viewport);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });

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
    return renderTiles(selectedFamilyMapViews.map((view) => ({ ...view.map_view, id: view.id })), viewport, surfaceSize.width, surfaceSize.height);
  }, [selectedFamilyMapViews, surfaceSize, viewport]);

  function updateViewport(next: MapViewportState) {
    viewportRef.current = next;
    onViewportChange(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (familyTrayOpen) {
      return;
    }
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
    if (familyTrayOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
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
    if (familyTrayOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    updateViewport(
      zoomAroundPoint(
        viewportRef.current,
        selectedMap.map_view,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        viewportRef.current.zoom - event.deltaY / 360,
      ),
    );
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (familyTrayOpen || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    updateViewport(
      zoomAroundPoint(
        viewportRef.current,
        selectedMap.map_view,
        { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
        surfaceSize.width,
        surfaceSize.height,
        viewportRef.current.zoom + 0.75,
      ),
    );
  }

  return (
    <section className="pageSurface">
      <div
        ref={containerRef}
        className="mapSurface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        <div className="mapBackdrop" />
        {familyTrayOpen ? (
          <button
            type="button"
            className="trayScrim"
            aria-label="Close chart tray"
            onClick={() => setFamilyTrayOpen(false)}
          />
        ) : null}
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

        <div className="chartDock" onPointerDown={(event) => event.stopPropagation()} onPointerUp={(event) => event.stopPropagation()}>
          <button
            type="button"
            className={`chartButton${familyTrayOpen ? " isOpen" : ""}`}
            onClick={() => setFamilyTrayOpen((open) => !open)}
          >
            <span className="chartButtonLabel">{selectedFamily.launcherLabel}</span>
          </button>
          <section className={`chartTray${familyTrayOpen ? " isOpen" : ""}`} aria-label="Chart family">
            {chartFamilies.map((family) => {
              const available = availableFamilies.has(family.id);
              const active = selectedMap.map_view.chart_family === family.id;
              return (
                <button
                  key={family.id}
                  type="button"
                  className={`trayButton${active ? " isActive" : ""}`}
                  disabled={!available}
                  onClick={() => {
                    const nextMap = mapViews.find((view) => view.map_view.chart_family === family.id);
                    if (nextMap) {
                      onSelectMapId(nextMap.id);
                    }
                    setFamilyTrayOpen(false);
                  }}
                >
                  {family.label}
                </button>
              );
            })}
          </section>
        </div>

        <button
          type="button"
          className="navElement"
          onPointerDown={(event) => event.stopPropagation()}
          onPointerUp={(event) => event.stopPropagation()}
          onClick={onOpenPlan}
        >
          <span className="navElementTop">{legSummary}</span>
          <span className="navElementBottom">° ° ^| ° °</span>
        </button>

        {debugTileLabels ? (
          <div className="debugCorner">search={locationSearch || "(empty)"} debugTiles=on {center.lat.toFixed(3)}/{center.lon.toFixed(3)} z{viewport.zoom.toFixed(2)}</div>
        ) : null}
      </div>
    </section>
  );
}

function FlightPlanPage(props: { legSummary: string; onBack: () => void; onOpenCharts: () => void }) {
  const [selectedWaypointIndex, setSelectedWaypointIndex] = useState<number | null>(null);
  const rows = useMemo(
    () =>
      samplePlan.legs.map((leg, index) => ({
        id: `${index}:${navRefLabel(leg.from)}-${navRefLabel(leg.to)}`,
        waypoint: navRefLabel(leg.to),
        distance: index === 0 ? "18.4" : "11.2",
        ete: index === 0 ? "0:07" : "0:04",
        course: index === 0 ? "342" : "161",
      })),
    [],
  );

  return (
    <section className="appPage planPage">
      <div className="pageChrome">
        <button type="button" className="toolbarButton" onClick={props.onBack}>
          MAP
        </button>
      </div>

      <div className="planTable">
        <div className="planHeader planWaypointCell">Waypoint</div>
        <div className="planHeader">Dist (nm)</div>
        <div className="planHeader">ETE (h:m)</div>
        <div className="planHeader">Course (°)</div>
        {rows.map((row, index) => (
          <Fragment key={row.id}>
            <button key={`${row.id}:waypoint`} type="button" className="planWaypointCell planWaypointButton" onClick={() => setSelectedWaypointIndex(index)}>
              {row.waypoint}
            </button>
            <div className="planCell">
              {row.distance}
            </div>
            <div className="planCell">
              {row.ete}
            </div>
            <div className="planCell">
              {row.course}
            </div>
          </Fragment>
        ))}
      </div>

      <div className="planFooter">{props.legSummary}</div>

      {selectedWaypointIndex !== null ? (
        <>
          <button type="button" className="trayScrim" aria-label="Close waypoint actions" onClick={() => setSelectedWaypointIndex(null)} />
          <section className="waypointModal" aria-label="Waypoint actions">
            {waypointActions.map((action) => (
              <button
                key={action}
                type="button"
                className="trayButton"
                onClick={() => {
                  if (action === "Charts") {
                    props.onOpenCharts();
                  }
                  setSelectedWaypointIndex(null);
                }}
              >
                {action}
              </button>
            ))}
          </section>
        </>
      ) : null}
    </section>
  );
}

function ChartsPage(props: {
  selectedAirport: (typeof chartPage)["airports"][number] | null;
  selectedChart: ChartAsset | null;
  onBack: () => void;
  onSelectAirport: (airportId: string) => void;
  onSelectChart: (chartId: string) => void;
}) {
  const { selectedAirport, selectedChart, onBack, onSelectAirport, onSelectChart } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [imageSize, setImageSize] = useState<{ width: number; height: number } | null>(null);
  const [viewport, setViewport] = useState<ImageViewportState | null>(null);
  const viewportRef = useRef<ImageViewportState | null>(null);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<{ zoom: number; distance: number; midpoint: ScreenPoint } | null>(null);
  const [airportTrayOpen, setAirportTrayOpen] = useState(false);
  const [chartTrayOpen, setChartTrayOpen] = useState(false);

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

  useEffect(() => {
    if (!imageSize || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const next = createInitialImageViewport(imageSize.width, imageSize.height, surfaceSize.width, surfaceSize.height);
    viewportRef.current = next;
    setViewport(next);
  }, [imageSize, selectedChart?.id, surfaceSize.width, surfaceSize.height]);

  const trayOpen = airportTrayOpen || chartTrayOpen;
  const overscrollPx = 64;

  function updateViewport(next: ImageViewportState) {
    viewportRef.current = next;
    setViewport(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !imageSize || trayOpen) {
      return;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    activePointersRef.current.set(event.pointerId, point);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2) {
      const [first, second] = Array.from(activePointersRef.current.values());
      pinchRef.current = {
        zoom: viewportRef.current.zoom,
        distance: distanceBetween(first, second),
        midpoint: midpoint(first, second),
      };
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !imageSize || trayOpen) {
      return;
    }
    const point = { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY };
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    const pointers = Array.from(activePointersRef.current.values());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      updateViewport(
        dragImageViewport(
          viewportRef.current,
          dx,
          dy,
          imageSize.width,
          imageSize.height,
          surfaceSize.width,
          surfaceSize.height,
          overscrollPx,
        ),
      );
      dragRef.current = { id: event.pointerId, last: point };
      return;
    }
    if (pointers.length >= 2 && pinchRef.current) {
      const [first, second] = pointers;
      const nextDistance = distanceBetween(first, second);
      const nextMidpoint = midpoint(first, second);
      const zoomDelta = pinchRef.current.distance > 0 ? Math.log2(nextDistance / pinchRef.current.distance) : 0;
      let next = zoomImageAroundPoint(
        viewportRef.current,
        pinchRef.current.midpoint.x,
        pinchRef.current.midpoint.y,
        clampImageZoom(pinchRef.current.zoom + zoomDelta),
        imageSize.width,
        imageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      next = dragImageViewport(
        next,
        nextMidpoint.x - pinchRef.current.midpoint.x,
        nextMidpoint.y - pinchRef.current.midpoint.y,
        imageSize.width,
        imageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      updateViewport(next);
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
    if (!viewportRef.current || !imageSize || trayOpen) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    updateViewport(
      zoomImageAroundPoint(
        viewportRef.current,
        event.nativeEvent.offsetX,
        event.nativeEvent.offsetY,
        viewportRef.current.zoom - event.deltaY / 360,
        imageSize.width,
        imageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      ),
    );
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (!viewportRef.current || !imageSize || trayOpen) {
      return;
    }
    updateViewport(
      zoomImageAroundPoint(
        viewportRef.current,
        event.nativeEvent.offsetX,
        event.nativeEvent.offsetY,
        viewportRef.current.zoom + 0.75,
        imageSize.width,
        imageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      ),
    );
  }

  return (
    <section className="pageSurface">
      <div
        ref={containerRef}
        className="mapSurface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        <div className="mapBackdrop" />
        {trayOpen ? (
          <button
            type="button"
            className="trayScrim"
            aria-label="Close chart tray"
            onClick={() => {
              setAirportTrayOpen(false);
              setChartTrayOpen(false);
            }}
          />
        ) : null}

        {selectedChart && viewport ? (
          <img
            className="chartImage"
            src={selectedChart.asset_url}
            alt={selectedChart.label}
            draggable={false}
            onLoad={(event) =>
              setImageSize({
                width: event.currentTarget.naturalWidth,
                height: event.currentTarget.naturalHeight,
              })
            }
            style={{
              left: `${viewport.left}px`,
              top: `${viewport.top}px`,
              transform: `scale(${viewport.zoom})`,
            }}
          />
        ) : null}

        <div className="chartDock chartDockDouble">
          <div className="chartDockColumn">
            <button
              type="button"
              className={`chartButton${airportTrayOpen ? " isOpen" : ""}`}
              onClick={() => {
                setAirportTrayOpen((open) => !open);
                setChartTrayOpen(false);
              }}
            >
              <span className="chartButtonLabel">{selectedAirport?.label ?? "---"}</span>
            </button>
            <section className={`chartTray${airportTrayOpen ? " isOpen" : ""}`}>
              {chartPage.airports.map((airport) => (
                <button
                  key={airport.id}
                  type="button"
                  className={`trayButton${airport.id === selectedAirport?.id ? " isActive" : ""}`}
                  onClick={() => {
                    onSelectAirport(airport.id);
                    setAirportTrayOpen(false);
                  }}
                >
                  {airport.label}
                </button>
              ))}
            </section>
          </div>

          <div className="chartDockColumn">
            <button
              type="button"
              className={`chartButton chartButtonWide${chartTrayOpen ? " isOpen" : ""}`}
              onClick={() => {
                setChartTrayOpen((open) => !open);
                setAirportTrayOpen(false);
              }}
            >
              <span className="chartButtonLabel chartButtonLabelWide">{selectedChart?.label ?? "---"}</span>
            </button>
            <section className={`chartTray${chartTrayOpen ? " isOpen" : ""}`}>
              {selectedAirport?.charts.map((chart) => (
                <button
                  key={chart.id}
                  type="button"
                  className={`trayButton${chart.id === selectedChart?.id ? " isActive" : ""}`}
                  onClick={() => {
                    onSelectChart(chart.id);
                    setChartTrayOpen(false);
                  }}
                >
                  {chart.label}
                </button>
              ))}
            </section>
          </div>
        </div>

        <button type="button" className="toolbarButton toolbarButtonTopRight" onClick={onBack}>
          PLAN
        </button>
      </div>
    </section>
  );
}

function navRefLabel(value: { Airport: string } | { Navaid: string } | { Fix: string }) {
  if ("Airport" in value) return value.Airport;
  if ("Navaid" in value) return value.Navaid;
  return value.Fix;
}

function distanceBetween(first: ScreenPoint, second: ScreenPoint) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function midpoint(first: ScreenPoint, second: ScreenPoint): ScreenPoint {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}
