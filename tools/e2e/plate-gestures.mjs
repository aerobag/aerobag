// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Viewport projections are published by the rendered viewer only when its
// selected image and input geometry are ready. A launcher label is insufficient.
export function parsePlateViewport(entries) {
  const id = entries[0]?.id ?? "";
  const match = /^parity:plate-viewport:chart:(.+):zoom:([\d.]+):left:([-\d.]+):top:([-\d.]+)$/.exec(id);
  if (!match) return null;
  const [, chartId, zoom, left, top] = match;
  const viewport = { chartId, zoom: Number(zoom), left: Number(left), top: Number(top) };
  if (![viewport.zoom, viewport.left, viewport.top].every(Number.isFinite)) {
    throw new Error(`invalid plate viewport: ${id}`);
  }
  return viewport;
}

export function plateGestureCompleted(before, after, gesture) {
  if (!before || !after || after.chartId !== before.chartId) return false;
  if (gesture.zoom != null) return Math.sign(after.zoom - before.zoom) === -Math.sign(gesture.zoom);
  // A recenter/zoom or navigation to a different document cannot pass a pan.
  if (after.zoom !== before.zoom) return false;
  return ["x", "y"].some((axis) => {
    const delta = gesture.pan[axis];
    const field = axis === "x" ? "left" : "top";
    return delta !== 0 && Math.sign(after[field] - before[field]) === Math.sign(delta);
  });
}

export async function gesturePlate(runtime, description, chartId, gesture) {
  const read = async () => parsePlateViewport(await runtime.driver.readProjection("parity:plate-viewport:"));
  let before = null;
  const after = await runtime.transition(description, {
    // The producer's loaded-image/layout boundary replaces repeated equal
    // samples. Those samples could agree forever while the image was pending.
    readinessSamples: 1,
    completionSamples: 1,
    ready: async () => {
      const viewport = await read();
      if (viewport?.chartId !== chartId) return null;
      const surface = await runtime.driver.readElement("plate-surface");
      if (!surface || surface.actionable === false) return null;
      before = viewport;
      return { ...surface, viewport };
    },
    act: (surface) => gesture.zoom != null
      ? runtime.driver.zoom("plate-surface", gesture.zoom, surface)
      : runtime.driver.drag("plate-surface", gesture.pan, surface),
    complete: read,
    completionSatisfied: (value) => plateGestureCompleted(before, value, gesture),
    diagnose: async () => ({ before, after: await read(), gesture }),
  });
  return { before, after };
}
