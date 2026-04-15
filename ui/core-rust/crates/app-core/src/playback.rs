use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    errors::{AppError, AppErrorKind, AppResult},
    geometry::LatLon,
    situation::{Situation, SituationPosition},
};

const DEFAULT_PLAYBACK_RATE: f64 = 1.0;
const PLAYBACK_PREVIEW_BINS: usize = 160;

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
    pub registration: Option<String>,
    pub icao: Option<String>,
    pub aircraft_type: Option<String>,
    pub point_count: usize,
    pub duration_seconds: f64,
    pub cursor_seconds: f64,
    pub rate: f64,
    pub speed_profile_norm: Vec<Option<f64>>,
    pub altitude_profile_norm: Vec<Option<f64>>,
}

impl Default for PlaybackUiState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Empty,
            source_path: None,
            registration: None,
            icao: None,
            aircraft_type: None,
            point_count: 0,
            duration_seconds: 0.0,
            cursor_seconds: 0.0,
            rate: DEFAULT_PLAYBACK_RATE,
            speed_profile_norm: Vec::new(),
            altitude_profile_norm: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackPoint {
    pub elapsed_seconds: f64,
    pub position: LatLon,
    pub altitude_ft: Option<f64>,
    pub speed_kt: Option<f64>,
    pub orientation_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackTrace {
    pub registration: Option<String>,
    pub icao: Option<String>,
    pub aircraft_type: Option<String>,
    pub points: Vec<PlaybackPoint>,
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
    pub fn load_trace_json(&mut self, source_path: String, trace_json: &str) -> AppResult<Situation> {
        let trace = parse_trace_json(trace_json)?;
        let initial_situation = situation_at_cursor(&trace, 0.0).unwrap_or_default();
        self.trace = Some(trace);
        self.source_path = Some(source_path);
        self.status = PlaybackStatus::Paused;
        self.rate = DEFAULT_PLAYBACK_RATE;
        self.cursor_seconds = 0.0;
        self.anchor_wallclock_epoch_ms = None;
        self.anchor_cursor_seconds = 0.0;
        Ok(initial_situation)
    }

    pub fn play(&mut self, now_epoch_ms: f64) -> Option<Situation> {
        if self.trace.is_none() {
            return None;
        }
        self.status = PlaybackStatus::Playing;
        self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
        self.anchor_cursor_seconds = self.cursor_seconds;
        self.current_situation()
    }

    pub fn pause(&mut self, now_epoch_ms: f64) -> Option<Situation> {
        if self.trace.is_none() {
            return None;
        }
        self.advance_cursor(now_epoch_ms);
        self.status = PlaybackStatus::Paused;
        self.anchor_wallclock_epoch_ms = None;
        self.anchor_cursor_seconds = self.cursor_seconds;
        self.current_situation()
    }

    pub fn seek(&mut self, cursor_seconds: f64, now_epoch_ms: f64) -> Option<Situation> {
        let trace = self.trace.as_ref()?;
        self.cursor_seconds = clamp_cursor(trace, cursor_seconds);
        if self.status == PlaybackStatus::Playing {
            self.anchor_wallclock_epoch_ms = Some(now_epoch_ms);
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.current_situation()
    }

    pub fn set_rate(&mut self, rate: f64, now_epoch_ms: f64) -> Option<Situation> {
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
        self.current_situation()
    }

    pub fn tick(&mut self, now_epoch_ms: f64) -> Option<Situation> {
        let duration_seconds = duration_seconds(self.trace.as_ref()?);
        if self.status != PlaybackStatus::Playing {
            return self.current_situation();
        }
        self.advance_cursor(now_epoch_ms);
        if (self.cursor_seconds - duration_seconds).abs() < 1e-6 {
            self.status = PlaybackStatus::Paused;
            self.anchor_wallclock_epoch_ms = None;
            self.anchor_cursor_seconds = self.cursor_seconds;
        }
        self.current_situation()
    }

    pub fn ui_state(&self) -> PlaybackUiState {
        let Some(trace) = self.trace.as_ref() else {
            return PlaybackUiState::default();
        };
        PlaybackUiState {
            status: self.status.clone(),
            source_path: self.source_path.clone(),
            registration: trace.registration.clone(),
            icao: trace.icao.clone(),
            aircraft_type: trace.aircraft_type.clone(),
            point_count: trace.points.len(),
            duration_seconds: duration_seconds(trace),
            cursor_seconds: self.cursor_seconds,
            rate: self.rate,
            speed_profile_norm: build_profile(trace, |point| point.speed_kt),
            altitude_profile_norm: build_profile(trace, |point| point.altitude_ft),
        }
    }

    pub fn current_situation(&self) -> Option<Situation> {
        let trace = self.trace.as_ref()?;
        situation_at_cursor(trace, self.cursor_seconds)
    }

    fn advance_cursor(&mut self, now_epoch_ms: f64) {
        let Some(trace) = self.trace.as_ref() else {
            return;
        };
        let Some(anchor_wallclock_epoch_ms) = self.anchor_wallclock_epoch_ms else {
            return;
        };
        let elapsed_seconds = ((now_epoch_ms - anchor_wallclock_epoch_ms) / 1000.0).max(0.0);
        self.cursor_seconds = clamp_cursor(trace, self.anchor_cursor_seconds + elapsed_seconds * self.rate);
    }
}

fn parse_trace_json(trace_json: &str) -> AppResult<PlaybackTrace> {
    let value: Value = serde_json::from_str(trace_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("failed to parse playback trace json: {err}"),
    })?;
    let Some(object) = value.as_object() else {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "playback trace root must be a json object".to_string(),
        });
    };
    let registration = object.get("r").and_then(Value::as_str).map(str::to_string);
    let icao = object.get("icao").and_then(Value::as_str).map(str::to_string);
    let aircraft_type = object.get("t").and_then(Value::as_str).map(str::to_string);
    let trace_entries = object.get("trace").and_then(Value::as_array).ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "playback trace missing trace array".to_string(),
    })?;
    let mut points = Vec::new();
    for entry in trace_entries {
        let Some(items) = entry.as_array() else {
            continue;
        };
        if items.len() < 6 {
            continue;
        }
        let Some(elapsed_seconds) = items[0].as_f64() else {
            continue;
        };
        let Some(lat) = items[1].as_f64() else {
            continue;
        };
        let Some(lon) = items[2].as_f64() else {
            continue;
        };
        points.push(PlaybackPoint {
            elapsed_seconds,
            position: LatLon { lat, lon },
            altitude_ft: items[3].as_f64(),
            speed_kt: items[4].as_f64(),
            orientation_deg: items[5].as_f64(),
        });
    }
    if points.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "playback trace contains no usable points".to_string(),
        });
    }
    Ok(PlaybackTrace {
        registration,
        icao,
        aircraft_type,
        points,
    })
}

fn duration_seconds(trace: &PlaybackTrace) -> f64 {
    trace.points.last().map(|point| point.elapsed_seconds).unwrap_or(0.0)
}

fn clamp_cursor(trace: &PlaybackTrace, cursor_seconds: f64) -> f64 {
    cursor_seconds.clamp(0.0, duration_seconds(trace))
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
    let max_value = values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
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
    let t = ((cursor_seconds - lower.elapsed_seconds) / (upper.elapsed_seconds - lower.elapsed_seconds))
        .clamp(0.0, 1.0);
    Some(Situation {
        position: SituationPosition::LatLon {
            lat: interpolate(lower.position.lat, upper.position.lat, t),
            lon: interpolate(lower.position.lon, upper.position.lon, t),
        },
        orientation_deg: interpolate_optional_angle(lower.orientation_deg, upper.orientation_deg, t),
        speed_kt: interpolate_optional(lower.speed_kt, upper.speed_kt, t),
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
}
