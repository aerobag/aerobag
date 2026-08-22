// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

pub const AIRCRAFT_DEFINITION_SCHEMA_VERSION: u32 = 2;
pub const AIRCRAFT_DEFINITION_KEY_PREFIX: &str = "aircraft/definition/";
pub const DEFAULT_AIRCRAFT_DEFINITION_HASH: &str =
    "d3bb0ffe0b906f2adc10f0e1aecb97ac449f65a8b081f0502cd547a25cc9fc9b";
pub const DEFAULT_AIRCRAFT_PROFILE_ID: &str = "normal-cruise";
pub const MAX_AIRCRAFT_DEFINITION_JSON_BYTES: usize = 64 * 1024;
const MAX_AIRCRAFT_TEXT_BYTES: usize = 160;
const MAX_AIRCRAFT_SOURCE_BYTES: usize = 4 * 1024;
const MAX_AIRCRAFT_PROFILES: usize = 16;
const MAX_AIRCRAFT_PERFORMANCE_POINTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftDefinition {
    pub schema_version: u32,
    pub lineage_id: String,
    pub manufacturer: String,
    pub model: String,
    pub label: String,
    pub plan_view_path: String,
    pub default_profile_id: String,
    pub profiles: Vec<AircraftPerformanceProfileDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftPerformanceProfileDefinition {
    pub id: String,
    pub label: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_weight_lb: Option<f64>,
    pub cruise: CruisePerformanceModel,
    pub climb: ClimbPerformanceModel,
    pub descent: DescentPerformanceModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum CruisePerformanceModel {
    PressureAltitudeTable {
        points: Vec<AircraftCruisePerformancePoint>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClimbPerformanceModel {
    ConstantIasRate {
        ias_kt: f64,
        fuel_flow_gph: f64,
        vertical_speed_fpm: f64,
    },
    PressureAltitudeTable {
        points: Vec<AircraftVerticalPerformancePoint>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum DescentPerformanceModel {
    CruiseOffset {
        tas_offset_kt: f64,
        vertical_speed_fpm: f64,
    },
    PressureAltitudeTable {
        points: Vec<AircraftVerticalPerformancePoint>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftCruisePerformancePoint {
    pub pressure_altitude_ft: f64,
    pub true_airspeed_kt: f64,
    pub fuel_flow_gph: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AircraftPerformanceAirspeedBasis {
    Indicated,
    True,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftVerticalPerformancePoint {
    pub pressure_altitude_ft: f64,
    pub airspeed_basis: AircraftPerformanceAirspeedBasis,
    pub airspeed_kt: f64,
    pub fuel_flow_gph: f64,
    pub vertical_speed_fpm: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftSelection {
    pub definition_hash: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftLibraryMembership {
    pub included: bool,
}

impl<'de> Deserialize<'de> for AircraftLibraryMembership {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireMembership {
            included: bool,
            // Normalize the short-lived development tombstone contract to hidden.
            #[serde(default)]
            deleted: bool,
        }

        let wire = WireMembership::deserialize(deserializer)?;
        let _ = wire.deleted;
        Ok(Self {
            included: wire.included,
        })
    }
}

pub fn default_aircraft_selection() -> AircraftSelection {
    AircraftSelection {
        definition_hash: DEFAULT_AIRCRAFT_DEFINITION_HASH.to_string(),
        profile_id: DEFAULT_AIRCRAFT_PROFILE_ID.to_string(),
    }
}

impl AircraftDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != AIRCRAFT_DEFINITION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported aircraft definition schema {}",
                self.schema_version
            ));
        }
        require_text("lineage_id", &self.lineage_id)?;
        require_text("manufacturer", &self.manufacturer)?;
        require_text("model", &self.model)?;
        require_text("label", &self.label)?;
        validate_plan_view_path(&self.plan_view_path)?;
        require_text("default_profile_id", &self.default_profile_id)?;
        if self.profiles.is_empty() {
            return Err("aircraft definition has no performance profiles".to_string());
        }
        if self.profiles.len() > MAX_AIRCRAFT_PROFILES {
            return Err(format!(
                "aircraft definition has more than {MAX_AIRCRAFT_PROFILES} performance profiles"
            ));
        }

        let mut profile_ids = BTreeSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !profile_ids.insert(profile.id.as_str()) {
                return Err(format!("duplicate aircraft profile id {}", profile.id));
            }
        }
        if !profile_ids.contains(self.default_profile_id.as_str()) {
            return Err(format!(
                "default aircraft profile {} does not exist",
                self.default_profile_id
            ));
        }
        let mut superseded_hashes = BTreeSet::new();
        for hash in &self.supersedes {
            validate_aircraft_definition_hash(hash)?;
            if !superseded_hashes.insert(hash) {
                return Err(format!(
                    "duplicate superseded aircraft definition hash {hash}"
                ));
            }
        }
        Ok(())
    }

    /// Hashes the normalized typed representation rather than source JSON formatting.
    pub fn content_hash(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("failed to encode aircraft definition: {error}"))?;
        Ok(hex_sha256(&bytes))
    }

    pub fn profile(&self, profile_id: &str) -> Option<&AircraftPerformanceProfileDefinition> {
        self.profiles
            .iter()
            .find(|profile| profile.id == profile_id)
    }
}

impl AircraftPerformanceProfileDefinition {
    fn validate(&self) -> Result<(), String> {
        require_text("profile.id", &self.id)?;
        require_text("profile.label", &self.label)?;
        require_bounded_text("profile.source", &self.source, MAX_AIRCRAFT_SOURCE_BYTES)?;
        if self
            .reference_weight_lb
            .is_some_and(|weight| !weight.is_finite() || weight <= 0.0)
        {
            return Err(format!("profile {} has invalid reference weight", self.id));
        }
        let cruise_points = match &self.cruise {
            CruisePerformanceModel::PressureAltitudeTable { points } => points,
        };
        validate_cruise_points(&self.id, cruise_points)?;
        match &self.climb {
            ClimbPerformanceModel::ConstantIasRate {
                ias_kt,
                fuel_flow_gph,
                vertical_speed_fpm,
            } => validate_constant_vertical_model(
                &self.id,
                "climb",
                *ias_kt,
                *fuel_flow_gph,
                *vertical_speed_fpm,
                true,
            )?,
            ClimbPerformanceModel::PressureAltitudeTable { points } => {
                validate_vertical_points(&self.id, "climb", points, true)?
            }
        }
        match &self.descent {
            DescentPerformanceModel::CruiseOffset {
                tas_offset_kt,
                vertical_speed_fpm,
            } => {
                if !tas_offset_kt.is_finite()
                    || !vertical_speed_fpm.is_finite()
                    || *vertical_speed_fpm >= 0.0
                {
                    return Err(format!("profile {} has invalid descent model", self.id));
                }
            }
            DescentPerformanceModel::PressureAltitudeTable { points } => {
                validate_vertical_points(&self.id, "descent", points, false)?
            }
        }
        Ok(())
    }
}

pub fn aircraft_definition_key(hash: &str) -> Result<String, String> {
    validate_aircraft_definition_hash(hash)?;
    Ok(format!("{AIRCRAFT_DEFINITION_KEY_PREFIX}{hash}"))
}

pub fn validate_aircraft_definition_hash(hash: &str) -> Result<(), String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("invalid aircraft definition SHA-256: {hash}"));
    }
    Ok(())
}

fn require_text(field: &str, value: &str) -> Result<(), String> {
    require_bounded_text(field, value, MAX_AIRCRAFT_TEXT_BYTES)
}

fn require_bounded_text(field: &str, value: &str, maximum_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("aircraft definition {field} is empty"))
    } else if value.len() > maximum_bytes {
        Err(format!(
            "aircraft definition {field} exceeds {maximum_bytes} bytes"
        ))
    } else {
        Ok(())
    }
}

