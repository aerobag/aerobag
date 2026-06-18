use had_key::upper_component;
use serde::{Deserialize, Serialize, Serializer};

const NAV_COORDINATE_DECIMAL_SCALE: f64 = 10_000_000.0;

fn round_nav_coordinate(value: f64) -> f64 {
    let rounded = (value * NAV_COORDINATE_DECIMAL_SCALE).round() / NAV_COORDINATE_DECIMAL_SCALE;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

fn serialize_nav_coordinate<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(round_nav_coordinate(*value))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProcedureLatLon {
    #[serde(serialize_with = "serialize_nav_coordinate")]
    pub lat: f64,
    #[serde(serialize_with = "serialize_nav_coordinate")]
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    Sid,
    Star,
    Approach,
}

impl Default for ProcedureKind {
    fn default() -> Self {
        Self::Approach
    }
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
    #[serde(default)]
    pub kind: ProcedureKind,
    #[serde(default)]
    pub runway_transition: Option<String>,
    #[serde(default)]
    pub enroute_transition: Option<String>,
}

impl Default for ProcedureGeometryKey {
    fn default() -> Self {
        Self {
            airport_id: String::new(),
            procedure_id: String::new(),
            kind: ProcedureKind::Approach,
            runway_transition: None,
            enroute_transition: None,
        }
    }
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
    #[serde(default, skip_serializing_if = "is_default")]
    pub style: ProcedureGeometryPathStyle,
    pub elements: Vec<ProcedureGeometryElement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_terminal_course_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureSequencingRule {
    Continue,
    Suspend,
}

impl Default for ProcedureSequencingRule {
    fn default() -> Self {
        Self::Continue
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryWaypoint {
    pub nav_ref: ProcedureNavRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryLegBundle {
    #[serde(default, skip_serializing)]
    pub id: String,
    pub role: ProcedureSegmentRole,
    pub from: ProcedureNavRef,
    pub to: ProcedureNavRef,
    pub path_termination: ProcedurePathTermination,
    #[serde(default, skip_serializing)]
    pub leg_sequence: i32,
    pub path: ProcedureGeometryPath,
    #[serde(default, skip_serializing)]
    pub waypoints: Vec<ProcedureGeometryWaypoint>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub sequencing_after: ProcedureSequencingRule,
    #[serde(default, skip_serializing)]
    pub source_row_sequences: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureDataQualityAnnotation {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcedureGeometryComponent {
    LegBundles {
        leg_bundles: Vec<ProcedureGeometryLegBundle>,
    },
    SegmentRef {
        segment_ref: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometrySegmentRecord {
    pub leg_bundles: Vec<ProcedureGeometryLegBundle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureGeometryRecord {
    #[serde(default, skip_serializing)]
    pub key: ProcedureGeometryKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_discontinuity: Option<ProcedureDiscontinuity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ProcedureGeometryComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leg_bundles: Vec<ProcedureGeometryLegBundle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_quality: Vec<ProcedureDataQualityAnnotation>,
}

pub fn derive_procedure_geometry_waypoints(
    leg_bundles: &[ProcedureGeometryLegBundle],
) -> Vec<ProcedureGeometryWaypoint> {
    let mut waypoints = Vec::new();
    for bundle in leg_bundles {
        if waypoints
            .last()
            .is_some_and(|waypoint: &ProcedureGeometryWaypoint| waypoint.nav_ref == bundle.to)
        {
            continue;
        }
        waypoints.push(ProcedureGeometryWaypoint {
            nav_ref: bundle.to.clone(),
            name: None,
        });
    }
    waypoints
}

pub fn populate_derived_procedure_geometry_fields(record: &mut ProcedureGeometryRecord) {
    for bundle in &mut record.leg_bundles {
        if bundle.waypoints.is_empty() {
            bundle.waypoints.push(ProcedureGeometryWaypoint {
                nav_ref: bundle.to.clone(),
                name: None,
            });
        }
    }
}

fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    value == &T::default()
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

pub fn procedure_geometry_segment_navdb_key(segment_ref: &str) -> String {
    format!(
        "procedure/geometry-segment/{}",
        upper_component(segment_ref)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_omits_geometry_defaults_and_derivable_fields() {
        let record = sample_record();
        let value = serde_json::to_value(&record).expect("serialize procedure geometry record");

        assert!(value.get("key").is_none());
        assert!(value.get("terminal_discontinuity").is_none());
        assert!(value.get("data_quality").is_none());

        let bundle = &value["leg_bundles"][0];
        assert!(bundle.get("id").is_none());
        assert!(bundle.get("leg_sequence").is_none());
        assert!(bundle.get("waypoints").is_none());
        assert!(bundle.get("sequencing_after").is_none());
        assert!(bundle.get("source_row_sequences").is_none());
        assert!(bundle["path"].get("style").is_none());
        assert!(bundle["path"]
            .get("effective_terminal_course_deg")
            .is_none());
    }

    #[test]
    fn omitted_geometry_fields_deserialize_to_runtime_defaults() {
        let json = serde_json::json!({
            "leg_bundles": [{
                "role": "common",
                "from": { "kind": "airport", "value": "KAAA" },
                "to": { "kind": "fix", "value": "FIXA" },
                "path_termination": "track_to_fix",
                "path": { "elements": [] }
            }]
        });

        let mut record: ProcedureGeometryRecord =
            serde_json::from_value(json).expect("deserialize omitted defaults");
        assert_eq!(record.key, ProcedureGeometryKey::default());
        assert_eq!(record.terminal_discontinuity, None);
        assert!(record.data_quality.is_empty());
        assert!(record.leg_bundles[0].id.is_empty());
        assert_eq!(record.leg_bundles[0].leg_sequence, 0);
        assert_eq!(
            record.leg_bundles[0].path.style,
            ProcedureGeometryPathStyle::Solid
        );
        assert_eq!(
            record.leg_bundles[0].sequencing_after,
            ProcedureSequencingRule::Continue
        );
        assert!(record.leg_bundles[0].source_row_sequences.is_empty());
        assert!(record.leg_bundles[0].waypoints.is_empty());

        populate_derived_procedure_geometry_fields(&mut record);
        assert_eq!(record.leg_bundles[0].waypoints.len(), 1);
        assert_eq!(
            record.leg_bundles[0].waypoints[0].nav_ref,
            ProcedureNavRef::Fix("FIXA".to_string())
        );
    }

    #[test]
    fn serde_omits_diagnostic_leg_provenance() {
        let mut record = sample_record();
        record.leg_bundles[0].source_row_sequences = vec![10, 20];

        let value = serde_json::to_value(&record).expect("serialize procedure geometry record");
        assert!(value["leg_bundles"][0]
            .get("source_row_sequences")
            .is_none());

        let old_payload = serde_json::json!({
            "leg_bundles": [{
                "id": "leg-1",
                "role": "common",
                "from": { "kind": "airport", "value": "KAAA" },
                "to": { "kind": "fix", "value": "FIXA" },
                "path_termination": "track_to_fix",
                "leg_sequence": 10,
                "path": { "elements": [] },
                "source_row_sequences": [10, 20]
            }]
        });
        let decoded: ProcedureGeometryRecord =
            serde_json::from_value(old_payload).expect("deserialize old provenance field");
        assert_eq!(decoded.leg_bundles[0].id, "leg-1");
        assert_eq!(decoded.leg_bundles[0].leg_sequence, 10);
        assert_eq!(decoded.leg_bundles[0].source_row_sequences, vec![10, 20]);
    }

    #[test]
    fn procedure_lat_lon_serializes_with_nav_coordinate_precision() {
        let value = serde_json::to_value(ProcedureLatLon {
            lat: 47.49313888888889,
            lon: -122.215750055,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "lat": 47.4931389,
                "lon": -122.2157501,
            })
        );
    }

    #[test]
    fn waypoint_derivation_matches_deduplicated_bundle_destinations() {
        let mut record = sample_record();
        record.leg_bundles.push(ProcedureGeometryLegBundle {
            id: "leg-2".to_string(),
            role: ProcedureSegmentRole::Common,
            from: ProcedureNavRef::Fix("FIXA".to_string()),
            to: ProcedureNavRef::Fix("FIXA".to_string()),
            path_termination: ProcedurePathTermination::TrackToFix,
            leg_sequence: 20,
            path: ProcedureGeometryPath {
                style: ProcedureGeometryPathStyle::Solid,
                elements: Vec::new(),
                effective_terminal_course_deg: None,
            },
            waypoints: vec![ProcedureGeometryWaypoint {
                nav_ref: ProcedureNavRef::Fix("FIXA".to_string()),
                name: None,
            }],
            sequencing_after: ProcedureSequencingRule::Continue,
            source_row_sequences: Vec::new(),
        });

        let derived = derive_procedure_geometry_waypoints(&record.leg_bundles);
        assert_eq!(
            derived,
            vec![ProcedureGeometryWaypoint {
                nav_ref: ProcedureNavRef::Fix("FIXA".to_string()),
                name: None,
            }]
        );
    }

    fn sample_record() -> ProcedureGeometryRecord {
        ProcedureGeometryRecord {
            key: ProcedureGeometryKey {
                airport_id: "KAAA".to_string(),
                procedure_id: "RNAV-A".to_string(),
                kind: ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition: Some("TRANS".to_string()),
            },
            terminal_discontinuity: None,
            components: Vec::new(),
            leg_bundles: vec![ProcedureGeometryLegBundle {
                id: "leg-1".to_string(),
                role: ProcedureSegmentRole::Common,
                from: ProcedureNavRef::Airport("KAAA".to_string()),
                to: ProcedureNavRef::Fix("FIXA".to_string()),
                path_termination: ProcedurePathTermination::TrackToFix,
                leg_sequence: 10,
                path: ProcedureGeometryPath {
                    style: ProcedureGeometryPathStyle::Solid,
                    elements: Vec::new(),
                    effective_terminal_course_deg: None,
                },
                waypoints: vec![ProcedureGeometryWaypoint {
                    nav_ref: ProcedureNavRef::Fix("FIXA".to_string()),
                    name: None,
                }],
                sequencing_after: ProcedureSequencingRule::Continue,
                source_row_sequences: Vec::new(),
            }],
            data_quality: Vec::new(),
        }
    }
}
