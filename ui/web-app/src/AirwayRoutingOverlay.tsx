// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import type { NavSymbolFeature } from "./generated/navQueryWire";
import type { UiAirwayRouteDragPhase, UiAirwayRoutePosition, UiAirwayRouting } from "./generated/sessionPageWire";
import type { UiSession, UiSessionSnapshot } from "./domain/appCoreAdapter";
import type { MapViewportState } from "./domain/mapViewport";
import { MapGeometryLayer, type MapGeometryBinding } from "./MapGeometryLayer";
import { measureRouteLabel, routeLabelBounds, routeLabelIndices, routeLabelLayout, type LabelRect } from "./domain/routeLabelLayout";
import "./airwayRouting.css";

type Props = {
  view: UiAirwayRouting; session: UiSession; geometry: MapGeometryBinding;
  onViewport: (viewport: MapViewportState) => void;
  onSnapshot: (snapshot: UiSessionSnapshot, source: string) => void;
  onError: (error: unknown) => void;
  renderSymbol: (feature: NavSymbolFeature) => ReactNode;
  renderActionIcon: (symbolId: string) => ReactNode;
};
type Drag = { pointer: number; insert: number; moving: number | null; startX: number; startY: number; moved: boolean; remove: string | null };
type Request = { phase: UiAirwayRouteDragPhase; position: UiAirwayRoutePosition; radius: number; drag: Drag };

