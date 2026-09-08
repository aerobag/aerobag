// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::UiGeometryPoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiLabelRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRouteLabelLayout {
    pub anchor: UiGeometryPoint,
    pub baseline: UiGeometryPoint,
    pub bounds: UiLabelRect,
    pub leader: Option<UiGeometryPoint>,
}

pub struct UiRouteLabelCandidate<'a> {
    pub from: UiGeometryPoint,
    pub to: UiGeometryPoint,
    pub label: &'a str,
    pub important: bool,
}

/// One protected callout per continuous identical constraint. Choose the longest
/// visible member so zooming/panning never leaves an offscreen representative.
pub fn ui_route_label_indices(
    legs: &[UiRouteLabelCandidate<'_>],
    width: f64,
    height: f64,
) -> Vec<usize> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < legs.len() {
        let first = index;
        index += 1;
        if !legs[first].important {
            result.push(first);
            continue;
        }
        while index < legs.len()
            && legs[index].important
            && legs[index].label == legs[first].label
            && legs[index - 1].to == legs[index].from
        {
            index += 1;
        }
        let mut best = None;
        let mut length = -1.0;
        for (offset, leg) in legs[first..index].iter().enumerate() {
            if let Some((start, end)) = visible_interval(leg.from, leg.to, width, height) {
                let visible_length =
                    (leg.to.x - leg.from.x).hypot(leg.to.y - leg.from.y) * (end - start);
                // Treat subpixel differences as ties across f32/f64 renderers.
                if visible_length > length + 1e-4 {
                    length = visible_length;
                    best = Some(first + offset);
                }
            }
        }
        if let Some(best) = best {
            result.push(best);
        }
    }
    result.sort_by_key(|index| !legs[*index].important);
    result
}

fn visible_interval(
    from: UiGeometryPoint,
    to: UiGeometryPoint,
    width: f64,
    height: f64,
) -> Option<(f64, f64)> {
    let (mut start, mut end) = (0.0_f64, 1.0_f64);
    for (origin, delta, limit) in [
        (from.x, to.x - from.x, width),
        (from.y, to.y - from.y, height),
    ] {
        if delta.abs() < 1e-9 {
            if origin < 0.0 || origin > limit {
                return None;
            }
        } else {
            let a = -origin / delta;
            let b = (limit - origin) / delta;
            start = start.max(a.min(b));
            end = end.min(a.max(b));
        }
    }
    (start <= end).then_some((start, end))
}

/// All dimensions use CSS pixels / density-independent pixels in the displayed
/// (bearing-aware) map frame. Bounds include the text halo and collision padding.
pub fn ui_route_label_bounds(baseline: UiGeometryPoint, text_width: f64) -> UiLabelRect {
    UiLabelRect {
        left: baseline.x - text_width / 2.0 - 4.0,
        top: baseline.y - 17.0,
        right: baseline.x + text_width / 2.0 + 4.0,
        bottom: baseline.y + 7.0,
    }
}

/// Important labels are never discarded for a collision. Search for the nearest
/// clear position, then fall back to the least overlap if the viewport is full.
/// Ordinary labels have one candidate and yield to all previously placed labels.
pub fn ui_route_label_layout(
    from: UiGeometryPoint,
    to: UiGeometryPoint,
    width: f64,
    height: f64,
    text_width: f64,
    occupied: &[UiLabelRect],
    important: bool,
) -> Option<UiRouteLabelLayout> {
    if width < 32.0 || height < 40.0 {
        return None;
    }
    // Clip the segment, not its midpoint: a visible highest-MEA leg needs a
    // callout even when its original midpoint lies beyond the screen edge.
    let (start, end) = visible_interval(from, to, width, height)?;
    let fraction = if important { (start + end) / 2.0 } else { 0.5 };
    let anchor = UiGeometryPoint {
        x: from.x + (to.x - from.x) * fraction,
        y: from.y + (to.y - from.y) * fraction,
    };
    let preferred = UiGeometryPoint {
        x: anchor.x,
        y: anchor.y - 10.0,
    };
    let overlap = |bounds: UiLabelRect| {
        occupied
            .iter()
            .map(|other| {
                (bounds.right.min(other.right) - bounds.left.max(other.left)).max(0.0)
                    * (bounds.bottom.min(other.bottom) - bounds.top.max(other.top)).max(0.0)
            })
            .sum::<f64>()
    };
    let mut baseline = preferred;
    if important {
        let inset = (text_width / 2.0 + 12.0).min(width / 2.0);
        let step_x = (text_width + 12.0).max(32.0);
        let columns = (width / step_x).ceil() as i32;
        let rows = (height / 32.0).ceil() as i32;
        let mut best = (f64::INFINITY, f64::INFINITY);
        for row in -rows..=rows {
            for column in -columns..=columns {
                let candidate = UiGeometryPoint {
                    x: (preferred.x + f64::from(column) * step_x).clamp(inset, width - inset),
                    y: (preferred.y + f64::from(row) * 32.0).clamp(25.0, height - 15.0),
                };
                let score = (
                    overlap(ui_route_label_bounds(candidate, text_width)),
                    (candidate.x - preferred.x).powi(2) + (candidate.y - preferred.y).powi(2),
                );
                if score < best {
                    best = score;
                    baseline = candidate;
                }
            }
        }
    } else {
        let bounds = ui_route_label_bounds(baseline, text_width);
        if bounds.left < 8.0
            || bounds.right > width - 8.0
            || bounds.top < 8.0
            || bounds.bottom > height - 8.0
            || overlap(bounds) > 0.0
        {
            return None;
        }
    }
    let bounds = ui_route_label_bounds(baseline, text_width);
    let leader = ((baseline.x - preferred.x).abs() > 1.0 || (baseline.y - preferred.y).abs() > 1.0)
        .then_some(UiGeometryPoint {
            x: anchor.x.clamp(bounds.left, bounds.right),
            y: anchor.y.clamp(bounds.top, bounds.bottom),
        });
    Some(UiRouteLabelLayout {
        anchor,
        baseline,
        bounds,
        leader,
    })
}
