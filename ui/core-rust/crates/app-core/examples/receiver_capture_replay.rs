// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validate an app-exported private capture. Print aggregate counts only.
use app_core::{
    receiver::{
        capture_replay::{replay_export, ReplayEvent},
        decode_product,
        live::LiveInput,
        ApplicationRecord, Product, Weather, WeatherKind,
    },
    weather_sources::{StationReport, StationReportKind, StationWeather, WeatherSource},
};
use std::{collections::BTreeMap, fs::File};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: receiver_capture_replay PRIVATE-receiver-capture.zip")?;
    let mut live = LiveInput::default();
    let mut weather = StationWeather::default();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut failures = BTreeMap::<String, u64>::new();
    let summary = replay_export(File::open(path)?, |clock, event| {
        match event {
            ReplayEvent::Connected => {
                // Includes captures from different boots. Never order their
                // monotonic sensor clocks against one another.
                live = LiveInput::default();
                live.set_connected(true);
            }
            ReplayEvent::Disconnected => {
                live.set_connected(false);
            }
            ReplayEvent::Message(ApplicationRecord::Message {
                message_id,
                attributes,
                data,
            }) => {
                if attributes != 0 {
                    *counts.entry("uninterpreted_attributes".into()).or_default() += 1;
                    return;
                }
                let product = match decode_product(message_id, &data) {
                    Ok(product) => product,
                    Err(error) => {
                        *failures.entry(error.to_string()).or_default() += 1;
                        return;
                    }
                };
                let label = match product {
                    Product::Traffic(traffic) => {
                        live.ingest(&traffic, clock);
                        if live.snapshot().ownship_at(clock).is_some() {
                            *counts.entry("current_ownship".into()).or_default() += 1;
                        }
                        "traffic"
                    }
                    Product::Nmea { sentence } => {
                        live.ingest_nmea(&sentence, clock);
                        "nmea"
                    }
                    Product::Weather(Weather::Radar(radar)) => {
                        if let Err(error) = live.ingest_radar(&radar, clock) {
                            *failures.entry(error.into()).or_default() += 1;
                        }
                        "radar"
                    }
                    Product::Weather(Weather::Text { kind, reports, .. }) => {
                        let report_kind = match kind {
                            WeatherKind::Metar => Some(StationReportKind::Metar),
                            WeatherKind::Taf => Some(StationReportKind::Taf),
                            _ => None,
                        };
                        if let Some(kind) = report_kind {
                            for report in reports {
                                let result =
                                    chrono::DateTime::from_timestamp_millis(clock.wall_epoch_ms)
                                        .ok_or("invalid recorded receipt time")
                                        .and_then(|time| {
                                            StationReport::from_receiver(kind, &report, time)
                                        })
                                        .and_then(|report| {
                                            weather.ingest(WeatherSource::Receiver, report)
                                        });
                                match result {
                                    Ok(_) => {
                                        *counts
                                            .entry(format!("normalized_{kind:?}"))
                                            .or_default() += 1
                                    }
                                    Err(error) => *failures.entry(error.into()).or_default() += 1,
                                }
                            }
                        }
                        "text_weather"
                    }
                    Product::Attitude(_) => "attitude",
                    Product::PressureUnverified { .. } => "pressure_unverified",
                    Product::AttitudeSource { .. } => "attitude_source",
                    Product::GpsSource { .. } => "gps_source",
                    Product::Weather(_) => "other_weather",
                    Product::Uninterpreted { .. } => "uninterpreted",
                };
                *counts.entry(label.into()).or_default() += 1;
            }
            ReplayEvent::Message(ApplicationRecord::Control { .. }) => unreachable!(),
        }
    })?;
    let snapshot = weather.snapshot()?;
    if StationWeather::restore(&snapshot)?.snapshot()? != snapshot {
        return Err("weather cache restoration changed records".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"capture":summary,"products":counts,"failures":failures})
        )?
    );
    if !failures.is_empty() {
        return Err("capture contains product failures".into());
    }
    Ok(())
}
