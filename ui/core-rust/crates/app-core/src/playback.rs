// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    errors::{AppError, AppErrorKind, AppResult},
    geometry::LatLon,
    ownship::{OwnshipSourceKind, SourceConnectionState},
    situation::{Situation, SituationPosition},
};

const DEFAULT_PLAYBACK_RATE: f64 = 1.0;
const PLAYBACK_TICK_INTERVAL_MS: u32 = 100;
const PLAYBACK_PREVIEW_BINS: usize = 160;
const ADSB_PLAYBACK_GAP_THRESHOLD_SECONDS: f64 = 120.0;
const GPS_CAPTURE_GAP_THRESHOLD_SECONDS: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Empty,
    Paused,
    Playing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackUiState {
    pub status: PlaybackStatus,
    pub source_path: Option<String>,
    pub title_label: String,
    pub registration: Option<String>,
    pub icao: Option<String>,
    pub aircraft_type: Option<String>,
    pub point_count: usize,
    pub duration_seconds: f64,
    pub cursor_seconds: f64,
    pub cursor_label: String,
    pub duration_label: String,
    pub rate: f64,
    pub tick_interval_ms: u32,
    pub speed_profile_norm: Vec<Option<f64>>,
    pub altitude_profile_norm: Vec<Option<f64>>,
    pub gap_spans: Vec<PlaybackGapSpan>,
}

