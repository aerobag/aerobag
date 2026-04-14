use crate::{LatLon, LegDisplayElement, LegDisplayPath, ProcedureLegMaterializationRecord};
use crate::planning::LegDisplayPathStyle;

const NOMINAL_HOLD_GROUND_SPEED_KT: f64 = 120.0;
const STANDARD_RATE_TURN_DEG_PER_SEC: f64 = 3.0;
const NOMINAL_PROCEDURE_TURN_INITIAL_OUTBOUND_DISTANCE_NM: f64 = 5.0;
const NOMINAL_PROCEDURE_TURN_GROUND_SPEED_KT: f64 = 120.0;
const NOMINAL_PROCEDURE_TURN_BARB_TIME_MIN: f64 = 2.0;

pub fn display_path_for_procedure_leg(
    segment_records: &[ProcedureLegMaterializationRecord],
    leg_start: &ProcedureLegMaterializationRecord,
    leg_end: &ProcedureLegMaterializationRecord,
    hold_record: Option<&ProcedureLegMaterializationRecord>,
) -> Option<LegDisplayPath> {
    if leg_end.path_termination.trim() == "PI" {
        return procedure_turn_display_path(leg_end);
    }
    let arrival_course_deg = procedure_arrival_course_deg(leg_start, leg_end);
    if let Some(hold) = hold_record {
        if let Some(path) = missed_approach_display_path(segment_records, leg_start, leg_end, hold) {
            return Some(path);
        }
        let mut path = hold_display_path(hold, arrival_course_deg)?;
        if hold.path_termination.trim() == "HF" {
            let start = leg_start.nav_position?;
            let fix = hold.nav_position?;
            if distance_between_points_nm(start, fix) > 0.05 {
                path.elements.insert(0, LegDisplayElement::Segment { start, end: fix });
            }
        }
        return Some(path);
    }
    None
}

fn hold_display_path(
    leg: &ProcedureLegMaterializationRecord,
    arrival_course_deg: Option<f64>,
) -> Option<LegDisplayPath> {
    let hold_kind = leg.path_termination.trim();
    if !matches!(hold_kind, "HF" | "HM") {
        return None;
    }
    let fix = leg.nav_position?;
    let inbound_course_deg = leg.magnetic_course_deg? + course_reference_variation_deg(leg);
    let turn_direction = leg.turn_direction.as_deref().unwrap_or("").trim();
    let clockwise = match turn_direction {
        "L" => false,
        "R" => true,
        _ => return None,
    };
    let leg_length_nm = nominal_hold_leg_length_nm(leg.route_distance_or_time.as_deref())?;
    let turn_radius_nm = nominal_standard_rate_turn_radius_nm();
    Some(build_hold_display_path(
        fix,
        inbound_course_deg,
        clockwise,
        leg_length_nm,
        turn_radius_nm,
        arrival_course_deg,
        hold_kind == "HF",
    ))
}

fn procedure_arrival_course_deg(
    leg_start: &ProcedureLegMaterializationRecord,
    leg_end: &ProcedureLegMaterializationRecord,
) -> Option<f64> {
    Some(bearing_from(leg_start.nav_position?, leg_end.nav_position?))
}

fn procedure_turn_display_path(leg: &ProcedureLegMaterializationRecord) -> Option<LegDisplayPath> {
    let fix = leg.nav_position?;
    let barb_course_deg = leg.magnetic_course_deg? + course_reference_variation_deg(leg);
    let clockwise = match leg.turn_direction.as_deref().unwrap_or("").trim() {
        "L" => false,
        "R" => true,
        _ => return None,
    };
    let outbound_course_deg = if clockwise {
        normalize_bearing_degrees(barb_course_deg + 45.0)
    } else {
        normalize_bearing_degrees(barb_course_deg - 45.0)
    };
    let outbound_end = destination_point(
        fix,
        outbound_course_deg,
        NOMINAL_PROCEDURE_TURN_INITIAL_OUTBOUND_DISTANCE_NM,
    );
    let barb_end = destination_point(
        outbound_end,
        barb_course_deg,
        nominal_procedure_turn_barb_distance_nm(),
    );
    let turn_radius_nm = missed_approach_turn_radius_nm();
    let turn_center = turn_center_for_heading_change(
        barb_end,
        barb_course_deg,
        clockwise,
        turn_radius_nm,
    );
    let return_heading_deg = normalize_bearing_degrees(barb_course_deg + 180.0);
    let intercept_start = point_on_turn_center(
        turn_center,
        return_heading_deg,
        clockwise,
        turn_radius_nm,
    );
    let inbound_course_deg = normalize_bearing_degrees(outbound_course_deg + 180.0);
    let intercept = intersect_heading_with_course(
        intercept_start,
        return_heading_deg,
        fix,
        inbound_course_deg,
        fix,
    )?;
    Some(LegDisplayPath {
        style: LegDisplayPathStyle::Dashed,
        elements: vec![
            LegDisplayElement::Segment {
                start: fix,
                end: outbound_end,
            },
            LegDisplayElement::Segment {
                start: outbound_end,
                end: barb_end,
            },
            LegDisplayElement::Arc {
                center: turn_center,
                radius_nm: turn_radius_nm,
                start: barb_end,
                end: intercept_start,
                clockwise,
                sweep_degrees: 180.0,
            },
            LegDisplayElement::Segment {
                start: intercept_start,
                end: intercept,
            },
            LegDisplayElement::Segment {
                start: intercept,
                end: fix,
            },
        ],
    })
}

