// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Private corpus validator. Prints aggregate counts only, never identities,
//! coordinates, report bodies, or authentication material.
use std::{collections::BTreeMap, io::BufRead, path::Path};

use app_core::receiver::{
    decode_product, ApplicationRecord, ApplicationStream, Frame, Framer, Product, Weather,
    WeatherKind,
};
use app_core::weather_sources::{StationReport, StationReportKind, StationWeather, WeatherSource};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};

fn hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 || !value.is_ascii() {
        return Err("invalid hex".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(Into::into))
        .collect()
}

// This is fixture clock calibration, not a navigation/NMEA input path. The
// Android HCI wall time in this particular archive is wrong by about seven hours.
fn capture_clock_anchor(path: &Path) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    let input = std::io::BufReader::new(std::fs::File::open(path)?);
    for line in input.lines() {
        let frame: serde_json::Value = serde_json::from_str(&line?)?;
        if frame["direction"] != "gtx_to_tablet" {
            continue;
        }
        let Some(records) = frame["application_records"].as_array() else {
            continue;
        };
        for record in records {
            if record["message_id"].as_u64() != Some(0x2003) {
                continue;
            }
            let bytes = hex(record["data_hex"].as_str().ok_or("missing NMEA data")?)?;
            let Product::Nmea { sentence, .. } = decode_product(0x2003, &bytes)? else {
                continue;
            };
            let fields = sentence.split(',').collect::<Vec<_>>();
            if fields.len() < 10 || fields[2] != "A" {
                continue;
            }
            let date = NaiveDateTime::parse_from_str(
                &format!("{} {}", fields[9], fields[1]),
                "%d%m%y %H%M%S%.f",
            )?
            .and_utc();
            return Ok(date - relative_time(&frame)?);
        }
    }
    Err("capture has no valid RMC clock anchor".into())
}