impl Default for PlaybackUiState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Empty,
            source_path: None,
            title_label: "Playback".to_string(),
            registration: None,
            icao: None,
            aircraft_type: None,
            point_count: 0,
            duration_seconds: 0.0,
            cursor_seconds: 0.0,
            cursor_label: format_playback_clock(0.0),
            duration_label: format_playback_clock(0.0),
            rate: DEFAULT_PLAYBACK_RATE,
            tick_interval_ms: PLAYBACK_TICK_INTERVAL_MS,
            speed_profile_norm: Vec::new(),
            altitude_profile_norm: Vec::new(),
            gap_spans: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackGapSpan {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackPoint {
    pub elapsed_seconds: f64,
    pub position: LatLon,
    pub altitude_ft: Option<f64>,
    pub speed_kt: Option<f64>,
    pub orientation_deg: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackGapBehavior {
    Skip,
    PreserveUnavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackStatusEvent {
    pub elapsed_seconds: f64,
    pub connection_state: SourceConnectionState,
    pub status_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackTrace {
    pub title_label: Option<String>,
    pub registration: Option<String>,
    pub icao: Option<String>,
    pub aircraft_type: Option<String>,
    pub source_kind: OwnshipSourceKind,
    pub source_display_name: String,
    gap_behavior: PlaybackGapBehavior,
    gap_threshold_seconds: f64,
    pub points: Vec<PlaybackPoint>,
    pub status_events: Vec<PlaybackStatusEvent>,
    pub gap_spans: Vec<PlaybackGapSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackOwnshipState {
    pub source_kind: OwnshipSourceKind,
    pub display_name: String,
    pub connection_state: SourceConnectionState,
    pub status_label: String,
    pub situation: Option<Situation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSessionState {
    trace: Option<PlaybackTrace>,
    source_path: Option<String>,
    status: PlaybackStatus,
    rate: f64,
    cursor_seconds: f64,
    anchor_wallclock_epoch_ms: Option<f64>,
    anchor_cursor_seconds: f64,
}

impl Default for PlaybackSessionState {
    fn default() -> Self {
        Self {
            trace: None,
            source_path: None,
            status: PlaybackStatus::Empty,
            rate: DEFAULT_PLAYBACK_RATE,
            cursor_seconds: 0.0,
            anchor_wallclock_epoch_ms: None,
            anchor_cursor_seconds: 0.0,
        }
    }
}

impl PlaybackSessionState {
    pub fn load_trace_json(
        &mut self,
        source_path: String,
        trace_json: &str,
    ) -> AppResult<PlaybackOwnshipState> {
        let trace = parse_trace_json(trace_json)?;
        let initial_state = ownship_state_at_cursor(&trace, 0.0);
        self.trace = Some(trace);
        self.source_path = Some(source_path);
        self.status = PlaybackStatus::Paused;
        self.rate = DEFAULT_PLAYBACK_RATE;
        self.cursor_seconds = 0.0;
        self.anchor_wallclock_epoch_ms = None;
        self.anchor_cursor_seconds = 0.0;
        Ok(initial_state)
    }

    pub fn play(&mut self, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        if self.trace.is_none() {
            return None;
        }
        self.status = PlaybackStatus::Playing;
        self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
        self.anchor_cursor_seconds = self.cursor_seconds;
        self.current_ownship_state()
    }

    pub fn pause(&mut self, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        if self.trace.is_none() {
            return None;
        }
        self.advance_cursor(now_epoch_ms);
        self.status = PlaybackStatus::Paused;
        self.anchor_wallclock_epoch_ms = None;
        self.anchor_cursor_seconds = self.cursor_seconds;
        self.current_ownship_state()
    }

    pub fn seek(&mut self, cursor_seconds: f64, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        let trace = self.trace.as_ref()?;
        self.cursor_seconds = skip_gap_at_or_after(trace, clamp_cursor(trace, cursor_seconds));
        if self.status == PlaybackStatus::Playing {
            self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.current_ownship_state()
    }

    pub fn jog(&mut self, delta_seconds: f64, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        let trace = self.trace.as_ref()?;
        self.cursor_seconds = move_cursor_skipping_gaps(trace, self.cursor_seconds, delta_seconds);
        if self.status == PlaybackStatus::Playing {
            self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.current_ownship_state()
    }

    pub fn set_rate(&mut self, rate: f64, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        let _trace = self.trace.as_ref()?;
        let clamped_rate = if rate.is_finite() {
            rate.clamp(0.1, 16.0)
        } else {
            DEFAULT_PLAYBACK_RATE
        };
        if self.status == PlaybackStatus::Playing {
            self.advance_cursor(now_epoch_ms);
            self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.rate = clamped_rate;
        self.current_ownship_state()
    }

    pub fn tick(&mut self, now_epoch_ms: f64) -> Option<PlaybackOwnshipState> {
        let duration_seconds = duration_seconds(self.trace.as_ref()?);
        if self.status != PlaybackStatus::Playing {
            return self.current_ownship_state();
        }
        self.advance_cursor(now_epoch_ms);
        if (self.cursor_seconds - duration_seconds).abs() < 1e-6 {
            self.status = PlaybackStatus::Paused;
            self.anchor_wallclock_epoch_ms = None;
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.current_ownship_state()
    }

    pub fn ui_state(&self) -> PlaybackUiState {
        let Some(trace) = self.trace.as_ref() else {
            return PlaybackUiState::default();
        };
        PlaybackUiState {
            status: self.status.clone(),
            source_path: self.source_path.clone(),
            title_label: trace
                .title_label
                .clone()
                .or_else(|| trace.registration.clone())
                .or_else(|| trace.icao.clone())
                .unwrap_or_else(|| "Trace".to_string()),
            registration: trace.registration.clone(),
            icao: trace.icao.clone(),
            aircraft_type: trace.aircraft_type.clone(),
            point_count: trace.points.len(),
            duration_seconds: duration_seconds(trace),
            cursor_seconds: self.cursor_seconds,
            cursor_label: format_playback_clock(self.cursor_seconds),
            duration_label: format_playback_clock(duration_seconds(trace)),
            rate: self.rate,
            tick_interval_ms: PLAYBACK_TICK_INTERVAL_MS,
            speed_profile_norm: build_profile(trace, |point| point.speed_kt),
            altitude_profile_norm: build_profile(trace, |point| point.altitude_ft),
            gap_spans: trace.gap_spans.clone(),
        }
    }

    pub fn current_ownship_state(&self) -> Option<PlaybackOwnshipState> {
        let trace = self.trace.as_ref()?;
        Some(ownship_state_at_cursor(trace, self.cursor_seconds))
    }

    pub fn current_situation(&self) -> Option<Situation> {
        self.current_ownship_state()?.situation
    }

    fn advance_cursor(&mut self, now_epoch_ms: f64) {
        let Some(trace) = self.trace.as_ref() else {
            return;
        };
        let Some(anchor_wallclock_epoch_ms) = self.anchor_wallclock_epoch_ms else {
            return;
        };
        let elapsed_seconds = ((now_epoch_ms - anchor_wallclock_epoch_ms) / 1000.0).max(0.0);
        let raw_cursor_seconds = clamp_cursor(
            trace,
            self.anchor_cursor_seconds + elapsed_seconds * self.rate,
        );
        let skipped_cursor_seconds = skip_gap_at_or_after(trace, raw_cursor_seconds);
        self.cursor_seconds = skipped_cursor_seconds;
        if (skipped_cursor_seconds - raw_cursor_seconds).abs() > f64::EPSILON {
            self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
            self.anchor_cursor_seconds = skipped_cursor_seconds;
        }
    }
}

fn parse_trace_json(trace_json: &str) -> AppResult<PlaybackTrace> {
    if looks_like_gps_capture_jsonl(trace_json) {
        return parse_gps_capture_jsonl(trace_json);
    }
    let value: Value = match serde_json::from_str(trace_json) {
        Ok(value) => value,
        Err(json_error) => {
            return parse_gps_capture_jsonl(trace_json).map_err(|gps_error| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!(
                    "failed to parse playback trace as ADS-B json ({json_error}) or GPS capture jsonl ({})",
                    gps_error.message
                ),
            });
        }
    };
    if is_gps_capture_value(&value) {
        return parse_gps_capture_jsonl(trace_json);
    }
    parse_adsb_trace_value(value)
}

fn parse_adsb_trace_value(value: Value) -> AppResult<PlaybackTrace> {
    let Some(object) = value.as_object() else {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "playback trace root must be a json object".to_string(),
        });
    };
    let registration = object.get("r").and_then(Value::as_str).map(str::to_string);
    let icao = object
        .get("icao")
        .and_then(Value::as_str)
        .map(str::to_string);
    let aircraft_type = object.get("t").and_then(Value::as_str).map(str::to_string);
    let trace_entries = object
        .get("trace")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "playback trace missing trace array".to_string(),
        })?;
    let mut raw_points = Vec::new();
    for entry in trace_entries {
        let Some(items) = entry.as_array() else {
            continue;
        };
        // adsb.fi trace rows are positional tuples, not self-describing records.
        // Our current field mapping is documented in docs/adsb_fi_trace_format.md:
        //   [0]=seconds since root timestamp [1]=lat [2]=lon [3]=alt_ft [4]=speed_kt [5]=track_deg
        // Some rows also include a named object later in the tuple, but playback
        // does not rely on that auxiliary payload.
        if items.len() < 6 {
            continue;
        }
        let Some(trace_seconds) = items[0].as_f64().filter(|value| value.is_finite()) else {
            continue;
        };
        let Some(lat) = items[1].as_f64().filter(|value| value.is_finite()) else {
            continue;
        };
        let Some(lon) = items[2].as_f64().filter(|value| value.is_finite()) else {
            continue;
        };
        raw_points.push(PlaybackPoint {
            elapsed_seconds: trace_seconds,
            position: LatLon { lat, lon },
            altitude_ft: items[3].as_f64(),
            speed_kt: items[4].as_f64(),
            orientation_deg: items[5].as_f64(),
        });
    }
    normalize_playback_trace(
        raw_points,
        Vec::new(),
        None,
        registration,
        icao,
        aircraft_type,
        OwnshipSourceKind::AdsbTrackPlayback,
        "ADS-B Trace Playback".to_string(),
        PlaybackGapBehavior::Skip,
        ADSB_PLAYBACK_GAP_THRESHOLD_SECONDS,
    )
}

fn parse_gps_capture_jsonl(trace_json: &str) -> AppResult<PlaybackTrace> {
    let mut raw_points = Vec::new();
    let mut raw_status_events = Vec::new();
    let mut parsed_lines = 0usize;
    for line in trace_json
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        let logged_at_epoch_ms = object.get("logged_at_epoch_ms").and_then(Value::as_i64);
        let Some(data) = object.get("data").and_then(Value::as_object) else {
            continue;
        };
        let kind = data.get("kind").and_then(Value::as_str);
        match kind {
            Some("sample") => {
                let Some(sample) = data.get("sample").and_then(Value::as_object) else {
                    continue;
                };
                let event_time_epoch_ms = sample
                    .get("event_time_epoch_ms")
                    .and_then(Value::as_i64)
                    .or(logged_at_epoch_ms);
                let Some(event_time_epoch_ms) = event_time_epoch_ms else {
                    continue;
                };
                let Some(position) = sample.get("position").and_then(Value::as_object) else {
                    continue;
                };
                let Some(lat) = position
                    .get("lat")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                else {
                    continue;
                };
                let Some(lon) = position
                    .get("lon")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                else {
                    continue;
                };
                raw_points.push(PlaybackPoint {
                    elapsed_seconds: event_time_epoch_ms as f64 / 1000.0,
                    position: LatLon { lat, lon },
                    altitude_ft: sample.get("altitude_msl_ft").and_then(Value::as_f64),
                    speed_kt: sample.get("ground_speed_kt").and_then(Value::as_f64),
                    orientation_deg: sample
                        .get("track_deg_true")
                        .and_then(Value::as_f64)
                        .or_else(|| sample.get("heading_deg_true").and_then(Value::as_f64)),
                });
                raw_status_events.push(PlaybackStatusEvent {
                    elapsed_seconds: event_time_epoch_ms as f64 / 1000.0,
                    connection_state: SourceConnectionState::Connected,
                    status_label: "GPS fix".to_string(),
                });
                parsed_lines += 1;
            }
            Some("status") => {
                let Some(update) = data.get("update").and_then(Value::as_object) else {
                    continue;
                };
                let Some(event_time_epoch_ms) = logged_at_epoch_ms else {
                    continue;
                };
                raw_status_events.push(PlaybackStatusEvent {
                    elapsed_seconds: event_time_epoch_ms as f64 / 1000.0,
                    connection_state: parse_connection_state(update.get("connection_state"))
                        .unwrap_or(SourceConnectionState::Unavailable),
                    status_label: update
                        .get("status_label")
                        .and_then(Value::as_str)
                        .unwrap_or("GPS")
                        .to_string(),
                });
                parsed_lines += 1;
            }
            _ => {}
        }
    }
    if parsed_lines == 0 {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "GPS capture contains no usable ownship capture records".to_string(),
        });
    }
    normalize_playback_trace(
        raw_points,
        raw_status_events,
        Some("GPS Capture".to_string()),
        None,
        None,
        None,
        OwnshipSourceKind::GpxPlayback,
        "GPS Capture Playback".to_string(),
        PlaybackGapBehavior::PreserveUnavailable,
        GPS_CAPTURE_GAP_THRESHOLD_SECONDS,
    )
}

#[allow(clippy::too_many_arguments)]
fn normalize_playback_trace(
    mut raw_points: Vec<PlaybackPoint>,
    mut raw_status_events: Vec<PlaybackStatusEvent>,
    title_label: Option<String>,
    registration: Option<String>,
    icao: Option<String>,
    aircraft_type: Option<String>,
    source_kind: OwnshipSourceKind,
    source_display_name: String,
    gap_behavior: PlaybackGapBehavior,
    gap_threshold_seconds: f64,
) -> AppResult<PlaybackTrace> {
    if raw_points.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "playback trace contains no usable points".to_string(),
        });
    }
    raw_points.sort_by(|a, b| a.elapsed_seconds.total_cmp(&b.elapsed_seconds));
    raw_points.dedup_by(|a, b| {
        (a.elapsed_seconds - b.elapsed_seconds).abs() < f64::EPSILON
            && (a.position.lat - b.position.lat).abs() < f64::EPSILON
            && (a.position.lon - b.position.lon).abs() < f64::EPSILON
    });
    raw_status_events.sort_by(|a, b| a.elapsed_seconds.total_cmp(&b.elapsed_seconds));
    let trace_start_seconds = raw_points
        .iter()
        .map(|point| point.elapsed_seconds)
        .chain(raw_status_events.iter().map(|event| event.elapsed_seconds))
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let points: Vec<PlaybackPoint> = raw_points
        .into_iter()
        .map(|mut point| {
            point.elapsed_seconds = (point.elapsed_seconds - trace_start_seconds).max(0.0);
            point
        })
        .collect();
    let status_events: Vec<PlaybackStatusEvent> = raw_status_events
        .into_iter()
        .map(|mut event| {
            event.elapsed_seconds = (event.elapsed_seconds - trace_start_seconds).max(0.0);
            event
        })
        .collect();
    let gap_spans = find_gap_spans(&points, gap_threshold_seconds);
    Ok(PlaybackTrace {
        title_label,
        registration,
        icao,
        aircraft_type,
        source_kind,
        source_display_name,
        gap_behavior,
        gap_threshold_seconds,
        points,
        status_events,
        gap_spans,
    })
}

