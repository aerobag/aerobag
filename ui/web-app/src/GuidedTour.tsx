// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { UiGuidedTour, UiTourAction } from "./generated/sessionPageWire";
import "./guidedTour.css";

export const GuidedTourContext = createContext<UiGuidedTour | null>(null);
export const GuidedTourFeedback = createContext<(generation: number, error: string) => void>(() => {});
export const useGuidedTour = () => useContext(GuidedTourContext);

// Semantic anchors belong to the presentation adapter. Tour content and actions
// are exported by core; these selectors only locate the rendered controls.
const anchors: Record<string, string> = {
  "chart-plate": '[data-testid="page-button-chart"], [data-testid="page-button-plate"], [data-testid="page-button-return-chart"], [data-testid="page-button-return-plate"]',
  cdi: '[data-testid="nav-cdi"]', home: '[data-testid="page-button-home"]',
  "base-map": '[data-testid="chart-family-button"]', "base-map-menu": '[data-testid="chart-family-button-tray"]',
  layers: '[data-testid="layers-button"]', "layers-menu": '[data-testid="layers-button-tray"]',
  "route-entry": '[data-testid="plan-append-route-input"]',
  "plan-row": '.waypointModal', "plan-row-activate": '[data-testid="plan-row-action-activate_leg"]',
  "plan-remove-all-above": '[data-testid="plan-row-action-remove_all_above"]',
  "plan-add-airway": '[data-testid="plan-row-action-add_airway"]',
  "plan-find-route": '[data-testid="plan-row-action-find_route"]',
  "plate-airport-menu": '[data-testid="plate-airport-button-tray"], [data-testid="plate-airport-button"]',
  "plate-folder": '[data-testid="plate-folder-button"]', "plate-folder-content": '.plateFolderGrid',
  "home-guided-tour": '[data-testid="home-button-guided_tour"]',
  "home-cloud": '[data-testid="home-button-cloud"]',
  "home-about": '[data-testid="home-button-about"]', "home-settings": '[data-testid="home-button-settings"]', "home-status": '[data-testid="home-button-data_status"]',
  "airway-picker": '[data-testid="plan-airway-picker"]',
  "flight-plan": '.planTableWrap.isStructured', map: '.mapSurface',
  "routing-summary": '[data-testid="parity:airway-routing-summary"]',
  "routing-gnss": '[data-testid="parity:airway-routing-control-gnss"]',
  "routing-apply": '[data-testid="parity:airway-routing-control-apply_route"]',
  "routing-route": '.airwayRoutingPanel', estimate: '[data-testid="plan-estimate-mode"]',
  "aircraft-models": '[data-testid="altitude-planner-control-aircraft"]',
  "use-model": '[data-testid="altitude-planner-wind-action-ready_forecast"]', "altitude-table": '.altitudeComparisonTable',
  inspector: '[data-testid="map-selection-tray"]', "inspector-weather": '[data-testid="map-selection-tray"] [data-testid*="-wx"]',
  "inspector-supplement": '[data-testid="map-selection-tray"] [data-testid*="-csup"]', "inspector-plates": '[data-testid="map-selection-tray"] [data-testid*="plates"]',
  "inspector-insert": '[data-testid="map-selection-tray"] [data-testid*="insert"]',
  weather: '.weatherDetailModal', notams: '.airportNotamSection', "airport-info": '.airportInfoModal',
  "plan-preview": '[data-testid="tray-option-__direct_situation__"]',
  "inspector-spot": '[data-tour-anchor="inspector-spot"]',
  "map-spot-marker": '[data-tour-anchor="map-spot-marker"]',
  "ownship-menu": '.chartTraySituation', "preview-controls": '.situationTransportRow',
  center: '[data-testid="center-here-button"]', orientation: '[data-testid="map-orientation-button"]',
  "cloud-create": '[data-testid="cloud-action-begin_create"]',
};

