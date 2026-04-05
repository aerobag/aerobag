import { useEffect, useState } from "react";
import { loadBestAvailableAdapter } from "./domain/appCoreAdapter";
import { ContentViewModel } from "./domain/contentViewModel";
import { installedInventory, remoteOnlyInventory, sampleCatalog, samplePlan } from "./domain/sampleData";
import type { AppState, ContentPolicy } from "./domain/types";

const policyOptions: Array<{ value: ContentPolicy; label: string }> = [
  { value: "StreamAllowed", label: "Stream Allowed" },
  { value: "PreferLocal", label: "Prefer Local" },
  { value: "OfflineRequired", label: "Offline Required" },
];

export default function App() {
  const [model, setModel] = useState<ContentViewModel | null>(null);
  const [state, setState] = useState<AppState | null>(null);
  const [inventoryMode, setInventoryMode] = useState<"remote" | "installed">("remote");
  const [backendLabel, setBackendLabel] = useState("Loading adapter...");

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      const loaded = await loadBestAvailableAdapter();
      const nextModel = new ContentViewModel(loaded.adapter);
      let next = await nextModel.loadPlan(samplePlan);
      next = await nextModel.setPolicy("StreamAllowed");
      next = await nextModel.refresh(remoteOnlyInventory);
      if (!cancelled) {
        setModel(nextModel);
        setState(next);
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
      </section>
    </main>
  );
}
