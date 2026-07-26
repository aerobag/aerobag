// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::errors::{AppError, AppErrorKind, AppResult};
use crate::geodesy::initial_course_deg;
use crate::geometry::LatLon;
use crate::ids::AirportId;
use crate::map_overlay::{NavSymbolFeature, WeatherDetailUiView};
use crate::{FlightDataCell, FlightDataCellTone, FlightDataColumn};

pub(crate) const OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON: &str =
    "Restore FP before editing the flight plan.";
const AIRWAY_ENDPOINT_REMOVE_DISABLED_REASON: &str = "Only airway endpoints can be removed.";
const WAYPOINT_REMOVE_DISABLED_REASON: &str =
    "This waypoint cannot be removed from the flight plan.";
const AIRWAY_REMOVE_DISABLED_REASON: &str = "This airway cannot be removed from the flight plan.";
const PROCEDURE_REMOVE_DISABLED_REASON: &str =
    "This procedure cannot be removed from the flight plan.";
const REMOVE_ALL_ABOVE_DISABLED_REASON: &str =
    "This row cannot be used as a Remove All Above target.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlan {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub route_components: Vec<RouteComponent>,
    pub route_component_uids: Vec<String>,
    pub route_component_uid_counter: u64,
    #[serde(default)]
    pub resolved_legs: Vec<ResolvedLeg>,
    #[serde(default)]
    pub guidance: Option<GuidanceState>,
    pub departure: Option<AirportId>,
    pub destination: Option<AirportId>,
    pub alternate: Option<AirportId>,
    pub cruise_altitude_ft: Option<i32>,
    pub notes: Option<String>,
    pub updated_at_epoch_ms: i64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanLeg {
    pub from: NavRef,
    pub to: NavRef,
    pub airway: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteComponent {
    Waypoint { waypoint: NavRef },
    Airway { airway: AirwaySegment },
    Procedure { procedure: ProcedureSegment },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwaySegment {
    pub name: String,
    #[serde(default)]
    pub branch_key: Option<String>,
    pub entry: NavRef,
    pub exit: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureSegment {
    pub airport_id: AirportId,
    pub procedure_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    #[serde(default)]
    pub terminal_discontinuity: Option<ProcedureDiscontinuity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_quality: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathTermination {
    InitialFix,
    TrackToFix,
    CourseToFix,
    DirectToFix,
    HeadingToManual,
    HeadingToAltitude,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    Sid,
    Star,
    Approach,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureDiscontinuity {
    Vectors,
    Hold,
    Other(String),
}

impl ProcedureDiscontinuity {
    pub fn display_label(&self) -> &str {
        match self {
            ProcedureDiscontinuity::Vectors => "VECTORS",
            ProcedureDiscontinuity::Hold => "HOLD",
            ProcedureDiscontinuity::Other(label) => label.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConcretizedNavItem {
    Waypoint {
        nav_ref: NavRef,
    },
    Discontinuity {
        discontinuity: ProcedureDiscontinuity,
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLeg {
    pub id: String,
    pub from: NavRef,
    pub to: NavRef,
    pub source: ResolvedLegSource,
    pub procedure_provenance: Option<ProcedureLegProvenance>,
}

#[derive(Serialize, Deserialize)]
struct ResolvedLegSerde {
    id: String,
    from: NavRef,
    to: NavRef,
    source: ResolvedLegSource,
    #[serde(default)]
    procedure_airport_id: Option<String>,
    #[serde(default)]
    procedure_provenance: Option<ProcedureLegProvenance>,
}

impl Serialize for ResolvedLeg {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ResolvedLegSerde {
            id: self.id.clone(),
            from: self.from.clone(),
            to: self.to.clone(),
            source: self.source.clone(),
            procedure_airport_id: self
                .procedure_provenance
                .as_ref()
                .map(|provenance| provenance.airport_id.clone()),
            procedure_provenance: self.procedure_provenance.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ResolvedLeg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ResolvedLegSerde::deserialize(deserializer)?;
        Ok(Self {
            id: raw.id,
            from: raw.from,
            to: raw.to,
            source: raw.source,
            procedure_provenance: raw.procedure_provenance.or_else(|| {
                raw.procedure_airport_id
                    .map(|airport_id| ProcedureLegProvenance {
                        airport_id,
                        procedure_id: String::new(),
                        kind: ProcedureKind::Approach,
                        role: ProcedureSegmentRole::Common,
                        path_termination: PathTermination::Other(String::new()),
                        leg_sequence: 0,
                        display_path: None,
                    })
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedLegSource {
    RouteComponent {
        component_index: usize,
    },
    SyntheticBridge {
        from_component_index: usize,
        to_component_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureLegProvenance {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub role: ProcedureSegmentRole,
    pub path_termination: PathTermination,
    pub leg_sequence: i32,
    #[serde(default)]
    pub display_path: Option<LegDisplayPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegDisplayPath {
    #[serde(default)]
    pub style: LegDisplayPathStyle,
    pub elements: Vec<LegDisplayElement>,
    #[serde(default)]
    pub effective_terminal_course_deg: Option<f64>,
    #[serde(skip)]
    pub debug_element_sources: Vec<String>,
    #[serde(skip)]
    pub debug_element_roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalState {
    pub terminal_position: LatLon,
    #[serde(default)]
    pub drawn_terminal_course_deg: Option<f64>,
    #[serde(default)]
    pub logical_terminal_course_deg: Option<f64>,
    #[serde(default)]
    pub terminal_anchor: Option<NavRef>,
    #[serde(default)]
    pub established_course_deg: Option<f64>,
    #[serde(default)]
    pub incoming_course_to_anchor_deg: Option<f64>,
    #[serde(default)]
    pub outgoing_course_from_anchor_deg: Option<f64>,
    pub hold_state: HoldTerminalState,
    pub procedure_turn_state: ProcedureTurnTerminalState,
    pub common_segment_state: CommonSegmentTerminalState,
    pub coded_fix_satisfaction: CodedFixSatisfaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldTerminalState {
    None,
    HoldLeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureTurnTerminalState {
    None,
    ProcedureTurnLeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonSegmentTerminalState {
    NotCommon,
    CommonSegment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodedFixSatisfaction {
    Unknown,
    AtAnchor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartRequirement {
    AtFix {
        anchor: NavRef,
        #[serde(default)]
        anchor_position: Option<LatLon>,
    },
    DirectToFix {
        anchor: NavRef,
        #[serde(default)]
        anchor_position: Option<LatLon>,
        #[serde(default)]
        continuation_course_deg: Option<f64>,
        #[serde(default)]
        continuation_anchor: Option<NavRef>,
        #[serde(default)]
        continuation_anchor_position: Option<LatLon>,
    },
    YieldableCourseToFix {
        anchor: NavRef,
        #[serde(default)]
        anchor_position: Option<LatLon>,
        continuation_course_deg: Option<f64>,
        continuation_anchor: Option<NavRef>,
        #[serde(default)]
        continuation_anchor_position: Option<LatLon>,
    },
    ReentryToAnchor {
        from_anchor: NavRef,
        #[serde(default)]
        from_anchor_position: Option<LatLon>,
        to_anchor: NavRef,
    },
    EstablishedOnCourse {
        course_deg: f64,
        anchor: Option<NavRef>,
        #[serde(default)]
        anchor_position: Option<LatLon>,
    },
    InterceptCourse {
        course_deg: f64,
        anchor: Option<NavRef>,
        #[serde(default)]
        anchor_position: Option<LatLon>,
    },
    ResumeCommonSegment {
        anchor: Option<NavRef>,
        course_deg: Option<f64>,
        #[serde(default)]
        anchor_position: Option<LatLon>,
        #[serde(default)]
        target_anchor: Option<NavRef>,
        #[serde(default)]
        target_anchor_position: Option<LatLon>,
    },
    EnterHold {
        anchor: NavRef,
        inbound_course_deg: Option<f64>,
        #[serde(default)]
        anchor_position: Option<LatLon>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffDecision {
    ContinueAsDrawn,
    ResumeAtAnchor,
    ResumeThroughAnchorKink,
    SkipStaleFix,
    YieldToFollowingCourse,
    BuildJoinGeometry,
    EnterHold,
    Invalid,
}

fn normalize_bearing_degrees(bearing_deg: f64) -> f64 {
    bearing_deg.rem_euclid(360.0)
}

fn angular_difference_degrees(left: f64, right: f64) -> f64 {
    let mut delta = (normalize_bearing_degrees(left) - normalize_bearing_degrees(right)).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    delta
}

fn local_to_en(origin: LatLon, point: LatLon) -> (f64, f64) {
    let lat_scale_nm = 60.0;
    let mean_lat_rad = ((origin.lat + point.lat) * 0.5).to_radians();
    let lon_scale_nm = 60.0 * mean_lat_rad.cos();
    (
        (point.lon - origin.lon) * lon_scale_nm,
        (point.lat - origin.lat) * lat_scale_nm,
    )
}

fn course_unit_vector(course_deg: f64) -> (f64, f64) {
    let radians = course_deg.to_radians();
    (radians.sin(), radians.cos())
}

fn positions_nearly_equal(left: LatLon, right: LatLon) -> bool {
    let lat_delta_nm = (left.lat - right.lat).abs() * 60.0;
    let mean_lat_rad = ((left.lat + right.lat) * 0.5).to_radians();
    let lon_delta_nm = (left.lon - right.lon).abs() * 60.0 * mean_lat_rad.cos();
    (lat_delta_nm * lat_delta_nm + lon_delta_nm * lon_delta_nm).sqrt() <= 0.05
}

pub fn basic_terminal_state(
    terminal_position: LatLon,
    terminal_course_deg: Option<f64>,
    terminal_anchor: Option<NavRef>,
    common_segment: bool,
) -> TerminalState {
    TerminalState {
        terminal_position,
        drawn_terminal_course_deg: terminal_course_deg,
        logical_terminal_course_deg: terminal_course_deg,
        terminal_anchor,
        established_course_deg: terminal_course_deg,
        incoming_course_to_anchor_deg: terminal_course_deg,
        outgoing_course_from_anchor_deg: terminal_course_deg,
        hold_state: HoldTerminalState::None,
        procedure_turn_state: ProcedureTurnTerminalState::None,
        common_segment_state: if common_segment {
            CommonSegmentTerminalState::CommonSegment
        } else {
            CommonSegmentTerminalState::NotCommon
        },
        coded_fix_satisfaction: CodedFixSatisfaction::Unknown,
    }
}

pub fn resume_probe_terminal_state(
    terminal_position: LatLon,
    terminal_course_deg: f64,
    incoming_course_to_anchor_deg: Option<f64>,
    hold_like: bool,
) -> TerminalState {
    TerminalState {
        terminal_position,
        drawn_terminal_course_deg: Some(terminal_course_deg),
        logical_terminal_course_deg: Some(terminal_course_deg),
        terminal_anchor: None,
        established_course_deg: Some(terminal_course_deg),
        incoming_course_to_anchor_deg,
        outgoing_course_from_anchor_deg: Some(terminal_course_deg),
        hold_state: if hold_like {
            HoldTerminalState::HoldLeg
        } else {
            HoldTerminalState::None
        },
        procedure_turn_state: ProcedureTurnTerminalState::None,
        common_segment_state: CommonSegmentTerminalState::NotCommon,
        coded_fix_satisfaction: CodedFixSatisfaction::Unknown,
    }
}

pub fn terminal_state_with_leg_characteristics(
    terminal_position: LatLon,
    drawn_terminal_course_deg: Option<f64>,
    logical_terminal_course_deg: Option<f64>,
    terminal_anchor: Option<NavRef>,
    role: ProcedureSegmentRole,
    path_termination: &PathTermination,
) -> TerminalState {
    TerminalState {
        terminal_position,
        drawn_terminal_course_deg,
        logical_terminal_course_deg,
        terminal_anchor,
        established_course_deg: logical_terminal_course_deg,
        incoming_course_to_anchor_deg: drawn_terminal_course_deg,
        outgoing_course_from_anchor_deg: logical_terminal_course_deg,
        hold_state: if matches!(path_termination, PathTermination::Other(code) if matches!(code.trim(), "HF" | "HM"))
        {
            HoldTerminalState::HoldLeg
        } else {
            HoldTerminalState::None
        },
        procedure_turn_state: if matches!(path_termination, PathTermination::Other(code) if code.trim() == "PI")
        {
            ProcedureTurnTerminalState::ProcedureTurnLeg
        } else {
            ProcedureTurnTerminalState::None
        },
        common_segment_state: if role == ProcedureSegmentRole::Common {
            CommonSegmentTerminalState::CommonSegment
        } else {
            CommonSegmentTerminalState::NotCommon
        },
        coded_fix_satisfaction: CodedFixSatisfaction::AtAnchor,
    }
}

pub fn at_fix_requirement(anchor: NavRef, anchor_position: Option<LatLon>) -> StartRequirement {
    StartRequirement::AtFix {
        anchor,
        anchor_position,
    }
}

pub fn established_on_course_requirement(
    course_deg: f64,
    anchor: Option<NavRef>,
    anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::EstablishedOnCourse {
        course_deg,
        anchor,
        anchor_position,
    }
}

pub fn intercept_course_requirement(
    course_deg: f64,
    anchor: Option<NavRef>,
    anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::InterceptCourse {
        course_deg,
        anchor,
        anchor_position,
    }
}

pub fn direct_to_fix_with_course_continuation_requirement(
    anchor: NavRef,
    anchor_position: Option<LatLon>,
    continuation_course_deg: Option<f64>,
    continuation_anchor: Option<NavRef>,
    continuation_anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::DirectToFix {
        anchor,
        anchor_position,
        continuation_course_deg,
        continuation_anchor,
        continuation_anchor_position,
    }
}

pub fn resume_common_segment_requirement(
    anchor: Option<NavRef>,
    course_deg: Option<f64>,
    anchor_position: Option<LatLon>,
    target_anchor: Option<NavRef>,
    target_anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::ResumeCommonSegment {
        anchor,
        course_deg,
        anchor_position,
        target_anchor,
        target_anchor_position,
    }
}

pub fn yieldable_course_to_fix_requirement(
    anchor: NavRef,
    anchor_position: Option<LatLon>,
    continuation_course_deg: Option<f64>,
    continuation_anchor: Option<NavRef>,
    continuation_anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::YieldableCourseToFix {
        anchor,
        anchor_position,
        continuation_course_deg,
        continuation_anchor,
        continuation_anchor_position,
    }
}

pub fn enter_hold_requirement(
    anchor: NavRef,
    inbound_course_deg: Option<f64>,
    anchor_position: Option<LatLon>,
) -> StartRequirement {
    StartRequirement::EnterHold {
        anchor,
        inbound_course_deg,
        anchor_position,
    }
}

pub fn start_requirement_from_leg_characteristics(
    path_termination: &PathTermination,
    anchor: NavRef,
    anchor_position: Option<LatLon>,
    terminal_course_deg: Option<f64>,
) -> StartRequirement {
    match path_termination {
        PathTermination::InitialFix | PathTermination::TrackToFix => {
            at_fix_requirement(anchor, anchor_position)
        }
        PathTermination::CourseToFix => established_on_course_requirement(
            terminal_course_deg.unwrap_or(0.0),
            Some(anchor),
            anchor_position,
        ),
        PathTermination::DirectToFix => direct_to_fix_with_course_continuation_requirement(
            anchor,
            anchor_position,
            None,
            None,
            None,
        ),
        PathTermination::HeadingToManual | PathTermination::HeadingToAltitude => {
            established_on_course_requirement(
                terminal_course_deg.unwrap_or(0.0),
                Some(anchor),
                anchor_position,
            )
        }
        PathTermination::Other(label) if matches!(label.trim(), "HF" | "HM") => {
            enter_hold_requirement(anchor, terminal_course_deg, anchor_position)
        }
        PathTermination::Other(label) if label.trim() == "PI" => intercept_course_requirement(
            terminal_course_deg.unwrap_or(0.0),
            Some(anchor),
            anchor_position,
        ),
        PathTermination::Other(_) => resume_common_segment_requirement(
            Some(anchor.clone()),
            terminal_course_deg,
            anchor_position,
            Some(anchor),
            anchor_position,
        ),
    }
}

pub fn common_resume_candidate_decision(
    terminal_position: LatLon,
    terminal_course_deg: f64,
    incoming_course_to_anchor_deg: Option<f64>,
    previous_was_hold_like: bool,
    anchor: Option<NavRef>,
    course_deg: f64,
    anchor_position: LatLon,
    target_anchor: Option<NavRef>,
    target_anchor_position: LatLon,
) -> HandoffDecision {
    let terminal_state = resume_probe_terminal_state(
        terminal_position,
        terminal_course_deg,
        incoming_course_to_anchor_deg,
        previous_was_hold_like,
    );
    let start_requirement = resume_common_segment_requirement(
        anchor,
        Some(course_deg),
        Some(anchor_position),
        target_anchor,
        Some(target_anchor_position),
    );
    reconcile_handoff(&terminal_state, &start_requirement)
}

pub fn reentry_to_anchor_requirement(
    from_anchor: NavRef,
    from_anchor_position: Option<LatLon>,
    to_anchor: NavRef,
) -> StartRequirement {
    StartRequirement::ReentryToAnchor {
        from_anchor,
        from_anchor_position,
        to_anchor,
    }
}

pub fn reconcile_handoff(
    terminal_state: &TerminalState,
    start_requirement: &StartRequirement,
) -> HandoffDecision {
    match start_requirement {
        StartRequirement::DirectToFix {
            anchor_position: Some(direct_fix),
            continuation_course_deg: Some(continuation_course_deg),
            continuation_anchor_position: Some(continuation_anchor_position),
            ..
        } => {
            let Some(current_course_deg) = terminal_state
                .logical_terminal_course_deg
                .or(terminal_state.drawn_terminal_course_deg)
            else {
                return HandoffDecision::ContinueAsDrawn;
            };
            if angular_difference_degrees(current_course_deg, *continuation_course_deg) > 25.0 {
                return HandoffDecision::ContinueAsDrawn;
            }
            let bearing_to_continuation = initial_course_deg(
                terminal_state.terminal_position,
                *continuation_anchor_position,
            );
            if angular_difference_degrees(bearing_to_continuation, *continuation_course_deg) > 45.0
            {
                return HandoffDecision::ContinueAsDrawn;
            }
            let bearing_to_direct_fix =
                initial_course_deg(terminal_state.terminal_position, *direct_fix);
            let reciprocal_course_deg = normalize_bearing_degrees(*continuation_course_deg + 180.0);
            if angular_difference_degrees(bearing_to_direct_fix, reciprocal_course_deg) <= 45.0 {
                HandoffDecision::SkipStaleFix
            } else {
                HandoffDecision::ContinueAsDrawn
            }
        }
        StartRequirement::YieldableCourseToFix {
            anchor_position: Some(stale_fix),
            continuation_course_deg: Some(continuation_course_deg),
            continuation_anchor,
            continuation_anchor_position,
            ..
        } => {
            let Some(current_course_deg) = terminal_state
                .logical_terminal_course_deg
                .or(terminal_state.drawn_terminal_course_deg)
            else {
                return HandoffDecision::ContinueAsDrawn;
            };
            if angular_difference_degrees(current_course_deg, *continuation_course_deg) > 25.0 {
                return HandoffDecision::ContinueAsDrawn;
            }
            let reciprocal_course_deg = normalize_bearing_degrees(*continuation_course_deg + 180.0);
            let bearing_to_stale_fix =
                initial_course_deg(terminal_state.terminal_position, *stale_fix);
            if angular_difference_degrees(bearing_to_stale_fix, reciprocal_course_deg) > 45.0 {
                return HandoffDecision::ContinueAsDrawn;
            }
            if continuation_anchor
                .as_ref()
                .zip(terminal_state.terminal_anchor.as_ref())
                .is_some_and(|(continuation_anchor, terminal_anchor)| {
                    continuation_anchor == terminal_anchor
                })
            {
                return HandoffDecision::YieldToFollowingCourse;
            }
            let Some(continuation_anchor_position) = continuation_anchor_position else {
                return HandoffDecision::ContinueAsDrawn;
            };
            let bearing_to_continuation = initial_course_deg(
                terminal_state.terminal_position,
                *continuation_anchor_position,
            );
            if angular_difference_degrees(bearing_to_continuation, *continuation_course_deg) <= 45.0
            {
                HandoffDecision::YieldToFollowingCourse
            } else {
                HandoffDecision::ContinueAsDrawn
            }
        }
        StartRequirement::ReentryToAnchor {
            from_anchor_position: Some(from_anchor_position),
            to_anchor,
            ..
        } => {
            let Some(current_course_deg) = terminal_state
                .logical_terminal_course_deg
                .or(terminal_state.drawn_terminal_course_deg)
            else {
                return HandoffDecision::ContinueAsDrawn;
            };
            if terminal_state.terminal_anchor.as_ref() != Some(to_anchor) {
                return HandoffDecision::ContinueAsDrawn;
            }
            let heading_to_from_anchor =
                initial_course_deg(terminal_state.terminal_position, *from_anchor_position);
            if angular_difference_degrees(current_course_deg, heading_to_from_anchor) > 10.0 {
                HandoffDecision::SkipStaleFix
            } else {
                HandoffDecision::ContinueAsDrawn
            }
        }
        StartRequirement::ResumeCommonSegment {
            course_deg: Some(course_deg),
            anchor_position: Some(course_anchor_position),
            target_anchor_position: Some(target_fix_position),
            ..
        } => {
            let Some(current_course_deg) = terminal_state
                .logical_terminal_course_deg
                .or(terminal_state.drawn_terminal_course_deg)
            else {
                return HandoffDecision::ContinueAsDrawn;
            };
            let offset = local_to_en(*course_anchor_position, terminal_state.terminal_position);
            let course_unit = course_unit_vector(*course_deg);
            let normal = (-course_unit.1, course_unit.0);
            let cross_track_nm = (offset.0 * normal.0 + offset.1 * normal.1).abs();
            if cross_track_nm > 0.5 {
                return HandoffDecision::ContinueAsDrawn;
            }
            let anchored_at_course_anchor =
                positions_nearly_equal(terminal_state.terminal_position, *course_anchor_position)
                    && terminal_state.incoming_course_to_anchor_deg.is_some_and(
                        |incoming_course_deg| {
                            angular_difference_degrees(current_course_deg, incoming_course_deg)
                                <= 20.0
                        },
                    );
            if terminal_state.hold_state != HoldTerminalState::HoldLeg
                && !anchored_at_course_anchor
                && angular_difference_degrees(current_course_deg, *course_deg) > 20.0
            {
                return HandoffDecision::ContinueAsDrawn;
            }
            let bearing_to_target =
                initial_course_deg(terminal_state.terminal_position, *target_fix_position);
            if angular_difference_degrees(bearing_to_target, *course_deg) > 45.0 {
                return HandoffDecision::ContinueAsDrawn;
            }
            if anchored_at_course_anchor {
                HandoffDecision::ResumeThroughAnchorKink
            } else {
                HandoffDecision::ResumeAtAnchor
            }
        }
        _ => HandoffDecision::ContinueAsDrawn,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LegDisplayPathStyle {
    #[default]
    Solid,
    Dashed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LegDisplayElement {
    Segment {
        start: LatLon,
        end: LatLon,
    },
    Arc {
        center: LatLon,
        radius_nm: f64,
        start: LatLon,
        end: LatLon,
        clockwise: bool,
        sweep_degrees: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceState {
    pub active_leg_index: usize,
    #[serde(default)]
    pub active_detail_index: Option<usize>,
    #[serde(default)]
    pub display_split_leg_id: Option<String>,
    pub sequencing_mode: SequencingMode,
    pub direct_to: Option<DirectToState>,
    #[serde(default)]
    pub suspend_reason: Option<SuspendReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FlightPlanRowId(pub String);

impl FlightPlanRowId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequencingMode {
    FollowPlan,
    Suspended,
    DirectTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuspendReason {
    Manual,
    Boundary,
    RouteEnd,
    DirectToComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DirectToTargetRow {
    Planned { row_id: FlightPlanRowId },
    Temporary { row_id: FlightPlanRowId },
}

impl DirectToTargetRow {
    pub fn row_id(&self) -> &FlightPlanRowId {
        match self {
            Self::Planned { row_id } | Self::Temporary { row_id } => row_id,
        }
    }

    pub fn is_planned(&self) -> bool {
        matches!(self, Self::Planned { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectToState {
    pub start: NavRef,
    pub target: NavRef,
    pub target_row: DirectToTargetRow,
    pub resume_row_id: Option<FlightPlanRowId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanUiState {
    pub components: Vec<RouteComponentUiView>,
    pub resolved_legs: Vec<ResolvedLegUiView>,
    #[serde(default)]
    pub data_columns: Vec<FlightDataColumn>,
    pub display_rows: Vec<FlightPlanDisplayRowUiView>,
    #[serde(default)]
    pub controls: Vec<FlightPlanControlUiView>,
    pub guidance: Option<GuidanceUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightPlanControlId {
    ActivateNextLeg,
    RestoreDirectTo,
    SequenceActiveLeg,
    StopNavigation,
    SuspendSequencing,
    UnsuspendSequencing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightPlanControlUiView {
    pub id: FlightPlanControlId,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightPlanDisplayRowKind {
    Waypoint,
    Group,
    Discontinuity,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightPlanRowActionId {
    ActivateLeg,
    DirectTo,
    Remove,
    RemoveAllAbove,
    InsertBefore,
    InsertAfter,
    MoveUp,
    MoveDown,
    WaypointInfo,
    Weather,
    AddAirway,
    SelectProcedure,
    Plates,
    ShowPlate,
    RemoveAirway,
    RemoveProcedure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightPlanRowActionExecution {
    UiController,
    CoreSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlightPlanRowNavigationAction {
    OpenAirportCharts { airport_id: String },
    OpenPlateTarget { airport_id: String, target: String },
}

fn default_row_action_execution() -> FlightPlanRowActionExecution {
    FlightPlanRowActionExecution::UiController
}

fn default_dismiss_tray_on_success() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightPlanRowActionUiView {
    pub id: FlightPlanRowActionId,
    #[serde(default)]
    pub uid: String,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default = "default_row_action_execution")]
    pub execution: FlightPlanRowActionExecution,
    #[serde(default = "default_dismiss_tray_on_success")]
    pub dismiss_tray_on_success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<FlightPlanRowNavigationAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather_detail: Option<WeatherDetailUiView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanDisplayRowUiView {
    #[serde(default)]
    pub uid: String,
    pub label: String,
    pub row_kind: FlightPlanDisplayRowKind,
    pub component_kind: Option<RouteComponentViewKind>,
    pub component_uid: Option<String>,
    pub component_index: Option<usize>,
    pub procedure_id: Option<String>,
    pub procedure_kind: Option<ProcedureKind>,
    pub leg_index: Option<usize>,
    #[serde(default)]
    pub data_cells: Vec<FlightDataCell>,
    #[serde(default)]
    pub show_plate_target_id: Option<String>,
    pub chart_airport_id: Option<String>,
    pub nav_ref: Option<NavRef>,
    #[serde(default)]
    pub symbol_feature: Option<NavSymbolFeature>,
    pub depth: usize,
    pub active: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub synthetic_direct_to: bool,
    pub can_add_airway_after: bool,
    pub can_add_procedure_before: bool,
    pub can_remove_component: bool,
    pub can_reorder_component: bool,
    pub can_reorder_up: bool,
    pub can_reorder_down: bool,
    pub replace_procedure_component_index: Option<usize>,
    pub start_component_index: Option<usize>,
    pub end_component_index: Option<usize>,
    pub origin_anchor: Option<NavRef>,
    pub destination_anchor: Option<NavRef>,
    pub preceding_waypoint: Option<NavRef>,
    pub following_waypoint: Option<NavRef>,
    #[serde(default)]
    pub action_matrix: Vec<Vec<FlightPlanRowActionUiView>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteComponentViewKind {
    Waypoint,
    Airway,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteComponentUiView {
    pub uid: String,
    pub component_index: usize,
    pub kind: RouteComponentViewKind,
    pub summary: String,
    pub procedure_id: Option<String>,
    pub procedure_kind: Option<ProcedureKind>,
    pub chart_airport_id: Option<String>,
    pub nav_ref: Option<NavRef>,
    pub items: Vec<ConcretizedNavItem>,
    pub active: bool,
    pub can_add_airway_after: bool,
    pub can_add_procedure_before: bool,
    pub can_remove: bool,
    pub can_reorder: bool,
    pub can_reorder_up: bool,
    pub can_reorder_down: bool,
    pub replace_procedure_component_index: Option<usize>,
    pub preceding_waypoint: Option<NavRef>,
    pub following_waypoint: Option<NavRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLegUiView {
    pub leg_index: usize,
    pub leg_id: String,
    pub component_index: Option<usize>,
    pub from: NavRef,
    pub to: NavRef,
    pub active: bool,
    pub suspend_boundary_after: bool,
    pub display_path: Option<LegDisplayPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceUiView {
    pub sequencing_mode: SequencingMode,
    pub active_leg_index: Option<usize>,
    pub active_from_row_uid: Option<String>,
    pub active_to_row_uid: Option<String>,
    pub active_component_index: Option<usize>,
    pub active_leg: Option<PlanLeg>,
    #[serde(default)]
    pub nav_element: NavElementUiView,
    pub direct_to: Option<DirectToUiView>,
    pub suspend_boundary_after_active_leg: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NavElementUiView {
    pub active_leg_summary: String,
    pub cdi_indicator_dots: Option<f32>,
    pub cdi_offscale_readout: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectToUiView {
    pub start: NavRef,
    pub target: NavRef,
    pub target_row_id: FlightPlanRowId,
    pub on_plan_target: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavRef {
    Airport(String),
    Navaid(String),
    ArincNavaid {
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
    TerminalNavaid {
        airport_id: String,
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
    Fix(String),
    LatLon(LatLon),
    Spot(LatLon),
}

impl NavRef {
    pub fn airport_code(&self) -> Option<&str> {
        match self {
            NavRef::Airport(code) => Some(code.as_str()),
            _ => None,
        }
    }
}

fn default_true() -> bool {
    true
}

impl FlightPlan {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn normalized(mut self) -> Self {
        normalize_route_component_uids(&mut self);
        if self.resolved_legs.is_empty() && !self.route_components.is_empty() {
            self.resolved_legs = resolved_legs_from_waypoint_components(&self.route_components);
        }

        self
    }
}

impl Default for FlightPlan {
    fn default() -> Self {
        Self {
            id: "plan-empty".to_string(),
            name: "Flight Plan".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }
}

fn normalize_route_component_uids(plan: &mut FlightPlan) {
    let mut seen = BTreeMap::<String, ()>::new();
    let mut next_counter = plan.route_component_uid_counter;
    let mut normalized = Vec::with_capacity(plan.route_components.len());
    for index in 0..plan.route_components.len() {
        let existing = plan
            .route_component_uids
            .get(index)
            .cloned()
            .unwrap_or_default();
        let uid = if !existing.is_empty() && !seen.contains_key(&existing) {
            existing
        } else {
            loop {
                let candidate = format!("fpc:{next_counter:016x}");
                next_counter += 1;
                if !seen.contains_key(&candidate) {
                    break candidate;
                }
            }
        };
        seen.insert(uid.clone(), ());
        normalized.push(uid);
    }
    plan.route_component_uids = normalized;
    plan.route_component_uid_counter = next_counter;
}

fn component_uid(plan: &FlightPlan, component_index: usize) -> String {
    plan.route_component_uids
        .get(component_index)
        .cloned()
        .expect("normalized flight plan missing route component uid")
}

#[derive(Debug, Clone)]
struct PlannedDirectToTarget {
    row_id: FlightPlanRowId,
    target_leg_index: Option<usize>,
    resume_row_id: Option<FlightPlanRowId>,
}

fn planned_row_id_for_leg_index(plan: &FlightPlan, leg_index: usize) -> AppResult<FlightPlanRowId> {
    let mut matches = project_identity_rows(plan).into_iter().filter(|row| {
        row.row_kind == FlightPlanDisplayRowKind::Waypoint && row.leg_index == Some(leg_index)
    });
    let row = matches.next().ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("resolved leg {leg_index} has no projected waypoint row"),
    })?;
    if matches.next().is_some() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("resolved leg {leg_index} projects to multiple waypoint rows"),
        });
    }
    Ok(FlightPlanRowId(row.uid))
}

fn planned_direct_to_targets_for_nav_ref(
    plan: &FlightPlan,
    target: &NavRef,
) -> AppResult<Vec<PlannedDirectToTarget>> {
    let rows = project_identity_rows(plan);
    rows.into_iter()
        .filter(|row| {
            row.row_kind == FlightPlanDisplayRowKind::Waypoint
                && row.nav_ref.as_ref() == Some(target)
        })
        .map(|row| {
            let target_leg_index = row.leg_index;
            let resume_leg_index = if let Some(target_leg_index) = target_leg_index {
                resume_leg_index_after_leg(plan, target_leg_index)
            } else {
                row.component_index.and_then(|component_index| {
                    plan.resolved_legs.iter().position(|leg| {
                        leg_starts_at_route_component(leg, component_index) && &leg.from == target
                    })
                })
            };
            Ok(PlannedDirectToTarget {
                row_id: FlightPlanRowId(row.uid),
                target_leg_index,
                resume_row_id: resume_leg_index
                    .map(|leg_index| planned_row_id_for_leg_index(plan, leg_index))
                    .transpose()?,
            })
        })
        .collect()
}

fn allocate_temporary_direct_to_row_id(plan: &mut FlightPlan) -> FlightPlanRowId {
    let row_id = FlightPlanRowId(format!(
        "flight-plan-row:{:016x}",
        plan.route_component_uid_counter
    ));
    plan.route_component_uid_counter += 1;
    row_id
}

fn planned_direct_to_target_for_row_id(
    plan: &FlightPlan,
    row_id: &FlightPlanRowId,
) -> AppResult<(NavRef, PlannedDirectToTarget)> {
    let row = project_identity_rows(plan)
        .into_iter()
        .find(|row| row.uid == row_id.as_str())
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("direct-to target row does not exist: {}", row_id.as_str()),
        })?;
    if row.row_kind != FlightPlanDisplayRowKind::Waypoint {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "direct-to target row is not a waypoint: {}",
                row_id.as_str()
            ),
        });
    }
    let target = row.nav_ref.ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("direct-to target row has no location: {}", row_id.as_str()),
    })?;
    let target_leg_index = row.leg_index;
    let resume_leg_index = if let Some(target_leg_index) = target_leg_index {
        resume_leg_index_after_leg(plan, target_leg_index)
    } else {
        row.component_index.and_then(|component_index| {
            plan.resolved_legs.iter().position(|leg| {
                leg_starts_at_route_component(leg, component_index) && leg.from == target
            })
        })
    };
    Ok((
        target,
        PlannedDirectToTarget {
            row_id: row_id.clone(),
            target_leg_index,
            resume_row_id: resume_leg_index
                .map(|leg_index| planned_row_id_for_leg_index(plan, leg_index))
                .transpose()?,
        },
    ))
}

impl RouteComponent {
    fn is_waypoint(&self) -> bool {
        matches!(self, RouteComponent::Waypoint { .. })
    }
}

pub fn activate_direct_to(
    plan: &FlightPlan,
    from_position: LatLon,
    target: NavRef,
) -> AppResult<FlightPlan> {
    let mut plan = plan.clone().normalized();
    let planned_targets = planned_direct_to_targets_for_nav_ref(&plan, &target)?;
    let planned_target = match planned_targets.as_slice() {
        [target] => Some(target.clone()),
        _ => None,
    };
    let target_leg_index = planned_target
        .as_ref()
        .and_then(|target| target.target_leg_index);
    let resume_row_id = planned_target
        .as_ref()
        .and_then(|target| target.resume_row_id.clone());
    let target_row = planned_target
        .map(|target| DirectToTargetRow::Planned {
            row_id: target.row_id,
        })
        .unwrap_or_else(|| DirectToTargetRow::Temporary {
            row_id: allocate_temporary_direct_to_row_id(&mut plan),
        });
    let active_leg_index = target_leg_index
        .or_else(|| {
            plan.guidance
                .as_ref()
                .map(|guidance| guidance.active_leg_index)
        })
        .unwrap_or(0);

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index,
            active_detail_index: target_leg_index
                .and_then(|index| first_guidance_detail_index_for_leg(&plan, index)),
            display_split_leg_id: target_leg_index
                .and_then(|index| plan.resolved_legs.get(index))
                .map(|leg| leg.id.clone()),
            sequencing_mode: SequencingMode::DirectTo,
            direct_to: Some(DirectToState {
                start: NavRef::LatLon(from_position),
                target,
                target_row,
                resume_row_id,
            }),
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_direct_to_row(
    plan: &FlightPlan,
    from_position: LatLon,
    target_row_id: &FlightPlanRowId,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let (target, planned_target) = planned_direct_to_target_for_row_id(&plan, target_row_id)?;
    let active_leg_index = planned_target
        .target_leg_index
        .or_else(|| {
            plan.guidance
                .as_ref()
                .map(|guidance| guidance.active_leg_index)
        })
        .unwrap_or(0);
    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index,
            active_detail_index: planned_target
                .target_leg_index
                .and_then(|index| first_guidance_detail_index_for_leg(&plan, index)),
            display_split_leg_id: planned_target
                .target_leg_index
                .and_then(|index| plan.resolved_legs.get(index))
                .map(|leg| leg.id.clone()),
            sequencing_mode: SequencingMode::DirectTo,
            direct_to: Some(DirectToState {
                start: NavRef::LatLon(from_position),
                target,
                target_row: DirectToTargetRow::Planned {
                    row_id: planned_target.row_id,
                },
                resume_row_id: planned_target.resume_row_id,
            }),
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn restore_direct_to(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let Some(guidance) = plan.guidance.as_ref() else {
        return Ok(plan);
    };
    if guidance.sequencing_mode != SequencingMode::DirectTo {
        return Ok(plan);
    }
    let has_route = !plan.resolved_legs.is_empty();
    let active_leg_index = if has_route {
        guidance
            .active_leg_index
            .min(plan.resolved_legs.len().saturating_sub(1))
    } else {
        0
    };
    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index,
            active_detail_index: if has_route {
                first_guidance_detail_index_for_leg(&plan, active_leg_index)
            } else {
                None
            },
            display_split_leg_id: None,
            sequencing_mode: if has_route {
                SequencingMode::FollowPlan
            } else {
                SequencingMode::Suspended
            },
            direct_to: None,
            suspend_reason: if has_route {
                None
            } else {
                Some(SuspendReason::RouteEnd)
            },
        }),
        ..plan
    })
}

pub fn activate_leg(plan: &FlightPlan, leg_index: usize) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if leg_index >= plan.resolved_legs.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("leg index out of bounds: {leg_index}"),
        });
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: leg_index,
            active_detail_index: first_guidance_detail_index_for_leg(&plan, leg_index),
            display_split_leg_id: plan.resolved_legs.get(leg_index).map(|leg| leg.id.clone()),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_leg_at_detail_index(
    plan: &FlightPlan,
    leg_index: usize,
    detail_index: usize,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let detail = guidance_detail_ref_by_index(&plan, detail_index).ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("guidance detail index out of bounds: {detail_index}"),
    })?;
    if detail.leg_index != leg_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "guidance detail {detail_index} belongs to leg {}, not {leg_index}",
                detail.leg_index
            ),
        });
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: leg_index,
            active_detail_index: Some(detail_index),
            display_split_leg_id: plan.resolved_legs.get(leg_index).map(|leg| leg.id.clone()),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_next_leg(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot activate next leg without guidance state".to_string(),
    })?;
    let next_leg_index = guidance
        .active_leg_index
        .checked_add(1)
        .filter(|index| *index < plan.resolved_legs.len())
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "no next leg is available".to_string(),
        })?;

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: next_leg_index,
            active_detail_index: first_guidance_detail_index_for_leg(&plan, next_leg_index),
            display_split_leg_id: plan
                .resolved_legs
                .get(next_leg_index)
                .map(|leg| leg.id.clone()),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn stop_navigation(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if plan.guidance.is_none() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot stop navigation without guidance state".to_string(),
        });
    }

    Ok(FlightPlan {
        guidance: None,
        ..plan
    })
}

pub fn suspend_sequencing(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot suspend sequencing without guidance state".to_string(),
    })?;

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            active_detail_index: guidance.active_detail_index,
            display_split_leg_id: guidance.display_split_leg_id.clone(),
            sequencing_mode: SequencingMode::Suspended,
            direct_to: None,
            suspend_reason: Some(SuspendReason::Manual),
        }),
        ..plan
    })
}

pub fn unsuspend_sequencing(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot unsuspend sequencing without guidance state".to_string(),
    })?;

    if guidance.sequencing_mode != SequencingMode::Suspended {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "sequencing is not suspended".to_string(),
        });
    }

    if let Some(hold_start_detail) =
        terminal_hold_start_detail_index_for_leg(&plan, guidance.active_leg_index)
    {
        if guidance
            .active_detail_index
            .is_some_and(|detail_index| detail_index >= hold_start_detail)
        {
            return Ok(FlightPlan {
                guidance: Some(GuidanceState {
                    active_leg_index: guidance.active_leg_index,
                    active_detail_index: guidance.active_detail_index,
                    display_split_leg_id: guidance.display_split_leg_id.clone(),
                    sequencing_mode: SequencingMode::FollowPlan,
                    direct_to: None,
                    suspend_reason: None,
                }),
                ..plan
            });
        }
    }

    if should_suspend_after_active_leg(&plan, guidance.active_leg_index) {
        return activate_next_leg(&plan);
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            active_detail_index: guidance.active_detail_index,
            display_split_leg_id: guidance.display_split_leg_id.clone(),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn sequence_active_leg(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot sequence a plan without guidance state".to_string(),
    })?;

    let next_guidance = match guidance.sequencing_mode {
        SequencingMode::DirectTo => {
            let direct_to = guidance.direct_to.ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "direct-to sequencing mode requires direct-to state".to_string(),
            })?;

            match direct_to_resume_leg_index(&plan, &direct_to)
                .filter(|index| plan.resolved_legs[*index].from == direct_to.target)
            {
                Some(resume_leg_index) => GuidanceState {
                    active_leg_index: resume_leg_index,
                    active_detail_index: first_guidance_detail_index_for_leg(
                        &plan,
                        resume_leg_index,
                    ),
                    display_split_leg_id: plan
                        .resolved_legs
                        .get(resume_leg_index)
                        .map(|leg| leg.id.clone()),
                    sequencing_mode: SequencingMode::FollowPlan,
                    direct_to: None,
                    suspend_reason: None,
                },
                None => GuidanceState {
                    active_leg_index: direct_to_target_leg_index(&plan, &direct_to)
                        .unwrap_or(guidance.active_leg_index),
                    active_detail_index: direct_to_target_leg_index(&plan, &direct_to)
                        .and_then(|index| first_guidance_detail_index_for_leg(&plan, index))
                        .or(guidance.active_detail_index),
                    display_split_leg_id: direct_to_target_leg_index(&plan, &direct_to)
                        .and_then(|index| plan.resolved_legs.get(index))
                        .map(|leg| leg.id.clone())
                        .or_else(|| guidance.display_split_leg_id.clone()),
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::DirectToComplete),
                },
            }
        }
        SequencingMode::FollowPlan => {
            if plan.resolved_legs.is_empty() {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "cannot sequence an empty resolved route".to_string(),
                });
            }

            let active_detail_index = guidance
                .active_detail_index
                .or_else(|| first_guidance_detail_index_for_leg(&plan, guidance.active_leg_index))
                .unwrap_or(0);
            let active_detail = guidance_detail_ref_by_index(&plan, active_detail_index)
                .ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: format!("guidance detail index out of bounds: {active_detail_index}"),
                })?;
            let next_leg_index = guidance.active_leg_index + 1;
            if next_leg_index < plan.resolved_legs.len() {
                if should_suspend_after_active_leg(&plan, guidance.active_leg_index) {
                    GuidanceState {
                        active_leg_index: guidance.active_leg_index,
                        active_detail_index: Some(active_detail.detail_index),
                        display_split_leg_id: guidance.display_split_leg_id.clone(),
                        sequencing_mode: SequencingMode::Suspended,
                        direct_to: None,
                        suspend_reason: Some(SuspendReason::Boundary),
                    }
                } else {
                    GuidanceState {
                        active_leg_index: next_leg_index,
                        active_detail_index: first_guidance_detail_index_for_leg(
                            &plan,
                            next_leg_index,
                        ),
                        display_split_leg_id: plan
                            .resolved_legs
                            .get(next_leg_index)
                            .map(|leg| leg.id.clone()),
                        sequencing_mode: SequencingMode::FollowPlan,
                        direct_to: None,
                        suspend_reason: None,
                    }
                }
            } else if should_suspend_after_active_leg(&plan, guidance.active_leg_index) {
                GuidanceState {
                    active_leg_index: guidance.active_leg_index,
                    active_detail_index: Some(active_detail.detail_index),
                    display_split_leg_id: guidance.display_split_leg_id.clone(),
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::Boundary),
                }
            } else {
                GuidanceState {
                    active_leg_index: guidance.active_leg_index,
                    active_detail_index: Some(active_detail.detail_index),
                    display_split_leg_id: guidance.display_split_leg_id.clone(),
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::RouteEnd),
                }
            }
        }
        SequencingMode::Suspended => guidance,
    };

    Ok(FlightPlan {
        guidance: Some(next_guidance),
        ..plan
    })
}

pub fn sequence_active_detail(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot sequence a plan without guidance state".to_string(),
    })?;
    if guidance.sequencing_mode != SequencingMode::FollowPlan {
        return sequence_active_leg(&plan);
    }
    let active_detail_index = guidance
        .active_detail_index
        .or_else(|| first_guidance_detail_index_for_leg(&plan, guidance.active_leg_index))
        .unwrap_or(0);
    let active_detail =
        guidance_detail_ref_by_index(&plan, active_detail_index).ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("guidance detail index out of bounds: {active_detail_index}"),
        })?;
    let next_detail_index = active_detail.detail_index + 1;
    let Some(next_detail) = guidance_detail_ref_by_index(&plan, next_detail_index) else {
        if active_detail_is_terminal_hold_detail(&plan, &active_detail) {
            return sequence_after_terminal_hold(&plan, &guidance);
        }
        return sequence_active_leg(&plan);
    };
    if next_detail.leg_index != guidance.active_leg_index {
        if active_detail_is_terminal_hold_detail(&plan, &active_detail) {
            return sequence_after_terminal_hold(&plan, &guidance);
        }
        return sequence_active_leg(&plan);
    }
    if terminal_hold_start_detail_index_for_leg(&plan, guidance.active_leg_index)
        == Some(next_detail_index)
    {
        return Ok(FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: guidance.active_leg_index,
                active_detail_index: Some(next_detail_index),
                display_split_leg_id: guidance.display_split_leg_id.clone(),
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            ..plan
        });
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            active_detail_index: Some(next_detail_index),
            display_split_leg_id: guidance.display_split_leg_id.clone(),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

fn active_detail_is_terminal_hold_detail(
    plan: &FlightPlan,
    active_detail: &GuidanceDetailRef,
) -> bool {
    terminal_hold_start_detail_index_for_leg(plan, active_detail.leg_index)
        .is_some_and(|hold_start| active_detail.detail_index >= hold_start)
}

fn sequence_after_terminal_hold(
    plan: &FlightPlan,
    guidance: &GuidanceState,
) -> AppResult<FlightPlan> {
    match activate_next_leg(plan) {
        Ok(next) => Ok(next),
        Err(err) if err.kind == AppErrorKind::UnsupportedOperation => Ok(FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: guidance.active_leg_index,
                active_detail_index: guidance.active_detail_index,
                display_split_leg_id: guidance.display_split_leg_id.clone(),
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::RouteEnd),
            }),
            ..plan.clone()
        }),
        Err(err) => Err(err),
    }
}

pub fn active_guidance_leg(plan: &FlightPlan) -> Option<PlanLeg> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone()?;
    if !guidance_projects_active_leg(&plan, &guidance) {
        return None;
    }

    match guidance.sequencing_mode {
        SequencingMode::DirectTo => {
            let direct_to = guidance.direct_to?;
            Some(PlanLeg {
                from: direct_to.start,
                to: direct_to.target,
                airway: None,
            })
        }
        SequencingMode::FollowPlan => {
            plan.resolved_legs
                .get(guidance.active_leg_index)
                .map(|leg| PlanLeg {
                    from: leg.from.clone(),
                    to: leg.to.clone(),
                    airway: None,
                })
        }
        SequencingMode::Suspended => plan
            .resolved_legs
            .get(guidance.active_leg_index)
            .map(|leg| PlanLeg {
                from: leg.from.clone(),
                to: leg.to.clone(),
                airway: None,
            }),
    }
}

pub(crate) fn guidance_projects_active_leg(plan: &FlightPlan, guidance: &GuidanceState) -> bool {
    match guidance.sequencing_mode {
        SequencingMode::DirectTo => guidance.direct_to.is_some(),
        SequencingMode::FollowPlan => plan.resolved_legs.get(guidance.active_leg_index).is_some(),
        SequencingMode::Suspended => {
            let preserve_active_leg = match guidance.suspend_reason {
                Some(SuspendReason::Manual) => true,
                Some(SuspendReason::Boundary) if guidance_is_in_terminal_hold(plan, guidance) => {
                    true
                }
                Some(
                    SuspendReason::Boundary
                    | SuspendReason::RouteEnd
                    | SuspendReason::DirectToComplete,
                ) => false,
                None => {
                    guidance.direct_to.is_none()
                        && !should_suspend_after_active_leg(plan, guidance.active_leg_index)
                        && guidance.active_leg_index + 1 < plan.resolved_legs.len()
                }
            };
            preserve_active_leg && plan.resolved_legs.get(guidance.active_leg_index).is_some()
        }
    }
}

fn guidance_is_in_terminal_hold(plan: &FlightPlan, guidance: &GuidanceState) -> bool {
    guidance.active_detail_index.is_some_and(|detail_index| {
        terminal_hold_start_detail_index_for_leg(plan, guidance.active_leg_index)
            .is_some_and(|hold_start| detail_index >= hold_start)
    })
}

fn flight_plan_control(
    id: FlightPlanControlId,
    label: &str,
    enabled: bool,
    disabled_reason: &'static str,
) -> FlightPlanControlUiView {
    FlightPlanControlUiView {
        id,
        label: label.to_string(),
        enabled,
        disabled_reason: (!enabled).then(|| disabled_reason.to_string()),
    }
}

fn project_flight_plan_controls(plan: &FlightPlan) -> Vec<FlightPlanControlUiView> {
    let (
        can_activate_next_leg,
        can_restore_direct_to,
        can_sequence_active_leg,
        can_stop_navigation,
        can_suspend,
        can_unsuspend,
    ) = plan
        .guidance
        .as_ref()
        .map(|guidance| {
            let can_restore_direct_to = guidance.sequencing_mode == SequencingMode::DirectTo
                && guidance
                    .direct_to
                    .as_ref()
                    .is_some_and(|direct_to| !direct_to.target_row.is_planned());
            let can_sequence_active_leg = match guidance.sequencing_mode {
                SequencingMode::DirectTo => guidance.direct_to.is_some(),
                SequencingMode::FollowPlan => !plan.resolved_legs.is_empty(),
                SequencingMode::Suspended => false,
            };
            (
                guidance.active_leg_index + 1 < plan.resolved_legs.len(),
                can_restore_direct_to,
                can_sequence_active_leg,
                true,
                guidance.sequencing_mode != SequencingMode::Suspended,
                guidance.sequencing_mode == SequencingMode::Suspended,
            )
        })
        .unwrap_or((false, false, false, false, false, false));

    vec![
        flight_plan_control(
            FlightPlanControlId::ActivateNextLeg,
            "Next\nLeg",
            can_activate_next_leg,
            "No next leg is available.",
        ),
        flight_plan_control(
            FlightPlanControlId::SequenceActiveLeg,
            "SQNC",
            can_sequence_active_leg,
            if plan
                .guidance
                .as_ref()
                .is_some_and(|guidance| guidance.sequencing_mode == SequencingMode::Suspended)
            {
                "Unsuspend sequencing before sequencing the active leg."
            } else {
                "No active leg is available to sequence."
            },
        ),
        flight_plan_control(
            FlightPlanControlId::StopNavigation,
            "STOP\nNAV",
            can_stop_navigation,
            "No active guidance is available to stop.",
        ),
        flight_plan_control(
            FlightPlanControlId::SuspendSequencing,
            "SUSP",
            can_suspend,
            if plan.guidance.is_none() {
                "No active guidance is available to suspend."
            } else if plan
                .guidance
                .as_ref()
                .is_some_and(|guidance| guidance.sequencing_mode == SequencingMode::Suspended)
            {
                "Sequencing is already suspended."
            } else {
                "Sequencing cannot be suspended now."
            },
        ),
        flight_plan_control(
            FlightPlanControlId::UnsuspendSequencing,
            "Unsusp",
            can_unsuspend,
            "Sequencing is not suspended.",
        ),
        flight_plan_control(
            FlightPlanControlId::RestoreDirectTo,
            "Restore\nFP",
            can_restore_direct_to,
            "No off-plan Direct-To is active.",
        ),
    ]
}

fn project_component_ui_views(
    plan: &FlightPlan,
    active_component_index: Option<usize>,
) -> Vec<RouteComponentUiView> {
    let grouped_legs = grouped_component_legs(&plan);
    let projected_items =
        dedupe_component_items_for_projection(&plan.route_components, &grouped_legs);
    plan.route_components
        .iter()
        .enumerate()
        .map(|(component_index, component)| {
            let component_nav_ref = match component {
                RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
                RouteComponent::Airway { .. } | RouteComponent::Procedure { .. } => None,
            };
            let preceding_waypoint =
                adjacent_waypoint_component(&plan.route_components, component_index, -1);
            let following_waypoint =
                adjacent_waypoint_component(&plan.route_components, component_index, 1);
            let replace_procedure_component_index =
                replaceable_procedure_component_before(&plan, component_index);
            RouteComponentUiView {
                uid: component_uid(&plan, component_index),
                component_index,
                kind: component_view_kind(component),
                summary: component_summary(component),
                procedure_id: component_procedure_id(component),
                procedure_kind: component_procedure_kind(component),
                chart_airport_id: component_chart_airport_id(component),
                nav_ref: component_nav_ref,
                items: projected_items
                    .get(component_index)
                    .cloned()
                    .unwrap_or_default(),
                active: active_component_index == Some(component_index),
                can_add_airway_after: matches!(component, RouteComponent::Waypoint { .. })
                    && matches!(
                        plan.route_components.get(component_index + 1),
                        Some(RouteComponent::Waypoint { .. }) | None
                    ),
                can_add_procedure_before: matches!(
                    component,
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Airport(_)
                    }
                ) && (can_insert_procedure_before_component(
                    &plan,
                    component_index,
                ) || replace_procedure_component_index.is_some()),
                can_remove: can_remove_component(&plan, component_index),
                can_reorder: can_reorder_component(&plan, component_index),
                can_reorder_up: can_reorder_component_in_direction(&plan, component_index, -1),
                can_reorder_down: can_reorder_component_in_direction(&plan, component_index, 1),
                replace_procedure_component_index,
                preceding_waypoint,
                following_waypoint,
            }
        })
        .collect()
}

pub(crate) fn project_identity_rows(plan: &FlightPlan) -> Vec<FlightPlanDisplayRowUiView> {
    let components = project_component_ui_views(plan, None);
    project_display_rows(plan, &components, &[])
}

pub fn project_ui_state(plan: &FlightPlan) -> FlightPlanUiState {
    let plan = plan.clone().normalized();
    let active_component_index = plan
        .guidance
        .as_ref()
        .and_then(|guidance| active_component_index_for_guidance(&plan, guidance));
    let components = project_component_ui_views(&plan, active_component_index);

    let resolved_legs: Vec<ResolvedLegUiView> = plan
        .resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| ResolvedLegUiView {
            leg_index,
            leg_id: leg.id.clone(),
            component_index: match leg.source {
                ResolvedLegSource::RouteComponent { component_index } => Some(component_index),
                ResolvedLegSource::SyntheticBridge { .. } => None,
            },
            from: leg.from.clone(),
            to: leg.to.clone(),
            active: plan.guidance.as_ref().is_some_and(|guidance| {
                guidance.sequencing_mode != SequencingMode::DirectTo
                    && guidance.active_leg_index == leg_index
            }),
            suspend_boundary_after: should_suspend_after_active_leg(&plan, leg_index),
            display_path: leg
                .procedure_provenance
                .as_ref()
                .and_then(|provenance| provenance.display_path.clone()),
        })
        .collect();

    let mut display_rows = project_display_rows(&plan, &components, &resolved_legs);
    populate_default_flight_data_cells(&mut display_rows);
    let (active_from_row_uid, active_to_row_uid) = active_guidance_row_uids(&plan, &display_rows);

    let guidance = plan.guidance.as_ref().map(|guidance| GuidanceUiView {
        sequencing_mode: guidance.sequencing_mode.clone(),
        active_leg_index: if plan.resolved_legs.is_empty() {
            None
        } else {
            Some(guidance.active_leg_index)
        },
        active_from_row_uid,
        active_to_row_uid,
        active_component_index,
        active_leg: active_guidance_leg(&plan),
        nav_element: project_nav_element_ui(&plan),
        direct_to: guidance.direct_to.as_ref().map(|direct_to| DirectToUiView {
            start: direct_to.start.clone(),
            target: direct_to.target.clone(),
            target_row_id: direct_to.target_row.row_id().clone(),
            on_plan_target: direct_to.target_row.is_planned(),
        }),
        suspend_boundary_after_active_leg: should_suspend_after_active_leg(
            &plan,
            guidance.active_leg_index,
        ),
    });

    FlightPlanUiState {
        display_rows,
        components,
        resolved_legs,
        data_columns: crate::flight_data::flight_plan_columns(),
        guidance,
        controls: project_flight_plan_controls(&plan),
    }
}