fn nominal_procedure_turn_barb_distance_nm() -> f64 {
    NOMINAL_PROCEDURE_TURN_GROUND_SPEED_KT * (NOMINAL_PROCEDURE_TURN_BARB_TIME_MIN / 60.0)
}

const NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT: f64 = 90.0;
const NOMINAL_MISSED_APPROACH_CLIMB_FTPM: f64 = 500.0;

fn missed_approach_display_path(
    segment_records: &[ProcedureLegMaterializationRecord],
    leg_start: &ProcedureLegMaterializationRecord,
    _leg_end: &ProcedureLegMaterializationRecord,
    hold_record: &ProcedureLegMaterializationRecord,
) -> Option<LegDisplayPath> {
    if hold_record.path_termination.trim() != "HM" {
        return None;
    }
    let mut current_position = leg_start.nav_position?;
    let mut current_course_deg =
        leg_start.magnetic_course_deg.map(|course| course + course_reference_variation_deg(leg_start));
    let mut current_altitude_ft = leg_start.altitude_1_ft;
    let mut elements = Vec::new();

    let mut steps = segment_records
        .iter()
        .filter(|record| record.sequence > leg_start.sequence && record.sequence <= hold_record.sequence)
        .collect::<Vec<_>>();
    steps.sort_by_key(|record| record.sequence);

    for step in steps {
        match step.path_termination.trim() {
            "CA" => {
                let course_deg = if step.defining_nav_ref.is_none() && step.nav_ref.is_none() {
                    current_course_deg.or_else(|| {
                        step.magnetic_course_deg
                            .map(|course| course + course_reference_variation_deg(step))
                    })?
                } else {
                    step.magnetic_course_deg? + course_reference_variation_deg(step)
                };
                let start_alt_ft = current_altitude_ft?;
                let target_alt_ft = step.altitude_1_ft?;
                let climb_minutes =
                    ((target_alt_ft - start_alt_ft).max(0.0)) / NOMINAL_MISSED_APPROACH_CLIMB_FTPM;
                let climb_distance_nm = NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT * (climb_minutes / 60.0);
                let climb_end = destination_point(current_position, course_deg, climb_distance_nm);
                elements.push(LegDisplayElement::Segment {
                    start: current_position,
                    end: climb_end,
                });
                current_position = climb_end;
                current_course_deg = Some(course_deg);
                current_altitude_ft = Some(target_alt_ft);
            }
            "VI" => {
                let initial_course_deg = current_course_deg?;
                let turn_heading_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
                let turn_clockwise = step.turn_direction.as_deref().unwrap_or("").trim() == "R";
                let turn_radius_nm = missed_approach_turn_radius_nm();
                let turn_center = turn_center_for_heading_change(
                    current_position,
                    initial_course_deg,
                    turn_clockwise,
                    turn_radius_nm,
                );
                let turn_end = point_on_turn_center(
                    turn_center,
                    turn_heading_deg,
                    turn_clockwise,
                    turn_radius_nm,
                );
                elements.push(LegDisplayElement::Arc {
                    center: turn_center,
                    radius_nm: turn_radius_nm,
                    start: current_position,
                    end: turn_end,
                    clockwise: turn_clockwise,
                    sweep_degrees: heading_sweep_degrees(
                        initial_course_deg,
                        turn_heading_deg,
                        turn_clockwise,
                    ),
                });
                current_position = turn_end;
                current_course_deg = Some(turn_heading_deg);
            }
            "DF" => {
                let fix = step.nav_position?;
                let direct_course_deg = bearing_from(current_position, fix);
                let turn_clockwise = match step.turn_direction.as_deref().unwrap_or("").trim() {
                    "L" => false,
                    "R" => true,
                    _ => shortest_turn_clockwise(current_course_deg?, direct_course_deg),
                };
                let initial_course_deg = current_course_deg?;
                if angular_difference_degrees(initial_course_deg, direct_course_deg) > 1.0 {
                    let turn_radius_nm = missed_approach_turn_radius_nm();
                    let turn_center = turn_center_for_heading_change(
                        current_position,
                        initial_course_deg,
                        turn_clockwise,
                        turn_radius_nm,
                    );
                    let turn_end = point_on_turn_center(
                        turn_center,
                        direct_course_deg,
                        turn_clockwise,
                        turn_radius_nm,
                    );
                    elements.push(LegDisplayElement::Arc {
                        center: turn_center,
                        radius_nm: turn_radius_nm,
                        start: current_position,
                        end: turn_end,
                        clockwise: turn_clockwise,
                        sweep_degrees: heading_sweep_degrees(
                            initial_course_deg,
                            direct_course_deg,
                            turn_clockwise,
                        ),
                    });
                    current_position = turn_end;
                }
                elements.push(LegDisplayElement::Segment {
                    start: current_position,
                    end: fix,
                });
                current_position = fix;
                current_course_deg = Some(direct_course_deg);
            }
            "CF" => {
                let fix = step.nav_position?;
                let course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
                if let Some(defining_nav) = step.defining_nav_position {
                    if let Some(current_heading_deg) = current_course_deg {
                        if let Some(turn_clockwise) = cf_turn_direction(step) {
                            if let Some(directed_elements) = directed_cf_join_elements(
                                current_position,
                                current_heading_deg,
                                turn_clockwise,
                                defining_nav,
                                course_deg,
                                fix,
                                missed_approach_turn_radius_nm(),
                            ) {
                                elements.extend(directed_elements);
                            } else {
                                elements.push(LegDisplayElement::Segment {
                                    start: current_position,
                                    end: fix,
                                });
                            }
                        } else {
                            let intercept = intersect_heading_with_course(
                                current_position,
                                current_heading_deg,
                                defining_nav,
                                course_deg,
                                fix,
                            )?;
                            let intercept_is_reasonable =
                                cf_intercept_is_reasonable(intercept, defining_nav, course_deg, fix);
                            if intercept_is_reasonable {
                                if distance_between_points_nm(current_position, intercept) > 0.05 {
                                    elements.push(LegDisplayElement::Segment {
                                        start: current_position,
                                        end: intercept,
                                    });
                                }
                                if distance_between_points_nm(intercept, fix) > 0.05 {
                                    elements.push(LegDisplayElement::Segment {
                                        start: intercept,
                                        end: fix,
                                    });
                                }
                            } else {
                                elements.push(LegDisplayElement::Segment {
                                    start: current_position,
                                    end: fix,
                                });
                            }
                        }
                    } else {
                        elements.push(LegDisplayElement::Segment {
                            start: current_position,
                            end: fix,
                        });
                    }
                } else {
                    elements.push(LegDisplayElement::Segment {
                        start: current_position,
                        end: fix,
                    });
                }
                current_position = fix;
                current_course_deg = Some(course_deg);
            }
            "TF" => {
                let fix = step.nav_position?;
                if distance_between_points_nm(current_position, fix) > 0.05 {
                    elements.push(LegDisplayElement::Segment {
                        start: current_position,
                        end: fix,
                    });
                }
                current_course_deg = Some(bearing_from(current_position, fix));
                current_position = fix;
            }
            "HM" => {
                if let Some(hold_path) = hold_display_path(step, current_course_deg) {
                    elements.extend(hold_path.elements);
                }
            }
            _ => {}
        }
    }
    Some(LegDisplayPath { style: LegDisplayPathStyle::Solid, elements })
}

