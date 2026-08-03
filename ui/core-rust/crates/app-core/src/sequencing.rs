// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{FlightPlanRouteFinishLine, LatLon};

const EARTH_RADIUS_NM: f64 = 3440.065;
// A bisector is an infinite plane, but only the portion near its route intersection is a
// meaningful sequencing boundary. Debug rendering and crossing detection share this limit.
pub(crate) const FINISH_BOUNDARY_LOCALITY_NM: f64 = 10.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SequencingFinishPlane {
    point: LatLon,
    normal_course_deg: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SequencingFinishCriterion {
    Plane(SequencingFinishPlane),
    ArcSector {
        center: LatLon,
        finish_point: LatLon,
        finish_bearing_deg: f64,
        untraveled_mid_bearing_deg: f64,
        clockwise: bool,
    },
}

impl SequencingFinishCriterion {
    pub(crate) fn crossed_by(self, previous: LatLon, current: LatLon) -> bool {
        match self {
            Self::Plane(plane) => plane_crossed_by(plane, previous, current),
            Self::ArcSector {
                center,
                finish_point,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                clockwise,
            } => arc_sector_crossed_by(
                center,
                finish_point,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                clockwise,
                previous,
                current,
            ),
        }
    }

    pub(crate) fn finish_lines(self) -> Vec<FlightPlanRouteFinishLine> {
        match self {
            Self::Plane(plane) => {
                let line_course = normalize_course_degrees(plane.normal_course_deg + 90.0);
                vec![FlightPlanRouteFinishLine {
                    start: destination_point(
                        plane.point,
                        line_course + 180.0,
                        FINISH_BOUNDARY_LOCALITY_NM,
                    ),
                    end: destination_point(plane.point, line_course, FINISH_BOUNDARY_LOCALITY_NM),
                }]
            }
            Self::ArcSector {
                center,
                finish_point,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                ..
            } => [finish_bearing_deg, untraveled_mid_bearing_deg]
                .into_iter()
                .filter_map(|bearing| {
                    ray_segment_within_finish_locality(center, bearing, finish_point)
                })
                .collect(),
        }
    }
}

pub(crate) fn plane_finish_criterion(
    point: LatLon,
    inbound_course_deg: f64,
    outbound_course_deg: f64,
) -> SequencingFinishCriterion {
    SequencingFinishCriterion::Plane(SequencingFinishPlane {
        point,
        normal_course_deg: finish_line_normal_course_deg(inbound_course_deg, outbound_course_deg),
    })
}

pub(crate) fn arc_finish_criterion(
    center: LatLon,
    end: LatLon,
    clockwise: bool,
    sweep_degrees: f64,
) -> Option<SequencingFinishCriterion> {
    let sweep = sweep_degrees.abs().min(360.0);
    let untraveled_sweep = 360.0 - sweep;
    if untraveled_sweep <= 1e-6 {
        return None;
    }
    let finish_bearing_deg = crate::initial_course_deg(center, end);
    let untraveled_mid_bearing_deg = if clockwise {
        normalize_course_degrees(finish_bearing_deg + untraveled_sweep / 2.0)
    } else {
        normalize_course_degrees(finish_bearing_deg - untraveled_sweep / 2.0)
    };
    Some(SequencingFinishCriterion::ArcSector {
        center,
        finish_point: end,
        finish_bearing_deg,
        untraveled_mid_bearing_deg,
        clockwise,
    })
}

fn finish_line_normal_course_deg(inbound_course_deg: f64, outbound_course_deg: f64) -> f64 {
    let inbound = inbound_course_deg.to_radians();
    let outbound = outbound_course_deg.to_radians();
    let x = inbound.sin() + outbound.sin();
    let y = inbound.cos() + outbound.cos();
    if x.hypot(y) < 1e-9 {
        normalize_course_degrees(inbound_course_deg)
    } else {
        normalize_course_degrees(x.atan2(y).to_degrees())
    }
}

fn plane_crossed_by(
    finish_plane: SequencingFinishPlane,
    previous: LatLon,
    current: LatLon,
) -> bool {
    let previous_offset = local_offset_nm(finish_plane.point, previous);
    let current_offset = local_offset_nm(finish_plane.point, current);
    let normal = bearing_unit_vector(finish_plane.normal_course_deg);
    let previous_side = dot(previous_offset, normal);
    let current_side = dot(current_offset, normal);
    // Sequencing is an observed crossing, not merely a position some arbitrary distance past
    // the plane. This also prevents activation while already beyond a future finish boundary.
    if previous_side > 0.0 || current_side <= 0.0 {
        return false;
    }

    let crossing_fraction = previous_side / (previous_side - current_side);
    let crossing = add(
        previous_offset,
        scale(subtract(current_offset, previous_offset), crossing_fraction),
    );
    magnitude(crossing) <= FINISH_BOUNDARY_LOCALITY_NM
}

#[allow(clippy::too_many_arguments)]
fn arc_sector_crossed_by(
    center: LatLon,
    finish_point: LatLon,
    finish_bearing_deg: f64,
    untraveled_mid_bearing_deg: f64,
    clockwise: bool,
    previous: LatLon,
    current: LatLon,
) -> bool {
    let previous_offset = local_offset_nm(center, previous);
    let current_offset = local_offset_nm(center, current);
    let motion = subtract(current_offset, previous_offset);
    if magnitude(motion) <= f64::EPSILON {
        return false;
    }
    let finish_offset = local_offset_nm(center, finish_point);

    [finish_bearing_deg, untraveled_mid_bearing_deg]
        .into_iter()
        .filter_map(|boundary_bearing| {
            motion_ray_intersection_fraction(
                previous_offset,
                motion,
                bearing_unit_vector(boundary_bearing),
            )
        })
        .any(|crossing_fraction| {
            // Reaching the boundary exactly is not yet passing it.
            if !(-1e-9..1.0 - 1e-9).contains(&crossing_fraction) {
                return false;
            }
            let crossing = add(
                previous_offset,
                scale(motion, crossing_fraction.clamp(0.0, 1.0)),
            );
            if magnitude(subtract(crossing, finish_offset)) > FINISH_BOUNDARY_LOCALITY_NM {
                return false;
            }

            let sample_delta = 1e-6;
            let before = add(
                previous_offset,
                scale(motion, crossing_fraction - sample_delta),
            );
            let after = add(
                previous_offset,
                scale(motion, crossing_fraction + sample_delta),
            );
            !offset_is_in_arc_finish_sector(
                before,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                clockwise,
            ) && offset_is_in_arc_finish_sector(
                after,
                finish_bearing_deg,
                untraveled_mid_bearing_deg,
                clockwise,
            )
        })
}

fn motion_ray_intersection_fraction(
    motion_start: (f64, f64),
    motion: (f64, f64),
    ray_direction: (f64, f64),
) -> Option<f64> {
    let denominator = cross(motion, ray_direction);
    if denominator.abs() <= 1e-12 {
        return None;
    }
    let motion_fraction = -cross(motion_start, ray_direction) / denominator;
    let intersection = add(motion_start, scale(motion, motion_fraction));
    (dot(intersection, ray_direction) >= -1e-9).then_some(motion_fraction)
}

fn offset_is_in_arc_finish_sector(
    offset: (f64, f64),
    finish_bearing_deg: f64,
    untraveled_mid_bearing_deg: f64,
    clockwise: bool,
) -> bool {
    if magnitude(offset) <= f64::EPSILON {
        return false;
    }
    bearing_is_in_arc_finish_sector(
        normalize_course_degrees(offset.0.atan2(offset.1).to_degrees()),
        finish_bearing_deg,
        untraveled_mid_bearing_deg,
        clockwise,
    )
}

fn bearing_is_in_arc_finish_sector(
    bearing_deg: f64,
    finish_bearing_deg: f64,
    untraveled_mid_bearing_deg: f64,
    clockwise: bool,
) -> bool {
    let sector_width = if clockwise {
        clockwise_delta_degrees(finish_bearing_deg, untraveled_mid_bearing_deg)
    } else {
        clockwise_delta_degrees(untraveled_mid_bearing_deg, finish_bearing_deg)
    };
    let distance_into_sector = if clockwise {
        clockwise_delta_degrees(finish_bearing_deg, bearing_deg)
    } else {
        clockwise_delta_degrees(bearing_deg, finish_bearing_deg)
    };
    distance_into_sector <= sector_width + 1e-6
}

fn clockwise_delta_degrees(from_deg: f64, to_deg: f64) -> f64 {
    (normalize_course_degrees(to_deg) - normalize_course_degrees(from_deg)).rem_euclid(360.0)
}

fn ray_segment_within_finish_locality(
    center: LatLon,
    bearing_deg: f64,
    finish_point: LatLon,
) -> Option<FlightPlanRouteFinishLine> {
    let finish_offset = local_offset_nm(center, finish_point);
    let ray = bearing_unit_vector(bearing_deg);
    let projected_distance = dot(finish_offset, ray);
    let perpendicular_distance_sq = magnitude(finish_offset).powi(2) - projected_distance.powi(2);
    let remaining_radius_sq =
        FINISH_BOUNDARY_LOCALITY_NM.powi(2) - perpendicular_distance_sq.max(0.0);
    if remaining_radius_sq < 0.0 {
        return None;
    }
    let along_radius = remaining_radius_sq.sqrt();
    let start_distance = (projected_distance - along_radius).max(0.0);
    let end_distance = projected_distance + along_radius;
    if end_distance < start_distance {
        return None;
    }
    Some(FlightPlanRouteFinishLine {
        start: destination_point(center, bearing_deg, start_distance),
        end: destination_point(center, bearing_deg, end_distance),
    })
}

fn local_offset_nm(origin: LatLon, position: LatLon) -> (f64, f64) {
    let delta_lon_deg = (position.lon - origin.lon + 540.0).rem_euclid(360.0) - 180.0;
    let east_nm = delta_lon_deg.to_radians() * origin.lat.to_radians().cos() * EARTH_RADIUS_NM;
    let north_nm = (position.lat - origin.lat).to_radians() * EARTH_RADIUS_NM;
    (east_nm, north_nm)
}

fn destination_point(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    let angular_distance = distance_nm / EARTH_RADIUS_NM;
    let bearing_rad = bearing_deg.to_radians();
    let lat1 = origin.lat.to_radians();
    let lon1 = origin.lon.to_radians();
    let lat2 = (lat1.sin() * angular_distance.cos()
        + lat1.cos() * angular_distance.sin() * bearing_rad.cos())
    .asin();
    let lon2 = lon1
        + (bearing_rad.sin() * angular_distance.sin() * lat1.cos())
            .atan2(angular_distance.cos() - lat1.sin() * lat2.sin());
    LatLon {
        lat: lat2.to_degrees(),
        lon: (lon2.to_degrees() + 540.0).rem_euclid(360.0) - 180.0,
    }
}

fn bearing_unit_vector(bearing_deg: f64) -> (f64, f64) {
    let bearing = bearing_deg.to_radians();
    (bearing.sin(), bearing.cos())
}

fn normalize_course_degrees(course_deg: f64) -> f64 {
    course_deg.rem_euclid(360.0)
}

fn add(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 + right.0, left.1 + right.1)
}