fn validate_plan_view_path(path: &str) -> Result<(), String> {
    const MAX_PATH_BYTES: usize = 16 * 1024;
    if path.len() > MAX_PATH_BYTES {
        return Err("aircraft definition plan_view_path is too large".to_string());
    }
    let tokens = path.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.first() != Some(&"M") || tokens.last() != Some(&"Z") {
        return Err(
            "aircraft definition plan_view_path must start with M and end with Z".to_string(),
        );
    }
    let mut index = 0usize;
    let mut command_count = 0usize;
    let mut minimum_x = f64::INFINITY;
    let mut maximum_x = f64::NEG_INFINITY;
    let mut minimum_y = f64::INFINITY;
    let mut maximum_y = f64::NEG_INFINITY;
    while index < tokens.len() {
        let command = tokens[index];
        index += 1;
        let coordinate_count = match command {
            "M" | "L" => 2,
            "C" => 6,
            "Z" => 0,
            _ => {
                return Err(format!(
                    "aircraft definition plan_view_path has unsupported command {command}"
                ))
            }
        };
        if command == "M" && command_count != 0 {
            return Err("aircraft definition plan_view_path must contain one outline".to_string());
        }
        if command == "Z" && index != tokens.len() {
            return Err("aircraft definition plan_view_path must end after Z".to_string());
        }
        if index + coordinate_count > tokens.len() {
            return Err(format!(
                "aircraft definition plan_view_path command {command} is incomplete"
            ));
        }
        for coordinate_index in 0..coordinate_count {
            let token = tokens[index + coordinate_index];
            let coordinate = token.parse::<f64>().map_err(|_| {
                format!("aircraft definition plan_view_path has invalid coordinate {token}")
            })?;
            if !coordinate.is_finite() {
                return Err(
                    "aircraft definition plan_view_path has a non-finite coordinate".to_string(),
                );
            }
            if token
                .split_once('.')
                .is_some_and(|(_, fraction)| fraction.len() > 3)
            {
                return Err("aircraft definition plan_view_path coordinates must use at most three decimal places".to_string());
            }
            if coordinate_index % 2 == 0 {
                minimum_x = minimum_x.min(coordinate);
                maximum_x = maximum_x.max(coordinate);
            } else {
                minimum_y = minimum_y.min(coordinate);
                maximum_y = maximum_y.max(coordinate);
            }
        }
        index += coordinate_count;
        command_count += 1;
    }
    if command_count < 3 {
        return Err("aircraft definition plan_view_path has no usable outline".to_string());
    }
    if minimum_x < -50.01 || maximum_x > 50.01 || minimum_x > -49.0 || maximum_x < 49.0 {
        return Err(
            "aircraft definition plan_view_path must normalize its wingspan to X=-50..50"
                .to_string(),
        );
    }
    if minimum_y >= 0.0 || maximum_y <= 0.0 {
        return Err(
            "aircraft definition plan_view_path must surround its rotation center".to_string(),
        );
    }
    Ok(())
}