fn populate_default_flight_data_cells(rows: &mut [FlightPlanDisplayRowUiView]) {
    let computer = crate::FlightDataComputer::default();
    for row in rows {
        row.data_cells = computer.flight_plan_row_cells(
            row.row_kind != FlightPlanDisplayRowKind::Group,
            None,
            None,
            None,
            None,
            FlightDataCellTone::Planned,
        );
    }
}

fn project_nav_element_ui(plan: &FlightPlan) -> NavElementUiView {
    let active_leg = active_guidance_leg(plan);
    NavElementUiView {
        active_leg_summary: active_leg
            .as_ref()
            .map(|leg| {
                format!(
                    "{} \u{2192} {}",
                    nav_ref_label(&leg.from),
                    nav_ref_label(&leg.to)
                )
            })
            .unwrap_or_default(),
        cdi_indicator_dots: None,
        cdi_offscale_readout: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidanceDetailRef {
    pub detail_index: usize,
    pub leg_index: usize,
    pub leg_id: String,
    pub element_index: usize,
}

pub fn guidance_detail_refs(plan: &FlightPlan) -> Vec<GuidanceDetailRef> {
    let mut details = Vec::new();
    let mut detail_index = 0usize;
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        let element_count = leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
            .map(|path| usize::max(path.elements.len(), 1))
            .unwrap_or(1);
        for element_index in 0..element_count {
            details.push(GuidanceDetailRef {
                detail_index,
                leg_index,
                leg_id: leg.id.clone(),
                element_index,
            });
            detail_index += 1;
        }
    }
    details
}

pub fn first_guidance_detail_index_for_leg(plan: &FlightPlan, leg_index: usize) -> Option<usize> {
    guidance_detail_refs(plan)
        .into_iter()
        .find(|detail| detail.leg_index == leg_index)
        .map(|detail| detail.detail_index)
}

pub fn guidance_detail_ref_by_index(
    plan: &FlightPlan,
    detail_index: usize,
) -> Option<GuidanceDetailRef> {
    guidance_detail_refs(plan)
        .into_iter()
        .find(|detail| detail.detail_index == detail_index)
}

fn project_display_rows(
    plan: &FlightPlan,
    components: &[RouteComponentUiView],
    _resolved_legs: &[ResolvedLegUiView],
) -> Vec<FlightPlanDisplayRowUiView> {
    let direct_to = plan.guidance.as_ref().and_then(|guidance| {
        (guidance.sequencing_mode == SequencingMode::DirectTo)
            .then_some(guidance.direct_to.as_ref())
            .flatten()
    });
    let direct_to_overlay = flight_plan_has_direct_to_overlay(plan);
    let mut rows = Vec::new();
    let mut child_occurrences: BTreeMap<usize, usize> = BTreeMap::new();
    let mut child_waypoint_occurrences: BTreeMap<(usize, String), usize> = BTreeMap::new();
    for component in components {
        let chart_airport_id = component.chart_airport_id.clone();
        if component.kind == RouteComponentViewKind::Waypoint {
            let nav_ref = component.nav_ref.clone();
            let projected_nav_ref = projected_component_waypoint_nav_ref(component);
            let origin_anchor = projected_nav_ref.clone();
            let destination_anchor = component.following_waypoint.clone();
            let end_component_index = if destination_anchor.is_some() {
                Some(component.component_index + 1)
            } else {
                None
            };
            let leg_index = top_level_waypoint_row_leg_index(plan, component.component_index);
            let uid = top_level_waypoint_row_uid(component, leg_index);
            let actions = assign_action_uids(
                &uid,
                waypoint_actions_for_row(
                    FlightPlanDisplayRowKind::Waypoint,
                    0,
                    leg_index,
                    projected_nav_ref.as_ref(),
                    component.can_add_airway_after,
                    component.can_add_procedure_before,
                    component.can_remove,
                    component.can_reorder_up,
                    component.can_reorder_down,
                    component.component_index.into(),
                    chart_airport_id.as_ref(),
                    origin_anchor.as_ref(),
                ),
            );
            rows.push(FlightPlanDisplayRowUiView {
                uid,
                label: nav_ref
                    .as_ref()
                    .map(nav_ref_label)
                    .unwrap_or_else(|| component.summary.clone()),
                row_kind: FlightPlanDisplayRowKind::Waypoint,
                component_kind: Some(component.kind.clone()),
                component_uid: Some(component.uid.clone()),
                component_index: Some(component.component_index),
                procedure_id: component.procedure_id.clone(),
                procedure_kind: component.procedure_kind.clone(),
                leg_index,
                data_cells: Vec::new(),
                show_plate_target_id: None,
                chart_airport_id,
                nav_ref,
                symbol_feature: None,
                depth: 0,
                active: component.active,
                enabled: !direct_to_overlay,
                disabled_reason: None,
                synthetic_direct_to: false,
                can_add_airway_after: component.can_add_airway_after,
                can_add_procedure_before: component.can_add_procedure_before,
                can_remove_component: component.can_remove,
                can_reorder_component: component.can_reorder,
                can_reorder_up: component.can_reorder_up,
                can_reorder_down: component.can_reorder_down,
                replace_procedure_component_index: component.replace_procedure_component_index,
                start_component_index: Some(component.component_index),
                end_component_index,
                origin_anchor,
                destination_anchor,
                preceding_waypoint: component.preceding_waypoint.clone(),
                following_waypoint: component.following_waypoint.clone(),
                action_matrix: action_matrix_from_actions(&actions),
            });
        } else {
            let origin_anchor = component.preceding_waypoint.clone();
            let destination_anchor = component.following_waypoint.clone();
            let uid = format!("component:{}:{:?}:group", component.uid, component.kind);
            let actions = assign_action_uids(&uid, group_row_actions(component));
            rows.push(FlightPlanDisplayRowUiView {
                uid: uid.clone(),
                label: structured_component_label(component),
                row_kind: FlightPlanDisplayRowKind::Group,
                component_kind: Some(component.kind.clone()),
                component_uid: Some(component.uid.clone()),
                component_index: Some(component.component_index),
                procedure_id: component.procedure_id.clone(),
                procedure_kind: component.procedure_kind.clone(),
                leg_index: None,
                data_cells: Vec::new(),
                show_plate_target_id: None,
                chart_airport_id,
                nav_ref: None,
                symbol_feature: None,
                depth: 0,
                active: component.active,
                enabled: !direct_to_overlay,
                disabled_reason: None,
                synthetic_direct_to: false,
                can_add_airway_after: component.can_add_airway_after,
                can_add_procedure_before: component.can_add_procedure_before,
                can_remove_component: component.can_remove,
                can_reorder_component: component.can_reorder,
                can_reorder_up: component.can_reorder_up,
                can_reorder_down: component.can_reorder_down,
                replace_procedure_component_index: None,
                start_component_index: None,
                end_component_index: None,
                origin_anchor: origin_anchor.clone(),
                destination_anchor: destination_anchor.clone(),
                preceding_waypoint: component.preceding_waypoint.clone(),
                following_waypoint: component.following_waypoint.clone(),
                action_matrix: action_matrix_from_actions(&actions),
            });
            let airway_child_waypoints = if component.kind == RouteComponentViewKind::Airway {
                component
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        ConcretizedNavItem::Waypoint { nav_ref } => Some(nav_ref.clone()),
                        ConcretizedNavItem::Discontinuity { .. } => None,
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut airway_child_waypoint_index = 0usize;
            for item in &component.items {
                match item {
                    ConcretizedNavItem::Waypoint { nav_ref } => {
                        let occurrence = child_occurrences
                            .entry(component.component_index)
                            .and_modify(|count| *count += 1)
                            .or_insert(0);
                        let waypoint_occurrence = child_waypoint_occurrences
                            .entry((component.component_index, nav_ref_key(nav_ref)))
                            .and_modify(|count| *count += 1)
                            .or_insert(0);
                        let leg_index = child_waypoint_row_leg_index(
                            plan,
                            component.component_index,
                            airway_child_waypoint_index,
                            *waypoint_occurrence,
                            nav_ref,
                        );
                        let uid = child_waypoint_row_uid(
                            &component.uid,
                            component.kind.clone(),
                            *occurrence,
                            nav_ref,
                            leg_index,
                        );
                        let actions = assign_action_uids(
                            &uid,
                            child_waypoint_actions_for_row(
                                FlightPlanDisplayRowKind::Waypoint,
                                component.kind.clone(),
                                leg_index,
                                Some(nav_ref),
                                airway_child_waypoint_index,
                                airway_child_waypoints.len(),
                            ),
                        );
                        airway_child_waypoint_index += 1;
                        rows.push(FlightPlanDisplayRowUiView {
                            uid,
                            label: nav_ref_label(nav_ref),
                            row_kind: FlightPlanDisplayRowKind::Waypoint,
                            component_kind: Some(component.kind.clone()),
                            component_uid: Some(component.uid.clone()),
                            component_index: Some(component.component_index),
                            procedure_id: component.procedure_id.clone(),
                            procedure_kind: component.procedure_kind.clone(),
                            leg_index,
                            data_cells: Vec::new(),
                            show_plate_target_id: None,
                            chart_airport_id: airport_id_from_nav_ref(nav_ref),
                            nav_ref: Some(nav_ref.clone()),
                            symbol_feature: None,
                            depth: 1,
                            active: component.active,
                            enabled: !direct_to_overlay,
                            disabled_reason: None,
                            synthetic_direct_to: false,
                            can_add_airway_after: false,
                            can_add_procedure_before: false,
                            can_remove_component: false,
                            can_reorder_component: false,
                            can_reorder_up: false,
                            can_reorder_down: false,
                            replace_procedure_component_index: None,
                            start_component_index: None,
                            end_component_index: None,
                            origin_anchor: None,
                            destination_anchor: None,
                            preceding_waypoint: component.preceding_waypoint.clone(),
                            following_waypoint: component.following_waypoint.clone(),
                            action_matrix: action_matrix_from_actions(&actions),
                        })
                    }
                    ConcretizedNavItem::Discontinuity {
                        discontinuity,
                        label,
                    } => {
                        let occurrence = child_occurrences
                            .entry(component.component_index)
                            .and_modify(|count| *count += 1)
                            .or_insert(0);
                        let uid = format!(
                            "component:{}:{:?}:disc:{}:{}",
                            component.uid, component.kind, occurrence, label
                        );
                        let leg_index = match discontinuity {
                            ProcedureDiscontinuity::Hold => last_guidance_leg_index_for_component(
                                plan,
                                component.component_index,
                            ),
                            ProcedureDiscontinuity::Vectors | ProcedureDiscontinuity::Other(_) => {
                                None
                            }
                        };
                        rows.push(FlightPlanDisplayRowUiView {
                            uid: uid.clone(),
                            label: label.clone(),
                            row_kind: FlightPlanDisplayRowKind::Discontinuity,
                            component_kind: Some(component.kind.clone()),
                            component_uid: Some(component.uid.clone()),
                            component_index: Some(component.component_index),
                            procedure_id: component.procedure_id.clone(),
                            procedure_kind: component.procedure_kind.clone(),
                            leg_index,
                            data_cells: Vec::new(),
                            show_plate_target_id: None,
                            chart_airport_id: None,
                            nav_ref: None,
                            symbol_feature: None,
                            depth: 1,
                            active: false,
                            enabled: !direct_to_overlay,
                            disabled_reason: None,
                            synthetic_direct_to: false,
                            can_add_airway_after: false,
                            can_add_procedure_before: false,
                            can_remove_component: false,
                            can_reorder_component: false,
                            can_reorder_up: false,
                            can_reorder_down: false,
                            replace_procedure_component_index: None,
                            start_component_index: None,
                            end_component_index: None,
                            origin_anchor: None,
                            destination_anchor: None,
                            preceding_waypoint: component.preceding_waypoint.clone(),
                            following_waypoint: component.following_waypoint.clone(),
                            action_matrix: action_matrix_from_actions(&assign_action_uids(
                                &uid,
                                vec![core_session_action(
                                    FlightPlanRowActionId::ActivateLeg,
                                    leg_index.is_some(),
                                )],
                            )),
                        })
                    }
                }
            }
        }
    }

    if let Some(direct_to) = direct_to.filter(|_| direct_to_overlay) {
        let chart_airport_id = airport_id_from_nav_ref(&direct_to.target);
        let uid = direct_to.target_row.row_id().0.clone();
        rows.push(FlightPlanDisplayRowUiView {
            uid: uid.clone(),
            label: nav_ref_label(&direct_to.target),
            row_kind: FlightPlanDisplayRowKind::Waypoint,
            component_kind: Some(RouteComponentViewKind::Waypoint),
            component_uid: None,
            component_index: None,
            procedure_id: None,
            procedure_kind: None,
            leg_index: None,
            data_cells: Vec::new(),
            show_plate_target_id: None,
            chart_airport_id,
            nav_ref: Some(direct_to.target.clone()),
            symbol_feature: None,
            depth: 0,
            active: true,
            enabled: true,
            disabled_reason: None,
            synthetic_direct_to: true,
            can_add_airway_after: false,
            can_add_procedure_before: false,
            can_remove_component: false,
            can_reorder_component: false,
            can_reorder_up: false,
            can_reorder_down: false,
            replace_procedure_component_index: None,
            start_component_index: None,
            end_component_index: None,
            origin_anchor: Some(direct_to.start.clone()),
            destination_anchor: Some(direct_to.target.clone()),
            preceding_waypoint: None,
            following_waypoint: None,
            action_matrix: action_matrix_from_actions(&assign_action_uids(
                &uid,
                vec![action(FlightPlanRowActionId::WaypointInfo, false)],
            )),
        });
    }

    if direct_to_overlay {
        for row in &mut rows {
            if !row.synthetic_direct_to {
                row.enabled = false;
                row.disabled_reason = Some(OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON.to_string());
                for action in flight_plan_row_actions_mut(row) {
                    action.enabled = false;
                    action.disabled_reason =
                        Some(OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON.to_string());
                }
            }
        }
    }

    if let Some(active_leg_index) = plan.guidance.as_ref().and_then(|guidance| {
        (guidance.sequencing_mode != SequencingMode::DirectTo).then_some(guidance.active_leg_index)
    }) {
        let active_detail = plan
            .guidance
            .as_ref()
            .and_then(|guidance| guidance.active_detail_index)
            .and_then(|detail_index| guidance_detail_ref_by_index(plan, detail_index));
        let active_hold_detail_start =
            terminal_hold_start_detail_index_for_leg(plan, active_leg_index);
        let active_in_terminal_hold = active_hold_detail_start.is_some_and(|hold_start| {
            active_detail.as_ref().is_some_and(|detail| {
                detail.leg_index == active_leg_index && detail.detail_index >= hold_start
            })
        });
        for row in &mut rows {
            if row.leg_index == Some(active_leg_index) {
                let hold_row = row.row_kind == FlightPlanDisplayRowKind::Discontinuity
                    && row.label == ProcedureDiscontinuity::Hold.display_label();
                for action in flight_plan_row_actions_mut(row) {
                    if action.id == FlightPlanRowActionId::ActivateLeg {
                        if (hold_row && active_in_terminal_hold)
                            || (!hold_row && !active_in_terminal_hold)
                        {
                            action.enabled = false;
                            action.disabled_reason =
                                Some("This leg is already active.".to_string());
                        }
                    }
                }
            }
        }
    }

    for row in &mut rows {
        refresh_flight_plan_row_action_navigation(row);
    }

    rows
}

fn active_guidance_row_uids(
    plan: &FlightPlan,
    rows: &[FlightPlanDisplayRowUiView],
) -> (Option<String>, Option<String>) {
    let Some(guidance) = plan.guidance.as_ref() else {
        return (None, None);
    };
    let Some(active_leg) = active_guidance_leg(plan) else {
        return (None, None);
    };

    if let Some(hold_start_detail) =
        terminal_hold_start_detail_index_for_leg(plan, guidance.active_leg_index)
    {
        let active_hold_detail = guidance
            .active_detail_index
            .is_some_and(|detail_index| detail_index >= hold_start_detail);
        if active_hold_detail {
            let to_index = rows.iter().position(|row| {
                row.row_kind == FlightPlanDisplayRowKind::Discontinuity
                    && row.label == ProcedureDiscontinuity::Hold.display_label()
                    && row.leg_index == Some(guidance.active_leg_index)
            });
            let Some(to_index) = to_index else {
                return (None, None);
            };
            let from_index = rows[..to_index].iter().rposition(|row| {
                row.row_kind == FlightPlanDisplayRowKind::Waypoint
                    && row.leg_index == Some(guidance.active_leg_index)
                    && row.nav_ref.as_ref() == Some(&active_leg.to)
            });
            return (
                from_index.map(|index| rows[index].uid.clone()),
                Some(rows[to_index].uid.clone()),
            );
        }
    }

    let to_index = match guidance.sequencing_mode {
        SequencingMode::DirectTo => guidance.direct_to.as_ref().and_then(|direct_to| {
            rows.iter()
                .position(|row| row.uid == direct_to.target_row.row_id().as_str())
        }),
        SequencingMode::FollowPlan | SequencingMode::Suspended => rows.iter().position(|row| {
            row.row_kind == FlightPlanDisplayRowKind::Waypoint
                && row.leg_index == Some(guidance.active_leg_index)
                && row.nav_ref.as_ref() == Some(&active_leg.to)
        }),
    };

    let Some(to_index) = to_index else {
        return (None, None);
    };
    let from_index = if guidance.sequencing_mode == SequencingMode::DirectTo
        && matches!(active_leg.from, NavRef::LatLon(_))
    {
        None
    } else {
        rows[..to_index].iter().rposition(|row| {
            row.row_kind == FlightPlanDisplayRowKind::Waypoint
                && row.nav_ref.as_ref() == Some(&active_leg.from)
        })
    };

    (
        from_index.map(|index| rows[index].uid.clone()),
        Some(rows[to_index].uid.clone()),
    )
}

pub fn terminal_hold_start_element_index_for_leg(
    plan: &FlightPlan,
    leg_index: usize,
) -> Option<usize> {
    let leg = plan.resolved_legs.get(leg_index)?;
    let ResolvedLegSource::RouteComponent { component_index } = leg.source else {
        return None;
    };
    let Some(RouteComponent::Procedure { procedure }) = plan.route_components.get(component_index)
    else {
        return None;
    };
    if procedure.terminal_discontinuity != Some(ProcedureDiscontinuity::Hold) {
        return None;
    }
    if last_guidance_leg_index_for_component(plan, component_index) != Some(leg_index) {
        return None;
    }
    let element_count = leg
        .procedure_provenance
        .as_ref()
        .and_then(|provenance| provenance.display_path.as_ref())
        .map(|path| path.elements.len())?;
    (element_count >= 5).then_some(element_count - 4)
}

pub fn terminal_hold_start_detail_index_for_leg(
    plan: &FlightPlan,
    leg_index: usize,
) -> Option<usize> {
    let hold_start_element = terminal_hold_start_element_index_for_leg(plan, leg_index)?;
    guidance_detail_refs(plan)
        .into_iter()
        .find(|detail| detail.leg_index == leg_index && detail.element_index == hold_start_element)
        .map(|detail| detail.detail_index)
}

fn replaceable_procedure_component_before(
    plan: &FlightPlan,
    component_index: usize,
) -> Option<usize> {
    let RouteComponent::Waypoint {
        waypoint: NavRef::Airport(ref airport_id),
    } = plan.route_components.get(component_index)?
    else {
        return None;
    };
    let previous_index = component_index.checked_sub(1)?;
    let RouteComponent::Procedure { procedure } = plan.route_components.get(previous_index)? else {
        return None;
    };
    if procedure.kind == ProcedureKind::Approach && procedure.airport_id.0 == *airport_id {
        Some(previous_index)
    } else {
        None
    }
}

fn can_insert_procedure_before_component(plan: &FlightPlan, component_index: usize) -> bool {
    if component_index == 0 {
        return true;
    }
    matches!(
        component_index
            .checked_sub(1)
            .and_then(|index| plan.route_components.get(index)),
        Some(RouteComponent::Waypoint { .. }) | Some(RouteComponent::Airway { .. })
    )
}

fn group_row_actions(component: &RouteComponentUiView) -> Vec<FlightPlanRowActionUiView> {
    match component.kind {
        RouteComponentViewKind::Airway => vec![
            core_session_action_with_disabled_reason(
                FlightPlanRowActionId::RemoveAirway,
                component.can_remove,
                AIRWAY_REMOVE_DISABLED_REASON,
            ),
            core_session_action(FlightPlanRowActionId::RemoveAllAbove, true),
        ],
        RouteComponentViewKind::Procedure => vec![
            action(
                FlightPlanRowActionId::ShowPlate,
                component.chart_airport_id.is_some() && component.procedure_id.is_some(),
            ),
            core_session_action_with_disabled_reason(
                FlightPlanRowActionId::RemoveProcedure,
                component.can_remove,
                PROCEDURE_REMOVE_DISABLED_REASON,
            ),
        ],
        RouteComponentViewKind::Waypoint => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn waypoint_actions_for_row(
    row_kind: FlightPlanDisplayRowKind,
    depth: usize,
    leg_index: Option<usize>,
    nav_ref: Option<&NavRef>,
    can_add_airway_after: bool,
    can_add_procedure_before: bool,
    can_remove_component: bool,
    can_reorder_up: bool,
    can_reorder_down: bool,
    component_index: Option<usize>,
    chart_airport_id: Option<&String>,
    origin_anchor: Option<&NavRef>,
) -> Vec<FlightPlanRowActionUiView> {
    if row_kind != FlightPlanDisplayRowKind::Waypoint {
        return Vec::new();
    }
    if depth == 0 {
        vec![
            core_session_action(FlightPlanRowActionId::ActivateLeg, leg_index.is_some()),
            core_session_action(FlightPlanRowActionId::DirectTo, nav_ref.is_some()),
            core_session_action_with_disabled_reason(
                FlightPlanRowActionId::Remove,
                component_index.is_some() && can_remove_component,
                WAYPOINT_REMOVE_DISABLED_REASON,
            ),
            core_session_action(
                FlightPlanRowActionId::RemoveAllAbove,
                component_index.is_some(),
            ),
            action(
                FlightPlanRowActionId::InsertBefore,
                component_index.is_some(),
            ),
            action(
                FlightPlanRowActionId::InsertAfter,
                component_index.is_some(),
            ),
            move_action(FlightPlanRowActionId::MoveUp, can_reorder_up),
            move_action(FlightPlanRowActionId::MoveDown, can_reorder_down),
            action(FlightPlanRowActionId::WaypointInfo, false),
            action(FlightPlanRowActionId::Weather, false),
            action(
                FlightPlanRowActionId::AddAirway,
                can_add_airway_after && origin_anchor.is_some(),
            ),
            action(
                FlightPlanRowActionId::SelectProcedure,
                can_add_procedure_before && component_index.is_some() && chart_airport_id.is_some(),
            ),
            action(FlightPlanRowActionId::Plates, chart_airport_id.is_some()),
        ]
    } else {
        vec![
            core_session_action(FlightPlanRowActionId::ActivateLeg, leg_index.is_some()),
            core_session_action(FlightPlanRowActionId::DirectTo, nav_ref.is_some()),
        ]
    }
}

fn child_waypoint_actions_for_row(
    row_kind: FlightPlanDisplayRowKind,
    component_kind: RouteComponentViewKind,
    leg_index: Option<usize>,
    nav_ref: Option<&NavRef>,
    waypoint_index: usize,
    waypoint_count: usize,
) -> Vec<FlightPlanRowActionUiView> {
    if row_kind != FlightPlanDisplayRowKind::Waypoint {
        return Vec::new();
    }
    let mut actions = vec![
        core_session_action(FlightPlanRowActionId::ActivateLeg, leg_index.is_some()),
        core_session_action(FlightPlanRowActionId::DirectTo, nav_ref.is_some()),
    ];
    if component_kind == RouteComponentViewKind::Airway {
        let is_endpoint = waypoint_index == 0 || waypoint_index + 1 == waypoint_count;
        actions.push(core_session_action_with_disabled_reason(
            FlightPlanRowActionId::Remove,
            is_endpoint,
            AIRWAY_ENDPOINT_REMOVE_DISABLED_REASON,
        ));
        actions.push(core_session_action(
            FlightPlanRowActionId::RemoveAllAbove,
            nav_ref.is_some(),
        ));
    }
    actions
}

fn top_level_waypoint_row_leg_index(plan: &FlightPlan, component_index: usize) -> Option<usize> {
    let Some(RouteComponent::Waypoint { waypoint }) = plan.route_components.get(component_index)
    else {
        return None;
    };
    if component_index == 0 {
        return None;
    }
    plan.resolved_legs
        .iter()
        .enumerate()
        .find(|(_, leg)| {
            &leg.to == waypoint
                && match leg.source {
                    ResolvedLegSource::RouteComponent {
                        component_index: source_component_index,
                    } => source_component_index + 1 == component_index,
                    ResolvedLegSource::SyntheticBridge {
                        to_component_index, ..
                    } => to_component_index == component_index,
                }
        })
        .map(|(index, _)| index)
}

fn child_waypoint_row_leg_index(
    plan: &FlightPlan,
    component_index: usize,
    item_index: usize,
    waypoint_occurrence: usize,
    nav_ref: &NavRef,
) -> Option<usize> {
    let local_leg_index = plan
        .resolved_legs
        .iter()
        .enumerate()
        .filter(|(_, leg)| {
            (matches!(leg.source, ResolvedLegSource::RouteComponent { component_index: source_component_index } if source_component_index == component_index)
                || matches!(leg.source, ResolvedLegSource::SyntheticBridge { to_component_index, .. } if to_component_index == component_index))
                && &leg.to == nav_ref
        })
        .nth(waypoint_occurrence)
        .map(|(index, _)| index);
    if local_leg_index.is_some() || item_index != 0 {
        return local_leg_index;
    }

    let first_component_leg_index =
        plan.resolved_legs
            .iter()
            .enumerate()
            .find_map(|(leg_index, leg)| match leg.source {
                ResolvedLegSource::RouteComponent {
                    component_index: source_component_index,
                } if source_component_index == component_index => Some(leg_index),
                _ => None,
            })?;
    first_component_leg_index
        .checked_sub(1)
        .filter(|previous_leg_index| {
            plan.resolved_legs
                .get(*previous_leg_index)
                .is_some_and(|leg| &leg.to == nav_ref)
        })
}

fn last_guidance_leg_index_for_component(
    plan: &FlightPlan,
    component_index: usize,
) -> Option<usize> {
    plan.resolved_legs
        .iter()
        .enumerate()
        .rev()
        .find(|(_, leg)| {
            matches!(leg.source, ResolvedLegSource::RouteComponent { component_index: source_component_index } if source_component_index == component_index)
        })
        .map(|(index, _)| index)
}

fn top_level_waypoint_row_uid(
    component: &RouteComponentUiView,
    _leg_index: Option<usize>,
) -> String {
    format!("component:{}:{:?}:waypoint", component.uid, component.kind)
}

fn child_waypoint_row_uid(
    component_uid: &str,
    kind: RouteComponentViewKind,
    occurrence: usize,
    nav_ref: &NavRef,
    _leg_index: Option<usize>,
) -> String {
    format!(
        "component:{component_uid}:{kind:?}:child:{occurrence}:{}",
        nav_ref_key(nav_ref)
    )
}

fn action(id: FlightPlanRowActionId, enabled: bool) -> FlightPlanRowActionUiView {
    let disabled_reason = row_action_disabled_reason(&id, enabled);
    FlightPlanRowActionUiView {
        label: action_label(&id).to_string(),
        uid: String::new(),
        id,
        enabled,
        disabled_reason,
        execution: FlightPlanRowActionExecution::UiController,
        dismiss_tray_on_success: true,
        navigation: None,
        weather_detail: None,
    }
}

fn core_session_action(id: FlightPlanRowActionId, enabled: bool) -> FlightPlanRowActionUiView {
    let disabled_reason = row_action_disabled_reason(&id, enabled);
    FlightPlanRowActionUiView {
        label: action_label(&id).to_string(),
        uid: String::new(),
        id,
        enabled,
        disabled_reason,
        execution: FlightPlanRowActionExecution::CoreSession,
        dismiss_tray_on_success: true,
        navigation: None,
        weather_detail: None,
    }
}

fn core_session_action_with_disabled_reason(
    id: FlightPlanRowActionId,
    enabled: bool,
    disabled_reason: &'static str,
) -> FlightPlanRowActionUiView {
    let mut action = core_session_action(id, enabled);
    if !enabled {
        action.disabled_reason = Some(disabled_reason.to_string());
    }
    action
}

fn move_action(id: FlightPlanRowActionId, enabled: bool) -> FlightPlanRowActionUiView {
    let disabled_reason = row_action_disabled_reason(&id, enabled);
    FlightPlanRowActionUiView {
        label: action_label(&id).to_string(),
        uid: String::new(),
        id,
        enabled,
        disabled_reason,
        execution: FlightPlanRowActionExecution::CoreSession,
        dismiss_tray_on_success: false,
        navigation: None,
        weather_detail: None,
    }
}

fn row_action_disabled_reason(id: &FlightPlanRowActionId, enabled: bool) -> Option<String> {
    (!enabled).then(|| {
        match id {
            FlightPlanRowActionId::ActivateLeg => "This row is not a flyable leg.",
            FlightPlanRowActionId::DirectTo => "This row has no Direct-To target.",
            FlightPlanRowActionId::Remove => WAYPOINT_REMOVE_DISABLED_REASON,
            FlightPlanRowActionId::RemoveAllAbove => REMOVE_ALL_ABOVE_DISABLED_REASON,
            FlightPlanRowActionId::InsertBefore => "Choose a top-level route row to insert before.",
            FlightPlanRowActionId::InsertAfter => "Choose a top-level route row to insert after.",
            FlightPlanRowActionId::MoveUp => "This route element is already at the top.",
            FlightPlanRowActionId::MoveDown => "This route element is already at the bottom.",
            FlightPlanRowActionId::WaypointInfo => "Waypoint info is not available yet.",
            FlightPlanRowActionId::Weather => {
                "No METAR, TAF, or airport NOTAM is available for this station."
            }
            FlightPlanRowActionId::AddAirway => {
                "Airway insertion requires a named waypoint with airway connections."
            }
            FlightPlanRowActionId::SelectProcedure => "Procedures can be added at airports only.",
            FlightPlanRowActionId::Plates => "No airport plates are associated with this row.",
            FlightPlanRowActionId::ShowPlate => "This procedure has no plate to show.",
            FlightPlanRowActionId::RemoveAirway => AIRWAY_REMOVE_DISABLED_REASON,
            FlightPlanRowActionId::RemoveProcedure => PROCEDURE_REMOVE_DISABLED_REASON,
        }
        .to_string()
    })
}

fn assign_action_uids(
    row_uid: &str,
    mut actions: Vec<FlightPlanRowActionUiView>,
) -> Vec<FlightPlanRowActionUiView> {
    for action in &mut actions {
        if action.uid.is_empty() {
            action.uid = format!("{row_uid}:action:{:?}", action.id);
        }
    }
    actions
}

fn action_matrix_from_actions(
    actions: &[FlightPlanRowActionUiView],
) -> Vec<Vec<FlightPlanRowActionUiView>> {
    let rows = vec![
        vec![
            FlightPlanRowActionId::ActivateLeg,
            FlightPlanRowActionId::DirectTo,
        ],
        vec![
            FlightPlanRowActionId::InsertBefore,
            FlightPlanRowActionId::MoveUp,
        ],
        vec![
            FlightPlanRowActionId::InsertAfter,
            FlightPlanRowActionId::MoveDown,
        ],
        vec![
            FlightPlanRowActionId::Remove,
            FlightPlanRowActionId::RemoveAllAbove,
        ],
        vec![
            FlightPlanRowActionId::AddAirway,
            FlightPlanRowActionId::SelectProcedure,
        ],
        vec![
            FlightPlanRowActionId::WaypointInfo,
            FlightPlanRowActionId::Plates,
        ],
        vec![FlightPlanRowActionId::Weather],
    ];
    let mut used = Vec::new();
    let mut matrix = rows
        .into_iter()
        .filter_map(|row| {
            let actions_in_row = row
                .into_iter()
                .filter_map(|id| {
                    actions.iter().find(|action| action.id == id).map(|action| {
                        used.push(id);
                        action.clone()
                    })
                })
                .collect::<Vec<_>>();
            (!actions_in_row.is_empty()).then_some(actions_in_row)
        })
        .collect::<Vec<_>>();
    for action in actions {
        if !used.iter().any(|id| *id == action.id) {
            matrix.push(vec![action.clone()]);
        }
    }
    matrix
}

pub(crate) fn flight_plan_row_actions(
    row: &FlightPlanDisplayRowUiView,
) -> impl Iterator<Item = &FlightPlanRowActionUiView> {
    row.action_matrix.iter().flatten()
}

pub(crate) fn flight_plan_row_actions_mut(
    row: &mut FlightPlanDisplayRowUiView,
) -> impl Iterator<Item = &mut FlightPlanRowActionUiView> {
    row.action_matrix.iter_mut().flatten()
}

pub(crate) fn refresh_flight_plan_row_action_navigation(row: &mut FlightPlanDisplayRowUiView) {
    let chart_airport_id = row.chart_airport_id.clone();
    let show_plate_target_id = row.show_plate_target_id.clone();
    for action in flight_plan_row_actions_mut(row) {
        action.navigation = match action.id {
            FlightPlanRowActionId::Plates => chart_airport_id.as_ref().map(|airport_id| {
                FlightPlanRowNavigationAction::OpenAirportCharts {
                    airport_id: airport_id.clone(),
                }
            }),
            FlightPlanRowActionId::ShowPlate => {
                match (chart_airport_id.as_ref(), show_plate_target_id.as_ref()) {
                    (Some(airport_id), Some(target)) => {
                        Some(FlightPlanRowNavigationAction::OpenPlateTarget {
                            airport_id: airport_id.clone(),
                            target: target.clone(),
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        };
    }
}

fn nav_ref_key(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(id) => format!("airport:{id}"),
        NavRef::Navaid(id) => format!("navaid:{id}"),
        NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => format!("arinc-navaid:{section_code}:{subsection_code}:{icao_code}:{identifier}"),
        NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => format!(
            "terminal-navaid:{airport_id}:{section_code}:{subsection_code}:{icao_code}:{identifier}"
        ),
        NavRef::Fix(id) => format!("fix:{id}"),
        NavRef::LatLon(position) => format!("latlon:{:.7}:{:.7}", position.lat, position.lon),
        NavRef::Spot(position) => format!("spot:{:.7}:{:.7}", position.lat, position.lon),
    }
}

fn action_label(id: &FlightPlanRowActionId) -> &'static str {
    match id {
        FlightPlanRowActionId::ActivateLeg => "Activate Leg",
        FlightPlanRowActionId::DirectTo => "Direct-To",
        FlightPlanRowActionId::Remove => "Remove",
        FlightPlanRowActionId::RemoveAllAbove => "Remove All Above",
        FlightPlanRowActionId::InsertBefore => "Insert Before",
        FlightPlanRowActionId::InsertAfter => "Insert After",
        FlightPlanRowActionId::MoveUp => "Move Up",
        FlightPlanRowActionId::MoveDown => "Move Down",
        FlightPlanRowActionId::WaypointInfo => "Waypoint Info",
        FlightPlanRowActionId::Weather => "WX",
        FlightPlanRowActionId::AddAirway => "Add Airway",
        FlightPlanRowActionId::SelectProcedure => "Select Procedure",
        FlightPlanRowActionId::Plates => "Plates",
        FlightPlanRowActionId::ShowPlate => "Show Plate",
        FlightPlanRowActionId::RemoveAirway => "Remove Airway",
        FlightPlanRowActionId::RemoveProcedure => "Remove Procedure",
    }
}

fn projected_component_waypoint_nav_ref(component: &RouteComponentUiView) -> Option<NavRef> {
    component.items.iter().find_map(|item| match item {
        ConcretizedNavItem::Waypoint { nav_ref } => Some(nav_ref.clone()),
        ConcretizedNavItem::Discontinuity { .. } => None,
    })
}

fn component_procedure_id(component: &RouteComponent) -> Option<String> {
    match component {
        RouteComponent::Procedure { procedure } => Some(procedure.procedure_id.clone()),
        _ => None,
    }
}

fn component_procedure_kind(component: &RouteComponent) -> Option<ProcedureKind> {
    match component {
        RouteComponent::Procedure { procedure } => Some(procedure.kind.clone()),
        _ => None,
    }
}

fn component_chart_airport_id(component: &RouteComponent) -> Option<String> {
    match component {
        RouteComponent::Waypoint { waypoint } => airport_id_from_nav_ref(waypoint),
        RouteComponent::Procedure { procedure } => Some(procedure.airport_id.0.clone()),
        RouteComponent::Airway { .. } => None,
    }
}

fn airport_id_from_nav_ref(nav_ref: &NavRef) -> Option<String> {
    match nav_ref {
        NavRef::Airport(code) => Some(code.clone()),
        _ => None,
    }
}

fn structured_component_label(component: &RouteComponentUiView) -> String {
    component.summary.clone()
}

fn adjacent_waypoint_component(
    components: &[RouteComponent],
    component_index: usize,
    direction: isize,
) -> Option<NavRef> {
    let adjacent_index = if direction < 0 {
        component_index.checked_sub(direction.unsigned_abs())?
    } else {
        component_index.checked_add(direction as usize)?
    };
    match components.get(adjacent_index) {
        Some(RouteComponent::Waypoint { waypoint }) => Some(waypoint.clone()),
        _ => None,
    }
}

fn can_remove_component(plan: &FlightPlan, component_index: usize) -> bool {
    delete_component(plan, component_index).is_ok()
}

fn can_reorder_component(plan: &FlightPlan, component_index: usize) -> bool {
    component_index < plan.route_components.len() && plan.route_components.len() > 1
}

fn can_reorder_component_in_direction(
    plan: &FlightPlan,
    component_index: usize,
    direction: isize,
) -> bool {
    if !can_reorder_component(plan, component_index) {
        return false;
    }
    let target_index = if direction < 0 {
        component_index.checked_sub(direction.unsigned_abs())
    } else {
        component_index.checked_add(direction as usize)
    };
    target_index.is_some_and(|index| index < plan.route_components.len())
}

#[derive(Debug, Clone)]
struct RebuiltRouteComponent {
    // Preserved rows carry their old stable UID; genuinely new rows leave this empty.
    // Guidance restoration anchors to these UIDs instead of trusting shifted indices.
    uid: Option<String>,
    component: RouteComponent,
    preserved_legs: Option<Vec<ResolvedLeg>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuidanceRebuildPolicy {
    PreserveByRowUid,
    Clear,
}

#[derive(Debug, Clone)]
struct ActiveGuidanceAnchor {
    from_row_uid: Option<String>,
    to_row_uid: Option<String>,
    guidance: GuidanceState,
    active_detail_element_index: Option<usize>,
}

fn rebuilt_existing_component(
    plan: &FlightPlan,
    old_grouped_legs: &BTreeMap<usize, Vec<ResolvedLeg>>,
    old_index: usize,
) -> RebuiltRouteComponent {
    RebuiltRouteComponent {
        uid: plan.route_component_uids.get(old_index).cloned(),
        component: plan.route_components[old_index].clone(),
        preserved_legs: old_grouped_legs.get(&old_index).cloned(),
    }
}

fn rebuilt_new_component(
    component: RouteComponent,
    preserved_legs: Option<Vec<ResolvedLeg>>,
) -> RebuiltRouteComponent {
    RebuiltRouteComponent {
        uid: None,
        component,
        preserved_legs,
    }
}

fn rebuild_plan_from_uid_components(
    old_plan: &FlightPlan,
    rebuilt_components: Vec<RebuiltRouteComponent>,
    guidance_policy: GuidanceRebuildPolicy,
) -> AppResult<FlightPlan> {
    let old_plan = old_plan.clone().normalized();
    let old_guidance = old_plan.guidance.clone();
    let active_anchor = (guidance_policy == GuidanceRebuildPolicy::PreserveByRowUid)
        .then(|| capture_active_guidance_anchor(&old_plan))
        .flatten();
    let (route_components, route_component_uids, route_component_uid_counter, grouped_legs) =
        materialize_rebuilt_components(&old_plan, rebuilt_components);
    let resolved_legs =
        rebuild_resolved_legs_with_grouped_components(&route_components, &grouped_legs);
    let mut plan = FlightPlan {
        route_components,
        route_component_uids,
        route_component_uid_counter,
        resolved_legs,
        guidance: None,
        ..old_plan
    }
    .normalized();
    plan.guidance = match guidance_policy {
        GuidanceRebuildPolicy::PreserveByRowUid => match active_anchor {
            Some(anchor) => restore_guidance_from_row_uid_anchor(&plan, &anchor)?,
            None => revalidate_guidance_after_plan_edit(old_guidance, &plan)?,
        },
        GuidanceRebuildPolicy::Clear => None,
    };
    Ok(plan)
}

pub(crate) fn rebuild_plan_with_nav_materializations(
    plan: &FlightPlan,
    replacements: Vec<(usize, RouteComponent, Vec<ResolvedLeg>)>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let replacements = replacements
        .into_iter()
        .map(|(index, component, legs)| (index, (component, legs)))
        .collect::<BTreeMap<_, _>>();
    let old_grouped_legs = grouped_component_legs(&plan);
    let rebuilt_components = (0..plan.route_components.len())
        .map(|index| {
            if let Some((component, legs)) = replacements.get(&index) {
                RebuiltRouteComponent {
                    uid: plan.route_component_uids.get(index).cloned(),
                    component: component.clone(),
                    preserved_legs: Some(legs.clone()),
                }
            } else {
                rebuilt_existing_component(&plan, &old_grouped_legs, index)
            }
        })
        .collect();
    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

fn allocate_route_component_uid(next_counter: &mut u64) -> String {
    let uid = format!("fpc:{next_counter:016x}");
    *next_counter += 1;
    uid
}

fn materialize_rebuilt_components(
    plan: &FlightPlan,
    rebuilt_components: Vec<RebuiltRouteComponent>,
) -> (
    Vec<RouteComponent>,
    Vec<String>,
    u64,
    BTreeMap<usize, Vec<ResolvedLeg>>,
) {
    let mut next_counter = plan.route_component_uid_counter;
    let mut seen = BTreeMap::<String, ()>::new();
    let mut route_components = Vec::with_capacity(rebuilt_components.len());
    let mut route_component_uids = Vec::with_capacity(rebuilt_components.len());
    let mut grouped_legs = BTreeMap::<usize, Vec<ResolvedLeg>>::new();

    for (new_index, rebuilt) in rebuilt_components.into_iter().enumerate() {
        let uid = rebuilt
            .uid
            .filter(|uid| !uid.is_empty() && !seen.contains_key(uid))
            .unwrap_or_else(|| allocate_route_component_uid(&mut next_counter));
        seen.insert(uid.clone(), ());
        if let Some(legs) = rebuilt.preserved_legs {
            grouped_legs.insert(new_index, rewrite_grouped_legs_source(&legs, new_index));
        }
        route_component_uids.push(uid);
        route_components.push(rebuilt.component);
    }

    (
        route_components,
        route_component_uids,
        next_counter,
        grouped_legs,
    )
}

fn capture_active_guidance_anchor(plan: &FlightPlan) -> Option<ActiveGuidanceAnchor> {
    let guidance = plan.guidance.clone()?;
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        return None;
    }
    let rows = guidance_anchor_display_rows(plan);
    let (from_row_uid, to_row_uid) = active_guidance_row_uids(plan, &rows);
    let active_detail_element_index = guidance
        .active_detail_index
        .and_then(|detail_index| guidance_detail_ref_by_index(plan, detail_index))
        .filter(|detail| detail.leg_index == guidance.active_leg_index)
        .map(|detail| detail.element_index);
    Some(ActiveGuidanceAnchor {
        from_row_uid,
        to_row_uid,
        guidance,
        active_detail_element_index,
    })
}

fn restore_guidance_from_row_uid_anchor(
    plan: &FlightPlan,
    anchor: &ActiveGuidanceAnchor,
) -> AppResult<Option<GuidanceState>> {
    if anchor.to_row_uid.is_none() {
        return Ok(None);
    }

    for leg_index in 0..plan.resolved_legs.len() {
        let mut candidate_guidance = anchor.guidance.clone();
        candidate_guidance.active_leg_index = leg_index;
        candidate_guidance.direct_to = None;
        candidate_guidance.display_split_leg_id =
            plan.resolved_legs.get(leg_index).map(|leg| leg.id.clone());
        candidate_guidance.active_detail_index =
            active_detail_index_for_restored_leg(plan, leg_index, anchor);
        let candidate_plan = FlightPlan {
            guidance: Some(candidate_guidance.clone()),
            ..plan.clone()
        };
        let rows = guidance_anchor_display_rows(&candidate_plan);
        let candidate_uids = active_guidance_row_uids(&candidate_plan, &rows);
        if candidate_uids == (anchor.from_row_uid.clone(), anchor.to_row_uid.clone()) {
            return revalidate_guidance_after_plan_edit(Some(candidate_guidance), &candidate_plan);
        }
    }

    Ok(None)
}

fn active_detail_index_for_restored_leg(
    plan: &FlightPlan,
    leg_index: usize,
    anchor: &ActiveGuidanceAnchor,
) -> Option<usize> {
    let element_index = anchor.active_detail_element_index.unwrap_or(0);
    guidance_detail_refs(plan)
        .into_iter()
        .find(|detail| detail.leg_index == leg_index && detail.element_index == element_index)
        .map(|detail| detail.detail_index)
        .or_else(|| first_guidance_detail_index_for_leg(plan, leg_index))
}

fn guidance_anchor_display_rows(plan: &FlightPlan) -> Vec<FlightPlanDisplayRowUiView> {
    let plan = plan.clone().normalized();
    let grouped_legs = grouped_component_legs(&plan);
    let projected_items =
        dedupe_component_items_for_projection(&plan.route_components, &grouped_legs);
    let active_component_index = plan
        .guidance
        .as_ref()
        .and_then(|guidance| active_component_index_for_guidance(&plan, guidance));
    let components = plan
        .route_components
        .iter()
        .enumerate()
        .map(|(component_index, component)| {
            let component_nav_ref = match component {
                RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
                RouteComponent::Airway { .. } | RouteComponent::Procedure { .. } => None,
            };
            RouteComponentUiView {
                uid: component_uid(&plan, component_index),
                component_index,
                kind: component_view_kind(component),
                summary: component_summary(component),
                procedure_id: component_procedure_id(component),
                procedure_kind: component_procedure_kind(component),
                chart_airport_id: component_chart_airport_id(component),
                nav_ref: component_nav_ref,
                items: projected_items
                    .get(component_index)
                    .cloned()
                    .unwrap_or_default(),
                active: active_component_index == Some(component_index),
                can_add_airway_after: false,
                can_add_procedure_before: false,
                can_remove: false,
                can_reorder: false,
                can_reorder_up: false,
                can_reorder_down: false,
                replace_procedure_component_index: None,
                preceding_waypoint: adjacent_waypoint_component(
                    &plan.route_components,
                    component_index,
                    -1,
                ),
                following_waypoint: adjacent_waypoint_component(
                    &plan.route_components,
                    component_index,
                    1,
                ),
            }
        })
        .collect::<Vec<_>>();
    project_display_rows(&plan, &components, &[])
}

pub fn delete_component(plan: &FlightPlan, component_index: usize) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }

    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::new();
    for old_index in 0..plan.route_components.len() {
        if old_index == component_index {
            continue;
        }
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

fn airway_points_and_legs(
    plan: &FlightPlan,
    component_index: usize,
) -> AppResult<(AirwaySegment, Vec<NavRef>, Vec<ResolvedLeg>)> {
    let plan = plan.clone().normalized();
    let airway = match plan.route_components.get(component_index) {
        Some(RouteComponent::Airway { airway }) => airway.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };
    let legs = grouped_component_legs(&plan)
        .get(&component_index)
        .cloned()
        .unwrap_or_default();
    let mut points = Vec::new();
    if let Some(first) = legs.first() {
        points.push(first.from.clone());
        points.extend(legs.iter().map(|leg| leg.to.clone()));
    } else {
        points.push(airway.entry.clone());
        if airway.exit != airway.entry {
            points.push(airway.exit.clone());
        }
    }
    Ok((airway, points, legs))
}

fn rebuild_with_airway_replacement(
    plan: &FlightPlan,
    component_index: usize,
    replacement: Option<(RouteComponent, Option<Vec<ResolvedLeg>>)>,
    drop_before_target: bool,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }

    let start_index = if drop_before_target {
        component_index
    } else {
        0
    };
    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::new();

    for old_index in start_index..plan.route_components.len() {
        if old_index == component_index {
            let Some((component, replacement_legs)) = replacement.clone() else {
                continue;
            };
            rebuilt_components.push(RebuiltRouteComponent {
                uid: plan.route_component_uids.get(old_index).cloned(),
                component,
                preserved_legs: replacement_legs,
            });
            continue;
        }

        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

fn airway_replacement_from_remaining_points(
    airway: AirwaySegment,
    remaining_points: Vec<NavRef>,
    remaining_legs: Vec<ResolvedLeg>,
    following_waypoint: Option<NavRef>,
) -> Option<(RouteComponent, Option<Vec<ResolvedLeg>>)> {
    match remaining_points.as_slice() {
        [] => None,
        [single] if following_waypoint.as_ref() == Some(single) => None,
        [single] => Some((
            RouteComponent::Waypoint {
                waypoint: single.clone(),
            },
            None,
        )),
        [entry, .., exit] => Some((
            RouteComponent::Airway {
                airway: AirwaySegment {
                    entry: entry.clone(),
                    exit: exit.clone(),
                    ..airway
                },
            },
            Some(remaining_legs),
        )),
    }
}

pub fn remove_airway_child_waypoint(
    plan: &FlightPlan,
    component_index: usize,
    nav_ref: &NavRef,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let (airway, points, legs) = airway_points_and_legs(&plan, component_index)?;
    let Some(point_index) = points.iter().position(|point| point == nav_ref) else {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "airway child waypoint is not in component: {}",
                nav_ref_label(nav_ref)
            ),
        });
    };
    let last_index = points.len().saturating_sub(1);
    let preceding_waypoint =
        adjacent_waypoint_component(&plan.route_components, component_index, -1);
    let following_waypoint =
        adjacent_waypoint_component(&plan.route_components, component_index, 1);
    let first_visible_index = if preceding_waypoint.as_ref() == points.first() && points.len() > 1 {
        1
    } else {
        0
    };
    let last_visible_index = if following_waypoint.as_ref() == points.last() && points.len() > 1 {
        last_index.saturating_sub(1)
    } else {
        last_index
    };
    if point_index != first_visible_index && point_index != last_visible_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: AIRWAY_ENDPOINT_REMOVE_DISABLED_REASON.to_string(),
        });
    }

    let (remaining_points, remaining_legs) = if point_index == first_visible_index {
        (
            points[point_index + 1..].to_vec(),
            legs.get(point_index + 1..).unwrap_or(&[]).to_vec(),
        )
    } else {
        (
            points[..point_index].to_vec(),
            legs.get(..point_index.saturating_sub(1))
                .unwrap_or(&[])
                .to_vec(),
        )
    };
    let replacement = airway_replacement_from_remaining_points(
        airway,
        remaining_points,
        remaining_legs,
        following_waypoint,
    );
    rebuild_with_airway_replacement(&plan, component_index, replacement, false)
}

pub fn remove_all_above_airway_child_waypoint(
    plan: &FlightPlan,
    component_index: usize,
    nav_ref: &NavRef,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let (airway, points, legs) = airway_points_and_legs(&plan, component_index)?;
    let Some(point_index) = points.iter().position(|point| point == nav_ref) else {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "airway child waypoint is not in component: {}",
                nav_ref_label(nav_ref)
            ),
        });
    };

    let remaining_points = points.get(point_index + 1..).unwrap_or(&[]).to_vec();
    let remaining_legs = legs.get(point_index + 1..).unwrap_or(&[]).to_vec();
    let following_waypoint =
        adjacent_waypoint_component(&plan.route_components, component_index, 1);
    let replacement = airway_replacement_from_remaining_points(
        airway,
        remaining_points,
        remaining_legs,
        following_waypoint,
    );
    rebuild_with_airway_replacement(&plan, component_index, replacement, true)
}

