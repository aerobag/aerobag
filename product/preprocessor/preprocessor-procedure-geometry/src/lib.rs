#![cfg_attr(test, allow(dead_code))]

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use crate::procedure_geometry_constants::{
    BORROWED_LATER_HOLD_FOR_PI_SOURCE_PREFIX, CF_DIRECT_SHORTCUT_MISALIGNED_SOURCE_PREFIX,
    EXPLICIT_MISSED_TURN_SOURCE_PREFIX, EXPLICIT_PUBLISHED_TURN_SOURCE_PREFIX,
    INFERRED_MISSED_TURN_SOURCE_PREFIX, INVENTED_PI_ENTRY_REVERSAL_SOURCE_PREFIX,
    MAX_APPROACH_DISPLAY_ELEMENT_DISTANCE_NM, MAX_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM,
    MAX_PUBLISHED_HOLD_OR_MISSED_SEGMENT_DISTANCE_NM,
    MAX_SID_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM,
    MAX_STAR_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM, MIN_ARC_SWEEP_DEG,
    MIN_GEOMETRY_DISTANCE_NM, PLATE_EXCEPTION_MISSED_TURN_SOURCE_PREFIX, POSITION_EPSILON_DEG,
};
use app_core::planning::ProcedureLegProvenance;
use app_core::{
    basic_terminal_state, common_resume_candidate_decision, cross_track_left_nm,
    direct_to_fix_with_course_continuation_requirement, great_circle_distance_nm,
    initial_course_deg, reconcile_handoff, reentry_to_anchor_requirement,
    yieldable_course_to_fix_requirement, AirportId, AppError, AppErrorKind, AppResult,
    CommonResumeCandidateInput, ConcretizedNavItem, HandoffDecision, LatLon, LegDisplayElement,
    LegDisplayPath, LegDisplayPathStyle, MaterializedProcedure, NavRef, PathTermination,
    ProcedureDiscontinuity, ProcedureKind, ProcedureOptions, ProcedureSegment,
    ProcedureSegmentRole, ProcedureSpecChoice, ResolvedLeg, ResolvedLegSource, StartRequirement,
    TerminalState,
};
#[cfg(test)]
use app_core::{
    nav_kv_key_for_query, start_requirement_from_leg_characteristics,
    terminal_state_with_leg_characteristics, NavKvLookup, NavKvQuery, NavKvStore,
};
use procedure_geometry_types as pgt;
use serde::{Deserialize, Serialize};

pub mod arinc_ambiguity_resolutions;
pub mod procedure_geometry;
mod procedure_geometry_constants;
pub mod procedure_legs;

pub mod planning {
    pub use app_core::planning::*;
}

pub use procedure_geometry::{
    build_trailing_course_to_intercept_display_path, display_path_for_procedure_leg,
    display_path_for_procedure_leg_before_following_segment, display_path_for_resumed_common_cf,
    display_path_for_single_procedure_step,
    display_path_for_terminal_tf_to_following_common_course, FollowingProcedureSegment,
};
pub use procedure_legs::{
    interpret_path_termination, leading_procedure_discontinuity, terminal_procedure_discontinuity,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureVariantKey {
    pub airport_id: String,
    pub procedure_id: String,
    pub route_type: String,
    pub transition_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureDistinctRow {
    pub route_type: String,
    pub transition_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureLegMaterializationRecord {
    pub key: ProcedureVariantKey,
    pub sequence: i32,
    pub nav_ref: Option<NavRef>,
    pub nav_position: Option<LatLon>,
    pub nav_magnetic_variation_deg: Option<f64>,
    pub defining_nav_ref: Option<NavRef>,
    pub defining_nav_position: Option<LatLon>,
    pub defining_nav_magnetic_variation_deg: Option<f64>,
    pub arc_center_fix_ref: Option<NavRef>,
    pub arc_center_fix_position: Option<LatLon>,
    pub arc_radius_nm: Option<f64>,
    pub airport_magnetic_variation_deg: Option<f64>,
    pub altitude_1_ft: Option<f64>,
    pub altitude_2_ft: Option<f64>,
    pub path_termination: String,
    pub path_termination_kind: PathTermination,
    pub turn_direction: Option<String>,
    pub theta_deg: Option<f64>,
    pub magnetic_course_deg: Option<f64>,
    pub route_distance_or_time: Option<String>,
    pub departure_anchor_ref: Option<NavRef>,
    pub departure_anchor_position: Option<LatLon>,
    pub departure_anchor_altitude_ft: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct ProcedureLegMaterializationRecordSerde {
    key: ProcedureVariantKey,
    sequence: i32,
    nav_ref: Option<NavRef>,
    #[serde(default)]
    nav_position: Option<LatLon>,
    #[serde(default)]
    nav_magnetic_variation_deg: Option<f64>,
    #[serde(default)]
    defining_nav_ref: Option<NavRef>,
    #[serde(default)]
    defining_nav_position: Option<LatLon>,
    #[serde(default)]
    defining_nav_magnetic_variation_deg: Option<f64>,
    #[serde(default)]
    arc_center_fix_ref: Option<NavRef>,
    #[serde(default)]
    arc_center_fix_position: Option<LatLon>,
    #[serde(default)]
    arc_radius_nm: Option<f64>,
    #[serde(default)]
    airport_magnetic_variation_deg: Option<f64>,
    #[serde(default)]
    altitude_1_ft: Option<f64>,
    #[serde(default)]
    altitude_2_ft: Option<f64>,
    path_termination: String,
    #[serde(default)]
    path_termination_kind: Option<PathTermination>,
    #[serde(default)]
    turn_direction: Option<String>,
    #[serde(default)]
    theta_deg: Option<f64>,
    #[serde(default)]
    magnetic_course_deg: Option<f64>,
    #[serde(default)]
    route_distance_or_time: Option<String>,
    #[serde(default)]
    departure_anchor_ref: Option<NavRef>,
    #[serde(default)]
    departure_anchor_position: Option<LatLon>,
    #[serde(default)]
    departure_anchor_altitude_ft: Option<f64>,
}

impl Serialize for ProcedureLegMaterializationRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ProcedureLegMaterializationRecordSerde {
            key: self.key.clone(),
            sequence: self.sequence,
            nav_ref: self.nav_ref.clone(),
            nav_position: self.nav_position,
            nav_magnetic_variation_deg: self.nav_magnetic_variation_deg,
            defining_nav_ref: self.defining_nav_ref.clone(),
            defining_nav_position: self.defining_nav_position,
            defining_nav_magnetic_variation_deg: self.defining_nav_magnetic_variation_deg,
            arc_center_fix_ref: self.arc_center_fix_ref.clone(),
            arc_center_fix_position: self.arc_center_fix_position,
            arc_radius_nm: self.arc_radius_nm,
            airport_magnetic_variation_deg: self.airport_magnetic_variation_deg,
            altitude_1_ft: self.altitude_1_ft,
            altitude_2_ft: self.altitude_2_ft,
            path_termination: self.path_termination.clone(),
            path_termination_kind: Some(self.path_termination_kind.clone()),
            turn_direction: self.turn_direction.clone(),
            theta_deg: self.theta_deg,
            magnetic_course_deg: self.magnetic_course_deg,
            route_distance_or_time: self.route_distance_or_time.clone(),
            departure_anchor_ref: self.departure_anchor_ref.clone(),
            departure_anchor_position: self.departure_anchor_position,
            departure_anchor_altitude_ft: self.departure_anchor_altitude_ft,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcedureLegMaterializationRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ProcedureLegMaterializationRecordSerde::deserialize(deserializer)?;
        Ok(Self {
            key: raw.key,
            sequence: raw.sequence,
            nav_ref: raw.nav_ref,
            nav_position: raw.nav_position,
            nav_magnetic_variation_deg: raw.nav_magnetic_variation_deg,
            defining_nav_ref: raw.defining_nav_ref,
            defining_nav_position: raw.defining_nav_position,
            defining_nav_magnetic_variation_deg: raw.defining_nav_magnetic_variation_deg,
            arc_center_fix_ref: raw.arc_center_fix_ref,
            arc_center_fix_position: raw.arc_center_fix_position,
            arc_radius_nm: raw.arc_radius_nm,
            airport_magnetic_variation_deg: raw.airport_magnetic_variation_deg,
            altitude_1_ft: raw.altitude_1_ft,
            altitude_2_ft: raw.altitude_2_ft,
            path_termination_kind: raw
                .path_termination_kind
                .unwrap_or_else(|| interpret_path_termination(&raw.path_termination)),
            path_termination: raw.path_termination,
            turn_direction: raw.turn_direction,
            theta_deg: raw.theta_deg,
            magnetic_course_deg: raw.magnetic_course_deg,
            route_distance_or_time: raw.route_distance_or_time,
            departure_anchor_ref: raw.departure_anchor_ref,
            departure_anchor_position: raw.departure_anchor_position,
            departure_anchor_altitude_ft: raw.departure_anchor_altitude_ft,
        })
    }
}

pub fn describe_procedure_options_from_rows(
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    rows: Vec<ProcedureDistinctRow>,
) -> AppResult<ProcedureOptions> {
    if kind == ProcedureKind::Approach {
        let enroute_transitions = rows
            .iter()
            .filter(|row| row.route_type == "A")
            .map(|row| row.transition_id.clone())
            .filter(|transition| !transition.is_empty() && transition != "ALL")
            .collect::<Vec<_>>();
        let has_common_segment = approach_common_route_type(&rows).is_some();
        let valid_choices = if enroute_transitions.is_empty() {
            vec![ProcedureSpecChoice {
                runway_transition: None,
                enroute_transition: None,
            }]
        } else {
            enroute_transitions
                .iter()
                .cloned()
                .map(|enroute_transition| ProcedureSpecChoice {
                    runway_transition: None,
                    enroute_transition: Some(enroute_transition),
                })
                .collect::<Vec<_>>()
        };

        return Ok(ProcedureOptions {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            kind,
            runway_transitions: Vec::new(),
            enroute_transitions,
            has_common_segment,
            valid_choices,
        });
    }

    let layout = procedure_layout(kind.clone());
    let mut runway_transitions = rows
        .iter()
        .filter(|row| layout.runway_route_types.contains(&row.route_type.as_str()))
        .map(|row| row.transition_id.clone())
        .filter(|transition| !transition.is_empty() && transition != "ALL")
        .collect::<Vec<_>>();
    runway_transitions.sort();
    runway_transitions.dedup();
    let mut enroute_transitions = rows
        .iter()
        .filter(|row| {
            layout
                .enroute_route_types
                .contains(&row.route_type.as_str())
        })
        .map(|row| row.transition_id.clone())
        .filter(|transition| !transition.is_empty() && transition != "ALL")
        .collect::<Vec<_>>();
    enroute_transitions.sort();
    enroute_transitions.dedup();
    let has_common_segment = rows
        .iter()
        .any(|row| layout.common_route_types.contains(&row.route_type.as_str()));
    let has_supported_segment =
        has_common_segment || !runway_transitions.is_empty() || !enroute_transitions.is_empty();

    let runway_choices = if runway_transitions.is_empty() {
        vec![None]
    } else {
        runway_transitions
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
    };
    let enroute_choices = if enroute_transitions.is_empty() {
        vec![None]
    } else {
        enroute_transitions
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
    };
    let valid_choices = if has_supported_segment {
        runway_choices
            .into_iter()
            .flat_map(|runway_transition| {
                enroute_choices
                    .iter()
                    .cloned()
                    .map(move |enroute_transition| ProcedureSpecChoice {
                        runway_transition: runway_transition.clone(),
                        enroute_transition,
                    })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    Ok(ProcedureOptions {
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind,
        runway_transitions,
        enroute_transitions,
        has_common_segment,
        valid_choices,
    })
}

fn bearing_degrees(from: LatLon, to: LatLon) -> f64 {
    initial_course_deg(from, to)
}

fn angular_difference_degrees(left: f64, right: f64) -> f64 {
    let mut delta = (normalize_bearing_degrees(left) - normalize_bearing_degrees(right)).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    delta
}

fn normalize_bearing_degrees(bearing_deg: f64) -> f64 {
    bearing_deg.rem_euclid(360.0)
}

fn route_destination_point(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    let angular_distance = distance_nm / 3440.065;
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
        lon: lon2.to_degrees(),
    }
}

pub struct MaterializeProcedureInput<'a> {
    pub airport_id: &'a str,
    pub procedure_id: &'a str,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    pub component_index: usize,
    pub rows: Vec<ProcedureDistinctRow>,
    pub legs: Vec<ProcedureLegMaterializationRecord>,
}

pub fn materialize_procedure_from_records(
    input: MaterializeProcedureInput<'_>,
) -> AppResult<MaterializedProcedure> {
    let MaterializeProcedureInput {
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        component_index,
        rows,
        mut legs,
    } = input;
    let options =
        describe_procedure_options_from_rows(airport_id, procedure_id, kind.clone(), rows.clone())?;
    let requested = ProcedureSpecChoice {
        runway_transition: runway_transition
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        enroute_transition: enroute_transition
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    if !options
        .valid_choices
        .iter()
        .any(|choice| choice == &requested)
    {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "invalid procedure selection for {} {}: runway={:?} enroute={:?}",
                airport_id.trim(),
                procedure_id.trim(),
                requested.runway_transition,
                requested.enroute_transition
            ),
        });
    }

    let mut data_quality = arinc_ambiguity_resolutions::repair_known_bad_procedure_fields(
        airport_id,
        procedure_id,
        &mut legs,
    );

    let mut segments = Vec::<(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )>::new();

    if kind == ProcedureKind::Approach {
        let common_route_type = approach_common_route_type(&rows);
        let common_legs = common_route_type.as_deref().map(|route_type| {
            filter_procedure_records(&legs, airport_id, procedure_id, route_type, "")
        });

        if let Some(enroute_transition) = requested.enroute_transition.as_deref() {
            for mut transition_legs in chained_approach_transition_segments(
                &legs,
                airport_id,
                procedure_id,
                enroute_transition,
                common_legs.as_deref(),
            ) {
                if let Some(common_legs) = common_legs.as_deref() {
                    borrow_sibling_transition_hold_for_common_if_course_reversal(
                        &legs,
                        &mut transition_legs,
                        common_legs,
                        &mut data_quality,
                    );
                }
                let items = concretize_procedure_materialization_legs(&transition_legs, false);
                segments.push((
                    MaterializedSegmentRole::EnrouteTransition,
                    transition_legs,
                    items,
                    false,
                ));
            }
        }

        if let Some(common_legs) = common_legs {
            let items = concretize_procedure_materialization_legs(&common_legs, false);
            segments.push((MaterializedSegmentRole::Common, common_legs, items, false));
        }

        let concretized_items = merge_concretized_segments_from_records(
            segments
                .iter()
                .map(|(_, _, items, _)| items.clone())
                .collect::<Vec<_>>(),
        );
        let terminal_discontinuity = match concretized_items.last() {
            Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => {
                Some(discontinuity.clone())
            }
            _ => None,
        };
        let resolved_legs = resolve_procedure_materialization_legs_with_provenance(
            airport_id,
            procedure_id,
            kind.clone(),
            component_index,
            true,
            true,
            &segments,
        )?;
        append_data_quality_from_resolved_legs(&mut data_quality, &resolved_legs);

        return Ok(MaterializedProcedure {
            procedure: ProcedureSegment {
                airport_id: AirportId(airport_id.trim().to_string()),
                procedure_id: procedure_id.trim().to_string(),
                display_label: None,
                kind,
                runway_transition: None,
                enroute_transition: requested.enroute_transition,
                terminal_discontinuity,
                data_quality: data_quality.clone(),
            },
            resolved_legs,
        });
    }

    let layout = procedure_layout(kind.clone());
    let segment_specs = match kind {
        ProcedureKind::Sid => vec![
            (
                MaterializedSegmentRole::RunwayTransition,
                layout.runway_route_types,
                requested.runway_transition.as_deref(),
            ),
            (
                MaterializedSegmentRole::Common,
                layout.common_route_types,
                options
                    .has_common_segment
                    .then_some(layout.common_transition_id),
            ),
            (
                MaterializedSegmentRole::EnrouteTransition,
                layout.enroute_route_types,
                requested.enroute_transition.as_deref(),
            ),
        ],
        ProcedureKind::Star => vec![
            (
                MaterializedSegmentRole::EnrouteTransition,
                layout.enroute_route_types,
                requested.enroute_transition.as_deref(),
            ),
            (
                MaterializedSegmentRole::Common,
                layout.common_route_types,
                options
                    .has_common_segment
                    .then_some(layout.common_transition_id),
            ),
            (
                MaterializedSegmentRole::RunwayTransition,
                layout.runway_route_types,
                requested.runway_transition.as_deref(),
            ),
        ],
        ProcedureKind::Approach => unreachable!("approaches return above"),
    };
    for (role, route_types, transition_id) in segment_specs {
        let Some(transition_id) = transition_id else {
            continue;
        };
        let mut segment_legs = filter_procedure_records_for_route_types(
            &legs,
            airport_id,
            procedure_id,
            route_types,
            transition_id,
        );
        if kind == ProcedureKind::Sid && matches!(&role, MaterializedSegmentRole::RunwayTransition)
        {
            anchor_departure_segment(&mut segment_legs);
        }
        let items = concretize_procedure_materialization_legs(&segment_legs, false);
        segments.push((role, segment_legs, items, false));
    }
    if kind == ProcedureKind::Sid {
        normalize_sid_heading_intercept_anchors(&mut segments);
        normalize_sid_vector_targets(&mut segments);
    }

    let concretized_items = merge_concretized_segments_from_records(
        segments
            .iter()
            .map(|(_, _, items, _)| items.clone())
            .collect::<Vec<_>>(),
    );
    let terminal_discontinuity = match concretized_items.last() {
        Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => {
            Some(discontinuity.clone())
        }
        _ => None,
    };
    let resolved_legs = resolve_procedure_materialization_legs_with_provenance(
        airport_id,
        procedure_id,
        kind.clone(),
        component_index,
        true,
        true,
        &segments,
    )?;
    append_data_quality_from_resolved_legs(&mut data_quality, &resolved_legs);

    Ok(MaterializedProcedure {
        procedure: ProcedureSegment {
            airport_id: AirportId(airport_id.trim().to_string()),
            procedure_id: procedure_id.trim().to_string(),
            display_label: None,
            kind,
            runway_transition: requested.runway_transition,
            enroute_transition: requested.enroute_transition,
            terminal_discontinuity,
            data_quality: data_quality.clone(),
        },
        resolved_legs,
    })
}

fn normalize_sid_heading_intercept_anchors(
    segments: &mut [(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) {
    for (_, legs, items, _) in segments {
        let intercept_anchor_indexes = legs
            .windows(3)
            .enumerate()
            .filter_map(|(index, window)| {
                (window[0].path_termination.trim() == "VI"
                    && window[1].path_termination.trim() == "IF"
                    && window[2].path_termination.trim() == "TF")
                    .then_some(index + 1)
            })
            .collect::<Vec<_>>();
        if intercept_anchor_indexes.is_empty() {
            continue;
        }
        for index in intercept_anchor_indexes {
            legs[index].nav_ref = None;
        }
        *items = concretize_procedure_materialization_legs(legs, false);
    }
}

fn normalize_sid_vector_targets(
    segments: &mut Vec<(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )>,
) {
    let mut index = 0;
    while index < segments.len() {
        if !matches!(segments[index].0, MaterializedSegmentRole::RunwayTransition) {
            index += 1;
            continue;
        }
        let Some(vector_index) = segments[index].1.windows(2).position(|pair| {
            matches!(pair[0].path_termination.trim(), "FM" | "VM")
                && matches!(pair[1].path_termination.trim(), "CF" | "DF")
        }) else {
            index += 1;
            continue;
        };
        let vector_target_index = vector_index + 1;
        let target_is_last = vector_target_index + 1 == segments[index].1.len();
        let target_repeated_by_next_common = segments
            .get(index + 1)
            .filter(|(role, _, _, _)| matches!(role, MaterializedSegmentRole::Common))
            .and_then(|(_, legs, _, _)| {
                legs.iter()
                    .find(|record| record.path_termination.trim() == "IF")
            })
            .is_some_and(|common_if| {
                same_materialized_fix(&segments[index].1[vector_target_index], common_if)
            });

        if target_is_last && target_repeated_by_next_common {
            segments[index].1.pop();
            segments[index].2 =
                concretize_procedure_materialization_legs(&segments[index].1, false);
            index += 1;
            continue;
        }

        let mut continuation_legs = segments[index].1.split_off(vector_target_index);
        let vector_target = &mut continuation_legs[0];
        vector_target.path_termination = "IF".to_string();
        vector_target.path_termination_kind = PathTermination::InitialFix;
        vector_target.turn_direction = None;
        vector_target.theta_deg = None;
        vector_target.magnetic_course_deg = None;
        vector_target.defining_nav_ref = None;
        vector_target.defining_nav_position = None;
        vector_target.defining_nav_magnetic_variation_deg = None;
        vector_target.route_distance_or_time = None;

        segments[index].2 = concretize_procedure_materialization_legs(&segments[index].1, false);
        let continuation_items =
            concretize_procedure_materialization_legs(&continuation_legs, false);
        segments.insert(
            index + 1,
            (
                MaterializedSegmentRole::RunwayTransition,
                continuation_legs,
                continuation_items,
                false,
            ),
        );
        index += 2;
    }
}

struct ProcedureLayout {
    runway_route_types: &'static [&'static str],
    enroute_route_types: &'static [&'static str],
    common_route_types: &'static [&'static str],
    common_transition_id: &'static str,
}

enum MaterializedSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

fn procedure_layout(kind: ProcedureKind) -> ProcedureLayout {
    match kind {
        ProcedureKind::Sid => ProcedureLayout {
            runway_route_types: &["1", "4", "T"],
            enroute_route_types: &["3", "6"],
            common_route_types: &["2", "5"],
            common_transition_id: "",
        },
        ProcedureKind::Star => ProcedureLayout {
            runway_route_types: &["3", "6"],
            enroute_route_types: &["1", "4"],
            common_route_types: &["2", "5"],
            common_transition_id: "",
        },
        ProcedureKind::Approach => ProcedureLayout {
            runway_route_types: &[],
            enroute_route_types: &[],
            common_route_types: &[],
            common_transition_id: "",
        },
    }
}

fn approach_common_route_type(rows: &[ProcedureDistinctRow]) -> Option<String> {
    rows.iter()
        .find(|row| row.route_type != "A")
        .map(|row| row.route_type.clone())
}

fn filter_procedure_records(
    legs: &[ProcedureLegMaterializationRecord],
    airport_id: &str,
    procedure_id: &str,
    route_type: &str,
    transition_id: &str,
) -> Vec<ProcedureLegMaterializationRecord> {
    let mut filtered = legs
        .iter()
        .filter(|leg| {
            leg.key.airport_id.trim() == airport_id.trim()
                && leg.key.procedure_id.trim() == procedure_id.trim()
                && leg.key.route_type.trim() == route_type.trim()
                && leg.key.transition_id.trim() == transition_id.trim()
        })
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by_key(|leg| leg.sequence);
    filtered
}

fn filter_procedure_records_for_route_types(
    legs: &[ProcedureLegMaterializationRecord],
    airport_id: &str,
    procedure_id: &str,
    route_types: &[&str],
    transition_id: &str,
) -> Vec<ProcedureLegMaterializationRecord> {
    let mut filtered = legs
        .iter()
        .filter(|leg| {
            leg.key.airport_id.trim() == airport_id.trim()
                && leg.key.procedure_id.trim() == procedure_id.trim()
                && route_types.contains(&leg.key.route_type.trim())
                && if transition_id.trim().is_empty() {
                    matches!(leg.key.transition_id.trim(), "" | "ALL")
                } else {
                    leg.key.transition_id.trim() == transition_id.trim()
                }
        })
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by_key(|leg| leg.sequence);
    filtered
}

fn anchor_departure_segment(legs: &mut Vec<ProcedureLegMaterializationRecord>) {
    let Some(first) = legs.first().cloned() else {
        return;
    };
    if first.nav_ref.is_some()
        && !matches!(
            first.path_termination.trim(),
            "CF" | "DF" | "FA" | "FM" | "VA" | "VI" | "VM"
        )
    {
        return;
    }
    let (Some(anchor_ref), Some(anchor_position)) = (
        first.departure_anchor_ref.clone(),
        first.departure_anchor_position,
    ) else {
        return;
    };
    let preserve_departure_course = first.path_termination.trim() == "FA";
    let departure_anchor_altitude_ft = first.departure_anchor_altitude_ft;
    let mut anchor = first;
    anchor.sequence = anchor.sequence.saturating_sub(1);
    anchor.nav_ref = Some(anchor_ref);
    anchor.nav_position = Some(anchor_position);
    anchor.nav_magnetic_variation_deg = anchor.airport_magnetic_variation_deg;
    anchor.defining_nav_ref = None;
    anchor.defining_nav_position = None;
    anchor.defining_nav_magnetic_variation_deg = None;
    anchor.path_termination = "IF".to_string();
    anchor.path_termination_kind = PathTermination::InitialFix;
    anchor.turn_direction = None;
    anchor.theta_deg = None;
    if !preserve_departure_course {
        anchor.magnetic_course_deg = None;
    }
    anchor.altitude_1_ft = departure_anchor_altitude_ft;
    anchor.altitude_2_ft = None;
    anchor.route_distance_or_time = None;
    legs.insert(0, anchor);
}

#[cfg(test)]
pub(crate) fn nav_ref_position_from_store(
    store: &NavKvStore,
    airport_id: &str,
    nav_ref: &NavRef,
) -> Option<LatLon> {
    if let NavRef::Navaid(identifier) = nav_ref {
        if !bare_navaid_position_lookup_is_unique(store, identifier)? {
            return None;
        }
    }
    for procedure_airport_id in [Some(airport_id.to_string()), None] {
        let Some(key) = nav_kv_key_for_query(&NavKvQuery::NavRefPosition {
            nav_ref: nav_ref.clone(),
            procedure_airport_id,
        }) else {
            continue;
        };
        match store.get_bytes(&key).ok()? {
            NavKvLookup::Hit(bytes) => {
                if let Some(position) = serde_json::from_slice(&bytes).ok().flatten() {
                    return Some(position);
                }
            }
            NavKvLookup::MissingKey | NavKvLookup::MissingPages(_) => {}
        }
    }
    None
}

#[cfg(test)]
fn bare_navaid_position_lookup_is_unique(store: &NavKvStore, identifier: &str) -> Option<bool> {
    let identifier = identifier.trim().to_uppercase();
    let key = nav_kv_key_for_query(&NavKvQuery::WaypointIdentifier { identifier })?;
    let bytes = match store.get_bytes(&key).ok()? {
        NavKvLookup::Hit(bytes) => bytes,
        NavKvLookup::MissingKey => return Some(false),
        NavKvLookup::MissingPages(_) => return None,
    };
    serde_json::from_slice::<NavRef>(&bytes)
        .ok()
        .map(|nav_ref| matches!(nav_ref, NavRef::Navaid(_)))
}

#[cfg(test)]
pub(crate) fn enrich_procedure_materialization_records_from_store(
    store: &NavKvStore,
    airport_id: &str,
    records: Vec<ProcedureLegMaterializationRecord>,
) -> Vec<ProcedureLegMaterializationRecord> {
    records
        .into_iter()
        .map(|mut record| {
            if record.nav_position.is_none() {
                record.nav_position = record
                    .nav_ref
                    .as_ref()
                    .and_then(|nav_ref| nav_ref_position_from_store(store, airport_id, nav_ref));
            }
            if record.defining_nav_position.is_none() {
                record.defining_nav_position = record
                    .defining_nav_ref
                    .as_ref()
                    .and_then(|nav_ref| nav_ref_position_from_store(store, airport_id, nav_ref));
            }
            if record.arc_center_fix_position.is_none() {
                record.arc_center_fix_position = record
                    .arc_center_fix_ref
                    .as_ref()
                    .and_then(|nav_ref| nav_ref_position_from_store(store, airport_id, nav_ref));
            }
            record
        })
        .collect()
}

fn chained_approach_transition_segments(
    legs: &[ProcedureLegMaterializationRecord],
    airport_id: &str,
    procedure_id: &str,
    selected_transition: &str,
    common_legs: Option<&[ProcedureLegMaterializationRecord]>,
) -> Vec<Vec<ProcedureLegMaterializationRecord>> {
    let mut segments = Vec::new();
    let mut current_transition = selected_transition.trim().to_string();
    let mut seen_transitions = std::collections::HashSet::<String>::new();

    loop {
        if current_transition.is_empty() || !seen_transitions.insert(current_transition.clone()) {
            break;
        }
        let transition_legs =
            filter_procedure_records(legs, airport_id, procedure_id, "A", &current_transition);
        if transition_legs.is_empty() {
            break;
        }

        // Some approach procedures chain A-route fragments before reaching the
        // common/runway segment. KRUQ R20 / JOTTA is the motivating case:
        // A/JOTTA ends at YIDPO, then A/YIDPO continues to ZUGMY before the
        // runway route begins. Without following that chain, we create a real
        // gap from YIDPO to the runway segment.
        let next_transition_nav_ref = transition_legs
            .last()
            .and_then(|record| record.nav_ref.as_ref())
            .cloned();

        segments.push(transition_legs);

        let Some(next_transition_nav_ref) = next_transition_nav_ref else {
            break;
        };
        if common_legs.is_some_and(|records| {
            records.iter().any(|record| {
                record.path_termination.trim() == "IF"
                    && record.nav_ref.as_ref() == Some(&next_transition_nav_ref)
            })
        }) {
            break;
        }
        let next_transition = describe_nav_ref(&next_transition_nav_ref);
        if filter_procedure_records(legs, airport_id, procedure_id, "A", &next_transition)
            .is_empty()
        {
            break;
        }
        current_transition = next_transition;
    }

    segments
}

fn borrow_sibling_transition_hold_for_common_if_course_reversal(
    all_legs: &[ProcedureLegMaterializationRecord],
    transition_legs: &mut Vec<ProcedureLegMaterializationRecord>,
    common_legs: &[ProcedureLegMaterializationRecord],
    data_quality: &mut Vec<String>,
) {
    let Some(last_transition_leg) = transition_legs.last() else {
        return;
    };
    let Some(common_if) = common_legs
        .iter()
        .find(|record| record.path_termination.trim() == "IF")
    else {
        return;
    };
    if last_transition_leg.nav_ref.is_none() || last_transition_leg.nav_ref != common_if.nav_ref {
        return;
    }
    if transition_legs.iter().any(|record| {
        matches!(record.path_termination.trim(), "HA" | "HF" | "HM")
            && record.nav_ref.is_some()
            && record.nav_ref == common_if.nav_ref
    }) {
        return;
    }
    let Some(first_common_course) = common_legs
        .iter()
        .filter(|record| record.sequence > common_if.sequence)
        .find(|record| record.path_termination.trim() == "CF")
        .and_then(|record| record.magnetic_course_deg)
    else {
        return;
    };
    let Some(arrival_course) = last_transition_leg.magnetic_course_deg else {
        return;
    };
    let turn_to_common_course_deg = angular_difference_degrees(arrival_course, first_common_course);
    if !arinc_ambiguity_resolutions::borrow_sibling_transition_hold_for_common_if_course_reversal(
        last_transition_leg.key.airport_id.trim(),
        last_transition_leg.key.procedure_id.trim(),
        last_transition_leg.key.transition_id.trim(),
        common_if.sequence,
        turn_to_common_course_deg,
    ) {
        return;
    }
    let Some(mut borrowed_hold) = all_legs
        .iter()
        .find(|candidate| {
            candidate.key.airport_id == last_transition_leg.key.airport_id
                && candidate.key.procedure_id == last_transition_leg.key.procedure_id
                && candidate.key.route_type.trim() == "A"
                && candidate.key.transition_id != last_transition_leg.key.transition_id
                && matches!(candidate.path_termination.trim(), "HA" | "HF" | "HM")
                && candidate.nav_ref.is_some()
                && candidate.nav_ref == common_if.nav_ref
                && candidate.magnetic_course_deg.is_some()
                && matches!(
                    candidate.turn_direction.as_deref().map(str::trim),
                    Some("L" | "R")
                )
                && candidate
                    .route_distance_or_time
                    .as_deref()
                    .is_some_and(|value| {
                        let value = value.trim();
                        !value.is_empty()
                    })
        })
        .cloned()
    else {
        return;
    };
    borrowed_hold.key.transition_id = last_transition_leg.key.transition_id.clone();
    borrowed_hold.sequence = last_transition_leg.sequence + 1;
    transition_legs.push(borrowed_hold);
    push_unique_data_quality(
        data_quality,
        arinc_ambiguity_resolutions::BORROWED_SIBLING_TRANSITION_HOLD_WARNING,
    );
}

fn append_data_quality_from_resolved_legs(
    data_quality: &mut Vec<String>,
    resolved_legs: &[ResolvedLeg],
) {
    for source in resolved_legs
        .iter()
        .filter_map(|leg| leg.procedure_provenance.as_ref())
        .filter_map(|provenance| provenance.display_path.as_ref())
        .flat_map(|path| path.debug_element_sources.iter())
    {
        if source.contains(BORROWED_LATER_HOLD_FOR_PI_SOURCE_PREFIX) {
            push_unique_data_quality(
                data_quality,
                arinc_ambiguity_resolutions::BORROWED_LATER_HOLD_FOR_PI_WARNING,
            );
        }
        if source.contains(INVENTED_PI_ENTRY_REVERSAL_SOURCE_PREFIX) {
            push_unique_data_quality(
                data_quality,
                arinc_ambiguity_resolutions::INVENTED_PI_ENTRY_REVERSAL_WARNING,
            );
        }
        if source.contains(PLATE_EXCEPTION_MISSED_TURN_SOURCE_PREFIX) {
            push_unique_data_quality(
                data_quality,
                arinc_ambiguity_resolutions::UNSPECIFIED_MISSED_TURN_PLATE_EXCEPTION_WARNING,
            );
        } else if source.contains(INFERRED_MISSED_TURN_SOURCE_PREFIX) {
            push_unique_data_quality(
                data_quality,
                arinc_ambiguity_resolutions::UNSPECIFIED_MISSED_TURN_FROM_GEOMETRY_WARNING,
            );
        }
    }
}

fn push_unique_data_quality(data_quality: &mut Vec<String>, message: &str) {
    if !data_quality.iter().any(|existing| existing == message) {
        data_quality.push(message.to_string());
    }
}

fn resolve_procedure_materialization_legs_with_provenance(
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    component_index: usize,
    validate_heading_continuity: bool,
    validate_display_geometry: bool,
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) -> AppResult<Vec<ResolvedLeg>> {
    let mut resolved = Vec::<ResolvedLeg>::new();
    let mut previous_display_path: Option<LegDisplayPath> = None;
    let mut previous_leg_to: Option<NavRef> = None;
    let mut heading_checks = Vec::<DisplayElementHeadingSignature>::new();
    let mut next_heading_step_index = 0usize;
    let required_procedure_turn_sequences =
        required_procedure_turn_sequences_for_segments(segments);
    let mut row_ledger = ProcedureRowDispositionLedger::new(segments);

    for (segment_index, (role, leg_records, _, reversed)) in segments.iter().enumerate() {
        if segment_index > 0
            && segments[segment_index - 1]
                .1
                .last()
                .and_then(terminal_procedure_discontinuity)
                .is_some()
        {
            validate_heading_continuity_checks(
                &heading_checks,
                validate_heading_continuity,
                procedure_id,
                &kind,
            )?;
            heading_checks.clear();
            next_heading_step_index = 0;
            previous_display_path = None;
            previous_leg_to = None;
        }
        let resolved_segment_start = resolved.len();
        let next_segment_records = segments
            .get(segment_index + 1)
            .map(|(_, records, _, _)| records.as_slice());
        let mut fix_records = leg_records
            .iter()
            .filter(|leg| leg.nav_ref.is_some())
            .collect::<Vec<_>>();
        if *reversed {
            fix_records.reverse();
        }
        let role = procedure_segment_role(role);
        let traversal_policy = segment_traversal_policy(
            previous_display_path.as_ref(),
            previous_leg_to.as_ref(),
            resolved.last(),
            leg_records,
            &fix_records,
        );

        for (index, pair) in fix_records.windows(2).enumerate() {
            if traversal_policy.should_skip_window(index) {
                row_ledger.mark_ignored(pair[1], "skipped by segment traversal policy");
                continue;
            }
            let previous_path_state = previous_display_path_state(previous_display_path.as_ref());
            let planned_window = plan_procedure_window(
                index,
                [pair[0], pair[1]],
                ProcedureWindowPlanningContext {
                    fix_records: &fix_records,
                    leg_records,
                    role: role.clone(),
                    common_resume_target: traversal_policy.common_resume_target,
                    previous_display_path: previous_display_path.as_ref(),
                    previous_leg_to: previous_leg_to.as_ref(),
                    next_segment_records,
                    resolved_last: resolved.last().cloned(),
                },
            )?;
            let Some(window_link) = planned_window else {
                let previous_context = previous_window_context(
                    previous_display_path.as_ref(),
                    resolved.last(),
                    pair[0],
                );
                if common_resume_yields_current_feeder_cf(
                    [pair[0], pair[1]],
                    leg_records,
                    previous_display_path.as_ref(),
                    previous_context,
                    next_segment_records,
                    role.clone(),
                ) {
                    row_ledger.mark_ignored(pair[1], "common segment supersedes current feeder CF");
                } else if same_fix_df_between_fa_and_hold(fix_records.as_slice(), index) {
                    row_ledger.mark_ignored(
                        pair[1],
                        "same-fix DF superseded by preceding FA and following hold",
                    );
                } else if same_fix_hold_after_redundant_df(fix_records.as_slice(), index) {
                    row_ledger.mark_ignored(pair[1], "same-fix hold follows redundant DF after FA");
                } else if same_fix_cf_after_pi_course_reversal(fix_records.as_slice(), index) {
                    row_ledger.mark_ignored(
                        pair[1],
                        "same-fix CF satisfied by preceding PI course reversal",
                    );
                } else if row_returns_to_following_segment_if(pair[1], next_segment_records) {
                    row_ledger.mark_ignored(pair[1], "following segment resumes at same IF anchor");
                }
                continue;
            };
            row_ledger.mark_window_consumed(
                leg_records,
                window_link.display_leg_start,
                window_link.effective_leg_end,
                window_link.hold_record,
                window_link.provenance_record,
            );
            if let Some(suppressed_record) = window_link.suppressed_record {
                row_ledger.mark_ignored(
                    suppressed_record,
                    "common segment supersedes current feeder CF",
                );
            }
            if window_link.render_as_empty_join {
                previous_leg_to = Some(window_link.to);
                continue;
            }
            let initial_position_override = if window_link.inherit_previous_state {
                previous_path_state.terminal_position
            } else {
                None
            };
            let initial_course_override = if window_link.inherit_previous_state {
                previous_path_state.terminal_course
            } else {
                None
            };
            let display_path = if window_link.render_as_empty_join {
                Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: Vec::new(),
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                })
            } else if window_link.render_as_resumed_common_cf {
                display_path_for_resumed_common_cf(
                    pair[1],
                    initial_position_override,
                    initial_course_override,
                )
            } else if window_link.display_leg_start.sequence
                == window_link.effective_leg_end.sequence
                && (matches!(
                    window_link.display_leg_start.path_termination.trim(),
                    "PI" | "RF"
                ) || (window_link.render_inherited_single_tf_step
                    && matches!(
                        window_link.display_leg_start.path_termination.trim(),
                        "CF" | "TF"
                    )))
            {
                display_path_for_single_procedure_step(
                    leg_records,
                    window_link.display_leg_start,
                    initial_position_override,
                    initial_course_override,
                )
            } else {
                next_segment_records
                    .and_then(|records| {
                        let allow_hold_exit_to_following_course =
                            window_link.hold_record.is_some_and(|hold| {
                                matches!(hold.path_termination.trim(), "HA" | "HF" | "HM")
                            });
                        display_path_for_procedure_leg_before_following_segment(
                            leg_records,
                            window_link.display_leg_start,
                            window_link.effective_leg_end,
                            window_link.hold_record,
                            initial_position_override,
                            initial_course_override,
                            FollowingProcedureSegment {
                                records,
                                allow_hold_exit_to_following_course,
                            },
                        )
                    })
                    .or_else(|| {
                        display_path_for_procedure_leg(
                            leg_records,
                            window_link.display_leg_start,
                            window_link.effective_leg_end,
                            window_link.hold_record,
                            initial_position_override,
                            initial_course_override,
                        )
                    })
            };
            let previous_to = window_link.to.clone();
            previous_display_path =
                append_resolved_procedure_leg(AppendResolvedProcedureLegInput {
                    resolved: &mut resolved,
                    heading_checks: &mut heading_checks,
                    next_heading_step_index: &mut next_heading_step_index,
                    procedure_id,
                    airport_id,
                    kind: &kind,
                    role: &role,
                    component_index,
                    spec: append_spec_for_window_link(pair[0], window_link, display_path),
                });
            previous_leg_to = Some(previous_to);
        }

        if let Some(last_fix) = fix_records.last().copied() {
            if let Some(trailing_record) = leg_records
                .iter()
                .filter(|record| record.sequence > last_fix.sequence)
                .max_by_key(|record| record.sequence)
            {
                let trailing_plan = plan_trailing_procedure_window(
                    last_fix,
                    trailing_record,
                    TailPlanningContext {
                        leg_records,
                        previous_display_path: previous_display_path.as_ref(),
                        previous_leg_to: previous_leg_to.as_ref(),
                        next_segment_records,
                    },
                )?;
                if let Some(tail_link) = trailing_plan {
                    row_ledger.mark_tail_consumed(last_fix, tail_link.provenance_record);
                    let previous_to = tail_link.nav_ref.clone();
                    previous_display_path =
                        append_resolved_procedure_leg(AppendResolvedProcedureLegInput {
                            resolved: &mut resolved,
                            heading_checks: &mut heading_checks,
                            next_heading_step_index: &mut next_heading_step_index,
                            procedure_id,
                            airport_id,
                            kind: &kind,
                            role: &role,
                            component_index,
                            spec: append_spec_for_tail_link(last_fix, tail_link),
                        });
                    previous_leg_to = Some(previous_to);
                }
            }
        }

        if fix_records.len() == 1 {
            let standalone = fix_records[0];
            if standalone.path_termination.trim() == "PI" {
                let standalone_plan = plan_standalone_pi_window(
                    standalone,
                    TailPlanningContext {
                        leg_records,
                        previous_display_path: previous_display_path.as_ref(),
                        previous_leg_to: previous_leg_to.as_ref(),
                        next_segment_records,
                    },
                )?;
                let Some(tail_link) = standalone_plan else {
                    continue;
                };
                row_ledger.mark_emitted(standalone);
                let previous_to = tail_link.nav_ref.clone();
                previous_display_path =
                    append_resolved_procedure_leg(AppendResolvedProcedureLegInput {
                        resolved: &mut resolved,
                        heading_checks: &mut heading_checks,
                        next_heading_step_index: &mut next_heading_step_index,
                        procedure_id,
                        airport_id,
                        kind: &kind,
                        role: &role,
                        component_index,
                        spec: append_spec_for_tail_link(standalone, tail_link),
                    });
                previous_leg_to = Some(previous_to);
            }
        }

        if let Some(discontinuity) = leg_records
            .last()
            .and_then(terminal_procedure_discontinuity)
        {
            if let Some(last_leg) = resolved[resolved_segment_start..].last_mut() {
                if let Some(provenance) = last_leg.procedure_provenance.as_mut() {
                    provenance.discontinuity_after = Some(discontinuity);
                }
            }
        }
    }

    mark_unspecified_sid_segment_boundary_course_reversals(&mut resolved, &kind, segments);

    if validate_display_geometry {
        validate_no_zero_length_legs(&resolved, procedure_id, &kind)?;
        validate_materialized_geometry_rows_have_display_paths(&resolved, procedure_id)?;
        validate_no_absurdly_long_display_elements(&resolved, procedure_id, &kind)?;
        validate_display_path_geometry_stitches(&resolved, procedure_id)?;
        app_core::planning::validate_final_procedure_geometry(&resolved)?;
    }
    if validate_heading_continuity {
        row_ledger.validate_all_rows_explained(procedure_id)?;
        validate_required_procedure_turns_materialized(
            &required_procedure_turn_sequences,
            &resolved,
            procedure_id,
        )?;
        if kind == ProcedureKind::Approach {
            validate_explicit_missed_direct_turns_materialized(segments, &resolved, procedure_id)?;
        }
    }
    validate_heading_continuity_checks(
        &heading_checks,
        validate_heading_continuity,
        procedure_id,
        &kind,
    )?;

    Ok(resolved)
}

