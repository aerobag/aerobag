use crate::planning::LegDisplayPathStyle;
use crate::{
    heading_signature_for_element, LatLon, LegDisplayElement, LegDisplayPath,
    ProcedureLegMaterializationRecord,
};

macro_rules! debug_source {
    () => {
        format!("{}:{}", file!(), line!())
    };
}

macro_rules! push_segment {
    ($elements:expr, $sources:expr, $start:expr, $end:expr) => {{
        $elements.push(LegDisplayElement::Segment {
            start: $start,
            end: $end,
        });
        $sources.push(debug_source!());
    }};
}

macro_rules! push_arc {
    ($elements:expr, $sources:expr, $center:expr, $radius_nm:expr, $start:expr, $end:expr, $clockwise:expr, $sweep_degrees:expr) => {{
        $elements.push(LegDisplayElement::Arc {
            center: $center,
            radius_nm: $radius_nm,
            start: $start,
            end: $end,
            clockwise: $clockwise,
            sweep_degrees: $sweep_degrees,
        });
        $sources.push(debug_source!());
    }};
}

fn solid_path(elements: Vec<LegDisplayElement>, debug_element_sources: Vec<String>) -> LegDisplayPath {
    LegDisplayPath {
        style: LegDisplayPathStyle::Solid,
        elements,
        debug_element_sources,
    }
}

fn dashed_path(elements: Vec<LegDisplayElement>, debug_element_sources: Vec<String>) -> LegDisplayPath {
    LegDisplayPath {
        style: LegDisplayPathStyle::Dashed,
        elements,
        debug_element_sources,
    }
}

fn extend_elements_with_sources(
    dest_elements: &mut Vec<LegDisplayElement>,
    dest_sources: &mut Vec<String>,
    elements: Vec<LegDisplayElement>,
    mut sources: Vec<String>,
    fallback_source: String,
) {
    if sources.len() != elements.len() {
        sources.resize(elements.len(), fallback_source);
    }
    dest_elements.extend(elements);
    dest_sources.extend(sources);
}

fn extend_sources_for_new_elements(
    sources: &mut Vec<String>,
    old_len: usize,
    elements: &[LegDisplayElement],
    source: String,
) {
    sources.extend(std::iter::repeat_n(
        source,
        elements.len().saturating_sub(old_len),
    ));
}

const NOMINAL_HOLD_GROUND_SPEED_KT: f64 = 120.0;
const STANDARD_RATE_TURN_DEG_PER_SEC: f64 = 3.0;
const NOMINAL_PROCEDURE_TURN_INITIAL_OUTBOUND_DISTANCE_NM: f64 = 5.0;
const NOMINAL_PROCEDURE_TURN_GROUND_SPEED_KT: f64 = 120.0;
const NOMINAL_PROCEDURE_TURN_BARB_TIME_MIN: f64 = 2.0;
const NOMINAL_MANUAL_TERMINATION_DISTANCE_NM: f64 = 4.0;
const MIN_GEOMETRY_DISTANCE_NM: f64 = 0.05;
const NEAR_INTERCEPT_SNAP_DISTANCE_NM: f64 = 0.1;
const TO_FIX_TERMINATION_SNAP_DISTANCE_NM: f64 = 0.1;
const MIN_ARC_SWEEP_DEG: f64 = 0.5;
const POSITION_EPSILON_DEG: f64 = 0.0005;

pub fn display_path_for_procedure_leg(
    segment_records: &[ProcedureLegMaterializationRecord],
    leg_start: &ProcedureLegMaterializationRecord,
    leg_end: &ProcedureLegMaterializationRecord,
    hold_record: Option<&ProcedureLegMaterializationRecord>,
    initial_position_override: Option<LatLon>,
    initial_course_override: Option<f64>,
) -> Option<LegDisplayPath> {
    let terminal_record = if leg_start.sequence == leg_end.sequence {
        segment_records
            .iter()
            .filter(|record| record.sequence > leg_start.sequence)
            .max_by_key(|record| record.sequence)
            .unwrap_or(leg_end)
    } else {
        hold_record.unwrap_or(leg_end)
    };
    build_procedure_leg_display_path(
        segment_records,
        leg_start,
        terminal_record,
        initial_position_override,
        initial_course_override,
    )
}

pub fn display_path_for_resumed_common_cf(
    step: &ProcedureLegMaterializationRecord,
    initial_position_override: Option<LatLon>,
    initial_course_override: Option<f64>,
) -> Option<LegDisplayPath> {
    if step.path_termination.trim() != "CF" {
        return None;
    }
    let fix = step.nav_position?;
    let course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    let mut current_position = initial_position_override?;
    let current_heading_deg = initial_course_override?;
    let mut elements = Vec::new();
    let mut debug_sources = Vec::new();

    if angular_difference_degrees(current_heading_deg, course_deg) > 5.0 {
        let prior_len = elements.len();
        let turn_end = append_heading_change(
            &mut elements,
            current_position,
            current_heading_deg,
            course_deg,
            shortest_turn_clockwise(current_heading_deg, course_deg),
            0.0,
            missed_approach_turn_radius_nm(),
        );
        extend_sources_for_new_elements(
            &mut debug_sources,
            prior_len,
            &elements,
            debug_source!(),
        );
        current_position = turn_end;
    }

    if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
        push_segment!(elements, debug_sources, current_position, fix);
    }

    Some(LegDisplayPath {
        style: LegDisplayPathStyle::Solid,
        elements,
        debug_element_sources: debug_sources,
    })
}

pub fn build_trailing_course_to_intercept_display_path(
    trailing_record: &ProcedureLegMaterializationRecord,
    initial_position_override: Option<LatLon>,
    initial_course_override: Option<f64>,
    next_segment_records: &[ProcedureLegMaterializationRecord],
) -> Option<LegDisplayPath> {
    if trailing_record.path_termination.trim() != "CI" {
        return None;
    }
    let start = initial_position_override?;
    let intercept_step = next_segment_records
        .iter()
        .find(|record| record.path_termination.trim() == "CF")?;
    let (mut elements, intercept, _flown_course_deg) =
        course_to_intercept_path(
            trailing_record,
            start,
            initial_course_override,
            Some(intercept_step),
        )?;
    let mut altitude_ft = trailing_record.altitude_1_ft;
    let mut debug_sources = Vec::new();
    let _ = append_course_track_path(
        &mut elements,
        &mut debug_sources,
        intercept,
        Some(intercept_step.magnetic_course_deg? + course_reference_variation_deg(intercept_step)),
        &mut altitude_ft,
        intercept_step,
        TrackTermination::ToFix(intercept_step.nav_position?),
    )?;
    Some(solid_path(elements, debug_sources))
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
    let turn_center =
        turn_center_for_heading_change(barb_end, barb_course_deg, clockwise, turn_radius_nm);
    let return_heading_deg = normalize_bearing_degrees(barb_course_deg + 180.0);
    let intercept_start =
        point_on_turn_center(turn_center, return_heading_deg, clockwise, turn_radius_nm);
    let inbound_course_deg = normalize_bearing_degrees(outbound_course_deg + 180.0);
    let intercept = intersect_heading_with_course(
        intercept_start,
        return_heading_deg,
        fix,
        inbound_course_deg,
        fix,
    )?;
    let mut elements = Vec::new();
    let mut sources = Vec::new();
    push_segment!(elements, sources, fix, outbound_end);
    push_segment!(elements, sources, outbound_end, barb_end);
    push_arc!(
        elements,
        sources,
        turn_center,
        turn_radius_nm,
        barb_end,
        intercept_start,
        clockwise,
        180.0
    );
    push_segment!(elements, sources, intercept_start, intercept);
    push_segment!(elements, sources, intercept, fix);
    Some(dashed_path(elements, sources))
}

fn arc_to_fix_path_from_start(
    start: LatLon,
    leg_end: &ProcedureLegMaterializationRecord,
) -> Option<LegDisplayPath> {
    let end = leg_end.nav_position?;
    let center = leg_end.defining_nav_position?;
    let clockwise = match leg_end.turn_direction.as_deref().unwrap_or("").trim() {
        "L" => false,
        "R" => true,
        _ => return None,
    };
    let start_radius_nm = distance_between_points_nm(center, start);
    let end_radius_nm = distance_between_points_nm(center, end);
    if start_radius_nm <= MIN_GEOMETRY_DISTANCE_NM || end_radius_nm <= MIN_GEOMETRY_DISTANCE_NM {
        return None;
    }
    let radius_nm = start_radius_nm;
    let start_on_arc = start;
    let end_on_arc = end;
    let sweep_degrees = heading_sweep_degrees(
        bearing_from(center, start_on_arc),
        bearing_from(center, end_on_arc),
        clockwise,
    );
    if sweep_degrees <= MIN_ARC_SWEEP_DEG {
        return None;
    }
    let mut elements = Vec::new();
    let mut sources = Vec::new();
    push_arc!(
        elements,
        sources,
        center,
        radius_nm,
        start_on_arc,
        end_on_arc,
        clockwise,
        sweep_degrees
    );
    Some(solid_path(elements, sources))
}