function clippedBounds(el: Element): DOMRect | null {
  if (!el.checkVisibility({ checkVisibilityCSS: true })) return null;
  const b = el.getBoundingClientRect();
  let left=Math.max(0,b.left), top=Math.max(0,b.top), right=Math.min(innerWidth,b.right), bottom=Math.min(innerHeight,b.bottom);
  for(let parent=el.parentElement;parent;parent=parent.parentElement) {
    const style=getComputedStyle(parent), p=parent.getBoundingClientRect();
    if (/(hidden|auto|scroll|clip)/.test(style.overflowX)) { left=Math.max(left,p.left); right=Math.min(right,p.right); }
    if (/(hidden|auto|scroll|clip)/.test(style.overflowY)) { top=Math.max(top,p.top); bottom=Math.min(bottom,p.bottom); }
  }
  return right>left&&bottom>top ? new DOMRect(left,top,right-left,bottom-top) : null;
}

export function guidedTourTargets(tour: UiGuidedTour): DOMRect[] {
  return tour.targets.flatMap(target => {
    const selector = target === "aircraft-model-option" && tour.option_uid
      ? `[data-testid="tray-option-${CSS.escape(tour.option_uid)}"]` : anchors[target];
    if (!selector) return [];
    const visible=[...document.querySelectorAll(selector)].map(clippedBounds).filter((b): b is DOMRect=>b!==null);
    return visible.slice(0,1);
  });
}

export function guidedTourPosition(tour: UiGuidedTour, targets: DOMRect[], width: number, height: number,
  viewportWidth = innerWidth, viewportHeight = innerHeight) {
  const right = Math.max(12, viewportWidth - width - 20);
  const bottom = Math.max(12, viewportHeight - height - 100);
  if (tour.placement === "top_right") return { left: right, top: 20 };
  if (tour.placement === "bottom_right") return { left: right, top: bottom };
  if (tour.placement === "center") return { left: Math.max(12, (viewportWidth-width)/2), top: Math.max(12, (viewportHeight-height)/2) };
  const candidates = [{ left: 20, top: 20 }, { left: right, top: 20 }, { left: 20, top: bottom }, { left: right, top: bottom }];
  const overlap = (p: typeof candidates[number]) => targets.reduce((sum, b) => sum
    + Math.max(0, Math.min(p.left+width+20,b.right)-Math.max(p.left-20,b.left))
    * Math.max(0, Math.min(p.top+height+20,b.bottom)-Math.max(p.top-20,b.top)), 0);
  return candidates.sort((a,b) => overlap(a)-overlap(b))[0];
}

