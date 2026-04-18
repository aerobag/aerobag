use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;
use crate::planning::{
    interpret_path_termination, ConcretizedNavItem, NavRef, PathTermination,
    ProcedureKind, ResolvedLeg,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPoint {
    pub airway_name: String,
    pub sequence: i32,
    pub position: LatLon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayFixPoint {
    pub airway_name: String,
    pub sequence: i32,
    pub position: LatLon,
    pub nav_ref: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayBranch {
    pub display_name: String,
    pub branch_key: String,
    pub points: Vec<AirwayFixPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwaySuggestion {
    pub airway_name: String,
    pub nearest_branch_key: Option<String>,
    pub nearest_nav_ref: NavRef,
    pub nearest_sequence: i32,
    pub distance_from_anchor_nm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaypointIdentifierSuggestion {
    pub identifier: String,
    pub nav_ref: NavRef,
    pub kind: String,
    pub display_name: String,
    pub distance_from_anchor_nm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayEntryCandidate {
    pub airway_name: String,
    pub branch_key: String,
    pub branch_point_index: usize,
    pub sequence: i32,
    pub nav_ref: NavRef,
    pub distance_from_anchor_nm: f64,
    pub previous_nav_ref: Option<NavRef>,
    pub next_nav_ref: Option<NavRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayExitCandidate {
    pub airway_name: String,
    pub branch_key: String,
    pub branch_point_index: usize,
    pub sequence: i32,
    pub nav_ref: NavRef,
    pub leg_offset_from_entry: isize,
    pub is_entry: bool,
    pub distance_from_target_nm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayExitSelection {
    pub airway_name: String,
    pub branch_key: String,
    pub entry_branch_point_index: usize,
    pub recommended_exit_branch_point_index: Option<usize>,
    pub candidates: Vec<AirwayExitCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayAutoSelection {
    pub airway_name: String,
    pub branch_key: String,
    pub entry: AirwayEntryCandidate,
    pub exit: AirwayExitCandidate,
    pub origin_distance_nm: f64,
    pub destination_distance_nm: f64,
    pub total_anchor_distance_nm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPresentationPoint {
    pub branch_point_index: usize,
    pub sequence: i32,
    pub nav_ref: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPresentationPlan {
    pub airway_name: String,
    pub branch_key: String,
    pub points: Vec<AirwayPresentationPoint>,
    pub suggested_entry_index: usize,
    pub suggested_exit_index: Option<usize>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureLegRecord {
    pub key: ProcedureVariantKey,
    pub sequence: i32,
    pub fix_identifier: String,
    pub path_termination: String,
    pub path_termination_kind: PathTermination,
    pub inferred_kind: ProcedureKind,
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
    pub airport_magnetic_variation_deg: Option<f64>,
    pub altitude_1_ft: Option<f64>,
    pub altitude_2_ft: Option<f64>,
    pub path_termination: String,
    pub path_termination_kind: PathTermination,
    pub turn_direction: Option<String>,
    pub magnetic_course_deg: Option<f64>,
    pub route_distance_or_time: Option<String>,
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
    magnetic_course_deg: Option<f64>,
    #[serde(default)]
    route_distance_or_time: Option<String>,
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
            airport_magnetic_variation_deg: self.airport_magnetic_variation_deg,
            altitude_1_ft: self.altitude_1_ft,
            altitude_2_ft: self.altitude_2_ft,
            path_termination: self.path_termination.clone(),
            path_termination_kind: Some(self.path_termination_kind.clone()),
            turn_direction: self.turn_direction.clone(),
            magnetic_course_deg: self.magnetic_course_deg,
            route_distance_or_time: self.route_distance_or_time.clone(),
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
            airport_magnetic_variation_deg: raw.airport_magnetic_variation_deg,
            altitude_1_ft: raw.altitude_1_ft,
            altitude_2_ft: raw.altitude_2_ft,
            path_termination_kind: raw
                .path_termination_kind
                .unwrap_or_else(|| interpret_path_termination(&raw.path_termination)),
            path_termination: raw.path_termination,
            turn_direction: raw.turn_direction,
            magnetic_course_deg: raw.magnetic_course_deg,
            route_distance_or_time: raw.route_distance_or_time,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureSummary {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CifpTppMatchRow {
    pub airport_id: String,
    pub cifp_id: String,
    pub plate_id: String,
    pub plate_label: String,
    pub package_id: String,
    pub public: i32,
    pub priority: i32,
    pub match_kind: String,
    pub is_primary: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CifpTppMatch {
    pub airport_id: String,
    pub cifp_id: String,
    pub plate_id: String,
    pub plate_label: String,
    pub package_id: String,
    pub match_kind: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureSpecChoice {
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureOptions {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub runway_transitions: Vec<String>,
    pub enroute_transitions: Vec<String>,
    pub has_common_segment: bool,
    pub valid_choices: Vec<ProcedureSpecChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterializedProcedure {
    pub procedure: crate::planning::ProcedureSegment,
    pub concretized_items: Vec<ConcretizedNavItem>,
    pub resolved_legs: Vec<ResolvedLeg>,
}