fn radius_to_fix_path_from_start(
    start: LatLon,
    leg_end: &ProcedureLegMaterializationRecord,
) -> Option<LegDisplayPath> {
    let end = leg_end.nav_position?;
    let center = leg_end.arc_center_fix_position?;
    let clockwise = match leg_end.turn_direction.as_deref().unwrap_or("").trim() {
        "L" => false,
        "R" => true,
        _ => return None,
    };
    let start_radius_nm = distance_between_points_nm(center, start);
    let end_radius_nm = distance_between_points_nm(center, end);
    if start_radius_nm <= MIN_GEOMETRY_DISTANCE_NM || end_radius_nm <= MIN_GEOMETRY_DISTANCE_NM {
        return None;
    }
    let radius_nm = leg_end
        .arc_radius_nm
        .filter(|radius| *radius > MIN_GEOMETRY_DISTANCE_NM)
        .unwrap_or(start_radius_nm);
    let start_on_arc = start;
    let end_on_arc = end;
    let sweep_degrees = heading_sweep_degrees(
        bearing_from(center, start_on_arc),
        bearing_from(center, end_on_arc),
        clockwise,
    );
    if sweep_degrees <= MIN_ARC_SWEEP_DEG {
        return None;
    }
    let mut elements = Vec::new();
    let mut sources = Vec::new();
    push_arc!(
        elements,
        sources,
        center,
        radius_nm,
        start_on_arc,
        end_on_arc,
        clockwise,
        sweep_degrees
    );
    Some(solid_path(elements, sources))
}

fn nominal_procedure_turn_barb_distance_nm() -> f64 {
    distance_nm_for_minutes_at_speed_kt(
        NOMINAL_PROCEDURE_TURN_GROUND_SPEED_KT,
        NOMINAL_PROCEDURE_TURN_BARB_TIME_MIN,
    )
}

const NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT: f64 = 90.0;
const NOMINAL_MISSED_APPROACH_CLIMB_FTPM: f64 = 500.0;
const NOMINAL_COURSE_INTERCEPT_ANGLE_DEG: f64 = 30.0;
const MAX_EXTRA_STRAIGHT_BEFORE_VI_TURN_NM: f64 = 2.0;

enum TrackTermination {
    ToFix(LatLon),
    ToAltitude(Option<f64>),
    ToDme { center: LatLon, radius_nm: f64 },
}