fn relative_time(frame: &serde_json::Value) -> Result<Duration, Box<dyn std::error::Error>> {
    let seconds = frame["time_relative"]
        .as_f64()
        .ok_or("missing relative time")?;
    if !seconds.is_finite() || !(0.0..=365.0 * 86_400.0).contains(&seconds) {
        return Err("capture relative time out of bounds".into());
    }
    Ok(Duration::milliseconds((seconds * 1_000.0).round() as i64))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: receiver_replay PRIVATE-flight-frames.jsonl")?;
    let anchor = capture_clock_anchor(Path::new(&path))?;
    let input = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut counts = BTreeMap::<String, usize>::new();
    let mut failures = BTreeMap::<String, usize>::new();
    let mut rejected_reports = BTreeMap::<String, usize>::new();
    let mut station_weather = StationWeather::default();
    let mut framer = Framer::default();
    let mut application = ApplicationStream::default();
    let mut previous = None;
    let mut live = app_core::receiver::live::LiveInput::default();
    live.set_connected(true);
    for line in input.lines() {
        let frame: serde_json::Value = serde_json::from_str(&line?)?;
        if frame["direction"] != "gtx_to_tablet" {
            continue;
        }
        let Some(records) = frame["application_records"].as_array() else {
            continue;
        };
        let fields = frame["header_fields_4_6"]
            .as_array()
            .ok_or("missing transport fields")?;
        let raw = Frame {
            kind: fields[0].as_u64().ok_or("transport kind")? as u8,
            sequence: fields[1].as_u64().ok_or("transport sequence")? as u8,
            acknowledgement: fields[2].as_u64().ok_or("transport acknowledgement")? as u8,
            payload: hex(frame["payload_hex"].as_str().ok_or("missing payload")?)?,
        }
        .encode()?;
        if frame["length"].as_u64() != Some(raw.len() as u64) {
            return Err("transport length differs from capture".into());
        }
        let mut actual = Vec::new();
        // Reconstruct framing from the captured payload and header fields, then
        // deliberately split socket reads. Original HCI checksums are verified
        // by the archive extractor, not by reconstructing them here.
        for chunk in raw.chunks(31) {
            for decoded in framer.feed(chunk)? {
                if decoded.kind == 3 {
                    application.reset();
                    previous = None;
                } else if decoded.kind == 2 && !decoded.payload.is_empty() {
                    let signature = (decoded.sequence, decoded.payload.clone());
                    if previous.as_ref() != Some(&signature) {
                        actual.extend(application.feed(&decoded.payload)?);
                    }
                    previous = Some(signature);
                }
            }
        }
        let expected = records
            .iter()
            .filter_map(|record| {
                record["message_id"].as_u64().map(|id| {
                    Ok(ApplicationRecord::Message {
                        message_id: id as u32,
                        attributes: record["file_flags"].as_u64().unwrap_or(0) as u8,
                        data: hex(record["data_hex"].as_str().ok_or("missing message data")?)?,
                    })
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        actual.retain(|record| matches!(record, ApplicationRecord::Message { .. }));
        if actual != expected {
            return Err(format!(
                "application reassembly differs at HCI frame {}",
                frame["hci_frame"]
            )
            .into());
        }
        for record in actual {
            let ApplicationRecord::Message {
                message_id: id,
                attributes,
                data,
            } = record
            else {
                unreachable!()
            };
            if attributes != 0 {
                *counts
                    .entry("uninterpreted_file_attributes".into())
                    .or_default() += 1;
                continue;
            }
            match decode_product(id, &data) {
                Ok(product) => {
                    let label = match product {
                        Product::Attitude(_) => "attitude".into(),
                        Product::PressureUnverified { .. } => "pressure_unverified".into(),
                        Product::AttitudeSource { .. } => "attitude_source".into(),
                        Product::GpsSource { .. } => "gps_source".into(),
                        Product::Nmea { ref sentence } => {
                            let clock = app_core::receiver::capture::CaptureClock {
                                monotonic_ms: relative_time(&frame)?.num_milliseconds() as u64,
                                wall_epoch_ms: (anchor + relative_time(&frame)?).timestamp_millis(),
                            };
                            live.ingest_nmea(sentence, clock);
                            if live
                                .snapshot()
                                .ownship_at(clock)
                                .is_some_and(|sample| sample.altitude_msl_ft.is_some())
                            {
                                *counts.entry("nmea_fused_msl".into()).or_default() += 1;
                            }
                            "nmea_checksum_valid".into()
                        }
                        Product::Traffic(ref traffic) => {
                            let clock = app_core::receiver::capture::CaptureClock {
                                monotonic_ms: relative_time(&frame)?.num_milliseconds() as u64,
                                wall_epoch_ms: (anchor + relative_time(&frame)?).timestamp_millis(),
                            };
                            live.ingest(traffic, clock);
                            if live.snapshot().ownship_at(clock).is_some() {
                                *counts.entry("traffic_current_ownship".into()).or_default() += 1;
                            }
                            *counts.entry("traffic_targets".into()).or_default() +=
                                traffic.targets.len();
                            if traffic.ownship.latitude.is_some() {
                                *counts.entry("traffic_valid_ownship".into()).or_default() += 1;
                            }
                            "traffic".into()
                        }
                        Product::Weather(Weather::Text { kind, reports, .. }) => {
                            *counts.entry(format!("reports_{kind:?}")).or_default() +=
                                reports.len();
                            let station_kind = match kind {
                                WeatherKind::Metar => Some(StationReportKind::Metar),
                                WeatherKind::Taf => Some(StationReportKind::Taf),
                                _ => None,
                            };
                            if let Some(station_kind) = station_kind {
                                let received_at = anchor + relative_time(&frame)?;
                                for report in &reports {
                                    let normalized = StationReport::from_receiver(
                                        station_kind,
                                        report,
                                        received_at,
                                    )
                                    .and_then(|record| {
                                        if station_kind == StationReportKind::Metar {
                                            *counts
                                                .entry(format!(
                                                    "metar_category_{}",
                                                    record.flight_category().unwrap_or("unknown")
                                                ))
                                                .or_default() += 1;
                                            *counts
                                                .entry(format!(
                                                    "metar_cloud_{}",
                                                    record.cloud_symbol().unwrap_or("unknown")
                                                ))
                                                .or_default() += 1;
                                        }
                                        if record
                                            .timing(received_at)
                                            .timestamp_ahead_of_receipt_ms
                                            .is_some()
                                        {
                                            *counts
                                                .entry(format!(
                                                    "timestamp_ahead_of_receipt_{kind:?}"
                                                ))
                                                .or_default() += 1;
                                        }
                                        station_weather.ingest(WeatherSource::Receiver, record)
                                    });
                                    match normalized {
                                        Ok(_) => {
                                            *counts
                                                .entry(format!("normalized_{kind:?}"))
                                                .or_default() += 1
                                        }
                                        Err(error) => {
                                            *rejected_reports
                                                .entry(format!("normalize_{kind:?}: {error}"))
                                                .or_default() += 1
                                        }
                                    }
                                }
                            }
                            format!("weather_{kind:?}")
                        }
                        Product::Weather(Weather::Radar(radar)) => {
                            let clock = app_core::receiver::capture::CaptureClock {
                                monotonic_ms: relative_time(&frame)?.num_milliseconds() as u64,
                                wall_epoch_ms: (anchor + relative_time(&frame)?).timestamp_millis(),
                            };
                            match live.ingest_radar(&radar, clock) {
                                Ok(true) => {
                                    *counts.entry("radar_frames_prepared".into()).or_default() += 1;
                                }
                                Ok(false) => {
                                    *counts.entry("radar_duplicate_or_old".into()).or_default() +=
                                        1;
                                }
                                Err(error) => {
                                    *failures
                                        .entry(format!("radar_prepare: {error}"))
                                        .or_default() += 1;
                                }
                            }
                            format!("radar_{:?}", radar.kind)
                        }
                        Product::Weather(Weather::Empty { .. }) => "weather_empty".into(),
                        Product::Weather(Weather::Uninterpreted { .. }) => {
                            "weather_uninterpreted".into()
                        }
                        Product::Uninterpreted { .. } => "uninterpreted".into(),
                    };
                    *counts.entry(label).or_default() += 1;
                }
                Err(error) => {
                    *failures.entry(format!("{id:08x}: {error}")).or_default() += 1;
                }
            }
        }
    }
    let snapshot = station_weather.snapshot()?;
    let restored = StationWeather::restore(&snapshot)?;
    if restored.snapshot()? != snapshot {
        return Err("receiver weather cache restart changed stored reports".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "counts": counts,
            "failures": failures,
            "rejected_reports": rejected_reports,
        }))?
    );
    if !failures.is_empty() {
        return Err("receiver corpus contains decoding failures".into());
    }
    Ok(())
}
