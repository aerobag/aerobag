// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { ImageViewportState } from "./imageViewport";
import type { FlightPlanRouteSegment, LatLon, PlateGeoref } from "./types";

export type PlateFlightPlanScreenSegment = {
  id: string;
  status: FlightPlanRouteSegment["status"];
  style: FlightPlanRouteSegment["style"];
  path: { x: number; y: number }[];
};

export function plateImagePoint(position: LatLon, georef: PlateGeoref) {
  if (!Number.isFinite(position.lat) || !Number.isFinite(position.lon)) {
    return null;
  }
  const point = georef.kind === "plate_transform_v1"
    ? {
        x: (position.lon - georef.top_left_lon) * georef.pixels_per_longitude,
        y: (position.lat - georef.top_left_lat) * georef.pixels_per_latitude,
      }
    : {
        x:
          position.lon * georef.pixel_x_from_lon +
          position.lat * georef.pixel_x_from_lat +
          georef.pixel_x_offset,
        y:
          position.lon * georef.pixel_y_from_lon +
          position.lat * georef.pixel_y_from_lat +
          georef.pixel_y_offset,
      };
  return Number.isFinite(point.x) && Number.isFinite(point.y) ? point : null;
}

export function projectPlateFlightPlanSegments(args: {
  segments: FlightPlanRouteSegment[];
  georef: PlateGeoref;
  imageSize: { width: number; height: number };
  viewport: ImageViewportState;
  displaySize: { width: number; height: number };
  surfaceSize: { width: number; height: number };
}): PlateFlightPlanScreenSegment[] {
  const { segments, georef, imageSize, viewport, displaySize, surfaceSize } = args;
  if (
    imageSize.width <= 0 || imageSize.height <= 0 ||
    displaySize.width <= 0 || displaySize.height <= 0 ||
    surfaceSize.width <= 0 || surfaceSize.height <= 0
  ) {
    return [];
  }
  const scaleX = displaySize.width / imageSize.width;
  const scaleY = displaySize.height / imageSize.height;
  return segments.flatMap((segment) => {
    const sourcePath = segment.path.length > 0 ? segment.path : [segment.from, segment.to];
    const path = sourcePath.map((position) => {
      const imagePoint = plateImagePoint(position, georef);
      return imagePoint
        ? {
            x: viewport.left + imagePoint.x * scaleX,
            y: viewport.top + imagePoint.y * scaleY,
          }
        : null;
    });
    if (path.some((point) => point === null)) {
      return [];
    }
    const screenPath = path as { x: number; y: number }[];
    if (screenPath.length < 2 || !pathBoundsIntersectSurface(screenPath, surfaceSize)) {
      return [];
    }
    return [{ id: segment.id, status: segment.status, style: segment.style, path: screenPath }];
  });
}

function pathBoundsIntersectSurface(
  path: { x: number; y: number }[],
  surfaceSize: { width: number; height: number },
) {
  const margin = 12;
  const minX = Math.min(...path.map((point) => point.x));
  const maxX = Math.max(...path.map((point) => point.x));
  const minY = Math.min(...path.map((point) => point.y));
  const maxY = Math.max(...path.map((point) => point.y));
  return maxX >= -margin && minX <= surfaceSize.width + margin &&
    maxY >= -margin && minY <= surfaceSize.height + margin;
}
