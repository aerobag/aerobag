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
  imageDisplaySize,
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
type TrayOption = {
  id: string;
  label: string;
  active?: boolean;
  disabled?: boolean;
  onSelect: () => void;
};

const chartFamilies: Array<{ id: ChartFamilyId; label: string; launcherLabel: string }> = [
  { id: "sectional", label: "SECTIONAL", launcherLabel: "SEC" },
  { id: "tac", label: "TAC", launcherLabel: "TAC" },
  { id: "ifr_low", label: "IFR-LOW", launcherLabel: "IFR L" },
  { id: "ifr_high", label: "IFR-HIGH", launcherLabel: "IFR H" },
];

const pageOptions: Array<{ id: AppPage; label: string; launcherLabel: string }> = [
  { id: "map", label: "CHART", launcherLabel: "CHT" },
  { id: "charts", label: "PLATE", launcherLabel: "PLT" },
  { id: "plan", label: "PLAN", launcherLabel: "PLN" },
];

const waypointActions = ["Remove", "Insert", "Reorder", "Waypoint Info", "Add Airway", "Select Procedure", "Charts"];
const webUiStateStorageKey = "aerobag.web.uiState.v1";

type PersistedWebUiState = {
  page?: AppPage;
  selectedAirportId?: string;
  selectedChartId?: string;
  recentAirportIds?: string[];
};