fn validate_materialized_geometry_rows_have_display_paths(
    resolved: &[ResolvedLeg],
    procedure_id: &str,
) -> AppResult<()> {
    for leg in resolved {
        let Some(provenance) = leg.procedure_provenance.as_ref() else {
            continue;
        };
        if !procedure_path_termination_requires_geometry(&provenance.path_termination) {
            continue;
        }
        if leg.from == leg.to
            && !matches!(
                provenance.path_termination,
                PathTermination::Other(ref label)
                    if matches!(label.trim(), "PI" | "HA" | "HF" | "HM")
            )
        {
            continue;
        }
        if provenance
            .display_path
            .as_ref()
            .is_some_and(|path| !path.elements.is_empty())
        {
            continue;
        }
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "procedure geometry row has no drawable display path for {} route {:?} seq {} {:?}",
                procedure_id.trim(),
                provenance.role,
                provenance.leg_sequence,
                provenance.path_termination,
            ),
        });
    }
    Ok(())
}

fn procedure_path_termination_requires_geometry(path_termination: &PathTermination) -> bool {
    match path_termination {
        PathTermination::InitialFix => false,
        PathTermination::Other(label) => matches!(
            label.trim(),
            "AF" | "CF" | "DF" | "FA" | "FC" | "HA" | "HF" | "HM" | "PI" | "RF" | "TF"
        ),
        _ => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProcedureRowDispositionKey {
    route_type: String,
    transition_id: String,
    sequence: i32,
}

impl ProcedureRowDispositionKey {
    fn from_record(record: &ProcedureLegMaterializationRecord) -> Self {
        Self {
            route_type: record.key.route_type.trim().to_string(),
            transition_id: record.key.transition_id.trim().to_string(),
            sequence: record.sequence,
        }
    }
}

struct ProcedureRowDispositionLedger {
    rows: std::collections::BTreeMap<ProcedureRowDispositionKey, ProcedureLegMaterializationRecord>,
    dispositions: std::collections::BTreeMap<ProcedureRowDispositionKey, Vec<String>>,
}

impl ProcedureRowDispositionLedger {
    fn new(
        segments: &[(
            MaterializedSegmentRole,
            Vec<ProcedureLegMaterializationRecord>,
            Vec<ConcretizedNavItem>,
            bool,
        )],
    ) -> Self {
        let mut ledger = Self {
            rows: std::collections::BTreeMap::new(),
            dispositions: std::collections::BTreeMap::new(),
        };
        for (_, records, _, _) in segments {
            let fix_record_count = records
                .iter()
                .filter(|record| record.nav_ref.is_some())
                .count();
            for record in records {
                let key = ProcedureRowDispositionKey::from_record(record);
                ledger
                    .rows
                    .entry(key.clone())
                    .or_insert_with(|| record.clone());
                if record.path_termination.trim() == "IF" {
                    ledger.mark(record, "deliberately ignored: IF anchor");
                } else if record.nav_ref.is_none() {
                    ledger.mark(record, "deliberately ignored: non-fix procedural row");
                } else if fix_record_count == 1
                    && matches!(record.path_termination.trim(), "HA" | "HF" | "HM")
                {
                    ledger.mark(
                        record,
                        "deliberately ignored: standalone hold row without an inbound segment",
                    );
                } else if same_fix_hold_without_inbound_segment(records, record) {
                    ledger.mark(
                        record,
                        "deliberately ignored: same-fix hold row without an inbound segment",
                    );
                }
            }
        }
        ledger
    }

    fn mark(&mut self, record: &ProcedureLegMaterializationRecord, reason: impl Into<String>) {
        let key = ProcedureRowDispositionKey::from_record(record);
        self.dispositions
            .entry(key)
            .or_default()
            .push(reason.into());
    }

    fn mark_emitted(&mut self, record: &ProcedureLegMaterializationRecord) {
        self.mark(record, "emitted geometry/provenance");
    }

    fn mark_ignored(&mut self, record: &ProcedureLegMaterializationRecord, reason: &str) {
        self.mark(record, format!("deliberately ignored: {reason}"));
    }

    fn mark_tail_consumed(
        &mut self,
        anchor_record: &ProcedureLegMaterializationRecord,
        provenance_record: &ProcedureLegMaterializationRecord,
    ) {
        if anchor_record.sequence != provenance_record.sequence {
            self.mark(anchor_record, "consumed as tail anchor");
        }
        self.mark_emitted(provenance_record);
    }

    fn mark_window_consumed(
        &mut self,
        leg_records: &[ProcedureLegMaterializationRecord],
        display_leg_start: &ProcedureLegMaterializationRecord,
        effective_leg_end: &ProcedureLegMaterializationRecord,
        hold_record: Option<&ProcedureLegMaterializationRecord>,
        provenance_record: &ProcedureLegMaterializationRecord,
    ) {
        let start = display_leg_start.sequence.min(effective_leg_end.sequence);
        let end = display_leg_start.sequence.max(effective_leg_end.sequence);
        for record in leg_records.iter().filter(|record| {
            record.sequence >= start
                && record.sequence <= end
                && record.key.route_type == display_leg_start.key.route_type
                && record.key.transition_id == display_leg_start.key.transition_id
        }) {
            if record.sequence == provenance_record.sequence
                && record.key.route_type == provenance_record.key.route_type
                && record.key.transition_id == provenance_record.key.transition_id
            {
                self.mark_emitted(record);
            } else {
                self.mark(record, "consumed by rendered window");
            }
        }
        if let Some(hold_record) = hold_record {
            if hold_record.sequence < start || hold_record.sequence > end {
                self.mark(hold_record, "used as auxiliary hold record");
            }
        }
    }

    fn validate_all_rows_explained(&self, procedure_id: &str) -> AppResult<()> {
        for (key, record) in &self.rows {
            if self
                .dispositions
                .get(key)
                .is_some_and(|dispositions| !dispositions.is_empty())
            {
                continue;
            }
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "procedure row has no resolver disposition for {} route {} transition {} seq {} {}",
                    procedure_id.trim(),
                    record.key.route_type.trim(),
                    record.key.transition_id.trim(),
                    record.sequence,
                    record.path_termination.trim(),
                ),
            });
        }
        Ok(())
    }
}

fn same_fix_hold_without_inbound_segment(
    records: &[ProcedureLegMaterializationRecord],
    hold_record: &ProcedureLegMaterializationRecord,
) -> bool {
    if !matches!(hold_record.path_termination.trim(), "HA" | "HF" | "HM") {
        return false;
    }
    if !records.iter().any(|record| {
        record.sequence < hold_record.sequence
            && record.path_termination.trim() == "IF"
            && same_materialized_fix(record, hold_record)
    }) {
        return false;
    }
    records
        .iter()
        .filter(|record| record.nav_ref.is_some())
        .all(|record| same_materialized_fix(record, hold_record))
}

fn same_materialized_fix(
    first: &ProcedureLegMaterializationRecord,
    second: &ProcedureLegMaterializationRecord,
) -> bool {
    if first.nav_ref.is_some() && first.nav_ref == second.nav_ref {
        return true;
    }
    match (first.nav_position, second.nav_position) {
        (Some(first), Some(second)) => great_circle_distance_nm(first, second) <= 0.05,
        _ => false,
    }
}

fn display_element_start_position_for_validation(element: &LegDisplayElement) -> LatLon {
    match element {
        LegDisplayElement::Segment { start, .. } | LegDisplayElement::Arc { start, .. } => *start,
    }
}

fn display_element_end_position_for_validation(element: &LegDisplayElement) -> LatLon {
    match element {
        LegDisplayElement::Segment { end, .. } | LegDisplayElement::Arc { end, .. } => *end,
    }
}

fn mark_unspecified_sid_segment_boundary_course_reversals(
    resolved: &mut [ResolvedLeg],
    kind: &ProcedureKind,
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) {
    if *kind != ProcedureKind::Sid {
        return;
    }
    for index in 0..resolved.len().saturating_sub(1) {
        let (through_incoming, from_outgoing) = resolved.split_at_mut(index + 1);
        let incoming = &mut through_incoming[index];
        let outgoing = &from_outgoing[0];
        let is_unspecified_reversal = incoming
            .procedure_provenance
            .as_ref()
            .zip(outgoing.procedure_provenance.as_ref())
            .is_some_and(|(incoming_provenance, outgoing_provenance)| {
                if incoming_provenance.role != ProcedureSegmentRole::RunwayTransition
                    || incoming_provenance.discontinuity_after.is_some()
                {
                    return false;
                }
                let Some(incoming_path) = incoming_provenance.display_path.as_ref() else {
                    return false;
                };
                let Some(outgoing_path) = outgoing_provenance.display_path.as_ref() else {
                    return false;
                };
                let Some(incoming_position) =
                    previous_display_path_terminal_position(incoming_path)
                else {
                    return false;
                };
                let Some(incoming_course_deg) = final_course_of_display_path(incoming_path) else {
                    return false;
                };
                let Some((outgoing_position, outgoing_course_deg, _, _)) = outgoing_path
                    .elements
                    .first()
                    .and_then(heading_signature_for_element)
                else {
                    return false;
                };
                let turn_deg = angular_difference_degrees(incoming_course_deg, outgoing_course_deg);
                let reciprocal_runway_to_common =
                    outgoing_provenance.role == ProcedureSegmentRole::Common && turn_deg >= 165.0;
                let source_requires_unspecified_course_reversal = turn_deg >= 150.0
                    && arinc_ambiguity_resolutions::sid_segment_boundary_requires_unspecified_course_reversal(
                        &incoming_provenance.airport_id,
                        &incoming_provenance.procedure_id,
                        &describe_nav_ref(&incoming.to),
                    );
                let explicit_runway_turn_into_reversal = turn_deg >= 150.0
                    && source_turn_direction_for_provenance(incoming_provenance, segments)
                        .is_some_and(|turn| matches!(turn, "L" | "R"));
                display_positions_stitch(incoming_position, outgoing_position)
                    && (reciprocal_runway_to_common
                        || source_requires_unspecified_course_reversal
                        || explicit_runway_turn_into_reversal)
            });
        if is_unspecified_reversal {
            if let Some(provenance) = incoming.procedure_provenance.as_mut() {
                provenance.discontinuity_after =
                    Some(ProcedureDiscontinuity::Other("COURSE REVERSAL".to_string()));
            }
        }
    }
}

fn source_turn_direction_for_provenance<'a>(
    provenance: &ProcedureLegProvenance,
    segments: &'a [(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) -> Option<&'a str> {
    segments
        .iter()
        .filter(|(role, _, _, _)| procedure_segment_role(role) == provenance.role)
        .flat_map(|(_, records, _, _)| records)
        .find(|record| record.sequence == provenance.leg_sequence)
        .and_then(|record| record.turn_direction.as_deref())
        .map(str::trim)
}

fn required_procedure_turn_sequences_for_segments(
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) -> std::collections::BTreeSet<i32> {
    let mut required = std::collections::BTreeSet::<i32>::new();

    for (segment_index, (role, leg_records, _, _)) in segments.iter().enumerate() {
        let chained_leading_pi_is_redundant =
            matches!(role, MaterializedSegmentRole::EnrouteTransition)
                && segment_index > 0
                && matches!(
                    segments
                        .get(segment_index - 1)
                        .map(|(prev_role, _, _, _)| prev_role),
                    Some(MaterializedSegmentRole::EnrouteTransition)
                )
                && leg_records
                    .first()
                    .filter(|record| record.path_termination.trim() == "PI")
                    .zip(
                        segments
                            .get(segment_index - 1)
                            .and_then(|(_, previous_records, _, _)| previous_records.last()),
                    )
                    .is_some_and(|(first_record, previous_record)| {
                        first_record.nav_ref.is_some()
                            && first_record.nav_ref == previous_record.nav_ref
                    });

        for record in leg_records {
            if record.path_termination.trim() != "PI" {
                continue;
            }
            if chained_leading_pi_is_redundant
                && leg_records
                    .first()
                    .is_some_and(|first_record| first_record.sequence == record.sequence)
            {
                continue;
            }
            required.insert(record.sequence);
        }
    }

    required
}