pub fn remove_all_above(plan: &FlightPlan, component_index: usize) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }
    let keep_from = component_index.saturating_add(1);
    let old_grouped_legs = grouped_component_legs(&plan);
    let rebuilt_components = (keep_from..plan.route_components.len())
        .map(|old_index| rebuilt_existing_component(&plan, &old_grouped_legs, old_index))
        .collect::<Vec<_>>();

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

pub fn delete_waypoint_component(
    plan: &FlightPlan,
    component_index: usize,
) -> AppResult<FlightPlan> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Waypoint { .. }) => delete_component(plan, component_index),
        Some(RouteComponent::Airway { .. }) | Some(RouteComponent::Procedure { .. }) => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot delete inside a grouped route component directly; flatten or remove the whole component".to_string(),
        }),
        None => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

pub fn insert_airport_waypoint(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    airport_id: &str,
) -> AppResult<FlightPlan> {
    let airport_id = airport_id.trim().to_ascii_uppercase();
    if airport_id.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "airport id is required".to_string(),
        });
    }

    insert_waypoint(plan, component_index, before, NavRef::Airport(airport_id))
}

pub fn flight_plan_contains_nav_ref(plan: &FlightPlan, nav_ref: &NavRef) -> bool {
    plan.route_components
        .iter()
        .any(|component| match component {
            RouteComponent::Waypoint { waypoint } => waypoint == nav_ref,
            RouteComponent::Airway { airway } => {
                airway.entry == *nav_ref || airway.exit == *nav_ref
            }
            RouteComponent::Procedure { procedure } => {
                matches!(nav_ref, NavRef::Airport(id) if id == &procedure.airport_id.0)
            }
        })
}

