// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub const FEET_PER_NM: f64 = 6076.12;
pub const FEET_PER_DEGREE_LAT: f64 = 60.0 * FEET_PER_NM;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AirspaceSegment {
    Line {
        to: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        clockwise: bool,
        to: [f64; 2],
    },
}

pub fn expand_airspace_path(start: [f64; 2], segments: &[AirspaceSegment]) -> Vec<[f64; 2]> {
    let mut points = Vec::with_capacity(segments.len() + 1);
    let mut current = start;
    points.push(current);
    for segment in segments {
        match segment {
            AirspaceSegment::Line { to } => {
                current = *to;
                points.push(current);
            }
            AirspaceSegment::Arc {
                center,
                clockwise,
                to,
            } => {
                append_airspace_arc_points(&mut points, current, *center, *to, *clockwise);
                current = *to;
            }
        }
    }
    points
}

pub fn append_airspace_arc_points(
    points: &mut Vec<[f64; 2]>,
    start: [f64; 2],
    center: [f64; 2],
    end: [f64; 2],
    clockwise: bool,
) {
    let projection = AirspaceLocalProjection::new(center[0], center[1]);
    let start_xy = projection.project(start);
    let end_xy = projection.project(end);
    let start_angle = start_xy[1].atan2(start_xy[0]);
    let end_angle = end_xy[1].atan2(end_xy[0]);
    let sweep = if clockwise {
        -positive_angle_delta(end_angle, start_angle)
    } else {
        positive_angle_delta(start_angle, end_angle)
    };
    let steps = ((sweep.abs().to_degrees() / 4.0).ceil() as usize).clamp(1, 180);
    let radius = (start_xy[0] * start_xy[0] + start_xy[1] * start_xy[1]).sqrt();
    for step in 1..=steps {
        let fraction = step as f64 / steps as f64;
        let angle = start_angle + sweep * fraction;
        let point = projection.unproject([radius * angle.cos(), radius * angle.sin()]);
        if points.last() != Some(&point) {
            points.push(point);
        }
    }
    if points.last() != Some(&end) {
        points.push(end);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AirspaceLocalProjection {
    origin_lon: f64,
    origin_lat: f64,
    feet_per_degree_lon: f64,
    feet_per_degree_lat: f64,
}

impl AirspaceLocalProjection {
    pub fn new(origin_lon: f64, origin_lat: f64) -> Self {
        let feet_per_degree_lon =
            FEET_PER_DEGREE_LAT * origin_lat.to_radians().cos().abs().max(0.001);
        Self {
            origin_lon,
            origin_lat,
            feet_per_degree_lon,
            feet_per_degree_lat: FEET_PER_DEGREE_LAT,
        }
    }

    pub fn project(&self, point: [f64; 2]) -> [f64; 2] {
        [
            (point[0] - self.origin_lon) * self.feet_per_degree_lon,
            (point[1] - self.origin_lat) * self.feet_per_degree_lat,
        ]
    }

    pub fn unproject(&self, point: [f64; 2]) -> [f64; 2] {
        [
            self.origin_lon + point[0] / self.feet_per_degree_lon,
            self.origin_lat + point[1] / self.feet_per_degree_lat,
        ]
    }
}

pub fn positive_angle_delta(from: f64, to: f64) -> f64 {
    (to - from).rem_euclid(std::f64::consts::TAU)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_line_and_arc_segments() {
        let start = [-120.0, 46.0];
        let center = [-120.0, 45.9];
        let end = [-119.9, 46.0];
        let points = expand_airspace_path(
            start,
            &[
                AirspaceSegment::Arc {
                    center,
                    clockwise: true,
                    to: end,
                },
                AirspaceSegment::Line { to: start },
            ],
        );
        assert_eq!(points.first(), Some(&start));
        assert_eq!(points.last(), Some(&start));
        assert!(points.len() > 3);
    }
}