fn validate_no_zero_length_legs(
    resolved: &[ResolvedLeg],
    procedure_id: &str,
    kind: &ProcedureKind,
) -> AppResult<()> {
    for leg in resolved {
        let path = leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref());

        if leg.from == leg.to && path.is_none() {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "procedure zero-length leg without display path for {}: {} -> {} id={} seq={:?} pt={:?}",
                    procedure_id.trim(),
                    describe_nav_ref(&leg.from),
                    describe_nav_ref(&leg.to),
                    leg.id,
                    leg.procedure_provenance.as_ref().map(|p| p.leg_sequence),
                    leg.procedure_provenance.as_ref().map(|p| &p.path_termination),
                ),
            });
        }

        let Some(path) = path else {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "procedure leg without display path for {}: {} -> {} id={} seq={:?} pt={:?}",
                    procedure_id.trim(),
                    describe_nav_ref(&leg.from),
                    describe_nav_ref(&leg.to),
                    leg.id,
                    leg.procedure_provenance.as_ref().map(|p| p.leg_sequence),
                    leg.procedure_provenance
                        .as_ref()
                        .map(|p| &p.path_termination),
                ),
            });
        };

        if path.elements.is_empty() {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "procedure leg with empty display path for {}: {} -> {} id={} seq={:?} pt={:?}",
                    procedure_id.trim(),
                    describe_nav_ref(&leg.from),
                    describe_nav_ref(&leg.to),
                    leg.id,
                    leg.procedure_provenance.as_ref().map(|p| p.leg_sequence),
                    leg.procedure_provenance
                        .as_ref()
                        .map(|p| &p.path_termination),
                ),
            });
        }

        let mut has_nonzero_geometry = false;
        for (index, element) in path.elements.iter().enumerate() {
            let is_missed_turn = path.debug_element_sources.get(index).is_some_and(|source| {
                source.starts_with(EXPLICIT_MISSED_TURN_SOURCE_PREFIX)
                    || source.starts_with(INFERRED_MISSED_TURN_SOURCE_PREFIX)
                    || source.starts_with(PLATE_EXCEPTION_MISSED_TURN_SOURCE_PREFIX)
            });
            let is_explicit_missed_turn = path
                .debug_element_sources
                .get(index)
                .is_some_and(|source| source.starts_with(EXPLICIT_MISSED_TURN_SOURCE_PREFIX));
            let is_explicit_published_turn = path
                .debug_element_sources
                .get(index)
                .is_some_and(|source| source.starts_with(EXPLICIT_PUBLISHED_TURN_SOURCE_PREFIX));
            match element {
                LegDisplayElement::Segment { start, end } => {
                    if positions_nearly_equal(*start, *end) {
                        return Err(AppError {
                            kind: AppErrorKind::InvalidFlightPlan,
                            message: format!(
                                "procedure zero-length segment for {} leg={} element#{} at ({:.6},{:.6})",
                                procedure_id.trim(),
                                leg.id,
                                index,
                                start.lat,
                                start.lon,
                            ),
                        });
                    }
                    has_nonzero_geometry = true;
                }
                LegDisplayElement::Arc {
                    center,
                    radius_nm,
                    start,
                    end,
                    clockwise,
                    sweep_degrees,
                } => {
                    if sweep_degrees.abs() > 270.0
                        && !(*kind == ProcedureKind::Sid
                            && (is_explicit_published_turn || is_explicit_missed_turn))
                    {
                        return Err(AppError {
                            kind: AppErrorKind::InvalidFlightPlan,
                            message: format!(
                                "procedure excessive arc sweep for {} leg={} element#{} center=({:.6},{:.6}) radius_nm={:.2} start=({:.6},{:.6}) end=({:.6},{:.6}) clockwise={} sweep_deg={:.1} source={}",
                                procedure_id.trim(),
                                leg.id,
                                index,
                                center.lat,
                                center.lon,
                                radius_nm,
                                start.lat,
                                start.lon,
                                end.lat,
                                end.lon,
                                clockwise,
                                sweep_degrees,
                                path.debug_element_sources
                                    .get(index)
                                    .map(String::as_str)
                                    .unwrap_or("unknown"),
                            ),
                        });
                    }
                    if !is_missed_turn
                        && (*radius_nm <= MIN_GEOMETRY_DISTANCE_NM
                            || sweep_degrees.abs() <= MIN_ARC_SWEEP_DEG)
                    {
                        return Err(AppError {
                            kind: AppErrorKind::InvalidFlightPlan,
                            message: format!(
                                "procedure degenerate arc for {} leg={} element#{} center=({:.6},{:.6}) radius_nm={:.2} sweep_deg={:.2}",
                                procedure_id.trim(),
                                leg.id,
                                index,
                                center.lat,
                                center.lon,
                                radius_nm,
                                sweep_degrees,
                            ),
                        });
                    }
                    if !is_missed_turn && positions_nearly_equal(*start, *end) {
                        return Err(AppError {
                            kind: AppErrorKind::InvalidFlightPlan,
                            message: format!(
                                "procedure zero-length arc endpoints for {} leg={} element#{} at ({:.6},{:.6})",
                                procedure_id.trim(),
                                leg.id,
                                index,
                                start.lat,
                                start.lon,
                            ),
                        });
                    }
                    has_nonzero_geometry = true;
                }
            }
        }

        if leg.from == leg.to && !has_nonzero_geometry {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "procedure zero-length self leg without geometry for {}: {}",
                    procedure_id.trim(),
                    leg.id,
                ),
            });
        }
    }
    Ok(())
}

fn validate_no_absurdly_long_display_elements(
    resolved: &[ResolvedLeg],
    procedure_id: &str,
    kind: &ProcedureKind,
) -> AppResult<()> {
    for leg in resolved {
        let Some(path) = leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
        else {
            continue;
        };
        for (index, element) in path.elements.iter().enumerate() {
            let distance_nm = match element {
                LegDisplayElement::Segment { start, end } => great_circle_distance_nm(*start, *end),
                LegDisplayElement::Arc {
                    radius_nm,
                    sweep_degrees,
                    ..
                } => radius_nm * sweep_degrees.abs().to_radians(),
            };
            let allowed_distance_nm =
                allowed_procedure_display_element_distance_nm(kind, leg, element);
            if distance_nm > allowed_distance_nm {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: format!(
                        "procedure display path element exceeds {:.0} NM for {} leg={} element#{} distance_nm={:.1} pt={:?}",
                        allowed_distance_nm,
                        procedure_id.trim(),
                        leg.id,
                        index,
                        distance_nm,
                        leg.procedure_provenance.as_ref().map(|p| &p.path_termination),
                    ),
                });
            }
        }
    }
    Ok(())
}

fn allowed_procedure_display_element_distance_nm(
    kind: &ProcedureKind,
    leg: &ResolvedLeg,
    element: &LegDisplayElement,
) -> f64 {
    if *kind == ProcedureKind::Sid
        && leg
            .procedure_provenance
            .as_ref()
            .is_some_and(|provenance| provenance.role == ProcedureSegmentRole::EnrouteTransition)
    {
        return MAX_SID_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM;
    }
    if *kind == ProcedureKind::Star
        && leg
            .procedure_provenance
            .as_ref()
            .is_some_and(|provenance| provenance.role == ProcedureSegmentRole::EnrouteTransition)
    {
        return MAX_STAR_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM;
    }
    if *kind != ProcedureKind::Approach {
        return MAX_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM;
    }
    let Some(provenance) = leg.procedure_provenance.as_ref() else {
        return MAX_APPROACH_DISPLAY_ELEMENT_DISTANCE_NM;
    };
    let is_charted_feeder_transition = provenance.role == ProcedureSegmentRole::EnrouteTransition;
    if is_charted_feeder_transition {
        return MAX_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM;
    }
    let is_published_hold_or_missed_segment =
        matches!(
            provenance.path_termination,
            PathTermination::HeadingToManual
        ) && matches!(element, LegDisplayElement::Segment { .. });
    if is_published_hold_or_missed_segment {
        return MAX_PUBLISHED_HOLD_OR_MISSED_SEGMENT_DISTANCE_NM;
    }
    MAX_APPROACH_DISPLAY_ELEMENT_DISTANCE_NM
}

fn validate_display_path_geometry_stitches(
    resolved: &[ResolvedLeg],
    procedure_id: &str,
) -> AppResult<()> {
    let mut previous_leg_end: Option<(&str, LatLon, bool)> = None;

    for leg in resolved {
        let Some(path) = leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
        else {
            continue;
        };

        for (index, window) in path.elements.windows(2).enumerate() {
            let previous_end = display_element_end_position_for_validation(&window[0]);
            let current_start = display_element_start_position_for_validation(&window[1]);
            if !display_positions_stitch(previous_end, current_start) {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: format!(
                        "procedure display path internal gap for {} leg={} elements={}->{} gap_nm={:.2} end=({:.6},{:.6}) start=({:.6},{:.6}) sources={:?}->{:?}",
                        procedure_id.trim(),
                        leg.id,
                        index,
                        index + 1,
                        great_circle_distance_nm(previous_end, current_start),
                        previous_end.lat,
                        previous_end.lon,
                        current_start.lat,
                        current_start.lon,
                        path.debug_element_sources.get(index),
                        path.debug_element_sources.get(index + 1),
                    ),
                });
            }
        }

        if let Some(first_element) = path.elements.first() {
            let leg_start = display_element_start_position_for_validation(first_element);
            if let Some((previous_leg_id, previous_end, previous_suspends)) = previous_leg_end {
                if !previous_suspends && !display_positions_stitch(previous_end, leg_start) {
                    return Err(AppError {
                        kind: AppErrorKind::InvalidFlightPlan,
                        message: format!(
                            "procedure display path gap for {} between legs {} -> {} gap_nm={:.2} end=({:.6},{:.6}) start=({:.6},{:.6})",
                            procedure_id.trim(),
                            previous_leg_id,
                            leg.id,
                            great_circle_distance_nm(previous_end, leg_start),
                            previous_end.lat,
                            previous_end.lon,
                            leg_start.lat,
                            leg_start.lon,
                        ),
                    });
                }
            }
        }

        if let Some(last_element) = path.elements.last() {
            previous_leg_end = Some((
                leg.id.as_str(),
                display_element_end_position_for_validation(last_element),
                leg.procedure_provenance
                    .as_ref()
                    .is_some_and(|provenance| provenance.discontinuity_after.is_some()),
            ));
        }
    }
    Ok(())
}

fn display_positions_stitch(left: LatLon, right: LatLon) -> bool {
    positions_nearly_equal(left, right) || great_circle_distance_nm(left, right) <= 0.05
}

fn validate_required_procedure_turns_materialized(
    required_sequences: &std::collections::BTreeSet<i32>,
    resolved: &[ResolvedLeg],
    procedure_id: &str,
) -> AppResult<()> {
    if required_sequences.is_empty() {
        return Ok(());
    }

    let emitted_sequences = resolved
        .iter()
        .filter_map(|leg| {
            leg.procedure_provenance.as_ref().and_then(|provenance| {
                matches!(
                    provenance.path_termination,
                    PathTermination::Other(ref label) if label.trim() == "PI"
                )
                .then_some(provenance.leg_sequence)
            })
        })
        .collect::<std::collections::BTreeSet<_>>();

    let missing_sequences = required_sequences
        .difference(&emitted_sequences)
        .copied()
        .collect::<Vec<_>>();
    if missing_sequences.is_empty() {
        return Ok(());
    }

    Err(AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "procedure turn required but not materialized for {} at sequences {:?}",
            procedure_id.trim(),
            missing_sequences,
        ),
    })
}

fn validate_explicit_missed_direct_turns_materialized(
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
    resolved: &[ResolvedLeg],
    procedure_id: &str,
) -> AppResult<()> {
    for (role, records, _, _) in segments {
        if matches!(role, MaterializedSegmentRole::EnrouteTransition) {
            continue;
        }
        for record in records.iter().filter(|record| {
            record.path_termination.trim() == "DF"
                && matches!(
                    record.turn_direction.as_deref().map(str::trim),
                    Some("L" | "R")
                )
                && record.nav_position.is_some()
        }) {
            let turn_clockwise = record.turn_direction.as_deref().map(str::trim) == Some("R");
            let fix = record.nav_position.expect("checked above");
            if explicit_missed_direct_to_same_fix_hold(records, record, fix) {
                continue;
            }
            let route_prefix = format!(
                "procedure-{}-{}-",
                procedure_id.trim(),
                record.key.route_type.trim()
            );
            let paths = resolved
                .iter()
                .filter(|leg| leg.id.starts_with(&route_prefix))
                .filter_map(|leg| {
                    leg.procedure_provenance
                        .as_ref()
                        .filter(|provenance| provenance.leg_sequence >= record.sequence)
                        .and_then(|provenance| provenance.display_path.as_ref())
                })
                .collect::<Vec<_>>();
            if paths.is_empty() {
                continue;
            }
            if paths
                .iter()
                .any(|path| explicit_turn_path_has_arc_before_fix(path, fix, turn_clockwise))
            {
                continue;
            }
            if paths
                .iter()
                .any(|path| explicit_turn_path_has_arc_from_fix(path, fix, turn_clockwise))
            {
                continue;
            }
            if paths.iter().any(|path| {
                explicit_missed_direct_to_hold_turn_is_visually_negligible(
                    path, records, record, fix,
                )
            }) {
                continue;
            }
            if paths
                .iter()
                .any(|path| explicit_missed_direct_turn_is_visually_negligible(path, fix))
            {
                continue;
            }
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "explicit missed direct-to turn not materialized for {} route {} seq {} turn {}",
                    procedure_id.trim(),
                    record.key.route_type.trim(),
                    record.sequence,
                    record.turn_direction.as_deref().unwrap_or("").trim(),
                ),
            });
        }
    }
    Ok(())
}

fn explicit_missed_direct_to_same_fix_hold(
    records: &[ProcedureLegMaterializationRecord],
    record: &ProcedureLegMaterializationRecord,
    fix: LatLon,
) -> bool {
    records
        .iter()
        .filter(|candidate| candidate.sequence > record.sequence)
        .min_by_key(|candidate| candidate.sequence)
        .is_some_and(|next_record| {
            matches!(next_record.path_termination.trim(), "HA" | "HF" | "HM")
                && next_record.nav_position == Some(fix)
        })
}

fn explicit_missed_direct_to_hold_turn_is_visually_negligible(
    path: &LegDisplayPath,
    records: &[ProcedureLegMaterializationRecord],
    record: &ProcedureLegMaterializationRecord,
    fix: LatLon,
) -> bool {
    let Some(next_record) = records
        .iter()
        .filter(|candidate| candidate.sequence > record.sequence)
        .min_by_key(|candidate| candidate.sequence)
    else {
        return false;
    };
    if !matches!(next_record.path_termination.trim(), "HA" | "HF" | "HM")
        || next_record.nav_position != Some(fix)
    {
        return false;
    }
    let Some(LegDisplayElement::Segment { start, end }) = path.elements.first() else {
        return false;
    };
    if !positions_nearly_equal(*end, fix) {
        return false;
    }
    let distance_nm = great_circle_distance_nm(*start, *end);
    // Some missed DF rows carry an explicit turn direction even when the fix is so
    // close/aligned with an immediate hold that drawing a separate pre-fix arc
    // would be decorative or actively misleading.
    // KAIA I30/AIA reaches AIA after 0.9 nm; KBHK R31/DIXLE reaches KIXCO after
    // 19.5 nm on a straight direct-to before entering the published hold.
    distance_nm <= 25.0
}

fn explicit_missed_direct_turn_is_visually_negligible(path: &LegDisplayPath, fix: LatLon) -> bool {
    for (index, element) in path.elements.iter().enumerate() {
        let LegDisplayElement::Segment { start, end } = element else {
            continue;
        };
        if !positions_nearly_equal(*end, fix) {
            continue;
        }
        if great_circle_distance_nm(*start, *end) > 25.0 {
            return false;
        }
        let Some(previous) = index
            .checked_sub(1)
            .and_then(|previous_index| path.elements.get(previous_index))
        else {
            return true;
        };
        let Some((_, _, _, previous_course_deg)) = heading_signature_for_element(previous) else {
            return false;
        };
        let direct_course_deg = initial_course_deg(*start, *end);
        return angular_difference_degrees(previous_course_deg, direct_course_deg) <= 10.0;
    }
    false
}

fn explicit_turn_path_has_arc_before_fix(
    path: &LegDisplayPath,
    fix: LatLon,
    turn_clockwise: bool,
) -> bool {
    let mut saw_directed_arc = false;
    for element in &path.elements {
        match element {
            LegDisplayElement::Arc { clockwise, .. } => {
                if *clockwise == turn_clockwise {
                    saw_directed_arc = true;
                }
            }
            LegDisplayElement::Segment { end, .. } => {
                if positions_nearly_equal(*end, fix) {
                    return saw_directed_arc;
                }
            }
        }
    }
    false
}

fn explicit_turn_path_has_arc_from_fix(
    path: &LegDisplayPath,
    fix: LatLon,
    turn_clockwise: bool,
) -> bool {
    matches!(
        path.elements.first(),
        Some(LegDisplayElement::Arc {
            start,
            clockwise,
            ..
        }) if *clockwise == turn_clockwise && positions_nearly_equal(*start, fix)
    )
}

#[derive(Clone)]
struct DisplayElementHeadingSignature {
    step_index: usize,
    airport_id: String,
    procedure_id: String,
    procedure_role: ProcedureSegmentRole,
    path_termination: String,
    start_position: LatLon,
    start_course_deg: f64,
    start_label: String,
    start_magnetic_variation_deg: Option<f64>,
    end_position: LatLon,
    end_course_deg: f64,
    drawn_end_course_deg: f64,
    end_label: String,
    end_magnetic_variation_deg: Option<f64>,
    element_kind: DisplayElementKind,
    element_role: Option<String>,
    turn_direction: Option<String>,
    discontinuity_after: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayElementKind {
    Segment,
    Arc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HeadingContinuityAllowance {
    PublishedAcuteTurn,
    InternalGeneratedDisplayPathTurn,
    HoldEntryExitToPublishedCourse,
    HoldEntryGeneratedCourseIntercept,
    PublishedHoldEntry,
    ChartedArcHandoff,
    GeneratedTurnArcEntry,
    PublishedProcedureTurnInternal,
    GeneratedDirectToFixInternal,
    GeneratedPathExitToPublishedCourse,
    PublishedWaypointTurnWithRoom,
    DeferredFlyByTfTurn,
    PublishedCourseIntercept,
    PublishedShortFeederToFinalCourse,
    PublishedFcCfFeederToFinalCourse,
    PublishedProcedureTurnEntry,
    PublishedProcedureTurnExit,
    PublishedMissedRouteTurnWithRoom,
    ShortMapToAirportNavaidDirect,
}

impl HeadingContinuityAllowance {
    #[cfg(test)]
    fn name(self) -> &'static str {
        match self {
            Self::PublishedAcuteTurn => "published_acute_turn",
            Self::InternalGeneratedDisplayPathTurn => "internal_generated_display_path_turn",
            Self::HoldEntryExitToPublishedCourse => "hold_entry_exit_to_published_course",
            Self::HoldEntryGeneratedCourseIntercept => "hold_entry_generated_course_intercept",
            Self::PublishedHoldEntry => "published_hold_entry",
            Self::ChartedArcHandoff => "charted_arc_handoff",
            Self::GeneratedTurnArcEntry => "generated_turn_arc_entry",
            Self::PublishedProcedureTurnInternal => "published_procedure_turn_internal",
            Self::GeneratedDirectToFixInternal => "generated_direct_to_fix_internal",
            Self::GeneratedPathExitToPublishedCourse => "generated_path_exit_to_published_course",
            Self::PublishedWaypointTurnWithRoom => "published_waypoint_turn_with_room",
            Self::DeferredFlyByTfTurn => "deferred_fly_by_tf_turn",
            Self::PublishedCourseIntercept => "published_course_intercept",
            Self::PublishedShortFeederToFinalCourse => "published_short_feeder_to_final_course",
            Self::PublishedFcCfFeederToFinalCourse => "published_fc_cf_feeder_to_final_course",
            Self::PublishedProcedureTurnEntry => "published_procedure_turn_entry",
            Self::PublishedProcedureTurnExit => "published_procedure_turn_exit",
            Self::PublishedMissedRouteTurnWithRoom => "published_missed_route_turn_with_room",
            Self::ShortMapToAirportNavaidDirect => "short_map_to_airport_navaid_direct",
        }
    }

    #[cfg(test)]
    fn all() -> [Self; 19] {
        [
            Self::PublishedAcuteTurn,
            Self::InternalGeneratedDisplayPathTurn,
            Self::HoldEntryExitToPublishedCourse,
            Self::HoldEntryGeneratedCourseIntercept,
            Self::PublishedHoldEntry,
            Self::ChartedArcHandoff,
            Self::GeneratedTurnArcEntry,
            Self::PublishedProcedureTurnInternal,
            Self::GeneratedDirectToFixInternal,
            Self::GeneratedPathExitToPublishedCourse,
            Self::PublishedWaypointTurnWithRoom,
            Self::DeferredFlyByTfTurn,
            Self::PublishedCourseIntercept,
            Self::PublishedShortFeederToFinalCourse,
            Self::PublishedFcCfFeederToFinalCourse,
            Self::PublishedProcedureTurnEntry,
            Self::PublishedProcedureTurnExit,
            Self::PublishedMissedRouteTurnWithRoom,
            Self::ShortMapToAirportNavaidDirect,
        ]
    }
}

fn heading_signatures_for_leg(
    starting_step_index: usize,
    display_path: Option<&LegDisplayPath>,
    from_record: &ProcedureLegMaterializationRecord,
    to_record: &ProcedureLegMaterializationRecord,
    path_termination: &str,
    procedure_role: &ProcedureSegmentRole,
    turn_direction: Option<&str>,
) -> Vec<DisplayElementHeadingSignature> {
    if let Some(path) = display_path {
        let last_index = path.elements.len().saturating_sub(1);
        return path
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                let (start_position, start_course_deg, end_position, mut end_course_deg) =
                    heading_signature_for_element(element)?;
                let drawn_end_course_deg = end_course_deg;
                if index == last_index {
                    if let Some(logical_end_course_deg) = path.effective_terminal_course_deg {
                        end_course_deg = logical_end_course_deg;
                    }
                }
                let element_role = debug_element_role(path, index);
                Some(DisplayElementHeadingSignature {
                    step_index: starting_step_index + index,
                    airport_id: from_record.key.airport_id.trim().to_string(),
                    procedure_id: from_record.key.procedure_id.trim().to_string(),
                    procedure_role: procedure_role.clone(),
                    path_termination: path_termination.to_string(),
                    start_position,
                    start_course_deg,
                    start_label: if index == 0 {
                        describe_record_anchor(from_record)
                    } else {
                        "synthesized-path".to_string()
                    },
                    start_magnetic_variation_deg: if index == 0 {
                        record_magnetic_variation_deg(from_record)
                    } else {
                        None
                    },
                    end_position,
                    end_course_deg,
                    drawn_end_course_deg,
                    end_label: if index == last_index {
                        describe_record_anchor(to_record)
                    } else {
                        "synthesized-path".to_string()
                    },
                    end_magnetic_variation_deg: if index == last_index {
                        record_magnetic_variation_deg(to_record)
                    } else {
                        None
                    },
                    element_kind: display_element_kind(element),
                    element_role,
                    turn_direction: turn_direction.map(str::to_string),
                    discontinuity_after: false,
                })
            })
            .collect::<Vec<_>>();
    }
    let Some(start) = from_record.nav_position else {
        return Vec::new();
    };
    let Some(end) = to_record.nav_position else {
        return Vec::new();
    };
    let course = bearing_degrees(start, end);
    vec![DisplayElementHeadingSignature {
        step_index: starting_step_index,
        airport_id: from_record.key.airport_id.trim().to_string(),
        procedure_id: from_record.key.procedure_id.trim().to_string(),
        procedure_role: procedure_role.clone(),
        path_termination: path_termination.to_string(),
        start_position: start,
        start_course_deg: course,
        start_label: describe_record_anchor(from_record),
        start_magnetic_variation_deg: record_magnetic_variation_deg(from_record),
        end_position: end,
        end_course_deg: course,
        drawn_end_course_deg: course,
        end_label: describe_record_anchor(to_record),
        end_magnetic_variation_deg: record_magnetic_variation_deg(to_record),
        element_kind: DisplayElementKind::Segment,
        element_role: None,
        turn_direction: turn_direction.map(str::to_string),
        discontinuity_after: false,
    }]
}

fn debug_element_role(path: &LegDisplayPath, index: usize) -> Option<String> {
    path.debug_element_roles
        .get(index)
        .filter(|role| !role.is_empty())
        .cloned()
        .or_else(|| {
            let source = path.debug_element_sources.get(index)?;
            source
                .split_once('@')
                .map(|(role, _)| role)
                .filter(|role| matches!(*role, "hold_entry" | "hold_racetrack"))
                .map(str::to_string)
        })
}

fn validate_heading_continuity_checks(
    checks: &[DisplayElementHeadingSignature],
    validate_heading_continuity: bool,
    procedure_id: &str,
    kind: &ProcedureKind,
) -> AppResult<()> {
    if !validate_heading_continuity {
        return Ok(());
    }
    let mut worst_gap: Option<(
        f64,
        &DisplayElementHeadingSignature,
        &DisplayElementHeadingSignature,
    )> = None;
    let mut worst_violation: Option<(
        f64,
        f64,
        &'static str,
        &DisplayElementHeadingSignature,
        &DisplayElementHeadingSignature,
    )> = None;
    for index in 0..checks.len().saturating_sub(1) {
        let previous_previous = index.checked_sub(1).and_then(|prior| checks.get(prior));
        let previous = &checks[index];
        let current = &checks[index + 1];
        let next = checks.get(index + 2);
        if previous.discontinuity_after {
            continue;
        }
        if !display_positions_stitch(previous.end_position, current.start_position) {
            let gap_nm = great_circle_distance_nm(previous.end_position, current.start_position);
            if worst_gap
                .as_ref()
                .is_none_or(|(worst_gap_nm, ..)| gap_nm > *worst_gap_nm)
            {
                worst_gap = Some((gap_nm, previous, current));
            }
            continue;
        }
        let allowed_delta_deg =
            continuity_heading_tolerance_deg(previous_previous, previous, current, next, kind);
        for (delta, heading_mode) in [
            (
                angular_difference_degrees(previous.end_course_deg, current.start_course_deg),
                "logical",
            ),
            (
                angular_difference_degrees(previous.drawn_end_course_deg, current.start_course_deg),
                "drawn",
            ),
        ] {
            if delta > allowed_delta_deg
                && worst_violation
                    .as_ref()
                    .is_none_or(|(worst_delta, ..)| delta > *worst_delta)
            {
                worst_violation = Some((delta, allowed_delta_deg, heading_mode, previous, current));
            }
        }
    }
    if let Some((gap_nm, previous, current)) = worst_gap {
        let fix_description = if previous.end_label == current.start_label {
            previous.end_label.clone()
        } else {
            format!("{} -> {}", previous.end_label, current.start_label)
        };
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "procedure path continuity violated for {}: gap_nm={:.2} between steps={:02}->{:02} at {} end=({:.6},{:.6}) start=({:.6},{:.6})",
                procedure_id.trim(),
                gap_nm,
                previous.step_index,
                current.step_index,
                fix_description,
                previous.end_position.lat,
                previous.end_position.lon,
                current.start_position.lat,
                current.start_position.lon,
            ),
        });
    }
    if let Some((delta, allowed_delta_deg, heading_mode, previous, current)) = worst_violation {
        let fix_description = if previous.end_label == current.start_label {
            previous.end_label.clone()
        } else {
            format!("{} -> {}", previous.end_label, current.start_label)
        };
        let inbound_course_deg = if heading_mode == "drawn" {
            previous.drawn_end_course_deg
        } else {
            previous.end_course_deg
        };
        let inbound_magnetic_heading = magnetic_heading_degrees(
            inbound_course_deg,
            previous
                .end_magnetic_variation_deg
                .or(current.start_magnetic_variation_deg),
        );
        let outbound_magnetic_heading = magnetic_heading_degrees(
            current.start_course_deg,
            current
                .start_magnetic_variation_deg
                .or(previous.end_magnetic_variation_deg),
        );
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "procedure {} heading continuity violated for {}: {:.1} deg (allowed {:.1}) at {} ({:.6},{:.6}) inbound_mh={:.1} outbound_mh={:.1} steps={:02}->{:02} prev={} {:?} {}->{} prev_len_nm={:.1} current={} {:?} {}->{} current_len_nm={:.1}",
                heading_mode,
                procedure_id.trim(),
                delta,
                allowed_delta_deg,
                fix_description,
                previous.end_position.lat,
                previous.end_position.lon,
                inbound_magnetic_heading,
                outbound_magnetic_heading,
                previous.step_index,
                current.step_index,
                previous.path_termination,
                previous.element_kind,
                previous.start_label,
                previous.end_label,
                great_circle_distance_nm(previous.start_position, previous.end_position),
                current.path_termination,
                current.element_kind,
                current.start_label,
                current.end_label,
                great_circle_distance_nm(current.start_position, current.end_position),
            ),
        });
    }
    Ok(())
}