pub fn flight_plan_has_direct_to_overlay(plan: &FlightPlan) -> bool {
    let direct_to = plan.guidance.as_ref().and_then(|guidance| {
        (guidance.sequencing_mode == SequencingMode::DirectTo)
            .then_some(guidance.direct_to.as_ref())
            .flatten()
    });
    direct_to.is_some_and(|state| !state.target_row.is_planned())
}

pub fn top_level_waypoint_component_index(plan: &FlightPlan, nav_ref: &NavRef) -> Option<usize> {
    plan.route_components
        .iter()
        .position(|component| matches!(component, RouteComponent::Waypoint { waypoint } if waypoint == nav_ref))
}

pub fn top_level_waypoint_component_count(plan: &FlightPlan, nav_ref: &NavRef) -> usize {
    plan.route_components
        .iter()
        .filter(|component| matches!(component, RouteComponent::Waypoint { waypoint } if waypoint == nav_ref))
        .count()
}

pub fn insert_waypoint(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    waypoint: NavRef,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }

    let inserted = RouteComponent::Waypoint { waypoint };
    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::with_capacity(plan.route_components.len() + 1);
    for old_index in 0..plan.route_components.len() {
        if old_index == component_index && before {
            rebuilt_components.push(rebuilt_new_component(inserted.clone(), None));
        }
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
        if old_index == component_index && !before {
            rebuilt_components.push(rebuilt_new_component(inserted.clone(), None));
        }
    }

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

