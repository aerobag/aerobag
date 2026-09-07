// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

"use strict";

const CutlinePoints = {
  minimumCount: 3,

  nearest(points, position) {
    let nearest = -1;
    let distance = Infinity;
    points.forEach((point, index) => {
      const candidate = Math.hypot(point[0] - position[0], point[1] - position[1]);
      if (candidate < distance) {
        nearest = index;
        distance = candidate;
      }
    });
    return nearest;
  },

  insertAfter(points, index) {
    const next = points[(index + 1) % points.length];
    const current = points[index];
    const result = points.slice();
    result.splice(index + 1, 0, [
      (current[0] + next[0]) / 2,
      (current[1] + next[1]) / 2,
    ]);
    return { points: result, selectedIndex: index + 1 };
  },

  remove(points, index) {
    if (points.length <= this.minimumCount) {
      return null;
    }
    const result = points.slice();
    result.splice(index, 1);
    return { points: result, selectedIndex: Math.min(index, result.length - 1) };
  },

  nudge(points, index, direction, amount) {
    const result = points.slice();
    result[index] = [
      points[index][0] + direction[0] * amount,
      points[index][1] + direction[1] * amount,
    ];
    return { points: result, selectedIndex: index };
  },

  hasArea(points) {
    if (points.length < this.minimumCount) {
      return false;
    }
    const twiceArea = points.reduce((sum, point, index) => {
      const next = points[(index + 1) % points.length];
      return sum + point[0] * next[1] - next[0] * point[1];
    }, 0);
    return Math.abs(twiceArea) > 1;
  },
};
