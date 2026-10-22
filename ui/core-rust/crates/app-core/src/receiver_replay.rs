// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A bounded timeline of received inputs, not a second weather/traffic model.
use std::{collections::BTreeMap, sync::Arc};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

use crate::{
    receiver::{capture::CaptureClock, live::LiveInput, Product, Weather, WeatherKind},
    weather_controller::{LiveNexradInstalledState, NexradFrameStorage, WeatherController},
    weather_sources::{StationReport, StationReportKind},
    MetarProductPayload, TafProductPayload,
};

pub const FORMAT: &str = "aerobag-receiver-replay";
pub const MAX_JSON_BYTES: usize = 96 * 1024 * 1024;

pub fn validate(text: &str) -> Result<(), String> {
    let trace = Trace::parse(text)?;
    Reduction::default().advance(
        &trace,
        &mut WeatherController::default(),
        trace.document.duration_ms,
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub schema_version: u32,
    pub title: String,
    pub start_epoch_ms: i64,
    pub duration_ms: i64,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub elapsed_ms: i64,
    #[serde(flatten)]
    pub input: Input,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    Connection {
        connected: bool,
    },
    Receiver {
        message_id: u32,
        bytes_base64: String,
    },
    InternetMetars {
        payload: MetarProductPayload,
    },
    InternetTafs {
        payload: TafProductPayload,
    },
    InternetRadar {
        version: String,
        manifest: serde_json::Value,
        tiles_base64: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Trace {
    pub document: Document,
    decoded: Vec<Option<Product>>,
}

impl Trace {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_JSON_BYTES {
            return Err("Receiver replay exceeds 96 MiB".into());
        }
        let document: Document = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if document.format != FORMAT || document.schema_version != 1 {
            return Err("Unsupported receiver replay format".into());
        }
        if !(0..=12 * 60 * 60_000).contains(&document.duration_ms)
            || document.events.len() > 250_000
            || chrono::DateTime::from_timestamp_millis(document.start_epoch_ms).is_none()
            || document
                .start_epoch_ms
                .checked_add(document.duration_ms)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .is_none()
        {
            return Err("Invalid receiver replay bounds".into());
        }
        let mut previous = 0;
        let mut decoded = Vec::with_capacity(document.events.len());
        for event in &document.events {
            if !(previous..=document.duration_ms).contains(&event.elapsed_ms) {
                return Err("Receiver replay events must be ordered within its duration".into());
            }
            previous = event.elapsed_ms;
            decoded.push(match &event.input {
                Input::Receiver {
                    message_id,
                    bytes_base64,
                } => {
                    let bytes = STANDARD.decode(bytes_base64).map_err(|e| e.to_string())?;
                    Some(
                        crate::receiver::decode_product(*message_id, &bytes)
                            .map_err(|e| e.to_string())?,
                    )
                }
                Input::InternetRadar {
                    version,
                    manifest,
                    tiles_base64,
                } => {
                    let expected = crate::session::replay_nexrad_tile_urls(version, manifest)?;
                    if expected != tiles_base64.keys().cloned().collect() {
                        return Err("Replay radar must include exactly all manifest tiles".into());
                    }
                    for value in tiles_base64.values() {
                        let bytes = STANDARD.decode(value).map_err(|error| error.to_string())?;
                        use image::ImageDecoder;
                        let decoder =
                            image::codecs::png::PngDecoder::new(std::io::Cursor::new(&bytes))
                                .map_err(|error| error.to_string())?;
                        if decoder.dimensions().0 > 4096 || decoder.dimensions().1 > 4096 {
                            return Err("Replay radar tile exceeds image bounds".into());
                        }
                    }
                    None
                }
                _ => None,
            });
        }
        Ok(Self { document, decoded })
    }

    pub fn duration_seconds(&self) -> f64 {
        self.document.duration_ms as f64 / 1000.0
    }
}

pub(crate) struct Reduction {
    pub live: LiveInput,
    pub activity: crate::receiver::activity::Activity,
    pub sensors: crate::situation_controller::SituationController,
    index: usize,
    elapsed_ms: i64,
}

impl Default for Reduction {
    fn default() -> Self {
        Self {
            live: LiveInput::default(),
            activity: Default::default(),
            sensors: Default::default(),
            index: 0,
            elapsed_ms: 0,
        }
    }
}

impl Reduction {
    pub fn elapsed_ms(&self) -> i64 {
        self.elapsed_ms
    }

    pub fn advance(
        &mut self,
        trace: &Trace,
        weather: &mut WeatherController,
        target_ms: i64,
    ) -> Result<(), String> {
        if target_ms < self.elapsed_ms {
            return Err("Rewind requires a fresh receiver reduction".into());
        }
        while self.index < trace.document.events.len() {
            let event = &trace.document.events[self.index];
            if event.elapsed_ms > target_ms {
                break;
            }
            self.advance_deadlines(trace, weather, event.elapsed_ms);
            let clock = self.clock(trace, event.elapsed_ms);
            self.activity
                .advance(clock.monotonic_ms, self.live.snapshot().connected);
            if let Some(product) = trace.decoded[self.index].as_ref() {
                self.activity.observe(product, clock.monotonic_ms);
            }
            match &event.input {
                Input::Connection { connected } => {
                    self.live.set_connected(*connected);
                    self.activity.advance(clock.monotonic_ms, *connected);
                }
                Input::Receiver { .. } => match trace.decoded[self.index].as_ref().unwrap() {
                    Product::Traffic(traffic) => self.live.ingest(traffic, clock),
                    Product::Nmea { sentence } => self.live.ingest_nmea(sentence, clock),
                    Product::Weather(Weather::Radar(radar)) => {
                        self.live
                            .ingest_radar(radar, clock)
                            .map_err(str::to_string)?;
                    }
                    Product::Weather(Weather::Text { kind, reports, .. }) => {
                        let kind = match kind {
                            WeatherKind::Metar => Some(StationReportKind::Metar),
                            WeatherKind::Taf => Some(StationReportKind::Taf),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let now = chrono::DateTime::from_timestamp_millis(clock.wall_epoch_ms)
                                .unwrap();
                            let reports = reports
                                .iter()
                                .map(|report| StationReport::from_receiver(kind, report, now))
                                .collect::<Result<Vec<_>, _>>()
                                .map_err(str::to_string)?;
                            weather
                                .ingest_receiver_reports(reports)
                                .map_err(str::to_string)?;
                        }
                    }
                    _ => {}
                },
                Input::InternetMetars { payload } => {
                    weather
                        .runtime_mut()
                        .station_weather
                        .install_metars(
                            payload.clone(),
                            chrono::DateTime::from_timestamp_millis(clock.wall_epoch_ms).unwrap(),
                        )
                        .map_err(str::to_string)?;
                }
                Input::InternetTafs { payload } => {
                    weather
                        .runtime_mut()
                        .station_weather
                        .install_tafs(
                            payload.clone(),
                            chrono::DateTime::from_timestamp_millis(clock.wall_epoch_ms).unwrap(),
                        )
                        .map_err(str::to_string)?;
                }
                Input::InternetRadar {
                    version,
                    manifest,
                    tiles_base64,
                } => {
                    let tiles = tiles_base64
                        .iter()
                        .map(|(url, value)| {
                            STANDARD
                                .decode(value)
                                .map(|bytes| (url.clone(), bytes))
                                .map_err(|e| e.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    // An installed replay frame owns all of its pixels locally.
                    let runtime = weather.runtime_mut();
                    runtime.nexrad_tile_cache.extend(tiles);
                    runtime.nexrad_installed.insert(
                        version.clone(),
                        LiveNexradInstalledState {
                            version: version.clone(),
                            storage: NexradFrameStorage::Memory,
                            manifest: manifest.clone(),
                        },
                    );
                }
            }
            self.live.expire(clock);
            let live = self.live.snapshot();
            let pressure = live.pressure_at(clock);
            self.sensors.barometer_mut().observe_receiver(
                live.available,
                pressure.map(|(_, altitude)| altitude),
                pressure.map_or(clock.wall_epoch_ms, |(time, _)| time),
                clock.wall_epoch_ms,
            );
            if live.available {
                if !self.sensors.receiver().available {
                    self.sensors.register_source(live.registration());
                }
                if let Some(sample) = live
                    .ownship_at(clock)
                    .filter(|sample| self.sensors.receiver().ownship.as_ref() != Some(*sample))
                {
                    self.sensors.push_sample(sample.clone());
                }
                self.sensors.update_source_status(live.source_status(clock));
                self.sensors.refresh_ownship_at(clock.wall_epoch_ms);
            }
            self.sensors.set_receiver(live.clone());
            weather.set_receiver_radar(live.radar.clone());
            weather.refresh_nexrad_selection(clock.wall_epoch_ms);
            self.index += 1;
        }
        self.advance_deadlines(trace, weather, target_ms);
        self.live.expire(self.clock(trace, target_ms));
        let clock = self.clock(trace, target_ms);
        self.activity
            .advance(clock.monotonic_ms, self.live.snapshot().connected);
        let live = self.live.snapshot();
        if live.available {
            self.sensors.update_source_status(live.source_status(clock));
        }
        self.sensors.set_receiver(live);
        self.sensors.refresh_ownship_at(clock.wall_epoch_ms);
        Ok(())
    }

    fn advance_deadlines(&mut self, trace: &Trace, weather: &mut WeatherController, target: i64) {
        // Selection can change between packets. Visit its exact age/expiry
        // deadlines so a seek and incremental playback make identical choices.
        let start = trace.document.start_epoch_ms;
        loop {
            let next = weather.next_nexrad_selection_deadline(start + self.elapsed_ms);
            let Some(next) = next.filter(|time| *time <= start + target) else {
                break;
            };
            self.elapsed_ms = next - start;
            weather.refresh_nexrad_selection(next);
        }
        self.elapsed_ms = target;
        weather.refresh_nexrad_selection(start + target);
    }

    pub fn clock(&self, trace: &Trace, elapsed_ms: i64) -> CaptureClock {
        CaptureClock {
            wall_epoch_ms: trace.document.start_epoch_ms + elapsed_ms,
            monotonic_ms: elapsed_ms as u64,
        }
    }
}

pub(crate) fn trace_error(message: String) -> crate::AppError {
    crate::AppError {
        kind: crate::AppErrorKind::InvalidManifest,
        message: format!("Receiver replay: {message}"),
    }
}

pub(crate) type SharedTrace = Arc<Trace>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather_controller::NexradSource;
    const MINUTE: i64 = 60_000;

    fn start() -> i64 {
        chrono::DateTime::parse_from_rfc3339("2026-10-10T12:00:00Z")
            .unwrap()
            .timestamp_millis()
    }

    fn internet_metars(minute: u32, present: bool) -> Input {
        let observed =
            chrono::DateTime::from_timestamp_millis(start() + i64::from(minute) * MINUTE).unwrap();
        Input::InternetMetars {
            payload: MetarProductPayload {
                schema_version: 3,
                version_label: format!("net-{minute}"),
                generated_at_utc: Some(observed),
                observed_at_utc: None,
                metar_count: None,
                metars_by_station: if present {
                    std::collections::HashMap::from([(
                        "KTEST".into(),
                        crate::MetarRecord {
                            station_id: "KTEST".into(),
                            latitude: 47.0,
                            longitude: -122.0,
                            raw_text: format!(
                                "METAR KTEST 1012{minute:02}Z 00000KT 10SM CLR 10/08 A3000"
                            ),
                            observed_at_utc: Some(observed.to_rfc3339()),
                            flight_category: Some("VFR".into()),
                            clouds: None,
                        },
                    )])
                } else {
                    Default::default()
                },
            },
        }
    }

    fn trace() -> Trace {
        let mut entries = Vec::<(i64, Input, Option<Product>)>::new();
        entries.push((0, Input::Connection { connected: true }, None));
        let receiver = |minute, product| {
            (
                minute * MINUTE,
                Input::Receiver {
                    message_id: 0,
                    bytes_base64: String::new(),
                },
                Some(product),
            )
        };
        entries.push(receiver(
            0,
            Product::Traffic(crate::receiver::test_support::traffic(start())),
        ));
        entries.push(receiver(
            0,
            Product::Weather(Weather::Radar(crate::receiver::test_support::radar())),
        ));
        entries.push((MINUTE, internet_metars(1, true), None));
        entries.push(receiver(
            2,
            Product::Weather(Weather::Text {
                kind: WeatherKind::Metar,
                issue_time: crate::receiver::PartialUtc {
                    day: 10,
                    hour: 12,
                    minute: 2,
                },
                reports: vec![crate::receiver::TextReport {
                    text: "METAR KTEST 101202Z 00000KT 10SM CLR 11/08 A3000".into(),
                    station: Some("KTEST".into()),
                    notam_identifier: None,
                    record_type_raw: None,
                }],
                additional_segments: vec![],
            }),
        ));
        entries.push((3 * MINUTE, internet_metars(3, true), None));
        entries.push((4 * MINUTE, internet_metars(4, false), None));
        entries.push(receiver(
            11,
            Product::Traffic(crate::receiver::test_support::traffic(
                start() + 11 * MINUTE,
            )),
        ));
        let (events, decoded) = entries
            .into_iter()
            .map(|(elapsed_ms, input, product)| (Event { elapsed_ms, input }, product))
            .unzip();
        Trace {
            document: Document {
                format: FORMAT.into(),
                schema_version: 1,
                title: "fixture".into(),
                start_epoch_ms: start(),
                duration_ms: 75 * MINUTE,
                events,
            },
            decoded,
        }
    }

    #[test]
    fn receiver_replay_seek_matches_incremental_inputs_expiry_and_weather_replacements() {
        let trace = trace();
        let mut playing = Reduction::default();
        let mut weather = WeatherController::default();
        for target in [
            0,
            4999,
            5000,
            9999,
            10000,
            MINUTE,
            2 * MINUTE,
            3 * MINUTE,
            4 * MINUTE,
            11 * MINUTE,
            11 * MINUTE + 5000,
            60 * MINUTE,
            75 * MINUTE,
        ] {
            playing.advance(&trace, &mut weather, target).unwrap();
            let mut seeking = Reduction::default();
            let mut sought_weather = WeatherController::default();
            seeking
                .advance(&trace, &mut sought_weather, target)
                .unwrap();
            assert_eq!(
                playing.live.snapshot(),
                seeking.live.snapshot(),
                "sensor state at {target}"
            );
            assert_eq!(
                playing.activity.view(target as u64),
                seeking.activity.view(target as u64),
                "activity history at {target}"
            );
            assert_eq!(
                weather.runtime().station_weather.snapshot().unwrap(),
                sought_weather.runtime().station_weather.snapshot().unwrap(),
                "weather at {target}"
            );
            assert_eq!(
                weather.nexrad_source(),
                sought_weather.nexrad_source(),
                "source at {target}"
            );
            assert_eq!(
                weather.runtime().nexrad_transitions,
                sought_weather.runtime().nexrad_transitions
            );
        }
        // Rewind drops the later Internet replacement and receiver reception.
        let mut rewind = Reduction::default();
        let mut earlier = WeatherController::default();
        rewind.advance(&trace, &mut earlier, MINUTE).unwrap();
        let activity = rewind.activity.view(MINUTE as u64).unwrap();
        assert_eq!(activity.strips[1].legend[0].text, "METAR: not received", "neither later receiver weather nor earlier Internet weather counts as receiver reception");
        assert_eq!(
            activity.strips[1].legend[2].text,
            "Radar: last received 1m ago"
        );
        let report = earlier
            .runtime()
            .station_weather
            .query()
            .metar("KTEST")
            .unwrap();
        assert!(report.raw_text.contains("101201Z"));
        assert!(
            weather
                .runtime()
                .station_weather
                .query()
                .metar("KTEST")
                .unwrap()
                .raw_text
                .contains("101202Z"),
            "Internet removal must leave the receiver report visible"
        );
        assert_eq!(earlier.nexrad_source(), NexradSource::Receiver);
        assert!(rewind.advance(&trace, &mut earlier, 0).is_err());
    }

    #[test]
    fn receiver_replay_visits_hysteresis_deadlines_between_packets() {
        let mut trace = trace();
        let input = Input::InternetRadar {
            version: "comparison".into(),
            manifest: serde_json::json!({ "state_id": "comparison", "observed_at_utc": "2026-10-10T12:05:00Z" }),
            tiles_base64: BTreeMap::new(),
        };
        let index = trace
            .document
            .events
            .partition_point(|event| event.elapsed_ms < 5 * MINUTE);
        trace.document.events.insert(
            index,
            Event {
                elapsed_ms: 5 * MINUTE,
                input,
            },
        );
        trace.decoded.insert(index, None);
        let mut playing = Reduction::default();
        let mut weather = WeatherController::default();
        for target in [
            0,
            5 * MINUTE,
            8 * MINUTE - 1,
            8 * MINUTE,
            10 * MINUTE,
            12 * MINUTE,
        ] {
            playing.advance(&trace, &mut weather, target).unwrap();
            let mut direct = Reduction::default();
            let mut sought = WeatherController::default();
            direct.advance(&trace, &mut sought, target).unwrap();
            assert_eq!(
                weather.runtime().nexrad_transitions,
                sought.runtime().nexrad_transitions
            );
            assert_eq!(
                weather.nexrad_source(),
                if target < 8 * MINUTE {
                    NexradSource::Receiver
                } else {
                    NexradSource::Internet
                }
            );
        }
        assert_eq!(
            weather.runtime().nexrad_transitions.back().unwrap().0,
            start() + 8 * MINUTE,
            "The switch occurs at the age threshold, not at the next packet or seek destination"
        );
    }

    #[test]
    fn receiver_replay_validates_timeline_and_real_decoder_before_installation() {
        let document = Document {
            format: FORMAT.into(),
            schema_version: 1,
            title: "fixture".into(),
            start_epoch_ms: start(),
            duration_ms: 1000,
            events: vec![
                Event {
                    elapsed_ms: 0,
                    input: Input::Connection { connected: true },
                },
                Event {
                    elapsed_ms: 1000,
                    input: Input::Receiver {
                        message_id: 0x2100,
                        bytes_base64: STANDARD.encode([0; 16]),
                    },
                },
            ],
        };
        assert!(validate(&serde_json::to_string(&document).unwrap()).is_ok());
        let mut broken = document.clone();
        broken.events[1].elapsed_ms = 1001;
        assert!(validate(&serde_json::to_string(&broken).unwrap()).is_err());
        let Input::Receiver { bytes_base64, .. } = &mut broken.events[1].input else {
            panic!()
        };
        *bytes_base64 = STANDARD.encode([0; 15]);
        broken.events[1].elapsed_ms = 1000;
        assert!(validate(&serde_json::to_string(&broken).unwrap())
            .unwrap_err()
            .contains("attitude length"));
    }
}
