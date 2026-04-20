use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SituationPosition {
    None,
    LatLon { lat: f64, lon: f64 },
}

impl Default for SituationPosition {
    fn default() -> Self {
        Self::None
    }
}

impl SituationPosition {
    pub fn lat_lon(&self) -> Option<LatLon> {
        match self {
            Self::None => None,
            Self::LatLon { lat, lon } => Some(LatLon {
                lat: *lat,
                lon: *lon,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Situation {
    pub position: SituationPosition,
    pub orientation_deg: Option<f64>,
    pub speed_kt: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
}