fn looks_like_gps_capture_jsonl(trace_json: &str) -> bool {
    trace_json.lines().take(8).any(|line| {
        line.contains("ownship.gps_capture.")
            || line.contains("\"kind\":\"sample\"")
            || line.contains("\"kind\":\"status\"")
    })
}

fn is_gps_capture_value(value: &Value) -> bool {
    value
        .get("tag")
        .and_then(Value::as_str)
        .is_some_and(|tag| tag.starts_with("ownship.gps_capture."))
        || value
            .get("data")
            .and_then(|data| data.get("kind"))
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "sample" || kind == "status")
}

fn parse_connection_state(value: Option<&Value>) -> Option<SourceConnectionState> {
    match value.and_then(Value::as_str)? {
        "unavailable" => Some(SourceConnectionState::Unavailable),
        "searching" => Some(SourceConnectionState::Searching),
        "connected" => Some(SourceConnectionState::Connected),
        "stale" => Some(SourceConnectionState::Stale),
        "failed" => Some(SourceConnectionState::Failed),
        _ => None,
    }
}

fn ownship_state_at_cursor(trace: &PlaybackTrace, cursor_seconds: f64) -> PlaybackOwnshipState {
    let status_event = status_event_at_cursor(trace, cursor_seconds);
    let mut connection_state = status_event
        .map(|event| event.connection_state)
        .unwrap_or(SourceConnectionState::Connected);
    let mut status_label = status_event
        .map(|event| event.status_label.clone())
        .unwrap_or_else(|| "Connected".to_string());
    let mut situation = situation_at_cursor(trace, cursor_seconds);
    if trace.gap_behavior == PlaybackGapBehavior::PreserveUnavailable
        && connection_state != SourceConnectionState::Connected
    {
        situation = None;
    }
    if situation.is_none() && connection_state == SourceConnectionState::Connected {
        connection_state = SourceConnectionState::Stale;
        status_label = "Stale".to_string();
    }
    PlaybackOwnshipState {
        source_kind: trace.source_kind,
        display_name: trace.source_display_name.clone(),
        connection_state,
        status_label,
        situation,
    }
}