pub fn move_component(
    plan: &FlightPlan,
    component_index: usize,
    delta: isize,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if delta == 0 {
        return Ok(plan);
    }
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }

    let target_index = component_index as isize + delta;
    if target_index < 0 || target_index >= plan.route_components.len() as isize {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component move out of bounds: {component_index} -> {target_index}"),
        });
    }

    let mut old_indices = (0..plan.route_components.len()).collect::<Vec<_>>();
    let moved = old_indices.remove(component_index);
    old_indices.insert(target_index as usize, moved);

    let old_grouped_legs = grouped_component_legs(&plan);
    let rebuilt_components = old_indices
        .into_iter()
        .map(|old_index| rebuilt_existing_component(&plan, &old_grouped_legs, old_index))
        .collect::<Vec<_>>();

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

pub fn flatten_component_to_waypoints(
    plan: &FlightPlan,
    component_index: usize,
    waypoints: Vec<NavRef>,
) -> AppResult<FlightPlan> {
    if waypoints.len() < 2 {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "flattening requires at least two waypoints".to_string(),
        });
    }

    match plan.route_components.get(component_index) {
        Some(RouteComponent::Waypoint { .. }) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "waypoint components are already explicit".to_string(),
            })
        }
        Some(RouteComponent::Airway { .. }) | Some(RouteComponent::Procedure { .. }) => {}
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    let plan = plan.clone().normalized();
    let replacements = waypoints
        .into_iter()
        .map(|waypoint| RouteComponent::Waypoint { waypoint })
        .collect::<Vec<_>>();
    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::new();
    for old_index in 0..plan.route_components.len() {
        if old_index == component_index {
            for replacement in replacements.iter().cloned() {
                rebuilt_components.push(rebuilt_new_component(replacement, None));
            }
            continue;
        }
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )
}

pub fn insert_airway_between_waypoints(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    if start_component_index >= end_component_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "airway insertion requires an increasing waypoint span, got start={start_component_index} end={end_component_index}"
            ),
        });
    }

    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway insertion start must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }
    match plan.route_components.get(end_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway insertion end must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {end_component_index}"),
            })
        }
    }

    if plan.route_components[start_component_index + 1..end_component_index]
        .iter()
        .any(|component| !component.is_waypoint())
    {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message:
                "airway insertion cannot replace a span containing grouped components; flatten or remove them first"
                    .to_string(),
        });
    }

    let consume_start = matches!(
        plan.route_components.get(start_component_index),
        Some(RouteComponent::Waypoint { waypoint }) if waypoint == &airway.entry
    );
    let consume_end = matches!(
        plan.route_components.get(end_component_index),
        Some(RouteComponent::Waypoint { waypoint }) if waypoint == &airway.exit
    );
    let preserve_start_end = if consume_start {
        start_component_index
    } else {
        start_component_index + 1
    };
    let preserve_end_start = if consume_end {
        end_component_index + 1
    } else {
        end_component_index
    };

    let mut rebuilt_components = Vec::<RebuiltRouteComponent>::new();
    let old_grouped_legs = grouped_component_legs(&plan);

    for old_index in 0..preserve_start_end {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    rebuilt_components.push(rebuilt_new_component(
        RouteComponent::Airway {
            airway: airway.clone(),
        },
        Some(airway_legs),
    ));

    for old_index in preserve_end_start..plan.route_components.len() {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway insertion"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn insert_airway_after_waypoint(
    plan: &FlightPlan,
    start_component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway tail insertion start must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }

    if start_component_index + 1 != plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message:
                "airway tail insertion requires the selected waypoint to be the end of the route"
                    .to_string(),
        });
    }

    let consume_start = matches!(
        plan.route_components.get(start_component_index),
        Some(RouteComponent::Waypoint { waypoint }) if waypoint == &airway.entry
    );
    let preserve_start_end = if consume_start {
        start_component_index
    } else {
        start_component_index + 1
    };

    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::<RebuiltRouteComponent>::new();
    for old_index in 0..preserve_start_end {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }
    rebuilt_components.push(rebuilt_new_component(
        RouteComponent::Airway {
            airway: airway.clone(),
        },
        Some(airway_legs),
    ));

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway insertion"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn insert_airway_after_airway(
    plan: &FlightPlan,
    start_component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Airway {
            airway: existing_airway,
        }) if existing_airway.exit == airway.entry => {}
        Some(RouteComponent::Airway {
            airway: existing_airway,
        }) => {
            return Err(AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "{} cannot start after {}",
                    airway.name,
                    nav_ref_label(&existing_airway.exit)
                ),
            })
        }
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway chaining start must be an airway component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }

    if start_component_index + 1 != plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "airway chaining requires the selected airway to be the end of the route"
                .to_string(),
        });
    }

    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = (0..plan.route_components.len())
        .map(|old_index| rebuilt_existing_component(&plan, &old_grouped_legs, old_index))
        .collect::<Vec<_>>();
    rebuilt_components.push(rebuilt_new_component(
        RouteComponent::Airway {
            airway: airway.clone(),
        },
        Some(airway_legs),
    ));

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway insertion"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn insert_procedure_between_waypoints(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    procedure: ProcedureSegment,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    if start_component_index >= end_component_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "procedure insertion requires an increasing waypoint span, got start={start_component_index} end={end_component_index}"
            ),
        });
    }

    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "procedure insertion start must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }
    match plan.route_components.get(end_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "procedure insertion end must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {end_component_index}"),
            })
        }
    }

    if plan.route_components[start_component_index + 1..end_component_index]
        .iter()
        .any(|component| !component.is_waypoint())
    {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message:
                "procedure insertion cannot replace a span containing grouped components; flatten or remove them first"
                    .to_string(),
        });
    }

    let mut rebuilt_components = Vec::<RebuiltRouteComponent>::new();
    let old_grouped_legs = grouped_component_legs(&plan);

    for old_index in 0..=start_component_index {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    rebuilt_components.push(rebuilt_new_component(
        RouteComponent::Procedure {
            procedure: procedure.clone(),
        },
        Some(procedure_legs),
    ));

    for old_index in end_component_index..plan.route_components.len() {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after procedure insertion"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn insert_initial_procedure_before_airport(
    plan: &FlightPlan,
    airport_component_index: usize,
    procedure: ProcedureSegment,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    if airport_component_index != 0 {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "initial procedure insertion requires the first component, got {airport_component_index}"
            ),
        });
    }

    let plan = plan.clone().normalized();
    match plan.route_components.get(airport_component_index) {
        Some(RouteComponent::Waypoint {
            waypoint: NavRef::Airport(_),
        }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "initial procedure insertion target must be an airport waypoint"
                    .to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "component index out of bounds: 0".to_string(),
            })
        }
    }

    let old_grouped_legs = grouped_component_legs(&plan);
    let mut rebuilt_components = Vec::<RebuiltRouteComponent>::new();

    rebuilt_components.push(rebuilt_new_component(
        RouteComponent::Procedure {
            procedure: procedure.clone(),
        },
        Some(procedure_legs),
    ));

    for old_index in 0..plan.route_components.len() {
        rebuilt_components.push(rebuilt_existing_component(
            &plan,
            &old_grouped_legs,
            old_index,
        ));
    }

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after procedure insertion"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn materialize_airway_exit_before_component(
    plan: &FlightPlan,
    component_index: usize,
) -> AppResult<(FlightPlan, usize)> {
    let plan = plan.clone().normalized();
    let previous_index = component_index.checked_sub(1).ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "target component has no predecessor".to_string(),
    })?;
    let Some(RouteComponent::Airway { airway }) = plan.route_components.get(previous_index) else {
        return Ok((plan, component_index));
    };
    if matches!(
        plan.route_components.get(component_index),
        Some(RouteComponent::Waypoint { waypoint }) if waypoint == &airway.exit
    ) {
        return Ok((plan, component_index));
    }

    let repaired = insert_waypoint(&plan, previous_index, false, airway.exit.clone())?;
    Ok((repaired, component_index + 1))
}

pub fn replace_airway_component(
    plan: &FlightPlan,
    component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Airway { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    if airway_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "replacement airway must contain at least one resolved leg".to_string(),
        });
    }

    let old_grouped_legs = grouped_component_legs(&plan);
    let rebuilt_components = (0..plan.route_components.len())
        .map(|old_index| {
            if old_index == component_index {
                RebuiltRouteComponent {
                    uid: plan.route_component_uids.get(old_index).cloned(),
                    component: RouteComponent::Airway {
                        airway: airway.clone(),
                    },
                    preserved_legs: Some(airway_legs.clone()),
                }
            } else {
                rebuilt_existing_component(&plan, &old_grouped_legs, old_index)
            }
        })
        .collect::<Vec<_>>();

    let rebuilt = rebuild_plan_from_uid_components(
        &plan,
        rebuilt_components,
        GuidanceRebuildPolicy::PreserveByRowUid,
    )?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway replacement"
                .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn replace_procedure_component(
    plan: &FlightPlan,
    component_index: usize,
    procedure: ProcedureSegment,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Procedure { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    if procedure_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "replacement procedure must contain at least one resolved leg".to_string(),
        });
    }

    let old_grouped_legs = grouped_component_legs(&plan);
    let rebuilt_components = (0..plan.route_components.len())
        .map(|old_index| {
            if old_index == component_index {
                RebuiltRouteComponent {
                    uid: plan.route_component_uids.get(old_index).cloned(),
                    component: RouteComponent::Procedure {
                        procedure: procedure.clone(),
                    },
                    preserved_legs: Some(procedure_legs.clone()),
                }
            } else {
                rebuilt_existing_component(&plan, &old_grouped_legs, old_index)
            }
        })
        .collect::<Vec<_>>();

    let rebuilt =
        rebuild_plan_from_uid_components(&plan, rebuilt_components, GuidanceRebuildPolicy::Clear)?;
    if rebuilt.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message:
                "flight plan must contain at least one flyable leg after procedure replacement"
                    .to_string(),
        });
    }
    Ok(rebuilt)
}

pub fn change_airway_entry(
    plan: &FlightPlan,
    component_index: usize,
    entry: NavRef,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Airway { airway }) => airway.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_airway_component(
        plan,
        component_index,
        AirwaySegment { entry, ..existing },
        airway_legs,
    )
}

pub fn change_airway_exit(
    plan: &FlightPlan,
    component_index: usize,
    exit: NavRef,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Airway { airway }) => airway.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_airway_component(
        plan,
        component_index,
        AirwaySegment { exit, ..existing },
        airway_legs,
    )
}

pub fn change_procedure_enroute_transition(
    plan: &FlightPlan,
    component_index: usize,
    enroute_transition: Option<String>,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Procedure { procedure }) => procedure.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_procedure_component(
        plan,
        component_index,
        ProcedureSegment {
            enroute_transition,
            ..existing
        },
        procedure_legs,
    )
}

pub fn change_procedure_runway_transition(
    plan: &FlightPlan,
    component_index: usize,
    runway_transition: Option<String>,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Procedure { procedure }) => procedure.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_procedure_component(
        plan,
        component_index,
        ProcedureSegment {
            runway_transition,
            ..existing
        },
        procedure_legs,
    )
}

pub fn interpret_path_termination(code: &str) -> PathTermination {
    match code.trim() {
        "IF" => PathTermination::InitialFix,
        "TF" => PathTermination::TrackToFix,
        "CF" => PathTermination::CourseToFix,
        "DF" => PathTermination::DirectToFix,
        "VM" | "FM" => PathTermination::HeadingToManual,
        "VA" | "CA" => PathTermination::HeadingToAltitude,
        other => PathTermination::Other(other.to_string()),
    }
}

fn resolved_legs_from_waypoint_components(components: &[RouteComponent]) -> Vec<ResolvedLeg> {
    let mut legs = Vec::new();

    for (index, pair) in components.windows(2).enumerate() {
        let [left, right] = pair else {
            continue;
        };

        let (
            RouteComponent::Waypoint { waypoint: from },
            RouteComponent::Waypoint { waypoint: to },
        ) = (left, right)
        else {
            continue;
        };

        legs.push(ResolvedLeg {
            id: format!("component-{index}-{}", index + 1),
            from: from.clone(),
            to: to.clone(),
            source: ResolvedLegSource::RouteComponent {
                component_index: index,
            },
            procedure_provenance: None,
        });
    }

    legs
}

fn rebuild_resolved_legs_with_grouped_components(
    components: &[RouteComponent],
    grouped_component_legs: &BTreeMap<usize, Vec<ResolvedLeg>>,
) -> Vec<ResolvedLeg> {
    let mut resolved = Vec::new();
    let per_component_items =
        dedupe_component_items_for_projection(components, grouped_component_legs);
    let flattened = flatten_visible_nav_sequence(&per_component_items);
    let mut previous_waypoint: Option<(usize, NavRef)> = None;
    let mut synthetic_leg_index = 0usize;

    for item in flattened {
        match item {
            FlattenedNavItem::Break => previous_waypoint = None,
            FlattenedNavItem::Waypoint(component_index, nav_ref) => {
                if let Some((from_component_index, from)) = previous_waypoint.as_ref() {
                    if from != &nav_ref {
                        if let Some(grouped_leg) = grouped_leg_for_visible_pair(
                            grouped_component_legs,
                            *from_component_index,
                            component_index,
                            from,
                            &nav_ref,
                        ) {
                            resolved.push(grouped_leg.clone());
                        } else {
                            resolved.push(ResolvedLeg {
                                id: format!(
                                    "component-{synthetic_leg_index}-{}",
                                    synthetic_leg_index + 1
                                ),
                                from: from.clone(),
                                to: nav_ref.clone(),
                                source: ResolvedLegSource::SyntheticBridge {
                                    from_component_index: *from_component_index,
                                    to_component_index: component_index,
                                },
                                procedure_provenance: None,
                            });
                            synthetic_leg_index += 1;
                        }
                    }
                }
                previous_waypoint = Some((component_index, nav_ref));
            }
        }
    }

    resolved
}

fn grouped_leg_for_visible_pair<'a>(
    grouped_component_legs: &'a BTreeMap<usize, Vec<ResolvedLeg>>,
    from_component_index: usize,
    to_component_index: usize,
    from: &NavRef,
    to: &NavRef,
) -> Option<&'a ResolvedLeg> {
    if let Some(legs) = grouped_component_legs.get(&from_component_index) {
        if let Some(leg) = legs.iter().find(|leg| leg.from == *from && leg.to == *to) {
            return Some(leg);
        }
    }
    if to_component_index != from_component_index {
        if let Some(legs) = grouped_component_legs.get(&to_component_index) {
            if let Some(leg) = legs.iter().find(|leg| leg.from == *from && leg.to == *to) {
                return Some(leg);
            }
        }
    }
    None
}

fn grouped_component_legs(plan: &FlightPlan) -> BTreeMap<usize, Vec<ResolvedLeg>> {
    let mut by_component = BTreeMap::<usize, Vec<ResolvedLeg>>::new();

    for leg in &plan.resolved_legs {
        let ResolvedLegSource::RouteComponent { component_index } = leg.source else {
            continue;
        };
        let Some(component) = plan.route_components.get(component_index) else {
            continue;
        };
        if component.is_waypoint() {
            continue;
        }
        by_component
            .entry(component_index)
            .or_default()
            .push(leg.clone());
    }

    by_component
}

fn component_view_kind(component: &RouteComponent) -> RouteComponentViewKind {
    match component {
        RouteComponent::Waypoint { .. } => RouteComponentViewKind::Waypoint,
        RouteComponent::Airway { .. } => RouteComponentViewKind::Airway,
        RouteComponent::Procedure { .. } => RouteComponentViewKind::Procedure,
    }
}

fn component_summary(component: &RouteComponent) -> String {
    match component {
        RouteComponent::Waypoint { waypoint } => nav_ref_label(waypoint),
        RouteComponent::Airway { airway } => airway.name.clone(),
        RouteComponent::Procedure { procedure } => procedure
            .display_label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(procedure.procedure_id.as_str())
            .to_string(),
    }
}

fn raw_component_ui_items(
    component: &RouteComponent,
    grouped_legs: Vec<ResolvedLeg>,
) -> Vec<ConcretizedNavItem> {
    match component {
        RouteComponent::Waypoint { waypoint } => {
            vec![ConcretizedNavItem::Waypoint {
                nav_ref: waypoint.clone(),
            }]
        }
        RouteComponent::Airway { airway } => {
            let mut items = Vec::new();
            let mut push_waypoint = |nav_ref: NavRef| {
                let duplicate = matches!(
                    items.last(),
                    Some(ConcretizedNavItem::Waypoint { nav_ref: existing }) if *existing == nav_ref
                );
                if !duplicate {
                    items.push(ConcretizedNavItem::Waypoint { nav_ref });
                }
            };

            push_waypoint(airway.entry.clone());
            if grouped_legs.is_empty() {
                push_waypoint(airway.exit.clone());
            } else {
                for leg in grouped_legs {
                    push_waypoint(leg.to.clone());
                }
            }
            items
        }
        RouteComponent::Procedure { procedure } => {
            let mut items = Vec::new();
            let mut push_waypoint = |nav_ref: NavRef| {
                let duplicate = matches!(
                    items.last(),
                    Some(ConcretizedNavItem::Waypoint { nav_ref: existing }) if *existing == nav_ref
                );
                if !duplicate {
                    items.push(ConcretizedNavItem::Waypoint { nav_ref });
                }
            };

            if let Some(first) = grouped_legs.first() {
                push_waypoint(first.from.clone());
                for leg in grouped_legs {
                    push_waypoint(leg.to.clone());
                }
            }

            if let Some(discontinuity) = procedure.terminal_discontinuity.clone() {
                items.push(ConcretizedNavItem::Discontinuity {
                    label: discontinuity.display_label().to_string(),
                    discontinuity,
                });
            }

            items
        }
    }
}

fn active_component_index_for_guidance(
    plan: &FlightPlan,
    guidance: &GuidanceState,
) -> Option<usize> {
    match guidance.sequencing_mode {
        SequencingMode::DirectTo => guidance
            .direct_to
            .as_ref()
            .and_then(|direct_to| direct_to_target_row(plan, direct_to))
            .and_then(|row| row.component_index),
        SequencingMode::FollowPlan | SequencingMode::Suspended => {
            match plan.resolved_legs.get(guidance.active_leg_index)?.source {
                ResolvedLegSource::RouteComponent { component_index } => Some(component_index),
                ResolvedLegSource::SyntheticBridge { .. } => None,
            }
        }
    }
}

fn nav_ref_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(code) | NavRef::Navaid(code) | NavRef::Fix(code) => code.clone(),
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => {
            identifier.clone()
        }
        NavRef::LatLon(position) => format!("{:.4},{:.4}", position.lat, position.lon),
        NavRef::Spot(position) => format!("SPOT\n{:.2},{:.2}", position.lat, position.lon),
    }
}

fn rewrite_grouped_legs_source(legs: &[ResolvedLeg], component_index: usize) -> Vec<ResolvedLeg> {
    legs.iter()
        .cloned()
        .map(|mut leg| {
            leg.source = ResolvedLegSource::RouteComponent { component_index };
            leg
        })
        .collect()
}

