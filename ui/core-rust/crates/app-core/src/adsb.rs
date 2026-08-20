// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::ownship::OwnshipTextAction;
use crate::{
    great_circle_distance_nm, CoreResourceRequest, CoreResourceSource, LatLon, MapSelectionAction,
    MapSelectionCategory, MapSelectionHighlight, MapSelectionItem, MapSelectionSessionAction,
    MapSurfaceMetrics, OwnshipSourceId, OwnshipSourceKind, SituationSample, SourceConnectionState,
};

const AIRPLANES_LIVE_BASE_URL: &str = "https://api.airplanes.live/v2";
const TRAFFIC_RESOURCE_PREFIX: &str = "adsb/airplanes_live/traffic/";
const OWNSHIP_RESOURCE_PREFIX: &str = "adsb/airplanes_live/ownship/";
pub(crate) const INTERNET_ADSB_SOURCE_ID: &str = "internet-adsb";
pub(crate) const FOLLOW_ADSB_TARGET_ACTION_ID: &str = "ownship:follow_adsb_target";
const TRAFFIC_POLL_INTERVAL_MS: i64 = 60_000;
const OWNSHIP_POLL_INTERVAL_MS: i64 = 60_000;
pub(crate) const OWNSHIP_STALE_AFTER_MS: i64 = OWNSHIP_POLL_INTERVAL_MS * 2;
const REQUEST_MIN_INTERVAL_MS: i64 = 1_100;
const FAILURE_BACKOFF_MAX_MS: i64 = 65_000;
const MIN_QUERY_RADIUS_NM: f64 = 10.0;
const MAX_QUERY_RADIUS_NM: f64 = 250.0;
const COVERAGE_MARGIN: f64 = 1.15;
const MAX_TRAFFIC_RECORDS: usize = 5_000;
const MAX_TRAFFIC_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_OWNSHIP_RESPONSE_BYTES: u64 = 512 * 1024;
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
    pub detail_label: String,
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