fn status_event_at_cursor(
    trace: &PlaybackTrace,
    cursor_seconds: f64,
) -> Option<&PlaybackStatusEvent> {
    trace
        .status_events
        .iter()
        .rev()
        .find(|event| event.elapsed_seconds <= cursor_seconds)
}

fn duration_seconds(trace: &PlaybackTrace) -> f64 {
    trace
        .points
        .last()
        .map(|point| point.elapsed_seconds)
        .unwrap_or(0.0)
}

fn clamp_cursor(trace: &PlaybackTrace, cursor_seconds: f64) -> f64 {
    cursor_seconds.clamp(0.0, duration_seconds(trace))
}

fn find_gap_spans(points: &[PlaybackPoint], gap_threshold_seconds: f64) -> Vec<PlaybackGapSpan> {
    points
        .windows(2)
        .filter_map(|window| {
            let start_seconds = window[0].elapsed_seconds;
            let end_seconds = window[1].elapsed_seconds;
            (end_seconds - start_seconds > gap_threshold_seconds).then_some(PlaybackGapSpan {
                start_seconds,
                end_seconds,
            })
        })
        .collect()
}

fn format_playback_clock(seconds: f64) -> String {
    let total_seconds = seconds.floor().max(0.0) as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let remainder = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{remainder:02}")
    } else {
        format!("{minutes}:{remainder:02}")
    }
}

