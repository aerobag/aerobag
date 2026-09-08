// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

pub const AIRWAY_ROUTING_MANIFEST_KEY: &str = "airway/routing/manifest";
pub const AIRWAY_ROUTING_SCHEMA_VERSION: u32 = 1;
pub const AIRWAY_ROUTING_CHUNK_SIZE: usize = 128;

pub fn airway_routing_chunk_key(index: u32) -> String {
    format!("airway/routing/chunk/{index:05}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirwayRoutingManifest {
    pub schema_version: u32,
    pub chunk_count: u32,
    pub node_count: u32,
    pub edge_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirwayRoutingChunk {
    pub schema_version: u32,
    pub nodes: Vec<AirwayRoutingNode>,
}

/// Coordinate-only points connect published segments, but are not drag targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AirwayRoutingReference {
    Airport(String),
    Navaid(String),
    Fix(String),
    LatLon { lat: f64, lon: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirwayRoutingNode {
    pub id: u32,
    pub nav_ref: AirwayRoutingReference,
    pub lat: f64,
    pub lon: f64,
    pub edges: Vec<AirwayRoutingEdge>,
}

/// One directed published segment; its altitudes apply in this direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirwayRoutingEdge {
    pub to: u32,
    pub airway_name: String,
    pub branch_key: String,
    pub from_sequence: i32,
    pub to_sequence: i32,
    pub distance_nm: f64,
    pub mea_ft: Option<u32>,
    pub gnss_mea_ft: Option<u32>,
    pub maximum_altitude_ft: Option<u32>,
    pub crossing_altitude_ft: Option<u32>,
    pub crossing_point: String,
    pub signal_gap: bool,
}