fn build_procedure_leg_display_path(
    segment_records: &[ProcedureLegMaterializationRecord],
    leg_start: &ProcedureLegMaterializationRecord,
    terminal_record: &ProcedureLegMaterializationRecord,
    initial_position_override: Option<LatLon>,
    initial_course_override: Option<f64>,
) -> Option<LegDisplayPath> {
    let mut current_position = initial_position_override.or(leg_start.nav_position)?;
    let mut current_course_deg = initial_course_override.or_else(|| {
        leg_start
            .magnetic_course_deg
            .map(|course| course + course_reference_variation_deg(leg_start))
    });
    let mut current_altitude_ft = segment_records
        .iter()
        .filter(|record| record.sequence <= leg_start.sequence)
        .filter_map(|record| record.altitude_1_ft)
        .next_back()
        .or(leg_start.altitude_1_ft);
    let mut elements = Vec::new();
    let mut debug_sources = Vec::new();

    let mut steps = segment_records
        .iter()
        .filter(|record| {
            record.sequence >= leg_start.sequence && record.sequence <= terminal_record.sequence
        })
        .collect::<Vec<_>>();
    steps.sort_by_key(|record| record.sequence);

    for (index, step) in steps.iter().enumerate() {
        match step.path_termination.trim() {
            "FM" | "HA" => {
                panic!(
                    "{} procedure legs are not implemented: {} {} {} {} seq {}",
                    step.path_termination.trim(),
                    step.key.airport_id.trim(),
                    step.key.procedure_id.trim(),
                    step.key.route_type.trim(),
                    step.key.transition_id.trim(),
                    step.sequence
                );
            }
            "IF" => {
                if let Some(fix) = step.nav_position {
                    if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                        let direct_course_deg = bearing_from(current_position, fix);
                        if let Some(current_heading_deg) = current_course_deg {
                            if angular_difference_degrees(current_heading_deg, direct_course_deg) > 5.0 {
                                let prior_len = elements.len();
                                let turn_end = append_heading_change(
                                    &mut elements,
                                    current_position,
                                    current_heading_deg,
                                    direct_course_deg,
                                    shortest_turn_clockwise(current_heading_deg, direct_course_deg),
                                    0.0,
                                    missed_approach_turn_radius_nm(),
                                );
                                extend_sources_for_new_elements(
                                    &mut debug_sources,
                                    prior_len,
                                    &elements,
                                    debug_source!(),
                                );
                                current_position = turn_end;
                            }
                        }
                        if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                            push_segment!(elements, debug_sources, current_position, fix);
                        }
                        current_course_deg = Some(direct_course_deg);
                    } else {
                        if let (Some(current_heading_deg), Some(next_step)) =
                            (current_course_deg, steps.get(index + 1).copied())
                        {
                            if next_step.path_termination.trim() == "CF" {
                                if let Some(next_fix) = next_step.nav_position {
                                    let direct_course_deg = bearing_from(fix, next_fix);
                                    if angular_difference_degrees(current_heading_deg, direct_course_deg)
                                        > 5.0
                                    {
                                        let prior_len = elements.len();
                                        let turn_end = append_heading_change(
                                            &mut elements,
                                            current_position,
                                            current_heading_deg,
                                            direct_course_deg,
                                            shortest_turn_clockwise(
                                                current_heading_deg,
                                                direct_course_deg,
                                            ),
                                            0.0,
                                            missed_approach_turn_radius_nm(),
                                        );
                                        extend_sources_for_new_elements(
                                            &mut debug_sources,
                                            prior_len,
                                            &elements,
                                            debug_source!(),
                                        );
                                        current_position = turn_end;
                                        current_course_deg = Some(direct_course_deg);
                                        continue;
                                    }
                                }
                            }
                        }
                        current_course_deg = step
                            .magnetic_course_deg
                            .map(|course| course + course_reference_variation_deg(step))
                            .or(current_course_deg);
                    }
                    current_position = fix;
                }
            }
            "CA" => {
                let course_deg = current_or_step_course_deg(step, current_course_deg)?;
                let prior_len = elements.len();
                current_position = extend_climb_segment(
                    &mut elements,
                    current_position,
                    course_deg,
                    &mut current_altitude_ft,
                    step.altitude_1_ft,
                );
                extend_sources_for_new_elements(
                    &mut debug_sources,
                    prior_len,
                    &elements,
                    debug_source!(),
                );
                current_course_deg = Some(course_deg);
            }
            "FA" => {
                let (new_position, course_deg) = append_course_track_path(
                    &mut elements,
                    &mut debug_sources,
                    current_position,
                    current_course_deg,
                    &mut current_altitude_ft,
                    step,
                    TrackTermination::ToAltitude(step.altitude_1_ft),
                )?;
                current_position = new_position;
                current_course_deg = Some(course_deg);
            }
            "CD" => {
                let center = step.defining_nav_position?;
                let radius_nm = parse_distance_tenths_nm(step.route_distance_or_time.as_deref())?;
                let (new_position, course_deg) = append_course_track_path(
                    &mut elements,
                    &mut debug_sources,
                    current_position,
                    current_course_deg,
                    &mut current_altitude_ft,
                    step,
                    TrackTermination::ToDme { center, radius_nm },
                )?;
                current_position = new_position;
                current_course_deg = Some(course_deg);
            }
            "FC" => {
                let course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
                let fix = step.nav_position?;
                if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, fix);
                }
                current_position = fix;
                let distance_nm = parse_distance_tenths_nm(step.route_distance_or_time.as_deref())?;
                if distance_nm > MIN_GEOMETRY_DISTANCE_NM {
                    let end = destination_point(current_position, course_deg, distance_nm);
                    if distance_between_points_nm(current_position, end) > MIN_GEOMETRY_DISTANCE_NM {
                        push_segment!(elements, debug_sources, current_position, end);
                    }
                    current_position = end;
                }
                current_course_deg = Some(course_deg);
            }
            "VA" | "VI" | "VM" => {
                let prior_len = elements.len();
                let new_position = append_heading_leg_path(
                    &mut elements,
                    step,
                    current_position,
                    current_course_deg,
                    &mut current_altitude_ft,
                    steps.get(index + 1).copied(),
                )?;
                extend_sources_for_new_elements(
                    &mut debug_sources,
                    prior_len,
                    &elements,
                    debug_source!(),
                );
                current_position = new_position;
                current_course_deg =
                    Some(step.magnetic_course_deg? + course_reference_variation_deg(step));
            }
            "CI" => {
                let next_cf_step = steps
                    .iter()
                    .skip(index + 1)
                    .copied()
                    .find(|candidate| candidate.path_termination.trim() == "CF");
                let (ci_elements, end, course_deg) = course_to_intercept_path(
                    step,
                    current_position,
                    current_course_deg,
                    next_cf_step,
                )?;
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    ci_elements,
                    Vec::new(),
                    debug_source!(),
                );
                current_position = end;
                current_course_deg = Some(course_deg);
            }
            "VD" => {
                let (vd_elements, end, course_deg) = heading_to_dme_distance_path(
                    step,
                    current_position,
                    current_course_deg,
                )?;
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    vd_elements,
                    Vec::new(),
                    debug_source!(),
                );
                current_position = end;
                current_course_deg = Some(course_deg);
            }
            "VR" => {
                let (vr_elements, end, course_deg) = heading_to_radial_termination_path(
                    step,
                    current_position,
                    current_course_deg,
                )?;
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    vr_elements,
                    Vec::new(),
                    debug_source!(),
                );
                current_position = end;
                current_course_deg = Some(course_deg);
            }
            "AF" => {
                let path = arc_to_fix_path_from_start(current_position, step)?;
                current_position = step.nav_position?;
                current_course_deg = path.elements.last().and_then(display_element_end_course_deg);
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    path.elements,
                    path.debug_element_sources,
                    debug_source!(),
                );
            }
            "RF" => {
                let path = radius_to_fix_path_from_start(current_position, step)?;
                current_position = step.nav_position?;
                current_course_deg = path.elements.last().and_then(display_element_end_course_deg);
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    path.elements,
                    path.debug_element_sources,
                    debug_source!(),
                );
            }
            "PI" => {
                let fix = step.nav_position?;
                let mut path = procedure_turn_display_path(step)?;
                if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, fix);
                }
                current_position = fix;
                if let (Some(current_heading_deg), Some(first_element)) =
                    (current_course_deg, path.elements.first_mut())
                {
                    if let Some((_, outbound_course_deg, _, _)) = heading_signature_for_element(first_element) {
                        if angular_difference_degrees(current_heading_deg, outbound_course_deg) > 5.0 {
                            let prior_len = elements.len();
                            let turn_clockwise =
                                shortest_turn_clockwise(current_heading_deg, outbound_course_deg);
                            let turn_end = append_heading_change(
                                &mut elements,
                                current_position,
                                current_heading_deg,
                                outbound_course_deg,
                                turn_clockwise,
                                0.0,
                                missed_approach_turn_radius_nm(),
                            );
                            extend_sources_for_new_elements(
                                &mut debug_sources,
                                prior_len,
                                &elements,
                                debug_source!(),
                            );
                            set_display_element_start_position(first_element, turn_end);
                        }
                    }
                }
                current_course_deg = path.elements.last().and_then(display_element_end_course_deg);
                extend_elements_with_sources(
                    &mut elements,
                    &mut debug_sources,
                    path.elements,
                    path.debug_element_sources,
                    debug_source!(),
                );
            }
            "DF" => {
                let fix = step.nav_position?;
                if distance_between_points_nm(current_position, fix) <= MIN_GEOMETRY_DISTANCE_NM {
                    current_position = fix;
                    continue;
                }
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
                    push_arc!(
                        elements,
                        debug_sources,
                        turn_center,
                        turn_radius_nm,
                        current_position,
                        turn_end,
                        turn_clockwise,
                        heading_sweep_degrees(
                            initial_course_deg,
                            direct_course_deg,
                            turn_clockwise,
                        )
                    );
                    current_position = turn_end;
                }
                push_segment!(elements, debug_sources, current_position, fix);
                current_position = fix;
                current_course_deg = Some(direct_course_deg);
            }
            "CF" => {
                let fix = step.nav_position?;
                let course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
                if step
                    .turn_direction
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                {
                    if let (Some(current_heading_deg), Some(start_alt_ft), Some(target_alt_ft)) =
                        (current_course_deg, current_altitude_ft, step.altitude_1_ft)
                    {
                        if target_alt_ft > start_alt_ft + 50.0
                            && angular_difference_degrees(current_heading_deg, course_deg) <= 5.0
                        {
                            let climb_minutes =
                                (target_alt_ft - start_alt_ft) / NOMINAL_MISSED_APPROACH_CLIMB_FTPM;
                            let climb_distance_nm = distance_nm_for_minutes_at_speed_kt(
                                NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT,
                                climb_minutes,
                            )
                                .min(distance_between_points_nm(current_position, fix));
                            let climb_end = destination_point(
                                current_position,
                                current_heading_deg,
                                climb_distance_nm,
                            );
                            if distance_between_points_nm(current_position, climb_end) > MIN_GEOMETRY_DISTANCE_NM {
                                push_segment!(elements, debug_sources, current_position, climb_end);
                                current_position = climb_end;
                            }
                            current_altitude_ft = Some(target_alt_ft);
                        }
                    }
                }
                if let Some((new_position, _)) = append_course_track_path(
                    &mut elements,
                    &mut debug_sources,
                    current_position,
                    current_course_deg,
                    &mut current_altitude_ft,
                    step,
                    TrackTermination::ToFix(fix),
                ) {
                    current_position = new_position;
                } else {
                    let direct_course_deg = bearing_from(current_position, fix);
                    if let Some(current_heading_deg) = current_course_deg {
                        if angular_difference_degrees(current_heading_deg, direct_course_deg) > 5.0 {
                            let turn_prior_len = elements.len();
                            let turn_end = append_heading_change(
                                &mut elements,
                                current_position,
                                current_heading_deg,
                                direct_course_deg,
                                cf_turn_direction(step).unwrap_or_else(|| {
                                    shortest_turn_clockwise(current_heading_deg, direct_course_deg)
                                }),
                                0.0,
                                missed_approach_turn_radius_nm(),
                            );
                            extend_sources_for_new_elements(
                                &mut debug_sources,
                                turn_prior_len,
                                &elements,
                                debug_source!(),
                            );
                            if distance_between_points_nm(turn_end, fix) > MIN_GEOMETRY_DISTANCE_NM {
                                push_segment!(elements, debug_sources, turn_end, fix);
                            }
                        } else if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                            push_segment!(elements, debug_sources, current_position, fix);
                        }
                    } else if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                        push_segment!(elements, debug_sources, current_position, fix);
                    }
                    current_position = fix;
                }
                current_course_deg = Some(course_deg);
            }
            "TF" => {
                let fix = step.nav_position?;
                let effective_fix = if distance_between_points_nm(current_position, fix)
                    <= MIN_GEOMETRY_DISTANCE_NM
                {
                    current_position
                } else {
                    fix
                };
                let outbound_course_deg = bearing_from(current_position, effective_fix);
                if distance_between_points_nm(current_position, effective_fix) > MIN_GEOMETRY_DISTANCE_NM {
                    if let Some(current_heading_deg) = current_course_deg {
                        if angular_difference_degrees(current_heading_deg, outbound_course_deg) > 5.0 {
                            let prior_len = elements.len();
                            let turn_clockwise =
                                shortest_turn_clockwise(current_heading_deg, outbound_course_deg);
                            let turn_end = append_heading_change(
                                &mut elements,
                                current_position,
                                current_heading_deg,
                                outbound_course_deg,
                                turn_clockwise,
                                0.0,
                                missed_approach_turn_radius_nm(),
                            );
                            extend_sources_for_new_elements(
                                &mut debug_sources,
                                prior_len,
                                &elements,
                                debug_source!(),
                            );
                            if distance_between_points_nm(turn_end, effective_fix) > MIN_GEOMETRY_DISTANCE_NM {
                                push_segment!(elements, debug_sources, turn_end, effective_fix);
                            }
                            current_course_deg = Some(outbound_course_deg);
                            current_position = effective_fix;
                            continue;
                        }
                    }
                }
                if distance_between_points_nm(current_position, effective_fix) > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, effective_fix);
                }
                current_course_deg = if distance_between_points_nm(current_position, effective_fix)
                    <= MIN_GEOMETRY_DISTANCE_NM
                {
                    step.magnetic_course_deg
                        .map(|course| course + course_reference_variation_deg(step))
                        .or(current_course_deg)
                } else {
                    Some(outbound_course_deg)
                };
                current_position = effective_fix;
            }
            "HF" | "HM" => {
                if let Some(hold_path) = hold_display_path(step, current_course_deg) {
                    current_position = hold_path
                        .elements
                        .last()
                        .and_then(display_element_end_position)
                        .unwrap_or(current_position);
                    current_course_deg = hold_path.elements.last().and_then(display_element_end_course_deg);
                    extend_elements_with_sources(
                        &mut elements,
                        &mut debug_sources,
                        hold_path.elements,
                        hold_path.debug_element_sources,
                        debug_source!(),
                    );
                }
            }
            _ => {}
        }
    }
    snap_nearby_display_element_boundaries(&mut elements);
    prune_degenerate_display_elements_with_sources(&mut elements, &mut debug_sources);
    (!elements.is_empty()).then_some(solid_path(elements, debug_sources))
}