fn positions_nearly_equal(a: LatLon, b: LatLon) -> bool {
    (a.lat - b.lat).abs() < POSITION_EPSILON_DEG && (a.lon - b.lon).abs() < POSITION_EPSILON_DEG
}

fn continuity_heading_tolerance_deg(
    previous_previous: Option<&DisplayElementHeadingSignature>,
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
    next: Option<&DisplayElementHeadingSignature>,
    kind: &ProcedureKind,
) -> f64 {
    if *kind == ProcedureKind::Sid
        && matches!(current.path_termination.as_str(), "FM" | "VM")
        && current.element_kind == DisplayElementKind::Arc
        && current.start_label != "synthesized-path"
        && current.end_label == "synthesized-path"
    {
        return 30.0;
    }
    if let Some((allowance, allowed_delta_deg)) = named_heading_continuity_allowance_with_context(
        previous_previous,
        previous,
        current,
        next,
        kind,
    ) {
        if *kind == ProcedureKind::Sid
            && allowance == HeadingContinuityAllowance::GeneratedPathExitToPublishedCourse
        {
            let leading_track_before_downstream_hold =
                matches!(current.path_termination.as_str(), "HA" | "HF" | "HM")
                    && current.element_role.is_none()
                    && great_circle_distance_nm(current.start_position, current.end_position)
                        >= 1.5;
            return if matches!(previous.turn_direction.as_deref(), Some("L" | "R"))
                || leading_track_before_downstream_hold
            {
                120.0
            } else {
                45.0
            };
        }
        if *kind == ProcedureKind::Sid
            && allowance == HeadingContinuityAllowance::PublishedWaypointTurnWithRoom
        {
            // SID route sections can join at a named waypoint with a sharper
            // turn than approach geometry permits. A reciprocal runway-to-common
            // join is handled as an unspecified course-reversal discontinuity;
            // keep a finite cap on every other join so misordered routes fail.
            if arinc_ambiguity_resolutions::sid_segment_boundary_requires_unspecified_course_reversal(
                &previous.airport_id,
                &previous.procedure_id,
                &previous.end_label,
            ) && angular_difference_degrees(previous.end_course_deg, current.start_course_deg)
                >= 150.0
            {
                return 180.0;
            }
            if previous.procedure_role == ProcedureSegmentRole::RunwayTransition
                && current.procedure_role == ProcedureSegmentRole::Common
                && angular_difference_degrees(previous.end_course_deg, current.start_course_deg)
                    >= 165.0
            {
                return 180.0;
            }
            if previous.procedure_role == ProcedureSegmentRole::RunwayTransition
                && current.procedure_role != ProcedureSegmentRole::RunwayTransition
                && matches!(previous.turn_direction.as_deref(), Some("L" | "R"))
                && angular_difference_degrees(previous.end_course_deg, current.start_course_deg)
                    >= 150.0
            {
                return 180.0;
            }
            return 150.0;
        }
        return allowed_delta_deg;
    }
    continuity_path_boundary_tolerance_deg(previous, current)
}

fn named_heading_continuity_allowance_with_context(
    previous_previous: Option<&DisplayElementHeadingSignature>,
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
    next: Option<&DisplayElementHeadingSignature>,
    kind: &ProcedureKind,
) -> Option<(HeadingContinuityAllowance, f64)> {
    if let Some(allowed_delta_deg) = published_acute_turn_heading_tolerance_deg(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedAcuteTurn,
            allowed_delta_deg,
        ));
    }
    if enters_published_hold(previous, current) {
        return Some((HeadingContinuityAllowance::PublishedHoldEntry, 125.0));
    }
    if is_internal_published_procedure_turn(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedProcedureTurnInternal,
            180.0,
        ));
    }
    if is_internal_generated_direct_to_fix(previous, current) {
        return Some((
            HeadingContinuityAllowance::GeneratedDirectToFixInternal,
            160.0,
        ));
    }
    if is_internal_generated_display_path_turn(previous, current) {
        return Some((
            HeadingContinuityAllowance::InternalGeneratedDisplayPathTurn,
            120.0,
        ));
    }
    if is_hold_entry_generated_course_intercept(previous, current) {
        return Some((
            HeadingContinuityAllowance::HoldEntryGeneratedCourseIntercept,
            35.0,
        ));
    }
    if is_internal_hold_geometry_turn(previous, current) {
        return Some((
            HeadingContinuityAllowance::InternalGeneratedDisplayPathTurn,
            120.0,
        ));
    }
    if leaves_hold_entry_to_published_course(previous, current) {
        return Some((
            HeadingContinuityAllowance::HoldEntryExitToPublishedCourse,
            if matches!(previous.element_role.as_deref(), Some("hold_entry")) {
                45.0
            } else {
                35.0
            },
        ));
    }
    if leaves_hold_geometry_to_published_course(previous, current) {
        return Some((
            HeadingContinuityAllowance::HoldEntryExitToPublishedCourse,
            35.0,
        ));
    }
    if is_charted_arc_handoff(previous, current) {
        return Some((HeadingContinuityAllowance::ChartedArcHandoff, 135.0));
    }
    if enters_generated_turn_arc(previous, current) {
        return Some((HeadingContinuityAllowance::GeneratedTurnArcEntry, 10.0));
    }
    if is_deferred_fly_by_tf_turn(previous, current) {
        return Some((HeadingContinuityAllowance::DeferredFlyByTfTurn, 120.0));
    }
    if is_published_procedure_turn_exit(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedProcedureTurnExit,
            180.0,
        ));
    }
    if is_published_waypoint_turn_with_room(previous, current, kind) {
        return Some((
            HeadingContinuityAllowance::PublishedWaypointTurnWithRoom,
            120.0,
        ));
    }
    if is_published_course_intercept(previous, current) {
        return Some((HeadingContinuityAllowance::PublishedCourseIntercept, 120.0));
    }
    if is_published_short_feeder_to_final_course(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedShortFeederToFinalCourse,
            50.0,
        ));
    }
    if is_published_fc_cf_feeder_to_final_course(previous_previous, previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedFcCfFeederToFinalCourse,
            50.0,
        ));
    }
    if is_published_procedure_turn_entry(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedProcedureTurnEntry,
            arinc_ambiguity_resolutions::SIMPLE_PI_ENTRY_MAX_TURN_DEG,
        ));
    }
    if is_published_missed_route_turn_with_room(previous, current) {
        return Some((
            HeadingContinuityAllowance::PublishedMissedRouteTurnWithRoom,
            120.0,
        ));
    }
    if is_short_map_to_airport_navaid_direct(previous, current, next) {
        return Some((
            HeadingContinuityAllowance::ShortMapToAirportNavaidDirect,
            20.0,
        ));
    }
    if is_published_waypoint_turn_via_short_fc_stub(previous, current, next) {
        return Some((
            HeadingContinuityAllowance::PublishedWaypointTurnWithRoom,
            120.0,
        ));
    }
    if leaves_generated_path_to_published_course(previous, current) {
        return Some((
            HeadingContinuityAllowance::GeneratedPathExitToPublishedCourse,
            20.0,
        ));
    }
    None
}

fn is_internal_generated_display_path_turn(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    // Synthetic vertices are generated by a single display path to draw flyable
    // turns/climbs/intercepts; keep strict 10 degree validation at real fixes.
    if matches!(
        previous.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    ) || matches!(
        current.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    ) {
        return false;
    }
    previous.end_label == "synthesized-path" && current.start_label == "synthesized-path"
}

fn is_internal_published_procedure_turn(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    previous.path_termination == "PI"
        && current.path_termination == "PI"
        && previous.end_label == "synthesized-path"
        && current.start_label == "synthesized-path"
}

fn is_internal_generated_direct_to_fix(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    previous.path_termination == "DF"
        && current.path_termination == "DF"
        && previous.end_label == "synthesized-path"
        && current.start_label == "synthesized-path"
}

fn is_hold_entry_generated_course_intercept(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if !matches!(previous.element_role.as_deref(), Some("hold_entry"))
        || previous.end_label != "synthesized-path"
        || current.start_label != "synthesized-path"
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    (matches!(current.element_role.as_deref(), Some("hold_entry"))
        && previous.element_kind == DisplayElementKind::Arc)
        || (previous.element_kind == DisplayElementKind::Segment
            && current.path_termination == "PI"
            && current.element_role.is_none())
}

fn is_internal_hold_geometry_turn(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    let previous_is_hold = matches!(
        previous.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    );
    let current_is_hold = matches!(
        current.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    );
    previous_is_hold
        && current_is_hold
        && previous.end_label == "synthesized-path"
        && current.start_label == "synthesized-path"
}

fn leaves_hold_entry_to_published_course(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if !matches!(previous.path_termination.as_str(), "HA" | "HF" | "HM") {
        return false;
    }
    if previous.start_label != "synthesized-path" || previous.end_label == "synthesized-path" {
        return false;
    }
    if !positions_nearly_equal(previous.end_position, current.start_position) {
        return false;
    }
    matches!(current.path_termination.as_str(), "CF" | "TF")
}

fn leaves_hold_geometry_to_published_course(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if !matches!(
        previous.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    ) || matches!(
        current.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    ) {
        return false;
    }
    if previous.end_label != "synthesized-path" || current.start_label != "synthesized-path" {
        return false;
    }
    if !positions_nearly_equal(previous.end_position, current.start_position) {
        return false;
    }
    matches!(current.path_termination.as_str(), "CF" | "TF")
}

fn enters_published_hold(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    let current_is_hold_geometry = matches!(
        current.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    );
    let previous_is_hold_geometry = matches!(
        previous.element_role.as_deref(),
        Some("hold_entry" | "hold_racetrack")
    );
    current_is_hold_geometry
        && !previous_is_hold_geometry
        && positions_nearly_equal(previous.end_position, current.start_position)
}

fn is_charted_arc_handoff(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    matches!(previous.path_termination.as_str(), "AF" | "RF")
        || matches!(current.path_termination.as_str(), "AF" | "RF")
        || (previous.element_kind == DisplayElementKind::Arc
            && previous.end_label != "synthesized-path")
        || (current.element_kind == DisplayElementKind::Arc
            && current.end_label != "synthesized-path")
        || (previous.element_kind == DisplayElementKind::Segment
            && current.element_kind == DisplayElementKind::Arc
            && previous.end_label != "synthesized-path"
            && previous.end_label == current.start_label
            && current.end_label == "synthesized-path"
            && previous_len_nm >= 1.5
            && current_len_nm >= 3.0)
}

fn enters_generated_turn_arc(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if angular_difference_degrees(previous.drawn_end_course_deg, previous.end_course_deg)
        > continuity_path_boundary_tolerance_deg(previous, current)
    {
        return false;
    }
    current.element_kind == DisplayElementKind::Arc
        && current.start_label != "synthesized-path"
        && current.end_label == "synthesized-path"
}

fn leaves_generated_path_to_published_course(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    previous.element_kind == DisplayElementKind::Segment
        && current.element_kind == DisplayElementKind::Segment
        && previous.start_label == "synthesized-path"
        && previous.end_label != "synthesized-path"
        && previous.end_label == current.start_label
}

