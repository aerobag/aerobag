// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WEATHER_STATION_CATALOG_KEY: &str = "weather/station-catalog";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherStationCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

/// Exact station coordinates from NASR AWOS, never an associated airport's ARP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherStationCatalog {
    pub schema_version: u32,
    pub stations: BTreeMap<String, WeatherStationCoordinates>,
}

impl WeatherStationCatalog {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != Self::SCHEMA_VERSION || self.stations.len() > 65536 {
            return Err("Unsupported or oversized weather station catalog".into());
        }
        for (id, position) in &self.stations {
            if !(2..=16).contains(&id.len())
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                || !(-90.0..=90.0).contains(&position.latitude)
                || !(-180.0..=180.0).contains(&position.longitude)
            {
                return Err(format!("Invalid weather station catalog entry {id:?}"));
            }
        }
        Ok(())
    }
}