fn skip_gap_at_or_after(trace: &PlaybackTrace, cursor_seconds: f64) -> f64 {
    if trace.gap_behavior != PlaybackGapBehavior::Skip {
        return cursor_seconds;
    }
    for gap in &trace.gap_spans {
        if cursor_seconds > gap.start_seconds && cursor_seconds < gap.end_seconds {
            return gap.end_seconds;
        }
    }
    cursor_seconds
}

fn move_cursor_skipping_gaps(
    trace: &PlaybackTrace,
    cursor_seconds: f64,
    delta_seconds: f64,
) -> f64 {
    if delta_seconds == 0.0 {
        return skip_gap_at_or_after(trace, clamp_cursor(trace, cursor_seconds));
    }
    if delta_seconds > 0.0 {
        move_cursor_forward_skipping_gaps(trace, cursor_seconds, delta_seconds)
    } else {
        move_cursor_backward_skipping_gaps(trace, cursor_seconds, -delta_seconds)
    }
}

fn move_cursor_forward_skipping_gaps(
    trace: &PlaybackTrace,
    cursor_seconds: f64,
    mut remaining_seconds: f64,
) -> f64 {
    if trace.gap_behavior != PlaybackGapBehavior::Skip {
        return clamp_cursor(trace, cursor_seconds + remaining_seconds);
    }
    let mut cursor = skip_gap_at_or_after(trace, clamp_cursor(trace, cursor_seconds));
    for gap in &trace.gap_spans {
        if gap.end_seconds <= cursor {
            continue;
        }
        if cursor < gap.start_seconds {
            let usable_seconds = gap.start_seconds - cursor;
            if remaining_seconds <= usable_seconds {
                return clamp_cursor(trace, cursor + remaining_seconds);
            }
            remaining_seconds -= usable_seconds;
        }
        cursor = gap.end_seconds;
    }
    clamp_cursor(trace, cursor + remaining_seconds)
}