fn is_published_waypoint_turn_with_room(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
    kind: &ProcedureKind,
) -> bool {
    if previous.end_label == "synthesized-path" || current.start_label == "synthesized-path" {
        return false;
    }
    if previous.end_label != current.start_label
        && !positions_nearly_equal(previous.end_position, current.start_position)
    {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    if previous.procedure_role == ProcedureSegmentRole::RunwayTransition
        && current.procedure_role == ProcedureSegmentRole::RunwayTransition
    {
        return previous_len_nm >= 1.0
            && current_len_nm
                >= if *kind == ProcedureKind::Star {
                    1.0
                } else {
                    1.5
                };
    }
    if previous.path_termination == "TF" && current.path_termination == "TF" {
        // Short published TF feeders can still chart a real fly-by waypoint turn
        // when the outbound leg gives enough room; KIND I05L/KELLY at CFVDC is
        // the motivating example. Preserve the earlier short-RNAV-stepdown rule
        // for tightly spaced fixes like KMCI I01R/HELAN at UJGAV.
        return (previous_len_nm >= 1.0 && current_len_nm >= 1.0)
            || (previous_len_nm >= 0.8 && current_len_nm >= 3.0);
    }
    previous_len_nm >= 1.5 && current_len_nm >= 1.5
}

fn is_published_waypoint_turn_via_short_fc_stub(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
    next: Option<&DisplayElementHeadingSignature>,
) -> bool {
    let Some(next) = next else {
        return false;
    };
    if !matches!(current.path_termination.as_str(), "FC" | "CF") {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
        || next.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    if previous.end_label == "synthesized-path"
        || current.start_label == "synthesized-path"
        || current.end_label != "synthesized-path"
        || next.start_label != "synthesized-path"
        || next.end_label == "synthesized-path"
    {
        return false;
    }
    if previous.end_label != current.start_label
        && !positions_nearly_equal(previous.end_position, current.start_position)
    {
        return false;
    }
    if !positions_nearly_equal(current.end_position, next.start_position) {
        return false;
    }
    if angular_difference_degrees(current.end_course_deg, next.start_course_deg) > 10.0 {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    let next_len_nm = great_circle_distance_nm(next.start_position, next.end_position);
    previous_len_nm >= 1.5 && current_len_nm <= 1.0 && next_len_nm >= 1.5
}

fn is_deferred_fly_by_tf_turn(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if previous.path_termination != "TF" || current.path_termination != "TF" {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    if previous.end_label == "synthesized-path"
        || current.start_label == "synthesized-path"
        || previous.end_label != current.start_label
    {
        return false;
    }
    // These are real TF fly-by corners that need turn anticipation/filleting.
    // Keep them named instead of blessing them as generic sharp turns.
    previous.start_label == "synthesized-path" || current.end_label == "synthesized-path"
}

fn is_published_course_intercept(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if current.path_termination != "CF" {
        return false;
    }
    if previous.end_label == "synthesized-path" || current.start_label == "synthesized-path" {
        return false;
    }
    if previous.end_label != current.start_label {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    if previous.path_termination == "TF" && previous_len_nm >= 1.5 && current_len_nm >= 1.0 {
        return true;
    }
    previous_len_nm >= 1.5 && current_len_nm >= 1.5
}

fn is_published_short_feeder_to_final_course(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if previous.path_termination != "TF" || current.path_termination != "CF" {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    if previous.end_label == "synthesized-path"
        || current.start_label == "synthesized-path"
        || previous.end_label != current.start_label
    {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    // Some ARINC transitions encode a tiny TF feeder into a charted CF final
    // intercept. The outbound CF may itself be short when the common segment is
    // split into stepdown fixes, so keep the structural TF->CF shape and narrow
    // angle budget as the guard rather than requiring a long first CF leg.
    (0.15..=1.6).contains(&previous_len_nm) && current_len_nm >= 1.0
}

fn is_published_fc_cf_feeder_to_final_course(
    previous_previous: Option<&DisplayElementHeadingSignature>,
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    let Some(previous_previous) = previous_previous else {
        return false;
    };
    if !matches!(
        previous_previous.path_termination.as_str(),
        "FC" | "CF" | "CA"
    ) || !matches!(previous.path_termination.as_str(), "CF" | "CA")
        || current.path_termination != "CF"
    {
        return false;
    }
    if previous_previous.element_kind != DisplayElementKind::Segment
        || previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    if previous_previous.end_label != "synthesized-path"
        || previous.start_label != "synthesized-path"
        || previous.end_label == "synthesized-path"
        || current.start_label == "synthesized-path"
        || previous.end_label != current.start_label
    {
        return false;
    }
    if angular_difference_degrees(previous_previous.end_course_deg, previous.start_course_deg)
        > 10.0
        || angular_difference_degrees(previous_previous.end_course_deg, previous.end_course_deg)
            > 10.0
    {
        return false;
    }
    let fc_len_nm = great_circle_distance_nm(
        previous_previous.start_position,
        previous_previous.end_position,
    );
    let cf_bridge_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let outbound_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    // KSTS L32/SGD encodes a course/distance FC followed by a short CF snap to
    // the named fix before turning onto the next published CF. The resolver
    // materializes that raw FC/CF pair as CA/CA display signatures; treat the
    // pair as one feeder, but keep both the bridge and combined feeder bounded.
    cf_bridge_len_nm <= 1.0
        && (0.5..=4.0).contains(&(fc_len_nm + cf_bridge_len_nm))
        && outbound_len_nm >= 1.0
}

fn is_published_procedure_turn_entry(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if previous.path_termination != "TF" || current.path_termination != "PI" {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    if previous.end_label == "synthesized-path"
        || current.start_label == "synthesized-path"
        || previous.end_label != current.start_label
    {
        return false;
    }
    if !positions_nearly_equal(previous.end_position, current.start_position) {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    // Short feeders into a charted PI are legitimate intercepts. The first PI
    // display element may be the shortened 0.5 NM outbound stub used for tight
    // "remain within" procedure turns, so do not require the old 3 NM default.
    previous_len_nm >= 0.9 && current_len_nm >= 0.5
}

fn is_published_procedure_turn_exit(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    previous.path_termination == "PI"
        && matches!(current.path_termination.as_str(), "CF" | "TF")
        && previous.end_label != "synthesized-path"
        && previous.end_label == current.start_label
        && positions_nearly_equal(previous.end_position, current.start_position)
}

fn is_published_missed_route_turn_with_room(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    if !matches!(current.path_termination.as_str(), "HA" | "HF" | "HM") {
        return false;
    }
    if previous.end_label == "synthesized-path" || current.start_label == "synthesized-path" {
        return false;
    }
    if previous.end_label != current.start_label {
        return false;
    }
    if previous.element_kind != DisplayElementKind::Segment
        || current.element_kind != DisplayElementKind::Segment
    {
        return false;
    }
    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    // Missed-approach hold fixes are often deliberately close to the runway.
    // KORF R32/SUNNS turns at CERIN after a rounded 1.0 nm, and KHSP R25/AHLER and
    // KCEK R35/BIE show the same charted close-in fix-to-hold pattern.
    if previous.end_label.starts_with("RW") {
        return previous_len_nm >= 0.9 && current_len_nm >= 0.5;
    }
    previous_len_nm >= 0.9 && current_len_nm >= 2.0
}

fn is_short_map_to_airport_navaid_direct(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
    next: Option<&DisplayElementHeadingSignature>,
) -> bool {
    fn airport_navaid_label(airport_id: &str) -> &str {
        airport_id
            .trim()
            .strip_prefix('K')
            .unwrap_or(airport_id.trim())
    }

    let airport_navaid = airport_navaid_label(&previous.airport_id);
    if airport_navaid.is_empty()
        || previous.airport_id != current.airport_id
        || previous.procedure_id != current.procedure_id
    {
        return false;
    }

    let current_len_nm = great_circle_distance_nm(current.start_position, current.end_position);
    if current.path_termination == "DF"
        && current.element_kind == DisplayElementKind::Segment
        && current.start_label.starts_with("RW")
        && current.end_label == airport_navaid
        && current_len_nm <= 1.5
    {
        // KSPW I12 and KSTC I13/I31 encode the missed approach as a direct-to
        // the colocated airport navaid immediately after the MAP/runway fix.
        // Keep this narrow so arbitrary short post-runway kinks still fail.
        return next.is_some_and(|next| {
            next.start_label == current.end_label
                && matches!(
                    next.path_termination.as_str(),
                    "CF" | "DF" | "HA" | "HF" | "HM"
                )
        });
    }

    let previous_len_nm = great_circle_distance_nm(previous.start_position, previous.end_position);
    previous.path_termination == "DF"
        && previous.element_kind == DisplayElementKind::Segment
        && previous.start_label.starts_with("RW")
        && previous.end_label == airport_navaid
        && current.start_label == previous.end_label
        && matches!(
            current.path_termination.as_str(),
            "CF" | "DF" | "HA" | "HF" | "HM"
        )
        && previous_len_nm <= 1.5
}

fn published_acute_turn_heading_tolerance_deg(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> Option<f64> {
    if allow_acute_turn_ksan_09_family_at_pgy(previous, current) {
        return Some(150.0);
    }
    if allow_acute_turn_kykm_vora_missed_at_ykm(previous, current) {
        return Some(180.0);
    }
    None
}

fn allow_acute_turn_ksan_09_family_at_pgy(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    arinc_ambiguity_resolutions::acute_turn_ksan_09_family_at_pgy(
        &previous.airport_id,
        &current.airport_id,
        &previous.procedure_id,
        &current.procedure_id,
        &previous.end_label,
        &current.start_label,
    )
}

fn allow_acute_turn_kykm_vora_missed_at_ykm(
    previous: &DisplayElementHeadingSignature,
    current: &DisplayElementHeadingSignature,
) -> bool {
    let inbound_magnetic_heading = magnetic_heading_degrees(
        previous.end_course_deg,
        previous
            .end_magnetic_variation_deg
            .or(current.start_magnetic_variation_deg),
    );
    let outbound_magnetic_heading = magnetic_heading_degrees(
        current.start_course_deg,
        current
            .start_magnetic_variation_deg
            .or(previous.end_magnetic_variation_deg),
    );
    arinc_ambiguity_resolutions::acute_turn_kykm_vora_missed_at_ykm(
        arinc_ambiguity_resolutions::AcuteTurnBoundary {
            previous_airport_id: &previous.airport_id,
            current_airport_id: &current.airport_id,
            previous_procedure_id: &previous.procedure_id,
            current_procedure_id: &current.procedure_id,
            previous_end_label: &previous.end_label,
            current_start_label: &current.start_label,
            inbound_magnetic_heading_deg: inbound_magnetic_heading,
            outbound_magnetic_heading_deg: outbound_magnetic_heading,
        },
    )
}

fn continuity_path_boundary_tolerance_deg(
    previous: &DisplayElementHeadingSignature,
    _current: &DisplayElementHeadingSignature,
) -> f64 {
    let default_tolerance_deg = 10.0;
    match (
        previous.airport_id.as_str(),
        previous.procedure_id.as_str(),
        "path_boundary_tolerance_deg",
    ) {
        // KHYA L24's missed-approach VI to 045 then CF to BOGEY consistently needs
        // about 11.7° of cleanup under our nominal geometry; the chart/coding itself
        // appears a bit awkward, so allow a slightly wider handoff there.
        ("KHYA", "L24", "path_boundary_tolerance_deg") => 15.0,
        _ => default_tolerance_deg,
    }
}

fn heading_signature_for_element(
    element: &LegDisplayElement,
) -> Option<(LatLon, f64, LatLon, f64)> {
    match element {
        LegDisplayElement::Segment { start, end } => {
            let course = bearing_degrees(*start, *end);
            Some((*start, course, *end, course))
        }
        LegDisplayElement::Arc {
            center,
            start,
            end,
            clockwise,
            ..
        } => {
            let start_radial_deg = bearing_degrees(*center, *start);
            let end_radial_deg = bearing_degrees(*center, *end);
            let start_course_deg = normalize_bearing_degrees(if *clockwise {
                start_radial_deg + 90.0
            } else {
                start_radial_deg - 90.0
            });
            let end_course_deg = normalize_bearing_degrees(if *clockwise {
                end_radial_deg + 90.0
            } else {
                end_radial_deg - 90.0
            });
            Some((*start, start_course_deg, *end, end_course_deg))
        }
    }
}

fn describe_record_anchor(record: &ProcedureLegMaterializationRecord) -> String {
    record
        .nav_ref
        .as_ref()
        .map(describe_nav_ref)
        .unwrap_or_else(|| "synthesized-path".to_string())
}

fn describe_nav_ref(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(code) => code.clone(),
        NavRef::Navaid(code) => code.clone(),
        NavRef::ArincNavaid { identifier, .. } => identifier.clone(),
        NavRef::TerminalNavaid { identifier, .. } => identifier.clone(),
        NavRef::Fix(code) => code.clone(),
        NavRef::LatLon(position) => format!("latlon:{:.4},{:.4}", position.lat, position.lon),
        NavRef::Spot(position) => format!("spot:{:.4},{:.4}", position.lat, position.lon),
    }
}

fn record_magnetic_variation_deg(record: &ProcedureLegMaterializationRecord) -> Option<f64> {
    record
        .nav_magnetic_variation_deg
        .or(record.defining_nav_magnetic_variation_deg)
        .or(record.airport_magnetic_variation_deg)
}

fn magnetic_heading_degrees(true_course_deg: f64, magnetic_variation_deg: Option<f64>) -> f64 {
    normalize_bearing_degrees(true_course_deg - magnetic_variation_deg.unwrap_or(0.0))
}

fn display_element_kind(element: &LegDisplayElement) -> DisplayElementKind {
    match element {
        LegDisplayElement::Segment { .. } => DisplayElementKind::Segment,
        LegDisplayElement::Arc { .. } => DisplayElementKind::Arc,
    }
}

fn should_skip_reconciliation_anchor_leg(
    previous_display_path: Option<&LegDisplayPath>,
    previous_leg_to: Option<&NavRef>,
    current_from_record: &ProcedureLegMaterializationRecord,
    current_from: &NavRef,
    current_to: &NavRef,
) -> bool {
    let Some(previous_leg_to) = previous_leg_to else {
        return false;
    };
    if previous_leg_to != current_to {
        return false;
    }
    if current_from == current_to {
        return false;
    }
    reentry_terminal_state(previous_display_path, previous_leg_to).is_some_and(|terminal_state| {
        reentry_candidate_skips(
            terminal_state,
            current_from_record,
            current_from,
            current_to,
        )
    })
}

fn reconciliation_resume_skip_through_index(
    previous_display_path: Option<&LegDisplayPath>,
    previous_leg_to: Option<&NavRef>,
    segment_records: &[ProcedureLegMaterializationRecord],
    fix_records: &[&ProcedureLegMaterializationRecord],
) -> Option<usize> {
    let previous_display_path = previous_display_path?;
    let previous_leg_to = previous_leg_to?;
    let max_reentry_sequence = common_resume_max_sequence(segment_records);
    let terminal_state = reentry_terminal_state(Some(previous_display_path), previous_leg_to)?;
    let reentry_index = fix_records
        .windows(2)
        .enumerate()
        .find_map(|(index, pair)| {
            if pair[1].sequence >= max_reentry_sequence {
                return None;
            }
            let current_to = pair[1].nav_ref.as_ref()?;
            if current_to != previous_leg_to {
                return None;
            }
            if pair[1].path_termination.trim() == "DF" {
                return None;
            }
            let current_from = pair[0].nav_ref.as_ref()?;
            reentry_candidate_skips(terminal_state.clone(), pair[0], current_from, current_to)
                .then_some(index)
        })?;
    Some(reentry_index)
}

fn reentry_terminal_state(
    previous_display_path: Option<&LegDisplayPath>,
    previous_leg_to: &NavRef,
) -> Option<TerminalState> {
    terminal_state_for_handoff(
        previous_display_path.and_then(previous_display_path_terminal_position),
        previous_display_path.and_then(final_course_of_display_path),
        Some(previous_leg_to.clone()),
        false,
    )
}

fn reentry_candidate_skips(
    terminal_state: TerminalState,
    from_record: &ProcedureLegMaterializationRecord,
    from_anchor: &NavRef,
    to_anchor: &NavRef,
) -> bool {
    start_requirement_for_reentry_to_anchor(from_record, from_anchor, to_anchor).is_some_and(
        |start_requirement| {
            matches!(
                reconcile_handoff(&terminal_state, &start_requirement),
                HandoffDecision::SkipStaleFix
            )
        },
    )
}

fn final_course_of_display_path(path: &LegDisplayPath) -> Option<f64> {
    if let Some(course_deg) = path.effective_terminal_course_deg {
        return Some(course_deg);
    }
    match path.elements.last()? {
        LegDisplayElement::Segment { start, end } => Some(bearing_degrees(*start, *end)),
        LegDisplayElement::Arc {
            center,
            end,
            clockwise,
            ..
        } => {
            let radial_deg = bearing_degrees(*center, *end);
            Some(normalize_bearing_degrees(if *clockwise {
                radial_deg + 90.0
            } else {
                radial_deg - 90.0
            }))
        }
    }
}

fn previous_display_path_terminal_position(path: &LegDisplayPath) -> Option<LatLon> {
    match path.elements.last()? {
        LegDisplayElement::Segment { end, .. } => Some(*end),
        LegDisplayElement::Arc { end, .. } => Some(*end),
    }
}

fn drawn_final_course_of_display_path(path: &LegDisplayPath) -> Option<f64> {
    match path.elements.last()? {
        LegDisplayElement::Segment { start, end } => Some(bearing_degrees(*start, *end)),
        LegDisplayElement::Arc {
            center,
            end,
            clockwise,
            ..
        } => {
            let radial_deg = bearing_degrees(*center, *end);
            Some(normalize_bearing_degrees(if *clockwise {
                radial_deg + 90.0
            } else {
                radial_deg - 90.0
            }))
        }
    }
}

#[cfg(test)]
fn terminal_state_for_resolved_leg(leg: &ResolvedLeg) -> Option<TerminalState> {
    let provenance = leg.procedure_provenance.as_ref()?;
    let path = provenance.display_path.as_ref()?;
    let terminal_position = previous_display_path_terminal_position(path)?;
    let drawn_terminal_course_deg = drawn_final_course_of_display_path(path);
    let logical_terminal_course_deg = final_course_of_display_path(path);
    Some(terminal_state_with_leg_characteristics(
        terminal_position,
        drawn_terminal_course_deg,
        logical_terminal_course_deg,
        Some(leg.to.clone()),
        provenance.role.clone(),
        &provenance.path_termination,
    ))
}

#[cfg(test)]
fn start_requirement_for_resolved_leg(leg: &ResolvedLeg) -> Option<StartRequirement> {
    let provenance = leg.procedure_provenance.as_ref()?;
    let anchor_position = Some(terminal_position_for_nav_ref(
        provenance.display_path.as_ref(),
    )?);
    let terminal_course_deg = provenance
        .display_path
        .as_ref()
        .and_then(final_course_of_display_path);
    Some(start_requirement_from_leg_characteristics(
        &provenance.path_termination,
        leg.to.clone(),
        anchor_position,
        terminal_course_deg,
    ))
}

#[cfg(test)]
fn terminal_position_for_nav_ref(display_path: Option<&LegDisplayPath>) -> Option<LatLon> {
    display_path.and_then(previous_display_path_terminal_position)
}

fn terminal_state_for_handoff(
    current_position: Option<LatLon>,
    current_course_deg: Option<f64>,
    terminal_anchor: Option<NavRef>,
    common_segment: bool,
) -> Option<TerminalState> {
    Some(basic_terminal_state(
        current_position?,
        current_course_deg,
        terminal_anchor,
        common_segment,
    ))
}

fn start_requirement_for_direct_to_fix_with_following_course(
    direct_to_fix_record: &ProcedureLegMaterializationRecord,
    following_course_record: &ProcedureLegMaterializationRecord,
) -> Option<StartRequirement> {
    Some(direct_to_fix_with_course_continuation_requirement(
        direct_to_fix_record.nav_ref.clone()?,
        direct_to_fix_record.nav_position,
        following_course_record.magnetic_course_deg.map(|course| {
            course + record_magnetic_variation_deg(following_course_record).unwrap_or(0.0)
        }),
        following_course_record.nav_ref.clone(),
        following_course_record.nav_position,
    ))
}

fn start_requirement_for_feeder_course_to_fix_with_common_resume(
    feeder_course_to_fix_record: &ProcedureLegMaterializationRecord,
    resumed_common_record: &ProcedureLegMaterializationRecord,
) -> Option<StartRequirement> {
    Some(yieldable_course_to_fix_requirement(
        feeder_course_to_fix_record.nav_ref.clone()?,
        feeder_course_to_fix_record.nav_position,
        resumed_common_record.magnetic_course_deg.map(|course| {
            course + record_magnetic_variation_deg(resumed_common_record).unwrap_or(0.0)
        }),
        resumed_common_record.nav_ref.clone(),
        resumed_common_record.nav_position,
    ))
}

#[cfg(test)]
fn local_to_en(origin: LatLon, point: LatLon) -> (f64, f64) {
    let lat_scale_nm = 60.0;
    let mean_lat_rad = ((origin.lat + point.lat) * 0.5).to_radians();
    let lon_scale_nm = 60.0 * mean_lat_rad.cos();
    (
        (point.lon - origin.lon) * lon_scale_nm,
        (point.lat - origin.lat) * lat_scale_nm,
    )
}

#[cfg(test)]
fn course_unit_vector(course_deg: f64) -> (f64, f64) {
    let radians = course_deg.to_radians();
    (radians.sin(), radians.cos())
}

fn start_requirement_for_reentry_to_anchor(
    from_record: &ProcedureLegMaterializationRecord,
    from_anchor: &NavRef,
    to_anchor: &NavRef,
) -> Option<StartRequirement> {
    Some(reentry_to_anchor_requirement(
        from_anchor.clone(),
        Some(from_record.nav_position?),
        to_anchor.clone(),
    ))
}

fn common_resume_yields_current_feeder_cf(
    pair: [&ProcedureLegMaterializationRecord; 2],
    leg_records: &[ProcedureLegMaterializationRecord],
    previous_display_path: Option<&LegDisplayPath>,
    previous: PreviousWindowContext,
    next_segment_records: Option<&[ProcedureLegMaterializationRecord]>,
    role: ProcedureSegmentRole,
) -> bool {
    role != ProcedureSegmentRole::Common
        && pair[1].path_termination.trim() == "CF"
        && next_segment_records.is_some_and(|next_records| {
            let projection =
                resume_projection_context(pair, leg_records, previous_display_path, previous);
            resumed_common_target_supersedes_feeder_cf(
                projection.display_path.as_ref(),
                projection.terminal_position,
                projection.terminal_course,
                projection.terminal_anchor,
                pair[1],
                next_records,
            )
        })
}

fn resumed_common_target_supersedes_feeder_cf(
    previous_display_path: Option<&LegDisplayPath>,
    previous_terminal_position: Option<LatLon>,
    previous_terminal_course: Option<f64>,
    previous_terminal_anchor: Option<NavRef>,
    feeder_course_to_fix_record: &ProcedureLegMaterializationRecord,
    next_segment_records: &[ProcedureLegMaterializationRecord],
) -> bool {
    if same_fix_feeder_cf_is_common_if_course(
        previous_terminal_position,
        previous_terminal_course,
        previous_terminal_anchor.as_ref(),
        feeder_course_to_fix_record,
        next_segment_records,
    ) {
        return true;
    }
    resumed_common_target(previous_display_path, false, next_segment_records).is_some_and(
        |resumed_common_target| {
            resumed_common_target.record.nav_ref.as_ref()
                != feeder_course_to_fix_record.nav_ref.as_ref()
                && should_yield_feeder_course_to_fix_to_resumed_common_segment(
                    previous_terminal_position,
                    previous_terminal_course,
                    previous_terminal_anchor,
                    feeder_course_to_fix_record,
                    resumed_common_target.record,
                )
        },
    )
}

fn same_fix_feeder_cf_is_common_if_course(
    previous_terminal_position: Option<LatLon>,
    previous_terminal_course: Option<f64>,
    previous_terminal_anchor: Option<&NavRef>,
    feeder_course_to_fix_record: &ProcedureLegMaterializationRecord,
    next_segment_records: &[ProcedureLegMaterializationRecord],
) -> bool {
    if feeder_course_to_fix_record.path_termination.trim() != "CF" {
        return false;
    }
    if previous_terminal_anchor != feeder_course_to_fix_record.nav_ref.as_ref() {
        return false;
    }
    let Some(feeder_fix) = feeder_course_to_fix_record.nav_position else {
        return false;
    };
    if !previous_terminal_position
        .is_some_and(|position| great_circle_distance_nm(position, feeder_fix) <= 0.05)
    {
        return false;
    }
    let Some(feeder_course_deg) = feeder_course_to_fix_record
        .magnetic_course_deg
        .map(|course| {
            course + record_magnetic_variation_deg(feeder_course_to_fix_record).unwrap_or(0.0)
        })
    else {
        return false;
    };
    if !previous_terminal_course
        .is_some_and(|course| angular_difference_degrees(course, feeder_course_deg) <= 10.0)
    {
        return false;
    }
    let Some(common_if) = next_segment_records
        .iter()
        .find(|record| record.path_termination.trim() == "IF")
    else {
        return false;
    };
    if !same_materialized_fix(common_if, feeder_course_to_fix_record) {
        return false;
    }
    let Some(first_common_cf) = next_segment_records
        .iter()
        .filter(|record| record.sequence > common_if.sequence)
        .find(|record| record.path_termination.trim() == "CF")
    else {
        return false;
    };
    let Some(common_course_deg) = first_common_cf
        .magnetic_course_deg
        .map(|course| course + record_magnetic_variation_deg(first_common_cf).unwrap_or(0.0))
    else {
        return false;
    };
    angular_difference_degrees(feeder_course_deg, common_course_deg) <= 10.0
}

fn same_fix_df_between_fa_and_hold(
    fix_records: &[&ProcedureLegMaterializationRecord],
    current_window_index: usize,
) -> bool {
    let (Some(from), Some(direct_to)) = (
        fix_records.get(current_window_index),
        fix_records.get(current_window_index + 1),
    ) else {
        return false;
    };
    if from.path_termination.trim() != "FA" || direct_to.path_termination.trim() != "DF" {
        return false;
    }
    if !same_materialized_fix(from, direct_to) {
        return false;
    }
    fix_records
        .get(current_window_index + 2)
        .is_some_and(|hold| {
            matches!(hold.path_termination.trim(), "HA" | "HF" | "HM")
                && same_materialized_fix(direct_to, hold)
        })
}

fn same_fix_hold_after_redundant_df(
    fix_records: &[&ProcedureLegMaterializationRecord],
    current_window_index: usize,
) -> bool {
    let (Some(prior), Some(direct_to), Some(hold)) = (
        current_window_index
            .checked_sub(1)
            .and_then(|index| fix_records.get(index)),
        fix_records.get(current_window_index),
        fix_records.get(current_window_index + 1),
    ) else {
        return false;
    };
    prior.path_termination.trim() == "FA"
        && direct_to.path_termination.trim() == "DF"
        && matches!(hold.path_termination.trim(), "HA" | "HF" | "HM")
        && same_materialized_fix(prior, direct_to)
        && same_materialized_fix(direct_to, hold)
}

fn same_fix_cf_after_pi_course_reversal(
    fix_records: &[&ProcedureLegMaterializationRecord],
    current_window_index: usize,
) -> bool {
    let (Some(procedure_turn), Some(course_to_fix)) = (
        fix_records.get(current_window_index),
        fix_records.get(current_window_index + 1),
    ) else {
        return false;
    };
    procedure_turn.path_termination.trim() == "PI"
        && course_to_fix.path_termination.trim() == "CF"
        && same_materialized_fix(procedure_turn, course_to_fix)
        && procedure_turn.defining_nav_ref == course_to_fix.defining_nav_ref
        && procedure_turn.defining_nav_position == course_to_fix.defining_nav_position
}

fn same_fix_fa_cf_course_reversal(
    fix_to_altitude: &ProcedureLegMaterializationRecord,
    course_to_fix: &ProcedureLegMaterializationRecord,
) -> bool {
    if fix_to_altitude.path_termination.trim() != "FA"
        || course_to_fix.path_termination.trim() != "CF"
        || !same_materialized_fix(fix_to_altitude, course_to_fix)
        || !matches!(
            course_to_fix.turn_direction.as_deref().map(str::trim),
            Some("L" | "R")
        )
    {
        return false;
    }
    let Some(outbound_course_deg) = fix_to_altitude
        .magnetic_course_deg
        .map(|course| course + record_magnetic_variation_deg(fix_to_altitude).unwrap_or(0.0))
    else {
        return false;
    };
    let Some(inbound_course_deg) = course_to_fix
        .magnetic_course_deg
        .map(|course| course + record_magnetic_variation_deg(course_to_fix).unwrap_or(0.0))
    else {
        return false;
    };
    angular_difference_degrees(outbound_course_deg, inbound_course_deg) >= 150.0
}

fn row_returns_to_following_segment_if(
    row: &ProcedureLegMaterializationRecord,
    next_segment_records: Option<&[ProcedureLegMaterializationRecord]>,
) -> bool {
    let Some(first_if) = next_segment_records.and_then(|records| {
        records
            .iter()
            .filter(|candidate| candidate.path_termination.trim() == "IF")
            .min_by_key(|candidate| candidate.sequence)
    }) else {
        return false;
    };
    row.nav_ref.is_some() && row.nav_ref == first_if.nav_ref
}

fn should_skip_degenerate_or_duplicate_window(
    from: &NavRef,
    to: &NavRef,
    path_termination: &str,
    resolved_last: Option<&ResolvedLeg>,
) -> bool {
    if from == to && matches!(path_termination, "HA" | "HF" | "HM" | "FC" | "TF") {
        return true;
    }
    resolved_last.is_some_and(|previous| previous.from == *from && previous.to == *to)
}

#[derive(Clone, Copy)]
struct PreviousWindowContext {
    terminal_position: Option<LatLon>,
    terminal_course: Option<f64>,
    previous_was_course_to_intercept: bool,
    previous_ended_with_hold_entry_geometry: bool,
    previous_leg_consumed_same_pi: bool,
    previous_leg_consumed_same_hold: bool,
}

struct ResumeProjectionContext {
    display_path: Option<LegDisplayPath>,
    terminal_position: Option<LatLon>,
    terminal_course: Option<f64>,
    terminal_anchor: Option<NavRef>,
}

#[derive(Clone, Copy)]
struct PreviousDisplayPathState {
    terminal_position: Option<LatLon>,
    terminal_course: Option<f64>,
}

fn previous_window_context(
    previous_display_path: Option<&LegDisplayPath>,
    resolved_last: Option<&ResolvedLeg>,
    current_pair_start: &ProcedureLegMaterializationRecord,
) -> PreviousWindowContext {
    PreviousWindowContext {
        terminal_position: previous_display_path.and_then(previous_display_path_terminal_position),
        terminal_course: previous_display_path.and_then(final_course_of_display_path),
        previous_was_course_to_intercept: resolved_last.is_some_and(|previous| {
            previous
                .procedure_provenance
                .as_ref()
                .is_some_and(|provenance| {
                    matches!(
                        &provenance.path_termination,
                        PathTermination::Other(label) if label.trim() == "CI"
                    )
                })
        }),
        // Raw HA/HF/HM provenance is collapsed to HeadingToManual in resolved legs, so use the
        // tagged display provenance to recognize a borrowed hold-entry exit like 4B8 R20/HFD.
        previous_ended_with_hold_entry_geometry: previous_display_path.is_some_and(|path| {
            path.debug_element_sources
                .last()
                .is_some_and(|source| source.starts_with("hold_entry@"))
        }),
        previous_leg_consumed_same_pi: resolved_last.is_some_and(|previous| {
            previous
                .procedure_provenance
                .as_ref()
                .is_some_and(|provenance| {
                    provenance.leg_sequence == current_pair_start.sequence
                        && matches!(
                            &provenance.path_termination,
                            PathTermination::Other(label) if label.trim() == "PI"
                        )
                })
        }),
        previous_leg_consumed_same_hold: resolved_last.is_some_and(|previous| {
            previous
                .procedure_provenance
                .as_ref()
                .is_some_and(|provenance| {
                    provenance.leg_sequence == current_pair_start.sequence
                        && matches!(
                            provenance.path_termination,
                            PathTermination::HeadingToManual
                        )
                        && matches!(
                            current_pair_start.path_termination.trim(),
                            "HA" | "HF" | "HM"
                        )
                })
        }),
    }
}

fn previous_display_path_state(
    previous_display_path: Option<&LegDisplayPath>,
) -> PreviousDisplayPathState {
    PreviousDisplayPathState {
        terminal_position: previous_display_path.and_then(previous_display_path_terminal_position),
        terminal_course: previous_display_path.and_then(final_course_of_display_path),
    }
}

fn tail_planning_state(
    last_fix: &ProcedureLegMaterializationRecord,
    trailing_record: &ProcedureLegMaterializationRecord,
    planning: TailPlanningContext<'_>,
) -> TailPlanningState {
    let previous_path_state = previous_display_path_state(planning.previous_display_path);
    let common_resume_skips_trailing_cf = last_fix.path_termination.trim() != "PI"
        && trailing_record.path_termination.trim() == "CF"
        && planning.next_segment_records.is_some_and(|next_records| {
            resumed_common_target_supersedes_feeder_cf(
                planning.previous_display_path,
                previous_path_state.terminal_position,
                previous_path_state.terminal_course,
                planning.previous_leg_to.cloned(),
                trailing_record,
                next_records,
            )
        });
    TailPlanningState {
        previous_path_state,
        common_resume_skips_trailing_cf,
    }
}

fn resume_projection_context(
    pair: [&ProcedureLegMaterializationRecord; 2],
    leg_records: &[ProcedureLegMaterializationRecord],
    previous_display_path: Option<&LegDisplayPath>,
    previous: PreviousWindowContext,
) -> ResumeProjectionContext {
    let display_path = if pair[0].path_termination.trim() == "PI" {
        let inferred_start = record_with_inferred_anchor_position(pair[0], leg_records, None);
        let enriched_leg_records = leg_records_with_replaced_record(leg_records, &inferred_start);
        display_path_for_single_procedure_step(
            &enriched_leg_records,
            &inferred_start,
            previous.terminal_position,
            previous.terminal_course,
        )
    } else {
        previous_display_path.cloned()
    };
    let terminal_position = display_path
        .as_ref()
        .and_then(previous_display_path_terminal_position);
    let terminal_course = display_path.as_ref().and_then(final_course_of_display_path);
    let terminal_anchor = display_path.as_ref().and_then(|_| pair[0].nav_ref.clone());
    ResumeProjectionContext {
        display_path,
        terminal_position,
        terminal_course,
        terminal_anchor,
    }
}

struct ProcedureWindowLink<'a> {
    from: NavRef,
    to: NavRef,
    effective_leg_end: &'a ProcedureLegMaterializationRecord,
    hold_record: Option<&'a ProcedureLegMaterializationRecord>,
    provenance_record: &'a ProcedureLegMaterializationRecord,
    inherit_previous_state: bool,
    display_leg_start: &'a ProcedureLegMaterializationRecord,
    render_as_empty_join: bool,
    render_as_resumed_common_cf: bool,
    render_inherited_single_tf_step: bool,
    suppressed_record: Option<&'a ProcedureLegMaterializationRecord>,
}

#[derive(Clone)]
struct ProcedureWindowPlanningContext<'a> {
    fix_records: &'a [&'a ProcedureLegMaterializationRecord],
    leg_records: &'a [ProcedureLegMaterializationRecord],
    role: ProcedureSegmentRole,
    common_resume_target: Option<CommonResumeTarget<'a>>,
    previous_display_path: Option<&'a LegDisplayPath>,
    previous_leg_to: Option<&'a NavRef>,
    next_segment_records: Option<&'a [ProcedureLegMaterializationRecord]>,
    resolved_last: Option<ResolvedLeg>,
}

struct ProcedureTailLink<'a> {
    nav_ref: NavRef,
    provenance_record: &'a ProcedureLegMaterializationRecord,
    display_path: Option<LegDisplayPath>,
}

#[derive(Clone, Copy)]
struct TailPlanningContext<'a> {
    leg_records: &'a [ProcedureLegMaterializationRecord],
    previous_display_path: Option<&'a LegDisplayPath>,
    previous_leg_to: Option<&'a NavRef>,
    next_segment_records: Option<&'a [ProcedureLegMaterializationRecord]>,
}

#[derive(Clone, Copy)]
struct TailPlanningState {
    previous_path_state: PreviousDisplayPathState,
    common_resume_skips_trailing_cf: bool,
}

fn leg_records_with_replaced_record(
    leg_records: &[ProcedureLegMaterializationRecord],
    replacement: &ProcedureLegMaterializationRecord,
) -> Vec<ProcedureLegMaterializationRecord> {
    leg_records
        .iter()
        .map(|record| {
            if record.sequence == replacement.sequence {
                replacement.clone()
            } else {
                record.clone()
            }
        })
        .collect()
}

fn record_with_inferred_anchor_position(
    record: &ProcedureLegMaterializationRecord,
    leg_records: &[ProcedureLegMaterializationRecord],
    next_segment_records: Option<&[ProcedureLegMaterializationRecord]>,
) -> ProcedureLegMaterializationRecord {
    if record.nav_position.is_some() && record.defining_nav_position.is_some() {
        return record.clone();
    }

    let mut inferred = record.clone();
    let sources = leg_records.iter().chain(
        next_segment_records
            .into_iter()
            .flat_map(|records| records.iter()),
    );

    if inferred.nav_position.is_none() {
        inferred.nav_position = sources
            .clone()
            .find(|candidate| {
                candidate.nav_ref == record.nav_ref && candidate.nav_position.is_some()
            })
            .and_then(|candidate| candidate.nav_position);
    }
    if inferred.defining_nav_position.is_none() {
        inferred.defining_nav_position = leg_records
            .iter()
            .chain(
                next_segment_records
                    .into_iter()
                    .flat_map(|records| records.iter()),
            )
            .find(|candidate| {
                candidate.defining_nav_ref == record.defining_nav_ref
                    && candidate.defining_nav_position.is_some()
            })
            .and_then(|candidate| candidate.defining_nav_position);
    }
    inferred
}

fn standalone_pi_record_with_following_if_anchor(
    standalone: &ProcedureLegMaterializationRecord,
    planning: TailPlanningContext<'_>,
) -> ProcedureLegMaterializationRecord {
    let inferred = record_with_inferred_anchor_position(
        standalone,
        planning.leg_records,
        planning.next_segment_records,
    );
    if standalone.path_termination.trim() != "PI" {
        return inferred;
    }
    let Some(next_if) = planning.next_segment_records.and_then(|records| {
        records
            .iter()
            .filter(|record| record.path_termination.trim() == "IF")
            .min_by_key(|record| record.sequence)
    }) else {
        return inferred;
    };
    let following_course_defining_nav_ref = planning.next_segment_records.and_then(|records| {
        records
            .iter()
            .filter(|record| record.sequence > next_if.sequence)
            .filter(|record| matches!(record.path_termination.trim(), "CF" | "FA" | "FC"))
            .find_map(|record| record.defining_nav_ref.clone())
    });
    let following_defining_nav_ref = next_if
        .defining_nav_ref
        .clone()
        .or(following_course_defining_nav_ref);
    if inferred.nav_ref != inferred.defining_nav_ref
        || following_defining_nav_ref != inferred.defining_nav_ref
        || next_if.nav_position.is_none()
    {
        return inferred;
    }
    if inferred
        .nav_position
        .zip(next_if.nav_position)
        .is_none_or(|(from, to)| great_circle_distance_nm(from, to) <= 0.25)
    {
        return inferred;
    }

    // KCHS D21/CHS publishes a standalone PI at the controlling VORTAC, while the
    // following common IF is the actual outbound course-reversal anchor. Attach the
    // PI shape there so the CF leg does not manufacture the outbound leg and then
    // fail heading continuity at the IF.
    let mut anchored = inferred;
    anchored.nav_ref = next_if.nav_ref.clone();
    anchored.nav_position = next_if.nav_position;
    anchored.nav_magnetic_variation_deg = next_if.nav_magnetic_variation_deg;
    anchored
}

struct ProcedureAppendSpec<'a> {
    from: NavRef,
    to: NavRef,
    heading_from_record: &'a ProcedureLegMaterializationRecord,
    heading_to_record: &'a ProcedureLegMaterializationRecord,
    provenance_record: &'a ProcedureLegMaterializationRecord,
    display_path: Option<LegDisplayPath>,
}

fn append_spec_for_window_link<'a>(
    pair_start: &'a ProcedureLegMaterializationRecord,
    window_link: ProcedureWindowLink<'a>,
    display_path: Option<LegDisplayPath>,
) -> ProcedureAppendSpec<'a> {
    ProcedureAppendSpec {
        from: window_link.from,
        to: window_link.to,
        heading_from_record: pair_start,
        heading_to_record: window_link.effective_leg_end,
        provenance_record: window_link.provenance_record,
        display_path,
    }
}

fn append_spec_for_tail_link<'a>(
    anchor_record: &'a ProcedureLegMaterializationRecord,
    tail_link: ProcedureTailLink<'a>,
) -> ProcedureAppendSpec<'a> {
    ProcedureAppendSpec {
        from: tail_link.nav_ref.clone(),
        to: tail_link.nav_ref,
        heading_from_record: anchor_record,
        heading_to_record: anchor_record,
        provenance_record: tail_link.provenance_record,
        display_path: tail_link.display_path,
    }
}

