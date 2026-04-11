use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Situation {
    pub position: SituationPosition,
    pub orientation_deg: Option<f64>,
    pub speed_kt: Option<f64>,
}

impl Default for Situation {
    fn default() -> Self {
        Self {
            position: SituationPosition::Unknown,
            orientation_deg: None,
            speed_kt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SituationPosition {
    Unknown,
    LatLon { lat: f64, lon: f64 },
    FlightPlanLocation { leg_index: usize, lat: f64, lon: f64 },
}

impl SituationPosition {
    pub fn lat_lon(&self) -> Option<LatLon> {
        match self {
            SituationPosition::Unknown => None,
            SituationPosition::LatLon { lat, lon }
            | SituationPosition::FlightPlanLocation { lat, lon, .. } => Some(LatLon {
                lat: *lat,
                lon: *lon,
            }),
        }
    }
}