fn display_element_end_position(element: &LegDisplayElement) -> Option<LatLon> {
    match element {
        LegDisplayElement::Segment { end, .. } => Some(*end),
        LegDisplayElement::Arc { end, .. } => Some(*end),
    }
}

fn display_element_start_position(element: &LegDisplayElement) -> Option<LatLon> {
    match element {
        LegDisplayElement::Segment { start, .. } => Some(*start),
        LegDisplayElement::Arc { start, .. } => Some(*start),
    }
}

fn set_display_element_start_position(element: &mut LegDisplayElement, start: LatLon) {
    match element {
        LegDisplayElement::Segment {
            start: element_start,
            ..
        } => *element_start = start,
        LegDisplayElement::Arc {
            start: element_start,
            ..
        } => *element_start = start,
    }
}

fn set_display_element_end_position(element: &mut LegDisplayElement, end: LatLon) {
    match element {
        LegDisplayElement::Segment {
            end: element_end,
            ..
        } => *element_end = end,
        LegDisplayElement::Arc {
            end: element_end,
            ..
        } => *element_end = end,
    }
}

fn snap_nearby_display_element_boundaries(elements: &mut [LegDisplayElement]) {
    for index in 1..elements.len() {
        let Some(previous_end) = display_element_end_position(&elements[index - 1]) else {
            continue;
        };
        let Some(current_start) = display_element_start_position(&elements[index]) else {
            continue;
        };
        if distance_between_points_nm(previous_end, current_start) <= MIN_GEOMETRY_DISTANCE_NM {
            set_display_element_start_position(&mut elements[index], previous_end);
        }
    }
}

fn current_or_step_course_deg(
    step: &ProcedureLegMaterializationRecord,
    current_course_deg: Option<f64>,
) -> Option<f64> {
    step.magnetic_course_deg
        .map(|course| course + course_reference_variation_deg(step))
        .or(current_course_deg)
}

fn append_course_track_path(
    elements: &mut Vec<LegDisplayElement>,
    debug_sources: &mut Vec<String>,
    current_position: LatLon,
    current_course_deg: Option<f64>,
    current_altitude_ft: &mut Option<f64>,
    step: &ProcedureLegMaterializationRecord,
    termination: TrackTermination,
) -> Option<(LatLon, f64)> {
    let course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    if let TrackTermination::ToFix(fix) = termination {
        if distance_between_points_nm(current_position, fix) <= MIN_GEOMETRY_DISTANCE_NM {
            return Some((current_position, course_deg));
        }
    }
    let course_anchor = step.defining_nav_position.or(step.nav_position)?;
    let track_limit = track_limit_position(&termination);
    let on_track_tolerance_nm = match termination {
        TrackTermination::ToDme { .. } => 0.5,
        _ => MIN_GEOMETRY_DISTANCE_NM,
    };
    let track_start = if distance_between_points_nm(current_position, course_anchor)
        <= MIN_GEOMETRY_DISTANCE_NM
    {
        if matches!(termination, TrackTermination::ToAltitude(_)) {
            if let Some(current_heading_deg) = current_course_deg {
                let heading_delta_deg = angular_difference_degrees(current_heading_deg, course_deg);
                if heading_delta_deg > 1.0 {
                    let turn_clockwise = shortest_turn_clockwise(current_heading_deg, course_deg);
                    let prior_len = elements.len();
                    append_heading_change(
                        elements,
                        current_position,
                        current_heading_deg,
                        course_deg,
                        turn_clockwise,
                        0.0,
                        missed_approach_turn_radius_nm(),
                    );
                    extend_sources_for_new_elements(
                        debug_sources,
                        prior_len,
                        elements,
                        debug_source!(),
                    );
                    elements
                        .last()
                        .and_then(display_element_end_position)
                        .unwrap_or(current_position)
                } else {
                    current_position
                }
            } else {
                current_position
            }
        } else {
            current_position
        }
    } else if let Some(current_heading_deg) = current_course_deg {
        if matches!(termination, TrackTermination::ToDme { .. })
            && angular_difference_degrees(current_heading_deg, course_deg) <= 20.0
        {
            current_position
        } else
        if current_position_is_on_track(
            current_position,
            course_anchor,
            course_deg,
            on_track_tolerance_nm,
        )
            && angular_difference_degrees(current_heading_deg, course_deg) <= 5.0
        {
            current_position
        } else if matches!(termination, TrackTermination::ToAltitude(_)) {
            let Some((join_elements, intercept)) = best_nominal_intercept_track_join(
                current_position,
                current_heading_deg,
                None,
                course_anchor,
                course_deg,
                track_limit,
                missed_approach_turn_radius_nm(),
            ) else {
                return None;
            };
            extend_elements_with_sources(
                elements,
                debug_sources,
                join_elements,
                Vec::new(),
                debug_source!(),
            );
            intercept
        } else if let Some(turn_clockwise) = cf_turn_direction(step) {
            if let Some((join_elements, intercept)) = directed_track_join_elements(
                current_position,
                current_heading_deg,
                turn_clockwise,
                course_anchor,
                course_deg,
                track_limit,
                missed_approach_turn_radius_nm(),
            ) {
                extend_elements_with_sources(
                    elements,
                    debug_sources,
                    join_elements,
                    Vec::new(),
                    debug_source!(),
                );
                intercept
            } else {
                let (join_elements, intercept) = best_nominal_intercept_track_join(
                    current_position,
                    current_heading_deg,
                    Some(turn_clockwise),
                    course_anchor,
                    course_deg,
                    track_limit,
                    missed_approach_turn_radius_nm(),
                )?;
                extend_elements_with_sources(
                    elements,
                    debug_sources,
                    join_elements,
                    Vec::new(),
                    debug_source!(),
                );
                intercept
            }
        } else {
            let direct_intercept = intersect_heading_with_course(
                current_position,
                current_heading_deg,
                course_anchor,
                course_deg,
                track_limit.unwrap_or(course_anchor),
            )
            .filter(|intercept| {
                track_intercept_is_reasonable(
                    current_position,
                    *intercept,
                    course_anchor,
                    course_deg,
                    track_limit,
                )
            });
            let intercept = if let Some(intercept) = direct_intercept {
                let intercept_distance_nm = distance_between_points_nm(current_position, intercept);
                if intercept_distance_nm <= NEAR_INTERCEPT_SNAP_DISTANCE_NM {
                    if let TrackTermination::ToFix(fix) = termination {
                        let direct_to_fix_course_deg = bearing_from(current_position, fix);
                        if angular_difference_degrees(current_heading_deg, direct_to_fix_course_deg)
                            > 5.0
                        {
                            let turn_prior_len = elements.len();
                            let turn_end = append_heading_change(
                                elements,
                                current_position,
                                current_heading_deg,
                                direct_to_fix_course_deg,
                                shortest_turn_clockwise(current_heading_deg, direct_to_fix_course_deg),
                                0.0,
                                missed_approach_turn_radius_nm(),
                            );
                            extend_sources_for_new_elements(
                                debug_sources,
                                turn_prior_len,
                                elements,
                                debug_source!(),
                            );
                            turn_end
                        } else {
                            current_position
                        }
                    } else {
                        current_position
                    }
                } else if intercept_distance_nm > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, intercept);
                    intercept
                } else {
                    current_position
                }
            } else {
                let (join_elements, intercept) = best_nominal_intercept_track_join(
                    current_position,
                    current_heading_deg,
                    None,
                    course_anchor,
                    course_deg,
                    track_limit,
                    missed_approach_turn_radius_nm(),
                )?;
                extend_elements_with_sources(
                    elements,
                    debug_sources,
                    join_elements,
                    Vec::new(),
                    debug_source!(),
                );
                intercept
            };
            intercept
        }
    } else {
        match termination {
            TrackTermination::ToFix(fix) => {
                if distance_between_points_nm(current_position, fix) > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, fix);
                }
                fix
            }
            TrackTermination::ToAltitude(_) => return None,
            TrackTermination::ToDme { center, radius_nm } => {
                let end =
                    forward_heading_circle_intersection(current_position, course_deg, center, radius_nm)?;
                if distance_between_points_nm(current_position, end) > MIN_GEOMETRY_DISTANCE_NM {
                    push_segment!(elements, debug_sources, current_position, end);
                }
                end
            }
        }
    };

    let final_position = match termination {
        TrackTermination::ToFix(fix) => {
            let distance_to_fix_nm = distance_between_points_nm(track_start, fix);
            if distance_to_fix_nm <= TO_FIX_TERMINATION_SNAP_DISTANCE_NM {
                if let Some(last_element) = elements.last_mut() {
                    set_display_element_end_position(last_element, fix);
                }
            } else if distance_to_fix_nm > MIN_GEOMETRY_DISTANCE_NM {
                push_segment!(elements, debug_sources, track_start, fix);
            }
            fix
        }
        TrackTermination::ToAltitude(target_altitude_ft) => extend_climb_segment(
            elements,
            track_start,
            course_deg,
            current_altitude_ft,
            target_altitude_ft,
        ),
        TrackTermination::ToDme { center, radius_nm } => {
            let end = forward_heading_circle_intersection(track_start, course_deg, center, radius_nm)?;
            if distance_between_points_nm(track_start, end) > MIN_GEOMETRY_DISTANCE_NM {
                push_segment!(elements, debug_sources, track_start, end);
            }
            end
        }
    };
    Some((final_position, course_deg))
}