fn resolve_procedure_window<'a>(
    current_window_index: usize,
    pair: [&'a ProcedureLegMaterializationRecord; 2],
    fix_records: &[&'a ProcedureLegMaterializationRecord],
    previous: PreviousWindowContext,
    leg_records: &[ProcedureLegMaterializationRecord],
    role: ProcedureSegmentRole,
) -> (
    &'a ProcedureLegMaterializationRecord,
    Option<&'a ProcedureLegMaterializationRecord>,
    &'a ProcedureLegMaterializationRecord,
) {
    let df_following_cf_record = if pair[1].path_termination.trim() == "DF" {
        fix_records
            .get(current_window_index + 2)
            .copied()
            .filter(|record| {
                record.path_termination.trim() == "CF"
                    && should_yield_direct_to_fix_to_following_course(
                        previous.terminal_position,
                        previous.terminal_course,
                        pair[0],
                        pair[1],
                        leg_records,
                        record,
                        role == ProcedureSegmentRole::Common,
                    )
            })
    } else {
        None
    };
    let fa_following_cf_record = if role == ProcedureSegmentRole::RunwayTransition
        && pair[1].path_termination.trim() == "FA"
    {
        fix_records
            .get(current_window_index + 2)
            .copied()
            .filter(|record| same_fix_fa_cf_course_reversal(pair[1], record))
    } else {
        None
    };
    let effective_leg_end = df_following_cf_record
        .or(fa_following_cf_record)
        .unwrap_or(pair[1]);
    let hold_record = if matches!(
        effective_leg_end.path_termination.trim(),
        "HA" | "HF" | "HM"
    ) {
        Some(effective_leg_end)
    } else {
        let next_hold_index =
            if df_following_cf_record.is_some() || fa_following_cf_record.is_some() {
                current_window_index + 3
            } else {
                current_window_index + 2
            };
        fix_records.get(next_hold_index).and_then(|next| {
            if matches!(next.path_termination.trim(), "HA" | "HF" | "HM")
                && next.nav_ref == effective_leg_end.nav_ref
            {
                Some(*next)
            } else {
                None
            }
        })
    };
    let provenance_record = if pair[0].path_termination.trim() == "PI" {
        // A PI-started window may carry the following CF geometry, but the emitted
        // leg still needs to credit the required procedure-turn row itself.
        pair[0]
    } else {
        hold_record.unwrap_or(effective_leg_end)
    };
    (effective_leg_end, hold_record, provenance_record)
}

struct ProcedureWindowContinuationPolicy {
    continuing_if_to_cf_join: bool,
    continuing_same_anchor_window: bool,
    continuing_from_fa_window: bool,
    continuing_from_previous_anchor: bool,
    continuing_from_previous_course: bool,
    continuing_from_consumed_hold: bool,
    resume_common_cf_from_previous_path: bool,
}

struct ProcedureWindowLinkInput<'records, 'context> {
    current_window_index: usize,
    from: &'context NavRef,
    to: &'context NavRef,
    pair: [&'records ProcedureLegMaterializationRecord; 2],
    hold_record: Option<&'records ProcedureLegMaterializationRecord>,
    role: ProcedureSegmentRole,
    traversal_policy: SegmentTraversalPolicy<'records>,
    previous: PreviousWindowContext,
    previous_leg_to: Option<&'context NavRef>,
}

impl ProcedureWindowContinuationPolicy {
    fn evaluate(input: &ProcedureWindowLinkInput<'_, '_>) -> Self {
        let ProcedureWindowLinkInput {
            current_window_index,
            from,
            to,
            pair,
            hold_record,
            role,
            traversal_policy,
            previous,
            previous_leg_to,
        } = input;
        let continuing_if_to_cf_join = (from != to)
            && pair[0].path_termination.trim() == "IF"
            && pair[1].path_termination.trim() == "CF"
            && previous.terminal_position.is_some_and(|previous_end| {
                let Some(anchor_position) = pair[0].nav_position else {
                    return false;
                };
                previous.previous_was_course_to_intercept
                    || great_circle_distance_nm(previous_end, anchor_position) > 0.25
            });
        let continuing_same_anchor_window = (from != to)
            && hold_record.is_some()
            && pair[0].path_termination.trim() == "CF"
            && pair[1].path_termination.trim() == "TF"
            && previous
                .terminal_position
                .zip(pair[0].nav_position)
                .is_some_and(|(previous_end, anchor_position)| {
                    great_circle_distance_nm(previous_end, anchor_position) <= 0.05
                });
        let continuing_from_fa_window = (from != to)
            && pair[0].path_termination.trim() == "FA"
            && previous_leg_to.is_some_and(|previous_to| previous_to == *from)
            && previous.terminal_position.is_some();
        let continuing_from_previous_anchor = previous
            .terminal_position
            .zip(pair[0].nav_position)
            .is_some_and(|(previous_end, anchor_position)| {
                great_circle_distance_nm(previous_end, anchor_position) <= 0.05
            });
        let continuing_from_previous_course = previous.previous_ended_with_hold_entry_geometry
            && *role == ProcedureSegmentRole::Common
            && previous_terminal_lies_on_window_course(*previous, *pair);
        let continuing_from_consumed_hold = previous.previous_leg_consumed_same_hold
            && matches!(pair[0].path_termination.trim(), "HA" | "HF" | "HM");
        let resume_common_cf_from_previous_path = *role == ProcedureSegmentRole::Common
            && traversal_policy.resumes_common_on_window(*current_window_index);
        Self {
            continuing_if_to_cf_join,
            continuing_same_anchor_window,
            continuing_from_fa_window,
            continuing_from_previous_anchor,
            continuing_from_previous_course,
            continuing_from_consumed_hold,
            resume_common_cf_from_previous_path,
        }
    }

    fn inherits_previous_state(&self, from: &NavRef, to: &NavRef) -> bool {
        from == to
            || self.continuing_if_to_cf_join
            || self.continuing_same_anchor_window
            || self.continuing_from_fa_window
            || self.continuing_from_previous_anchor
            || self.continuing_from_previous_course
            || self.continuing_from_consumed_hold
            || self.resume_common_cf_from_previous_path
    }
}

fn previous_terminal_lies_on_window_course(
    previous: PreviousWindowContext,
    pair: [&ProcedureLegMaterializationRecord; 2],
) -> bool {
    let (Some(previous_position), Some(previous_course), Some(start), Some(end)) = (
        previous.terminal_position,
        previous.terminal_course,
        pair[0].nav_position,
        pair[1].nav_position,
    ) else {
        return false;
    };
    let course = initial_course_deg(start, end);
    if angular_difference_degrees(previous_course, course) > 35.0 {
        return false;
    }
    if cross_track_left_nm(start, end, previous_position).abs() > 0.2 {
        return false;
    }
    let start_to_end = great_circle_distance_nm(start, end);
    let via_previous = great_circle_distance_nm(start, previous_position)
        + great_circle_distance_nm(previous_position, end);
    via_previous <= start_to_end + 0.2
}

struct ProcedureWindowLinkBehavior<'a> {
    display_leg_start: &'a ProcedureLegMaterializationRecord,
    inherit_previous_state: bool,
    render_as_empty_join: bool,
    render_as_resumed_common_cf: bool,
    render_inherited_single_tf_step: bool,
}

fn determine_procedure_window_link<'a>(
    input: ProcedureWindowLinkInput<'a, '_>,
) -> ProcedureWindowLinkBehavior<'a> {
    let policy = ProcedureWindowContinuationPolicy::evaluate(&input);
    let ProcedureWindowLinkInput {
        from,
        to,
        pair,
        previous,
        ..
    } = input;
    let starts_at_second_record = (pair[0].path_termination.trim() == "PI"
        && from != to
        && previous.previous_leg_consumed_same_pi)
        || (pair[0].path_termination.trim() == "RF" && policy.continuing_from_previous_anchor)
        || policy.resume_common_cf_from_previous_path
        || policy.continuing_from_previous_course
        || policy.continuing_from_consumed_hold;
    let display_leg_start = if starts_at_second_record {
        pair[1]
    } else {
        pair[0]
    };
    let terminal_at_window_end = previous
        .terminal_position
        .zip(pair[1].nav_position)
        .is_some_and(|(start, end)| great_circle_distance_nm(start, end) <= 0.05);
    let same_fix_course_handoff_after_hold_reversal = from == to
        && policy.continuing_from_consumed_hold
        && pair[1].path_termination.trim() == "CF";
    let render_as_empty_join = terminal_at_window_end
        && (policy.continuing_if_to_cf_join || same_fix_course_handoff_after_hold_reversal);
    ProcedureWindowLinkBehavior {
        display_leg_start,
        inherit_previous_state: policy.inherits_previous_state(from, to),
        render_as_empty_join,
        render_as_resumed_common_cf: policy.resume_common_cf_from_previous_path,
        render_inherited_single_tf_step: policy.continuing_from_previous_course
            || policy.continuing_from_consumed_hold
            || (pair[0].path_termination.trim() == "RF"
                && policy.continuing_from_previous_anchor
                && pair[1].path_termination.trim() == "TF"),
    }
}

fn plan_procedure_window<'a>(
    current_window_index: usize,
    pair: [&'a ProcedureLegMaterializationRecord; 2],
    planning: ProcedureWindowPlanningContext<'a>,
) -> AppResult<Option<ProcedureWindowLink<'a>>> {
    let previous_context = previous_window_context(
        planning.previous_display_path,
        planning.resolved_last.as_ref(),
        pair[0],
    );
    if pair[0].path_termination.trim() == "DF"
        && pair[1].path_termination.trim() == "CF"
        && previous_context
            .terminal_position
            .zip(pair[0].nav_position)
            .zip(pair[1].nav_position)
            .is_some_and(|((previous_end, direct_fix), following_fix)| {
                great_circle_distance_nm(previous_end, following_fix) <= 0.05
                    && great_circle_distance_nm(previous_end, direct_fix) > 0.25
            })
    {
        return Ok(None);
    }
    let from = pair[0].nav_ref.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "procedure leg materialization encountered missing from-anchor nav_ref at sequence {}",
            pair[0].sequence
        ),
    })?;
    let common_resume_skips_current_feeder_cf = common_resume_yields_current_feeder_cf(
        pair,
        planning.leg_records,
        planning.previous_display_path,
        previous_context,
        planning.next_segment_records,
        planning.role.clone(),
    );
    let (effective_leg_end, hold_record, provenance_record) =
        if common_resume_skips_current_feeder_cf
            && pair[0].path_termination.trim() == "PI"
            && !previous_context.previous_leg_consumed_same_pi
        {
            (pair[0], None, pair[0])
        } else {
            resolve_procedure_window(
                current_window_index,
                pair,
                planning.fix_records,
                previous_context,
                planning.leg_records,
                planning.role.clone(),
            )
        };
    let to = effective_leg_end.nav_ref.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "procedure leg materialization encountered missing to-anchor nav_ref at sequence {}",
            effective_leg_end.sequence
        ),
    })?;
    if common_resume_skips_current_feeder_cf && effective_leg_end.sequence != pair[0].sequence {
        return Ok(None);
    }
    if should_skip_reconciliation_anchor_leg(
        planning.previous_display_path,
        planning.previous_leg_to,
        pair[0],
        &from,
        &to,
    ) {
        return Ok(None);
    }
    if should_skip_degenerate_or_duplicate_window(
        &from,
        &to,
        pair[1].path_termination.trim(),
        planning.resolved_last.as_ref(),
    ) {
        return Ok(None);
    }
    let behavior = determine_procedure_window_link(ProcedureWindowLinkInput {
        current_window_index,
        from: &from,
        to: &to,
        pair,
        hold_record,
        role: planning.role,
        traversal_policy: SegmentTraversalPolicy {
            common_resume_target: planning.common_resume_target,
            skip_through_index: None,
        },
        previous: previous_context,
        previous_leg_to: planning.previous_leg_to,
    });
    Ok(Some(ProcedureWindowLink {
        from,
        to,
        effective_leg_end,
        hold_record,
        provenance_record,
        inherit_previous_state: behavior.inherit_previous_state,
        display_leg_start: behavior.display_leg_start,
        render_as_empty_join: behavior.render_as_empty_join,
        render_as_resumed_common_cf: behavior.render_as_resumed_common_cf,
        render_inherited_single_tf_step: behavior.render_inherited_single_tf_step,
        suppressed_record: if common_resume_skips_current_feeder_cf {
            Some(pair[1])
        } else {
            None
        },
    }))
}

fn plan_trailing_procedure_window<'a>(
    last_fix: &'a ProcedureLegMaterializationRecord,
    trailing_record: &'a ProcedureLegMaterializationRecord,
    planning: TailPlanningContext<'a>,
) -> AppResult<Option<ProcedureTailLink<'a>>> {
    let tail_state = tail_planning_state(last_fix, trailing_record, planning);
    let nav_ref = last_fix.nav_ref.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "trailing procedure leg materialization encountered missing nav_ref at sequence {}",
            last_fix.sequence
        ),
    })?;
    let initial_position_override = tail_state.previous_path_state.terminal_position;
    let initial_course_override = tail_state.previous_path_state.terminal_course;
    let display_path = if trailing_record.path_termination.trim() == "CI" {
        planning.next_segment_records.and_then(|next_records| {
            build_trailing_course_to_intercept_display_path(
                trailing_record,
                initial_position_override,
                initial_course_override,
                next_records,
            )
        })
    } else if tail_state.common_resume_skips_trailing_cf {
        None
    } else if last_fix.path_termination.trim() == "PI"
        && trailing_record.path_termination.trim() == "CF"
    {
        display_path_for_procedure_leg(
            planning.leg_records,
            trailing_record,
            trailing_record,
            None,
            initial_position_override,
            initial_course_override,
        )
    } else if matches!(trailing_record.path_termination.trim(), "HA" | "HF" | "HM") {
        planning
            .next_segment_records
            .and_then(|next_segment_records| {
                display_path_for_procedure_leg_before_following_segment(
                    planning.leg_records,
                    last_fix,
                    last_fix,
                    None,
                    initial_position_override,
                    initial_course_override,
                    FollowingProcedureSegment {
                        records: next_segment_records,
                        allow_hold_exit_to_following_course: true,
                    },
                )
            })
    } else if let Some(next_segment_records) = planning.next_segment_records {
        display_path_for_terminal_tf_to_following_common_course(
            last_fix,
            initial_position_override,
            initial_course_override,
            next_segment_records,
        )
        .or_else(|| {
            display_path_for_procedure_leg(
                planning.leg_records,
                last_fix,
                last_fix,
                None,
                initial_position_override,
                initial_course_override,
            )
        })
    } else {
        display_path_for_procedure_leg(
            planning.leg_records,
            last_fix,
            last_fix,
            None,
            initial_position_override,
            initial_course_override,
        )
    };
    Ok(Some(ProcedureTailLink {
        nav_ref,
        provenance_record: trailing_record,
        display_path,
    }))
}

fn plan_standalone_pi_window<'a>(
    standalone: &'a ProcedureLegMaterializationRecord,
    planning: TailPlanningContext<'a>,
) -> AppResult<Option<ProcedureTailLink<'a>>> {
    let standalone_with_position =
        standalone_pi_record_with_following_if_anchor(standalone, planning);
    let enriched_leg_records =
        leg_records_with_replaced_record(planning.leg_records, &standalone_with_position);
    let nav_ref = standalone_with_position
        .nav_ref
        .clone()
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "standalone PI leg materialization encountered missing nav_ref at sequence {}",
                standalone.sequence
            ),
        })?;
    let previous_path_state = previous_display_path_state(planning.previous_display_path);
    let display_path = planning
        .next_segment_records
        .and_then(|next_segment_records| {
            display_path_for_procedure_leg_before_following_segment(
                &enriched_leg_records,
                &standalone_with_position,
                &standalone_with_position,
                None,
                previous_path_state.terminal_position,
                previous_path_state.terminal_course,
                FollowingProcedureSegment {
                    records: next_segment_records,
                    allow_hold_exit_to_following_course: false,
                },
            )
        })
        .or_else(|| {
            display_path_for_procedure_leg(
                &enriched_leg_records,
                &standalone_with_position,
                &standalone_with_position,
                None,
                previous_path_state.terminal_position,
                previous_path_state.terminal_course,
            )
        });
    Ok(display_path.map(|display_path| ProcedureTailLink {
        nav_ref,
        provenance_record: standalone,
        display_path: Some(display_path),
    }))
}

struct AppendResolvedProcedureLegInput<'a> {
    resolved: &'a mut Vec<ResolvedLeg>,
    heading_checks: &'a mut Vec<DisplayElementHeadingSignature>,
    next_heading_step_index: &'a mut usize,
    procedure_id: &'a str,
    airport_id: &'a str,
    kind: &'a ProcedureKind,
    role: &'a ProcedureSegmentRole,
    component_index: usize,
    spec: ProcedureAppendSpec<'a>,
}

fn append_resolved_procedure_leg(
    input: AppendResolvedProcedureLegInput<'_>,
) -> Option<LegDisplayPath> {
    let AppendResolvedProcedureLegInput {
        resolved,
        heading_checks,
        next_heading_step_index,
        procedure_id,
        airport_id,
        kind,
        role,
        component_index,
        mut spec,
    } = input;
    let previous_display_path_index = resolved.iter().rposition(|leg| {
        leg.procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
            .is_some_and(|path| !path.elements.is_empty())
    });
    let previous_is_runway_transition = previous_display_path_index.is_some_and(|index| {
        resolved[index]
            .procedure_provenance
            .as_ref()
            .is_some_and(|provenance| provenance.role == ProcedureSegmentRole::RunwayTransition)
    });
    if *kind == ProcedureKind::Sid
        && *role == ProcedureSegmentRole::EnrouteTransition
        && previous_is_runway_transition
        // Conventional SIDs can publish fly-over turns at their controlling
        // navaid. RNAV SIDs (route types 4/6) should remain fly-by geometry;
        // a near reversal there is still a source/materialization failure.
        && spec.provenance_record.key.route_type.trim() == "3"
    {
        let target = spec
            .display_path
            .as_ref()
            .and_then(previous_display_path_terminal_position);
        let previous_path = previous_display_path_index
            .and_then(|index| resolved.get_mut(index))
            .and_then(|leg| leg.procedure_provenance.as_mut())
            .and_then(|provenance| provenance.display_path.as_mut());
        if let (Some(previous_path), Some(target)) = (previous_path, target) {
            let start = previous_display_path_terminal_position(previous_path);
            let inbound_course_deg = drawn_final_course_of_display_path(previous_path);
            if let (Some(start), Some(inbound_course_deg)) = (start, inbound_course_deg) {
                let outbound_course_deg = bearing_degrees(start, target);
                previous_path.effective_terminal_course_deg = Some(inbound_course_deg);
                if let Some(previous_signature) = heading_checks.last_mut() {
                    previous_signature.end_course_deg = inbound_course_deg;
                }
                if angular_difference_degrees(inbound_course_deg, outbound_course_deg) >= 150.0 {
                    spec.display_path = procedure_geometry::charted_flyover_turn_display_path(
                        start,
                        inbound_course_deg,
                        target,
                    );
                }
            }
        }
    }
    validate_display_path_terminal_matches_leg_to(procedure_id, &spec);
    if *kind == ProcedureKind::Sid && spec.provenance_record.turn_direction.as_deref() == Some("E")
    {
        if let Some(previous_signature) = heading_checks.last_mut() {
            previous_signature.discontinuity_after = true;
        }
        if let Some(previous_leg) = resolved.last_mut() {
            if let Some(provenance) = previous_leg.procedure_provenance.as_mut() {
                provenance.discontinuity_after =
                    Some(ProcedureDiscontinuity::Other("COURSE REVERSAL".to_string()));
            }
        }
    }
    let signatures = heading_signatures_for_leg(
        *next_heading_step_index,
        spec.display_path.as_ref(),
        spec.heading_from_record,
        spec.heading_to_record,
        spec.provenance_record.path_termination.trim(),
        role,
        spec.provenance_record.turn_direction.as_deref(),
    );
    *next_heading_step_index += signatures.len();
    heading_checks.extend(signatures);

    resolved.push(ResolvedLeg {
        id: format!(
            "procedure-{}-{}-{}",
            procedure_id.trim(),
            spec.provenance_record.key.route_type.trim(),
            spec.provenance_record.sequence
        ),
        from: spec.from,
        to: spec.to,
        source: ResolvedLegSource::RouteComponent { component_index },
        procedure_provenance: Some(ProcedureLegProvenance {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            kind: kind.clone(),
            role: role.clone(),
            path_termination: spec.provenance_record.path_termination_kind.clone(),
            leg_sequence: spec.provenance_record.sequence,
            discontinuity_after: None,
            display_path: spec.display_path.clone(),
        }),
    });
    spec.display_path
}

fn validate_display_path_terminal_matches_leg_to(
    procedure_id: &str,
    spec: &ProcedureAppendSpec<'_>,
) {
    if spec.from == spec.to {
        return;
    }
    if !display_path_should_end_at_leg_to(spec.provenance_record.path_termination.trim()) {
        return;
    }
    let Some(expected_end) = spec.heading_to_record.nav_position else {
        return;
    };
    let Some(actual_end) = spec
        .display_path
        .as_ref()
        .and_then(previous_display_path_terminal_position)
    else {
        return;
    };
    if great_circle_distance_nm(actual_end, expected_end) > MIN_GEOMETRY_DISTANCE_NM {
        panic!(
            "procedure display path terminal mismatch for {}: {} -> {} id=procedure-{}-{}-{} gap_nm={:.2} expected=({:.6},{:.6}) actual=({:.6},{:.6})",
            procedure_id.trim(),
            describe_nav_ref(&spec.from),
            describe_nav_ref(&spec.to),
            procedure_id.trim(),
            spec.provenance_record.key.route_type.trim(),
            spec.provenance_record.sequence,
            great_circle_distance_nm(actual_end, expected_end),
            expected_end.lat,
            expected_end.lon,
            actual_end.lat,
            actual_end.lon,
        );
    }
}

fn display_path_should_end_at_leg_to(path_termination: &str) -> bool {
    matches!(path_termination, "AF" | "CF" | "DF" | "RF" | "TF")
}

#[derive(Clone, Copy)]
struct CommonResumeTarget<'a> {
    index: usize,
    record: &'a ProcedureLegMaterializationRecord,
}

#[derive(Clone, Copy)]
struct SegmentTraversalPolicy<'a> {
    common_resume_target: Option<CommonResumeTarget<'a>>,
    skip_through_index: Option<usize>,
}

impl<'a> SegmentTraversalPolicy<'a> {
    fn should_skip_window(self, current_window_index: usize) -> bool {
        self.skip_through_index
            .is_some_and(|skip_index| current_window_index <= skip_index)
            || self
                .common_resume_target
                .is_some_and(|target| current_window_index + 1 < target.index)
    }

    fn resumes_common_on_window(self, current_window_index: usize) -> bool {
        self.common_resume_target
            .is_some_and(|target| current_window_index + 1 == target.index)
    }
}

#[derive(Clone, Copy)]
struct CommonResumeCandidate<'a> {
    index: usize,
    record: &'a ProcedureLegMaterializationRecord,
    fix: LatLon,
    course_anchor: LatLon,
    course_deg: f64,
    incoming_course_to_anchor_deg: Option<f64>,
}

fn common_resume_candidate<'a>(
    fix_records: &[&'a ProcedureLegMaterializationRecord],
    index: usize,
    current_position: LatLon,
    current_course_deg: f64,
) -> Option<CommonResumeCandidate<'a>> {
    let record = *fix_records.get(index)?;
    if record.path_termination.trim() != "CF" {
        return None;
    }
    let fix = record.nav_position?;
    let course_anchor = record.defining_nav_position.or(record.nav_position)?;
    let course_deg = record
        .magnetic_course_deg
        .map(|course| course + record_magnetic_variation_deg(record).unwrap_or(0.0))?;
    let incoming_course_to_anchor_deg = fix_records
        .get(index.saturating_sub(1))
        .and_then(|prior_record| {
            let prior_fix = prior_record.nav_position?;
            let prior_course_deg = prior_record.magnetic_course_deg.map(|course| {
                course + record_magnetic_variation_deg(prior_record).unwrap_or(0.0)
            })?;
            (positions_nearly_equal(current_position, prior_fix)
                && positions_nearly_equal(current_position, course_anchor))
            .then_some(prior_course_deg)
        })
        .or(Some(current_course_deg));
    Some(CommonResumeCandidate {
        index,
        record,
        fix,
        course_anchor,
        course_deg,
        incoming_course_to_anchor_deg,
    })
}

fn first_resumable_common_candidate<'a>(
    fix_records: &[&'a ProcedureLegMaterializationRecord],
    current_position: LatLon,
    current_course_deg: f64,
    previous_was_hold_like: bool,
    max_resumable_sequence: i32,
) -> Option<CommonResumeCandidate<'a>> {
    for index in 1..fix_records.len() {
        let Some(candidate) =
            common_resume_candidate(fix_records, index, current_position, current_course_deg)
        else {
            continue;
        };
        if candidate.record.sequence >= max_resumable_sequence {
            break;
        }
        if matches!(
            common_resume_candidate_decision(CommonResumeCandidateInput {
                terminal_position: current_position,
                terminal_course_deg: current_course_deg,
                incoming_course_to_anchor_deg: candidate.incoming_course_to_anchor_deg,
                previous_was_hold_like,
                anchor: candidate.record.nav_ref.clone(),
                course_deg: candidate.course_deg,
                anchor_position: candidate.course_anchor,
                target_anchor: candidate.record.nav_ref.clone(),
                target_anchor_position: candidate.fix,
            }),
            HandoffDecision::ResumeAtAnchor | HandoffDecision::ResumeThroughAnchorKink
        ) {
            return Some(candidate);
        }
    }
    None
}

fn resumed_common_target<'a>(
    previous_display_path: Option<&LegDisplayPath>,
    previous_was_hold_like: bool,
    segment_records: &'a [ProcedureLegMaterializationRecord],
) -> Option<CommonResumeTarget<'a>> {
    let fix_records = segment_records
        .iter()
        .filter(|record| record.nav_ref.is_some())
        .collect::<Vec<_>>();
    let previous_display_path = previous_display_path?;
    let current_position = previous_display_path_terminal_position(previous_display_path)?;
    let current_course_deg = final_course_of_display_path(previous_display_path)?;
    let max_resumable_sequence = common_resume_max_sequence(segment_records);
    first_resumable_common_candidate(
        &fix_records,
        current_position,
        current_course_deg,
        previous_was_hold_like,
        max_resumable_sequence,
    )
    .map(|candidate| CommonResumeTarget {
        index: candidate.index,
        record: candidate.record,
    })
}

fn common_resume_max_sequence(segment_records: &[ProcedureLegMaterializationRecord]) -> i32 {
    let first_nonfix_sequence = segment_records
        .iter()
        .find(|record| record.nav_ref.is_none())
        .map(|record| record.sequence)
        .unwrap_or(i32::MAX);
    first_nonfix_sequence
        .min(first_missed_like_sequence_after_final_course(segment_records).unwrap_or(i32::MAX))
}

fn first_missed_like_sequence_after_final_course(
    segment_records: &[ProcedureLegMaterializationRecord],
) -> Option<i32> {
    let mut saw_final_course = false;
    for record in segment_records {
        match record.path_termination.trim() {
            "CF" | "FA" | "RF" | "TF" => saw_final_course = true,
            "DF" | "HA" | "HF" | "HM" if saw_final_course => return Some(record.sequence),
            _ => {}
        }
    }
    None
}

fn segment_traversal_policy<'a>(
    previous_display_path: Option<&LegDisplayPath>,
    previous_leg_to: Option<&NavRef>,
    resolved_last: Option<&ResolvedLeg>,
    segment_records: &'a [ProcedureLegMaterializationRecord],
    fix_records: &[&'a ProcedureLegMaterializationRecord],
) -> SegmentTraversalPolicy<'a> {
    let previous_was_hold_like = resolved_last.is_some_and(|previous| {
        previous
            .procedure_provenance
            .as_ref()
            .is_some_and(|provenance| {
                matches!(
                    &provenance.path_termination,
                    PathTermination::Other(label)
                        if matches!(label.trim(), "HA" | "HF" | "HM")
                )
            })
    });
    SegmentTraversalPolicy {
        common_resume_target: resumed_common_target(
            previous_display_path,
            previous_was_hold_like,
            segment_records,
        ),
        skip_through_index: reconciliation_resume_skip_through_index(
            previous_display_path,
            previous_leg_to,
            segment_records,
            fix_records,
        ),
    }
}

fn project_terminal_state_through_intervening_climbs(
    current_position: Option<LatLon>,
    current_course_deg: Option<f64>,
    preceding_anchor_record: &ProcedureLegMaterializationRecord,
    direct_to_fix_record: &ProcedureLegMaterializationRecord,
    segment_records: &[ProcedureLegMaterializationRecord],
) -> (Option<LatLon>, Option<f64>) {
    let (Some(mut current_position), Some(mut current_course_deg)) =
        (current_position, current_course_deg)
    else {
        return (None, None);
    };
    let mut current_altitude_ft = preceding_anchor_record.altitude_1_ft;
    for record in segment_records.iter().filter(|record| {
        record.sequence > preceding_anchor_record.sequence
            && record.sequence < direct_to_fix_record.sequence
    }) {
        if record.path_termination.trim() != "CA" {
            continue;
        }
        let Some(course_deg) = record
            .magnetic_course_deg
            .map(|course| course + record_magnetic_variation_deg(record).unwrap_or(0.0))
            .or(Some(current_course_deg))
        else {
            continue;
        };
        let (Some(start_alt_ft), Some(target_alt_ft)) = (current_altitude_ft, record.altitude_1_ft)
        else {
            current_course_deg = course_deg;
            continue;
        };
        let climb_minutes = ((target_alt_ft - start_alt_ft).max(0.0)) / 500.0;
        let climb_distance_nm = (90.0 / 60.0) * climb_minutes;
        current_position = route_destination_point(current_position, course_deg, climb_distance_nm);
        current_course_deg = course_deg;
        current_altitude_ft = Some(target_alt_ft);
    }
    (Some(current_position), Some(current_course_deg))
}

fn should_yield_direct_to_fix_to_following_course(
    current_position: Option<LatLon>,
    current_course_deg: Option<f64>,
    preceding_anchor_record: &ProcedureLegMaterializationRecord,
    direct_to_fix_record: &ProcedureLegMaterializationRecord,
    segment_records: &[ProcedureLegMaterializationRecord],
    following_course_record: &ProcedureLegMaterializationRecord,
    common_segment: bool,
) -> bool {
    let (projected_position, projected_course_deg) =
        project_terminal_state_through_intervening_climbs(
            current_position,
            current_course_deg,
            preceding_anchor_record,
            direct_to_fix_record,
            segment_records,
        );
    terminal_state_for_handoff(
        projected_position,
        projected_course_deg,
        preceding_anchor_record.nav_ref.clone(),
        common_segment,
    )
    .zip(start_requirement_for_direct_to_fix_with_following_course(
        direct_to_fix_record,
        following_course_record,
    ))
    .is_some_and(|(terminal_state, start_requirement)| {
        matches!(
            reconcile_handoff(&terminal_state, &start_requirement),
            HandoffDecision::SkipStaleFix
        )
    })
}

fn should_yield_feeder_course_to_fix_to_resumed_common_segment(
    current_position: Option<LatLon>,
    current_course_deg: Option<f64>,
    current_anchor: Option<NavRef>,
    feeder_course_to_fix_record: &ProcedureLegMaterializationRecord,
    resumed_common_record: &ProcedureLegMaterializationRecord,
) -> bool {
    terminal_state_for_handoff(current_position, current_course_deg, current_anchor, false)
        .zip(
            start_requirement_for_feeder_course_to_fix_with_common_resume(
                feeder_course_to_fix_record,
                resumed_common_record,
            ),
        )
        .is_some_and(|(terminal_state, start_requirement)| {
            matches!(
                reconcile_handoff(&terminal_state, &start_requirement),
                HandoffDecision::YieldToFollowingCourse
            )
        })
}