#[derive(Debug, Clone, PartialEq)]
struct PendingOwnshipRequest {
    resource_id: String,
    registration: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdsbOwnshipLastResult {
    Observed,
    NoReport,
    NoPosition,
    StalePosition,
}

impl AdsbOwnshipLastResult {
    fn label(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::NoReport => "no report",
            Self::NoPosition => "no current position",
            Self::StalePosition => "stale position",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AdsbOwnshipUpdate {
    pub connection_state: SourceConnectionState,
    pub status_label: String,
    pub sample: Option<SituationSample>,
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
    ownship_registration: Option<String>,
    ownship_pending: Option<PendingOwnshipRequest>,
    ownship_next_poll_epoch_ms: Option<i64>,
    ownship_consecutive_failures: u32,
    ownship_last_result_epoch_ms: Option<i64>,
    ownship_last_result: Option<AdsbOwnshipLastResult>,
    ownship_last_error: Option<String>,
}

impl AdsbSessionState {
    pub(crate) fn handles_resource(resource_id: &str) -> bool {
        resource_id.starts_with(TRAFFIC_RESOURCE_PREFIX)
            || resource_id.starts_with(OWNSHIP_RESOURCE_PREFIX)
    }

    pub(crate) fn is_ownship_resource(resource_id: &str) -> bool {
        resource_id.starts_with(OWNSHIP_RESOURCE_PREFIX)
    }

    pub(crate) fn ownship_registration(&self) -> Option<&str> {
        self.ownship_registration.as_deref()
    }

    pub(crate) fn set_ownship_registration(&mut self, value: &str) -> Result<String, String> {
        let registration = normalize_registration_input(value)?;
        if self.ownship_registration.as_deref() != Some(registration.as_str()) {
            self.ownship_registration = Some(registration.clone());
            self.ownship_pending = None;
            self.ownship_next_poll_epoch_ms = None;
            self.ownship_consecutive_failures = 0;
            self.ownship_last_result_epoch_ms = None;
            self.ownship_last_result = None;
            self.ownship_last_error = None;
        }
        Ok(registration)
    }

    pub(crate) fn ownship_text_action(&self, selected: bool) -> Option<OwnshipTextAction> {
        selected.then(|| OwnshipTextAction {
            action_id: FOLLOW_ADSB_TARGET_ACTION_ID.to_string(),
            label: "Aircraft registration".to_string(),
            value: self.ownship_registration.clone().unwrap_or_default(),
            placeholder: "N1234".to_string(),
            submit_label: "Follow".to_string(),
            enabled: true,
            disabled_reason: None,
        })
    }

    pub(crate) fn prepare_ownship_request(&mut self, epoch_ms: i64) -> Option<CoreResourceRequest> {
        let registration = self.ownship_registration.clone()?;
        if self.ownship_pending.is_some()
            || self
                .ownship_next_poll_epoch_ms
                .is_some_and(|deadline| epoch_ms < deadline)
            || !self.provider_request_is_due(epoch_ms)
        {
            return None;
        }
        self.next_generation = self.next_generation.saturating_add(1);
        let resource_id = format!("{OWNSHIP_RESOURCE_PREFIX}{}", self.next_generation);
        self.ownship_pending = Some(PendingOwnshipRequest {
            resource_id: resource_id.clone(),
            registration: registration.clone(),
        });
        self.last_request_epoch_ms = Some(epoch_ms);
        Some(CoreResourceRequest {
            id: resource_id,
            source: CoreResourceSource::PublicUrl {
                url: format!("{AIRPLANES_LIVE_BASE_URL}/reg/{registration}"),
            },
            optional: false,
            max_response_bytes: Some(MAX_OWNSHIP_RESPONSE_BYTES),
        })
    }

    pub(crate) fn ownship_next_refresh_epoch_ms(&self, epoch_ms: i64) -> Option<i64> {
        self.ownship_registration.as_ref()?;
        if self.ownship_pending.is_some() {
            return None;
        }
        Some(
            self.ownship_next_poll_epoch_ms
                .unwrap_or(epoch_ms)
                .max(self.provider_throttle_deadline())
                .max(epoch_ms),
        )
    }

    pub(crate) fn ownship_status_detail(&self, epoch_ms: i64) -> String {
        let mut lines = vec![
            "source: Airplanes.live".to_string(),
            format!("polling every {}s", OWNSHIP_POLL_INTERVAL_MS / 1_000),
            match self.ownship_last_result_epoch_ms {
                Some(result_epoch_ms) => format!(
                    "last result {} ago",
                    format_ownship_result_age(epoch_ms.saturating_sub(result_epoch_ms))
                ),
                None => "last result never".to_string(),
            },
        ];
        if let Some(registration) = self.ownship_registration.as_deref() {
            let result = self
                .ownship_last_result
                .map(AdsbOwnshipLastResult::label)
                .unwrap_or("no report");
            lines.push(format!("{registration}: {result}"));
        }
        if let Some(error) = self.ownship_last_error.as_deref() {
            lines.push(format!("last error: {error}"));
        }
        lines.join("\n")
    }

    pub(crate) fn ownship_last_error(&self) -> Option<&str> {
        self.ownship_last_error.as_deref()
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
        let throttle_deadline = self.provider_throttle_deadline();
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
    ) -> Result<Option<AdsbOwnshipUpdate>, String> {
        if resource_id.starts_with(OWNSHIP_RESOURCE_PREFIX) {
            return self.ingest_ownship(resource_id, bytes, received_epoch_ms);
        }
        let Some(pending) = self.pending.as_ref() else {
            return Ok(None);
        };
        if pending.resource_id != resource_id {
            return Ok(None);
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
        Ok(None)
    }

    fn ingest_ownship(
        &mut self,
        resource_id: &str,
        bytes: &[u8],
        received_epoch_ms: i64,
    ) -> Result<Option<AdsbOwnshipUpdate>, String> {
        let Some(pending) = self.ownship_pending.as_ref() else {
            return Ok(Some(AdsbOwnshipUpdate {
                connection_state: SourceConnectionState::Searching,
                status_label: "Waiting for ADS-B target".to_string(),
                sample: None,
            }));
        };
        if pending.resource_id != resource_id {
            return Ok(Some(AdsbOwnshipUpdate {
                connection_state: SourceConnectionState::Searching,
                status_label: format!("Searching for {}", pending.registration),
                sample: None,
            }));
        }
        let registration = pending.registration.clone();
        let payload: AirplanesLiveResponse = serde_json::from_slice(bytes).map_err(|error| {
            format!("failed to decode Airplanes.live aircraft response: {error}")
        })?;
        if payload.ac.len() > MAX_TRAFFIC_RECORDS {
            return Err(format!(
                "Airplanes.live aircraft response contains {} aircraft; limit is {MAX_TRAFFIC_RECORDS}",
                payload.ac.len()
            ));
        }
        let source_epoch_ms = payload.now.unwrap_or(received_epoch_ms);
        let aircraft = payload
            .ac
            .into_iter()
            .filter_map(|record| normalize_aircraft(record, source_epoch_ms))
            .find(|aircraft| {
                aircraft
                    .registration
                    .as_deref()
                    .is_some_and(|candidate| normalize_registration(candidate) == registration)
            });
        let (result, update) = match aircraft {
            None => (
                AdsbOwnshipLastResult::NoReport,
                AdsbOwnshipUpdate {
                    connection_state: SourceConnectionState::Searching,
                    status_label: format!("{registration} is not currently visible"),
                    sample: None,
                },
            ),
            Some(aircraft) => match aircraft.position {
                None => (
                    AdsbOwnshipLastResult::NoPosition,
                    AdsbOwnshipUpdate {
                        connection_state: SourceConnectionState::Searching,
                        status_label: format!("{registration} has no current position"),
                        sample: None,
                    },
                ),
                Some(position) => {
                    let event_time_epoch_ms = aircraft.position_epoch_ms.unwrap_or(source_epoch_ms);
                    if received_epoch_ms.saturating_sub(event_time_epoch_ms)
                        > MAX_VISIBLE_POSITION_AGE_MS
                    {
                        (
                            AdsbOwnshipLastResult::StalePosition,
                            AdsbOwnshipUpdate {
                                connection_state: SourceConnectionState::Stale,
                                status_label: format!("{registration} position is stale"),
                                sample: None,
                            },
                        )
                    } else {
                        (
                            AdsbOwnshipLastResult::Observed,
                            AdsbOwnshipUpdate {
                                connection_state: SourceConnectionState::Connected,
                                status_label: format!("Following {registration}"),
                                sample: Some(SituationSample {
                                    source_id: OwnshipSourceId(INTERNET_ADSB_SOURCE_ID.to_string()),
                                    source_kind: OwnshipSourceKind::LiveNetworkTrack,
                                    event_time_epoch_ms,
                                    received_time_epoch_ms: received_epoch_ms,
                                    position: Some(position),
                                    horizontal_accuracy_m: None,
                                    vertical_accuracy_m: None,
                                    track_deg_true: aircraft.track_deg_true,
                                    heading_deg_true: None,
                                    ground_speed_kt: aircraft.ground_speed_kt,
                                    altitude_msl_ft: aircraft.altitude_msl_ft,
                                    pressure_altitude_ft: aircraft.pressure_altitude_ft,
                                    vertical_speed_fpm: aircraft.vertical_speed_fpm,
                                }),
                            },
                        )
                    }
                }
            },
        };
        self.complete_ownship_poll(received_epoch_ms, result);
        Ok(Some(update))
    }

    fn complete_ownship_poll(&mut self, received_epoch_ms: i64, result: AdsbOwnshipLastResult) {
        self.ownship_pending = None;
        self.ownship_next_poll_epoch_ms =
            Some(received_epoch_ms.saturating_add(OWNSHIP_POLL_INTERVAL_MS));
        self.ownship_consecutive_failures = 0;
        self.ownship_last_result_epoch_ms = Some(received_epoch_ms);
        self.ownship_last_result = Some(result);
        self.ownship_last_error = None;
    }

    pub(crate) fn record_failure(&mut self, resource_id: &str, message: &str, epoch_ms: i64) {
        if resource_id.starts_with(OWNSHIP_RESOURCE_PREFIX) {
            self.record_ownship_failure(resource_id, message, epoch_ms);
            return;
        }
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

    fn record_ownship_failure(&mut self, resource_id: &str, message: &str, epoch_ms: i64) {
        if self
            .ownship_pending
            .as_ref()
            .is_none_or(|pending| pending.resource_id != resource_id)
        {
            return;
        }
        self.ownship_pending = None;
        self.ownship_consecutive_failures = self.ownship_consecutive_failures.saturating_add(1);
        let exponent = self.ownship_consecutive_failures.saturating_sub(1).min(5);
        let delay_ms = OWNSHIP_POLL_INTERVAL_MS
            .saturating_mul(1_i64 << exponent)
            .min(FAILURE_BACKOFF_MAX_MS);
        self.ownship_next_poll_epoch_ms = Some(epoch_ms.saturating_add(delay_ms));
        self.ownship_last_error = Some(message.to_string());
    }

    fn provider_throttle_deadline(&self) -> i64 {
        self.last_request_epoch_ms
            .map(|last| last.saturating_add(REQUEST_MIN_INTERVAL_MS))
            .unwrap_or(i64::MIN)
    }

    fn provider_request_is_due(&self, epoch_ms: i64) -> bool {
        epoch_ms >= self.provider_throttle_deadline()
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
                let detail_label = relative_altitude_ft
                    .map(format_relative_altitude)
                    .unwrap_or(altitude_label);
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
                    detail_label,
                })
            })
            .collect()
    }