fn course_reference_variation_deg(leg: &ProcedureLegMaterializationRecord) -> f64 {
    if matches!(leg.defining_nav_ref, Some(crate::NavRef::Navaid(_))) {
        return leg
            .defining_nav_magnetic_variation_deg
            .or(leg.nav_magnetic_variation_deg)
            .or(leg.airport_magnetic_variation_deg)
            .unwrap_or(0.0);
    }
    if matches!(leg.nav_ref, Some(crate::NavRef::Navaid(_))) {
        return leg
            .nav_magnetic_variation_deg
            .or(leg.airport_magnetic_variation_deg)
            .unwrap_or(0.0);
    }
    leg.airport_magnetic_variation_deg.unwrap_or(0.0)
}

fn missed_approach_turn_radius_nm() -> f64 {
    let speed_nm_per_sec = NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT / 3600.0;
    let rate_rad_per_sec = STANDARD_RATE_TURN_DEG_PER_SEC.to_radians();
    speed_nm_per_sec / rate_rad_per_sec
}

fn turn_center_for_heading_change(
    turn_start: LatLon,
    initial_course_deg: f64,
    clockwise: bool,
    radius_nm: f64,
) -> LatLon {
    let initial = bearing_unit_vector(initial_course_deg);
    let normal = if clockwise {
        right_normal(initial)
    } else {
        left_normal(initial)
    };
    offset_latlon(turn_start, normal.0 * radius_nm, normal.1 * radius_nm)
}