fn validate_cruise_points(
    profile_id: &str,
    points: &[AircraftCruisePerformancePoint],
) -> Result<(), String> {
    if points.len() < 2 {
        return Err(format!(
            "profile {profile_id} cruise table needs at least two points"
        ));
    }
    if points.len() > MAX_AIRCRAFT_PERFORMANCE_POINTS {
        return Err(format!(
            "profile {profile_id} cruise table exceeds {MAX_AIRCRAFT_PERFORMANCE_POINTS} points"
        ));
    }
    validate_altitudes(
        profile_id,
        "cruise",
        points.iter().map(|point| point.pressure_altitude_ft),
    )?;
    if points.iter().any(|point| {
        !point.true_airspeed_kt.is_finite()
            || point.true_airspeed_kt <= 0.0
            || !point.fuel_flow_gph.is_finite()
            || point.fuel_flow_gph <= 0.0
    }) {
        return Err(format!(
            "profile {profile_id} cruise table has invalid performance"
        ));
    }
    Ok(())
}

fn validate_vertical_points(
    profile_id: &str,
    phase: &str,
    points: &[AircraftVerticalPerformancePoint],
    climbing: bool,
) -> Result<(), String> {
    if points.len() < 2 {
        return Err(format!(
            "profile {profile_id} {phase} table needs at least two points"
        ));
    }
    if points.len() > MAX_AIRCRAFT_PERFORMANCE_POINTS {
        return Err(format!(
            "profile {profile_id} {phase} table exceeds {MAX_AIRCRAFT_PERFORMANCE_POINTS} points"
        ));
    }
    validate_altitudes(
        profile_id,
        phase,
        points.iter().map(|point| point.pressure_altitude_ft),
    )?;
    if points.iter().any(|point| {
        !point.airspeed_kt.is_finite()
            || point.airspeed_kt <= 0.0
            || !point.fuel_flow_gph.is_finite()
            || point.fuel_flow_gph <= 0.0
            || !point.vertical_speed_fpm.is_finite()
            || if climbing {
                point.vertical_speed_fpm <= 0.0
            } else {
                point.vertical_speed_fpm >= 0.0
            }
    }) {
        return Err(format!(
            "profile {profile_id} {phase} table has invalid performance"
        ));
    }
    Ok(())
}

fn validate_constant_vertical_model(
    profile_id: &str,
    phase: &str,
    airspeed_kt: f64,
    fuel_flow_gph: f64,
    vertical_speed_fpm: f64,
    climbing: bool,
) -> Result<(), String> {
    if !airspeed_kt.is_finite()
        || airspeed_kt <= 0.0
        || !fuel_flow_gph.is_finite()
        || fuel_flow_gph <= 0.0
        || !vertical_speed_fpm.is_finite()
        || if climbing {
            vertical_speed_fpm <= 0.0
        } else {
            vertical_speed_fpm >= 0.0
        }
    {
        return Err(format!(
            "profile {profile_id} has invalid constant {phase} model"
        ));
    }
    Ok(())
}