fn move_cursor_backward_skipping_gaps(
    trace: &PlaybackTrace,
    cursor_seconds: f64,
    mut remaining_seconds: f64,
) -> f64 {
    if trace.gap_behavior != PlaybackGapBehavior::Skip {
        return clamp_cursor(trace, cursor_seconds - remaining_seconds);
    }
    let mut cursor = clamp_cursor(trace, cursor_seconds);
    for gap in trace.gap_spans.iter().rev() {
        if gap.start_seconds >= cursor {
            continue;
        }
        if cursor > gap.end_seconds {
            let usable_seconds = cursor - gap.end_seconds;
            if remaining_seconds <= usable_seconds {
                return clamp_cursor(trace, cursor - remaining_seconds);
            }
            remaining_seconds -= usable_seconds;
        }
        cursor = gap.start_seconds;
    }
    clamp_cursor(trace, cursor - remaining_seconds)
}

fn build_profile(
    trace: &PlaybackTrace,
    value_for_point: impl Fn(&PlaybackPoint) -> Option<f64>,
) -> Vec<Option<f64>> {
    let duration = duration_seconds(trace);
    if trace.points.is_empty() || duration <= 0.0 {
        return Vec::new();
    }
    let values: Vec<f64> = trace
        .points
        .iter()
        .filter_map(&value_for_point)
        .filter(|value| value.is_finite())
        .collect();
    if values.is_empty() {
        return vec![None; PLAYBACK_PREVIEW_BINS];
    }
    let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !max_value.is_finite() || max_value <= 0.0 {
        return vec![None; PLAYBACK_PREVIEW_BINS];
    }
    let mut bins: Vec<Option<f64>> = vec![None; PLAYBACK_PREVIEW_BINS];
    for point in &trace.points {
        let Some(value) = value_for_point(point).filter(|value| value.is_finite()) else {
            continue;
        };
        let bin_index = ((point.elapsed_seconds / duration) * ((PLAYBACK_PREVIEW_BINS - 1) as f64))
            .round()
            .clamp(0.0, (PLAYBACK_PREVIEW_BINS - 1) as f64) as usize;
        let normalized = (value / max_value).clamp(0.0, 1.0);
        bins[bin_index] = Some(match bins[bin_index] {
            Some(existing) => existing.max(normalized),
            None => normalized,
        });
    }
    bins
}

fn situation_at_cursor(trace: &PlaybackTrace, cursor_seconds: f64) -> Option<Situation> {
    let first = trace.points.first()?;
    let last = trace.points.last()?;
    if cursor_seconds < first.elapsed_seconds
        && trace.gap_behavior == PlaybackGapBehavior::PreserveUnavailable
    {
        return None;
    }
    if cursor_seconds <= first.elapsed_seconds {
        return Some(point_to_situation(first));
    }
    if cursor_seconds >= last.elapsed_seconds {
        return Some(point_to_situation(last));
    }
    let upper_index = trace
        .points
        .partition_point(|point| point.elapsed_seconds < cursor_seconds);
    let lower = trace.points.get(upper_index.saturating_sub(1))?;
    let upper = trace.points.get(upper_index)?;
    if (upper.elapsed_seconds - lower.elapsed_seconds).abs() < f64::EPSILON {
        return Some(point_to_situation(upper));
    }
    if upper.elapsed_seconds - lower.elapsed_seconds > trace.gap_threshold_seconds {
        return Some(point_to_situation(lower));
    }
    let t = ((cursor_seconds - lower.elapsed_seconds)
        / (upper.elapsed_seconds - lower.elapsed_seconds))
        .clamp(0.0, 1.0);
    Some(Situation {
        position: SituationPosition::LatLon {
            lat: interpolate(lower.position.lat, upper.position.lat, t),
            lon: interpolate(lower.position.lon, upper.position.lon, t),
        },
        orientation_deg: interpolate_optional_angle(
            lower.orientation_deg,
            upper.orientation_deg,
            t,
        ),
        speed_kt: interpolate_optional(lower.speed_kt, upper.speed_kt, t),
        altitude_msl_ft: interpolate_optional(lower.altitude_ft, upper.altitude_ft, t),
    })
}