    pub(crate) fn traffic_selection_category(
        &self,
        metrics: MapSurfaceMetrics,
        ownship: TrafficOwnshipAltitude,
        click: LatLon,
        epoch_ms: i64,
    ) -> MapSelectionCategory {
        let (click_x, click_y) = metrics.project_position(click);
        let mut matches = self
            .visible_traffic(metrics, ownship, epoch_ms)
            .into_iter()
            .filter_map(|visible| {
                let distance_px = ((visible.screen_x - click_x).powi(2)
                    + (visible.screen_y - click_y).powi(2))
                .sqrt();
                if distance_px > metrics.inspector_hit_radius_px() {
                    return None;
                }
                let aircraft = self
                    .aircraft
                    .iter()
                    .find(|aircraft| aircraft.id == visible.id)?;
                let registration = aircraft.registration.clone();
                let follow_action = registration.as_ref().map(|registration| {
                    serde_json::to_string(&MapSelectionSessionAction::FollowAdsbRegistration {
                        registration: registration.clone(),
                    })
                    .expect("ADS-B map action must serialize")
                });
                let detail = &visible.detail_label;
                Some((
                    distance_px,
                    MapSelectionItem {
                        id: format!("adsb:{}", aircraft.id),
                        label: registration
                            .clone()
                            .or_else(|| aircraft.callsign.clone())
                            .unwrap_or_else(|| aircraft.id.to_ascii_uppercase()),
                        sublabel: "ADS-B traffic".to_string(),
                        description: Some(format!("{}  {detail}", visible.label)),
                        distance: None,
                        secondary_description: aircraft.aircraft_type.clone(),
                        position: None,
                        elevation_msl_ft: None,
                        detail_text: None,
                        highlight: MapSelectionHighlight::AdsbTraffic {
                            id: aircraft.id.clone(),
                        },
                        nav_ref: None,
                        symbol_feature: None,
                        metar_feature: None,
                        weather_detail: None,
                        automatic_action_uid: None,
                        pirep_feature: None,
                        airspace_icon: None,
                        actions: vec![MapSelectionAction {
                            id: "follow_as_ownship".to_string(),
                            label: "Follow".to_string(),
                            enabled: follow_action.is_some(),
                            display_only: false,
                            action_uid: None,
                            placeholder: false,
                            detail_text: None,
                            detail_title: None,
                            detail_status: None,
                            weather_detail: None,
                            airport_info_airport_id: None,
                            disabled_reason: follow_action
                                .is_none()
                                .then(|| "Aircraft registration is unavailable".to_string()),
                            airspace_limit: None,
                            session_action: follow_action,
                            flight_plan_row_action: None,
                            navigation: None,
                            external_url: None,
                        }],
                    },
                ))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.0.total_cmp(&right.0));
        MapSelectionCategory {
            id: "adsb_traffic".to_string(),
            label: "Traffic".to_string(),
            items: matches.into_iter().map(|(_, item)| item).collect(),
        }
    }
}

fn format_ownship_result_age(age_ms: i64) -> String {
    let age_ms = age_ms.max(0);
    if age_ms < 60_000 {
        format!("{}s", age_ms / 1_000)
    } else {
        crate::freshness::format_age(age_ms)
    }
}

fn normalize_registration_input(value: &str) -> Result<String, String> {
    let normalized = normalize_registration(value);
    if !(2..=12).contains(&normalized.len())
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(
            "Aircraft registration must contain 2-12 letters, digits, or hyphens".to_string(),
        );
    }
    Ok(normalized)
}

fn normalize_registration(value: &str) -> String {
    value.trim().to_ascii_uppercase()
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
        assert_eq!(visible[0].detail_label, "+10");
    }

