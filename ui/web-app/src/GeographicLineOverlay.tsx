// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { MapGeometryLayer, type MapGeometryBinding } from "./MapGeometryLayer";
import type { GeographicLineAnnotation } from "./generated/sessionPageWire";

export function GeographicLineOverlay({ binding, annotation }: {
  binding: MapGeometryBinding;
  annotation: GeographicLineAnnotation | null | undefined;
}) {
  if (!annotation) return null;
  const points = annotation.points.map(binding.screen).map(point => `${point.x},${point.y}`).join(" ");
  const label = binding.screen(annotation.label_position);
  return <MapGeometryLayer binding={binding}>
    <svg className="geographicLineOverlay" width={binding.frame.width} height={binding.frame.height} data-testid="altitude-intercept-arc" aria-label={annotation.label}>
      <polyline points={points} className="geographicLineContrast" />
      <polyline points={points} className="geographicLineStroke" />
      <text x={label.x} y={label.y} dy="-10" textAnchor="middle" transform={`rotate(${annotation.label_bearing_deg},${label.x},${label.y})`}>{annotation.label}</text>
    </svg>
  </MapGeometryLayer>;
}