fn best_nominal_intercept_track_join(
    current_position: LatLon,
    current_heading_deg: f64,
    forced_turn_clockwise: Option<bool>,
    course_anchor: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
    turn_radius_nm: f64,
) -> Option<(Vec<LegDisplayElement>, LatLon)> {
    let mut best: Option<(f64, Vec<LegDisplayElement>, LatLon)> = None;
    for intercept_heading_deg in [
        normalize_bearing_degrees(course_deg - NOMINAL_COURSE_INTERCEPT_ANGLE_DEG),
        normalize_bearing_degrees(course_deg + NOMINAL_COURSE_INTERCEPT_ANGLE_DEG),
    ] {
        let turn_clockwise =
            forced_turn_clockwise.unwrap_or_else(|| shortest_turn_clockwise(current_heading_deg, intercept_heading_deg));
        let sweep = heading_sweep_degrees(current_heading_deg, intercept_heading_deg, turn_clockwise);
        if sweep < 1.0 || sweep > 270.0 {
            continue;
        }
        for extra_straight_nm in (0..=8).map(|index| index as f64 * 0.25) {
            if extra_straight_nm > MAX_EXTRA_STRAIGHT_BEFORE_VI_TURN_NM {
                continue;
            }
            let Some(candidate) = build_track_join_candidate(
                current_position,
                current_heading_deg,
                turn_clockwise,
                intercept_heading_deg,
                extra_straight_nm,
                course_anchor,
                course_deg,
                track_limit,
                turn_radius_nm,
            ) else {
                continue;
            };
            let score = sweep + (extra_straight_nm * 5.0);
            if best
                .as_ref()
                .is_none_or(|(best_score, _, _)| score < *best_score)
            {
                best = Some((score, candidate.elements, candidate.intercept));
            }
            break;
        }
    }
    best.map(|(_, elements, intercept)| (elements, intercept))
}

fn track_limit_position(termination: &TrackTermination) -> Option<LatLon> {
    match termination {
        TrackTermination::ToFix(fix) => Some(*fix),
        TrackTermination::ToAltitude(_) => None,
        TrackTermination::ToDme { .. } => None,
    }
}

fn extend_climb_segment(
    elements: &mut Vec<LegDisplayElement>,
    current_position: LatLon,
    course_deg: f64,
    current_altitude_ft: &mut Option<f64>,
    target_altitude_ft: Option<f64>,
) -> LatLon {
    let (Some(start_alt_ft), Some(target_alt_ft)) = (*current_altitude_ft, target_altitude_ft) else {
        return current_position;
    };
    let climb_minutes = ((target_alt_ft - start_alt_ft).max(0.0)) / NOMINAL_MISSED_APPROACH_CLIMB_FTPM;
    let climb_distance_nm =
        distance_nm_for_minutes_at_speed_kt(NOMINAL_MISSED_APPROACH_GROUND_SPEED_KT, climb_minutes);
    if climb_distance_nm <= MIN_GEOMETRY_DISTANCE_NM {
        *current_altitude_ft = Some(target_alt_ft);
        return current_position;
    }
    let climb_end = destination_point(current_position, course_deg, climb_distance_nm);
    elements.push(LegDisplayElement::Segment {
        start: current_position,
        end: climb_end,
    });
    *current_altitude_ft = Some(target_alt_ft);
    climb_end
}

fn append_heading_leg_path(
    elements: &mut Vec<LegDisplayElement>,
    step: &ProcedureLegMaterializationRecord,
    current_position: LatLon,
    current_course_deg: Option<f64>,
    current_altitude_ft: &mut Option<f64>,
    next_step: Option<&ProcedureLegMaterializationRecord>,
) -> Option<LatLon> {
    let initial_course_deg = current_course_deg?;
    let mut target_heading_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    let raw_turn_direction = step.turn_direction.as_deref().unwrap_or("").trim();
    let nominal_turn_clockwise = match raw_turn_direction {
        "L" => false,
        "R" => true,
        _ => true,
    };
    if let Some(next_step) = next_step {
        if next_step.path_termination.trim() == "CF"
            && next_step.defining_nav_position.is_some()
            && !raw_turn_direction.is_empty()
        {
            let next_magnetic_course_deg = next_step.magnetic_course_deg?;
            let next_course_deg =
                next_step.magnetic_course_deg? + course_reference_variation_deg(next_step);
            if angular_difference_degrees(step.magnetic_course_deg?, next_magnetic_course_deg) <= 1.0
            {
                target_heading_deg = intercept_heading_for_course(
                    next_course_deg,
                    nominal_turn_clockwise,
                    NOMINAL_COURSE_INTERCEPT_ANGLE_DEG,
                );
            }
        }
    }
    let heading_delta_deg = angular_difference_degrees(initial_course_deg, target_heading_deg);
    let turn_clockwise = match raw_turn_direction {
        "L" => false,
        "R" => true,
        _ => shortest_turn_clockwise(initial_course_deg, target_heading_deg),
    };
    let turn_radius_nm = missed_approach_turn_radius_nm();
    let mut path_position = current_position;
    if heading_delta_deg > 1.0 {
        let mut extra_straight_nm = 0.0;
        if let Some(next_step) = next_step {
            if next_step.path_termination.trim() == "CF" && next_step.defining_nav_position.is_some()
            {
                let next_fix = next_step.nav_position?;
                let next_course_deg =
                    next_step.magnetic_course_deg? + course_reference_variation_deg(next_step);
                let next_defining_nav = next_step.defining_nav_position?;
                if let Some(candidate_extra_straight_nm) = delayed_turn_start_for_track_capture(
                    current_position,
                    initial_course_deg,
                    target_heading_deg,
                    turn_clockwise,
                    turn_radius_nm,
                    next_defining_nav,
                    next_course_deg,
                    Some(next_fix),
                ) {
                    if candidate_extra_straight_nm > MAX_EXTRA_STRAIGHT_BEFORE_VI_TURN_NM {
                        panic!(
                            "needed {:.2}nm extra straight before {} turn for {} {} seq {}",
                            candidate_extra_straight_nm,
                            step.path_termination.trim(),
                            step.key.airport_id.trim(),
                            step.key.procedure_id.trim(),
                            step.sequence
                        );
                    }
                    extra_straight_nm = candidate_extra_straight_nm;
                }
            }
        }
        path_position = append_heading_change(
            elements,
            current_position,
            initial_course_deg,
            target_heading_deg,
            turn_clockwise,
            extra_straight_nm,
            turn_radius_nm,
        );
    }

    if step.path_termination.trim() == "VA" {
        path_position = extend_climb_segment(
            elements,
            path_position,
            target_heading_deg,
            current_altitude_ft,
            step.altitude_1_ft,
        );
    }

    if step.path_termination.trim() == "VM" && next_step.is_none() {
        let extension_end = destination_point(
            path_position,
            target_heading_deg,
            NOMINAL_MANUAL_TERMINATION_DISTANCE_NM,
        );
        elements.push(LegDisplayElement::Segment {
            start: path_position,
            end: extension_end,
        });
        path_position = extension_end;
    }

    Some(path_position)
}