fn dedupe_component_items_for_projection(
    components: &[RouteComponent],
    grouped_component_legs: &BTreeMap<usize, Vec<ResolvedLeg>>,
) -> Vec<Vec<ConcretizedNavItem>> {
    let mut per_component = components
        .iter()
        .enumerate()
        .map(|(component_index, component)| {
            raw_component_ui_items(
                component,
                grouped_component_legs
                    .get(&component_index)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    for boundary_index in 0..per_component.len().saturating_sub(1) {
        let left_waypoint = per_component[boundary_index]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, item)| match item {
                ConcretizedNavItem::Waypoint { nav_ref } => Some((index, nav_ref.clone())),
                ConcretizedNavItem::Discontinuity { .. } => None,
            });
        let right_waypoint = per_component[boundary_index + 1]
            .iter()
            .enumerate()
            .find_map(|(index, item)| match item {
                ConcretizedNavItem::Waypoint { nav_ref } => Some((index, nav_ref.clone())),
                ConcretizedNavItem::Discontinuity { .. } => None,
            });
        let (Some((left_index, left_nav_ref)), Some((right_index, right_nav_ref))) =
            (left_waypoint, right_waypoint)
        else {
            continue;
        };
        if left_nav_ref != right_nav_ref {
            continue;
        }
        let hide_right = matches!(
            components.get(boundary_index),
            Some(RouteComponent::Waypoint { .. })
        ) && !matches!(
            components.get(boundary_index + 1),
            Some(RouteComponent::Waypoint { .. })
        );
        if hide_right {
            per_component[boundary_index + 1].remove(right_index);
        } else {
            per_component[boundary_index].remove(left_index);
        }
    }

    per_component
}

#[derive(Debug, Clone, PartialEq)]
enum FlattenedNavItem {
    Waypoint(usize, NavRef),
    Break,
}

fn flatten_visible_nav_sequence(
    per_component_items: &[Vec<ConcretizedNavItem>],
) -> Vec<FlattenedNavItem> {
    let mut flattened = Vec::new();
    for (component_index, items) in per_component_items.iter().enumerate() {
        for item in items {
            match item {
                ConcretizedNavItem::Waypoint { nav_ref } => {
                    flattened.push(FlattenedNavItem::Waypoint(component_index, nav_ref.clone()));
                }
                ConcretizedNavItem::Discontinuity { .. } => {
                    flattened.push(FlattenedNavItem::Break);
                }
            }
        }
    }
    flattened
}

fn leg_index_by_id(resolved_legs: &[ResolvedLeg], leg_id: &str) -> Option<usize> {
    resolved_legs.iter().position(|leg| leg.id == leg_id)
}

fn leg_starts_at_route_component(leg: &ResolvedLeg, component_index: usize) -> bool {
    match leg.source {
        ResolvedLegSource::RouteComponent {
            component_index: source_component_index,
        } => source_component_index == component_index,
        ResolvedLegSource::SyntheticBridge {
            from_component_index,
            ..
        } => from_component_index == component_index,
    }
}

fn direct_to_target_row(
    plan: &FlightPlan,
    direct_to: &DirectToState,
) -> Option<FlightPlanDisplayRowUiView> {
    let DirectToTargetRow::Planned { row_id } = &direct_to.target_row else {
        return None;
    };
    project_identity_rows(plan)
        .into_iter()
        .find(|row| row.uid == row_id.as_str())
}

fn direct_to_target_leg_index(plan: &FlightPlan, direct_to: &DirectToState) -> Option<usize> {
    direct_to_target_row(plan, direct_to).and_then(|row| row.leg_index)
}

pub(crate) fn direct_to_resume_leg_index(
    plan: &FlightPlan,
    direct_to: &DirectToState,
) -> Option<usize> {
    let resume_row_id = direct_to.resume_row_id.as_ref()?;
    project_identity_rows(plan)
        .into_iter()
        .find(|row| row.uid == resume_row_id.as_str())
        .and_then(|row| row.leg_index)
}

fn revalidate_guidance_after_plan_edit(
    guidance: Option<GuidanceState>,
    plan: &FlightPlan,
) -> AppResult<Option<GuidanceState>> {
    let resolved_legs = &plan.resolved_legs;
    if resolved_legs.is_empty() {
        return Ok(None);
    }

    let Some(mut guidance) = guidance else {
        return Ok(None);
    };

    if guidance.active_leg_index >= resolved_legs.len() {
        guidance.active_leg_index = resolved_legs.len().saturating_sub(1);
    }

    let mut current_detail_index = 0usize;
    let mut first_detail_for_active_leg = None;
    let mut active_detail_still_valid = false;
    for (leg_index, leg) in resolved_legs.iter().enumerate() {
        let detail_count = leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
            .map(|path| path.elements.len().max(1))
            .unwrap_or(1);
        if leg_index == guidance.active_leg_index {
            first_detail_for_active_leg = Some(current_detail_index);
            if let Some(active_detail_index) = guidance.active_detail_index {
                active_detail_still_valid = active_detail_index >= current_detail_index
                    && active_detail_index < current_detail_index + detail_count;
            }
        }
        current_detail_index += detail_count;
    }
    if !active_detail_still_valid {
        guidance.active_detail_index = first_detail_for_active_leg;
    }

    if guidance
        .display_split_leg_id
        .as_deref()
        .and_then(|leg_id| leg_index_by_id(resolved_legs, leg_id))
        .is_none()
    {
        guidance.display_split_leg_id = None;
    }

    if let Some(direct_to) = guidance.direct_to.as_mut() {
        let target_row = direct_to_target_row(plan, direct_to);
        if let Some(target_leg_index) = target_row.as_ref().and_then(|row| row.leg_index) {
            guidance.active_leg_index = target_leg_index;
        }
        if target_row.is_none() && direct_to.target_row.is_planned() {
            direct_to.target_row = DirectToTargetRow::Temporary {
                row_id: direct_to.target_row.row_id().clone(),
            };
            direct_to.resume_row_id = None;
        } else {
            direct_to.resume_row_id = target_row
                .map(|row| {
                    let resume_leg_index = if let Some(target_leg_index) = row.leg_index {
                        resume_leg_index_after_leg(plan, target_leg_index)
                    } else {
                        row.component_index.and_then(|component_index| {
                            resolved_legs.iter().position(|leg| {
                                leg_starts_at_route_component(leg, component_index)
                                    && leg.from == direct_to.target
                            })
                        })
                    };
                    resume_leg_index
                        .map(|leg_index| planned_row_id_for_leg_index(plan, leg_index))
                        .transpose()
                })
                .transpose()?
                .flatten();
        }
    }

    Ok(Some(guidance))
}

fn should_suspend_after_active_leg(plan: &FlightPlan, active_leg_index: usize) -> bool {
    let Some(active_leg) = plan.resolved_legs.get(active_leg_index) else {
        return false;
    };
    let ResolvedLegSource::RouteComponent { component_index } = active_leg.source else {
        return false;
    };
    let Some(RouteComponent::Procedure { procedure }) = plan.route_components.get(component_index)
    else {
        return false;
    };
    if procedure.terminal_discontinuity.is_none() {
        return false;
    }

    let last_leg_for_component = plan
        .resolved_legs
        .iter()
        .enumerate()
        .filter_map(|(index, leg)| match leg.source {
            ResolvedLegSource::RouteComponent {
                component_index: leg_component_index,
            } if leg_component_index == component_index => Some(index),
            _ => None,
        })
        .max();

    last_leg_for_component == Some(active_leg_index)
}

fn resume_leg_index_after_leg(plan: &FlightPlan, leg_index: usize) -> Option<usize> {
    if should_suspend_after_active_leg(plan, leg_index) {
        return None;
    }

    plan.resolved_legs.get(leg_index + 1).map(|_| leg_index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activate_direct_to_test_leg(
        plan: &FlightPlan,
        from_position: LatLon,
        fixture_leg_id: &str,
    ) -> AppResult<FlightPlan> {
        let plan = plan.clone().normalized();
        let leg_index = plan
            .resolved_legs
            .iter()
            .position(|leg| leg.id == fixture_leg_id)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!("test fixture leg does not exist: {fixture_leg_id}"),
            })?;
        let row_id = planned_row_id_for_leg_index(&plan, leg_index)?;
        activate_direct_to_row(&plan, from_position, &row_id)
    }

    fn activate_direct_to_test_component(
        plan: &FlightPlan,
        from_position: LatLon,
        component_index: usize,
    ) -> AppResult<FlightPlan> {
        let plan = plan.clone().normalized();
        let row = project_identity_rows(&plan)
            .into_iter()
            .find(|row| {
                row.row_kind == FlightPlanDisplayRowKind::Waypoint
                    && row.depth == 0
                    && row.component_index == Some(component_index)
            })
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "test component does not project to a top-level waypoint row: {component_index}"
                ),
            })?;
        activate_direct_to_row(&plan, from_position, &FlightPlanRowId(row.uid))
    }

    #[test]
    fn empty_flight_plan_is_valid_startup_state() {
        let plan = crate::build_flight_plan(FlightPlan::empty()).expect("empty startup plan");
        assert!(plan.route_components.is_empty());
        assert!(plan.resolved_legs.is_empty());
        assert!(project_ui_state(&plan).display_rows.is_empty());
    }

    #[test]
    fn spot_flight_plan_label_uses_two_lines_and_coarse_coordinates() {
        assert_eq!(
            nav_ref_label(&NavRef::Spot(LatLon {
                lat: 47.626,
                lon: -122.194,
            })),
            "SPOT\n47.63,-122.19"
        );
    }

    fn sample_airway_component_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "Airway".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V16".to_string(),
                        branch_key: Some("V16-A".to_string()),
                        entry: NavRef::Fix("DODGR".to_string()),
                        exit: NavRef::Fix("PDZ".to_string()),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "airway-0".to_string(),
                    from: NavRef::Fix("DODGR".to_string()),
                    to: NavRef::Fix("LAHAB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-1".to_string(),
                    from: NavRef::Fix("LAHAB".to_string()),
                    to: NavRef::Fix("PDZ".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_seeded_reorder_plan() -> FlightPlan {
        let route_components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            },
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "V23".to_string(),
                    branch_key: Some("V23-A".to_string()),
                    entry: NavRef::Navaid("SEA".to_string()),
                    exit: NavRef::Fix("RAWER".to_string()),
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KUAO".to_string()),
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRDD".to_string()),
            },
        ];

        let grouped_legs = BTreeMap::from([(
            1usize,
            vec![
                ResolvedLeg {
                    id: "v23-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v23-1".to_string(),
                    from: NavRef::Fix("BTG".to_string()),
                    to: NavRef::Fix("VAMPS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v23-2".to_string(),
                    from: NavRef::Fix("VAMPS".to_string()),
                    to: NavRef::Fix("RAWER".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
        )]);
        let resolved_legs =
            rebuild_resolved_legs_with_grouped_components(&route_components, &grouped_legs);

        FlightPlan {
            id: "plan-seeded".to_string(),
            name: "Seeded".to_string(),
            route_components,
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs,
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRDD".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn interpret_path_termination_maps_known_codes() {
        assert_eq!(
            interpret_path_termination("IF"),
            PathTermination::InitialFix
        );
        assert_eq!(
            interpret_path_termination("TF"),
            PathTermination::TrackToFix
        );
        assert_eq!(
            interpret_path_termination("DF"),
            PathTermination::DirectToFix
        );
    }

    #[test]
    fn delete_waypoint_component_rejects_grouped_components() {
        let plan = sample_airway_component_plan();
        let err = delete_waypoint_component(&plan, 1).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn flatten_component_replaces_grouped_component_with_explicit_waypoints() {
        let plan = sample_airway_component_plan();
        let flattened = flatten_component_to_waypoints(
            &plan,
            1,
            vec![
                NavRef::Fix("DODGR".to_string()),
                NavRef::Fix("LAHAB".to_string()),
                NavRef::Fix("PDZ".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(flattened.route_components.len(), 5);
        assert!(matches!(
            flattened.route_components[1],
            RouteComponent::Waypoint { .. }
        ));
        assert_eq!(flattened.resolved_legs.len(), 4);
    }

    fn sample_waypoint_only_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-2".to_string(),
            name: "Waypoint only".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KHIO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Airport("KUAO".to_string()),
                    to: NavRef::Airport("KHIO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KHIO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_guided_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-3".to_string(),
            name: "Guided".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("OLM".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("OLM".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Fix("OLM".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn modda_zgood_normy_waypoint_plan() -> FlightPlan {
        crate::build_flight_plan(FlightPlan {
            id: "plan-modda".to_string(),
            name: "MODDA ZGOOD NORMY".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("MODDA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ZGOOD".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("NORMY".to_string()),
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..FlightPlan::empty()
        })
        .expect("build MODDA ZGOOD NORMY plan")
    }

    fn sample_duplicate_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-dup".to_string(),
            name: "Duplicate waypoint".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("IAF".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("PTURN".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("IAF".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KAAA".to_string()),
                    to: NavRef::Fix("IAF".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Fix("IAF".to_string()),
                    to: NavRef::Fix("PTURN".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Fix("PTURN".to_string()),
                    to: NavRef::Fix("IAF".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-3-4".to_string(),
                    from: NavRef::Fix("IAF".to_string()),
                    to: NavRef::Airport("KBBB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 3 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn procedure_without_terminal_discontinuity_bridges_to_following_waypoint() {
        let components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KAAA".to_string()),
            },
            RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "STAR1".to_string(),
                    display_label: None,
                    kind: ProcedureKind::Star,
                    runway_transition: Some("RW10".to_string()),
                    enroute_transition: Some("FOO".to_string()),
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KBBB".to_string()),
            },
        ];
        let grouped = BTreeMap::from([(
            1usize,
            vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("FOO".to_string()),
                to: NavRef::Fix("BAR".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: None,
            }],
        )]);

        let resolved = rebuild_resolved_legs_with_grouped_components(&components, &grouped);

        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].from, NavRef::Airport("KAAA".to_string()));
        assert_eq!(resolved[0].to, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[2].from, NavRef::Fix("BAR".to_string()));
        assert_eq!(resolved[2].to, NavRef::Airport("KBBB".to_string()));
    }

    #[test]
    fn procedure_with_terminal_discontinuity_does_not_bridge_to_following_waypoint() {
        let components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KAAA".to_string()),
            },
            RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "STAR1".to_string(),
                    display_label: None,
                    kind: ProcedureKind::Star,
                    runway_transition: Some("RW10".to_string()),
                    enroute_transition: Some("FOO".to_string()),
                    terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                    data_quality: Vec::new(),
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KBBB".to_string()),
            },
        ];
        let grouped = BTreeMap::from([(
            1usize,
            vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("FOO".to_string()),
                to: NavRef::Fix("BAR".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: None,
            }],
        )]);

        let resolved = rebuild_resolved_legs_with_grouped_components(&components, &grouped);

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].from, NavRef::Airport("KAAA".to_string()));
        assert_eq!(resolved[0].to, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[1].from, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[1].to, NavRef::Fix("BAR".to_string()));
    }

    fn sample_inserted_airway() -> (AirwaySegment, Vec<ResolvedLeg>) {
        (
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Navaid("SEA".to_string()),
                exit: NavRef::Fix("VAMPS".to_string()),
            },
            vec![
                ResolvedLeg {
                    id: "airway-V2-A-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("SUMMA".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-A-1".to_string(),
                    from: NavRef::Fix("SUMMA".to_string()),
                    to: NavRef::Fix("VAMPS".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_task_6_route_plan() -> FlightPlan {
        let route_components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            },
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "V2".to_string(),
                    branch_key: Some("V2".to_string()),
                    entry: NavRef::Navaid("SEA".to_string()),
                    exit: NavRef::Navaid("ELN".to_string()),
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KYKM".to_string()),
            },
        ];
        let grouped_legs = BTreeMap::from([(
            1,
            vec![
                ResolvedLeg {
                    id: "airway-V2-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("VAMPS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-1".to_string(),
                    from: NavRef::Fix("VAMPS".to_string()),
                    to: NavRef::Fix("BANDR".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-2".to_string(),
                    from: NavRef::Fix("BANDR".to_string()),
                    to: NavRef::Navaid("ELN".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
        )]);
        let resolved_legs =
            rebuild_resolved_legs_with_grouped_components(&route_components, &grouped_legs);

        FlightPlan {
            id: "task-6".to_string(),
            name: "KRNT SEA V2 ELN KYKM".to_string(),
            route_components,
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs,
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KYKM".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
        .normalized()
    }

    fn sample_inserted_procedure() -> (ProcedureSegment, Vec<ResolvedLeg>) {
        (
            ProcedureSegment {
                airport_id: AirportId("CYQG".to_string()),
                procedure_id: "AUTTO1".to_string(),
                display_label: None,
                kind: ProcedureKind::Sid,
                runway_transition: None,
                enroute_transition: Some("COLTS".to_string()),
                terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                data_quality: Vec::new(),
            },
            vec![
                ResolvedLeg {
                    id: "proc-autto1-0".to_string(),
                    from: NavRef::Fix("COLTS".to_string()),
                    to: NavRef::Fix("BOREK".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-1".to_string(),
                    from: NavRef::Fix("BOREK".to_string()),
                    to: NavRef::Fix("AXXIS".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-2".to_string(),
                    from: NavRef::Fix("AXXIS".to_string()),
                    to: NavRef::Fix("GIGGY".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_replaced_procedure() -> (ProcedureSegment, Vec<ResolvedLeg>) {
        (
            ProcedureSegment {
                airport_id: AirportId("CYQG".to_string()),
                procedure_id: "AUTTO1".to_string(),
                display_label: None,
                kind: ProcedureKind::Sid,
                runway_transition: None,
                enroute_transition: Some("PICUP".to_string()),
                terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                data_quality: Vec::new(),
            },
            vec![
                ResolvedLeg {
                    id: "proc-autto1-p0".to_string(),
                    from: NavRef::Fix("PICUP".to_string()),
                    to: NavRef::Fix("AXXIS".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-p1".to_string(),
                    from: NavRef::Fix("AXXIS".to_string()),
                    to: NavRef::Fix("GIGGY".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_retargeted_airway() -> (AirwaySegment, Vec<ResolvedLeg>) {
        (
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Fix("OLM".to_string()),
                exit: NavRef::Fix("BTG".to_string()),
            },
            vec![
                ResolvedLeg {
                    id: "airway-V2-A-r0".to_string(),
                    from: NavRef::Fix("OLM".to_string()),
                    to: NavRef::Fix("SUMMA".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-A-r1".to_string(),
                    from: NavRef::Fix("SUMMA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent {
                        component_index: 99,
                    },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_v165_plan_with_explicit_endpoints() -> FlightPlan {
        let airway = AirwaySegment {
            name: "V165".to_string(),
            branch_key: Some("V165-0".to_string()),
            entry: NavRef::Navaid("OLM".to_string()),
            exit: NavRef::Fix("RAWER".to_string()),
        };
        let points = vec![
            NavRef::Navaid("OLM".to_string()),
            NavRef::Fix("CETRA".to_string()),
            NavRef::Fix("HOKBO".to_string()),
            NavRef::Fix("UBG".to_string()),
            NavRef::Fix("RAWER".to_string()),
        ];
        let airway_legs = points
            .windows(2)
            .enumerate()
            .map(|(index, pair)| ResolvedLeg {
                id: format!("airway-v165-{index}"),
                from: pair[0].clone(),
                to: pair[1].clone(),
                source: ResolvedLegSource::RouteComponent { component_index: 2 },
                procedure_provenance: None,
            })
            .collect::<Vec<_>>();

        FlightPlan {
            id: "plan-v165".to_string(),
            name: "V165 explicit endpoints".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KOLM".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("OLM".to_string()),
                },
                RouteComponent::Airway { airway },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("RAWER".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KOLM".to_string()),
                    to: NavRef::Navaid("OLM".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                airway_legs[0].clone(),
                airway_legs[1].clone(),
                airway_legs[2].clone(),
                airway_legs[3].clone(),
                ResolvedLeg {
                    id: "component-3-4".to_string(),
                    from: NavRef::Fix("RAWER".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 3 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
        .normalized()
    }

    fn sample_two_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-2pt".to_string(),
            name: "Two waypoint".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![ResolvedLeg {
                id: "component-0-1".to_string(),
                from: NavRef::Airport("KRNT".to_string()),
                to: NavRef::Airport("KUAO".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: None,
            }],
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_four_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-4pt".to_string(),
            name: "Four waypoint".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBFI".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPDX".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Airport("KBFI".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Airport("KBFI".to_string()),
                    to: NavRef::Airport("KPAE".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Airport("KPAE".to_string()),
                    to: NavRef::Airport("KPDX".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPDX".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_single_component_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1pt".to_string(),
            name: "Single waypoint".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn insert_airway_between_waypoints_keeps_airway_atomic_and_bridges_resolved_route() {
        let plan = sample_waypoint_only_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(
            inserted.route_components[1],
            RouteComponent::Airway { .. }
        ));
        assert_eq!(inserted.resolved_legs.len(), 5);
        assert_eq!(
            inserted.resolved_legs[0].from,
            NavRef::Airport("KRNT".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[0].to,
            NavRef::Navaid("SEA".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[3].from,
            NavRef::Fix("VAMPS".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[3].to,
            NavRef::Airport("KUAO".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[4].from,
            NavRef::Airport("KUAO".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[4].to,
            NavRef::Airport("KHIO".to_string())
        );
    }

    #[test]
    fn insert_airway_between_waypoints_does_not_duplicate_matching_boundaries() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components[1] = RouteComponent::Waypoint {
            waypoint: NavRef::Fix("VAMPS".to_string()),
        };
        plan.resolved_legs[0].to = NavRef::Fix("VAMPS".to_string());
        let airway = AirwaySegment {
            name: "V2".to_string(),
            branch_key: Some("V2-A".to_string()),
            entry: NavRef::Airport("KRNT".to_string()),
            exit: NavRef::Fix("VAMPS".to_string()),
        };
        let airway_legs = vec![ResolvedLeg {
            id: "airway-V2-A-0".to_string(),
            from: NavRef::Airport("KRNT".to_string()),
            to: NavRef::Fix("VAMPS".to_string()),
            source: ResolvedLegSource::RouteComponent {
                component_index: 88,
            },
            procedure_provenance: None,
        }];

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 2);
        assert!(matches!(
            inserted.route_components[0],
            RouteComponent::Airway { .. }
        ));
        assert!(matches!(
            &inserted.route_components[1],
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport(id)
            } if id == "KHIO"
        ));
    }

    #[test]
    fn insert_airway_between_waypoints_rejects_grouped_components_inside_replaced_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.insert(
            1,
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "J1".to_string(),
                    branch_key: Some("J1-A".to_string()),
                    entry: NavRef::Fix("A".to_string()),
                    exit: NavRef::Fix("B".to_string()),
                },
            },
        );
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 0, 2, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn insert_airway_between_waypoints_rejects_non_waypoint_boundaries() {
        let plan = sample_airway_component_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn insert_airway_between_waypoints_preserves_grouped_components_outside_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.push(RouteComponent::Airway {
            airway: AirwaySegment {
                name: "J1".to_string(),
                branch_key: Some("J1-A".to_string()),
                entry: NavRef::Fix("FOO".to_string()),
                exit: NavRef::Fix("BAR".to_string()),
            },
        });
        plan.route_components.push(RouteComponent::Waypoint {
            waypoint: NavRef::Airport("KPDX".to_string()),
        });
        plan.resolved_legs.push(ResolvedLeg {
            id: "j1-0".to_string(),
            from: NavRef::Fix("FOO".to_string()),
            to: NavRef::Fix("BAR".to_string()),
            source: ResolvedLegSource::RouteComponent { component_index: 3 },
            procedure_provenance: None,
        });
        let (airway, airway_legs) = sample_inserted_airway();

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert!(inserted.route_components.iter().any(|component| matches!(
            component,
            RouteComponent::Airway { airway } if airway.name == "J1"
        )));
        assert!(inserted.resolved_legs.iter().any(|leg| leg.id == "j1-0"));
    }

    #[test]
    fn insert_airway_between_waypoints_requires_increasing_span() {
        let plan = sample_waypoint_only_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 1, 1, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_airway_component_keeps_atomic_slot_and_updates_bridges() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let (airway, airway_legs) = sample_retargeted_airway();

        let replaced = replace_airway_component(&plan, 1, airway, airway_legs).unwrap();

        assert_eq!(replaced.route_components.len(), 4);
        assert!(matches!(
            replaced.route_components[1],
            RouteComponent::Airway { .. }
        ));
        assert_eq!(
            replaced.resolved_legs[0].from,
            NavRef::Airport("KRNT".to_string())
        );
        assert_eq!(replaced.resolved_legs[0].to, NavRef::Fix("OLM".to_string()));
        assert_eq!(
            replaced.resolved_legs[3].from,
            NavRef::Fix("BTG".to_string())
        );
        assert_eq!(
            replaced.resolved_legs[3].to,
            NavRef::Airport("KUAO".to_string())
        );
        assert!(replaced.guidance.is_none());
    }

    #[test]
    fn change_airway_entry_preserves_airway_identity_and_updates_entry_bridge() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let (_, retargeted_legs) = sample_retargeted_airway();

        let changed =
            change_airway_entry(&plan, 1, NavRef::Fix("OLM".to_string()), retargeted_legs).unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "V2");
        assert_eq!(airway.branch_key.as_deref(), Some("V2-A"));
        assert_eq!(airway.entry, NavRef::Fix("OLM".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("VAMPS".to_string()));
        assert_eq!(changed.resolved_legs[0].to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn change_airway_exit_preserves_airway_identity_and_updates_exit_bridge() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let airway_legs = vec![
            ResolvedLeg {
                id: "airway-V2-A-e0".to_string(),
                from: NavRef::Navaid("SEA".to_string()),
                to: NavRef::Fix("SUMMA".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "airway-V2-A-e1".to_string(),
                from: NavRef::Fix("SUMMA".to_string()),
                to: NavRef::Fix("BTG".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
        ];

        let changed =
            change_airway_exit(&plan, 1, NavRef::Fix("BTG".to_string()), airway_legs).unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "V2");
        assert_eq!(airway.branch_key.as_deref(), Some("V2-A"));
        assert_eq!(airway.entry, NavRef::Navaid("SEA".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("BTG".to_string()));
        assert_eq!(
            changed.resolved_legs[3].from,
            NavRef::Fix("BTG".to_string())
        );
        assert_eq!(
            changed.resolved_legs[3].to,
            NavRef::Airport("KUAO".to_string())
        );
    }

    #[test]
    fn replace_airway_component_can_swap_airway_identity() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let replacement = AirwaySegment {
            name: "J1".to_string(),
            branch_key: Some("J1-A".to_string()),
            entry: NavRef::Fix("SEA".to_string()),
            exit: NavRef::Fix("BTG".to_string()),
        };
        let replacement_legs = vec![ResolvedLeg {
            id: "j1-r0".to_string(),
            from: NavRef::Fix("SEA".to_string()),
            to: NavRef::Fix("BTG".to_string()),
            source: ResolvedLegSource::RouteComponent {
                component_index: 99,
            },
            procedure_provenance: None,
        }];

        let replaced = replace_airway_component(&plan, 1, replacement, replacement_legs).unwrap();

        let RouteComponent::Airway { airway } = &replaced.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "J1");
        assert_eq!(airway.branch_key.as_deref(), Some("J1-A"));
    }

    #[test]
    fn replace_airway_component_rejects_non_airway_slot() {
        let (airway, airway_legs) = sample_inserted_airway();
        let err = replace_airway_component(&sample_waypoint_only_plan(), 0, airway, airway_legs)
            .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_airway_component_rejects_empty_leg_set() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let err = replace_airway_component(
            &plan,
            1,
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Fix("OLM".to_string()),
                exit: NavRef::Fix("BTG".to_string()),
            },
            Vec::new(),
        )
        .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn insert_procedure_between_waypoints_keeps_procedure_atomic_and_blocks_outbound_bridge_on_gap()
    {
        let plan = sample_waypoint_only_plan();
        let (procedure, procedure_legs) = sample_inserted_procedure();

        let inserted =
            insert_procedure_between_waypoints(&plan, 0, 1, procedure, procedure_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(
            inserted.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(inserted.resolved_legs.len(), 5);
        assert_eq!(
            inserted.resolved_legs[0].from,
            NavRef::Airport("KRNT".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[0].to,
            NavRef::Fix("COLTS".to_string())
        );
        assert_eq!(
            inserted.resolved_legs[3].to,
            NavRef::Fix("GIGGY".to_string())
        );
        assert!(inserted
            .resolved_legs
            .iter()
            .all(|leg| leg.to != NavRef::Airport("KUAO".to_string())));
    }

    #[test]
    fn insert_procedure_between_waypoints_preserves_active_guidance_before_insertion() {
        let mut plan = sample_four_waypoint_plan();
        plan.guidance = Some(GuidanceState {
            active_leg_index: 0,
            active_detail_index: Some(0),
            display_split_leg_id: None,
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        });
        let (procedure, procedure_legs) = sample_inserted_procedure();

        let inserted =
            insert_procedure_between_waypoints(&plan, 1, 2, procedure, procedure_legs).unwrap();

        let guidance = inserted
            .guidance
            .as_ref()
            .expect("procedure insertion should not drop active guidance");
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert_eq!(
            inserted.resolved_legs[guidance.active_leg_index].to,
            NavRef::Airport("KBFI".to_string())
        );
    }

    #[test]
    fn insert_procedure_between_waypoints_rejects_grouped_components_inside_replaced_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.insert(
            1,
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "J1".to_string(),
                    branch_key: Some("J1-A".to_string()),
                    entry: NavRef::Fix("A".to_string()),
                    exit: NavRef::Fix("B".to_string()),
                },
            },
        );
        let (procedure, procedure_legs) = sample_inserted_procedure();

        let err =
            insert_procedure_between_waypoints(&plan, 0, 2, procedure, procedure_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_procedure_component_updates_entry_bridge_and_keeps_atomic_slot() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let (replacement, replacement_legs) = sample_replaced_procedure();

        let replaced =
            replace_procedure_component(&inserted, 1, replacement, replacement_legs).unwrap();

        assert!(matches!(
            replaced.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(
            replaced.resolved_legs[0].to,
            NavRef::Fix("PICUP".to_string())
        );
        assert_eq!(
            replaced.resolved_legs[2].to,
            NavRef::Fix("GIGGY".to_string())
        );
        assert!(replaced
            .resolved_legs
            .iter()
            .all(|leg| leg.to != NavRef::Airport("KUAO".to_string())));
        assert!(replaced.guidance.is_none());
    }

    #[test]
    fn change_procedure_enroute_transition_preserves_identity_and_updates_spec() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let (_, replacement_legs) = sample_replaced_procedure();

        let changed = change_procedure_enroute_transition(
            &inserted,
            1,
            Some("PICUP".to_string()),
            replacement_legs,
        )
        .unwrap();

        let RouteComponent::Procedure { procedure } = &changed.route_components[1] else {
            panic!("expected procedure");
        };
        assert_eq!(procedure.procedure_id, "AUTTO1");
        assert_eq!(procedure.enroute_transition.as_deref(), Some("PICUP"));
        assert_eq!(
            changed.resolved_legs[0].to,
            NavRef::Fix("PICUP".to_string())
        );
    }

    #[test]
    fn change_procedure_runway_transition_preserves_identity_and_updates_spec() {
        let plan = FlightPlan {
            id: "plan-star".to_string(),
            name: "STAR".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ARD".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("47N".to_string()),
                        procedure_id: "CENTR1".to_string(),
                        display_label: None,
                        kind: ProcedureKind::Star,
                        runway_transition: Some("RW07".to_string()),
                        enroute_transition: None,
                        terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("47N".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "star-0".to_string(),
                    from: NavRef::Fix("ARD".to_string()),
                    to: NavRef::Fix("DYLIN".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "star-1".to_string(),
                    from: NavRef::Fix("DYLIN".to_string()),
                    to: NavRef::Fix("METRO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: None,
            destination: Some(AirportId("47N".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let replacement_legs = vec![
            ResolvedLeg {
                id: "star-rw25-0".to_string(),
                from: NavRef::Fix("ARD".to_string()),
                to: NavRef::Fix("DYLIN".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "star-rw25-1".to_string(),
                from: NavRef::Fix("DYLIN".to_string()),
                to: NavRef::Fix("METRO".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
        ];

        let changed = change_procedure_runway_transition(
            &plan,
            1,
            Some("RW25".to_string()),
            replacement_legs,
        )
        .unwrap();

        let RouteComponent::Procedure { procedure } = &changed.route_components[1] else {
            panic!("expected procedure");
        };
        assert_eq!(procedure.procedure_id, "CENTR1");
        assert_eq!(procedure.runway_transition.as_deref(), Some("RW25"));
    }

    #[test]
    fn replace_procedure_component_rejects_non_procedure_slot() {
        let (procedure, procedure_legs) = sample_inserted_procedure();
        let err =
            replace_procedure_component(&sample_waypoint_only_plan(), 0, procedure, procedure_legs)
                .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_direct_to_stashes_hidden_start_and_resume_leg() {
        let plan = sample_guided_waypoint_plan();

        let activated = activate_direct_to(
            &plan,
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(guidance.active_leg_index, 1);
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(
            direct_to.start,
            NavRef::LatLon(LatLon {
                lat: 47.490,
                lon: -122.216
            })
        );
        assert_eq!(direct_to.target, NavRef::Fix("OLM".to_string()));
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(1));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(2));

        let active_leg = active_guidance_leg(&activated).unwrap();
        assert_eq!(
            active_leg.from,
            NavRef::LatLon(LatLon {
                lat: 47.490,
                lon: -122.216
            })
        );
        assert_eq!(active_leg.to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn sequence_direct_to_resumes_underlying_plan_at_next_leg() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let sequenced = sequence_active_leg(&activated).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 2);
        assert!(guidance.direct_to.is_none());

        let active_leg = active_guidance_leg(&sequenced).unwrap();
        assert_eq!(active_leg.from, NavRef::Fix("OLM".to_string()));
        assert_eq!(active_leg.to, NavRef::Airport("KUAO".to_string()));
    }

    #[test]
    fn direct_to_canonical_navaid_resumes_underlying_plan() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("SPUUD".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("PDT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("CORDO".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "v4-spuud-pdt".to_string(),
                    from: NavRef::Fix("SPUUD".to_string()),
                    to: NavRef::Navaid("PDT".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v4-pdt-cordo".to_string(),
                    from: NavRef::Navaid("PDT".to_string()),
                    to: NavRef::Fix("CORDO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..FlightPlan::default()
        };

        let activated = activate_direct_to(
            &plan,
            LatLon {
                lat: 40.0,
                lon: -120.18,
            },
            NavRef::Navaid("PDT".to_string()),
        )
        .expect("direct-to PDT navaid");

        let direct_to = activated
            .guidance
            .as_ref()
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(0));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(1));

        let sequenced = sequence_active_leg(&activated).expect("sequence direct-to");
        let guidance = sequenced.guidance.as_ref().expect("sequenced guidance");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 1);
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn activate_leg_clears_direct_to_and_retargets_underlying_plan() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let resumed = activate_leg(&activated, 2).unwrap();

        let guidance = resumed.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 2);
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn off_plan_direct_to_suspends_after_hidden_leg_completes() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("BTG".to_string()),
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert!(!guidance.direct_to.as_ref().unwrap().target_row.is_planned());
        assert_eq!(guidance.direct_to.as_ref().unwrap().resume_row_id, None);

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
        assert!(active_guidance_leg(&sequenced).is_none());
    }

    #[test]
    fn follow_plan_sequencing_advances_until_route_end_then_suspends() {
        let once = sequence_active_leg(&sample_guided_waypoint_plan()).unwrap();
        assert_eq!(once.guidance.as_ref().unwrap().active_leg_index, 1);
        assert_eq!(
            once.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::FollowPlan
        );

        let twice = sequence_active_leg(&once).unwrap();
        assert_eq!(twice.guidance.as_ref().unwrap().active_leg_index, 2);
        assert_eq!(
            twice.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::FollowPlan
        );

        let done = sequence_active_leg(&twice).unwrap();
        assert_eq!(done.guidance.as_ref().unwrap().active_leg_index, 2);
        assert_eq!(
            done.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::Suspended
        );
    }

    #[test]
    fn sequencing_suspends_at_terminal_discontinuous_procedure_boundary() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let plan = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                active_detail_index: Some(3),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..inserted
        };

        let sequenced = sequence_active_leg(&plan).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 3);
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
    }

    #[test]
    fn procedure_waypoint_on_hold_leg_remains_activatable_while_holding() {
        let leg = ResolvedLeg {
            id: "khvr-hold-entry-to-etoho".to_string(),
            from: NavRef::Fix("CIBMI".to_string()),
            to: NavRef::Fix("ETOHO".to_string()),
            source: ResolvedLegSource::RouteComponent { component_index: 0 },
            procedure_provenance: Some(ProcedureLegProvenance {
                airport_id: "KHVR".to_string(),
                procedure_id: "R26".to_string(),
                kind: ProcedureKind::Approach,
                role: ProcedureSegmentRole::Common,
                path_termination: PathTermination::TrackToFix,
                leg_sequence: 10,
                display_path: Some(LegDisplayPath {
                    style: LegDisplayPathStyle::Solid,
                    elements: vec![
                        LegDisplayElement::Segment {
                            start: LatLon {
                                lat: 48.0,
                                lon: -109.0,
                            },
                            end: LatLon {
                                lat: 48.1,
                                lon: -109.0,
                            },
                        },
                        LegDisplayElement::Segment {
                            start: LatLon {
                                lat: 48.1,
                                lon: -109.0,
                            },
                            end: LatLon {
                                lat: 48.1,
                                lon: -108.9,
                            },
                        },
                        LegDisplayElement::Segment {
                            start: LatLon {
                                lat: 48.1,
                                lon: -108.9,
                            },
                            end: LatLon {
                                lat: 48.0,
                                lon: -108.9,
                            },
                        },
                        LegDisplayElement::Segment {
                            start: LatLon {
                                lat: 48.0,
                                lon: -108.9,
                            },
                            end: LatLon {
                                lat: 48.0,
                                lon: -109.0,
                            },
                        },
                        LegDisplayElement::Segment {
                            start: LatLon {
                                lat: 48.0,
                                lon: -109.0,
                            },
                            end: LatLon {
                                lat: 48.1,
                                lon: -109.0,
                            },
                        },
                    ],
                    effective_terminal_course_deg: None,
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            }),
        };
        let plan = FlightPlan {
            id: "khvr-hold-active".to_string(),
            name: "KHVR hold active".to_string(),
            route_components: vec![RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KHVR".to_string()),
                    procedure_id: "R26".to_string(),
                    display_label: None,
                    kind: ProcedureKind::Approach,
                    runway_transition: None,
                    enroute_transition: Some("ISITE".to_string()),
                    terminal_discontinuity: Some(ProcedureDiscontinuity::Hold),
                    data_quality: Vec::new(),
                },
            }],
            route_component_uids: vec!["row-proc".to_string()],
            route_component_uid_counter: 1,
            resolved_legs: vec![leg],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(1),
                display_split_leg_id: Some("khvr-hold-entry-to-etoho".to_string()),
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            departure: None,
            destination: Some(AirportId("KHVR".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let ui = project_ui_state(&plan);
        let etoho_row = ui
            .display_rows
            .iter()
            .find(|row| row.depth == 1 && row.label == "ETOHO")
            .expect("ETOHO procedure row");
        assert!(
            flight_plan_row_actions(etoho_row).any(|action| {
                action.id == FlightPlanRowActionId::ActivateLeg && action.enabled
            }),
            "ETOHO should be activatable while the same coarse leg is suspended in its terminal hold"
        );
        let hold_row = ui
            .display_rows
            .iter()
            .find(|row| row.depth == 1 && row.label == "HOLD")
            .expect("HOLD row");
        assert!(
            flight_plan_row_actions(hold_row).any(|action| {
                action.id == FlightPlanRowActionId::ActivateLeg && !action.enabled
            }),
            "already-active hold should keep its Activate Leg disabled"
        );
    }

    #[test]
    fn sequencing_continues_through_non_discontinuous_grouped_procedure() {
        let plan = FlightPlan {
            id: "plan-proc-seq".to_string(),
            name: "Procedure sequencing".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KAAA".to_string()),
                        procedure_id: "STAR1".to_string(),
                        display_label: None,
                        kind: ProcedureKind::Star,
                        runway_transition: Some("RW10".to_string()),
                        enroute_transition: Some("FOO".to_string()),
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KAAA".to_string()),
                    to: NavRef::Fix("FOO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-0".to_string(),
                    from: NavRef::Fix("FOO".to_string()),
                    to: NavRef::Fix("BAR".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Fix("BAR".to_string()),
                    to: NavRef::Airport("KBBB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 1,
                active_detail_index: Some(1),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let sequenced = sequence_active_leg(&plan).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 2);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn suspend_sequencing_forces_suspended_mode_and_clears_direct_to() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.49,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let suspended = suspend_sequencing(&activated).unwrap();

        let guidance = suspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
        let active_leg = active_guidance_leg(&suspended).unwrap();
        assert_eq!(active_leg.from, NavRef::Navaid("SEA".to_string()));
        assert_eq!(active_leg.to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn unsuspend_sequencing_restores_follow_plan_when_not_at_boundary() {
        let suspended = suspend_sequencing(&sample_guided_waypoint_plan()).unwrap();

        let unsuspended = unsuspend_sequencing(&suspended).unwrap();

        let guidance = unsuspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn unsuspend_sequencing_at_terminal_discontinuous_boundary_activates_next_leg() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let suspended = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                active_detail_index: Some(3),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            ..inserted
        };

        let unsuspended = unsuspend_sequencing(&suspended).unwrap();

        let guidance = unsuspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 4);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn activate_next_leg_crosses_suspend_boundary_explicitly() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let suspended = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                active_detail_index: Some(3),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            ..inserted
        };

        let next = activate_next_leg(&suspended).unwrap();

        let guidance = next.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 4);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn unsuspend_sequencing_rejects_non_suspended_mode() {
        let err = unsuspend_sequencing(&sample_guided_waypoint_plan()).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_next_leg_rejects_end_of_route() {
        let plan = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 2,
                active_detail_index: Some(2),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..sample_guided_waypoint_plan()
        };

        let err = activate_next_leg(&plan).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn stop_navigation_clears_guidance_without_mutating_route() {
        let plan = sample_guided_waypoint_plan();

        let stopped = stop_navigation(&plan).unwrap();

        assert_eq!(stopped.guidance, None);
        assert_eq!(stopped.route_components, plan.route_components);
        assert_eq!(stopped.resolved_legs, plan.resolved_legs);
    }

    #[test]
    fn stop_navigation_rejects_plan_without_guidance() {
        let err = stop_navigation(&sample_waypoint_only_plan()).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_direct_to_row_can_target_specific_duplicate_waypoint_occurrence() {
        let activated = activate_direct_to_test_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 2);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(2));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(3));
    }

    #[test]
    fn plan_edit_that_removes_targeted_occurrence_degrades_direct_to_to_off_plan() {
        let activated = activate_direct_to_test_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let edited = delete_waypoint_component(&activated, 3).unwrap();

        let guidance = edited.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert!(!direct_to.target_row.is_planned());
        assert!(direct_to.resume_row_id.is_none());
    }

    #[test]
    fn project_ui_state_expands_atomic_airway_component_without_exposing_bridge_legs() {
        let inserted = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();

        let ui = project_ui_state(&inserted);

        assert_eq!(ui.components.len(), 4);
        assert_eq!(ui.components[1].kind, RouteComponentViewKind::Airway);
        assert_eq!(ui.components[1].summary, "V2");
        assert_eq!(
            ui.components[1].items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Navaid("SEA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("SUMMA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("VAMPS".to_string())
                },
            ]
        );
        assert_eq!(
            ui.resolved_legs[0].from,
            NavRef::Airport("KRNT".to_string())
        );
        assert_eq!(ui.resolved_legs[0].to, NavRef::Navaid("SEA".to_string()));
    }

    #[test]
    fn project_ui_state_uses_procedure_display_label_for_group_summary() {
        let procedure = ProcedureSegment {
            airport_id: AirportId("KSEA".to_string()),
            procedure_id: "I34R".to_string(),
            display_label: Some("ILS or LOC 34R".to_string()),
            kind: ProcedureKind::Approach,
            runway_transition: None,
            enroute_transition: Some("JIPOX".to_string()),
            terminal_discontinuity: None,
            data_quality: Vec::new(),
        };
        let plan = FlightPlan {
            route_components: vec![RouteComponent::Procedure { procedure }],
            ..FlightPlan::default()
        }
        .normalized();

        let ui = project_ui_state(&plan);

        assert_eq!(ui.components.len(), 1);
        assert_eq!(ui.components[0].summary, "ILS or LOC 34R");
        assert_eq!(ui.display_rows[0].label, "ILS or LOC 34R");
    }

    #[test]
    fn project_ui_state_exposes_airway_editability_and_span_occupancy_flags() {
        let inserted = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();

        let ui = project_ui_state(&inserted);

        assert!(!ui.components[0].can_add_airway_after);
        assert_eq!(ui.components[0].following_waypoint, None);

        assert!(ui.components[2].can_add_airway_after);

        assert!(ui.components[1].can_remove);
        assert_eq!(
            ui.components[1].preceding_waypoint,
            Some(NavRef::Airport("KRNT".to_string()))
        );
        assert_eq!(
            ui.components[1].following_waypoint,
            Some(NavRef::Airport("KUAO".to_string()))
        );
    }

    #[test]
    fn project_ui_state_enables_remove_and_reorder_for_plain_waypoint_routes() {
        let ui = project_ui_state(&sample_waypoint_only_plan());

        assert!(ui.components.iter().all(|component| component.can_remove));
        assert!(ui.components.iter().all(|component| component.can_reorder));
        assert!(!ui.components[0].can_reorder_up);
        assert!(ui.components[0].can_reorder_down);
        assert!(ui.components[1].can_reorder_up);
        assert!(ui.components[1].can_reorder_down);
        assert!(ui.components[2].can_reorder_up);
        assert!(!ui.components[2].can_reorder_down);

        let grouped = project_ui_state(&sample_airway_component_plan());
        assert!(grouped.components[0].can_remove);
        assert!(grouped.components[0].can_reorder);
        assert!(!grouped.components[0].can_reorder_up);
        assert!(grouped.components[0].can_reorder_down);
    }

    #[test]
    fn move_row_actions_never_dismiss_the_tray() {
        let ui = project_ui_state(&sample_waypoint_only_plan());
        let action_for_component = |component_index: usize, id: FlightPlanRowActionId| {
            ui.display_rows
                .iter()
                .find(|row| row.component_index == Some(component_index))
                .and_then(|row| flight_plan_row_actions(row).find(|action| action.id == id))
                .expect("row action")
        };

        assert!(!action_for_component(0, FlightPlanRowActionId::MoveUp).enabled);
        assert!(action_for_component(0, FlightPlanRowActionId::MoveDown).enabled);
        assert!(action_for_component(1, FlightPlanRowActionId::MoveUp).enabled);
        assert!(action_for_component(1, FlightPlanRowActionId::MoveDown).enabled);
        assert!(action_for_component(2, FlightPlanRowActionId::MoveUp).enabled);
        assert!(!action_for_component(2, FlightPlanRowActionId::MoveDown).enabled);
        assert!(!action_for_component(0, FlightPlanRowActionId::MoveUp).dismiss_tray_on_success);
        assert!(!action_for_component(0, FlightPlanRowActionId::MoveDown).dismiss_tray_on_success);
        assert!(!action_for_component(1, FlightPlanRowActionId::MoveUp).dismiss_tray_on_success);
        assert!(!action_for_component(1, FlightPlanRowActionId::MoveDown).dismiss_tray_on_success);
        assert!(!action_for_component(2, FlightPlanRowActionId::MoveUp).dismiss_tray_on_success);
        assert!(!action_for_component(2, FlightPlanRowActionId::MoveDown).dismiss_tray_on_success);
    }

    #[test]
    fn top_level_waypoint_row_uid_survives_reorder() {
        let plan = sample_waypoint_only_plan().normalized();
        let before_ui = project_ui_state(&plan);
        let moved_row = before_ui
            .display_rows
            .iter()
            .find(|row| row.nav_ref == Some(NavRef::Airport("KHIO".to_string())))
            .expect("moved row");
        let moved_uid = moved_row.uid.clone();
        let moved_component_uid = moved_row.component_uid.clone();

        let after = move_component(&plan, 2, -1).expect("move component");
        let after_ui = project_ui_state(&after);
        let after_row = after_ui
            .display_rows
            .iter()
            .find(|row| row.component_uid == moved_component_uid)
            .expect("moved row after reorder");

        assert_eq!(after_row.uid, moved_uid);
        assert_eq!(after_row.component_index, Some(1));
    }

    #[test]
    fn insert_airway_after_waypoint_appends_atomic_airway_at_route_end() {
        let inserted = insert_airway_after_waypoint(
            &sample_waypoint_only_plan(),
            2,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(
            inserted.route_components.last(),
            Some(RouteComponent::Airway { .. })
        ));
        let last_leg = inserted.resolved_legs.last().unwrap();
        assert_eq!(last_leg.from, NavRef::Fix("SUMMA".to_string()));
        assert_eq!(last_leg.to, NavRef::Fix("VAMPS".to_string()));
    }

    #[test]
    fn insert_airway_after_waypoint_preserves_existing_active_leg_guidance() {
        let mut plan = sample_waypoint_only_plan();
        plan.guidance = Some(GuidanceState {
            active_leg_index: 0,
            active_detail_index: Some(0),
            display_split_leg_id: Some("component-0-1".to_string()),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        });

        let inserted = insert_airway_after_waypoint(
            &plan,
            2,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();

        let guidance = inserted.guidance.as_ref().expect("guidance preserved");
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert_eq!(
            guidance.display_split_leg_id.as_deref(),
            Some("component-0-1")
        );
        let active_leg = active_guidance_leg(&inserted).expect("active leg preserved");
        assert_eq!(active_leg.from, NavRef::Airport("KRNT".to_string()));
        assert_eq!(active_leg.to, NavRef::Airport("KUAO".to_string()));
    }

    #[test]
    fn final_waypoint_add_airway_row_projects_open_ended_insert_span() {
        let ui = project_ui_state(&sample_waypoint_only_plan());
        let middle_row = ui
            .display_rows
            .iter()
            .find(|row| row.component_index == Some(1) && row.depth == 0)
            .expect("middle waypoint row");
        assert_eq!(middle_row.start_component_index, Some(1));
        assert_eq!(middle_row.end_component_index, Some(2));
        assert_eq!(
            middle_row.origin_anchor,
            Some(NavRef::Airport("KUAO".to_string()))
        );
        assert_eq!(
            middle_row.destination_anchor,
            Some(NavRef::Airport("KHIO".to_string()))
        );

        let row = ui
            .display_rows
            .iter()
            .find(|row| row.component_index == Some(2) && row.depth == 0)
            .expect("final waypoint row");

        assert!(flight_plan_row_actions(row)
            .any(|action| action.id == FlightPlanRowActionId::AddAirway && action.enabled));
        assert_eq!(row.start_component_index, Some(2));
        assert_eq!(row.end_component_index, None);
        assert_eq!(row.origin_anchor, Some(NavRef::Airport("KHIO".to_string())));
        assert_eq!(row.destination_anchor, None);
    }

    #[test]
    fn materialized_airway_insert_accepts_projected_final_waypoint_span() {
        let ui = project_ui_state(&sample_waypoint_only_plan());
        let row = ui
            .display_rows
            .iter()
            .find(|row| row.component_index == Some(2) && row.depth == 0)
            .expect("final waypoint row");
        let selection = crate::AirwayAutoSelection {
            airway_name: "V2".to_string(),
            branch_key: "V2-A".to_string(),
            entry: crate::AirwayEntryCandidate {
                airway_name: "V2".to_string(),
                branch_key: "V2-A".to_string(),
                branch_point_index: 0,
                sequence: 0,
                nav_ref: NavRef::Navaid("SEA".to_string()),
                distance_from_anchor_nm: 0.0,
                previous_nav_ref: None,
                next_nav_ref: Some(NavRef::Fix("SUMMA".to_string())),
            },
            exit: crate::AirwayExitCandidate {
                airway_name: "V2".to_string(),
                branch_key: "V2-A".to_string(),
                branch_point_index: 2,
                sequence: 2,
                nav_ref: NavRef::Fix("VAMPS".to_string()),
                leg_offset_from_entry: 2,
                distance_from_target_nm: None,
                is_entry: false,
            },
            origin_distance_nm: 0.0,
            destination_distance_nm: 0.0,
            total_anchor_distance_nm: 0.0,
        };
        let (airway, airway_legs) = sample_inserted_airway();

        let mutation = crate::insert_airway_materialized_ui(
            &sample_waypoint_only_plan(),
            row.start_component_index.unwrap(),
            row.end_component_index,
            selection,
            airway,
            airway_legs,
        )
        .unwrap();

        assert_eq!(mutation.mutation.component_index, 3);
        assert!(matches!(
            mutation.mutation.plan.route_components.last(),
            Some(RouteComponent::Airway { .. })
        ));
    }

    #[test]
    fn projection_hides_airway_entry_atom_when_preceded_by_same_waypoint() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V23".to_string(),
                        branch_key: Some("V23-A".to_string()),
                        entry: NavRef::Navaid("SEA".to_string()),
                        exit: NavRef::Fix("RAWER".to_string()),
                    },
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "v23-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v23-1".to_string(),
                    from: NavRef::Fix("BTG".to_string()),
                    to: NavRef::Fix("RAWER".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            ..sample_single_component_plan()
        };

        let ui = project_ui_state(&plan);
        assert_eq!(
            ui.components[1].items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("BTG".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("RAWER".to_string())
                },
            ]
        );
        assert_eq!(ui.resolved_legs[0].from, NavRef::Navaid("SEA".to_string()));
        assert_eq!(ui.resolved_legs[0].to, NavRef::Fix("BTG".to_string()));
    }

    #[test]
    fn projection_hides_airway_exit_atom_when_followed_by_same_waypoint() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V23".to_string(),
                        branch_key: Some("V23-A".to_string()),
                        entry: NavRef::Navaid("SEA".to_string()),
                        exit: NavRef::Fix("UBG".to_string()),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("UBG".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "v23-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v23-1".to_string(),
                    from: NavRef::Fix("BTG".to_string()),
                    to: NavRef::Fix("UBG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
            ],
            ..sample_single_component_plan()
        };

        let ui = project_ui_state(&plan);
        assert_eq!(
            ui.components[0].items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Navaid("SEA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("BTG".to_string())
                },
            ]
        );
        let last_leg = ui.resolved_legs.last().unwrap();
        assert_eq!(last_leg.from, NavRef::Fix("BTG".to_string()));
        assert_eq!(last_leg.to, NavRef::Fix("UBG".to_string()));
    }

    #[test]
    fn projection_hides_only_first_airway_terminal_atom_at_airway_to_airway_boundary() {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V23".to_string(),
                        branch_key: Some("V23-A".to_string()),
                        entry: NavRef::Fix("PAE".to_string()),
                        exit: NavRef::Fix("UBG".to_string()),
                    },
                },
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V165".to_string(),
                        branch_key: Some("V165-A".to_string()),
                        entry: NavRef::Fix("UBG".to_string()),
                        exit: NavRef::Fix("BTG".to_string()),
                    },
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "v23-0".to_string(),
                    from: NavRef::Fix("PAE".to_string()),
                    to: NavRef::Fix("UBG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v165-0".to_string(),
                    from: NavRef::Fix("UBG".to_string()),
                    to: NavRef::Fix("SUMMA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "v165-1".to_string(),
                    from: NavRef::Fix("SUMMA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            ..sample_single_component_plan()
        };

        let ui = project_ui_state(&plan);
        assert_eq!(
            ui.components[0].items,
            vec![ConcretizedNavItem::Waypoint {
                nav_ref: NavRef::Fix("PAE".to_string())
            }]
        );
        assert_eq!(
            ui.components[1].items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("UBG".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("SUMMA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("BTG".to_string())
                },
            ]
        );
        assert_eq!(ui.resolved_legs[0].leg_id, "v23-0");
        assert_eq!(ui.resolved_legs[1].leg_id, "v165-0");
    }

    #[test]
    fn delete_component_allows_reducing_to_single_top_level_waypoint() {
        let deleted = delete_component(&sample_two_waypoint_plan(), 1).unwrap();

        assert_eq!(deleted.route_components.len(), 1);
        assert!(deleted.resolved_legs.is_empty());
        let ui = project_ui_state(&deleted);
        assert!(ui.components[0].can_remove);
        assert!(!ui.components[0].can_reorder);
    }

    #[test]
    fn remove_all_above_removes_selected_component_and_preceding_route() {
        let trimmed = remove_all_above(&sample_four_waypoint_plan(), 2).unwrap();

        assert_eq!(trimmed.route_components.len(), 1);
        assert!(matches!(
            trimmed.route_components[0],
            RouteComponent::Waypoint { waypoint: NavRef::Airport(ref id) } if id == "KPDX"
        ));
        assert!(trimmed.resolved_legs.is_empty());
    }

    #[test]
    fn route_component_uids_survive_reorder_and_delete() {
        let plan = sample_four_waypoint_plan().normalized();
        let original_uids = plan.route_component_uids.clone();

        let moved = move_component(&plan, 0, 2).unwrap();
        assert_eq!(
            moved.route_component_uids,
            vec![
                original_uids[1].clone(),
                original_uids[2].clone(),
                original_uids[0].clone(),
                original_uids[3].clone(),
            ]
        );

        let deleted = delete_component(&moved, 1).unwrap();
        assert_eq!(
            deleted.route_component_uids,
            vec![
                original_uids[1].clone(),
                original_uids[0].clone(),
                original_uids[3].clone(),
            ]
        );
    }

    #[test]
    fn deleting_row_before_active_airway_leg_preserves_active_leg_identity() {
        let plan = sample_task_6_route_plan();
        let sea_to_vamps_index = plan
            .resolved_legs
            .iter()
            .position(|leg| {
                leg.from == NavRef::Navaid("SEA".to_string())
                    && leg.to == NavRef::Fix("VAMPS".to_string())
            })
            .expect("SEA to VAMPS leg");
        let active = activate_leg(&plan, sea_to_vamps_index).expect("activate SEA -> VAMPS");

        let deleted = delete_component(&active, 0).expect("delete KRNT row");
        let active_leg = active_guidance_leg(&deleted).expect("active leg after delete");

        assert_eq!(
            (active_leg.from, active_leg.to),
            (
                NavRef::Navaid("SEA".to_string()),
                NavRef::Fix("VAMPS".to_string())
            )
        );
    }

    #[test]
    fn insert_waypoint_allocates_new_route_component_uid_without_reusing_existing_ones() {
        let plan = sample_two_waypoint_plan().normalized();
        let original_uids = plan.route_component_uids.clone();

        let inserted = insert_waypoint(&plan, 0, false, NavRef::Navaid("SEA".to_string())).unwrap();

        assert_eq!(inserted.route_component_uids.len(), 3);
        assert_eq!(inserted.route_component_uids[0], original_uids[0]);
        assert_eq!(inserted.route_component_uids[2], original_uids[1]);
        assert_ne!(
            inserted.route_component_uids[1],
            inserted.route_component_uids[0]
        );
        assert_ne!(
            inserted.route_component_uids[1],
            inserted.route_component_uids[2]
        );
    }

    #[test]
    fn insert_airport_waypoint_adds_explicit_waypoint_before_or_after_component() {
        let inserted_before =
            insert_airport_waypoint(&sample_two_waypoint_plan(), 1, true, " khio ").unwrap();
        assert_eq!(
            inserted_before.route_components[1],
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KHIO".to_string())
            }
        );
        assert_eq!(
            inserted_before.resolved_legs[0].to,
            NavRef::Airport("KHIO".to_string())
        );
        assert_eq!(
            inserted_before.resolved_legs[1].from,
            NavRef::Airport("KHIO".to_string())
        );

        let inserted_after =
            insert_airport_waypoint(&sample_two_waypoint_plan(), 0, false, "khio").unwrap();
        assert_eq!(
            inserted_after.route_components[1],
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KHIO".to_string())
            }
        );
        assert_eq!(
            inserted_after.resolved_legs[0].to,
            NavRef::Airport("KHIO".to_string())
        );
        assert_eq!(
            inserted_after.resolved_legs[1].from,
            NavRef::Airport("KHIO".to_string())
        );
    }

    #[test]
    fn project_ui_state_enables_insert_before_and_after_for_top_level_waypoints() {
        let ui = project_ui_state(&sample_two_waypoint_plan());
        let action_ids = flight_plan_row_actions(&ui.display_rows[0])
            .map(|action| (&action.id, action.enabled))
            .collect::<Vec<_>>();

        assert!(action_ids.contains(&(&FlightPlanRowActionId::InsertBefore, true)));
        assert!(action_ids.contains(&(&FlightPlanRowActionId::InsertAfter, true)));
    }

    #[test]
    fn waypoint_plates_action_carries_core_navigation_target() {
        let ui = project_ui_state(&sample_two_waypoint_plan());
        let plates = flight_plan_row_actions(&ui.display_rows[0])
            .find(|action| action.id == FlightPlanRowActionId::Plates)
            .expect("Plates action");

        assert_eq!(
            plates.navigation,
            Some(FlightPlanRowNavigationAction::OpenAirportCharts {
                airport_id: "KRNT".to_string(),
            })
        );
    }

    #[test]
    fn show_plate_navigation_uses_enriched_plate_target() {
        let mut row = FlightPlanDisplayRowUiView {
            uid: "procedure-row".to_string(),
            label: "VOR-A".to_string(),
            row_kind: FlightPlanDisplayRowKind::Group,
            component_kind: Some(RouteComponentViewKind::Procedure),
            component_uid: Some("procedure".to_string()),
            component_index: Some(0),
            procedure_id: Some("KPAE-VOR-A".to_string()),
            procedure_kind: Some(ProcedureKind::Approach),
            leg_index: None,
            data_cells: Vec::new(),
            show_plate_target_id: None,
            chart_airport_id: Some("KPAE".to_string()),
            nav_ref: None,
            symbol_feature: None,
            depth: 0,
            active: false,
            enabled: true,
            disabled_reason: None,
            synthetic_direct_to: false,
            can_add_airway_after: false,
            can_add_procedure_before: false,
            can_remove_component: true,
            can_reorder_component: false,
            can_reorder_up: false,
            can_reorder_down: false,
            replace_procedure_component_index: None,
            start_component_index: None,
            end_component_index: None,
            origin_anchor: None,
            destination_anchor: None,
            preceding_waypoint: None,
            following_waypoint: None,
            action_matrix: action_matrix_from_actions(&assign_action_uids(
                "procedure-row",
                vec![action(FlightPlanRowActionId::ShowPlate, true)],
            )),
        };

        refresh_flight_plan_row_action_navigation(&mut row);
        let show_plate = flight_plan_row_actions(&row)
            .find(|action| action.id == FlightPlanRowActionId::ShowPlate)
            .expect("Show Plate action");
        assert_eq!(show_plate.navigation, None);

        row.show_plate_target_id = Some("Plate:KPAE:KPAE-VOR-A".to_string());
        refresh_flight_plan_row_action_navigation(&mut row);
        let show_plate = flight_plan_row_actions(&row)
            .find(|action| action.id == FlightPlanRowActionId::ShowPlate)
            .expect("Show Plate action");
        assert_eq!(
            show_plate.navigation,
            Some(FlightPlanRowNavigationAction::OpenPlateTarget {
                airport_id: "KPAE".to_string(),
                target: "Plate:KPAE:KPAE-VOR-A".to_string(),
            })
        );
    }

    #[test]
    fn project_ui_state_exposes_conceptual_action_matrix_for_waypoint_menu() {
        let ui = project_ui_state(&sample_two_waypoint_plan());
        let action_matrix = ui.display_rows[0]
            .action_matrix
            .iter()
            .map(|row| {
                row.iter()
                    .map(|action| action.id.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            action_matrix,
            vec![
                vec![
                    FlightPlanRowActionId::ActivateLeg,
                    FlightPlanRowActionId::DirectTo,
                ],
                vec![
                    FlightPlanRowActionId::InsertBefore,
                    FlightPlanRowActionId::MoveUp,
                ],
                vec![
                    FlightPlanRowActionId::InsertAfter,
                    FlightPlanRowActionId::MoveDown,
                ],
                vec![
                    FlightPlanRowActionId::Remove,
                    FlightPlanRowActionId::RemoveAllAbove,
                ],
                vec![
                    FlightPlanRowActionId::AddAirway,
                    FlightPlanRowActionId::SelectProcedure,
                ],
                vec![
                    FlightPlanRowActionId::WaypointInfo,
                    FlightPlanRowActionId::Plates
                ],
                vec![FlightPlanRowActionId::Weather],
            ]
        );
    }

    #[test]
    fn project_ui_state_exposes_remove_all_above_for_nonfirst_waypoints() {
        let ui = project_ui_state(&sample_four_waypoint_plan());
        let first_actions = flight_plan_row_actions(&ui.display_rows[0])
            .map(|action| (&action.id, action.enabled))
            .collect::<Vec<_>>();
        let third_actions = flight_plan_row_actions(&ui.display_rows[2])
            .map(|action| (&action.id, action.enabled))
            .collect::<Vec<_>>();

        assert!(first_actions.contains(&(&FlightPlanRowActionId::RemoveAllAbove, true)));
        assert!(third_actions.contains(&(&FlightPlanRowActionId::RemoveAllAbove, true)));
    }

    #[test]
    fn remove_action_disabled_reasons_are_specific() {
        assert_eq!(
            row_action_disabled_reason(&FlightPlanRowActionId::Remove, false).as_deref(),
            Some(WAYPOINT_REMOVE_DISABLED_REASON)
        );
        assert_eq!(
            row_action_disabled_reason(&FlightPlanRowActionId::RemoveAirway, false).as_deref(),
            Some(AIRWAY_REMOVE_DISABLED_REASON)
        );
        assert_eq!(
            row_action_disabled_reason(&FlightPlanRowActionId::RemoveProcedure, false).as_deref(),
            Some(PROCEDURE_REMOVE_DISABLED_REASON)
        );
        assert_eq!(
            row_action_disabled_reason(&FlightPlanRowActionId::RemoveAllAbove, false).as_deref(),
            Some(REMOVE_ALL_ABOVE_DISABLED_REASON)
        );
    }

    #[test]
    fn move_component_reorders_top_level_components_even_when_grouped() {
        let moved = move_component(&sample_airway_component_plan(), 2, -1).unwrap();

        assert!(matches!(
            moved.route_components[0],
            RouteComponent::Waypoint { .. }
        ));
        assert!(matches!(
            moved.route_components[1],
            RouteComponent::Waypoint { .. }
        ));
        assert!(matches!(
            moved.route_components[2],
            RouteComponent::Airway { .. }
        ));
        let ui = project_ui_state(&moved);
        assert!(ui.components.iter().all(|component| component.can_reorder));
    }

    #[test]
    fn project_ui_state_enables_procedure_insertion_before_airport_with_waypoint_predecessor() {
        let ui = project_ui_state(&sample_waypoint_only_plan());

        assert!(ui.components[0].can_add_procedure_before);
        assert!(ui.components[1].can_add_procedure_before);
        assert!(ui.components[2].can_add_procedure_before);
    }

    #[test]
    fn project_ui_state_enables_procedure_insertion_for_single_airport_plan() {
        let ui = project_ui_state(&sample_single_component_plan());

        assert_eq!(ui.components.len(), 1);
        assert!(ui.components[0].can_add_procedure_before);
        let airport_row = ui
            .display_rows
            .iter()
            .find(|row| row.component_index == Some(0))
            .expect("single airport row");
        assert!(
            flight_plan_row_actions(airport_row)
                .any(|action| action.id == FlightPlanRowActionId::SelectProcedure && action.enabled),
            "single airport row should offer Select Procedure"
        );
    }

    #[test]
    fn insert_initial_procedure_before_airport_allows_airport_only_route_to_load_approach() {
        let plan = sample_single_component_plan();
        let procedure = ProcedureSegment {
            airport_id: AirportId("KRNT".to_string()),
            procedure_id: "R16".to_string(),
            display_label: None,
            kind: ProcedureKind::Approach,
            runway_transition: None,
            enroute_transition: Some("JAWBN".to_string()),
            terminal_discontinuity: None,
            data_quality: Vec::new(),
        };
        let procedure_legs = vec![
            ResolvedLeg {
                id: "procedure-R16-A-10".to_string(),
                from: NavRef::Fix("JAWBN".to_string()),
                to: NavRef::Fix("KANGU".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "procedure-R16-R-20".to_string(),
                from: NavRef::Fix("KANGU".to_string()),
                to: NavRef::Airport("KRNT".to_string()),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 99,
                },
                procedure_provenance: None,
            },
        ];

        let inserted =
            insert_initial_procedure_before_airport(&plan, 0, procedure, procedure_legs).unwrap();

        assert!(matches!(
            inserted.route_components[0],
            RouteComponent::Procedure { .. }
        ));
        assert!(matches!(
            inserted.route_components[1],
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport(ref airport)
            } if airport == "KRNT"
        ));
        assert_eq!(inserted.resolved_legs.len(), 2);
        assert!(inserted
            .resolved_legs
            .iter()
            .all(|leg| { leg.source == ResolvedLegSource::RouteComponent { component_index: 0 } }));
    }

    #[test]
    fn project_ui_state_enables_procedure_replacement_before_airport_with_matching_approach_predecessor(
    ) {
        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KUAO".to_string()),
                        procedure_id: "I35".to_string(),
                        display_label: None,
                        kind: ProcedureKind::Approach,
                        runway_transition: Some("RW35".to_string()),
                        enroute_transition: Some("FOO".to_string()),
                        terminal_discontinuity: None,
                        data_quality: Vec::new(),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Fix("FOO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-0".to_string(),
                    from: NavRef::Fix("FOO".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            ..sample_waypoint_only_plan()
        };

        let ui = project_ui_state(&plan);

        assert!(ui.components[2].can_add_procedure_before);
        assert_eq!(ui.components[2].replace_procedure_component_index, Some(1));
        let airport_row = ui
            .display_rows
            .iter()
            .find(|row| {
                row.component_index == Some(2) && row.row_kind == FlightPlanDisplayRowKind::Waypoint
            })
            .expect("airport row");
        assert_eq!(airport_row.replace_procedure_component_index, Some(1));
    }

    #[test]
    fn move_component_round_trip_preserves_seeded_grouped_materialization() {
        let initial = sample_seeded_reorder_plan();
        let initial_ui = project_ui_state(&initial);

        let moved_down_once = move_component(&initial, 0, 1).unwrap();
        let moved_down_twice = move_component(&moved_down_once, 1, 1).unwrap();
        let moved_to_bottom = move_component(&moved_down_twice, 2, 1).unwrap();

        let moved_up_once = move_component(&moved_to_bottom, 3, -1).unwrap();
        let moved_up_twice = move_component(&moved_up_once, 2, -1).unwrap();
        let final_plan = move_component(&moved_up_twice, 1, -1).unwrap();
        let final_ui = project_ui_state(&final_plan);

        assert_eq!(final_ui.components, initial_ui.components);
        assert_eq!(final_ui.resolved_legs, initial_ui.resolved_legs);
    }

    #[test]
    fn single_top_level_component_disables_reorder() {
        let ui = project_ui_state(&sample_single_component_plan());
        assert_eq!(ui.components.len(), 1);
        assert!(!ui.components[0].can_reorder);
        assert!(!ui.components[0].can_reorder_up);
        assert!(!ui.components[0].can_reorder_down);
    }

    #[test]
    fn project_ui_state_marks_active_procedure_component_and_discontinuity() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let guided = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 2,
                active_detail_index: Some(2),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..inserted
        };

        let ui = project_ui_state(&guided);

        assert!(ui.components[1].active);
        assert_eq!(ui.components[1].kind, RouteComponentViewKind::Procedure);
        assert!(matches!(
            ui.components[1].items.last(),
            Some(ConcretizedNavItem::Discontinuity {
                discontinuity: ProcedureDiscontinuity::Vectors,
                ..
            })
        ));
        assert_eq!(
            ui.guidance.as_ref().unwrap().active_component_index,
            Some(1)
        );
        assert_eq!(ui.guidance.as_ref().unwrap().active_leg_index, Some(2));
    }

    #[test]
    fn project_ui_state_exposes_nav_element_summary_without_session_geometry() {
        let guided = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..sample_waypoint_only_plan()
        };

        let ui = project_ui_state(&guided);
        let nav_element = &ui.guidance.as_ref().unwrap().nav_element;

        assert_eq!(nav_element.active_leg_summary, "KRNT \u{2192} KUAO");
        assert_eq!(nav_element.cdi_indicator_dots, None);
    }

    #[test]
    fn project_ui_state_exposes_direct_to_without_ui_recomputing_on_plan_status() {
        let activated = activate_direct_to_test_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let ui = project_ui_state(&activated);

        let guidance = ui.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(guidance.active_component_index, Some(3));
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert!(direct_to.on_plan_target);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert_eq!(
            guidance.active_to_row_uid.as_deref(),
            Some(direct_to.target_row_id.as_str())
        );
    }

    #[test]
    fn off_plan_direct_to_projects_disabled_plan_and_synthetic_target_row() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            NavRef::Airport("KPSC".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&activated);
        let rows = ui.display_rows;

        assert!(rows[..rows.len() - 1].iter().all(|row| !row.enabled));
        assert!(rows[..rows.len() - 1].iter().all(|row| {
            row.disabled_reason.as_deref() == Some(OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON)
        }));
        assert!(rows[..rows.len() - 1]
            .iter()
            .flat_map(flight_plan_row_actions)
            .all(|action| !action.enabled));
        assert!(rows[..rows.len() - 1]
            .iter()
            .flat_map(flight_plan_row_actions)
            .all(|action| action.disabled_reason.as_deref()
                == Some(OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON)));
        let direct_row = rows.last().expect("synthetic direct-to row");
        assert_eq!(direct_row.label, "KPSC");
        assert_eq!(
            direct_row.nav_ref,
            Some(NavRef::Airport("KPSC".to_string()))
        );
        assert!(direct_row.active);
        assert!(direct_row.enabled);
        assert!(direct_row.synthetic_direct_to);
        assert_eq!(
            ui.guidance
                .as_ref()
                .and_then(|guidance| guidance.direct_to.as_ref())
                .map(|direct_to| direct_to.on_plan_target),
            Some(false)
        );
        let restore_control = ui
            .controls
            .iter()
            .find(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo))
            .expect("restore direct-to control");
        assert_eq!(restore_control.label, "Restore\nFP");
        assert!(restore_control.enabled);
        assert_eq!(restore_control.disabled_reason, None);
        assert!(ui
            .controls
            .last()
            .is_some_and(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo)));
    }

    #[test]
    fn project_ui_state_projects_compact_flight_plan_control_labels() {
        let ui = project_ui_state(&sample_guided_waypoint_plan());

        assert_eq!(
            ui.controls
                .iter()
                .map(|control| control.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Next\nLeg",
                "SQNC",
                "STOP\nNAV",
                "SUSP",
                "Unsusp",
                "Restore\nFP"
            ]
        );
    }

    #[test]
    fn stop_navigation_control_is_enabled_only_when_guidance_exists() {
        let guided = project_ui_state(&sample_guided_waypoint_plan());
        let stopped = project_ui_state(&stop_navigation(&sample_guided_waypoint_plan()).unwrap());

        assert!(guided
            .controls
            .iter()
            .find(|control| matches!(&control.id, FlightPlanControlId::StopNavigation))
            .is_some_and(|control| control.enabled));
        assert!(stopped
            .controls
            .iter()
            .find(|control| matches!(&control.id, FlightPlanControlId::StopNavigation))
            .is_some_and(|control| !control.enabled));
    }

    #[test]
    fn on_plan_direct_to_keeps_flight_plan_projection_enabled() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&activated);
        let rows = ui.display_rows;

        assert!(rows.iter().all(|row| row.enabled));
        assert!(rows.iter().all(|row| !row.synthetic_direct_to));
        assert_eq!(
            ui.guidance
                .as_ref()
                .and_then(|guidance| guidance.direct_to.as_ref())
                .map(|direct_to| direct_to.on_plan_target),
            Some(true)
        );
        let restore_control = ui
            .controls
            .iter()
            .find(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo))
            .expect("restore direct-to control");
        assert_eq!(restore_control.label, "Restore\nFP");
        assert!(!restore_control.enabled);
        assert_eq!(
            restore_control.disabled_reason.as_deref(),
            Some("No off-plan Direct-To is active.")
        );
        assert!(ui
            .controls
            .last()
            .is_some_and(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo)));
    }

    #[test]
    fn direct_to_first_waypoint_is_on_plan_without_overlay() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            NavRef::Airport("KRNT".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&activated);
        let rows = ui.display_rows;

        assert!(rows.iter().all(|row| row.enabled));
        assert!(rows.iter().all(|row| !row.synthetic_direct_to));
        assert_eq!(
            ui.guidance
                .as_ref()
                .and_then(|guidance| guidance.direct_to.as_ref())
                .map(|direct_to| direct_to.on_plan_target),
            Some(true)
        );
        let restore_control = ui
            .controls
            .iter()
            .find(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo))
            .expect("restore direct-to control");
        assert_eq!(restore_control.label, "Restore\nFP");
        assert!(!restore_control.enabled);
        assert_eq!(
            restore_control.disabled_reason.as_deref(),
            Some("No off-plan Direct-To is active.")
        );
        assert!(ui
            .controls
            .last()
            .is_some_and(|control| matches!(&control.id, FlightPlanControlId::RestoreDirectTo)));
    }

    #[test]
    fn direct_to_route_origin_survives_guidance_revalidation_and_resumes_route() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            NavRef::Airport("KRNT".to_string()),
        )
        .unwrap();
        let revalidated =
            revalidate_guidance_after_plan_edit(activated.guidance.clone(), &activated)
                .expect("revalidation succeeds")
                .expect("revalidated guidance");
        let direct_to = revalidated.direct_to.as_ref().expect("direct-to state");
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), None);
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(0));

        let sequenced = sequence_active_detail(&FlightPlan {
            guidance: Some(revalidated),
            ..activated
        })
        .expect("sequence direct-to route origin");
        let guidance = sequenced.guidance.as_ref().expect("sequenced guidance");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert!(guidance.direct_to.is_none());
        assert_eq!(guidance.suspend_reason, None);
        assert_eq!(
            active_guidance_leg(&sequenced),
            Some(PlanLeg {
                from: NavRef::Airport("KRNT".to_string()),
                to: NavRef::Navaid("SEA".to_string()),
                airway: None,
            })
        );
    }

    #[test]
    fn direct_to_route_origin_component_survives_guidance_revalidation_and_resumes_route() {
        let activated = activate_direct_to_test_component(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            0,
        )
        .unwrap();
        let revalidated =
            revalidate_guidance_after_plan_edit(activated.guidance.clone(), &activated)
                .expect("revalidation succeeds")
                .expect("revalidated guidance");
        let direct_to = revalidated.direct_to.as_ref().expect("direct-to state");
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), None);
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(0));

        let sequenced = sequence_active_detail(&FlightPlan {
            guidance: Some(revalidated),
            ..activated
        })
        .expect("sequence direct-to route origin");
        let guidance = sequenced.guidance.as_ref().expect("sequenced guidance");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert!(guidance.direct_to.is_none());
        assert_eq!(guidance.suspend_reason, None);
        assert_eq!(
            active_guidance_leg(&sequenced),
            Some(PlanLeg {
                from: NavRef::Airport("KRNT".to_string()),
                to: NavRef::Navaid("SEA".to_string()),
                airway: None,
            })
        );
    }

    #[test]
    fn direct_to_real_waypoint_route_origin_component_recovers_missing_resume_leg() {
        let mut activated = activate_direct_to_test_component(
            &modda_zgood_normy_waypoint_plan(),
            LatLon {
                lat: 40.0,
                lon: -120.02,
            },
            0,
        )
        .unwrap();
        {
            let direct_to = activated
                .guidance
                .as_mut()
                .and_then(|guidance| guidance.direct_to.as_mut())
                .expect("direct-to state");
            assert_eq!(direct_to.target, NavRef::Fix("MODDA".to_string()));
            assert!(direct_to.target_row.is_planned());
            assert!(direct_to.resume_row_id.is_some());
            direct_to.resume_row_id = None;
        }

        let revalidated =
            revalidate_guidance_after_plan_edit(activated.guidance.clone(), &activated)
                .expect("revalidation succeeds")
                .expect("revalidated guidance");
        let expected_resume_row = planned_row_id_for_leg_index(&activated, 0).expect("resume row");
        assert_eq!(
            revalidated
                .direct_to
                .as_ref()
                .and_then(|direct_to| direct_to.resume_row_id.as_ref()),
            Some(&expected_resume_row)
        );
        let sequenced = sequence_active_detail(&FlightPlan {
            guidance: Some(revalidated),
            ..activated
        })
        .expect("sequence direct-to route origin");
        let guidance = sequenced.guidance.as_ref().expect("sequenced guidance");
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.active_detail_index, Some(0));
        assert!(guidance.direct_to.is_none());
        assert_eq!(
            active_guidance_leg(&sequenced),
            Some(PlanLeg {
                from: NavRef::Fix("MODDA".to_string()),
                to: NavRef::Fix("ZGOOD".to_string()),
                airway: None,
            })
        );
    }

    #[test]
    fn direct_to_duplicate_first_component_does_not_target_later_duplicate() {
        let activated = activate_direct_to_test_component(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            1,
        )
        .unwrap();
        let direct_to = activated
            .guidance
            .as_ref()
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");

        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(0));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(1));

        let activated = activate_direct_to_test_component(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            3,
        )
        .unwrap();
        let direct_to = activated
            .guidance
            .as_ref()
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");

        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(2));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(3));
    }

    #[test]
    fn direct_to_duplicate_component_projects_clicked_row_uid() {
        let activated = activate_direct_to_test_component(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            3,
        )
        .unwrap();
        let ui = project_ui_state(&activated);
        let rows = ui
            .display_rows
            .iter()
            .filter(|row| row.row_kind == FlightPlanDisplayRowKind::Waypoint && row.depth == 0)
            .collect::<Vec<_>>();
        let first_duplicate_uid = rows
            .iter()
            .find(|row| {
                row.nav_ref == Some(NavRef::Fix("IAF".to_string()))
                    && row.component_index == Some(1)
            })
            .expect("first IAF row")
            .uid
            .clone();
        let clicked_duplicate_uid = rows
            .iter()
            .find(|row| {
                row.nav_ref == Some(NavRef::Fix("IAF".to_string()))
                    && row.component_index == Some(3)
            })
            .expect("clicked IAF row")
            .uid
            .clone();

        assert_ne!(first_duplicate_uid, clicked_duplicate_uid);
        assert_eq!(
            ui.guidance
                .as_ref()
                .and_then(|guidance| guidance.active_from_row_uid.as_ref()),
            None
        );
        assert_eq!(
            ui.guidance
                .as_ref()
                .and_then(|guidance| guidance.active_to_row_uid.as_ref()),
            Some(&clicked_duplicate_uid)
        );
    }

    #[test]
    fn appending_duplicate_waypoint_preserves_active_row_and_activate_leg_actions() {
        let appended = insert_waypoint(
            &sample_duplicate_waypoint_plan(),
            4,
            false,
            NavRef::Airport("KAAA".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&appended);
        let rows = ui.display_rows;
        let guidance = ui.guidance.as_ref().expect("guidance");
        let active_to_uid = guidance
            .active_to_row_uid
            .as_ref()
            .expect("active to row uid");
        let active_to_row = rows
            .iter()
            .find(|row| row.uid == *active_to_uid)
            .expect("active to row");

        assert_eq!(guidance.active_leg_index, Some(0));
        assert_eq!(active_to_row.nav_ref, Some(NavRef::Fix("IAF".to_string())));
        for row in rows.iter().filter(|row| {
            row.row_kind == FlightPlanDisplayRowKind::Waypoint
                && row.component_index.is_some()
                && row.leg_index.is_some()
                && row.leg_index != guidance.active_leg_index
        }) {
            let activate_leg = flight_plan_row_actions(row)
                .find(|action| action.id == FlightPlanRowActionId::ActivateLeg)
                .expect("activate-leg action");
            assert!(
                activate_leg.enabled,
                "activate leg should stay enabled for {}",
                row.uid
            );
        }
    }

    #[test]
    fn direct_to_duplicate_route_origin_component_does_not_target_later_duplicate() {
        let mut plan = sample_duplicate_waypoint_plan();
        plan.route_components[4] = RouteComponent::Waypoint {
            waypoint: NavRef::Airport("KAAA".to_string()),
        };
        plan.resolved_legs[3].to = NavRef::Airport("KAAA".to_string());

        let activated = activate_direct_to_test_component(
            &plan,
            LatLon {
                lat: 47.5,
                lon: -122.0,
            },
            0,
        )
        .unwrap();
        let direct_to = activated
            .guidance
            .as_ref()
            .and_then(|guidance| guidance.direct_to.as_ref())
            .expect("direct-to state");

        assert_eq!(direct_to.target, NavRef::Airport("KAAA".to_string()));
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), None);
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(0));
    }

    #[test]
    fn duplicate_waypoint_rows_project_occurrence_specific_leg_indices() {
        let ui = project_ui_state(&sample_duplicate_waypoint_plan());
        let rows = ui
            .display_rows
            .iter()
            .filter(|row| row.row_kind == FlightPlanDisplayRowKind::Waypoint && row.depth == 0)
            .collect::<Vec<_>>();

        assert_eq!(rows[0].label, "KAAA");
        assert_eq!(rows[0].leg_index, None);
        assert_eq!(rows[1].label, "IAF");
        assert_eq!(rows[1].leg_index, Some(0));
        assert_eq!(rows[2].label, "PTURN");
        assert_eq!(rows[2].leg_index, Some(1));
        assert_eq!(rows[3].label, "IAF");
        assert_eq!(rows[3].leg_index, Some(2));
        assert_ne!(rows[1].uid, rows[3].uid);
    }

    #[test]
    fn adjacent_duplicate_waypoint_rows_keep_nav_refs() {
        let inserted = insert_waypoint(
            &sample_waypoint_only_plan(),
            2,
            true,
            NavRef::Airport("KHIO".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&inserted);
        let rows = ui
            .display_rows
            .iter()
            .filter(|row| row.row_kind == FlightPlanDisplayRowKind::Waypoint && row.depth == 0)
            .collect::<Vec<_>>();
        let hio_rows = rows
            .iter()
            .filter(|row| row.label == "KHIO")
            .collect::<Vec<_>>();

        assert_eq!(hio_rows.len(), 2);
        assert!(hio_rows
            .iter()
            .all(|row| row.nav_ref == Some(NavRef::Airport("KHIO".to_string()))));
    }

    #[test]
    fn adjacent_duplicate_waypoint_only_resolved_target_is_activatable() {
        let inserted = insert_waypoint(
            &sample_waypoint_only_plan(),
            2,
            true,
            NavRef::Airport("KHIO".to_string()),
        )
        .unwrap();
        let ui = project_ui_state(&inserted);
        let hio_rows = ui
            .display_rows
            .iter()
            .filter(|row| row.row_kind == FlightPlanDisplayRowKind::Waypoint && row.label == "KHIO")
            .collect::<Vec<_>>();

        assert_eq!(hio_rows.len(), 2);
        assert_eq!(hio_rows[0].leg_index, None);
        assert_eq!(hio_rows[1].leg_index, Some(1));
        assert!(
            !flight_plan_row_actions(hio_rows[0])
                .find(|action| action.id == FlightPlanRowActionId::ActivateLeg)
                .expect("activate-leg action")
                .enabled
        );
        assert!(
            flight_plan_row_actions(hio_rows[1])
                .find(|action| action.id == FlightPlanRowActionId::ActivateLeg)
                .expect("activate-leg action")
                .enabled
        );
    }

    #[test]
    fn waypoint_rows_expose_core_session_direct_to_action() {
        let inserted = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let ui = project_ui_state(&inserted);
        let child_row = ui
            .display_rows
            .iter()
            .find(|row| row.depth == 1 && row.row_kind == FlightPlanDisplayRowKind::Waypoint)
            .expect("nested airway waypoint row");
        let direct_to = flight_plan_row_actions(child_row)
            .find(|action| action.id == FlightPlanRowActionId::DirectTo)
            .expect("direct-to action");

        assert!(!child_row.uid.is_empty());
        assert!(!direct_to.uid.is_empty());
        assert!(direct_to.enabled);
        assert_eq!(
            direct_to.execution,
            FlightPlanRowActionExecution::CoreSession
        );
    }

    #[test]
    fn airway_child_rows_expose_remove_for_visible_endpoints_only_and_remove_all_above_for_all() {
        let ui = project_ui_state(&sample_v165_plan_with_explicit_endpoints());
        let airway_rows = ui
            .display_rows
            .iter()
            .filter(|row| {
                row.component_kind == Some(RouteComponentViewKind::Airway)
                    && row.row_kind == FlightPlanDisplayRowKind::Waypoint
            })
            .collect::<Vec<_>>();
        let row_actions = |label: &str| {
            let row = airway_rows
                .iter()
                .find(|row| row.label == label)
                .unwrap_or_else(|| panic!("airway row {label}"));
            flight_plan_row_actions(row)
                .map(|action| (action.id.clone(), action.enabled, action.execution.clone()))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            airway_rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            vec!["CETRA", "HOKBO", "UBG"]
        );
        assert!(row_actions("CETRA").contains(&(
            FlightPlanRowActionId::Remove,
            true,
            FlightPlanRowActionExecution::CoreSession
        )));
        assert!(row_actions("HOKBO").contains(&(
            FlightPlanRowActionId::Remove,
            false,
            FlightPlanRowActionExecution::CoreSession
        )));
        let hokbo_remove = airway_rows
            .iter()
            .find(|row| row.label == "HOKBO")
            .and_then(|row| {
                flight_plan_row_actions(row)
                    .find(|action| action.id == FlightPlanRowActionId::Remove)
            })
            .expect("HOKBO remove action");
        assert_eq!(
            hokbo_remove.disabled_reason.as_deref(),
            Some(AIRWAY_ENDPOINT_REMOVE_DISABLED_REASON)
        );
        assert!(row_actions("UBG").contains(&(
            FlightPlanRowActionId::Remove,
            true,
            FlightPlanRowActionExecution::CoreSession
        )));
        for label in ["CETRA", "HOKBO", "UBG"] {
            assert!(row_actions(label).contains(&(
                FlightPlanRowActionId::RemoveAllAbove,
                true,
                FlightPlanRowActionExecution::CoreSession
            )));
        }
    }

    #[test]
    fn remove_first_visible_airway_child_retargets_airway_entry() {
        let changed = remove_airway_child_waypoint(
            &sample_v165_plan_with_explicit_endpoints(),
            2,
            &NavRef::Fix("CETRA".to_string()),
        )
        .unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[2] else {
            panic!("expected airway");
        };
        assert_eq!(airway.entry, NavRef::Fix("HOKBO".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("RAWER".to_string()));
        assert!(changed
            .resolved_legs
            .iter()
            .any(|leg| leg.from == NavRef::Fix("HOKBO".to_string())
                && leg.to == NavRef::Fix("UBG".to_string())));
    }

    #[test]
    fn remove_last_visible_airway_child_retargets_airway_exit() {
        let changed = remove_airway_child_waypoint(
            &sample_v165_plan_with_explicit_endpoints(),
            2,
            &NavRef::Fix("UBG".to_string()),
        )
        .unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[2] else {
            panic!("expected airway");
        };
        assert_eq!(airway.entry, NavRef::Navaid("OLM".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("HOKBO".to_string()));
        assert!(changed
            .resolved_legs
            .iter()
            .any(|leg| leg.from == NavRef::Fix("CETRA".to_string())
                && leg.to == NavRef::Fix("HOKBO".to_string())));
    }

    #[test]
    fn remove_all_above_airway_child_retargets_airway_to_remaining_tail() {
        let changed = remove_all_above_airway_child_waypoint(
            &sample_v165_plan_with_explicit_endpoints(),
            2,
            &NavRef::Fix("CETRA".to_string()),
        )
        .unwrap();

        assert_eq!(changed.route_components.len(), 3);
        let RouteComponent::Airway { airway } = &changed.route_components[0] else {
            panic!("expected airway");
        };
        assert_eq!(airway.entry, NavRef::Fix("HOKBO".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("RAWER".to_string()));
    }

    #[test]
    fn remove_all_above_next_to_last_airway_child_collapses_to_singleton_waypoint() {
        let mut plan = sample_v165_plan_with_explicit_endpoints();
        plan.route_components.remove(3);
        plan = plan.normalized();
        let changed =
            remove_all_above_airway_child_waypoint(&plan, 2, &NavRef::Fix("UBG".to_string()))
                .unwrap();

        assert!(matches!(
            changed.route_components.first(),
            Some(RouteComponent::Waypoint {
                waypoint: NavRef::Fix(id)
            }) if id == "RAWER"
        ));
    }

    #[test]
    fn remove_all_above_last_airway_child_deletes_airway_block() {
        let changed = remove_all_above_airway_child_waypoint(
            &sample_v165_plan_with_explicit_endpoints(),
            2,
            &NavRef::Fix("RAWER".to_string()),
        )
        .unwrap();

        assert!(matches!(
            changed.route_components.first(),
            Some(RouteComponent::Waypoint {
                waypoint: NavRef::Fix(id)
            }) if id == "RAWER"
        ));
        assert!(!matches!(
            changed.route_components.first(),
            Some(RouteComponent::Airway { .. })
        ));
    }

    #[test]
    fn direct_to_last_leg_of_terminal_discontinuous_procedure_has_no_resume_leg() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();

        let activated = activate_direct_to_test_leg(
            &inserted,
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "proc-autto1-2",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(3));
        assert!(direct_to.resume_row_id.is_none());

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn direct_to_interior_leg_of_terminal_discontinuous_procedure_resumes_within_procedure() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();

        let activated = activate_direct_to_test_leg(
            &inserted,
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "proc-autto1-1",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert!(direct_to.target_row.is_planned());
        assert_eq!(direct_to_target_leg_index(&activated, direct_to), Some(2));
        assert_eq!(direct_to_resume_leg_index(&activated, direct_to), Some(3));

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 3);
    }

    #[test]
    fn leg_display_path_omits_debug_provenance_from_runtime_payload() {
        let path = LegDisplayPath {
            style: LegDisplayPathStyle::Solid,
            elements: Vec::new(),
            effective_terminal_course_deg: None,
            debug_element_sources: vec!["internal-source".to_string()],
            debug_element_roles: vec!["internal-role".to_string()],
        };

        let value = serde_json::to_value(&path).unwrap();

        assert!(value.get("debug_element_sources").is_none());
        assert!(value.get("debug_element_roles").is_none());
    }
}
