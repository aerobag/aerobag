// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useCallback, useState, type RefObject, type MutableRefObject, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { latLonToWorld, screenToWorld, worldToLatLon, worldToScreen, transformScreenPointBetweenFrames, type MapDisplayFrame, type MapViewportState } from "./domain/mapViewport";

type Point = {lat: number; lon: number};
/** Geometry is drawn in the committed, north-up content frame. The map's existing
 * bearing and immediate drag transforms move this host along with every other layer.
 * Pointer conversion instead reads the live display frame, including bearing.
 * Geographic overlays accept this binding rather than an independent viewport. */
export type MapGeometryBinding = {
  host: HTMLDivElement | null;
  frame: MapDisplayFrame;
  screen: (point: Point) => {x: number; y: number};
  /** Label collisions use the displayed bearing; placement returns to the
   * content frame so the same immediate map transform still owns movement. */
  labelScreen: (point: Point) => {x: number; y: number};
  labelToContent: (point: {x: number; y: number}) => {x: number; y: number};
  pointer: (clientX: number, clientY: number) => {position: Point; radius: number};
  displayViewport: () => MapViewportState;
};
export function useMapGeometryBinding(viewport: MapViewportState, width: number, height: number,
  surface: RefObject<HTMLDivElement>, liveViewport: MutableRefObject<MapViewportState>,
  mapUp: MutableRefObject<number>, contentTransform: MutableRefObject<HTMLDivElement | null>) {
  const [host, setHost] = useState<HTMLDivElement | null>(null);
  const bindContent = useCallback((element: HTMLDivElement | null) => {
    contentTransform.current = element;
    setHost(element);
  }, [contentTransform]);
  const displayViewport = () => ({...liveViewport.current, rotationDeg: mapUp.current});
  const contentFrame = {viewport: {...viewport, rotationDeg: 0}, width, height};
  const labelFrame = {viewport: {...viewport, rotationDeg: mapUp.current}, width, height};
  const binding: MapGeometryBinding = {
    host, frame: contentFrame, displayViewport,
    screen: (point) => worldToScreen({...viewport, rotationDeg: 0}, latLonToWorld(point.lat, point.lon), width, height),
    labelScreen: (point) => worldToScreen(labelFrame.viewport, latLonToWorld(point.lat, point.lon), width, height),
    labelToContent: (point) => transformScreenPointBetweenFrames(labelFrame, contentFrame, point),
    pointer: (clientX, clientY) => {
      const rect = surface.current!.getBoundingClientRect();
      const point = {x: clientX - rect.left, y: clientY - rect.top};
      const frame = displayViewport();
      const world = screenToWorld(frame, point, width, height);
      const position = worldToLatLon(world.x, world.y);
      const nearWorld = screenToWorld(frame, {x: point.x + 26, y: point.y}, width, height);
      const near = worldToLatLon(nearWorld.x, nearWorld.y);
      const radius = Math.max(0.01, Math.hypot((near.lat-position.lat)*60, (near.lon-position.lon)*60*Math.cos(position.lat*Math.PI/180)));
      return {position, radius};
    },
  };
  return {binding, bindContent};
}
export function MapGeometryLayer({binding, children}: {binding: MapGeometryBinding; children: ReactNode}) {
  return binding.host ? createPortal(children, binding.host) : null;
}
