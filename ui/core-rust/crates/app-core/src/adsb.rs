// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::{
    great_circle_distance_nm, CoreResourceRequest, CoreResourceSource, LatLon, MapSurfaceMetrics,
};

const AIRPLANES_LIVE_BASE_URL: &str = "https://api.airplanes.live/v2";
const TRAFFIC_RESOURCE_PREFIX: &str = "adsb/airplanes_live/traffic/";
// Ten seconds fits the provider's published contributor allowance when traffic is left on all day.
const TRAFFIC_POLL_INTERVAL_MS: i64 = 10_000;
const REQUEST_MIN_INTERVAL_MS: i64 = 1_100;
const FAILURE_BACKOFF_MAX_MS: i64 = 65_000;
const MIN_QUERY_RADIUS_NM: f64 = 10.0;
const MAX_QUERY_RADIUS_NM: f64 = 250.0;
const COVERAGE_MARGIN: f64 = 1.15;
const MAX_TRAFFIC_RECORDS: usize = 5_000;
const MAX_TRAFFIC_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_VISIBLE_POSITION_AGE_MS: i64 = 15_000;
const MAX_RELATIVE_ALTITUDE_FT: f64 = 10_000.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AdsbAircraft {
    pub id: String,
    pub registration: Option<String>,
    pub callsign: Option<String>,
    pub aircraft_type: Option<String>,
    pub position: Option<LatLon>,
    pub position_epoch_ms: Option<i64>,
    pub track_deg_true: Option<f64>,
    pub ground_speed_kt: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub vertical_speed_fpm: Option<f64>,
    pub on_ground: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleAdsbTraffic {
    pub id: String,
    pub screen_x: f64,
    pub screen_y: f64,
    #[serde(default)]
    pub track_deg_true: Option<f64>,
    pub label: String,
    pub altitude_label: String,
    #[serde(default)]
    pub relative_altitude_label: Option<String>,
    pub on_ground: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct TrafficOwnshipAltitude {
    pub altitude_msl_ft: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct TrafficCoverage {
    center: LatLon,
    radius_nm: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingTrafficRequest {
    resource_id: String,
    coverage: TrafficCoverage,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AdsbSessionState {
    aircraft: Vec<AdsbAircraft>,
    coverage: Option<TrafficCoverage>,
    pending: Option<PendingTrafficRequest>,
    next_generation: u64,
    last_request_epoch_ms: Option<i64>,
    next_poll_epoch_ms: Option<i64>,
    consecutive_failures: u32,
    last_error: Option<String>,
}

impl AdsbSessionState {
    pub(crate) fn handles_resource(resource_id: &str) -> bool {
        resource_id.starts_with(TRAFFIC_RESOURCE_PREFIX)
    }

    pub(crate) fn prepare_traffic_request(
        &mut self,
        metrics: MapSurfaceMetrics,
        epoch_ms: i64,
    ) -> Option<CoreResourceRequest> {
        if self.pending.is_some() {
            return None;
        }
        let visible_radius_nm = metrics
            .visible_radius_nm()
            .clamp(MIN_QUERY_RADIUS_NM, MAX_QUERY_RADIUS_NM);
        let desired = TrafficCoverage {
            center: metrics.viewport.center,
            radius_nm: (visible_radius_nm * COVERAGE_MARGIN)
                .clamp(MIN_QUERY_RADIUS_NM, MAX_QUERY_RADIUS_NM),
        };
        let coverage_missing = self.coverage.as_ref().is_none_or(|coverage| {
            great_circle_distance_nm(coverage.center, desired.center) + visible_radius_nm
                > coverage.radius_nm
        });
        let poll_due = self
            .next_poll_epoch_ms
            .is_none_or(|deadline| epoch_ms >= deadline);
        if self.consecutive_failures > 0 && !poll_due {
            return None;
        }
        if !coverage_missing && !poll_due {
            return None;
        }
        let throttle_deadline = self
            .last_request_epoch_ms
            .map(|last| last.saturating_add(REQUEST_MIN_INTERVAL_MS))
            .unwrap_or(i64::MIN);
        if epoch_ms < throttle_deadline {
            return None;
        }

        self.next_generation = self.next_generation.saturating_add(1);
        let resource_id = format!("{TRAFFIC_RESOURCE_PREFIX}{}", self.next_generation);
        let radius_nm = desired.radius_nm.ceil().clamp(1.0, MAX_QUERY_RADIUS_NM);
        let url = format!(
            "{AIRPLANES_LIVE_BASE_URL}/point/{:.5}/{:.5}/{:.0}",
            desired.center.lat, desired.center.lon, radius_nm
        );
        self.pending = Some(PendingTrafficRequest {
            resource_id: resource_id.clone(),
            coverage: TrafficCoverage {
                center: desired.center,
                radius_nm,
            },
        });
        self.last_request_epoch_ms = Some(epoch_ms);
        Some(CoreResourceRequest {
            id: resource_id,
            source: CoreResourceSource::PublicUrl { url },
            optional: false,
            max_response_bytes: Some(MAX_TRAFFIC_RESPONSE_BYTES),
        })
    }

    pub(crate) fn next_refresh_epoch_ms(
        &self,
        metrics: MapSurfaceMetrics,
        epoch_ms: i64,
    ) -> Option<i64> {
        if self.pending.is_some() {
            return None;
        }
        let visible_radius_nm = metrics
            .visible_radius_nm()
            .clamp(MIN_QUERY_RADIUS_NM, MAX_QUERY_RADIUS_NM);
        let coverage_missing = self.coverage.as_ref().is_none_or(|coverage| {
            great_circle_distance_nm(coverage.center, metrics.viewport.center) + visible_radius_nm
                > coverage.radius_nm
        });
        let throttle_deadline = self
            .last_request_epoch_ms
            .map(|last| last.saturating_add(REQUEST_MIN_INTERVAL_MS))
            .unwrap_or(epoch_ms);
        if coverage_missing {
            return Some(
                self.next_poll_epoch_ms
                    .unwrap_or(epoch_ms)
                    .max(throttle_deadline)
                    .max(epoch_ms),
            );
        }
        Some(
            self.next_poll_epoch_ms
                .unwrap_or(epoch_ms)
                .max(throttle_deadline),
        )
    }

    pub(crate) fn ingest(
        &mut self,
        resource_id: &str,
        bytes: &[u8],
        received_epoch_ms: i64,
    ) -> Result<(), String> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if pending.resource_id != resource_id {
            return Ok(());
        }
        let payload: AirplanesLiveResponse = serde_json::from_slice(bytes).map_err(|error| {
            format!("failed to decode Airplanes.live traffic response: {error}")
        })?;
        if payload.ac.len() > MAX_TRAFFIC_RECORDS {
            return Err(format!(
                "Airplanes.live traffic response contains {} aircraft; limit is {MAX_TRAFFIC_RECORDS}",
                payload.ac.len()
            ));
        }
        let source_epoch_ms = payload.now.unwrap_or(received_epoch_ms);
        self.aircraft = payload
            .ac
            .into_iter()
            .filter_map(|record| normalize_aircraft(record, source_epoch_ms))
            .collect();
        self.coverage = Some(pending.coverage.clone());
        self.pending = None;
        self.next_poll_epoch_ms = Some(received_epoch_ms.saturating_add(TRAFFIC_POLL_INTERVAL_MS));
        self.consecutive_failures = 0;
        self.last_error = None;
        Ok(())
    }

    pub(crate) fn record_failure(&mut self, resource_id: &str, message: &str, epoch_ms: i64) {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.resource_id != resource_id)
        {
            return;
        }
        self.pending = None;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let exponent = self.consecutive_failures.saturating_sub(1).min(5);
        let delay_ms = TRAFFIC_POLL_INTERVAL_MS
            .saturating_mul(1_i64 << exponent)
            .min(FAILURE_BACKOFF_MAX_MS);
        self.next_poll_epoch_ms = Some(epoch_ms.saturating_add(delay_ms));
        self.last_error = Some(message.to_string());
    }

    pub(crate) fn visible_traffic(
        &self,
        metrics: MapSurfaceMetrics,
        ownship: TrafficOwnshipAltitude,
        epoch_ms: i64,
    ) -> Vec<VisibleAdsbTraffic> {
        self.aircraft
            .iter()
            .filter_map(|aircraft| {
                let position = aircraft.position?;
                let position_epoch_ms = aircraft.position_epoch_ms?;
                if epoch_ms.saturating_sub(position_epoch_ms) > MAX_VISIBLE_POSITION_AGE_MS {
                    return None;
                }
                let (screen_x, screen_y) = metrics.project_position(position);
                if screen_x < -64.0
                    || screen_x > metrics.width_px + 64.0
                    || screen_y < -64.0
                    || screen_y > metrics.height_px + 64.0
                {
                    return None;
                }
                let altitude_label = if aircraft.on_ground {
                    "GND".to_string()
                } else {
                    aircraft
                        .pressure_altitude_ft
                        .or(aircraft.altitude_msl_ft)
                        .map(format_altitude)
                        .unwrap_or_else(|| "---".to_string())
                };
                let relative_altitude_ft = aircraft
                    .altitude_msl_ft
                    .zip(ownship.altitude_msl_ft)
                    .map(|(traffic, ownship)| traffic - ownship)
                    .or_else(|| {
                        aircraft
                            .pressure_altitude_ft
                            .zip(ownship.pressure_altitude_ft)
                            .map(|(traffic, ownship)| traffic - ownship)
                    });
                if relative_altitude_ft.is_some_and(|delta| delta.abs() > MAX_RELATIVE_ALTITUDE_FT)
                {
                    return None;
                }
                Some(VisibleAdsbTraffic {
                    id: aircraft.id.clone(),
                    screen_x,
                    screen_y,
                    track_deg_true: aircraft.track_deg_true,
                    label: aircraft
                        .callsign
                        .clone()
                        .or_else(|| aircraft.registration.clone())
                        .unwrap_or_else(|| aircraft.id.to_uppercase()),
                    altitude_label,
                    relative_altitude_label: relative_altitude_ft.map(format_relative_altitude),
                    on_ground: aircraft.on_ground,
                })
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct AirplanesLiveResponse {
    #[serde(default)]
    ac: Vec<AirplanesLiveAircraft>,
    #[serde(default)]
    now: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AirplanesLiveAircraft {
    hex: String,
    #[serde(default)]
    flight: Option<String>,
    #[serde(default)]
    r: Option<String>,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    seen_pos: Option<f64>,
    #[serde(default)]
    track: Option<f64>,
    #[serde(default)]
    gs: Option<f64>,
    #[serde(default)]
    alt_baro: Option<NumericOrGround>,
    #[serde(default)]
    alt_geom: Option<f64>,
    #[serde(default)]
    baro_rate: Option<f64>,
    #[serde(default)]
    geom_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NumericOrGround {
    Numeric(f64),
    Text(String),
}

fn normalize_aircraft(record: AirplanesLiveAircraft, source_epoch_ms: i64) -> Option<AdsbAircraft> {
    let id = record.hex.trim().to_ascii_lowercase();
    if id.is_empty() {
        return None;
    }
    let on_ground = matches!(
        record.alt_baro,
        Some(NumericOrGround::Text(ref value)) if value.eq_ignore_ascii_case("ground")
    );
    let pressure_altitude_ft = match record.alt_baro {
        Some(NumericOrGround::Numeric(value)) if value.is_finite() => Some(value),
        _ => None,
    };
    let position = record
        .lat
        .zip(record.lon)
        .filter(|(lat, lon)| {
            lat.is_finite()
                && lon.is_finite()
                && (-90.0..=90.0).contains(lat)
                && (-180.0..=180.0).contains(lon)
        })
        .map(|(lat, lon)| LatLon { lat, lon });
    let position_epoch_ms = position.and_then(|_| {
        record
            .seen_pos
            .filter(|age| age.is_finite())
            .map(|age| source_epoch_ms.saturating_sub((age.max(0.0) * 1_000.0).round() as i64))
    });
    Some(AdsbAircraft {
        id,
        registration: nonempty_trimmed(record.r),
        callsign: nonempty_trimmed(record.flight),
        aircraft_type: nonempty_trimmed(record.t),
        position,
        position_epoch_ms,
        track_deg_true: finite(record.track),
        ground_speed_kt: finite(record.gs),
        pressure_altitude_ft,
        altitude_msl_ft: finite(record.alt_geom),
        vertical_speed_fpm: finite(record.geom_rate).or_else(|| finite(record.baro_rate)),
        on_ground,
    })
}

fn nonempty_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn format_altitude(altitude_ft: f64) -> String {
    format!("{:.0}", (altitude_ft / 100.0).round())
}

fn format_relative_altitude(delta_ft: f64) -> String {
    format!("{:+03.0}", delta_ft / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MapViewport;

    fn metrics() -> MapSurfaceMetrics {
        MapSurfaceMetrics::new(
            MapViewport {
                center: LatLon {
                    lat: 47.45,
                    lon: -122.31,
                },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            800.0,
            600.0,
            1.0,
        )
    }

    #[test]
    fn normalizes_airplanes_live_response_and_projects_relative_altitude() {
        let mut state = AdsbSessionState::default();
        let request = state
            .prepare_traffic_request(metrics(), 1_785_900_880_000)
            .expect("first request");
        let bytes = br#"{
          "now": 1785900882001,
          "ac": [{
            "hex": "a757ae", "flight": "AAL727  ", "r": "N572UW", "t": "A321",
            "alt_baro": 2250, "alt_geom": 2300, "gs": 152.0, "track": 0.75,
            "geom_rate": -832, "lat": 47.451, "lon": -122.309, "seen_pos": 0.25
          }]
        }"#;
        state
            .ingest(&request.id, bytes, 1_785_900_882_100)
            .expect("ingest response");

        let visible = state.visible_traffic(
            metrics(),
            TrafficOwnshipAltitude {
                altitude_msl_ft: Some(1_300.0),
                pressure_altitude_ft: Some(1_250.0),
            },
            1_785_900_882_100,
        );
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].label, "AAL727");
        assert_eq!(visible[0].altitude_label, "23");
        assert_eq!(visible[0].relative_altitude_label.as_deref(), Some("+10"));
    }

    #[test]
    fn ground_string_is_not_misread_as_an_altitude() {
        let record: AirplanesLiveAircraft = serde_json::from_str(
            r#"{"hex":"abc123","alt_baro":"ground","lat":47.0,"lon":-122.0,"seen_pos":0}"#,
        )
        .expect("aircraft");
        let aircraft = normalize_aircraft(record, 1_000).expect("normalized aircraft");
        assert!(aircraft.on_ground);
        assert_eq!(aircraft.pressure_altitude_ft, None);
    }

    #[test]
    fn pending_request_prevents_duplicate_fetch_and_failure_backs_off() {
        let mut state = AdsbSessionState::default();
        let request = state
            .prepare_traffic_request(metrics(), 10_000)
            .expect("first request");
        assert!(state.prepare_traffic_request(metrics(), 10_500).is_none());
        state.record_failure(&request.id, "network unavailable", 10_500);
        assert_eq!(state.next_refresh_epoch_ms(metrics(), 10_500), Some(20_500));
        assert!(state.prepare_traffic_request(metrics(), 20_499).is_none());
        assert!(state.prepare_traffic_request(metrics(), 20_500).is_some());
    }

    #[test]
    fn ownship_altitude_filters_irrelevant_flight_level_traffic() {
        let mut state = AdsbSessionState::default();
        let request = state
            .prepare_traffic_request(metrics(), 1_000)
            .expect("first request");
        state
            .ingest(
                &request.id,
                br#"{"now":1000,"ac":[{"hex":"high","alt_geom":35000,"lat":47.45,"lon":-122.31,"seen_pos":0}]}"#,
                1_000,
            )
            .expect("ingest response");

        assert!(state
            .visible_traffic(
                metrics(),
                TrafficOwnshipAltitude {
                    altitude_msl_ft: Some(1_000.0),
                    pressure_altitude_ft: None,
                },
                1_000,
            )
            .is_empty());
    }
}