export function GuidedTourOverlay(props: {
  tour: UiGuidedTour; busy: boolean; error: string | null;
  onAction: (action: UiTourAction) => void;
}) {
  const { tour } = props;
  const panel = useRef<HTMLDivElement>(null);
  const action = useRef(props.onAction);
  action.current = props.onAction;
  const shortcuts = useRef(tour.shortcuts);
  shortcuts.current = tour.shortcuts;
  const [measurement, setMeasurement] = useState<{ generation: number; bounds: DOMRect[] }>({ generation: -1, bounds: [] });
  const bounds = measurement.bounds;
  const preparing = props.busy || (!props.error && (measurement.generation !== tour.generation || bounds.length < tour.targets.length));
  const [position, setPosition] = useState({ left: 24, top: 24 });
  useEffect(() => {
    const prior = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.focus();
    const blockKeys = (event: KeyboardEvent) => {
      const shortcut = shortcuts.current.find(binding => binding.key === event.key.toLowerCase());
      if (shortcut) {
        event.preventDefault(); event.stopImmediatePropagation();
        if (!event.repeat && !event.isComposing && !event.ctrlKey && !event.altKey && !event.metaKey) {
          // Reuse the current button's readiness and action, regardless of focus.
          panel.current?.querySelector<HTMLButtonElement>(`[data-testid="guided-tour-${shortcut.action}"]`)?.click();
        }
        return;
      }
      if (event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); action.current("close"); return; }
      if (event.key === "Tab") {
        const buttons = [...(panel.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        buttons[(index + (event.shiftKey ? buttons.length - 1 : 1)) % buttons.length]?.focus();
        event.preventDefault(); event.stopImmediatePropagation();
      } else if (!panel.current?.contains(event.target as Node)) {
        event.preventDefault(); event.stopImmediatePropagation();
      } else if (!["Enter", " "].includes(event.key)) { event.stopImmediatePropagation(); }
    };
    window.addEventListener("keydown", blockKeys, true);
    return () => { window.removeEventListener("keydown", blockKeys, true); prior?.focus(); };
  }, []);
  useLayoutEffect(() => {
    let frame = 0;
    let prior = "";
    const measure = () => {
      const next = guidedTourTargets(tour);
      const key = JSON.stringify(next.map(b => [b.x, b.y, b.width, b.height]));
      const w = panel.current?.offsetWidth ?? 380, h = panel.current?.offsetHeight ?? 240;
      const nextPosition = guidedTourPosition(tour, next, w, h);
      if (key !== prior) { prior = key; setMeasurement({ generation: tour.generation, bounds: next }); }
      setPosition(old => old.left === nextPosition.left && old.top === nextPosition.top ? old : nextPosition);
      frame = requestAnimationFrame(measure);
    };
    frame = requestAnimationFrame(measure);
    return () => cancelAnimationFrame(frame);
  }, [tour.generation]);
  const stop = (event: React.SyntheticEvent) => event.stopPropagation();
  return createPortal(<div className="guidedTourScrim" data-testid="guided-tour-scrim"
    onPointerDown={stop} onPointerUp={stop} onClick={stop} onDoubleClick={stop} onWheel={stop} onContextMenu={e => e.preventDefault()}>
    <svg className="guidedTourArrows" width="100%" height="100%" aria-hidden="true">
      {bounds.map((b,index) => {
        const pw = panel.current?.offsetWidth ?? 380, ph = panel.current?.offsetHeight ?? 240;
        const cx = position.left+pw/2, cy = position.top+ph/2;
        const tx = Math.min(Math.max(cx,b.left),b.right), ty = Math.min(Math.max(cy,b.top),b.bottom);
        const horizontal = Math.abs(tx-cx)/pw > Math.abs(ty-cy)/ph;
        const sx = horizontal ? (tx > cx ? position.left+pw : position.left) : cx;
        const sy = horizontal ? cy : (ty > cy ? position.top+ph : position.top);
        const length = Math.hypot(tx-sx,ty-sy) || 1, ux = (tx-sx)/length, uy = (ty-sy)/length;
        const arrow = `M${sx},${sy} L${tx},${ty} M${tx-ux*16-uy*8},${ty-uy*16+ux*8} L${tx},${ty} L${tx-ux*16+uy*8},${ty-uy*16-ux*8}`;
        return <g key={index}>{["outer", "outline", "color"].map(layer => <g key={layer} className={`guidedTourArrow-${layer}`}>
          <rect x={b.left-4} y={b.top-4} width={b.width+8} height={b.height+8} rx="9" />
          <path d={arrow} /></g>)}</g>;
      })}
    </svg>
    <div ref={panel} className={`guidedTourPanel${tour.presentation === "title_card" ? " isTitleCard" : ""}`} role="dialog" aria-modal="true" aria-labelledby="guided-tour-title" tabIndex={-1} style={position} data-testid="guided-tour-panel" data-tour-step={tour.step_id} data-tour-target-count={tour.targets.length}>
      {tour.presentation !== "title_card" ? <div className="guidedTourChapter">{tour.chapter} <span>{tour.position} / {tour.total}</span></div> : null}
      <h2 id="guided-tour-title">{tour.title}</h2>
      {tour.body ? <p>{tour.body}</p> : null}
      {props.error ? <p role="alert" className="guidedTourError">{props.error}</p> : null}
      {tour.restart_label ? <button type="button" className="guidedTourRestart" disabled={props.busy} onClick={() => props.onAction("restart")} data-testid="guided-tour-restart">{tour.restart_label}</button> : null}
      <div className="guidedTourButtons">
        <button type="button" onClick={() => props.onAction("close")} data-testid="guided-tour-close">{tour.close_label}</button>
        <button type="button" disabled={!tour.back_enabled || props.busy} onClick={() => props.onAction("back")} data-testid="guided-tour-back">Back</button>
        <button type="button" disabled={preparing} onClick={() => props.onAction("next")} data-testid="guided-tour-next">{preparing ? "Loading…" : tour.next_label}</button>
      </div>
    </div>
  </div>, document.body);
}
