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