fn validate_altitudes(
    profile_id: &str,
    phase: &str,
    altitudes: impl Iterator<Item = f64>,
) -> Result<(), String> {
    let mut previous = None;
    for altitude in altitudes {
        if !altitude.is_finite()
            || altitude < 0.0
            || previous.is_some_and(|previous| altitude <= previous)
        {
            return Err(format!(
                "profile {profile_id} {phase} altitudes are not strictly increasing"
            ));
        }
        previous = Some(altitude);
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> AircraftDefinition {
        AircraftDefinition {
            schema_version: AIRCRAFT_DEFINITION_SCHEMA_VERSION,
            lineage_id: "test-aircraft".to_string(),
            manufacturer: "Test".to_string(),
            model: "T1".to_string(),
            label: "TEST T1".to_string(),
            plan_view_path: "M -50 0 L 0 -20 L 50 0 L 0 20 Z".to_string(),
            default_profile_id: "normal".to_string(),
            profiles: vec![AircraftPerformanceProfileDefinition {
                id: "normal".to_string(),
                label: "NORMAL".to_string(),
                source: "test fixture".to_string(),
                reference_weight_lb: Some(2_000.0),
                cruise: CruisePerformanceModel::PressureAltitudeTable {
                    points: vec![
                        AircraftCruisePerformancePoint {
                            pressure_altitude_ft: 0.0,
                            true_airspeed_kt: 100.0,
                            fuel_flow_gph: 8.0,
                        },
                        AircraftCruisePerformancePoint {
                            pressure_altitude_ft: 10_000.0,
                            true_airspeed_kt: 110.0,
                            fuel_flow_gph: 8.0,
                        },
                    ],
                },
                climb: ClimbPerformanceModel::ConstantIasRate {
                    ias_kt: 75.0,
                    fuel_flow_gph: 10.0,
                    vertical_speed_fpm: 500.0,
                },
                descent: DescentPerformanceModel::CruiseOffset {
                    tas_offset_kt: 5.0,
                    vertical_speed_fpm: -500.0,
                },
            }],
            supersedes: Vec::new(),
        }
    }

    #[test]
    fn content_hash_is_stable_across_json_formatting() {
        let definition = definition();
        let pretty = serde_json::to_string_pretty(&definition).unwrap();
        let reparsed: AircraftDefinition = serde_json::from_str(&pretty).unwrap();
        assert_eq!(definition.content_hash(), reparsed.content_hash());
    }

    #[test]
    fn definition_hashes_have_one_canonical_lowercase_spelling() {
        assert!(validate_aircraft_definition_hash(&"a0".repeat(32)).is_ok());
        assert!(validate_aircraft_definition_hash(&"A0".repeat(32)).is_err());
    }

    #[test]
    fn default_profile_must_exist() {
        let mut definition = definition();
        definition.default_profile_id = "missing".to_string();
        assert!(definition
            .validate()
            .unwrap_err()
            .contains("does not exist"));
    }

    #[test]
    fn plan_view_path_accepts_only_normalized_absolute_outline_commands() {
        let mut relative = definition();
        relative.plan_view_path = "m -50 0 l 100 0 z".to_string();
        assert!(relative.validate().unwrap_err().contains("start with M"));

        let mut excess_precision = definition();
        excess_precision.plan_view_path = "M -50.0001 0 L 0 -20 L 50 0 L 0 20 Z".to_string();
        assert!(excess_precision
            .validate()
            .unwrap_err()
            .contains("three decimal places"));

        let mut unnormalized = definition();
        unnormalized.plan_view_path = "M -40 0 L 0 -20 L 40 0 L 0 20 Z".to_string();
        assert!(unnormalized
            .validate()
            .unwrap_err()
            .contains("normalize its wingspan"));
    }

    #[test]
    fn superseded_definition_hashes_must_be_unique() {
        let mut definition = definition();
        let predecessor = "1".repeat(64);
        definition.supersedes = vec![predecessor.clone(), predecessor];
        assert!(definition
            .validate()
            .unwrap_err()
            .contains("duplicate superseded"));
    }

    #[test]
    fn development_tombstones_normalize_to_hidden_membership() {
        let membership: AircraftLibraryMembership =
            serde_json::from_str(r#"{"included":false,"deleted":true}"#).unwrap();
        assert_eq!(membership, AircraftLibraryMembership { included: false });
        assert_eq!(
            serde_json::to_string(&membership).unwrap(),
            r#"{"included":false}"#,
        );
    }
}