export function AirwayRoutingOverlay(props: Props) {
  const {view, geometry} = props;
  const {width, height} = geometry.frame;
  const latest = useRef(props); latest.current = props;
  const fitted = useRef<string | null>(null);
  const drag = useRef<Drag | null>(null);
  const pending = useRef<Request | null>(null);
  const active = useRef(false);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [cursor, setCursor] = useState<UiAirwayRoutePosition | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [panelBounds, setPanelBounds] = useState<LabelRect | null>(null);
  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const update = () => {
      const bounds = element.getBoundingClientRect(), parent = element.parentElement!.getBoundingClientRect();
      setPanelBounds({left:bounds.left-parent.left, top:bounds.top-parent.top,
        right:bounds.right-parent.left, bottom:bounds.bottom-parent.top});
    };
    const observer = new ResizeObserver(update); observer.observe(element); update();
    return () => observer.disconnect();
  }, [width, height]);
  useEffect(() => {
    if (width <= 0 || height <= 0 || fitted.current === view.edit_id) return;
    let cancelled = false;
    void (async () => {
      const p = latest.current;
      const frame = await p.session.airwayRoutingViewport(width, height, geometry.displayViewport().rotationDeg ?? 0);
      if (cancelled || !frame) return;
      p.onSnapshot(await p.session.disengageMapFollow(frame), "airway_routing_fit");
      if (!cancelled) { fitted.current = view.edit_id; p.onViewport(frame); }
    })().catch((error) => latest.current.onError(error));
    return () => { cancelled = true; };
  }, [view.edit_id, width > 0 && height > 0]);

  async function drain() {
    if (active.current) return;
    active.current = true;
    setBusy(true);
    try {
      while (pending.current) {
        const request = pending.current; pending.current = null;
        const p = latest.current;
        const snapshot = await p.session.dragAirwayRoute(p.view.edit_id, request.phase, request.position,
          request.radius, request.drag.insert, request.drag.moving);
        p.onSnapshot(snapshot, "airway_routing_drag");
      }
    } catch (error) { pending.current = null; latest.current.onError(error); }
    finally { active.current = false; setBusy(false); }
  }
  function send(event: PointerEvent<SVGSVGElement>, phase: UiAirwayRouteDragPhase) {
    if (!drag.current) return;
    const {position: at, radius} = latest.current.geometry.pointer(event.clientX, event.clientY);
    pending.current = {phase, position: at, radius, drag: drag.current};
    setCursor(phase === "preview" ? at : null);
    void drain();
  }
  function begin(event: PointerEvent<SVGElement>, insert: number, moving: number | null) {
    if (busy || event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    drag.current = {pointer: event.pointerId, insert, moving, startX: event.clientX, startY: event.clientY, moved: false,
      remove: moving == null ? null : view.via_points[moving]?.remove_action.action_id ?? null};
    setDragging(true);
    event.currentTarget.ownerSVGElement?.setPointerCapture(event.pointerId);
  }
  function finish(event: PointerEvent<SVGSVGElement>, phase: UiAirwayRouteDragPhase) {
    if (drag.current?.pointer !== event.pointerId) return;
    event.stopPropagation();
    const gesture = drag.current;
    if (gesture.moved) send(event, phase);
    drag.current = null; setDragging(false); setCursor(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!gesture.moved && phase === "commit" && gesture.remove) void action(gesture.remove, true);
  }
  const screen = geometry.screen;
  const selected = view.route;
  const labelPoint = (position: UiAirwayRoutePosition, offset: number) => {
    const p = geometry.labelScreen(position); return {x:p.x, y:p.y+offset};
  };
  // Reserve measured point labels and the tray; protected MEAs relocate around
  // these before ordinary leg labels get any space.
  const labelBoxes = [
    ...(panelBounds ? [panelBounds] : []),
    ...(selected?.junctions ?? []).map((point) => routeLabelBounds(labelPoint(point.position,-24), measureRouteLabel(point.label))),
    ...view.via_points.map((point) => routeLabelBounds(labelPoint(point.position,-18), measureRouteLabel(point.label))),
    ...(selected?.crossings ?? []).map((point) => routeLabelBounds(labelPoint(point.position,40), measureRouteLabel(point.label,true))),
  ];
  const labelCandidates = (selected?.legs ?? []).map((leg) => ({from:geometry.labelScreen(leg.from),
    to:geometry.labelScreen(leg.to), label:leg.label, important:leg.highest_mea}));
  const labels = routeLabelIndices(labelCandidates,width,height).flatMap((index) => {
    const leg = selected!.legs[index], candidate = labelCandidates[index];
    const layout = routeLabelLayout(candidate.from, candidate.to, width, height,
      measureRouteLabel(leg.label,leg.highest_mea), labelBoxes, leg.highest_mea);
    if (!layout) return [];
    labelBoxes.push(layout.bounds);
    const p = geometry.labelToContent(layout.baseline), anchor = geometry.labelToContent(layout.anchor);
    const leader = layout.leader ? geometry.labelToContent(layout.leader) : null;
    return [<g key={index} data-testid={`airway-route-label-${index}`}>
      {leader ? <line className="routeLabelLeader" x1={anchor.x} y1={anchor.y} x2={leader.x} y2={leader.y}
        stroke="#941426" strokeWidth="1.5" /> : null}
      <g transform={`translate(${p.x} ${p.y})`}><g className="mapUpright"><text className={leg.highest_mea ? "highest" : ""}>{leg.label}</text></g></g>
    </g>];
  });
  async function action(id: string, pinClick = false) {
    if (busy || (dragging && !pinClick)) return;
    setBusy(true);
    try { props.onSnapshot(await props.session.performAirwayRoutingAction(id), "airway_routing_action"); }
    catch (error) { props.onError(error); }
    finally { setBusy(false); }
  }
  return <div className="airwayRoutingOverlay" data-testid="airway-routing-editor">
    <MapGeometryLayer binding={geometry}><svg className="airwayRoutingGeometry" width={width} height={height}
      onPointerMove={(event) => {const gesture=drag.current; if (gesture) {event.stopPropagation();
        if (Math.hypot(event.clientX-gesture.startX,event.clientY-gesture.startY)>8) gesture.moved=true;
        if (gesture.moved) send(event, "preview");}}}
      onPointerUp={(event) => finish(event, "commit")} onPointerCancel={(event) => finish(event, "cancel")}
      onLostPointerCapture={(event) => finish(event, "cancel")}>
      <polyline points={view.original_path.map((point) => {const p=screen(point); return `${p.x},${p.y}`;}).join(" ")}
        fill="none" stroke="#687986" strokeWidth="3" strokeDasharray="5 6" />
      {selected?.legs.map((leg, index) => {const a=screen(leg.from), b=screen(leg.to); return <g key={index}>
        <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="white" strokeWidth="8" />
        <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="#7b26cb" strokeWidth="4" strokeDasharray={leg.direct ? "7 6" : undefined} />
        <line className="routeDragHit" data-testid={`airway-route-leg-${index}`}
          x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke="transparent" strokeWidth="24"
          onPointerDown={(event) => begin(event, leg.via_insert_index, null)} />
      </g>;})}
      {!selected && view.original_path.length > 1 ? <polyline className="routeDragHit"
        points={view.original_path.map((point) => {const p=screen(point); return `${p.x},${p.y}`;}).join(" ")}
        fill="none" stroke="transparent" strokeWidth="24" onPointerDown={(event) => begin(event, 0, null)} /> : null}
      {labels}
      {selected?.crossings.map((crossing, index) => {const p=screen(crossing.position); return <g key={index}
        data-testid={`airway-route-crossing-${index}`} transform={`translate(${p.x} ${p.y})`}>
        <g className="mapUpright">
          <line x1="0" y1="16" x2="0" y2="26" stroke="#941426" strokeWidth="2" />
          <text className="highest" y="40">{crossing.label}</text>
        </g>
      </g>;})}
      {selected?.junctions.map((junction) => {const p=screen(junction.position); return <g key={junction.node_id}
        className="routeJunction" data-testid={`airway-route-junction-${junction.node_id}`} transform={`translate(${p.x} ${p.y})`}>
        <circle r="14" fill="white" />
        {junction.symbol_feature ? props.renderSymbol(junction.symbol_feature) : null}
        <g className="mapUpright"><text y="-24">{junction.label}</text></g>
      </g>;})}
      {view.via_points.map((via) => {const p=screen(via.position); return <g key={via.index}>
        <circle className="routeDragHit routePin" data-testid={`airway-route-via-${via.index}`} cx={p.x} cy={p.y} r="11" fill="white" stroke="#7b26cb" strokeWidth="4"
          role="button" tabIndex={0} aria-label={via.remove_action.label}
          onKeyDown={(event) => {if (event.key === "Enter" || event.key === " ") {event.preventDefault(); event.stopPropagation(); void action(via.remove_action.action_id);}}}
          onPointerDown={(event) => begin(event, via.index, via.index)} />
        <g transform={`translate(${p.x} ${p.y})`}><g className="mapUpright"><text y="-18">{via.label}</text></g></g>
      </g>;})}
      {cursor ? <circle cx={screen(cursor).x} cy={screen(cursor).y} r="14" fill="none" stroke="#7b26cb" strokeWidth="2" strokeDasharray="3 3" /> : null}
      {view.drag_target ? (() => {const p=screen(view.drag_target!); return <circle cx={p.x} cy={p.y} r="8" fill="#ffdc54" stroke="#3a244b" strokeWidth="2" />;})() : null}
    </svg></MapGeometryLayer>
    <div ref={panel} className="airwayRoutingPanel waypointActionTray" onPointerDown={(e) => e.stopPropagation()} onPointerUp={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
      <strong>{view.title}</strong>
      {view.route ? <div className="airwayRoutingSummary">{view.route.summary}</div> : null}
      {view.drag_label || view.message ? <div className="airwayRoutingMessage" aria-live="polite">{view.drag_label || view.message}</div> : null}
      <div className="airwayRoutingControls" role="toolbar" aria-label={view.title}>
        {view.controls.map(({button, selected, symbol_id}) => <button type="button"
          className={`trayButton trayButtonSquare planControlButton${selected ? " selectedControlHighlight" : ""}`}
          key={button.label} data-testid={button.test_id} aria-pressed={selected}
          disabled={busy || dragging || !button.enabled} onClick={() => void action(button.action_id)}>
          {props.renderActionIcon(symbol_id)}<span className="planControlButtonLabel">{button.label}</span>
        </button>)}
      </div>
    </div>
  </div>;
}