fn heading_to_dme_distance_path(
    step: &ProcedureLegMaterializationRecord,
    start: LatLon,
    current_course_deg: Option<f64>,
) -> Option<(Vec<LegDisplayElement>, LatLon, f64)> {
    let center = step.defining_nav_position?;
    let target_heading_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    let target_radius_nm = parse_distance_tenths_nm(step.route_distance_or_time.as_deref())?;
    let mut elements = Vec::new();
    let mut path_position = start;

    if let Some(initial_course_deg) = current_course_deg {
        let heading_delta_deg = angular_difference_degrees(initial_course_deg, target_heading_deg);
        if heading_delta_deg > 1.0 {
            let turn_clockwise = match step.turn_direction.as_deref().unwrap_or("").trim() {
                "L" => false,
                "R" => true,
                _ => shortest_turn_clockwise(initial_course_deg, target_heading_deg),
            };
            path_position = append_heading_change(
                &mut elements,
                path_position,
                initial_course_deg,
                target_heading_deg,
                turn_clockwise,
                0.0,
                missed_approach_turn_radius_nm(),
            );
        }
    }

    let end =
        forward_heading_circle_intersection(path_position, target_heading_deg, center, target_radius_nm)?;
    if distance_between_points_nm(path_position, end) <= MIN_GEOMETRY_DISTANCE_NM {
        return None;
    }
    elements.push(LegDisplayElement::Segment {
        start: path_position,
        end,
    });
    Some((elements, end, target_heading_deg))
}

fn heading_to_radial_termination_path(
    step: &ProcedureLegMaterializationRecord,
    start: LatLon,
    _current_course_deg: Option<f64>,
) -> Option<(Vec<LegDisplayElement>, LatLon, f64)> {
    let radial_anchor = step.defining_nav_position?;
    let flown_course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    let radial_deg = step.theta_deg? + course_reference_variation_deg(step);
    let mut elements = Vec::new();
    let end = intersect_heading_with_course(
        start,
        flown_course_deg,
        radial_anchor,
        radial_deg,
        radial_anchor,
    )?;
    if distance_between_points_nm(start, end) <= MIN_GEOMETRY_DISTANCE_NM {
        return None;
    }
    elements.push(LegDisplayElement::Segment { start, end });
    Some((elements, end, flown_course_deg))
}

fn course_to_intercept_path(
    step: &ProcedureLegMaterializationRecord,
    start: LatLon,
    current_course_deg: Option<f64>,
    intercept_step: Option<&ProcedureLegMaterializationRecord>,
) -> Option<(Vec<LegDisplayElement>, LatLon, f64)> {
    let flown_course_deg = step.magnetic_course_deg? + course_reference_variation_deg(step);
    let mut elements = Vec::new();
    let mut path_position = start;

    if let Some(initial_course_deg) = current_course_deg {
        let heading_delta_deg = angular_difference_degrees(initial_course_deg, flown_course_deg);
        if heading_delta_deg > 1.0 {
            let turn_clockwise = match step.turn_direction.as_deref().unwrap_or("").trim() {
                "L" => false,
                "R" => true,
                _ => shortest_turn_clockwise(initial_course_deg, flown_course_deg),
            };
            path_position = append_heading_change(
                &mut elements,
                path_position,
                initial_course_deg,
                flown_course_deg,
                turn_clockwise,
                0.0,
                missed_approach_turn_radius_nm(),
            );
        }
    }

    let intercept_step = intercept_step?;
    if intercept_step.path_termination.trim() != "CF" {
        return None;
    }
    let next_fix = intercept_step.nav_position?;
    let next_defining_nav = intercept_step.defining_nav_position?;
    let next_course_deg =
        intercept_step.magnetic_course_deg? + course_reference_variation_deg(intercept_step);
    let intercept = intersect_heading_with_course(
        path_position,
        flown_course_deg,
        next_defining_nav,
        next_course_deg,
        next_fix,
    )
    .filter(|candidate| {
        track_intercept_is_reasonable(
            path_position,
            *candidate,
            next_defining_nav,
            next_course_deg,
            Some(next_fix),
        )
    })?;
    if distance_between_points_nm(path_position, intercept) > MIN_GEOMETRY_DISTANCE_NM {
        elements.push(LegDisplayElement::Segment {
            start: path_position,
            end: intercept,
        });
    }
    Some((elements, intercept, flown_course_deg))
}