export default function App() {
  const locationSearch = typeof window !== "undefined" ? window.location.search : "";
  const debugTileLabels = new URLSearchParams(locationSearch).has("debugTiles");
  const persistedUiState = useMemo(readPersistedWebUiState, []);
  const [page, setPage] = useState<AppPage>(persistedUiState.page ?? "map");
  const [selectedMapId, setSelectedMapId] = useState<string>(mapViews[0].id);
  const initialRecentAirportIds = useMemo(
    () => mergeRecentAirportIds(chartPage.airports, persistedUiState.recentAirportIds ?? []),
    [persistedUiState],
  );
  const [recentAirportIds, setRecentAirportIds] = useState<string[]>(initialRecentAirportIds);
  const [selectedAirportId, setSelectedAirportId] = useState(
    () => resolveAirportId(chartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
  );
  const [selectedChartId, setSelectedChartId] = useState(
    () =>
      resolveChartId(
        chartPage.airports,
        resolveAirportId(chartPage.airports, persistedUiState.selectedAirportId, initialRecentAirportIds),
        persistedUiState.selectedChartId,
      ),
  );

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
  const orderedChartAirports = useMemo(
    () => orderAirportsByRecency(chartPage.airports, recentAirportIds),
    [recentAirportIds],
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
    const normalizedRecentAirportIds = mergeRecentAirportIds(chartPage.airports, recentAirportIds);
    if (!sameIds(normalizedRecentAirportIds, recentAirportIds)) {
      setRecentAirportIds(normalizedRecentAirportIds);
      return;
    }
    const normalizedAirportId = resolveAirportId(chartPage.airports, selectedAirportId, normalizedRecentAirportIds);
    if (normalizedAirportId !== selectedAirportId) {
      setSelectedAirportId(normalizedAirportId);
      return;
    }
    const normalizedChartId = resolveChartId(chartPage.airports, normalizedAirportId, selectedChartId);
    if (normalizedChartId !== selectedChartId) {
      setSelectedChartId(normalizedChartId);
    }
  }, [recentAirportIds, selectedAirportId, selectedChartId]);

  useEffect(() => {
    setMapViewport((current) => preserveViewportForMap(current, selectedMap.map_view));
  }, [selectedMap]);

  useEffect(() => {
    writePersistedWebUiState({
      page,
      selectedAirportId,
      selectedChartId,
      recentAirportIds,
    });
  }, [page, recentAirportIds, selectedAirportId, selectedChartId]);

  return (
    <main className="appShell">
      {page === "map" ? (
        <MapPage
          page={page}
          debugTileLabels={debugTileLabels}
          selectedMapId={selectedMapId}
          selectedMap={selectedMap}
          selectedFamilyMapViews={selectedFamilyMapViews}
          selectedFamily={selectedFamily}
          availableFamilies={availableFamilies}
          viewport={mapViewport}
          onViewportChange={setMapViewport}
          onSelectMapId={setSelectedMapId}
          onSelectPage={setPage}
          onOpenPlan={() => setPage("plan")}
          legSummary={legSummary}
          locationSearch={locationSearch}
        />
      ) : null}

      {page === "plan" ? (
        <FlightPlanPage
          page={page}
          legSummary={legSummary}
          onSelectPage={setPage}
          onOpenCharts={() => setPage("charts")}
        />
      ) : null}

      {page === "charts" ? (
        <ChartsPage
          page={page}
          airports={orderedChartAirports}
          selectedAirport={selectedAirport}
          selectedChart={selectedChart}
          onSelectPage={setPage}
          onSelectAirport={(airportId) => {
            setSelectedAirportId(airportId);
            setRecentAirportIds((current) => moveAirportToFront(current, airportId, chartPage.airports));
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
  page: AppPage;
  debugTileLabels: boolean;
  selectedMapId: string;
  selectedMap: (typeof mapViews)[number];
  selectedFamilyMapViews: (typeof mapViews);
  selectedFamily: (typeof chartFamilies)[number];
  availableFamilies: Set<string>;
  viewport: MapViewportState;
  onViewportChange: (next: MapViewportState) => void;
  onSelectMapId: (mapId: string) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  legSummary: string;
  locationSearch: string;
}) {
  const {
    debugTileLabels,
    page,
    selectedMap,
    selectedFamilyMapViews,
    selectedFamily,
    availableFamilies,
    viewport,
    onViewportChange,
    onSelectMapId,
    onSelectPage,
    onOpenPlan,
    legSummary,
    locationSearch,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [familyTrayOpen, setFamilyTrayOpen] = useState(false);
  const [pageTrayOpen, setPageTrayOpen] = useState(false);
  const [debugOpen, setDebugOpen] = useState(false);
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
  const debugSummary = useMemo(() => {
    const tileZooms = [...new Set(tiles.map((tile) => tile.zoom))].sort((a, b) => a - b);
    const packages = [...new Set(tiles.map((tile) => tile.packageName).filter((value): value is string => Boolean(value)))].sort();
    const mapIds = selectedFamilyMapViews.map((view) => view.id);
    return {
      tileZooms,
      packages,
      mapIds,
      tileCount: tiles.length,
    };
  }, [selectedFamilyMapViews, tiles]);

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

        <div className="chartDock">
          <TrayDock
            launcherLabel={pageOptions.find((option) => option.id === page)?.launcherLabel ?? "CHT"}
            open={pageTrayOpen}
            onToggle={() =>
              toggleModalTray(
                "page",
                { page: pageTrayOpen, family: familyTrayOpen },
                { setPage: setPageTrayOpen, setFamily: setFamilyTrayOpen },
              )
            }
            ariaLabel="Page"
            options={pageOptions.map((option) => ({
              id: option.id,
              label: option.label,
              active: option.id === page,
              onSelect: () => {
                onSelectPage(option.id);
                setPageTrayOpen(false);
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedFamily.launcherLabel}
            open={familyTrayOpen}
            onToggle={() =>
              toggleModalTray(
                "family",
                { page: pageTrayOpen, family: familyTrayOpen },
                { setPage: setPageTrayOpen, setFamily: setFamilyTrayOpen },
              )
            }
            ariaLabel="Chart family"
            options={chartFamilies.map((family) => {
              const available = availableFamilies.has(family.id);
              const active = selectedMap.map_view.chart_family === family.id;
              return {
                id: family.id,
                label: family.label,
                active,
                disabled: !available,
                onSelect: () => {
                  const nextMap = mapViews.find((view) => view.map_view.chart_family === family.id);
                  if (nextMap) {
                    onSelectMapId(nextMap.id);
                  }
                  setFamilyTrayOpen(false);
                },
              };
            })}
          />
        </div>

        <button
          type="button"
          className="navElement"
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
          onDoubleClick={stopDoubleClick}
          onClick={onOpenPlan}
        >
          <span className="navElementTop">{legSummary}</span>
          <span className="navElementBottom">° ° ^| ° °</span>
        </button>

        <div className="debugDock">
          <button
            type="button"
            className="debugLauncher"
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onClick={() => setDebugOpen((open) => !open)}
            aria-expanded={debugOpen}
            aria-label="Toggle debug details"
          >
            DBG
          </button>
          <section
            className={`debugPanel${debugOpen ? " isOpen" : ""}`}
            aria-label="Debug metadata"
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
          >
            <div className="debugLine">family {selectedFamily.launcherLabel}</div>
            <div className="debugLine">{center.lat.toFixed(3)}/{center.lon.toFixed(3)} z{viewport.zoom.toFixed(2)}</div>
            <div className="debugLine">tiles {debugSummary.tileCount}</div>
            <div className="debugLine">src z {debugSummary.tileZooms.length > 0 ? debugSummary.tileZooms.join(", ") : "(none)"}</div>
            <div className="debugLine">pkg {debugSummary.packages.length > 0 ? debugSummary.packages.join(", ") : "(none)"}</div>
            <div className="debugLine">maps {debugSummary.mapIds.join(", ")}</div>
            <div className="debugLine">search {locationSearch || "(empty)"}</div>
            <div className="debugLine">{debugTileLabels ? "debugTiles=on" : "debugTiles=off"}</div>
          </section>
        </div>
      </div>
    </section>
  );
}

function FlightPlanPage(props: { page: AppPage; legSummary: string; onSelectPage: (page: AppPage) => void; onOpenCharts: () => void }) {
  const [selectedWaypointIndex, setSelectedWaypointIndex] = useState<number | null>(null);
  const [pageTrayOpen, setPageTrayOpen] = useState(false);
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
        <div className="chartDock">
          <TrayDock
            launcherLabel={pageOptions.find((option) => option.id === props.page)?.launcherLabel ?? "PLN"}
            open={pageTrayOpen}
            onToggle={() => setPageTrayOpen((open) => !open)}
            ariaLabel="Page"
            options={pageOptions.map((option) => ({
              id: option.id,
              label: option.label,
              active: option.id === props.page,
              onSelect: () => {
                props.onSelectPage(option.id);
                setPageTrayOpen(false);
              },
            }))}
          />
        </div>
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
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
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

function TrayDock(props: {
  launcherLabel: string;
  open: boolean;
  onToggle: () => void;
  ariaLabel: string;
  wide?: boolean;
  options: TrayOption[];
}) {
  const { launcherLabel, open, onToggle, ariaLabel, wide, options } = props;
  return (
    <div className="chartDockColumn">
      <button
        type="button"
        className={`chartButton${wide ? " chartButtonWide" : ""}${open ? " isOpen" : ""}`}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={onToggle}
      >
        <span className={`chartButtonLabel${wide ? " chartButtonLabelWide" : ""}`}>{launcherLabel}</span>
      </button>
      <section className={`chartTray${wide ? " chartTrayWide" : ""}${open ? " isOpen" : ""}`} aria-label={ariaLabel} onPointerDown={stopPointer} onPointerUp={stopPointer}>
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`trayButton${option.active ? " isActive" : ""}`}
            disabled={option.disabled}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={option.onSelect}
          >
            {option.label}
          </button>
        ))}
      </section>
    </div>
  );
}

function ChartsPage(props: {
  page: AppPage;
  airports: (typeof chartPage)["airports"];
  selectedAirport: (typeof chartPage)["airports"][number] | null;
  selectedChart: ChartAsset | null;
  onSelectPage: (page: AppPage) => void;
  onSelectAirport: (airportId: string) => void;
  onSelectChart: (chartId: string) => void;
}) {
  const { page, airports, selectedAirport, selectedChart, onSelectPage, onSelectAirport, onSelectChart } = props;
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
  const [pageTrayOpen, setPageTrayOpen] = useState(false);
  const displaySize = useMemo(() => {
    if (!imageSize || !viewport || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    return imageDisplaySize(
      imageSize.width,
      imageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      viewport.zoom,
    );
  }, [imageSize, surfaceSize.height, surfaceSize.width, viewport]);

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

  const trayOpen = pageTrayOpen || airportTrayOpen || chartTrayOpen;
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
              setPageTrayOpen(false);
              setAirportTrayOpen(false);
              setChartTrayOpen(false);
            }}
          />
        ) : null}

        {selectedChart ? (
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
              left: `${viewport?.left ?? 0}px`,
              top: `${viewport?.top ?? 0}px`,
              width: displaySize ? `${displaySize.width}px` : undefined,
              height: displaySize ? `${displaySize.height}px` : undefined,
              visibility: viewport ? "visible" : "hidden",
            }}
          />
        ) : null}

        <div className="chartDock chartDockDouble">
          <TrayDock
            launcherLabel={pageOptions.find((option) => option.id === page)?.launcherLabel ?? "PLT"}
            open={pageTrayOpen}
            onToggle={() =>
              toggleExclusiveTray(
                "page",
                { page: pageTrayOpen, airport: airportTrayOpen, chart: chartTrayOpen },
                { setPage: setPageTrayOpen, setAirport: setAirportTrayOpen, setChart: setChartTrayOpen },
              )
            }
            ariaLabel="Page"
            options={pageOptions.map((option) => ({
              id: option.id,
              label: option.label,
              active: option.id === page,
              onSelect: () => {
                onSelectPage(option.id);
                setPageTrayOpen(false);
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedAirport?.label ?? "---"}
            open={airportTrayOpen}
            onToggle={() =>
              toggleExclusiveTray(
                "airport",
                { page: pageTrayOpen, airport: airportTrayOpen, chart: chartTrayOpen },
                { setPage: setPageTrayOpen, setAirport: setAirportTrayOpen, setChart: setChartTrayOpen },
              )
            }
            ariaLabel="Airport"
            options={airports.map((airport) => ({
              id: airport.id,
              label: airport.label,
              active: airport.id === selectedAirport?.id,
              onSelect: () => {
                onSelectAirport(airport.id);
                setAirportTrayOpen(false);
              },
            }))}
          />
          <TrayDock
            launcherLabel={selectedChart?.label ?? "---"}
            open={chartTrayOpen}
            onToggle={() =>
              toggleExclusiveTray(
                "chart",
                { page: pageTrayOpen, airport: airportTrayOpen, chart: chartTrayOpen },
                { setPage: setPageTrayOpen, setAirport: setAirportTrayOpen, setChart: setChartTrayOpen },
              )
            }
            ariaLabel="Chart"
            wide
            options={(selectedAirport?.charts ?? []).map((chart) => ({
              id: chart.id,
              label: chart.label,
              active: chart.id === selectedChart?.id,
              onSelect: () => {
                onSelectChart(chart.id);
                setChartTrayOpen(false);
              },
            }))}
          />
        </div>

      </div>
    </section>
  );
}

function readPersistedWebUiState(): PersistedWebUiState {
  if (typeof window === "undefined") {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(webUiStateStorageKey);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as PersistedWebUiState;
    return {
      page: parsed.page,
      selectedAirportId: parsed.selectedAirportId,
      selectedChartId: parsed.selectedChartId,
      recentAirportIds: Array.isArray(parsed.recentAirportIds) ? parsed.recentAirportIds.filter((value): value is string => typeof value === "string") : [],
    };
  } catch {
    return {};
  }
}

function writePersistedWebUiState(state: PersistedWebUiState) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(webUiStateStorageKey, JSON.stringify(state));
}

function mergeRecentAirportIds(
  airports: (typeof chartPage)["airports"],
  storedIds: string[],
) {
  const validIds = new Set(airports.map((airport) => airport.id));
  const orderedIds = storedIds.filter((id, index) => validIds.has(id) && storedIds.indexOf(id) === index);
  for (const airport of airports) {
    if (!orderedIds.includes(airport.id)) {
      orderedIds.push(airport.id);
    }
  }
  return orderedIds;
}

function orderAirportsByRecency(
  airports: (typeof chartPage)["airports"],
  recentAirportIds: string[],
) {
  const airportById = new Map(airports.map((airport) => [airport.id, airport]));
  return recentAirportIds
    .map((airportId) => airportById.get(airportId))
    .filter((airport): airport is (typeof chartPage)["airports"][number] => airport !== undefined);
}

function moveAirportToFront(
  currentIds: string[],
  airportId: string,
  airports: (typeof chartPage)["airports"],
) {
  return mergeRecentAirportIds(airports, [airportId, ...currentIds.filter((id) => id !== airportId)]);
}

function resolveAirportId(
  airports: (typeof chartPage)["airports"],
  candidateAirportId: string | undefined,
  recentAirportIds: string[],
) {
  if (candidateAirportId && airports.some((airport) => airport.id === candidateAirportId)) {
    return candidateAirportId;
  }
  return recentAirportIds[0] ?? airports[0]?.id ?? "";
}

function resolveChartId(
  airports: (typeof chartPage)["airports"],
  airportId: string,
  candidateChartId: string | undefined,
) {
  const airport = airports.find((entry) => entry.id === airportId);
  if (candidateChartId && airport?.charts.some((chart) => chart.id === candidateChartId)) {
    return candidateChartId;
  }
  return airport?.charts[0]?.id ?? "";
}

function sameIds(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function navRefLabel(value: { Airport: string } | { Navaid: string } | { Fix: string }) {
  if ("Airport" in value) return value.Airport;
  if ("Navaid" in value) return value.Navaid;
  return value.Fix;
}

function distanceBetween(first: ScreenPoint, second: ScreenPoint) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function stopPointer(event: React.PointerEvent<HTMLElement>) {
  event.stopPropagation();
}

function stopDoubleClick(event: React.MouseEvent<HTMLElement>) {
  event.preventDefault();
  event.stopPropagation();
}

function toggleModalTray(
  target: "page" | "family",
  state: { page: boolean; family: boolean },
  actions: {
    setPage: React.Dispatch<React.SetStateAction<boolean>>;
    setFamily: React.Dispatch<React.SetStateAction<boolean>>;
  },
) {
  if (target === "page") {
    if (state.family) {
      actions.setFamily(false);
      return;
    }
    actions.setPage((open) => !open);
    return;
  }
  if (state.page) {
    actions.setPage(false);
    return;
  }
  actions.setFamily((open) => !open);
}

function toggleExclusiveTray(
  target: "page" | "airport" | "chart",
  state: { page: boolean; airport: boolean; chart: boolean },
  actions: {
    setPage: React.Dispatch<React.SetStateAction<boolean>>;
    setAirport: React.Dispatch<React.SetStateAction<boolean>>;
    setChart: React.Dispatch<React.SetStateAction<boolean>>;
  },
) {
  if (target === "page") {
    if (state.airport || state.chart) {
      actions.setAirport(false);
      actions.setChart(false);
      return;
    }
    actions.setPage((open) => !open);
    return;
  }
  if (target === "airport") {
    if (state.page || state.chart) {
      actions.setPage(false);
      actions.setChart(false);
      return;
    }
    actions.setAirport((open) => !open);
    return;
  }
  if (state.page || state.airport) {
    actions.setPage(false);
    actions.setAirport(false);
    return;
  }
  actions.setChart((open) => !open);
}

function midpoint(first: ScreenPoint, second: ScreenPoint): ScreenPoint {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}