fn procedure_segment_role(role: &MaterializedSegmentRole) -> ProcedureSegmentRole {
    match role {
        MaterializedSegmentRole::EnrouteTransition => ProcedureSegmentRole::EnrouteTransition,
        MaterializedSegmentRole::Common => ProcedureSegmentRole::Common,
        MaterializedSegmentRole::RunwayTransition => ProcedureSegmentRole::RunwayTransition,
    }
}

fn concretize_procedure_materialization_legs(
    legs: &[ProcedureLegMaterializationRecord],
    reverse_segment_order: bool,
) -> Vec<ConcretizedNavItem> {
    let mut waypoints = legs
        .iter()
        .filter_map(|leg| leg.nav_ref.clone())
        .collect::<Vec<_>>();
    waypoints.dedup();

    let terminal_discontinuity = legs.last().and_then(terminal_procedure_discontinuity);
    let initial_discontinuity = legs
        .iter()
        .take_while(|leg| leg.nav_ref.is_none())
        .last()
        .and_then(leading_procedure_discontinuity);

    if reverse_segment_order {
        waypoints.reverse();
    }

    let mut items = waypoints
        .into_iter()
        .map(|nav_ref| ConcretizedNavItem::Waypoint { nav_ref })
        .collect::<Vec<_>>();

    if reverse_segment_order {
        if let Some(discontinuity) = initial_discontinuity {
            items.push(ConcretizedNavItem::Discontinuity {
                label: discontinuity.display_label().to_string(),
                discontinuity,
            });
        }
    } else if let Some(discontinuity) = terminal_discontinuity {
        items.push(ConcretizedNavItem::Discontinuity {
            label: discontinuity.display_label().to_string(),
            discontinuity,
        });
    }

    items
}

