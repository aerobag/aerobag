use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;
use crate::ids::AirportId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlan {
    pub id: String,
    pub name: String,
    pub legs: Vec<PlanLeg>,
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
pub enum NavRef {
    Airport(String),
    Navaid(String),
    Fix(String),
    LatLon(LatLon),
}

impl NavRef {
    pub fn airport_code(&self) -> Option<&str> {
        match self {
            NavRef::Airport(code) => Some(code.as_str()),
            _ => None,
        }
    }
}
