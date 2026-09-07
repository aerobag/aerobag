// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

"use strict";

const CoordinateInput = {
  parse(text, axis) {
    const limit = { latitude: 90, longitude: 180 }[axis];
    if (!limit) throw new Error("Unknown coordinate axis: " + axis);
    const parts = text.trim().split(/\s+/);
    if (parts[0] === "") return null;
    const decimal = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/;
    if (parts.length > 2 || parts.some((part) => !decimal.test(part))) {
      throw new Error("Enter decimal degrees or degrees minutes (for example 58 15).");
    }
    let value = Number(parts[0]);
    if (parts.length === 2) {
      const minutes = Number(parts[1]);
      if (!/^[+-]?\d+$/.test(parts[0]) || parts[1].startsWith("-") || minutes >= 60) {
        throw new Error("Use whole degrees and minutes from 0 up to (but not including) 60.");
      }
      value = (parts[0].startsWith("-") ? -1 : 1) * (Math.abs(value) + minutes / 60);
    }
    if (!Number.isFinite(value) || Math.abs(value) > limit) {
      throw new Error(axis + " must be between -" + limit + " and " + limit + ".");
    }
    return Math.round(value * 1e9) / 1e9;
  },
};

if (typeof module !== "undefined") module.exports = CoordinateInput;