fn point_on_turn_center(
    center: LatLon,
    tangent_course_deg: f64,
    clockwise: bool,
    radius_nm: f64,
) -> LatLon {
    let tangent = bearing_unit_vector(tangent_course_deg);
    let normal = if clockwise {
        right_normal(tangent)
    } else {
        left_normal(tangent)
    };
    offset_latlon(center, -normal.0 * radius_nm, -normal.1 * radius_nm)
}

fn tangent_rejoin_from_turn(
    turn_center: LatLon,
    radius_nm: f64,
    initial_course_deg: f64,
    clockwise: bool,
    fix: LatLon,
) -> Option<(LatLon, f64)> {
    let fix_en = to_local_en(turn_center, fix);
    let distance_sq = fix_en.0 * fix_en.0 + fix_en.1 * fix_en.1;
    if distance_sq <= radius_nm * radius_nm {
        return None;
    }
    let scale = (radius_nm * radius_nm) / distance_sq;
    let offset_scale = radius_nm * (distance_sq - radius_nm * radius_nm).sqrt() / distance_sq;
    let candidates = [
        (
            scale * fix_en.0 - offset_scale * fix_en.1,
            scale * fix_en.1 + offset_scale * fix_en.0,
        ),
        (
            scale * fix_en.0 + offset_scale * fix_en.1,
            scale * fix_en.1 - offset_scale * fix_en.0,
        ),
    ];
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let turn_end = offset_latlon(turn_center, candidate.0, candidate.1);
            let rejoin_course_deg = bearing_from(turn_end, fix);
            let sweep_degrees =
                heading_sweep_degrees(initial_course_deg, rejoin_course_deg, clockwise);
            if sweep_degrees < 1.0 {
                return None;
            }
            Some((turn_end, rejoin_course_deg, sweep_degrees))
        })
        .max_by(|left, right| left.2.partial_cmp(&right.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(turn_end, rejoin_course_deg, _)| (turn_end, rejoin_course_deg))
}

fn cf_turn_direction(step: &ProcedureLegMaterializationRecord) -> Option<bool> {
    match step.turn_direction.as_deref().unwrap_or("").trim() {
        "L" => Some(false),
        "R" => Some(true),
        _ => None,
    }
}

