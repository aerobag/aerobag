import { useEffect, useState } from "react";
import { loadBestAvailableAdapter } from "./domain/appCoreAdapter";
import { ContentViewModel } from "./domain/contentViewModel";
import { initialProbe, installedInventory, mapTileView, remoteOnlyInventory, sampleCatalog, sampleGeometry, samplePlan } from "./domain/sampleData";
import type { AppState, ChartFamilyId, ContentPolicy } from "./domain/types";
import type { AppCoreAdapter } from "./domain/appCoreAdapter";

const policyOptions: Array<{ value: ContentPolicy; label: string }> = [
  { value: "StreamAllowed", label: "Stream Allowed" },
  { value: "PreferLocal", label: "Prefer Local" },
  { value: "OfflineRequired", label: "Offline Required" },
];

export default function App() {
  const [adapter, setAdapter] = useState<AppCoreAdapter | null>(null);
  const [model, setModel] = useState<ContentViewModel | null>(null);
  const [state, setState] = useState<AppState | null>(null);
  const [inventoryMode, setInventoryMode] = useState<"remote" | "installed">("remote");
  const [backendLabel, setBackendLabel] = useState("Loading adapter...");
  const [selectedFamily, setSelectedFamily] = useState<ChartFamilyId>(initialProbe.family);
  const [probeLat, setProbeLat] = useState(initialProbe.lat);
  const [probeLon, setProbeLon] = useState(initialProbe.lon);
  const [selectedChart, setSelectedChart] = useState<typeof sampleCatalog.charts[number] | null>(null);

  const offshoreProbe = {
    lat: initialProbe.lat + 4,
    lon: initialProbe.lon + 4,
  };

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      const loaded = await loadBestAvailableAdapter();
      const nextModel = new ContentViewModel(loaded.adapter);
      let next = await nextModel.loadPlan(samplePlan);
      next = await nextModel.setPolicy("StreamAllowed");
      next = await nextModel.refresh(remoteOnlyInventory);
      const nextChart = await loaded.adapter.chartForPosition(
        sampleCatalog,
        sampleGeometry,
        initialProbe.family,
        initialProbe.lat,
        initialProbe.lon,
      );
      if (!cancelled) {
        setAdapter(loaded.adapter);
        setModel(nextModel);
        setState(next);
        setSelectedChart(nextChart);
        setBackendLabel(`${loaded.backend.toUpperCase()}: ${loaded.detail}`);
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
    };
  }, []);

  async function handlePolicyChange(policy: ContentPolicy) {
    if (!model || !state) {
      return;
    }
    const next = await model.setPolicy(policy);
    setState(next);
  }

  async function handleInventoryChange(mode: "remote" | "installed") {
    if (!model || !state) {
      return;
    }
    setInventoryMode(mode);
    const next = await model.refresh(mode === "installed" ? installedInventory : remoteOnlyInventory);
    setState(next);
  }

  async function handleProbeSelection(nextFamily: ChartFamilyId, nextLat: number, nextLon: number) {
    if (!adapter) {
      return;
    }
    setSelectedFamily(nextFamily);
    setProbeLat(nextLat);
    setProbeLon(nextLon);
    const nextChart = await adapter.chartForPosition(sampleCatalog, sampleGeometry, nextFamily, nextLat, nextLon);
    setSelectedChart(nextChart);
  }

  if (!state) {
    return (
      <main className="shell">
        <section className="hero">
          <p className="eyebrow">Avare Web Prototype</p>
          <h1>Loading shared content model</h1>
          <p className="lede">{backendLabel}</p>
        </section>
      </main>
    );
  }

  return (
    <main className="shell">
      <section className="hero">
        <p className="eyebrow">Avare Web Prototype</p>
        <h1>Content parity without offline symmetry</h1>
        <p className="lede">
          The desktop browser streams content on demand while preserving the same planning and content
          semantics the Android app will use offline.
        </p>
        <p className="backend">{backendLabel}</p>
      </section>

      <section className="grid">
        <article className="panel">
          <h2>Prototype inputs</h2>
          <dl className="facts">
            <div>
              <dt>Cycle</dt>
              <dd>{sampleCatalog.cycle}</dd>
            </div>
            <div>
              <dt>Plan</dt>
              <dd>{samplePlan.name}</dd>
            </div>
            <div>
              <dt>Route</dt>
              <dd>{samplePlan.departure} to {samplePlan.destination}</dd>
            </div>
          </dl>

          <fieldset className="controls">
            <legend>Content policy</legend>
            {policyOptions.map((option) => (
              <label key={option.value} className="radio">
                <input
                  type="radio"
                  name="policy"
                  checked={state.content_policy === option.value}
                  onChange={() => void handlePolicyChange(option.value)}
                />
                <span>{option.label}</span>
              </label>
            ))}
          </fieldset>

          <fieldset className="controls">
            <legend>Inventory mode</legend>
            <label className="radio">
              <input
                type="radio"
                name="inventory"
                checked={inventoryMode === "remote"}
                onChange={() => void handleInventoryChange("remote")}
              />
              <span>Remote only cache</span>
            </label>
            <label className="radio">
              <input
                type="radio"
                name="inventory"
                checked={inventoryMode === "installed"}
                onChange={() => void handleInventoryChange("installed")}
              />
              <span>Installed package</span>
            </label>
          </fieldset>
        </article>

        <article className="panel">
          <h2>Shared-state result</h2>
          <div className={`status ${state.last_content_report?.fully_satisfied ? "ok" : "warn"}`}>
            <strong>
              {state.last_content_report?.fully_satisfied ? "Ready for this policy" : "Coverage gap"}
            </strong>
            <span>
              {state.last_content_report?.fully_satisfied
                ? "The current content strategy satisfies the plan."
                : "This plan needs local content before the policy is satisfied."}
            </span>
          </div>

          <ul className="report">
            {state.last_content_report?.items.map((item) => (
              <li key={item.label} className="reportItem">
                <div>
                  <h3>{item.label}</h3>
                  <p>{item.availability.availability}</p>
                </div>
                <div className="badges">
                  <span className={item.availability.offline_usable ? "badge solid" : "badge"}>
                    offline {item.availability.offline_usable ? "yes" : "no"}
                  </span>
                  <span className={item.availability.cached ? "badge solid" : "badge"}>
                    cached {item.availability.cached ? "yes" : "no"}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </article>

        <article className="panel">
          <h2>Map lookup</h2>
          <p className="lede">
            This is the first real map-layer seam: the shared Rust core picks a chart from catalog metadata and
            transformed cutline geometry.
          </p>

          <fieldset className="controls">
            <legend>Chart family</legend>
            <label className="radio">
              <input
                type="radio"
                name="family"
                checked={selectedFamily === "tac"}
                onChange={() => void handleProbeSelection("tac", probeLat, probeLon)}
              />
              <span>TAC</span>
            </label>
            <label className="radio">
              <input
                type="radio"
                name="family"
                checked={selectedFamily === "sectional"}
                onChange={() => void handleProbeSelection("sectional", probeLat, probeLon)}
              />
              <span>Sectional</span>
            </label>
          </fieldset>

          <fieldset className="controls">
            <legend>Probe point</legend>
            <label className="radio">
              <input
                type="radio"
                name="probe"
                checked={probeLat === initialProbe.lat && probeLon === initialProbe.lon}
                onChange={() => void handleProbeSelection(selectedFamily, initialProbe.lat, initialProbe.lon)}
              />
              <span>Boston center</span>
            </label>
            <label className="radio">
              <input
                type="radio"
                name="probe"
                checked={probeLat === offshoreProbe.lat && probeLon === offshoreProbe.lon}
                onChange={() => void handleProbeSelection(selectedFamily, offshoreProbe.lat, offshoreProbe.lon)}
              />
              <span>Offshore gap</span>
            </label>
          </fieldset>

          <dl className="facts">
            <div>
              <dt>Lat</dt>
              <dd>{probeLat.toFixed(4)}</dd>
            </div>
            <div>
              <dt>Lon</dt>
              <dd>{probeLon.toFixed(4)}</dd>
            </div>
            <div>
              <dt>Chart</dt>
              <dd>{selectedChart?.display_name ?? "No matching chart"}</dd>
            </div>
          </dl>

          {selectedChart?.display_name === mapTileView.chart_name ? (
            <div className="tileViewport">
              <div
                className="tileGrid"
                style={{
                  gridTemplateColumns: `repeat(${mapTileView.radius * 2 + 1}, ${mapTileView.tile_size / 2}px)`,
                }}
              >
                {tileCells(mapTileView).map((tile) => (
                  <img
                    key={`${tile.x}-${tile.yTms}`}
                    className="tileImage"
                    src={`/prototype-tiles/${mapTileView.tile_root}/${mapTileView.chart_index}/${mapTileView.zoom}/${tile.x}/${tile.yTms}.webp`}
                    alt={`Chart tile ${tile.x}/${tile.yTms}`}
                    width={mapTileView.tile_size / 2}
                    height={mapTileView.tile_size / 2}
                  />
                ))}
                <div
                  className="probeMarker"
                  style={{
                    left: `${(mapTileView.radius + mapTileView.probe_offset_x) * (mapTileView.tile_size / 2)}px`,
                    top: `${(mapTileView.radius + mapTileView.probe_offset_y) * (mapTileView.tile_size / 2)}px`,
                  }}
                />
              </div>
            </div>
          ) : (
            <p className="lede">No tile viewport is loaded for this chart selection yet.</p>
          )}
        </article>
      </section>
    </main>
  );
}

function tileCells(view: typeof mapTileView): Array<{ x: number; yTms: number }> {
  const cells: Array<{ x: number; yTms: number }> = [];
  for (let dy = view.radius; dy >= -view.radius; dy -= 1) {
    for (let dx = -view.radius; dx <= view.radius; dx += 1) {
      cells.push({
        x: view.center_x + dx,
        yTms: view.center_y_tms + dy,
      });
    }
  }
  return cells;
}
