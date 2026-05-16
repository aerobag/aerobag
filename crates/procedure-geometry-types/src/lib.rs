use had_key::upper_component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProcedureLatLon {
    pub lat: f64,
    pub lon: f64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedurePathTermination {
    InitialFix,
    TrackToFix,
    CourseToFix,
    DirectToFix,
    HeadingToManual,
    HeadingToAltitude,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProcedureNavRef {
    Airport(String),
    Navaid(String),
    Fix(String),
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
    LatLon(ProcedureLatLon),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureGeometryChoice {
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureGeometryKey {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureGeometryElement {
    Segment {
        start: ProcedureLatLon,
        end: ProcedureLatLon,
    },
    Arc {
        center: ProcedureLatLon,
        radius_nm: f64,
        start: ProcedureLatLon,
        end: ProcedureLatLon,
        clockwise: bool,
        sweep_degrees: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureGeometryPathStyle {
    Solid,
    Dashed,
}

impl Default for ProcedureGeometryPathStyle {
    fn default() -> Self {
        Self::Solid
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryPath {
    #[serde(default)]
    pub style: ProcedureGeometryPathStyle,
    pub elements: Vec<ProcedureGeometryElement>,
    #[serde(default)]
    pub effective_terminal_course_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureSequencingRule {
    Continue,
    Suspend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryWaypoint {
    pub nav_ref: ProcedureNavRef,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryLegBundle {
    pub id: String,
    pub role: ProcedureSegmentRole,
    pub from: ProcedureNavRef,
    pub to: ProcedureNavRef,
    pub path_termination: ProcedurePathTermination,
    pub leg_sequence: i32,
    pub path: ProcedureGeometryPath,
    #[serde(default)]
    pub waypoints: Vec<ProcedureGeometryWaypoint>,
    pub sequencing_after: ProcedureSequencingRule,
    #[serde(default)]
    pub source_row_sequences: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureDataQualityAnnotation {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryRecord {
    pub key: ProcedureGeometryKey,
    #[serde(default)]
    pub terminal_discontinuity: Option<ProcedureDiscontinuity>,
    pub leg_bundles: Vec<ProcedureGeometryLegBundle>,
    #[serde(default)]
    pub data_quality: Vec<ProcedureDataQualityAnnotation>,
}

pub fn procedure_geometry_navdb_key(key: &ProcedureGeometryKey) -> String {
    format!(
        "procedure/geometry/{}/{}/{}/{}/{}",
        upper_component(&key.airport_id),
        procedure_kind_component(&key.kind),
        upper_component(&key.procedure_id),
        optional_transition_component(key.runway_transition.as_deref()),
        optional_transition_component(key.enroute_transition.as_deref())
    )
}

pub fn procedure_kind_component(kind: &ProcedureKind) -> &'static str {
    match kind {
        ProcedureKind::Sid => "SID",
        ProcedureKind::Star => "STAR",
        ProcedureKind::Approach => "APPROACH",
    }
}

fn optional_transition_component(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(upper_component)
        .unwrap_or_else(|| "_".to_string())
}