fn subtract(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 - right.0, left.1 - right.1)
}

fn scale(value: (f64, f64), factor: f64) -> (f64, f64) {
    (value.0 * factor, value.1 * factor)
}

fn dot(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.0 + left.1 * right.1
}

fn cross(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.1 - left.1 * right.0
}

fn magnitude(value: (f64, f64)) -> f64 {
    value.0.hypot(value.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn east_of(origin: LatLon, distance_nm: f64) -> LatLon {
        destination_point(origin, 90.0, distance_nm)
    }

    fn north_of(origin: LatLon, distance_nm: f64) -> LatLon {
        destination_point(origin, 0.0, distance_nm)
    }

    fn at_bearing(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
        destination_point(origin, bearing_deg, distance_nm)
    }

    #[test]
    fn plane_sequences_immediately_after_an_actual_crossing() {
        let finish = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let criterion = plane_finish_criterion(finish, 90.0, 90.0);

        assert!(criterion.crossed_by(east_of(finish, -0.01), east_of(finish, 0.01)));
    }

    #[test]
    fn plane_does_not_sequence_without_a_crossing() {
        let finish = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let criterion = plane_finish_criterion(finish, 90.0, 90.0);

        assert!(!criterion.crossed_by(east_of(finish, 0.01), east_of(finish, 0.02)));
    }

    #[test]
    fn plane_rejects_a_crossing_more_than_ten_nm_from_the_finish_point() {
        let finish = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let criterion = plane_finish_criterion(finish, 90.0, 90.0);

        assert!(!criterion.crossed_by(
            north_of(east_of(finish, -1.0), 10.1),
            north_of(east_of(finish, 1.0), 10.1),
        ));
    }

    #[test]
    fn plane_debug_line_matches_the_ten_nm_crossing_boundary() {
        let finish = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let line = plane_finish_criterion(finish, 90.0, 90.0)
            .finish_lines()
            .remove(0);

        assert!((crate::great_circle_distance_nm(finish, line.start) - 10.0).abs() < 1e-6);
        assert!((crate::great_circle_distance_nm(finish, line.end) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn arc_sequences_when_motion_crosses_into_its_finish_sector() {
        let center = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let finish = at_bearing(center, 210.0, 1.0);
        let criterion = arc_finish_criterion(center, finish, true, 210.0).expect("arc criterion");

        assert!(criterion.crossed_by(
            at_bearing(center, 200.0, 1.0),
            at_bearing(center, 220.0, 1.0),
        ));
        assert!(!criterion.crossed_by(
            at_bearing(center, 220.0, 1.0),
            at_bearing(center, 230.0, 1.0),
        ));
    }

    #[test]
    fn arc_rejects_a_finish_sector_crossing_outside_its_finite_boundaries() {
        let center = LatLon {
            lat: 40.0,
            lon: -120.0,
        };
        let finish = at_bearing(center, 210.0, 1.0);
        let criterion = arc_finish_criterion(center, finish, true, 210.0).expect("arc criterion");

        assert!(!criterion.crossed_by(
            at_bearing(center, 200.0, 20.0),
            at_bearing(center, 220.0, 20.0),
        ));
    }
}