fn directed_cf_join_elements(
    current_position: LatLon,
    current_heading_deg: f64,
    turn_clockwise: bool,
    course_anchor: LatLon,
    course_deg: f64,
    fix: LatLon,
    turn_radius_nm: f64,
) -> Option<Vec<LegDisplayElement>> {
    let heading_change_deg = angular_difference_degrees(current_heading_deg, course_deg);
    if heading_change_deg >= 175.0 {
        let turn_center = turn_center_for_heading_change(
            current_position,
            current_heading_deg,
            turn_clockwise,
            turn_radius_nm,
        );
        let turn_end = point_on_turn_center(
            turn_center,
            course_deg,
            turn_clockwise,
            turn_radius_nm,
        );
        let rejoin_course_deg = bearing_from(turn_end, fix);
        if angular_difference_degrees(rejoin_course_deg, course_deg) > 20.0 {
            return None;
        }
        let mut elements = vec![LegDisplayElement::Arc {
            center: turn_center,
            radius_nm: turn_radius_nm,
            start: current_position,
            end: turn_end,
            clockwise: turn_clockwise,
            sweep_degrees: heading_sweep_degrees(
                current_heading_deg,
                course_deg,
                turn_clockwise,
            ),
        }];
        if distance_between_points_nm(turn_end, fix) > 0.05 {
            elements.push(LegDisplayElement::Segment {
                start: turn_end,
                end: fix,
            });
        }
        return Some(elements);
    }
    let initial_dir = bearing_unit_vector(current_heading_deg);
    let initial_normal = if turn_clockwise {
        right_normal(initial_dir)
    } else {
        left_normal(initial_dir)
    };
    let final_dir = bearing_unit_vector(course_deg);
    let final_normal = if turn_clockwise {
        right_normal(final_dir)
    } else {
        left_normal(final_dir)
    };
    let base_turn_end = offset_latlon(
        current_position,
        (initial_normal.0 - final_normal.0) * turn_radius_nm,
        (initial_normal.1 - final_normal.1) * turn_radius_nm,
    );
    let turn_end = intersect_lines(base_turn_end, initial_dir, course_anchor, final_dir)?;
    let straight_nm = projection_along_course_nm(current_position, turn_end, initial_dir);
    if straight_nm < -0.05 {
        return None;
    }
    if !cf_intercept_is_reasonable(turn_end, course_anchor, course_deg, fix) {
        return None;
    }
    let turn_start = if straight_nm <= 0.05 {
        current_position
    } else {
        destination_point(current_position, current_heading_deg, straight_nm)
    };
    let turn_center = turn_center_for_heading_change(
        turn_start,
        current_heading_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    let turn_end_from_center = point_on_turn_center(
        turn_center,
        course_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    let mut elements = Vec::new();
    if distance_between_points_nm(current_position, turn_start) > 0.05 {
        elements.push(LegDisplayElement::Segment {
            start: current_position,
            end: turn_start,
        });
    }
    if distance_between_points_nm(turn_start, turn_end_from_center) > 0.05 {
        elements.push(LegDisplayElement::Arc {
            center: turn_center,
            radius_nm: turn_radius_nm,
            start: turn_start,
            end: turn_end_from_center,
            clockwise: turn_clockwise,
            sweep_degrees: heading_sweep_degrees(
                current_heading_deg,
                course_deg,
                turn_clockwise,
            ),
        });
    }
    if distance_between_points_nm(turn_end_from_center, fix) > 0.05 {
        elements.push(LegDisplayElement::Segment {
            start: turn_end_from_center,
            end: fix,
        });
    }
    Some(elements)
}

fn cf_intercept_is_reasonable(
    intercept: LatLon,
    defining_nav: LatLon,
    course_deg: f64,
    fix: LatLon,
) -> bool {
    let course_unit = bearing_unit_vector(course_deg);
    let intercept_projection = projection_along_course_nm(defining_nav, intercept, course_unit);
    let fix_projection = projection_along_course_nm(defining_nav, fix, course_unit);
    intercept_projection <= fix_projection + 0.05
}

fn intersect_lines(
    line1_anchor: LatLon,
    line1_dir: (f64, f64),
    line2_anchor: LatLon,
    line2_dir: (f64, f64),
) -> Option<LatLon> {
    let origin = line2_anchor;
    let line1_anchor_en = to_local_en(origin, line1_anchor);
    let cross = line1_dir.0 * line2_dir.1 - line1_dir.1 * line2_dir.0;
    if cross.abs() < 1e-6 {
        return None;
    }
    let delta = (-line1_anchor_en.0, -line1_anchor_en.1);
    let t = (delta.0 * line2_dir.1 - delta.1 * line2_dir.0) / cross;
    Some(offset_latlon(
        line1_anchor,
        line1_dir.0 * t,
        line1_dir.1 * t,
    ))
}

fn projection_along_course_nm(origin: LatLon, point: LatLon, course_unit: (f64, f64)) -> f64 {
    let east_north = to_local_en(origin, point);
    east_north.0 * course_unit.0 + east_north.1 * course_unit.1
}

fn heading_sweep_degrees(from_deg: f64, to_deg: f64, clockwise: bool) -> f64 {
    let mut delta = normalize_bearing_degrees(to_deg) - normalize_bearing_degrees(from_deg);
    if clockwise {
        while delta < 0.0 {
            delta += 360.0;
        }
        delta
    } else {
        while delta > 0.0 {
            delta -= 360.0;
        }
        delta.abs()
    }
}

fn intersect_heading_with_course(
    ray_start: LatLon,
    ray_heading_deg: f64,
    course_anchor: LatLon,
    course_heading_deg: f64,
    course_fix: LatLon,
) -> Option<LatLon> {
    let origin = course_anchor;
    let ray_start_en = to_local_en(origin, ray_start);
    let ray_dir = bearing_unit_vector(ray_heading_deg);
    let course_anchor_en = (0.0, 0.0);
    let course_dir = bearing_unit_vector(course_heading_deg);
    let cross = ray_dir.0 * course_dir.1 - ray_dir.1 * course_dir.0;
    if cross.abs() < 1e-6 {
        return Some(course_fix);
    }
    let delta = (course_anchor_en.0 - ray_start_en.0, course_anchor_en.1 - ray_start_en.1);
    let t = (delta.0 * course_dir.1 - delta.1 * course_dir.0) / cross;
    if t < 0.0 {
        return Some(course_fix);
    }
    Some(offset_latlon(ray_start, ray_dir.0 * t, ray_dir.1 * t))
}

fn to_local_en(origin: LatLon, point: LatLon) -> (f64, f64) {
    let mean_lat = ((origin.lat + point.lat) / 2.0).to_radians();
    let east_nm = (point.lon - origin.lon) * 60.0 * mean_lat.cos();
    let north_nm = (point.lat - origin.lat) * 60.0;
    (east_nm, north_nm)
}

fn nominal_hold_leg_length_nm(distance_or_time: Option<&str>) -> Option<f64> {
    let value = distance_or_time?.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(minutes_tenths) = value.strip_prefix('T') {
        let tenths = minutes_tenths.parse::<f64>().ok()?;
        let minutes = tenths / 10.0;
        return Some(NOMINAL_HOLD_GROUND_SPEED_KT * (minutes / 60.0));
    }
    let tenths_nm = value.parse::<f64>().ok()?;
    Some(tenths_nm / 10.0)
}

fn nominal_standard_rate_turn_radius_nm() -> f64 {
    let speed_nm_per_sec = NOMINAL_HOLD_GROUND_SPEED_KT / 3600.0;
    let rate_rad_per_sec = STANDARD_RATE_TURN_DEG_PER_SEC.to_radians();
    speed_nm_per_sec / rate_rad_per_sec
}

fn build_hold_display_path(
    fix: LatLon,
    inbound_course_deg: f64,
    clockwise: bool,
    leg_length_nm: f64,
    turn_radius_nm: f64,
    arrival_course_deg: Option<f64>,
    stop_when_established_inbound: bool,
) -> LegDisplayPath {
    let inbound = bearing_unit_vector(inbound_course_deg);
    let outbound = (-inbound.0, -inbound.1);
    let lateral = if clockwise {
        right_normal(inbound)
    } else {
        left_normal(inbound)
    };

    let inbound_end = fix;
    let first_turn_center = offset_latlon(
        inbound_end,
        lateral.0 * turn_radius_nm,
        lateral.1 * turn_radius_nm,
    );
    let outbound_start = offset_latlon(
        first_turn_center,
        lateral.0 * turn_radius_nm,
        lateral.1 * turn_radius_nm,
    );
    let outbound_end = offset_latlon(
        outbound_start,
        outbound.0 * leg_length_nm,
        outbound.1 * leg_length_nm,
    );
    let second_turn_center = offset_latlon(
        outbound_end,
        -lateral.0 * turn_radius_nm,
        -lateral.1 * turn_radius_nm,
    );
    let inbound_rejoin = offset_latlon(
        second_turn_center,
        -lateral.0 * turn_radius_nm,
        -lateral.1 * turn_radius_nm,
    );

    let entry_kind = arrival_course_deg
        .map(|arrival_course_deg| classify_hold_entry(arrival_course_deg, inbound_course_deg, clockwise));
    let mut elements = hold_entry_elements(
        fix,
        inbound_course_deg,
        clockwise,
        arrival_course_deg,
        leg_length_nm,
        turn_radius_nm,
    );
    if stop_when_established_inbound {
        if !matches!(entry_kind, Some(HoldEntryKind::Direct) | None) {
            return LegDisplayPath {
                style: LegDisplayPathStyle::Solid,
                elements,
            };
        }
    }
    elements.extend(vec![
        LegDisplayElement::Arc {
            center: first_turn_center,
            radius_nm: turn_radius_nm,
            start: inbound_end,
            end: outbound_start,
            clockwise,
            sweep_degrees: 180.0,
        },
        LegDisplayElement::Segment {
            start: outbound_start,
            end: outbound_end,
        },
        LegDisplayElement::Arc {
            center: second_turn_center,
            radius_nm: turn_radius_nm,
            start: outbound_end,
            end: inbound_rejoin,
            clockwise,
            sweep_degrees: 180.0,
        },
        LegDisplayElement::Segment {
            start: inbound_rejoin,
            end: inbound_end,
        },
    ]);

    LegDisplayPath {
        style: LegDisplayPathStyle::Solid,
        elements,
    }
}

fn hold_entry_elements(
    fix: LatLon,
    inbound_course_deg: f64,
    clockwise: bool,
    arrival_course_deg: Option<f64>,
    leg_length_nm: f64,
    turn_radius_nm: f64,
) -> Vec<LegDisplayElement> {
    let Some(arrival_course_deg) = arrival_course_deg else {
        return Vec::new();
    };
    match classify_hold_entry(arrival_course_deg, inbound_course_deg, clockwise) {
        HoldEntryKind::Direct => Vec::new(),
        HoldEntryKind::Parallel => {
            let outbound_course_deg = normalize_bearing_degrees(inbound_course_deg + 180.0);
            let entry_distance_nm = nominal_hold_entry_distance_nm(leg_length_nm);
            let entry_end = destination_point(fix, outbound_course_deg, entry_distance_nm);
            let entry_turn_clockwise = !clockwise;
            let turn_center = turn_center_for_heading_change(
                entry_end,
                outbound_course_deg,
                entry_turn_clockwise,
                turn_radius_nm,
            );
            let Some((turn_end, rejoin_course_deg)) = tangent_rejoin_from_turn(
                turn_center,
                turn_radius_nm,
                outbound_course_deg,
                entry_turn_clockwise,
                fix,
            ) else {
                return Vec::new();
            };
            vec![
                LegDisplayElement::Segment {
                    start: fix,
                    end: entry_end,
                },
                LegDisplayElement::Arc {
                    center: turn_center,
                    radius_nm: turn_radius_nm,
                    start: entry_end,
                    end: turn_end,
                    clockwise: entry_turn_clockwise,
                    sweep_degrees: heading_sweep_degrees(
                        outbound_course_deg,
                        rejoin_course_deg,
                        entry_turn_clockwise,
                    ),
                },
                LegDisplayElement::Segment {
                    start: turn_end,
                    end: fix,
                },
            ]
        }
        HoldEntryKind::Teardrop => {
            let outbound_course_deg = normalize_bearing_degrees(inbound_course_deg + 180.0);
            let teardrop_course_deg = if clockwise {
                normalize_bearing_degrees(outbound_course_deg - 30.0)
            } else {
                normalize_bearing_degrees(outbound_course_deg + 30.0)
            };
            let entry_distance_nm = nominal_hold_entry_distance_nm(leg_length_nm);
            let entry_end = destination_point(fix, teardrop_course_deg, entry_distance_nm);
            let turn_center = turn_center_for_heading_change(
                entry_end,
                teardrop_course_deg,
                clockwise,
                turn_radius_nm,
            );
            let Some((turn_end, rejoin_course_deg)) = tangent_rejoin_from_turn(
                turn_center,
                turn_radius_nm,
                teardrop_course_deg,
                clockwise,
                fix,
            ) else {
                return Vec::new();
            };
            vec![
                LegDisplayElement::Segment {
                    start: fix,
                    end: entry_end,
                },
                LegDisplayElement::Arc {
                    center: turn_center,
                    radius_nm: turn_radius_nm,
                    start: entry_end,
                    end: turn_end,
                    clockwise,
                    sweep_degrees: heading_sweep_degrees(
                        teardrop_course_deg,
                        rejoin_course_deg,
                        clockwise,
                    ),
                },
                LegDisplayElement::Segment {
                    start: turn_end,
                    end: fix,
                },
            ]
        }
    }
}

fn nominal_hold_entry_distance_nm(leg_length_nm: f64) -> f64 {
    leg_length_nm.min(NOMINAL_HOLD_GROUND_SPEED_KT / 60.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldEntryKind {
    Direct,
    Parallel,
    Teardrop,
}

fn classify_hold_entry(
    arrival_course_deg: f64,
    inbound_course_deg: f64,
    clockwise: bool,
) -> HoldEntryKind {
    let outbound_course_deg = normalize_bearing_degrees(inbound_course_deg + 180.0);
    let delta = clockwise_delta_degrees(outbound_course_deg, arrival_course_deg);
    if clockwise {
        if delta <= 110.0 {
            return HoldEntryKind::Parallel;
        }
        if delta >= 290.0 {
            return HoldEntryKind::Teardrop;
        }
        return HoldEntryKind::Direct;
    }
    if delta < 70.0 {
        return HoldEntryKind::Teardrop;
    }
    if delta >= 250.0 {
        return HoldEntryKind::Parallel;
    }
    HoldEntryKind::Direct
}

#[cfg(test)]
mod tests {
    use super::{classify_hold_entry, HoldEntryKind};

    #[test]
    fn left_hold_entry_sectors_match_expected_examples() {
        let inbound = 264.0;
        assert_eq!(
            classify_hold_entry(20.0, inbound, false),
            HoldEntryKind::Parallel
        );
        assert_eq!(
            classify_hold_entry(105.0, inbound, false),
            HoldEntryKind::Teardrop
        );
        assert_eq!(
            classify_hold_entry(200.0, inbound, false),
            HoldEntryKind::Direct
        );
    }

    #[test]
    fn right_hold_entry_sectors_mirror_left_hold() {
        let inbound = 264.0;
        assert_eq!(
            classify_hold_entry(120.0, inbound, true),
            HoldEntryKind::Parallel
        );
        assert_eq!(
            classify_hold_entry(40.0, inbound, true),
            HoldEntryKind::Teardrop
        );
        assert_eq!(
            classify_hold_entry(300.0, inbound, true),
            HoldEntryKind::Direct
        );
    }
}

fn bearing_unit_vector(course_deg: f64) -> (f64, f64) {
    let radians = course_deg.to_radians();
    (radians.sin(), radians.cos())
}

fn normalize_bearing_degrees(value: f64) -> f64 {
    let mut normalized = value % 360.0;
    if normalized < 0.0 {
        normalized += 360.0;
    }
    normalized
}

fn clockwise_delta_degrees(from_deg: f64, to_deg: f64) -> f64 {
    let mut delta = normalize_bearing_degrees(to_deg) - normalize_bearing_degrees(from_deg);
    if delta < 0.0 {
        delta += 360.0;
    }
    delta
}

fn shortest_turn_clockwise(from_deg: f64, to_deg: f64) -> bool {
    let clockwise_delta = normalize_bearing_degrees(to_deg) - normalize_bearing_degrees(from_deg);
    let clockwise_delta = if clockwise_delta < 0.0 {
        clockwise_delta + 360.0
    } else {
        clockwise_delta
    };
    clockwise_delta <= 180.0
}

fn angular_difference_degrees(left: f64, right: f64) -> f64 {
    let mut delta = (normalize_bearing_degrees(left) - normalize_bearing_degrees(right)).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    delta
}

fn bearing_from(from: LatLon, to: LatLon) -> f64 {
    let from_lat = from.lat.to_radians();
    let from_lon = from.lon.to_radians();
    let to_lat = to.lat.to_radians();
    let to_lon = to.lon.to_radians();
    let delta_lon = to_lon - from_lon;
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    normalize_bearing_degrees(y.atan2(x).to_degrees())
}

fn distance_between_points_nm(from: LatLon, to: LatLon) -> f64 {
    let mean_lat = ((from.lat + to.lat) / 2.0).to_radians();
    let east_nm = (to.lon - from.lon) * 60.0 * mean_lat.cos();
    let north_nm = (to.lat - from.lat) * 60.0;
    east_nm.hypot(north_nm)
}

fn left_normal((east, north): (f64, f64)) -> (f64, f64) {
    (-north, east)
}

fn right_normal((east, north): (f64, f64)) -> (f64, f64) {
    (north, -east)
}

fn offset_latlon(origin: LatLon, east_nm: f64, north_nm: f64) -> LatLon {
    let lat = origin.lat + north_nm / 60.0;
    let lon_scale = origin.lat.to_radians().cos().abs().max(0.01);
    let lon = origin.lon + east_nm / (60.0 * lon_scale);
    LatLon { lat, lon }
}

fn destination_point(origin: LatLon, course_deg: f64, distance_nm: f64) -> LatLon {
    let (east, north) = bearing_unit_vector(course_deg);
    offset_latlon(origin, east * distance_nm, north * distance_nm)
}