fn point_to_situation(point: &PlaybackPoint) -> Situation {
    Situation {
        position: SituationPosition::LatLon {
            lat: point.position.lat,
            lon: point.position.lon,
        },
        orientation_deg: point.orientation_deg,
        speed_kt: point.speed_kt,
        altitude_msl_ft: point.altitude_ft,
    }
}

fn interpolate(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn interpolate_optional(start: Option<f64>, end: Option<f64>, t: f64) -> Option<f64> {
    match (start, end) {
        (Some(start), Some(end)) => Some(interpolate(start, end, t)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn interpolate_optional_angle(start: Option<f64>, end: Option<f64>, t: f64) -> Option<f64> {
    match (start, end) {
        (Some(start), Some(end)) => {
            let delta = ((end - start + 540.0) % 360.0) - 180.0;
            Some((start + delta * t + 360.0) % 360.0)
        }
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_trace_to_interpolated_situation() {
        let trace = parse_trace_json(
            r#"{"icao":"a65bbc","r":"N509DT","t":"A21N","trace":[[0.0,10.0,20.0,0,100.0,350.0],[10.0,20.0,40.0,0,200.0,10.0]]}"#,
        )
        .unwrap();
        let situation = situation_at_cursor(&trace, 5.0).unwrap();
        assert_eq!(
            situation.position,
            SituationPosition::LatLon {
                lat: 15.0,
                lon: 30.0,
            }
        );
        assert_eq!(situation.speed_kt, Some(150.0));
        assert!(matches!(situation.orientation_deg, Some(value) if value > 359.0 || value < 1.0));
    }

    #[test]
    fn normalizes_trace_times_to_first_point_and_reports_gaps() {
        let trace = parse_trace_json(
            r#"{"timestamp":1727568000.0,"trace":[[100.0,10.0,20.0,0,100.0,90.0],[110.0,10.1,20.1,0,100.0,90.0],[400.0,11.0,21.0,0,100.0,90.0]]}"#,
        )
        .unwrap();

        assert_eq!(trace.points[0].elapsed_seconds, 0.0);
        assert_eq!(trace.points[1].elapsed_seconds, 10.0);
        assert_eq!(trace.points[2].elapsed_seconds, 300.0);
        assert_eq!(
            trace.gap_spans,
            vec![PlaybackGapSpan {
                start_seconds: 10.0,
                end_seconds: 300.0,
            }]
        );
    }

    #[test]
    fn playback_tick_skips_no_reception_gaps() {
        let mut playback = PlaybackSessionState::default();
        playback
            .load_trace_json(
                "test.json".to_string(),
                r#"{"trace":[[0.0,10.0,20.0,0,100.0,90.0],[10.0,10.1,20.1,0,100.0,90.0],[400.0,11.0,21.0,0,100.0,90.0]]}"#,
            )
            .unwrap();

        playback.play(0.0);
        playback.tick(11_000.0);

        assert_eq!(playback.ui_state().cursor_seconds, 400.0);
        assert_eq!(
            playback.current_situation().unwrap().position,
            SituationPosition::LatLon {
                lat: 11.0,
                lon: 21.0
            },
        );
    }

    #[test]
    fn playback_jog_preserves_motion_across_gaps() {
        let mut playback = PlaybackSessionState::default();
        playback
            .load_trace_json(
                "test.json".to_string(),
                r#"{"trace":[[0.0,10.0,20.0,0,100.0,90.0],[60.0,10.1,20.1,0,100.0,90.0],[400.0,11.0,21.0,0,100.0,90.0],[520.0,12.0,22.0,0,100.0,90.0]]}"#,
            )
            .unwrap();

        playback.seek(30.0, 0.0);
        playback.jog(120.0, 0.0);
        let cursor = playback.ui_state().cursor_seconds;
        assert!(
            (cursor - 490.0).abs() < 1e-6,
            "forward jog should spend remaining distance after the dead region, got {cursor}",
        );

        playback.jog(-120.0, 0.0);
        let cursor = playback.ui_state().cursor_seconds;
        assert!(
            (cursor - 30.0).abs() < 1e-6,
            "backward jog should spend remaining distance before the dead region, got {cursor}",
        );
    }

    #[test]
    fn parses_gps_capture_jsonl_as_replay_trace() {
        let trace = parse_trace_json(
            r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"searching","status_label":"Searching"}},"logged_at_epoch_ms":1000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix 7 m"}},"logged_at_epoch_ms":2000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":2000,"received_time_epoch_ms":2000,"source_kind":"device_gps","position":{"lat":47.0,"lon":-122.0},"altitude_msl_ft":100.0,"ground_speed_kt":30.0,"track_deg_true":90.0}},"logged_at_epoch_ms":2000,"tag":"ownship.gps_capture.sample"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"searching","status_label":"Searching"}},"logged_at_epoch_ms":5000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix 9 m"}},"logged_at_epoch_ms":13000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":13000,"received_time_epoch_ms":13000,"source_kind":"device_gps","position":{"lat":47.1,"lon":-122.1},"altitude_msl_ft":200.0,"ground_speed_kt":40.0,"track_deg_true":100.0}},"logged_at_epoch_ms":13000,"tag":"ownship.gps_capture.sample"}"#,
        )
        .unwrap();

        assert_eq!(trace.source_kind, OwnshipSourceKind::GpxPlayback);
        assert_eq!(trace.title_label.as_deref(), Some("GPS Capture"));
        assert_eq!(
            trace.gap_spans,
            vec![PlaybackGapSpan {
                start_seconds: 1.0,
                end_seconds: 12.0,
            }]
        );
        let before_fix = ownship_state_at_cursor(&trace, 0.0);
        assert_eq!(
            before_fix.connection_state,
            SourceConnectionState::Searching
        );
        assert!(before_fix.situation.is_none());

        let in_tunnel = ownship_state_at_cursor(&trace, 6.0);
        assert_eq!(in_tunnel.connection_state, SourceConnectionState::Searching);
        assert!(in_tunnel.situation.is_none());

        let after_tunnel = ownship_state_at_cursor(&trace, 12.0);
        assert_eq!(
            after_tunnel.connection_state,
            SourceConnectionState::Connected
        );
        assert_eq!(
            after_tunnel.situation.unwrap().position,
            SituationPosition::LatLon {
                lat: 47.1,
                lon: -122.1,
            }
        );
    }

    #[test]
    fn gps_capture_playback_does_not_skip_gaps() {
        let mut playback = PlaybackSessionState::default();
        playback
            .load_trace_json(
                "gps.jsonl".to_string(),
                r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":0,"received_time_epoch_ms":0,"source_kind":"device_gps","position":{"lat":10.0,"lon":20.0},"altitude_msl_ft":100.0}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.sample"}
{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"searching","status_label":"Searching"}},"logged_at_epoch_ms":6000,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":20000,"received_time_epoch_ms":20000,"source_kind":"device_gps","position":{"lat":11.0,"lon":21.0},"altitude_msl_ft":200.0}},"logged_at_epoch_ms":20000,"tag":"ownship.gps_capture.sample"}"#,
            )
            .unwrap();

        playback.play(0.0);
        let state = playback.tick(7_000.0).unwrap();
        assert_eq!(playback.ui_state().cursor_seconds, 7.0);
        assert_eq!(state.connection_state, SourceConnectionState::Searching);
        assert!(state.situation.is_none());
    }

    #[test]
    fn gps_capture_connected_status_keeps_ownship_between_slow_samples() {
        let mut playback = PlaybackSessionState::default();
        playback
            .load_trace_json(
                "gps.jsonl".to_string(),
                r#"{"data":{"kind":"status","source_kind":"device_gps","update":{"connection_state":"connected","status_label":"GPS fix"}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.status"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":0,"received_time_epoch_ms":0,"source_kind":"device_gps","position":{"lat":10.0,"lon":20.0},"altitude_msl_ft":100.0}},"logged_at_epoch_ms":0,"tag":"ownship.gps_capture.sample"}
{"data":{"kind":"sample","sample":{"event_time_epoch_ms":6200,"received_time_epoch_ms":6200,"source_kind":"device_gps","position":{"lat":10.1,"lon":20.1},"altitude_msl_ft":120.0}},"logged_at_epoch_ms":6200,"tag":"ownship.gps_capture.sample"}"#,
            )
            .unwrap();

        playback.play(0.0);
        let state = playback.tick(3_000.0).unwrap();
        assert_eq!(playback.ui_state().cursor_seconds, 3.0);
        assert_eq!(state.connection_state, SourceConnectionState::Connected);
        assert_eq!(
            state.situation.unwrap().position,
            SituationPosition::LatLon {
                lat: 10.0,
                lon: 20.0,
            },
        );
    }
}
