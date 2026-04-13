use crate::{LatLon, LegDisplayElement, LegDisplayPath, ProcedureLegMaterializationRecord};
use crate::planning::LegDisplayPathStyle;

const NOMINAL_HOLD_GROUND_SPEED_KT: f64 = 180.0;
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
    if let Some(hold) = hold_record {
        if let Some(path) = missed_approach_display_path(segment_records, leg_start, leg_end, hold) {
            return Some(path);
        }
        return hold_display_path(hold);
    }
    None
}

fn hold_display_path(leg: &ProcedureLegMaterializationRecord) -> Option<LegDisplayPath> {
    if leg.path_termination.trim() != "HM" {
        return None;
    }
    let fix = leg.nav_position?;
    let inbound_course_deg =
        leg.magnetic_course_deg? + leg.airport_magnetic_variation_deg.unwrap_or(0.0);
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
    ))
}

fn procedure_turn_display_path(leg: &ProcedureLegMaterializationRecord) -> Option<LegDisplayPath> {
    let fix = leg.nav_position?;
    let barb_course_deg =
        leg.magnetic_course_deg? + leg.airport_magnetic_variation_deg.unwrap_or(0.0);
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
    leg_end: &ProcedureLegMaterializationRecord,
    hold_record: &ProcedureLegMaterializationRecord,
) -> Option<LegDisplayPath> {
    if leg_end.path_termination.trim() != "CF" || hold_record.path_termination.trim() != "HM" {
        return None;
    }
    let start = leg_start.nav_position?;
    let fix = leg_end.nav_position?;
    let course_cf_deg = leg_end.magnetic_course_deg? + leg_end.airport_magnetic_variation_deg.unwrap_or(0.0);
    let recommended = leg_end.recommended_nav_position?;
    let ca = segment_records
        .iter()
        .find(|record| record.sequence > leg_start.sequence && record.sequence < leg_end.sequence && record.path_termination.trim() == "CA")?;
    let vi = segment_records
        .iter()
        .find(|record| record.sequence > leg_start.sequence && record.sequence < leg_end.sequence && record.path_termination.trim() == "VI")?;
    let start_alt_ft = leg_start.altitude_1_ft?;
    let target_alt_ft = ca.altitude_1_ft?;
    let climb_minutes = ((target_alt_ft - start_alt_ft).max(0.0)) / NOMINAL_MISSED_APPROACH_CLIMB_FTPM;
    let climb_distance_nm = NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT * (climb_minutes / 60.0);
    let initial_course_deg = ca.magnetic_course_deg? + ca.airport_magnetic_variation_deg.unwrap_or(0.0);
    let climb_end = destination_point(start, initial_course_deg, climb_distance_nm);

    let turn_heading_deg = vi.magnetic_course_deg? + vi.airport_magnetic_variation_deg.unwrap_or(0.0);
    let turn_clockwise = vi.turn_direction.as_deref().unwrap_or("").trim() == "R";
    let turn_radius_nm = missed_approach_turn_radius_nm();
    let turn_center = turn_center_for_heading_change(climb_end, initial_course_deg, turn_clockwise, turn_radius_nm);
    let turn_end = point_on_turn_center(turn_center, turn_heading_deg, turn_clockwise, turn_radius_nm);

    let intercept = intersect_heading_with_course(turn_end, turn_heading_deg, recommended, course_cf_deg, fix)?;
    let mut elements = vec![
        LegDisplayElement::Segment {
            start,
            end: climb_end,
        },
        LegDisplayElement::Arc {
            center: turn_center,
            radius_nm: turn_radius_nm,
            start: climb_end,
            end: turn_end,
            clockwise: turn_clockwise,
            sweep_degrees: heading_sweep_degrees(initial_course_deg, turn_heading_deg, turn_clockwise),
        },
        LegDisplayElement::Segment {
            start: turn_end,
            end: intercept,
        },
        LegDisplayElement::Segment {
            start: intercept,
            end: fix,
        },
    ];
    if let Some(hold_path) = hold_display_path(hold_record) {
        elements.extend(hold_path.elements);
    }
    Some(LegDisplayPath { style: LegDisplayPathStyle::Solid, elements })
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

    LegDisplayPath {
        style: LegDisplayPathStyle::Solid,
        elements: vec![
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
        ],
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
