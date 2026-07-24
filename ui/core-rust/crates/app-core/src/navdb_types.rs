// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;
use crate::map_overlay::NavSymbolFeature;
use crate::planning::{ConcretizedNavItem, NavRef, ProcedureKind, ResolvedLeg};

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
pub struct AirwaySpatialPoint {
    pub airway_name: String,
    pub branch_key: String,
    pub sequence: i32,
    pub position: LatLon,
    pub nav_ref: NavRef,
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
    pub distance_text: String,
    pub symbol_feature: Option<NavSymbolFeature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaypointIdentifierRecord {
    pub identifier: String,
    pub kind: String,
    pub display_name: String,
    pub lat: f64,
    pub lon: f64,
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
pub struct ProcedureSummary {
    pub airport_id: String,
    pub procedure_id: String,
    pub display_label: String,
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
    #[serde(default)]
    pub data_quality: Vec<String>,
}
