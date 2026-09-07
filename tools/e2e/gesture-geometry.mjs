// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export function clampDragEndpoint(start, delta, minimum, maximum) {
  return {
    x: Math.max(minimum.x, Math.min(maximum.x, start.x + delta.x)),
    y: Math.max(minimum.y, Math.min(maximum.y, start.y + delta.y)),
  };
}

export function timelineSeekDeltaX(cursor, duration, distance = 320) {
  return cursor >= duration / 2 ? -distance : distance;
}

function normalizedRect(rect) {
  if (!rect) return null;
  const left = Number(rect.left);
  const top = Number(rect.top);
  const width = Number(rect.width ?? (Number(rect.right) - left));
  const height = Number(rect.height ?? (Number(rect.bottom) - top));
  if (![left, top, width, height].every(Number.isFinite) || width <= 0 || height <= 0) {
    return null;
  }
  return { left, top, right: left + width, bottom: top + height, width, height };
}

const MAP_POINT_FRACTIONS = Object.freeze([
  [0.30, 0.70], [0.70, 0.70], [0.30, 0.30], [0.70, 0.30], [0.50, 0.50],
  [0.15, 0.70], [0.85, 0.70], [0.15, 0.30], [0.85, 0.30],
  [0.30, 0.85], [0.70, 0.85], [0.30, 0.15], [0.70, 0.15],
  [0.50, 0.70], [0.50, 0.30], [0.30, 0.50], [0.70, 0.50],
]);

/** Pick a stable map-relative point outside every smaller rendered UI rectangle. */
export function chooseUnobscuredMapPoint(surfaceBounds, renderedBounds, clearancePx = 8) {
  const surface = normalizedRect(surfaceBounds);
  if (!surface) throw new Error("map surface has invalid bounds");
  const surfaceArea = surface.width * surface.height;
  const obstacles = renderedBounds
    .map(normalizedRect)
    .filter((rect) => rect && rect.width * rect.height < surfaceArea * 0.5);

  for (const [x, y] of MAP_POINT_FRACTIONS) {
    const screenX = surface.left + surface.width * x;
    const screenY = surface.top + surface.height * y;
    const blocked = obstacles.some((rect) =>
      screenX >= rect.left - clearancePx && screenX <= rect.right + clearancePx &&
      screenY >= rect.top - clearancePx && screenY <= rect.bottom + clearancePx);
    if (!blocked) return { x, y, screenX, screenY };
  }
  return null;
}
