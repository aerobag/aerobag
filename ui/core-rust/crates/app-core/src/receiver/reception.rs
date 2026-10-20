// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::{Product, ReceivedMessage, WeatherKind};
use std::collections::BTreeMap;

/// Socket observations only. Restored weather and UI delivery never enter here.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Reception {
    attitude: u64,
    pressure: u64,
    traffic: u64,
    traffic_with_targets: u64,
    weather: BTreeMap<u32, WeatherReception>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
struct WeatherReception {
    empty: u64,
    nonempty: u64,
    malformed: u64,
    bytes: u64,
    last_ms: u64,
}

impl Reception {
    pub fn observe(&mut self, message: &ReceivedMessage, now_ms: u64) {
        match &message.product {
            Ok(Product::Attitude(_)) => self.attitude += 1,
            Ok(Product::PressureUnverified { .. }) => self.pressure += 1,
            Ok(Product::Traffic(traffic)) => {
                self.traffic += 1;
                self.traffic_with_targets += u64::from(!traffic.targets.is_empty());
            }
            _ => {}
        }
        if WeatherKind::from_message_id(message.message_id).is_some() {
            let entry = self.weather.entry(message.message_id).or_default();
            entry.empty += u64::from(message.payload_bytes == 0);
            entry.nonempty += u64::from(message.payload_bytes != 0);
            entry.malformed += u64::from(message.product.is_err());
            entry.bytes += message.payload_bytes as u64;
            entry.last_ms = now_ms;
        }
    }

    pub fn description(&self, now_ms: u64) -> String {
        let mut text = format!(
            "This connection only (cached data excluded): {} attitude, {} pressure; {} traffic snapshots, {} with targets.",
            self.attitude, self.pressure, self.traffic, self.traffic_with_targets,
        );
        if self.weather.is_empty() {
            text.push_str("\nWeather: no responses yet.");
        }
        for (id, entry) in &self.weather {
            let kind = WeatherKind::from_message_id(*id).expect("weather ID indexed on insertion");
            let label = match kind {
                WeatherKind::Metar => "METAR",
                WeatherKind::GraphicalMetar => "Graphical METAR",
                WeatherKind::Pirep => "PIREP",
                WeatherKind::Taf => "TAF",
                WeatherKind::Winds => "Winds",
                WeatherKind::Notam => "NOTAM",
                WeatherKind::Airmet => "AIRMET",
                WeatherKind::Sigmet => "SIGMET",
                WeatherKind::Sua => "SUA",
                WeatherKind::RegionalRadar => "Regional NEXRAD",
                WeatherKind::ConusRadar => "CONUS NEXRAD",
            };
            text.push_str(&format!(
                "\n{label}: {} nonempty / {} empty; {} bytes; last {}s ago; {} decode errors.",
                entry.nonempty,
                entry.empty,
                entry.bytes,
                now_ms.saturating_sub(entry.last_ms) / 1000,
                entry.malformed,
            ));
        }
        text
    }
}
