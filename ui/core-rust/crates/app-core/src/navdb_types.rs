// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;
use crate::map_overlay::NavSymbolFeature;
use crate::planning::{NavRef, ProcedureKind, ResolvedLeg};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AirwayFixPoint {
    pub(crate) airway_name: String,
    pub(crate) sequence: i32,
    pub(crate) position: LatLon,
    pub(crate) nav_ref: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AirwayBranch {
    pub(crate) display_name: String,
    pub(crate) branch_key: String,
    pub(crate) points: Vec<AirwayFixPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AirwaySpatialPoint {
    pub(crate) airway_name: String,
    pub(crate) branch_key: String,
    pub(crate) sequence: i32,
    pub(crate) position: LatLon,
    pub(crate) nav_ref: NavRef,
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
pub(crate) struct AirwayEntryCandidate {
    pub(crate) airway_name: String,
    pub(crate) branch_key: String,
    pub(crate) branch_point_index: usize,
    pub(crate) sequence: i32,
    pub(crate) nav_ref: NavRef,
    pub(crate) distance_from_anchor_nm: f64,
    pub(crate) previous_nav_ref: Option<NavRef>,
    pub(crate) next_nav_ref: Option<NavRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AirwayExitCandidate {
    pub(crate) airway_name: String,
    pub(crate) branch_key: String,
    pub(crate) branch_point_index: usize,
    pub(crate) sequence: i32,
    pub(crate) nav_ref: NavRef,
    pub(crate) leg_offset_from_entry: isize,
    pub(crate) is_entry: bool,
    pub(crate) distance_from_target_nm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPresentationPoint {
    pub uid: String,
    pub sequence: i32,
    pub nav_ref: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPresentationPlan {
    pub airway_name: String,
    pub branch_key: String,
    pub points: Vec<AirwayPresentationPoint>,
    pub suggested_entry_uid: String,
    pub suggested_exit_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirwayPresentationSelection {
    pub airway_name: String,
    pub branch_key: String,
    pub entry_point_uid: String,
    pub exit_point_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureSummary {
    pub airport_id: String,
    pub procedure_id: String,
    pub display_label: String,
    pub kind: ProcedureKind,
    pub accent_category: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CifpTppMatchRow {
    pub airport_id: String,
    pub cifp_id: String,
    pub procedure_kind: ProcedureKind,
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
    pub procedure_kind: ProcedureKind,
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
    pub resolved_legs: Vec<ResolvedLeg>,
}
