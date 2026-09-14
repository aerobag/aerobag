// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

pub const AIRWAY_ROUTING_GRAPH_KEY: &str = "airway/routing/graph";
pub const AIRWAY_ROUTING_SCHEMA_VERSION: u32 = 2;
pub const AIRWAY_ROUTING_MAX_BYTES: usize = 32 * 1024 * 1024;

/// One positional Postcard message. Shape changes require a NAVDB contract bump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirwayRoutingGraph {
    pub schema_version: u32,
    pub nodes: Vec<AirwayRoutingNode>,
}

impl AirwayRoutingGraph {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != AIRWAY_ROUTING_SCHEMA_VERSION || self.nodes.len() > 200_000 {
            return Err("Unsupported airway routing graph.".into());
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.id as usize != index
                || !valid_position(node.lat, node.lon)
                || matches!(node.nav_ref, AirwayRoutingReference::LatLon { lat, lon } if !valid_position(lat, lon))
                || node.edges.iter().any(|edge| {
                    edge.to as usize >= self.nodes.len()
                        || !edge.distance_nm.is_finite()
                        || edge.distance_nm <= 0.0
                })
            {
                return Err("Invalid airway routing graph.".into());
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self).map_err(|e| e.to_string())?;
        if bytes.len() > AIRWAY_ROUTING_MAX_BYTES {
            return Err("Airway routing graph exceeds size limit.".into());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > AIRWAY_ROUTING_MAX_BYTES {
            return Err("Airway routing graph exceeds size limit.".into());
        }
        let (version, _) = postcard::take_from_bytes::<u32>(bytes).map_err(|e| e.to_string())?;
        if version != AIRWAY_ROUTING_SCHEMA_VERSION {
            return Err(format!("Unsupported airway routing schema {version}."));
        }
        let (graph, remaining) =
            postcard::take_from_bytes::<Self>(bytes).map_err(|e| e.to_string())?;
        if !remaining.is_empty() {
            return Err("Trailing bytes in airway routing graph.".into());
        }
        graph.validate()?;
        Ok(graph)
    }
}

fn valid_position(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_graph_rejects_unknown_schema_trailing_bytes_and_truncation() {
        let graph = AirwayRoutingGraph {
            schema_version: AIRWAY_ROUTING_SCHEMA_VERSION,
            nodes: Vec::new(),
        };
        let mut bytes = graph.encode().unwrap();
        assert_eq!(AirwayRoutingGraph::decode(&bytes).unwrap(), graph);
        assert!(AirwayRoutingGraph::decode(&bytes[..1]).is_err());
        bytes.push(0);
        assert!(AirwayRoutingGraph::decode(&bytes).is_err());
        assert!(AirwayRoutingGraph::decode(&[99])
            .unwrap_err()
            .contains("schema 99"));
    }
}
