// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Pointer-rate mirror of app_core::ui_geometry::route_labels; shared conformance
// vectors fence placement, clipping and the protected-label collision fallback.
type Point = {x: number; y: number};
export type LabelRect = {left: number; top: number; right: number; bottom: number};
export type RouteLabelLayout = {anchor: Point; baseline: Point; bounds: LabelRect; leader: Point | null};
type RouteLabelCandidate = {from: Point; to: Point; label: string; important: boolean};

export function routeLabelIndices(legs: readonly RouteLabelCandidate[], width: number, height: number): number[] {
  const result: number[] = [];
  let index = 0;
  while (index < legs.length) {
    const first = index++;
    if (!legs[first].important) { result.push(first); continue; }
    while (index < legs.length && legs[index].important && legs[index].label === legs[first].label &&
      legs[index-1].to.x === legs[index].from.x && legs[index-1].to.y === legs[index].from.y) index++;
    let best: number | null = null, length = -1;
    for (let member=first; member<index; member++) {
      const leg = legs[member], interval = visibleInterval(leg.from,leg.to,width,height);
      if (interval) {
        const visibleLength = Math.hypot(leg.to.x-leg.from.x,leg.to.y-leg.from.y)*(interval[1]-interval[0]);
        if (visibleLength > length+1e-4) { length = visibleLength; best = member; }
      }
    }
    if (best != null) result.push(best);
  }
  return result.sort((a,b) => Number(legs[b].important)-Number(legs[a].important));
}

export function routeLabelBounds(baseline: Point, textWidth: number): LabelRect {
  return {left: baseline.x-textWidth/2-4, top: baseline.y-17,
    right: baseline.x+textWidth/2+4, bottom: baseline.y+7};
}

function visibleInterval(from: Point, to: Point, width: number, height: number): [number,number] | null {
  let start = 0, end = 1;
  for (const [origin, delta, limit] of [[from.x, to.x-from.x, width], [from.y, to.y-from.y, height]]) {
    if (Math.abs(delta) < 1e-9) {
      if (origin < 0 || origin > limit) return null;
    } else {
      const a = -origin/delta, b = (limit-origin)/delta;
      start = Math.max(start, Math.min(a,b)); end = Math.min(end, Math.max(a,b));
    }
  }
  return start <= end ? [start,end] : null;
}

export function routeLabelLayout(from: Point, to: Point, width: number, height: number,
  textWidth: number, occupied: readonly LabelRect[], important: boolean): RouteLabelLayout | null {
  if (width < 32 || height < 40) return null;
  const interval = visibleInterval(from,to,width,height);
  if (!interval) return null;
  const [start,end] = interval;
  const fraction = important ? (start+end)/2 : 0.5;
  const anchor = {x:from.x+(to.x-from.x)*fraction, y:from.y+(to.y-from.y)*fraction};
  const preferred = {x:anchor.x, y:anchor.y-10};
  const overlap = (bounds: LabelRect) => occupied.reduce((sum, other) => sum +
    Math.max(0, Math.min(bounds.right,other.right)-Math.max(bounds.left,other.left)) *
    Math.max(0, Math.min(bounds.bottom,other.bottom)-Math.max(bounds.top,other.top)), 0);
  let baseline = preferred;
  if (important) {
    const inset = Math.min(textWidth/2+12, width/2), stepX = Math.max(textWidth+12, 32);
    const columns = Math.ceil(width/stepX), rows = Math.ceil(height/32);
    let bestOverlap = Infinity, bestDistance = Infinity;
    for (let row=-rows; row<=rows; row++) for (let column=-columns; column<=columns; column++) {
      const candidate = {x:Math.max(inset, Math.min(width-inset, preferred.x+column*stepX)),
        y:Math.max(25, Math.min(height-15, preferred.y+row*32))};
      const area = overlap(routeLabelBounds(candidate, textWidth));
      const distance = (candidate.x-preferred.x)**2+(candidate.y-preferred.y)**2;
      if (area < bestOverlap || (area === bestOverlap && distance < bestDistance)) {
        bestOverlap = area; bestDistance = distance; baseline = candidate;
      }
    }
  } else {
    const bounds = routeLabelBounds(baseline, textWidth);
    if (bounds.left < 8 || bounds.right > width-8 || bounds.top < 8 || bounds.bottom > height-8 || overlap(bounds)>0) return null;
  }
  const bounds = routeLabelBounds(baseline, textWidth);
  const leader = Math.abs(baseline.x-preferred.x)>1 || Math.abs(baseline.y-preferred.y)>1
    ? {x:Math.max(bounds.left, Math.min(bounds.right, anchor.x)), y:Math.max(bounds.top, Math.min(bounds.bottom, anchor.y))} : null;
  return {anchor, baseline, bounds, leader};
}

let measurement: CanvasRenderingContext2D | null = null;
export function measureRouteLabel(text: string, important = false): number {
  measurement ??= document.createElement("canvas").getContext("2d");
  if (!measurement) throw new Error("2D canvas context is required to measure route labels");
  measurement.font = `${important ? 800 : 600} 13px system-ui`;
  return measurement.measureText(text).width;
}