fn merge_concretized_segments_from_records(
    segments: Vec<Vec<ConcretizedNavItem>>,
) -> Vec<ConcretizedNavItem> {
    let mut merged = Vec::<ConcretizedNavItem>::new();

    for segment in segments {
        for item in segment {
            let is_duplicate_boundary = matches!(
                (merged.last(), &item),
                (
                    Some(ConcretizedNavItem::Waypoint { nav_ref: left }),
                    ConcretizedNavItem::Waypoint { nav_ref: right }
                ) if left == right
            );
            if !is_duplicate_boundary {
                merged.push(item);
            }
        }
    }

    merged
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGeometryFinalRouteRejection {
    pub airport_id: String,
    pub procedure_id: String,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    pub error: AppError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGeometryMaterializationRejection {
    pub airport_id: String,
    pub procedure_id: String,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    pub error: AppError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGeometryCourseJoinBypassCandidate {
    pub airport_id: String,
    pub procedure_id: String,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    pub diagnostic: String,
}

#[derive(Debug)]
pub struct ProcedureGeometryRecordAudit {
    pub records: Vec<pgt::ProcedureGeometryRecord>,
    pub materialization_rejections: Vec<ProcedureGeometryMaterializationRejection>,
    pub final_route_rejections: Vec<ProcedureGeometryFinalRouteRejection>,
    pub course_join_bypass_candidates: Vec<ProcedureGeometryCourseJoinBypassCandidate>,
}

fn collect_procedure_geometry_records(
    procedure_kinds: BTreeMap<(String, String), ProcedureKind>,
    distinct_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
    materialization_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<ProcedureGeometryRecordAudit> {
    let mut records = Vec::new();
    let mut materialization_rejections = Vec::new();
    let mut final_route_rejections = Vec::new();
    let mut course_join_bypass_candidates = Vec::new();
    for ((airport_id, procedure_id), distinct_rows_value) in distinct_by_procedure {
        let Some(kind) = procedure_kinds
            .get(&(airport_id.clone(), procedure_id.clone()))
            .cloned()
        else {
            continue;
        };
        let Some(materialization_rows_value) =
            materialization_by_procedure.get(&(airport_id.clone(), procedure_id.clone()))
        else {
            continue;
        };
        let distinct_rows = serde_json::from_value::<Vec<ProcedureDistinctRow>>(
            serde_json::Value::Array(distinct_rows_value.clone()),
        )?;
        let materialization_rows = serde_json::from_value::<Vec<ProcedureLegMaterializationRecord>>(
            serde_json::Value::Array(materialization_rows_value.clone()),
        )?;
        let options = describe_procedure_options_from_rows(
            &airport_id,
            &procedure_id,
            kind.clone(),
            distinct_rows.clone(),
        )?;

        if options.valid_choices.is_empty() {
            continue;
        }

        for choice in options.valid_choices {
            let built = match materialize_procedure_from_records(MaterializeProcedureInput {
                airport_id: &airport_id,
                procedure_id: &procedure_id,
                kind: kind.clone(),
                runway_transition: choice.runway_transition.clone(),
                enroute_transition: choice.enroute_transition.clone(),
                component_index: 0,
                rows: distinct_rows.clone(),
                legs: materialization_rows.clone(),
            }) {
                Ok(built) => built,
                Err(error) => {
                    materialization_rejections.push(ProcedureGeometryMaterializationRejection {
                        airport_id: airport_id.clone(),
                        procedure_id: procedure_id.clone(),
                        runway_transition: choice.runway_transition.clone(),
                        enroute_transition: choice.enroute_transition.clone(),
                        error,
                    });
                    continue;
                }
            };
            if let Err(error) =
                app_core::planning::validate_materialized_procedure_final_route(&built)
            {
                final_route_rejections.push(ProcedureGeometryFinalRouteRejection {
                    airport_id: airport_id.clone(),
                    procedure_id: procedure_id.clone(),
                    runway_transition: choice.runway_transition.clone(),
                    enroute_transition: choice.enroute_transition.clone(),
                    error,
                });
            }
            for diagnostic in built
                .resolved_legs
                .iter()
                .filter_map(|leg| leg.procedure_provenance.as_ref())
                .filter_map(|provenance| provenance.display_path.as_ref())
                .flat_map(|path| path.debug_element_sources.iter())
                .filter(|source| source.starts_with(CF_DIRECT_SHORTCUT_MISALIGNED_SOURCE_PREFIX))
            {
                course_join_bypass_candidates.push(ProcedureGeometryCourseJoinBypassCandidate {
                    airport_id: airport_id.clone(),
                    procedure_id: procedure_id.clone(),
                    runway_transition: choice.runway_transition.clone(),
                    enroute_transition: choice.enroute_transition.clone(),
                    diagnostic: diagnostic.clone(),
                });
            }
            records.push(procedure_geometry_record_from_materialized(built, choice));
        }
    }
    Ok(ProcedureGeometryRecordAudit {
        records,
        materialization_rejections,
        final_route_rejections,
        course_join_bypass_candidates,
    })
}

pub fn audit_procedure_geometry_records(
    procedure_kinds: BTreeMap<(String, String), ProcedureKind>,
    distinct_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
    materialization_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<ProcedureGeometryRecordAudit> {
    collect_procedure_geometry_records(
        procedure_kinds,
        distinct_by_procedure,
        materialization_by_procedure,
    )
}

pub fn build_procedure_geometry_records(
    procedure_kinds: BTreeMap<(String, String), ProcedureKind>,
    distinct_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
    materialization_by_procedure: BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<Vec<pgt::ProcedureGeometryRecord>> {
    let audit = collect_procedure_geometry_records(
        procedure_kinds,
        distinct_by_procedure,
        materialization_by_procedure,
    )?;
    if let Some(rejection) = audit.materialization_rejections.first() {
        anyhow::bail!(
            "failed to materialize {} {} runway={:?} enroute={:?}: {} ({} total rejections)",
            rejection.airport_id,
            rejection.procedure_id,
            rejection.runway_transition,
            rejection.enroute_transition,
            rejection.error,
            audit.materialization_rejections.len(),
        );
    }
    if let Some(rejection) = audit.final_route_rejections.first() {
        anyhow::bail!(
            "final materialized route rejected for {} {} runway={:?} enroute={:?}: {} ({} total rejections)",
            rejection.airport_id,
            rejection.procedure_id,
            rejection.runway_transition,
            rejection.enroute_transition,
            rejection.error,
            audit.final_route_rejections.len(),
        );
    }
    Ok(audit.records)
}

pub fn procedure_kinds_from_lists(
    approach_lists: BTreeMap<String, BTreeSet<String>>,
    sid_lists: BTreeMap<String, BTreeSet<String>>,
    star_lists: BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<(String, String), ProcedureKind> {
    let mut kinds = BTreeMap::new();
    for (airport_id, procedure_ids) in approach_lists {
        for procedure_id in procedure_ids {
            kinds.insert((airport_id.clone(), procedure_id), ProcedureKind::Approach);
        }
    }
    for (airport_id, procedure_ids) in sid_lists {
        for procedure_id in procedure_ids {
            kinds.insert((airport_id.clone(), procedure_id), ProcedureKind::Sid);
        }
    }
    for (airport_id, procedure_ids) in star_lists {
        for procedure_id in procedure_ids {
            kinds.insert((airport_id.clone(), procedure_id), ProcedureKind::Star);
        }
    }
    kinds
}

fn procedure_geometry_record_from_materialized(
    built: MaterializedProcedure,
    choice: ProcedureSpecChoice,
) -> pgt::ProcedureGeometryRecord {
    let terminal_discontinuity = built.procedure.terminal_discontinuity.clone();
    let key = pgt::ProcedureGeometryKey {
        airport_id: built.procedure.airport_id.0.clone(),
        procedure_id: built.procedure.procedure_id.clone(),
        kind: procedure_kind_to_geometry(&built.procedure.kind),
        runway_transition: choice.runway_transition,
        enroute_transition: choice.enroute_transition,
    };
    let mut leg_bundles = built
        .resolved_legs
        .into_iter()
        .map(procedure_geometry_bundle_from_resolved_leg)
        .collect::<Vec<_>>();
    if terminal_discontinuity.is_some() {
        if let Some(last) = leg_bundles.last_mut() {
            last.sequencing_after = pgt::ProcedureSequencingRule::Suspend;
        }
    }
    pgt::ProcedureGeometryRecord {
        key,
        terminal_discontinuity: terminal_discontinuity.map(procedure_discontinuity_to_geometry),
        components: Vec::new(),
        leg_bundles,
        data_quality: built
            .procedure
            .data_quality
            .into_iter()
            .map(|message| pgt::ProcedureDataQualityAnnotation { message })
            .collect(),
    }
}

fn procedure_geometry_bundle_from_resolved_leg(
    leg: ResolvedLeg,
) -> pgt::ProcedureGeometryLegBundle {
    let provenance = leg.procedure_provenance;
    let path = provenance
        .as_ref()
        .and_then(|provenance| provenance.display_path.clone())
        .map(procedure_geometry_path_from_display_path)
        .unwrap_or_else(|| pgt::ProcedureGeometryPath {
            style: pgt::ProcedureGeometryPathStyle::Solid,
            elements: Vec::new(),
            effective_terminal_course_deg: None,
        });
    let role = provenance
        .as_ref()
        .map(|provenance| procedure_segment_role_to_geometry(&provenance.role))
        .unwrap_or(pgt::ProcedureSegmentRole::Common);
    let path_termination = provenance
        .as_ref()
        .map(|provenance| path_termination_to_geometry(&provenance.path_termination))
        .unwrap_or_else(|| pgt::ProcedurePathTermination::Other(String::new()));
    let leg_sequence = provenance
        .as_ref()
        .map(|provenance| provenance.leg_sequence)
        .unwrap_or_default();
    let discontinuity_after = provenance
        .as_ref()
        .and_then(|provenance| provenance.discontinuity_after.clone());

    pgt::ProcedureGeometryLegBundle {
        id: leg.id,
        role,
        from: nav_ref_to_geometry(leg.from),
        to: nav_ref_to_geometry(leg.to.clone()),
        path_termination,
        leg_sequence,
        path,
        waypoints: vec![pgt::ProcedureGeometryWaypoint {
            nav_ref: nav_ref_to_geometry(leg.to),
            name: None,
        }],
        sequencing_after: if discontinuity_after.is_some() {
            pgt::ProcedureSequencingRule::Suspend
        } else {
            pgt::ProcedureSequencingRule::Continue
        },
        discontinuity_after: discontinuity_after.map(procedure_discontinuity_to_geometry),
        source_row_sequences: if leg_sequence == 0 {
            Vec::new()
        } else {
            vec![leg_sequence]
        },
    }
}

fn procedure_geometry_path_from_display_path(path: LegDisplayPath) -> pgt::ProcedureGeometryPath {
    pgt::ProcedureGeometryPath {
        style: match path.style {
            LegDisplayPathStyle::Solid => pgt::ProcedureGeometryPathStyle::Solid,
            LegDisplayPathStyle::Dashed => pgt::ProcedureGeometryPathStyle::Dashed,
            LegDisplayPathStyle::Vectors => {
                panic!("vectors styling is derived from procedure sequencing at render time")
            }
        },
        elements: path
            .elements
            .into_iter()
            .map(|element| match element {
                LegDisplayElement::Segment { start, end } => {
                    pgt::ProcedureGeometryElement::Segment {
                        start: lat_lon_to_geometry(start),
                        end: lat_lon_to_geometry(end),
                    }
                }
                LegDisplayElement::Arc {
                    center,
                    radius_nm,
                    start,
                    end,
                    clockwise,
                    sweep_degrees,
                } => pgt::ProcedureGeometryElement::Arc {
                    center: lat_lon_to_geometry(center),
                    radius_nm,
                    start: lat_lon_to_geometry(start),
                    end: lat_lon_to_geometry(end),
                    clockwise,
                    sweep_degrees,
                },
            })
            .collect(),
        effective_terminal_course_deg: path.effective_terminal_course_deg,
    }
}

fn nav_ref_to_geometry(nav_ref: NavRef) -> pgt::ProcedureNavRef {
    match nav_ref {
        NavRef::Airport(id) => pgt::ProcedureNavRef::Airport(id),
        NavRef::Navaid(id) => pgt::ProcedureNavRef::Navaid(id),
        NavRef::Fix(id) => pgt::ProcedureNavRef::Fix(id),
        NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => pgt::ProcedureNavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => pgt::ProcedureNavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        NavRef::LatLon(value) | NavRef::Spot(value) => {
            pgt::ProcedureNavRef::LatLon(lat_lon_to_geometry(value))
        }
    }
}

fn lat_lon_to_geometry(value: LatLon) -> pgt::ProcedureLatLon {
    pgt::ProcedureLatLon {
        lat: value.lat,
        lon: value.lon,
    }
}

fn procedure_kind_to_geometry(kind: &ProcedureKind) -> pgt::ProcedureKind {
    match kind {
        ProcedureKind::Sid => pgt::ProcedureKind::Sid,
        ProcedureKind::Star => pgt::ProcedureKind::Star,
        ProcedureKind::Approach => pgt::ProcedureKind::Approach,
    }
}

fn procedure_discontinuity_to_geometry(
    discontinuity: ProcedureDiscontinuity,
) -> pgt::ProcedureDiscontinuity {
    match discontinuity {
        ProcedureDiscontinuity::Vectors => pgt::ProcedureDiscontinuity::Vectors,
        ProcedureDiscontinuity::Hold => pgt::ProcedureDiscontinuity::Hold,
        ProcedureDiscontinuity::Other(label) => pgt::ProcedureDiscontinuity::Other(label),
    }
}

fn procedure_segment_role_to_geometry(role: &ProcedureSegmentRole) -> pgt::ProcedureSegmentRole {
    match role {
        ProcedureSegmentRole::EnrouteTransition => pgt::ProcedureSegmentRole::EnrouteTransition,
        ProcedureSegmentRole::Common => pgt::ProcedureSegmentRole::Common,
        ProcedureSegmentRole::RunwayTransition => pgt::ProcedureSegmentRole::RunwayTransition,
    }
}

fn path_termination_to_geometry(path: &PathTermination) -> pgt::ProcedurePathTermination {
    match path {
        PathTermination::InitialFix => pgt::ProcedurePathTermination::InitialFix,
        PathTermination::TrackToFix => pgt::ProcedurePathTermination::TrackToFix,
        PathTermination::CourseToFix => pgt::ProcedurePathTermination::CourseToFix,
        PathTermination::DirectToFix => pgt::ProcedurePathTermination::DirectToFix,
        PathTermination::HeadingToManual => pgt::ProcedurePathTermination::HeadingToManual,
        PathTermination::HeadingToAltitude => pgt::ProcedurePathTermination::HeadingToAltitude,
        PathTermination::Other(value) => pgt::ProcedurePathTermination::Other(value.clone()),
    }
}

#[cfg(test)]
mod published_geometry_build_tests {
    use super::*;

    #[test]
    fn sid_generated_direct_path_can_turn_onto_published_course() {
        let fix = LatLon {
            lat: 33.95,
            lon: -118.20,
        };
        let previous = sid_heading_signature(
            "DF",
            LatLon {
                lat: 33.94,
                lon: -118.21,
            },
            fix,
            10.0,
            "synthesized-path",
            "KLIPR",
        );
        let current = sid_heading_signature(
            "TF",
            fix,
            LatLon {
                lat: 34.02,
                lon: -118.13,
            },
            44.0,
            "KLIPR",
            "KEGGS",
        );

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Sid,),
            45.0,
        );
    }

    #[test]
    fn short_sid_runway_leg_has_room_for_published_waypoint_turn() {
        let fix = LatLon {
            lat: 52.238328,
            lon: -174.213836,
        };
        let previous = sid_heading_signature(
            "DF",
            LatLon {
                lat: fix.lat - 1.49 / 60.0,
                lon: fix.lon,
            },
            fix,
            337.2,
            "RW34",
            "ZIKEF",
        );
        let current = sid_heading_signature(
            "TF",
            fix,
            LatLon {
                lat: fix.lat,
                lon: fix.lon - 11.3 / (60.0 * fix.lat.to_radians().cos()),
            },
            270.6,
            "ZIKEF",
            "BILNE",
        );

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Sid,),
            150.0,
        );
    }

    #[test]
    fn sid_generated_direct_can_turn_onto_track_before_downstream_hold() {
        let fix = LatLon {
            lat: 39.19,
            lon: -119.70,
        };
        let previous = sid_heading_signature(
            "DF",
            LatLon {
                lat: 39.19,
                lon: -119.71,
            },
            fix,
            89.0,
            "synthesized-path",
            "MOPNE",
        );
        let current = sid_heading_signature(
            "HA",
            fix,
            LatLon {
                lat: 39.00,
                lon: -119.62,
            },
            164.0,
            "MOPNE",
            "synthesized-path",
        );

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Sid,),
            120.0,
        );
    }

    #[test]
    fn sid_can_turn_from_fix_track_to_published_manual_heading() {
        let fix = LatLon {
            lat: 25.96,
            lon: -80.44,
        };
        let mut previous = sid_heading_signature(
            "TF",
            LatLon {
                lat: 25.85,
                lon: -80.45,
            },
            fix,
            9.0,
            "RUSSS",
            "RMIRO",
        );
        previous.end_course_deg = 26.0;
        let mut current = sid_heading_signature(
            "VM",
            fix,
            LatLon {
                lat: 25.963,
                lon: -80.439,
            },
            26.0,
            "RMIRO",
            "synthesized-path",
        );
        current.element_kind = DisplayElementKind::Arc;

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Sid,),
            30.0,
        );
    }

    #[test]
    fn sid_named_cf_branch_uses_published_waypoint_turn_allowance() {
        let fix = LatLon {
            lat: 34.257589,
            lon: -119.953203,
        };
        let previous = sid_heading_signature(
            "CF",
            LatLon {
                lat: 34.42,
                lon: -119.90,
            },
            fix,
            211.0,
            "synthesized-path",
            "FLOUT",
        );
        let mut current = sid_heading_signature(
            "CF",
            fix,
            LatLon {
                lat: 34.50,
                lon: -120.20,
            },
            335.0,
            "FLOUT",
            "synthesized-path",
        );
        current.procedure_role = ProcedureSegmentRole::EnrouteTransition;

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Sid,),
            150.0,
        );
    }

    #[test]
    fn star_published_tf_turn_allows_short_outbound_leg() {
        let fix = LatLon {
            lat: 29.7135083333333,
            lon: -98.5704027777778,
        };
        let previous = sid_heading_signature(
            "TF",
            LatLon {
                lat: 29.7090555555556,
                lon: -98.4309277777778,
            },
            fix,
            268.1,
            "SALZA",
            "DOMNK",
        );
        let current = sid_heading_signature(
            "TF",
            fix,
            LatLon {
                lat: 29.7282861111111,
                lon: -98.589525,
            },
            307.7,
            "DOMNK",
            "CARNE",
        );

        assert_eq!(
            continuity_heading_tolerance_deg(None, &previous, &current, None, &ProcedureKind::Star,),
            120.0,
        );
    }

    #[test]
    fn star_materialization_includes_all_labeled_common_segment() {
        let distinct_rows = vec![
            ProcedureDistinctRow {
                route_type: "4".to_string(),
                transition_id: "INYOE".to_string(),
            },
            ProcedureDistinctRow {
                route_type: "5".to_string(),
                transition_id: "ALL".to_string(),
            },
            ProcedureDistinctRow {
                route_type: "6".to_string(),
                transition_id: "RW10B".to_string(),
            },
        ];
        let leg = |route_type: &str,
                   transition_id: &str,
                   sequence: i32,
                   fix: &str,
                   lat: f64,
                   lon: f64,
                   path_termination: &str| {
            serde_json::from_value::<ProcedureLegMaterializationRecord>(serde_json::json!({
                "key": {
                    "airport_id": "KSFO",
                    "procedure_id": "ALWYS3",
                    "route_type": route_type,
                    "transition_id": transition_id,
                },
                "sequence": sequence,
                "nav_ref": { "Fix": fix },
                "nav_position": { "lat": lat, "lon": lon },
                "path_termination": path_termination,
            }))
            .expect("test STAR row should decode")
        };
        let rows = vec![
            leg("4", "INYOE", 10, "INYOE", 37.8, -120.0, "IF"),
            leg("4", "INYOE", 20, "DYAMD", 37.7, -120.4, "TF"),
            leg("5", "ALL", 10, "DYAMD", 37.7, -120.4, "IF"),
            leg("5", "ALL", 20, "ALWYS", 37.6, -120.8, "TF"),
            leg("6", "RW10B", 10, "ALWYS", 37.6, -120.8, "IF"),
            leg("6", "RW10B", 20, "HEFLY", 37.5, -121.2, "TF"),
        ];

        let result = materialize_procedure_from_records(MaterializeProcedureInput {
            airport_id: "KSFO",
            procedure_id: "ALWYS3",
            kind: ProcedureKind::Star,
            runway_transition: Some("RW10B".to_string()),
            enroute_transition: Some("INYOE".to_string()),
            component_index: 0,
            rows: distinct_rows,
            legs: rows,
        })
        .expect("ALL-labeled STAR common segment should materialize");

        assert!(result.resolved_legs.iter().any(|leg| {
            leg.procedure_provenance
                .as_ref()
                .is_some_and(|provenance| provenance.role == ProcedureSegmentRole::Common)
        }));
    }

    #[test]
    fn star_allows_published_long_enroute_transition_leg() {
        let result = build_procedure_geometry_records(
            BTreeMap::from([(
                ("KSFO".to_string(), "STINS4".to_string()),
                ProcedureKind::Star,
            )]),
            BTreeMap::from([(
                ("KSFO".to_string(), "STINS4".to_string()),
                vec![serde_json::json!({
                    "route_type": "1",
                    "transition_id": "RBG",
                })],
            )]),
            BTreeMap::from([(
                ("KSFO".to_string(), "STINS4".to_string()),
                vec![
                    serde_json::json!({
                        "key": {
                            "airport_id": "KSFO",
                            "procedure_id": "STINS4",
                            "route_type": "1",
                            "transition_id": "RBG",
                        },
                        "sequence": 10,
                        "nav_ref": { "Navaid": "RBG" },
                        "nav_position": { "lat": 43.1824166666667, "lon": -123.352241666667 },
                        "path_termination": "IF",
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KSFO",
                            "procedure_id": "STINS4",
                            "route_type": "1",
                            "transition_id": "RBG",
                        },
                        "sequence": 20,
                        "nav_ref": { "Navaid": "ENI" },
                        "nav_position": { "lat": 39.0532166666667, "lon": -123.274326666667 },
                        "path_termination": "TF",
                    }),
                ],
            )]),
        );

        assert!(result.is_ok(), "published 248 NM STAR leg: {result:?}");
    }

    #[test]
    fn sid_reciprocal_runway_to_enroute_boundary_suspends_for_course_reversal() {
        let runway = LatLon {
            lat: 34.43,
            lon: -119.84,
        };
        let flout = LatLon {
            lat: 34.2575888888889,
            lon: -119.953202777778,
        };
        let rzs = LatLon {
            lat: 34.5095319444444,
            lon: -119.770988055556,
        };
        let leg = |id: &str,
                   from: NavRef,
                   to: NavRef,
                   role: ProcedureSegmentRole,
                   start: LatLon,
                   end: LatLon| ResolvedLeg {
            id: id.to_string(),
            from,
            to,
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KSBA".to_string(),
                procedure_id: "FLOUT5".to_string(),
                kind: ProcedureKind::Sid,
                role,
                path_termination: PathTermination::CourseToFix,
                leg_sequence: 10,
                discontinuity_after: None,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![LegDisplayElement::Segment { start, end }],
                    effective_terminal_course_deg: None,
                    debug_element_sources: vec!["test".to_string()],
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let mut resolved = vec![
            leg(
                "runway-to-flout",
                NavRef::Fix("RW07".to_string()),
                NavRef::Fix("FLOUT".to_string()),
                ProcedureSegmentRole::RunwayTransition,
                runway,
                flout,
            ),
            leg(
                "flout-to-rzs",
                NavRef::Fix("FLOUT".to_string()),
                NavRef::Navaid("RZS".to_string()),
                ProcedureSegmentRole::EnrouteTransition,
                flout,
                rzs,
            ),
        ];

        mark_unspecified_sid_segment_boundary_course_reversals(
            &mut resolved,
            &ProcedureKind::Sid,
            &[],
        );

        assert_eq!(
            resolved[0]
                .procedure_provenance
                .as_ref()
                .unwrap()
                .discontinuity_after,
            Some(ProcedureDiscontinuity::Other("COURSE REVERSAL".to_string())),
        );
    }

    #[test]
    fn build_procedure_geometry_records_publishes_sid_geometry() {
        let mut kinds = BTreeMap::new();
        kinds.insert(
            ("KSEA".to_string(), "BANGR9".to_string()),
            ProcedureKind::Sid,
        );

        let mut distinct_by_procedure = BTreeMap::new();
        distinct_by_procedure.insert(
            ("KSEA".to_string(), "BANGR9".to_string()),
            vec![
                serde_json::json!({ "route_type": "4", "transition_id": "RW16L" }),
                serde_json::json!({ "route_type": "6", "transition_id": "ARRIE" }),
            ],
        );
        let mut materialization_by_procedure = BTreeMap::new();
        materialization_by_procedure.insert(
            ("KSEA".to_string(), "BANGR9".to_string()),
            vec![
                sid_test_leg(
                    "4",
                    "RW16L",
                    10,
                    "CAVOB",
                    LatLon {
                        lat: 47.30,
                        lon: -122.30,
                    },
                    "IF",
                    None,
                ),
                sid_test_leg(
                    "4",
                    "RW16L",
                    20,
                    "BANGR",
                    LatLon {
                        lat: 47.40,
                        lon: -122.20,
                    },
                    "DF",
                    Some("L"),
                ),
                sid_test_leg(
                    "6",
                    "ARRIE",
                    10,
                    "BANGR",
                    LatLon {
                        lat: 47.40,
                        lon: -122.20,
                    },
                    "IF",
                    None,
                ),
                sid_test_leg(
                    "6",
                    "ARRIE",
                    20,
                    "ARRIE",
                    LatLon {
                        lat: 47.50,
                        lon: -122.10,
                    },
                    "TF",
                    None,
                ),
            ],
        );

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("SID geometry should materialize");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.kind, pgt::ProcedureKind::Sid);
        assert_eq!(records[0].key.runway_transition.as_deref(), Some("RW16L"));
        assert_eq!(records[0].key.enroute_transition.as_deref(), Some("ARRIE"));
    }

    fn sid_heading_signature(
        path_termination: &str,
        start_position: LatLon,
        end_position: LatLon,
        course_deg: f64,
        start_label: &str,
        end_label: &str,
    ) -> DisplayElementHeadingSignature {
        DisplayElementHeadingSignature {
            step_index: 0,
            airport_id: "KLAX".to_string(),
            procedure_id: "ORCKA5".to_string(),
            procedure_role: ProcedureSegmentRole::RunwayTransition,
            path_termination: path_termination.to_string(),
            start_position,
            start_course_deg: course_deg,
            start_label: start_label.to_string(),
            start_magnetic_variation_deg: None,
            end_position,
            end_course_deg: course_deg,
            drawn_end_course_deg: course_deg,
            end_label: end_label.to_string(),
            end_magnetic_variation_deg: None,
            element_kind: DisplayElementKind::Segment,
            element_role: None,
            turn_direction: None,
            discontinuity_after: false,
        }
    }

    #[test]
    fn build_procedure_geometry_records_anchors_single_fix_sid_at_runway() {
        let kinds = BTreeMap::from([(
            ("1U7".to_string(), "FIROS1".to_string()),
            ProcedureKind::Sid,
        )]);
        let distinct_by_procedure = BTreeMap::from([(
            ("1U7".to_string(), "FIROS1".to_string()),
            vec![serde_json::json!({
                "route_type": "4",
                "transition_id": "RW10",
            })],
        )]);
        let materialization_by_procedure = BTreeMap::from([(
            ("1U7".to_string(), "FIROS1".to_string()),
            vec![
                serde_json::json!({
                    "key": {
                        "airport_id": "1U7",
                        "procedure_id": "FIROS1",
                        "route_type": "4",
                        "transition_id": "RW10",
                    },
                    "sequence": 10,
                    "nav_ref": null,
                    "path_termination": "VA",
                    "magnetic_course_deg": 104.1,
                    "departure_anchor_ref": { "Fix": "RW10" },
                    "departure_anchor_position": { "lat": 42.23, "lon": -111.15 },
                }),
                serde_json::json!({
                    "key": {
                        "airport_id": "1U7",
                        "procedure_id": "FIROS1",
                        "route_type": "4",
                        "transition_id": "RW10",
                    },
                    "sequence": 20,
                    "nav_ref": { "Fix": "FIROS" },
                    "nav_position": { "lat": 42.30, "lon": -111.00 },
                    "path_termination": "DF",
                }),
            ],
        )]);

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("single-fix SID should materialize from its runway anchor");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.kind, pgt::ProcedureKind::Sid);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_publishes_vector_sid_route_type_t() {
        let kinds = BTreeMap::from([(
            ("KPAE".to_string(), "PAINE6".to_string()),
            ProcedureKind::Sid,
        )]);
        let distinct_by_procedure = BTreeMap::from([(
            ("KPAE".to_string(), "PAINE6".to_string()),
            vec![serde_json::json!({
                "route_type": "T",
                "transition_id": "RW16L",
            })],
        )]);
        let materialization_by_procedure = BTreeMap::from([(
            ("KPAE".to_string(), "PAINE6".to_string()),
            vec![serde_json::json!({
                "key": {
                    "airport_id": "KPAE",
                    "procedure_id": "PAINE6",
                    "route_type": "T",
                    "transition_id": "RW16L",
                },
                "sequence": 10,
                "nav_ref": null,
                "path_termination": "VM",
                "magnetic_course_deg": 163.6,
                "airport_magnetic_variation_deg": 15.0,
                "departure_anchor_ref": { "Fix": "RW16L" },
                "departure_anchor_position": { "lat": 47.9218, "lon": -122.2855 },
                "departure_anchor_altitude_ft": 606.0,
            })],
        )]);

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("vector SID should materialize from its runway anchor");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.kind, pgt::ProcedureKind::Sid);
        assert_eq!(records[0].key.runway_transition.as_deref(), Some("RW16L"));
        assert_eq!(
            records[0].terminal_discontinuity,
            Some(pgt::ProcedureDiscontinuity::Vectors)
        );
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_anchors_vector_sid_fm_at_runway() {
        let kinds =
            BTreeMap::from([(("KBOI".to_string(), "BOI3".to_string()), ProcedureKind::Sid)]);
        let distinct_by_procedure = BTreeMap::from([(
            ("KBOI".to_string(), "BOI3".to_string()),
            vec![serde_json::json!({
                "route_type": "T",
                "transition_id": "RW10B",
            })],
        )]);
        let materialization_by_procedure = BTreeMap::from([(
            ("KBOI".to_string(), "BOI3".to_string()),
            vec![serde_json::json!({
                "key": {
                    "airport_id": "KBOI",
                    "procedure_id": "BOI3",
                    "route_type": "T",
                    "transition_id": "RW10B",
                },
                "sequence": 10,
                "nav_ref": { "Navaid": "BOI" },
                "nav_position": { "lat": 43.5528, "lon": -116.1921 },
                "path_termination": "FM",
                "magnetic_course_deg": 98.0,
                "airport_magnetic_variation_deg": 12.0,
                "departure_anchor_ref": { "Fix": "RW10B" },
                "departure_anchor_position": { "lat": 43.5680, "lon": -116.2300 },
                "departure_anchor_altitude_ft": 2871.0,
            })],
        )]);

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("FM vector SID should materialize from its runway anchor");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.runway_transition.as_deref(), Some("RW10B"));
        assert_eq!(
            records[0].terminal_discontinuity,
            Some(pgt::ProcedureDiscontinuity::Vectors)
        );
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_anchors_initial_direct_to_at_runway() {
        let kinds = BTreeMap::from([(
            ("KAPC".to_string(), "NAPAA4".to_string()),
            ProcedureKind::Sid,
        )]);
        let distinct_by_procedure = BTreeMap::from([(
            ("KAPC".to_string(), "NAPAA4".to_string()),
            vec![serde_json::json!({
                "route_type": "1",
                "transition_id": "RW24",
            })],
        )]);
        let materialization_by_procedure = BTreeMap::from([(
            ("KAPC".to_string(), "NAPAA4".to_string()),
            vec![serde_json::json!({
                "key": {
                    "airport_id": "KAPC",
                    "procedure_id": "NAPAA4",
                    "route_type": "1",
                    "transition_id": "RW24",
                },
                "sequence": 10,
                "nav_ref": { "Navaid": "SGD" },
                "nav_position": { "lat": 38.18, "lon": -122.37 },
                "path_termination": "DF",
                "departure_anchor_ref": { "Fix": "RW24" },
                "departure_anchor_position": { "lat": 38.22, "lon": -122.27 },
            })],
        )]);

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("an initial DF should start at its departure runway");

        assert_eq!(records.len(), 1);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn departure_segment_anchors_initial_fix_to_altitude_at_runway() {
        let mut legs =
            serde_json::from_value::<Vec<ProcedureLegMaterializationRecord>>(serde_json::json!([{
                "key": {
                    "airport_id": "KMSO",
                    "procedure_id": "GRZLY4",
                    "route_type": "1",
                    "transition_id": "RW30",
                },
                "sequence": 10,
                "nav_ref": { "Navaid": "MSO" },
                "nav_position": { "lat": 46.908, "lon": -114.084 },
                "defining_nav_ref": { "Navaid": "MSO" },
                "defining_nav_position": { "lat": 46.908, "lon": -114.084 },
                "path_termination": "FA",
                "magnetic_course_deg": 298.0,
                "altitude_1_ft": 7800.0,
                "departure_anchor_ref": { "Fix": "RW30" },
                "departure_anchor_position": { "lat": 46.916, "lon": -114.103 },
                "departure_anchor_altitude_ft": 3206.0,
            }]))
            .expect("test row should decode");

        anchor_departure_segment(&mut legs);

        assert_eq!(legs.len(), 2);
        assert_eq!(legs[0].path_termination, "IF");
        assert_eq!(legs[0].nav_ref, Some(NavRef::Fix("RW30".to_string())));
        assert_eq!(legs[0].magnetic_course_deg, Some(298.0));
        assert_eq!(legs[0].altitude_1_ft, Some(3206.0));
        assert_eq!(legs[1].path_termination, "FA");
    }

    #[test]
    fn build_procedure_geometry_records_anchors_cf_course_at_destination_fix() {
        let kinds = BTreeMap::from([(
            ("KAPC".to_string(), "OZIEE4".to_string()),
            ProcedureKind::Sid,
        )]);
        let distinct_by_procedure = BTreeMap::from([(
            ("KAPC".to_string(), "OZIEE4".to_string()),
            vec![serde_json::json!({
                "route_type": "4",
                "transition_id": "RW19L",
            })],
        )]);
        let materialization_by_procedure = BTreeMap::from([(
            ("KAPC".to_string(), "OZIEE4".to_string()),
            vec![
                serde_json::json!({
                    "key": {
                        "airport_id": "KAPC",
                        "procedure_id": "OZIEE4",
                        "route_type": "4",
                        "transition_id": "RW19L",
                    },
                    "sequence": 10,
                    "nav_ref": null,
                    "path_termination": "VI",
                    "magnetic_course_deg": 185.7,
                    "airport_magnetic_variation_deg": 15.0,
                    "departure_anchor_ref": { "Fix": "RW19L" },
                    "departure_anchor_position": { "lat": 38.21852341666667, "lon": -122.27830283333333 },
                }),
                serde_json::json!({
                    "key": {
                        "airport_id": "KAPC",
                        "procedure_id": "OZIEE4",
                        "route_type": "4",
                        "transition_id": "RW19L",
                    },
                    "sequence": 20,
                    "nav_ref": { "Fix": "KLARK" },
                    "nav_position": { "lat": 38.2167, "lon": -122.3996 },
                    "defining_nav_ref": { "Navaid": "HYP" },
                    "defining_nav_position": { "lat": 37.2194, "lon": -120.4002 },
                    "defining_nav_magnetic_variation_deg": 15.0,
                    "path_termination": "CF",
                    "magnetic_course_deg": 268.5,
                }),
                serde_json::json!({
                    "key": {
                        "airport_id": "KAPC",
                        "procedure_id": "OZIEE4",
                        "route_type": "4",
                        "transition_id": "RW19L",
                    },
                    "sequence": 30,
                    "nav_ref": { "Fix": "FIRTO" },
                    "nav_position": { "lat": 38.3136, "lon": -122.4067 },
                    "path_termination": "TF",
                }),
                serde_json::json!({
                    "key": {
                        "airport_id": "KAPC",
                        "procedure_id": "OZIEE4",
                        "route_type": "4",
                        "transition_id": "RW19L",
                    },
                    "sequence": 40,
                    "nav_ref": { "Fix": "OZIEE" },
                    "nav_position": { "lat": 38.4922, "lon": -122.3864 },
                    "path_termination": "TF",
                }),
            ],
        )]);

        let records = build_procedure_geometry_records(
            kinds,
            distinct_by_procedure,
            materialization_by_procedure,
        )
        .expect("CF geometry should use KLARK, not distant HYP, as its course-line anchor");

        assert_eq!(records.len(), 1);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_consumes_same_fix_fa_cf_reversal() {
        let key = ("KLGD".to_string(), "LGD1".to_string());
        let records = build_procedure_geometry_records(
            BTreeMap::from([(key.clone(), ProcedureKind::Sid)]),
            BTreeMap::from([(
                key.clone(),
                vec![serde_json::json!({
                    "route_type": "1",
                    "transition_id": "RW12",
                })],
            )]),
            BTreeMap::from([(
                key,
                vec![
                    serde_json::json!({
                        "key": {
                            "airport_id": "KLGD",
                            "procedure_id": "LGD1",
                            "route_type": "1",
                            "transition_id": "RW12",
                        },
                        "sequence": 10,
                        "nav_ref": null,
                        "path_termination": "VA",
                        "magnetic_course_deg": 121.0,
                        "airport_magnetic_variation_deg": 14.0,
                        "altitude_1_ft": 3118.0,
                        "departure_anchor_ref": { "Fix": "RW12" },
                        "departure_anchor_position": { "lat": 45.295549666666666, "lon": -118.01196477777778 },
                        "departure_anchor_altitude_ft": 2717.6,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KLGD",
                            "procedure_id": "LGD1",
                            "route_type": "1",
                            "transition_id": "RW12",
                        },
                        "sequence": 20,
                        "nav_ref": { "Navaid": "LGD" },
                        "nav_position": { "lat": 45.3424888888889, "lon": -117.962716666667 },
                        "defining_nav_ref": { "Navaid": "PDT" },
                        "defining_nav_position": { "lat": 45.6984222222222, "lon": -118.938711111111 },
                        "nav_magnetic_variation_deg": 14.0,
                        "defining_nav_magnetic_variation_deg": 14.0,
                        "path_termination": "CF",
                        "turn_direction": "L",
                        "magnetic_course_deg": 312.0,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KLGD",
                            "procedure_id": "LGD1",
                            "route_type": "1",
                            "transition_id": "RW12",
                        },
                        "sequence": 30,
                        "nav_ref": { "Navaid": "LGD" },
                        "nav_position": { "lat": 45.3424888888889, "lon": -117.962716666667 },
                        "defining_nav_ref": { "Navaid": "PDT" },
                        "defining_nav_position": { "lat": 45.6984222222222, "lon": -118.938711111111 },
                        "nav_magnetic_variation_deg": 14.0,
                        "defining_nav_magnetic_variation_deg": 14.0,
                        "path_termination": "FA",
                        "magnetic_course_deg": 342.0,
                        "altitude_1_ft": 7000.0,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KLGD",
                            "procedure_id": "LGD1",
                            "route_type": "1",
                            "transition_id": "RW12",
                        },
                        "sequence": 40,
                        "nav_ref": { "Navaid": "LGD" },
                        "nav_position": { "lat": 45.3424888888889, "lon": -117.962716666667 },
                        "defining_nav_ref": { "Navaid": "PDT" },
                        "defining_nav_position": { "lat": 45.6984222222222, "lon": -118.938711111111 },
                        "nav_magnetic_variation_deg": 14.0,
                        "defining_nav_magnetic_variation_deg": 14.0,
                        "path_termination": "CF",
                        "turn_direction": "R",
                        "magnetic_course_deg": 192.0,
                    }),
                ],
            )]),
        )
        .expect("same-fix FA/CF course reversal should be consumed as one SID path");

        assert_eq!(records.len(), 1);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_renders_hln5_rw27_reversal() {
        let key = ("KHLN".to_string(), "HLN5".to_string());
        let records = build_procedure_geometry_records(
            BTreeMap::from([(key.clone(), ProcedureKind::Sid)]),
            BTreeMap::from([(
                key.clone(),
                vec![serde_json::json!({
                    "route_type": "1",
                    "transition_id": "RW27",
                })],
            )]),
            BTreeMap::from([(
                key,
                vec![
                    serde_json::json!({
                        "key": {
                            "airport_id": "KHLN",
                            "procedure_id": "HLN5",
                            "route_type": "1",
                            "transition_id": "RW27",
                        },
                        "sequence": 10,
                        "nav_ref": null,
                        "path_termination": "VA",
                        "magnetic_course_deg": 270.7,
                        "airport_magnetic_variation_deg": 12.0,
                        "altitude_1_ft": 4277.0,
                        "departure_anchor_ref": { "Fix": "RW27" },
                        "departure_anchor_position": { "lat": 46.60298888888889, "lon": -111.96276527777778 },
                        "departure_anchor_altitude_ft": 3877.3,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KHLN",
                            "procedure_id": "HLN5",
                            "route_type": "1",
                            "transition_id": "RW27",
                        },
                        "sequence": 20,
                        "nav_ref": null,
                        "path_termination": "VI",
                        "turn_direction": "R",
                        "magnetic_course_deg": 10.0,
                        "airport_magnetic_variation_deg": 12.0,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KHLN",
                            "procedure_id": "HLN5",
                            "route_type": "1",
                            "transition_id": "RW27",
                        },
                        "sequence": 30,
                        "nav_ref": { "Navaid": "HLN" },
                        "nav_position": { "lat": 46.6068222222222, "lon": -111.953475 },
                        "defining_nav_ref": { "Navaid": "HLN" },
                        "defining_nav_position": { "lat": 46.6068222222222, "lon": -111.953475 },
                        "nav_magnetic_variation_deg": 12.0,
                        "defining_nav_magnetic_variation_deg": 12.0,
                        "path_termination": "FA",
                        "magnetic_course_deg": 340.0,
                        "altitude_1_ft": 8100.0,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KHLN",
                            "procedure_id": "HLN5",
                            "route_type": "1",
                            "transition_id": "RW27",
                        },
                        "sequence": 40,
                        "nav_ref": { "Navaid": "HLN" },
                        "nav_position": { "lat": 46.6068222222222, "lon": -111.953475 },
                        "defining_nav_ref": { "Navaid": "HLN" },
                        "defining_nav_position": { "lat": 46.6068222222222, "lon": -111.953475 },
                        "nav_magnetic_variation_deg": 12.0,
                        "defining_nav_magnetic_variation_deg": 12.0,
                        "path_termination": "CF",
                        "turn_direction": "L",
                        "magnetic_course_deg": 160.0,
                        "route_distance_or_time": "0150",
                    }),
                ],
            )]),
        )
        .expect("HLN5 RW27 should render its published outbound/inbound reversal");

        assert_eq!(records.len(), 1);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_renders_pxr1_grouped_runway_fa() {
        let key = ("KIWA".to_string(), "PXR1".to_string());
        let records = build_procedure_geometry_records(
            BTreeMap::from([(key.clone(), ProcedureKind::Sid)]),
            BTreeMap::from([(
                key.clone(),
                vec![serde_json::json!({
                    "route_type": "1",
                    "transition_id": "RW12B",
                })],
            )]),
            BTreeMap::from([(
                key,
                vec![
                    serde_json::json!({
                        "key": {
                            "airport_id": "KIWA",
                            "procedure_id": "PXR1",
                            "route_type": "1",
                            "transition_id": "RW12B",
                        },
                        "sequence": 10,
                        "nav_ref": { "Navaid": "IWA" },
                        "nav_position": { "lat": 33.3031694444444, "lon": -111.651436111111 },
                        "defining_nav_ref": { "Navaid": "IWA" },
                        "defining_nav_position": { "lat": 33.3031694444444, "lon": -111.651436111111 },
                        "nav_magnetic_variation_deg": 13.0,
                        "defining_nav_magnetic_variation_deg": 13.0,
                        "airport_magnetic_variation_deg": 13.0,
                        "path_termination": "FA",
                        "magnetic_course_deg": 122.0,
                        "altitude_1_ft": 2700.0,
                        "departure_anchor_ref": { "Airport": "KIWA" },
                        "departure_anchor_position": { "lat": 33.3078238888889, "lon": -111.655459166667 },
                        "departure_anchor_altitude_ft": 1384.1,
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": "KIWA",
                            "procedure_id": "PXR1",
                            "route_type": "1",
                            "transition_id": "RW12B",
                        },
                        "sequence": 20,
                        "nav_ref": { "Navaid": "PXR" },
                        "nav_position": { "lat": 33.4330166666667, "lon": -111.970202777778 },
                        "nav_magnetic_variation_deg": 12.0,
                        "path_termination": "DF",
                        "turn_direction": "R",
                    }),
                ],
            )]),
        )
        .expect("PXR1 RW12B should climb on IWA R-122 before turning direct PXR");

        assert_eq!(records.len(), 1);
        assert!(!records[0].leg_bundles.is_empty());
    }

    #[test]
    fn sid_vector_target_splits_runway_segment_before_resuming_at_fix() {
        let row = |sequence: i32, fix: Option<&str>, path_termination: &str| {
            serde_json::from_value::<ProcedureLegMaterializationRecord>(serde_json::json!({
                "key": {
                    "airport_id": "KJAX",
                    "procedure_id": "EXBOX2",
                    "route_type": "4",
                    "transition_id": "RW32",
                },
                "sequence": sequence,
                "nav_ref": fix.map(|fix| serde_json::json!({ "Fix": fix })),
                "nav_position": match fix {
                    Some("SAWGY") => Some(serde_json::json!({ "lat": 31.0, "lon": -81.7 })),
                    Some("EXBOX") => Some(serde_json::json!({ "lat": 30.8, "lon": -81.7 })),
                    _ => None,
                },
                "path_termination": path_termination,
            }))
            .expect("test procedure row should decode")
        };
        let mut segments = vec![
            (
                MaterializedSegmentRole::RunwayTransition,
                vec![
                    row(20, None, "VM"),
                    row(30, Some("SAWGY"), "DF"),
                    row(40, Some("EXBOX"), "TF"),
                ],
                Vec::new(),
                false,
            ),
            (
                MaterializedSegmentRole::EnrouteTransition,
                vec![row(10, Some("EXBOX"), "IF")],
                Vec::new(),
                false,
            ),
        ];

        normalize_sid_vector_targets(&mut segments);

        assert_eq!(segments.len(), 3, "vectors should split the runway route");
        assert_eq!(segments[0].1.last().unwrap().path_termination.trim(), "VM");
        assert_eq!(segments[1].1.first().unwrap().path_termination.trim(), "IF");
        assert_eq!(
            segments[1].1.first().unwrap().nav_ref,
            Some(NavRef::Fix("SAWGY".into()))
        );
        assert_eq!(
            segments[1].1.last().unwrap().nav_ref,
            Some(NavRef::Fix("EXBOX".into()))
        );
    }

    #[test]
    fn sid_heading_intercept_anchor_is_not_a_flown_fix_in_enroute_transition() {
        let row = |sequence: i32, fix: Option<&str>, path_termination: &str| {
            serde_json::from_value::<ProcedureLegMaterializationRecord>(serde_json::json!({
                "key": {
                    "airport_id": "KSMX",
                    "procedure_id": "BUELT4",
                    "route_type": "3",
                    "transition_id": "FRAMS",
                },
                "sequence": sequence,
                "nav_ref": fix.map(|fix| serde_json::json!({ "Fix": fix })),
                "nav_position": { "lat": 34.7, "lon": -120.3 },
                "path_termination": path_termination,
            }))
            .expect("test procedure row should decode")
        };
        let mut segments = vec![(
            MaterializedSegmentRole::EnrouteTransition,
            vec![
                row(20, None, "VI"),
                row(30, Some("BUELT"), "IF"),
                row(40, Some("GLJ"), "TF"),
            ],
            Vec::new(),
            false,
        )];

        normalize_sid_heading_intercept_anchors(&mut segments);

        assert_eq!(
            segments[0].1[1].nav_ref, None,
            "the IF anchors the intercepted course and must not become a flown fix",
        );
    }

    #[test]
    fn build_procedure_geometry_records_allows_published_sid_branch_turn() {
        let result = build_procedure_geometry_records(
            BTreeMap::from([(
                ("KAPC".to_string(), "LIZRD6".to_string()),
                ProcedureKind::Sid,
            )]),
            sid_branch_distinct_rows("KAPC", "LIZRD6"),
            sid_branch_materialization_rows("KAPC", "LIZRD6", 0.3572, 0.7660),
        );

        assert!(
            result.is_ok(),
            "published 130-degree SID branch: {result:?}"
        );
    }

    #[test]
    fn build_procedure_geometry_records_rejects_near_reversal_at_sid_branch() {
        let result = build_procedure_geometry_records(
            BTreeMap::from([(
                ("KZZZ".to_string(), "BADSID1".to_string()),
                ProcedureKind::Sid,
            )]),
            sid_branch_distinct_rows("KZZZ", "BADSID1"),
            sid_branch_materialization_rows("KZZZ", "BADSID1", 0.0152, 0.1736),
        );

        let error = result.expect_err("a 170-degree SID branch should be rejected");
        assert!(
            error.to_string().contains("instantaneous hairpin")
                || error.to_string().contains("heading continuity violated"),
            "unexpected rejection: {error:#}"
        );
    }

    #[test]
    fn audit_collects_every_materialization_rejection() {
        let mut kinds = BTreeMap::new();
        let mut distinct = BTreeMap::new();
        let mut materialization = BTreeMap::new();
        for (airport_id, procedure_id) in [("KZZ1", "BADSID1"), ("KZZ2", "BADSID2")] {
            kinds.insert(
                (airport_id.to_string(), procedure_id.to_string()),
                ProcedureKind::Sid,
            );
            distinct.extend(sid_branch_distinct_rows(airport_id, procedure_id));
            materialization.extend(sid_branch_materialization_rows(
                airport_id,
                procedure_id,
                0.0152,
                0.1736,
            ));
        }

        let audit = audit_procedure_geometry_records(kinds, distinct, materialization)
            .expect("audit should enumerate invalid choices instead of aborting");

        assert_eq!(audit.materialization_rejections.len(), 2);
        assert!(audit.records.is_empty());
    }

    #[test]
    fn build_procedure_geometry_records_publishes_star() {
        let airport_id = "KSEA";
        let procedure_id = "HAWKZ7";
        let result = build_procedure_geometry_records(
            BTreeMap::from([(
                (airport_id.to_string(), procedure_id.to_string()),
                ProcedureKind::Star,
            )]),
            BTreeMap::from([(
                (airport_id.to_string(), procedure_id.to_string()),
                vec![serde_json::json!({
                    "route_type": "2",
                    "transition_id": "",
                })],
            )]),
            BTreeMap::from([(
                (airport_id.to_string(), procedure_id.to_string()),
                vec![
                    serde_json::json!({
                        "key": {
                            "airport_id": airport_id,
                            "procedure_id": procedure_id,
                            "route_type": "2",
                            "transition_id": "",
                        },
                        "sequence": 10,
                        "nav_ref": { "Fix": "HAWKZ" },
                        "nav_position": { "lat": 47.0, "lon": -122.5 },
                        "path_termination": "IF",
                    }),
                    serde_json::json!({
                        "key": {
                            "airport_id": airport_id,
                            "procedure_id": procedure_id,
                            "route_type": "2",
                            "transition_id": "",
                        },
                        "sequence": 20,
                        "nav_ref": { "Fix": "MARNR" },
                        "nav_position": { "lat": 47.2, "lon": -122.3 },
                        "path_termination": "TF",
                    }),
                ],
            )]),
        )
        .expect("valid STAR geometry should build");

        assert_eq!(result.len(), 1, "valid STAR should be published");
        assert_eq!(result[0].key.kind, pgt::ProcedureKind::Star);
    }

    fn sid_branch_distinct_rows(
        airport_id: &str,
        procedure_id: &str,
    ) -> BTreeMap<(String, String), Vec<serde_json::Value>> {
        BTreeMap::from([(
            (airport_id.to_string(), procedure_id.to_string()),
            vec![
                serde_json::json!({ "route_type": "4", "transition_id": "RW01B" }),
                serde_json::json!({ "route_type": "6", "transition_id": "PYE" }),
            ],
        )])
    }

    fn sid_branch_materialization_rows(
        airport_id: &str,
        procedure_id: &str,
        outbound_lat: f64,
        outbound_lon: f64,
    ) -> BTreeMap<(String, String), Vec<serde_json::Value>> {
        let leg = |route_type: &str,
                   transition_id: &str,
                   sequence: i32,
                   fix: &str,
                   lat: f64,
                   lon: f64,
                   path_termination: &str| {
            serde_json::json!({
                "key": {
                    "airport_id": airport_id,
                    "procedure_id": procedure_id,
                    "route_type": route_type,
                    "transition_id": transition_id,
                },
                "sequence": sequence,
                "nav_ref": { "Fix": fix },
                "nav_position": { "lat": lat, "lon": lon },
                "path_termination": path_termination,
            })
        };
        BTreeMap::from([(
            (airport_id.to_string(), procedure_id.to_string()),
            vec![
                leg("4", "RW01B", 10, "SGD", 0.0, 0.0, "IF"),
                leg("4", "RW01B", 20, "LIZRD", 1.0, 0.0, "TF"),
                leg("6", "PYE", 10, "LIZRD", 1.0, 0.0, "IF"),
                leg("6", "PYE", 20, "LAKCI", outbound_lat, outbound_lon, "TF"),
            ],
        )])
    }

    fn sid_test_leg(
        route_type: &str,
        transition_id: &str,
        sequence: i32,
        fix: &str,
        position: LatLon,
        path_termination: &str,
        turn_direction: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "key": {
                "airport_id": "KSEA",
                "procedure_id": "BANGR9",
                "route_type": route_type,
                "transition_id": transition_id,
            },
            "sequence": sequence,
            "nav_ref": { "Fix": fix },
                "nav_position": { "lat": position.lat, "lon": position.lon },
            "path_termination": path_termination,
            "turn_direction": turn_direction,
        })
    }
}