    #[test]
    fn visible_traffic_selection_uses_the_follow_ownship_session_action() {
        let mut state = AdsbSessionState::default();
        let request = state
            .prepare_traffic_request(metrics(), 1_000)
            .expect("traffic request");
        state
            .ingest(
                &request.id,
                br#"{"now":1000,"ac":[{"hex":"abc123","flight":"TEST1","r":"N9124Y","t":"C172","alt_geom":4300,"lat":47.45,"lon":-122.31,"seen_pos":0}]}"#,
                1_000,
            )
            .expect("traffic response");

        let category = state.traffic_selection_category(
            metrics(),
            TrafficOwnshipAltitude::default(),
            LatLon {
                lat: 47.45,
                lon: -122.31,
            },
            1_000,
        );
        let item = category.items.first().expect("selected traffic");
        assert_eq!(item.label, "N9124Y");
        assert!(matches!(
            item.highlight,
            MapSelectionHighlight::AdsbTraffic { ref id } if id == "abc123"
        ));
        let action: MapSelectionSessionAction = serde_json::from_str(
            item.actions[0]
                .session_action
                .as_deref()
                .expect("follow action"),
        )
        .expect("decode follow action");
        assert_eq!(
            action,
            MapSelectionSessionAction::FollowAdsbRegistration {
                registration: "N9124Y".to_string(),
            }
        );
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
        assert_eq!(state.next_refresh_epoch_ms(metrics(), 10_500), Some(70_500));
        assert!(state.prepare_traffic_request(metrics(), 70_499).is_none());
        assert!(state.prepare_traffic_request(metrics(), 70_500).is_some());
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

    #[test]
    fn registration_query_produces_a_normalized_ownship_sample() {
        let mut state = AdsbSessionState::default();
        assert_eq!(
            state.set_ownship_registration(" n9124y ").expect("target"),
            "N9124Y"
        );
        let request = state.prepare_ownship_request(10_000).expect("request");
        assert_eq!(
            request.source,
            CoreResourceSource::PublicUrl {
                url: "https://api.airplanes.live/v2/reg/N9124Y".to_string(),
            }
        );
        let outcome = state
            .ingest(
                &request.id,
                br#"{"now":10500,"ac":[{"hex":"abc123","r":"N9124Y","alt_baro":4200,"alt_geom":4300,"gs":131,"track":87,"geom_rate":512,"lat":47.45,"lon":-122.31,"seen_pos":0.2}]}"#,
                10_600,
            )
            .expect("ingest");
        let update = outcome.expect("expected ownship update");
        assert_eq!(update.connection_state, SourceConnectionState::Connected);
        let sample = update.sample.expect("sample");
        assert_eq!(sample.source_id.0, INTERNET_ADSB_SOURCE_ID);
        assert_eq!(sample.source_kind, OwnshipSourceKind::LiveNetworkTrack);
        assert_eq!(
            sample.position,
            Some(LatLon {
                lat: 47.45,
                lon: -122.31
            })
        );
        assert_eq!(sample.track_deg_true, Some(87.0));
        assert_eq!(sample.ground_speed_kt, Some(131.0));
        assert_eq!(sample.altitude_msl_ft, Some(4300.0));
        assert_eq!(state.ownship_next_refresh_epoch_ms(10_600), Some(70_600));
        assert_eq!(
            state.ownship_status_detail(53_600),
            "source: Airplanes.live\npolling every 60s\nlast result 43s ago\nN9124Y: observed"
        );
    }

    #[test]
    fn empty_registration_response_is_a_successful_no_report_result() {
        let mut state = AdsbSessionState::default();
        state.set_ownship_registration("N1234").expect("target");
        let request = state.prepare_ownship_request(10_000).expect("request");
        let outcome = state
            .ingest(&request.id, br#"{"now":10500,"ac":[]}"#, 10_600)
            .expect("ingest");
        let update = outcome.expect("expected ownship update");
        assert_eq!(update.connection_state, SourceConnectionState::Searching);
        assert!(update.sample.is_none());
        assert_eq!(
            state.ownship_status_detail(12_600),
            "source: Airplanes.live\npolling every 60s\nlast result 2s ago\nN1234: no report"
        );
    }

    #[test]
    fn matching_aircraft_without_usable_position_is_not_reported_as_observed() {
        let mut state = AdsbSessionState::default();
        state.set_ownship_registration("N9124Y").expect("target");
        let request = state.prepare_ownship_request(10_000).expect("request");
        let outcome = state
            .ingest(
                &request.id,
                br#"{"now":10500,"ac":[{"hex":"abc123","r":"N9124Y","alt_baro":4200}]}"#,
                10_600,
            )
            .expect("ingest");
        let update = outcome.expect("expected ownship update");
        assert_eq!(update.connection_state, SourceConnectionState::Searching);
        assert!(update.sample.is_none());
        assert_eq!(
            state.ownship_status_detail(10_600),
            "source: Airplanes.live\npolling every 60s\nlast result 0s ago\nN9124Y: no current position"
        );
    }

    #[test]
    fn matching_aircraft_with_stale_position_is_not_reported_as_observed() {
        let mut state = AdsbSessionState::default();
        state.set_ownship_registration("N9124Y").expect("target");
        let request = state.prepare_ownship_request(10_000).expect("request");
        let outcome = state
            .ingest(
                &request.id,
                br#"{"now":10500,"ac":[{"hex":"abc123","r":"N9124Y","lat":47.45,"lon":-122.31,"seen_pos":16}]}"#,
                10_600,
            )
            .expect("ingest");
        let update = outcome.expect("expected ownship update");
        assert_eq!(update.connection_state, SourceConnectionState::Stale);
        assert!(update.sample.is_none());
        assert_eq!(
            state.ownship_status_detail(10_600),
            "source: Airplanes.live\npolling every 60s\nlast result 0s ago\nN9124Y: stale position"
        );
    }

    #[test]
    fn traffic_and_ownship_queries_share_the_provider_throttle() {
        let mut state = AdsbSessionState::default();
        state.set_ownship_registration("N9124Y").expect("target");
        assert!(state.prepare_ownship_request(10_000).is_some());
        assert!(state.prepare_traffic_request(metrics(), 10_500).is_none());
    }

    #[test]
    fn registration_input_rejects_url_shaped_values() {
        let mut state = AdsbSessionState::default();
        assert!(state
            .set_ownship_registration("N9124Y/point/47/-122")
            .is_err());
    }
}
