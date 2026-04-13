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
    pub path_termination: String,
    pub path_termination_kind: PathTermination,
}

#[derive(Serialize, Deserialize)]
struct ProcedureLegMaterializationRecordSerde {
    key: ProcedureVariantKey,
    sequence: i32,
    nav_ref: Option<NavRef>,
    path_termination: String,
    #[serde(default)]
    path_termination_kind: Option<PathTermination>,
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
            path_termination: self.path_termination.clone(),
            path_termination_kind: None,
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
            path_termination_kind: raw
                .path_termination_kind
                .unwrap_or_else(|| interpret_path_termination(&raw.path_termination)),
            path_termination: raw.path_termination,
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
