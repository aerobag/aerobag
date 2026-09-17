// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState } from "react";
import type { UiSession } from "./domain/appCoreAdapter";
import type { UiGlideRing } from "./generated/sessionPageWire";
import { MapGeometryLayer, type MapGeometryBinding } from "./MapGeometryLayer";
import { debugLog } from "./domain/debugLog";

export function GlideRingOverlay({session, geometry}: {session: UiSession; geometry: MapGeometryBinding}) {
  const [ring, setRing] = useState<UiGlideRing | null>(null);
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const update = async () => {
      let delay = 1_000;
      try {
        const next = await session.queryGlideRing();
        delay = next.recheck_after_ms;
        if (!cancelled) setRing(next);
      } catch (error) {
        if (!cancelled) {
          setRing(null);
          debugLog("glide.query.failed", {error: String(error)});
        }
      } finally {
        // One request in flight. Core decides whether motion, altitude, age or
        // a changed model warrants calculation; ordinary checks reuse its cache.
        if (!cancelled) timer = setTimeout(update, delay);
      }
    };
    void update();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [session]);
  return ring ? <GlideRingGeometry ring={ring} geometry={geometry} /> : null;
}

export function GlideRingGeometry({ring, geometry}: {ring: UiGlideRing; geometry: MapGeometryBinding}) {
  const label = ring.label_position ? geometry.screen(ring.label_position) : null;
  return <MapGeometryLayer binding={geometry}>
    <svg width={geometry.frame.width} height={geometry.frame.height} data-testid="glide-ring"
      aria-label="Estimated glide reach" style={{position: "absolute", inset: 0, zIndex: 8, pointerEvents: "none", overflow: "visible"}}>
      {ring.paths.map((points, index) => {
        const d = points.map((point, i) => { const p = geometry.screen(point); return `${i ? "L" : "M"}${p.x},${p.y}`; }).join(" ");
        return <g key={index} fill="none" strokeLinejoin="round">
          <path d={d} stroke="white" strokeWidth="6" />
          <path d={d} stroke="#087c85" strokeWidth="3" strokeDasharray="9 5" />
        </g>;
      })}
      {label ? <g transform={`translate(${label.x},${label.y})`}>
        <g className="mapUpright"
        textAnchor="middle" fontSize="14" fontWeight="600" fill="#06545b" stroke="white" strokeWidth="4" paintOrder="stroke">
        {ring.wind_direction_deg_true != null ? <g transform="translate(-24,-28)">
          <text data-testid="glide-wind-arrow" dominantBaseline="central"
            style={{transform: `rotate(calc(${ring.wind_direction_deg_true}deg - var(--map-up-deg, 0deg)))`}}>↑</text>
        </g> : null}
        <text x={ring.wind_direction_deg_true != null ? 6 : 0} y="-23">{ring.wind_label}</text>
        <text y="-6">{ring.speed_label}</text>
        </g>
      </g> : null}
    </svg>
  </MapGeometryLayer>;
}