fn delayed_turn_start_for_track_capture(
    current_position: LatLon,
    current_heading_deg: f64,
    target_heading_deg: f64,
    turn_clockwise: bool,
    turn_radius_nm: f64,
    course_anchor: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
) -> Option<f64> {
    for extra_straight_nm in (0..=8).map(|index| index as f64 * 0.25) {
        let turn_end = preview_heading_change_end(
            current_position,
            current_heading_deg,
            target_heading_deg,
            turn_clockwise,
            extra_straight_nm,
            turn_radius_nm,
        );
        let Some(intercept) = true_intersect_heading_with_course(
            turn_end,
            target_heading_deg,
            course_anchor,
            course_deg,
        ) else {
            continue;
        };
        if track_intercept_is_reasonable(
            turn_end,
            intercept,
            course_anchor,
            course_deg,
            track_limit,
        ) {
            return Some(extra_straight_nm);
        }
    }
    None
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

fn preview_heading_change_end(
    current_position: LatLon,
    initial_course_deg: f64,
    target_heading_deg: f64,
    turn_clockwise: bool,
    extra_straight_nm: f64,
    turn_radius_nm: f64,
) -> LatLon {
    if angular_difference_degrees(initial_course_deg, target_heading_deg) <= 1.0 {
        return if extra_straight_nm <= 0.0 {
            current_position
        } else {
            destination_point(current_position, initial_course_deg, extra_straight_nm)
        };
    }
    let turn_start = if extra_straight_nm <= 0.0 {
        current_position
    } else {
        destination_point(current_position, initial_course_deg, extra_straight_nm)
    };
    let turn_center = turn_center_for_heading_change(
        turn_start,
        initial_course_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    point_on_turn_center(
        turn_center,
        target_heading_deg,
        turn_clockwise,
        turn_radius_nm,
    )
}

fn append_heading_change(
    elements: &mut Vec<LegDisplayElement>,
    current_position: LatLon,
    initial_course_deg: f64,
    target_heading_deg: f64,
    turn_clockwise: bool,
    extra_straight_nm: f64,
    turn_radius_nm: f64,
) -> LatLon {
    let turn_start = if extra_straight_nm <= 0.0 {
        current_position
    } else {
        destination_point(current_position, initial_course_deg, extra_straight_nm)
    };
    if distance_between_points_nm(current_position, turn_start) > MIN_GEOMETRY_DISTANCE_NM {
        elements.push(LegDisplayElement::Segment {
            start: current_position,
            end: turn_start,
        });
    }
    if angular_difference_degrees(initial_course_deg, target_heading_deg) <= 1.0 {
        return turn_start;
    }
    let turn_center = turn_center_for_heading_change(
        turn_start,
        initial_course_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    let turn_end = point_on_turn_center(
        turn_center,
        target_heading_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    elements.push(LegDisplayElement::Arc {
        center: turn_center,
        radius_nm: turn_radius_nm,
        start: turn_start,
        end: turn_end,
        clockwise: turn_clockwise,
        sweep_degrees: heading_sweep_degrees(
            initial_course_deg,
            target_heading_deg,
            turn_clockwise,
        ),
    });
    turn_end
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
        .max_by(|left, right| {
            left.2
                .partial_cmp(&right.2)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(turn_end, rejoin_course_deg, _)| (turn_end, rejoin_course_deg))
}

fn cf_turn_direction(step: &ProcedureLegMaterializationRecord) -> Option<bool> {
    match step.turn_direction.as_deref().unwrap_or("").trim() {
        "L" => Some(false),
        "R" => Some(true),
        _ => None,
    }
}

fn directed_track_join_elements(
    current_position: LatLon,
    current_heading_deg: f64,
    turn_clockwise: bool,
    course_anchor: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
    turn_radius_nm: f64,
) -> Option<(Vec<LegDisplayElement>, LatLon)> {
    let heading_change_deg = angular_difference_degrees(current_heading_deg, course_deg);
    if heading_change_deg >= 175.0 {
        return best_near_reciprocal_track_join(
            current_position,
            current_heading_deg,
            turn_clockwise,
            course_anchor,
            course_deg,
            track_limit,
            turn_radius_nm,
        );
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
    if straight_nm < -MIN_GEOMETRY_DISTANCE_NM {
        return None;
    }
    if !track_intercept_is_reasonable(
        current_position,
        turn_end,
        course_anchor,
        course_deg,
        track_limit,
    ) {
        return None;
    }
    let turn_start = if straight_nm <= MIN_GEOMETRY_DISTANCE_NM {
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
    let turn_end_from_center =
        point_on_turn_center(turn_center, course_deg, turn_clockwise, turn_radius_nm);
    let has_turn_arc =
        distance_between_points_nm(turn_start, turn_end_from_center) > MIN_GEOMETRY_DISTANCE_NM;
    if heading_change_deg > 10.0 && !has_turn_arc {
        return None;
    }
    let mut elements = Vec::new();
    if distance_between_points_nm(current_position, turn_start) > MIN_GEOMETRY_DISTANCE_NM {
        elements.push(LegDisplayElement::Segment {
            start: current_position,
            end: turn_start,
        });
    }
    if has_turn_arc {
        elements.push(LegDisplayElement::Arc {
            center: turn_center,
            radius_nm: turn_radius_nm,
            start: turn_start,
            end: turn_end_from_center,
            clockwise: turn_clockwise,
            sweep_degrees: heading_sweep_degrees(current_heading_deg, course_deg, turn_clockwise),
        });
    }
    if !directed_join_connector_is_reasonable(turn_end_from_center, turn_end, course_deg) {
        return None;
    }
    let returned_turn_end = if distance_between_points_nm(turn_end_from_center, turn_end)
        > MIN_GEOMETRY_DISTANCE_NM
    {
        elements.push(LegDisplayElement::Segment {
            start: turn_end_from_center,
            end: turn_end,
        });
        turn_end
    } else {
        turn_end_from_center
    };
    Some((elements, returned_turn_end))
}

fn best_near_reciprocal_track_join(
    current_position: LatLon,
    current_heading_deg: f64,
    turn_clockwise: bool,
    course_anchor: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
    turn_radius_nm: f64,
) -> Option<(Vec<LegDisplayElement>, LatLon)> {
    let mut best: Option<(f64, Vec<LegDisplayElement>, LatLon)> = None;
    for intercept_heading_deg in [
        normalize_bearing_degrees(course_deg - NOMINAL_COURSE_INTERCEPT_ANGLE_DEG),
        normalize_bearing_degrees(course_deg + NOMINAL_COURSE_INTERCEPT_ANGLE_DEG),
    ] {
        let sweep =
            heading_sweep_degrees(current_heading_deg, intercept_heading_deg, turn_clockwise);
        if sweep < 1.0 || sweep > 270.0 {
            continue;
        }
        for extra_straight_nm in (0..=20).map(|index| index as f64 * 0.25) {
            let Some(candidate) = build_track_join_candidate(
                current_position,
                current_heading_deg,
                turn_clockwise,
                intercept_heading_deg,
                extra_straight_nm,
                course_anchor,
                course_deg,
                track_limit,
                turn_radius_nm,
            ) else {
                continue;
            };
            let on_course_distance_nm = track_limit
                .map(|limit| distance_between_points_nm(candidate.intercept, limit))
                .unwrap_or(0.0);
            let final_segment_heading_deg = if distance_between_points_nm(
                candidate.turn_end,
                candidate.intercept,
            ) > MIN_GEOMETRY_DISTANCE_NM
            {
                bearing_from(candidate.turn_end, candidate.intercept)
            } else {
                intercept_heading_deg
            };
            let course_alignment_penalty =
                angular_difference_degrees(final_segment_heading_deg, course_deg) * 10.0;
            let intercept_heading_penalty =
                angular_difference_degrees(intercept_heading_deg, course_deg) * 0.1;
            let on_course_reward = on_course_distance_nm * 25.0;
            let score =
                course_alignment_penalty + intercept_heading_penalty + sweep + extra_straight_nm
                    - on_course_reward;
            if best
                .as_ref()
                .is_none_or(|(best_score, _, _)| score < *best_score)
            {
                best = Some((score, candidate.elements, candidate.intercept));
            }
            break;
        }
    }
    best.map(|(_, elements, intercept)| (elements, intercept))
}

struct TrackJoinCandidate {
    elements: Vec<LegDisplayElement>,
    turn_end: LatLon,
    intercept: LatLon,
}

fn build_track_join_candidate(
    current_position: LatLon,
    current_heading_deg: f64,
    turn_clockwise: bool,
    intercept_heading_deg: f64,
    extra_straight_nm: f64,
    course_anchor: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
    turn_radius_nm: f64,
) -> Option<TrackJoinCandidate> {
    let turn_start = if extra_straight_nm <= 0.0 {
        current_position
    } else {
        destination_point(current_position, current_heading_deg, extra_straight_nm)
    };
    let turn_center = turn_center_for_heading_change(
        turn_start,
        current_heading_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    let turn_end = point_on_turn_center(
        turn_center,
        intercept_heading_deg,
        turn_clockwise,
        turn_radius_nm,
    );
    let intercept = true_intersect_heading_with_course(
        turn_end,
        intercept_heading_deg,
        course_anchor,
        course_deg,
    )?;
    if !track_intercept_is_reasonable(
        turn_end,
        intercept,
        course_anchor,
        course_deg,
        track_limit,
    ) {
        return None;
    }
    let mut elements = Vec::new();
    if distance_between_points_nm(current_position, turn_start) > MIN_GEOMETRY_DISTANCE_NM {
        elements.push(LegDisplayElement::Segment {
            start: current_position,
            end: turn_start,
        });
    }
    elements.push(LegDisplayElement::Arc {
        center: turn_center,
        radius_nm: turn_radius_nm,
        start: turn_start,
        end: turn_end,
        clockwise: turn_clockwise,
        sweep_degrees: heading_sweep_degrees(
            current_heading_deg,
            intercept_heading_deg,
            turn_clockwise,
        ),
    });
    let returned_intercept = if distance_between_points_nm(turn_end, intercept)
        > MIN_GEOMETRY_DISTANCE_NM
    {
        elements.push(LegDisplayElement::Segment {
            start: turn_end,
            end: intercept,
        });
        intercept
    } else {
        turn_end
    };
    Some(TrackJoinCandidate {
        elements,
        turn_end,
        intercept: returned_intercept,
    })
}

fn directed_join_connector_is_reasonable(
    connector_start: LatLon,
    connector_end: LatLon,
    course_deg: f64,
) -> bool {
    if distance_between_points_nm(connector_start, connector_end) <= MIN_GEOMETRY_DISTANCE_NM {
        return true;
    }
    let connector_heading_deg = bearing_from(connector_start, connector_end);
    if angular_difference_degrees(connector_heading_deg, course_deg) > 45.0 {
        return false;
    }
    projection_along_course_nm(connector_start, connector_end, bearing_unit_vector(course_deg))
        >= -MIN_GEOMETRY_DISTANCE_NM
}

fn track_intercept_is_reasonable(
    current_position: LatLon,
    intercept: LatLon,
    defining_nav: LatLon,
    course_deg: f64,
    track_limit: Option<LatLon>,
) -> bool {
    let Some(fix) = track_limit else {
        return true;
    };
    let course_unit = bearing_unit_vector(course_deg);
    let current_projection = projection_along_course_nm(defining_nav, current_position, course_unit);
    let intercept_projection = projection_along_course_nm(defining_nav, intercept, course_unit);
    let fix_projection = projection_along_course_nm(defining_nav, fix, course_unit);
    let lower_bound = current_projection.min(fix_projection) - MIN_GEOMETRY_DISTANCE_NM;
    let upper_bound = current_projection.max(fix_projection) + MIN_GEOMETRY_DISTANCE_NM;
    intercept_projection >= lower_bound && intercept_projection <= upper_bound
}

fn current_position_is_on_track(
    current_position: LatLon,
    course_anchor: LatLon,
    course_deg: f64,
    tolerance_nm: f64,
) -> bool {
    let offset = to_local_en(course_anchor, current_position);
    let course_unit = bearing_unit_vector(course_deg);
    let cross_track_nm = offset.0 * (-course_unit.1) + offset.1 * course_unit.0;
    cross_track_nm.abs() <= tolerance_nm
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

fn intercept_heading_for_course(course_deg: f64, clockwise: bool, intercept_angle_deg: f64) -> f64 {
    if clockwise {
        normalize_bearing_degrees(course_deg - intercept_angle_deg)
    } else {
        normalize_bearing_degrees(course_deg + intercept_angle_deg)
    }
}

fn intersect_heading_with_course(
    ray_start: LatLon,
    ray_heading_deg: f64,
    course_anchor: LatLon,
    course_heading_deg: f64,
    course_fix: LatLon,
) -> Option<LatLon> {
    intersect_heading_with_course_core(
        ray_start,
        ray_heading_deg,
        course_anchor,
        course_heading_deg,
    )
    .or(Some(course_fix))
}

fn true_intersect_heading_with_course(
    ray_start: LatLon,
    ray_heading_deg: f64,
    course_anchor: LatLon,
    course_heading_deg: f64,
) -> Option<LatLon> {
    intersect_heading_with_course_core(
        ray_start,
        ray_heading_deg,
        course_anchor,
        course_heading_deg,
    )
}

fn intersect_heading_with_course_core(
    ray_start: LatLon,
    ray_heading_deg: f64,
    course_anchor: LatLon,
    course_heading_deg: f64,
) -> Option<LatLon> {
    let origin = course_anchor;
    let ray_start_en = to_local_en(origin, ray_start);
    let ray_dir = bearing_unit_vector(ray_heading_deg);
    let course_dir = bearing_unit_vector(course_heading_deg);
    let cross = ray_dir.0 * course_dir.1 - ray_dir.1 * course_dir.0;
    if cross.abs() < 1e-6 {
        return None;
    }
    let delta = (-ray_start_en.0, -ray_start_en.1);
    let t = (delta.0 * course_dir.1 - delta.1 * course_dir.0) / cross;
    if t < 0.0 {
        return None;
    }
    Some(offset_latlon(ray_start, ray_dir.0 * t, ray_dir.1 * t))
}

fn forward_heading_circle_intersection(
    ray_start: LatLon,
    ray_heading_deg: f64,
    circle_center: LatLon,
    radius_nm: f64,
) -> Option<LatLon> {
    let start_en = to_local_en(circle_center, ray_start);
    let ray_dir = bearing_unit_vector(ray_heading_deg);
    let a = ray_dir.0 * ray_dir.0 + ray_dir.1 * ray_dir.1;
    let b = 2.0 * (start_en.0 * ray_dir.0 + start_en.1 * ray_dir.1);
    let c = start_en.0 * start_en.0 + start_en.1 * start_en.1 - radius_nm * radius_nm;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let sqrt_discriminant = discriminant.sqrt();
    let two_a = 2.0 * a;
    let t1 = (-b - sqrt_discriminant) / two_a;
    let t2 = (-b + sqrt_discriminant) / two_a;
    let t = [t1, t2]
        .into_iter()
        .filter(|candidate| *candidate >= 0.0)
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))?;
    Some(offset_latlon(ray_start, ray_dir.0 * t, ray_dir.1 * t))
}

pub(crate) fn display_element_end_course_deg(element: &LegDisplayElement) -> Option<f64> {
    match element {
        LegDisplayElement::Segment { start, end } => Some(bearing_from(*start, *end)),
        LegDisplayElement::Arc {
            center,
            end,
            clockwise,
            ..
        } => {
            let radial_deg = bearing_from(*center, *end);
            Some(if *clockwise {
                normalize_bearing_degrees(radial_deg + 90.0)
            } else {
                normalize_bearing_degrees(radial_deg - 90.0)
            })
        }
    }
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
        return Some(distance_nm_for_minutes_at_speed_kt(
            NOMINAL_HOLD_GROUND_SPEED_KT,
            minutes,
        ));
    }
    parse_distance_tenths_nm(Some(value))
}

fn parse_distance_tenths_nm(distance_or_time: Option<&str>) -> Option<f64> {
    let value = distance_or_time?.trim();
    if value.is_empty() || value.starts_with('T') {
        return None;
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

    let entry_kind = arrival_course_deg.map(|arrival_course_deg| {
        classify_hold_entry(arrival_course_deg, inbound_course_deg, clockwise)
    });
    let mut elements = hold_entry_elements(
        fix,
        inbound_course_deg,
        clockwise,
        arrival_course_deg,
        leg_length_nm,
        turn_radius_nm,
    );
    let mut debug_sources = vec![debug_source!(); elements.len()];
    if stop_when_established_inbound {
        if !matches!(entry_kind, Some(HoldEntryKind::Direct) | None) {
            return solid_path(elements, debug_sources);
        }
    }
    push_arc!(
        elements,
        debug_sources,
        first_turn_center,
        turn_radius_nm,
        inbound_end,
        outbound_start,
        clockwise,
        180.0
    );
    push_segment!(elements, debug_sources, outbound_start, outbound_end);
    push_arc!(
        elements,
        debug_sources,
        second_turn_center,
        turn_radius_nm,
        outbound_end,
        inbound_rejoin,
        clockwise,
        180.0
    );
    push_segment!(elements, debug_sources, inbound_rejoin, inbound_end);

    snap_nearby_display_element_boundaries(&mut elements);
    prune_degenerate_display_elements_with_sources(&mut elements, &mut debug_sources);

    solid_path(elements, debug_sources)
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
            let mut elements = vec![LegDisplayElement::Segment {
                start: fix,
                end: entry_end,
            }];
            if distance_between_points_nm(entry_end, turn_end) > MIN_GEOMETRY_DISTANCE_NM {
                elements.push(LegDisplayElement::Arc {
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
                });
            }
            if distance_between_points_nm(turn_end, fix) > MIN_GEOMETRY_DISTANCE_NM {
                elements.push(LegDisplayElement::Segment {
                    start: turn_end,
                    end: fix,
                });
            }
            elements
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
            let mut elements = vec![LegDisplayElement::Segment {
                start: fix,
                end: entry_end,
            }];
            if distance_between_points_nm(entry_end, turn_end) > MIN_GEOMETRY_DISTANCE_NM {
                elements.push(LegDisplayElement::Arc {
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
                });
            }
            if distance_between_points_nm(turn_end, fix) > MIN_GEOMETRY_DISTANCE_NM {
                elements.push(LegDisplayElement::Segment {
                    start: turn_end,
                    end: fix,
                });
            }
            elements
        }
    }
}

fn nominal_hold_entry_distance_nm(leg_length_nm: f64) -> f64 {
    leg_length_nm.min(distance_nm_for_minutes_at_speed_kt(
        NOMINAL_HOLD_GROUND_SPEED_KT,
        1.0,
    ))
}

fn distance_nm_for_minutes_at_speed_kt(speed_kt: f64, minutes: f64) -> f64 {
    speed_kt * (minutes / 60.0)
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

fn positions_nearly_equal_for_geometry(a: LatLon, b: LatLon) -> bool {
    (a.lat - b.lat).abs() < POSITION_EPSILON_DEG
        && (a.lon - b.lon).abs() < POSITION_EPSILON_DEG
}

fn prune_degenerate_display_elements_with_sources(
    elements: &mut Vec<LegDisplayElement>,
    debug_sources: &mut Vec<String>,
) {
    let original_elements = std::mem::take(elements);
    let original_sources = std::mem::take(debug_sources);
    for (index, element) in original_elements.into_iter().enumerate() {
        let keep = match &element {
            LegDisplayElement::Segment { start, end } => {
                !positions_nearly_equal_for_geometry(*start, *end)
            }
            LegDisplayElement::Arc {
                start,
                end,
                radius_nm,
                sweep_degrees,
                ..
            } => {
                !positions_nearly_equal_for_geometry(*start, *end)
                    && *radius_nm > MIN_GEOMETRY_DISTANCE_NM
                    && sweep_degrees.abs() > MIN_ARC_SWEEP_DEG
            }
        };
        if keep {
            elements.push(element);
            debug_sources.push(
                original_sources
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
            );
        }
    }
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
