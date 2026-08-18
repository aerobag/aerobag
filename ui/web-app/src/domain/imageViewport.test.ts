// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import conformance from "../generated/uiGeometryConformance.json";
import {
  clampImageViewport,
  createInitialImageViewport,
  dragImageViewport,
  zoomImageAroundPoint,
} from "./imageViewport";

describe("imageViewport", () => {
  it("fits and centers the image initially", () => {
    const viewport = createInitialImageViewport(1200, 800, 900, 700);
    expect(viewport.zoom).toBe(1);
    expect(viewport.left).toBeCloseTo(0);
    expect(viewport.top).toBeCloseTo(50);
  });

  it("preserves the anchor point when zooming", () => {
    const start = createInitialImageViewport(1200, 800, 900, 700);
    const anchor = { x: 300, y: 250 };
    const beforeLocal = {
      x: anchor.x - start.left,
      y: anchor.y - start.top,
    };
    const next = zoomImageAroundPoint(start, anchor.x, anchor.y, 2, 1200, 800, 900, 700, 64);
    const afterLocal = {
      x: (anchor.x - next.left) / next.zoom,
      y: (anchor.y - next.top) / next.zoom,
    };
    expect(afterLocal.x).toBeCloseTo(beforeLocal.x, 4);
    expect(afterLocal.y).toBeCloseTo(beforeLocal.y, 4);
  });

  it("allows one-thumb overscroll but no more", () => {
    const start = createInitialImageViewport(1200, 800, 900, 700);
    const dragged = dragImageViewport(start, 600, 500, 1200, 800, 900, 700, 64);
    const clamped = clampImageViewport(dragged, 1200, 800, 900, 700, 64);
    expect(clamped.left).toBeLessThanOrEqual(64);
    expect(clamped.top).toBeLessThanOrEqual(64);
  });

  it("keeps a real drag range when the fitted image is smaller than the viewport", () => {
    const start = createInitialImageViewport(600, 2400, 1200, 900);
    const leftDragged = dragImageViewport(start, -800, 0, 600, 2400, 1200, 900, 64);
    const rightDragged = dragImageViewport(start, 800, 0, 600, 2400, 1200, 900, 64);
    expect(leftDragged.left).toBeCloseTo(64, 4);
    expect(rightDragged.left).toBeCloseTo(911, 4);
  });

  it("matches core's shared out-of-range zoom clamp vector", () => {
    const vector = conformance.image_clamp;
    expect(clampImageViewport(
      vector.state,
      vector.image_width,
      vector.image_height,
      vector.viewport_width,
      vector.viewport_height,
      vector.overscroll,
    )).toEqual(vector.expected);
  });
});
