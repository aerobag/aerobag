// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, LocalResult, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use app_ui_contracts::session::FlightEstimateKind;

use crate::{
    great_circle_distance_nm, great_circle_intermediate, initial_course_deg, FlightDataCell,
    FlightPlanRowId, LatLon, TimeDisplayMode,
};

const MAX_INTEGRATION_STEP_NM: f64 = 1.0;
const MAX_INTEGRATION_STEP_SECONDS: f64 = 30.0;
const ALTITUDE_CAPTURE_FT: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltitudePlannerUnavailableReasonCode {
    AircraftProfileUnavailable,
    CruiseAltitudeUnavailable,
    PlanOriginAltitudeUnavailable,
    PlanDestinationAltitudeUnavailable,
    OwnshipAltitudeUnavailable,
    WindModelUnavailable,
    PerformanceRegimeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerUnavailableReason {
    pub code: AltitudePlannerUnavailableReasonCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltitudePlannerControlId {
    Aircraft,
    AircraftProfile,
    WindModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltitudePlannerDepartureInputField {
    Time,
    When,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerDepartureEditorUiView {
    pub title: String,
    pub time_label: String,
    pub time_value: String,
    pub basis_label: String,
    pub time_display_action_id: String,
    pub when_label: String,
    pub when_value: String,
    pub when_suffix: String,
    pub when_is_past: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerControlUiView {
    pub id: AltitudePlannerControlId,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AltitudePlannerControlOptionUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerControlOptionUiView {
    pub label: String,
    pub action_uid: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudeComparisonUiView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_uid: Option<String>,
    pub selected: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
    pub cells: Vec<FlightDataCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudeComparisonPanelUiView {
    pub columns: Vec<crate::FlightDataColumn>,
    pub rows: Vec<AltitudeComparisonUiView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerForecastUiView {
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltitudePlannerUiView {
    pub title: String,
    pub estimate_kind: FlightEstimateKind,
    pub estimate_summary: FlightPlanEstimateModeUiView,
    pub controls: Vec<AltitudePlannerControlUiView>,
    pub departure: AltitudePlannerDepartureEditorUiView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forecast: Option<AltitudePlannerForecastUiView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_reasons: Vec<AltitudePlannerUnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightPlanEstimateModeUiView {
    pub label: String,
    pub estimate_kind: FlightEstimateKind,
}

impl Default for AltitudePlannerUiView {
    fn default() -> Self {
        project_altitude_planner_ui(AltitudePlannerUiInput::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltitudePlannerUiInput {
    pub aircraft_label: Option<String>,
    pub aircraft_profile_label: Option<String>,
    pub aircraft_options: Vec<AltitudePlannerControlOptionUiView>,
    pub aircraft_profile_options: Vec<AltitudePlannerControlOptionUiView>,
    pub cruise_altitude_ft: Option<i32>,
    pub navigation_active: bool,
    pub ownship_altitude_available: bool,
    pub plan_origin_altitude_available: bool,
    pub plan_destination_altitude_available: bool,
    pub wind_model_label: String,
    pub wind_model_selected: bool,
    pub wind_model_available: bool,
    pub wind_model_selectable: bool,
    pub wind_model_action_uid: Option<String>,
    pub modeled_prediction_error: Option<String>,
    pub live_ground_speed_estimate_active: bool,
    pub departure_time_epoch_ms: Option<i64>,
    pub effective_departure_time_epoch_ms: i64,
    pub now_epoch_ms: i64,
    pub time_display_mode: TimeDisplayMode,
    pub local_time_zone: Tz,
    pub forecast: Option<AltitudePlannerForecastUiView>,
    pub wind_fallback: Option<AltitudePlannerWindFallback>,
    pub aircraft_advisory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AltitudePlannerWindFallback {
    ForecastCoverage {
        valid_from_epoch_ms: i64,
        valid_through_epoch_ms: i64,
        flight_start_epoch_ms: i64,
        flight_end_epoch_ms: i64,
    },
    Other {
        reason: String,
    },
}

impl Default for AltitudePlannerUiInput {
    fn default() -> Self {
        Self {
            aircraft_label: None,
            aircraft_profile_label: None,
            aircraft_options: Vec::new(),
            aircraft_profile_options: Vec::new(),
            cruise_altitude_ft: None,
            navigation_active: false,
            ownship_altitude_available: false,
            plan_origin_altitude_available: false,
            plan_destination_altitude_available: false,
            wind_model_label: "NO WIND".to_string(),
            wind_model_selected: false,
            wind_model_available: true,
            wind_model_selectable: false,
            wind_model_action_uid: None,
            modeled_prediction_error: None,
            live_ground_speed_estimate_active: false,
            departure_time_epoch_ms: None,
            effective_departure_time_epoch_ms: 0,
            now_epoch_ms: 0,
            time_display_mode: TimeDisplayMode::Local,
            local_time_zone: chrono_tz::UTC,
            forecast: None,
            wind_fallback: None,
            aircraft_advisory: None,
        }
    }
}

pub fn project_altitude_planner_ui(input: AltitudePlannerUiInput) -> AltitudePlannerUiView {
    let mut reasons = Vec::new();
    if input.aircraft_profile_label.is_none() {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::AircraftProfileUnavailable,
            "No aircraft performance profile is selected.",
        ));
    }
    if input.cruise_altitude_ft.is_none() {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::CruiseAltitudeUnavailable,
            "No cruise altitude is selected.",
        ));
    }
    if input.navigation_active && !input.ownship_altitude_available {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::OwnshipAltitudeUnavailable,
            "Active navigation has no ownship altitude.",
        ));
    }
    if !input.navigation_active && !input.plan_origin_altitude_available {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::PlanOriginAltitudeUnavailable,
            "The flight plan origin has no known elevation.",
        ));
    }
    if !input.plan_destination_altitude_available {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::PlanDestinationAltitudeUnavailable,
            "The flight plan destination has no known elevation.",
        ));
    }
    if input.wind_model_selected && !input.wind_model_available {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::WindModelUnavailable,
            "The selected wind model is unavailable for this route and departure time.",
        ));
    }
    if let Some(message) = input.modeled_prediction_error.as_deref() {
        reasons.push(reason(
            AltitudePlannerUnavailableReasonCode::PerformanceRegimeUnavailable,
            message,
        ));
    }

    let estimate_kind = if input.live_ground_speed_estimate_active {
        FlightEstimateKind::Basic
    } else if reasons.is_empty() {
        FlightEstimateKind::Modeled
    } else {
        FlightEstimateKind::Basic
    };
    let departure = project_departure_editor(&input);
    let aircraft_label = input
        .aircraft_label
        .unwrap_or_else(|| "DEFAULT".to_string());
    let aircraft_profile_label = input
        .aircraft_profile_label
        .unwrap_or_else(|| "BASIC".to_string());
    let estimate_summary_label = if input.live_ground_speed_estimate_active {
        "Estimate basis:\nGS extrapolated".to_string()
    } else if estimate_kind == FlightEstimateKind::Modeled {
        let wind_basis = if input.wind_fallback.is_some() {
            "No-wind fallback"
        } else {
            match input.wind_model_label.as_str() {
                "FORECAST" => "Forecast winds",
                "NO WIND" => "No wind",
                label => label,
            }
        };
        format!(
            "Estimate basis:\n{}\n{} cruise",
            wind_basis,
            input
                .cruise_altitude_ft
                .map(format_altitude_ft)
                .unwrap_or_else(|| "—".to_string())
        )
    } else {
        "Estimate basis:\nBasic estimate".to_string()
    };
    let forecast = match input.wind_fallback.as_ref() {
        Some(AltitudePlannerWindFallback::ForecastCoverage {
            valid_from_epoch_ms,
            valid_through_epoch_ms,
            flight_start_epoch_ms,
            flight_end_epoch_ms,
        }) => Some(AltitudePlannerForecastUiView {
            summary: if flight_start_epoch_ms < valid_from_epoch_ms {
                format!(
                    "Currently available forecast product valid from {}, flight starts at {}. Using NO-WIND model.",
                    format_planner_clock(
                        *valid_from_epoch_ms,
                        input.time_display_mode,
                        input.local_time_zone,
                    ),
                    format_planner_clock(
                        *flight_start_epoch_ms,
                        input.time_display_mode,
                        input.local_time_zone,
                    ),
                )
            } else {
                format!(
                    "Currently available forecast product valid until {}, flight extends until {}. Using NO-WIND model.",
                    format_planner_clock(
                        *valid_through_epoch_ms,
                        input.time_display_mode,
                        input.local_time_zone,
                    ),
                    format_planner_clock(
                        *flight_end_epoch_ms,
                        input.time_display_mode,
                        input.local_time_zone,
                    ),
                )
            },
        }),
        _ => input.forecast,
    };
    let mut advisories = match input.wind_fallback.as_ref() {
        Some(AltitudePlannerWindFallback::Other { reason }) => {
            vec![format!(
                "Forecast winds do not cover the complete trajectory ({reason}); showing no-wind/ISA estimates."
            )]
        }
        _ => Vec::new(),
    };
    advisories.extend(input.aircraft_advisory);
    AltitudePlannerUiView {
        title: "Altitude Planner".to_string(),
        estimate_kind,
        estimate_summary: FlightPlanEstimateModeUiView {
            label: estimate_summary_label,
            estimate_kind,
        },
        controls: vec![
            AltitudePlannerControlUiView {
                id: AltitudePlannerControlId::Aircraft,
                label: format!("AIRCRAFT\n{aircraft_label}"),
                enabled: !input.aircraft_options.is_empty(),
                action_uid: None,
                disabled_reason: input
                    .aircraft_options
                    .is_empty()
                    .then(|| "No alternate aircraft definitions are available.".to_string()),
                options: input.aircraft_options,
            },
            AltitudePlannerControlUiView {
                id: AltitudePlannerControlId::AircraftProfile,
                label: format!("PROFILE\n{aircraft_profile_label}"),
                enabled: !input.aircraft_profile_options.is_empty(),
                action_uid: None,
                disabled_reason: input
                    .aircraft_profile_options
                    .is_empty()
                    .then(|| "No alternate performance profiles are available.".to_string()),
                options: input.aircraft_profile_options,
            },
            AltitudePlannerControlUiView {
                id: AltitudePlannerControlId::WindModel,
                label: format!("WIND\n{}", input.wind_model_label),
                enabled: input.wind_model_selectable,
                action_uid: input.wind_model_action_uid,
                disabled_reason: (!input.wind_model_selectable)
                    .then(|| "No alternate wind models are available.".to_string()),
                options: Vec::new(),
            },
        ],
        departure,
        forecast,
        advisories,
        unavailable_reasons: reasons,
    }
}

fn format_planner_clock(epoch_ms: i64, basis: TimeDisplayMode, local_time_zone: Tz) -> String {
    crate::format_time_of_day(
        epoch_ms,
        basis,
        local_time_zone,
        crate::TimeOfDayStyle::Compact,
    )
    .with_basis()
}

fn project_departure_editor(
    input: &AltitudePlannerUiInput,
) -> AltitudePlannerDepartureEditorUiView {
    let effective = DateTime::<Utc>::from_timestamp_millis(input.effective_departure_time_epoch_ms)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let displayed_time = crate::format_time_of_day(
        effective.timestamp_millis(),
        input.time_display_mode,
        input.local_time_zone,
        crate::TimeOfDayStyle::Compact,
    );
    let time_value = displayed_time.value;
    let basis_label = displayed_time.basis_label;
    let when_value = input
        .departure_time_epoch_ms
        .map(|epoch_ms| format_departure_offset(epoch_ms - input.now_epoch_ms))
        .unwrap_or_else(|| "now".to_string());
    let when_is_past = input
        .departure_time_epoch_ms
        .is_some_and(|epoch_ms| epoch_ms < input.now_epoch_ms);
    let disabled_reason = input
        .navigation_active
        .then(|| "Active navigation always models from ownship at the current time.".to_string());
    AltitudePlannerDepartureEditorUiView {
        title: "Depart:".to_string(),
        time_label: String::new(),
        time_value,
        basis_label,
        time_display_action_id: crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string(),
        when_label: "=".to_string(),
        when_value,
        when_suffix: "from now".to_string(),
        when_is_past,
        enabled: !input.navigation_active,
        disabled_reason,
    }
}

fn format_departure_offset(offset_ms: i64) -> String {
    let negative = offset_ms < 0;
    let total_minutes = (offset_ms.unsigned_abs().saturating_add(30_000) / 60_000) as u64;
    if total_minutes == 0 {
        return "now".to_string();
    }
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    let value = parts.join(" ");
    if negative {
        format!("−{value}")
    } else {
        value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedAltitudePlannerDeparture {
    pub departure_time_epoch_ms: Option<i64>,
    pub basis: TimeDisplayMode,
}

pub fn parse_altitude_planner_departure_input(
    field: AltitudePlannerDepartureInputField,
    raw_input: &str,
    now_epoch_ms: i64,
    current_basis: TimeDisplayMode,
    local_time_zone: Tz,
) -> Result<ParsedAltitudePlannerDeparture, String> {
    match field {
        AltitudePlannerDepartureInputField::Time => {
            parse_departure_clock_time(raw_input, now_epoch_ms, current_basis, local_time_zone)
        }
        AltitudePlannerDepartureInputField::When => Ok(ParsedAltitudePlannerDeparture {
            departure_time_epoch_ms: parse_departure_offset(raw_input, now_epoch_ms)?,
            basis: current_basis,
        }),
    }
}

fn parse_departure_clock_time(
    raw_input: &str,
    now_epoch_ms: i64,
    current_basis: TimeDisplayMode,
    local_time_zone: Tz,
) -> Result<ParsedAltitudePlannerDeparture, String> {
    let mut value = raw_input.trim().to_ascii_lowercase();
    if value == "now" {
        return Ok(ParsedAltitudePlannerDeparture {
            departure_time_epoch_ms: None,
            basis: current_basis,
        });
    }
    let basis = if let Some(stripped) = value.strip_suffix("local") {
        value = stripped.trim_end().to_string();
        TimeDisplayMode::Local
    } else if let Some(stripped) = value.strip_suffix("lcl") {
        value = stripped.trim_end().to_string();
        TimeDisplayMode::Local
    } else if let Some(stripped) = value.strip_suffix('z') {
        value = stripped.trim_end().to_string();
        TimeDisplayMode::Utc
    } else {
        current_basis
    };
    let time = parse_clock_value(&value)?;
    let now = DateTime::<Utc>::from_timestamp_millis(now_epoch_ms)
        .ok_or_else(|| "current time is outside the supported UTC range".to_string())?;
    let candidate = match basis {
        TimeDisplayMode::Utc => {
            next_utc_clock_occurrence(now, time).map(|value| value.timestamp_millis())?
        }
        TimeDisplayMode::Local => next_local_clock_occurrence(now, time, local_time_zone)
            .map(|value| value.timestamp_millis())?,
    };
    Ok(ParsedAltitudePlannerDeparture {
        departure_time_epoch_ms: Some(candidate),
        basis,
    })
}

fn parse_clock_value(value: &str) -> Result<NaiveTime, String> {
    let (hours, minutes) = if let Some((hours, minutes)) = value.split_once(':') {
        if minutes.contains(':') {
            return Err("enter departure time as HHMM or HH:MM".to_string());
        }
        (hours, minutes)
    } else {
        if value.len() < 3 || value.len() > 4 {
            return Err("enter departure time as HHMM or HH:MM".to_string());
        }
        value.split_at(value.len() - 2)
    };
    let hours = hours
        .parse::<u32>()
        .map_err(|_| "departure hour is not a number".to_string())?;
    let minutes = minutes
        .parse::<u32>()
        .map_err(|_| "departure minute is not a number".to_string())?;
    NaiveTime::from_hms_opt(hours, minutes, 0)
        .ok_or_else(|| "departure time must be between 0000 and 2359".to_string())
}

fn next_utc_clock_occurrence(now: DateTime<Utc>, time: NaiveTime) -> Result<DateTime<Utc>, String> {
    let minute_floor = now
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| "current UTC time is invalid".to_string())?;
    let mut date = now.date_naive();
    for _ in 0..2 {
        let candidate = Utc.from_utc_datetime(&date.and_time(time));
        if candidate >= minute_floor {
            return Ok(candidate);
        }
        date = date
            .succ_opt()
            .ok_or_else(|| "departure date is outside the supported range".to_string())?;
    }
    Err("departure date is outside the supported range".to_string())
}

fn next_local_clock_occurrence(
    now: DateTime<Utc>,
    time: NaiveTime,
    time_zone: Tz,
) -> Result<DateTime<Utc>, String> {
    let local_now = now.with_timezone(&time_zone);
    let minute_floor = now
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| "current UTC time is invalid".to_string())?;
    let mut date = local_now.date_naive();
    for attempt in 0..2 {
        let naive = date.and_time(time);
        let candidates = match time_zone.from_local_datetime(&naive) {
            LocalResult::Single(candidate) => vec![candidate.with_timezone(&Utc)],
            LocalResult::Ambiguous(first, second) => {
                let mut candidates = vec![first.with_timezone(&Utc), second.with_timezone(&Utc)];
                candidates.sort();
                candidates
            }
            LocalResult::None if attempt == 0 => {
                return Err(format!(
                    "{} does not exist in {} because of a local clock change",
                    time.format("%H%M"),
                    time_zone
                ));
            }
            LocalResult::None => Vec::new(),
        };
        if let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| *candidate >= minute_floor)
        {
            return Ok(candidate);
        }
        date = date
            .succ_opt()
            .ok_or_else(|| "departure date is outside the supported range".to_string())?;
    }
    Err("departure date is outside the supported range".to_string())
}

fn parse_departure_offset(raw_input: &str, now_epoch_ms: i64) -> Result<Option<i64>, String> {
    let input = raw_input.trim().to_ascii_lowercase();
    if input == "now" || input == "0" || input == "0m" || input == "0h" {
        return Ok(None);
    }
    let (negative, input) = if let Some(value) = input.strip_prefix('-') {
        (true, value.trim_start())
    } else if let Some(value) = input.strip_prefix('−') {
        (true, value.trim_start())
    } else {
        (
            false,
            input.strip_prefix('+').unwrap_or(&input).trim_start(),
        )
    };
    let bytes = input.as_bytes();
    let mut cursor = 0;
    let mut total_seconds = 0.0;
    let mut terms = 0;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let number_start = cursor;
        while cursor < bytes.len() && (bytes[cursor].is_ascii_digit() || bytes[cursor] == b'.') {
            cursor += 1;
        }
        if number_start == cursor {
            return Err("enter a departure offset such as 3h or 1h 30m".to_string());
        }
        let amount = input[number_start..cursor]
            .parse::<f64>()
            .map_err(|_| "departure offset contains an invalid number".to_string())?;
        if !amount.is_finite() || amount < 0.0 {
            return Err("departure offset must be a nonnegative duration".to_string());
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let unit_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        let unit = if unit_start == cursor && cursor == bytes.len() && terms == 0 {
            // A lone number is useful shorthand in this compact hours-oriented field.
            "h"
        } else {
            &input[unit_start..cursor]
        };
        let multiplier = match unit {
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600.0,
            "d" | "day" | "days" => 86_400.0,
            _ => return Err("departure offset units must be minutes, hours, or days".to_string()),
        };
        total_seconds += amount * multiplier;
        terms += 1;
    }
    if terms == 0 || !total_seconds.is_finite() {
        return Err("enter a departure offset such as 3h or 1h 30m".to_string());
    }
    let magnitude_ms = (total_seconds * 1_000.0).round();
    if magnitude_ms > i64::MAX as f64 {
        return Err("departure offset is too large".to_string());
    }
    let offset_ms = if negative {
        -(magnitude_ms as i64)
    } else {
        magnitude_ms as i64
    };
    let departure = now_epoch_ms
        .checked_add(offset_ms)
        .ok_or_else(|| "departure time is outside the supported UTC range".to_string())?;
    DateTime::<Utc>::from_timestamp_millis(departure)
        .ok_or_else(|| "departure time is outside the supported UTC range".to_string())?;
    Ok((offset_ms != 0).then_some(departure))
}

fn format_altitude_ft(altitude_ft: i32) -> String {
    let digits = altitude_ft.abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if altitude_ft < 0 {
        grouped.insert(0, '-');
    }
    grouped
}

pub(crate) fn format_trajectory_planner_error(
    profile: &AircraftPerformanceProfile,
    error: &TrajectoryPlannerError,
) -> String {
    if let TrajectoryPlannerError::PerformanceOutOfRange { phase, altitude_ft } = error {
        let bounds = match *phase {
            "climb" => profile
                .climb
                .first()
                .zip(profile.climb.last())
                .map(|(first, last)| (first.pressure_altitude_ft, last.pressure_altitude_ft)),
            "cruise" => profile
                .cruise
                .first()
                .zip(profile.cruise.last())
                .map(|(first, last)| (first.pressure_altitude_ft, last.pressure_altitude_ft)),
            "descent" => profile
                .descent
                .first()
                .zip(profile.descent.last())
                .map(|(first, last)| (first.pressure_altitude_ft, last.pressure_altitude_ft)),
            _ => None,
        };
        if let Some((minimum_ft, maximum_ft)) = bounds {
            return format!(
                "{} {phase} performance covers {}–{} ft; this flight requires {} ft.",
                profile.profile_label,
                format_altitude_ft(minimum_ft.round() as i32),
                format_altitude_ft(maximum_ft.round() as i32),
                format_altitude_ft(altitude_ft.round() as i32),
            );
        }
    }
    format!("Modeled prediction is unavailable: {error}.")
}

fn reason(
    code: AltitudePlannerUnavailableReasonCode,
    message: &str,
) -> AltitudePlannerUnavailableReason {
    AltitudePlannerUnavailableReason {
        code,
        message: message.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CruisePerformancePoint {
    pub pressure_altitude_ft: f64,
    pub true_airspeed_kt: f64,
    pub fuel_flow_gph: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceAirspeedBasis {
    Indicated,
    True,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VerticalPerformancePoint {
    pub pressure_altitude_ft: f64,
    pub airspeed_basis: PerformanceAirspeedBasis,
    pub airspeed_kt: f64,
    pub fuel_flow_gph: f64,
    /// Positive values climb and negative values descend.
    pub vertical_speed_fpm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AircraftPerformanceProfile {
    pub schema_version: u32,
    pub aircraft_model_id: String,
    pub profile_id: String,
    pub profile_version: String,
    pub aircraft_label: String,
    pub profile_label: String,
    pub source: String,
    pub reference_weight_lb: Option<f64>,
    pub cruise: Vec<CruisePerformancePoint>,
    pub climb: Vec<VerticalPerformancePoint>,
    pub descent: Vec<VerticalPerformancePoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereSample {
    pub wind_east_kt: f64,
    pub wind_north_kt: f64,
    pub temperature_c: f64,
}

pub trait AtmosphereModel {
    fn sample(
        &self,
        position: LatLon,
        pressure_altitude_ft: f64,
        epoch_ms: i64,
    ) -> Result<AtmosphereSample, String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoWindIsaAtmosphere;

impl AtmosphereModel for NoWindIsaAtmosphere {
    fn sample(
        &self,
        _position: LatLon,
        pressure_altitude_ft: f64,
        _epoch_ms: i64,
    ) -> Result<AtmosphereSample, String> {
        Ok(AtmosphereSample {
            wind_east_kt: 0.0,
            wind_north_kt: 0.0,
            temperature_c: 15.0 - pressure_altitude_ft * 0.001_981_2,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryRouteLeg {
    pub row_id: FlightPlanRowId,
    pub path: Vec<LatLon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryPlanInput {
    pub legs: Vec<TrajectoryRouteLeg>,
    pub start_pressure_altitude_ft: f64,
    pub cruise_pressure_altitude_ft: f64,
    pub destination_pressure_altitude_ft: f64,
    pub departure_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryLegPrediction {
    pub row_id: FlightPlanRowId,
    pub segment_ete_seconds: f64,
    pub cumulative_ete_seconds: f64,
    pub segment_fuel_gal: f64,
    pub cumulative_fuel_gal: f64,
    pub ending_pressure_altitude_ft: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryPrediction {
    pub estimate_kind: FlightEstimateKind,
    pub legs: Vec<TrajectoryLegPrediction>,
    pub total_ete_seconds: f64,
    pub total_fuel_gal: f64,
    pub maximum_pressure_altitude_ft: f64,
    pub average_wind_east_kt: f64,
    pub average_wind_north_kt: f64,
    pub average_along_course_wind_kt: f64,
}

pub fn format_trajectory_wind(prediction: &TrajectoryPrediction) -> String {
    let speed_kt = prediction
        .average_wind_east_kt
        .hypot(prediction.average_wind_north_kt);
    let arrow = if speed_kt < 0.5 {
        "·"
    } else {
        const ARROWS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];
        let toward_deg = prediction
            .average_wind_east_kt
            .atan2(prediction.average_wind_north_kt)
            .to_degrees()
            .rem_euclid(360.0);
        ARROWS[((toward_deg + 22.5) / 45.0).floor() as usize % ARROWS.len()]
    };
    let component_kt = prediction.average_along_course_wind_kt.round() as i32;
    if speed_kt < 0.5 {
        arrow.to_string()
    } else if component_kt == 0 {
        format!("{arrow} 0")
    } else if component_kt > 0 {
        format!("{arrow} +{component_kt}")
    } else {
        format!("{arrow} −{}", component_kt.abs())
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum TrajectoryPlannerError {
    #[error("trajectory route contains no flyable geometry")]
    EmptyRoute,
    #[error("invalid {table} performance table: {message}")]
    InvalidPerformanceTable {
        table: &'static str,
        message: String,
    },
    #[error("{phase} performance does not cover {altitude_ft:.0} ft")]
    PerformanceOutOfRange {
        phase: &'static str,
        altitude_ft: f64,
    },
    #[error("atmosphere unavailable: {0}")]
    AtmosphereUnavailable(String),
    #[error("crosswind equals or exceeds true airspeed")]
    WindExceedsAirspeed,
    #[error("computed ground speed is not positive")]
    NonPositiveGroundSpeed,
}

#[derive(Debug, Clone, Copy)]
struct PhasePerformance {
    true_airspeed_kt: f64,
    fuel_flow_gph: f64,
    vertical_speed_fpm: f64,
}

#[derive(Debug, Clone, Copy)]
enum FlightPhase {
    Climb { target_altitude_ft: f64 },
    Cruise,
    Descent { target_altitude_ft: f64 },
}

pub struct TrajectoryPlanner<'a, A: AtmosphereModel + ?Sized> {
    profile: &'a AircraftPerformanceProfile,
    atmosphere: &'a A,
}

impl<'a, A: AtmosphereModel + ?Sized> TrajectoryPlanner<'a, A> {
    pub fn new(profile: &'a AircraftPerformanceProfile, atmosphere: &'a A) -> Self {
        Self {
            profile,
            atmosphere,
        }
    }

    pub fn predict(
        &self,
        input: &TrajectoryPlanInput,
    ) -> Result<TrajectoryPrediction, TrajectoryPlannerError> {
        validate_profile(self.profile)?;
        interpolate_cruise(&self.profile.cruise, input.cruise_pressure_altitude_ft)?;
        let route_distance_nm = input
            .legs
            .iter()
            .flat_map(|leg| leg.path.windows(2))
            .map(|segment| great_circle_distance_nm(segment[0], segment[1]))
            .sum::<f64>();
        if route_distance_nm <= f64::EPSILON {
            return Err(TrajectoryPlannerError::EmptyRoute);
        }

        let mut altitude_ft = input.start_pressure_altitude_ft;
        let mut maximum_altitude_ft = altitude_ft;
        let mut elapsed_seconds: f64 = 0.0;
        let mut fuel_gal: f64 = 0.0;
        let mut remaining_route_nm = route_distance_nm;
        let mut wind_east_nm_kt = 0.0;
        let mut wind_north_nm_kt = 0.0;
        let mut along_course_wind_nm_kt = 0.0;
        let mut terminal_descent_started = false;
        let mut predictions = Vec::with_capacity(input.legs.len());

        for leg in &input.legs {
            let leg_start_seconds = elapsed_seconds;
            let leg_start_fuel = fuel_gal;
            for edge in leg.path.windows(2) {
                let mut position = edge[0];
                let edge_end = edge[1];
                let mut edge_remaining_nm = great_circle_distance_nm(position, edge_end);
                while edge_remaining_nm > 1.0e-6 {
                    let atmosphere = self
                        .atmosphere
                        .sample(
                            position,
                            altitude_ft,
                            input.departure_epoch_ms + (elapsed_seconds * 1000.0).round() as i64,
                        )
                        .map_err(TrajectoryPlannerError::AtmosphereUnavailable)?;
                    let phase = if terminal_descent_started {
                        FlightPhase::Descent {
                            target_altitude_ft: input.destination_pressure_altitude_ft,
                        }
                    } else {
                        self.flight_phase(
                            altitude_ft,
                            input.cruise_pressure_altitude_ft,
                            input.destination_pressure_altitude_ft,
                            remaining_route_nm,
                            atmosphere.temperature_c,
                        )?
                    };
                    if matches!(
                        phase,
                        FlightPhase::Descent { target_altitude_ft }
                            if target_altitude_ft == input.destination_pressure_altitude_ft
                    ) {
                        // Top-of-descent is a one-way phase transition. Without
                        // this latch, reaching destination altitude a little early
                        // would incorrectly command another climb to cruise.
                        terminal_descent_started = true;
                    }
                    let performance =
                        self.performance(phase, altitude_ft, atmosphere.temperature_c)?;
                    let course_deg = initial_course_deg(position, edge_end);
                    let ground_speed_kt = ground_speed_along_course(
                        performance.true_airspeed_kt,
                        course_deg,
                        atmosphere.wind_east_kt,
                        atmosphere.wind_north_kt,
                    )?;
                    let time_limited_nm = ground_speed_kt * MAX_INTEGRATION_STEP_SECONDS / 3600.0;
                    let step_nm = edge_remaining_nm
                        .min(MAX_INTEGRATION_STEP_NM)
                        .min(time_limited_nm);
                    let step_seconds = step_nm / ground_speed_kt * 3600.0;
                    let course_rad = course_deg.to_radians();
                    let along_course_wind_kt = atmosphere.wind_east_kt * course_rad.sin()
                        + atmosphere.wind_north_kt * course_rad.cos();

                    elapsed_seconds += step_seconds;
                    fuel_gal += performance.fuel_flow_gph * step_seconds / 3600.0;
                    wind_east_nm_kt += atmosphere.wind_east_kt * step_nm;
                    wind_north_nm_kt += atmosphere.wind_north_kt * step_nm;
                    along_course_wind_nm_kt += along_course_wind_kt * step_nm;
                    altitude_ft = advance_altitude(
                        altitude_ft,
                        performance.vertical_speed_fpm,
                        step_seconds,
                        phase,
                    );
                    maximum_altitude_ft = maximum_altitude_ft.max(altitude_ft);

                    let fraction = (step_nm / edge_remaining_nm).clamp(0.0, 1.0);
                    position = great_circle_intermediate(position, edge_end, fraction);
                    edge_remaining_nm = (edge_remaining_nm - step_nm).max(0.0);
                    remaining_route_nm = (remaining_route_nm - step_nm).max(0.0);
                }
            }
            predictions.push(TrajectoryLegPrediction {
                row_id: leg.row_id.clone(),
                segment_ete_seconds: elapsed_seconds - leg_start_seconds,
                cumulative_ete_seconds: elapsed_seconds,
                segment_fuel_gal: fuel_gal - leg_start_fuel,
                cumulative_fuel_gal: fuel_gal,
                ending_pressure_altitude_ft: altitude_ft,
            });
        }

        Ok(TrajectoryPrediction {
            estimate_kind: FlightEstimateKind::Modeled,
            legs: predictions,
            total_ete_seconds: elapsed_seconds,
            total_fuel_gal: fuel_gal,
            maximum_pressure_altitude_ft: maximum_altitude_ft,
            average_wind_east_kt: wind_east_nm_kt / route_distance_nm,
            average_wind_north_kt: wind_north_nm_kt / route_distance_nm,
            average_along_course_wind_kt: along_course_wind_nm_kt / route_distance_nm,
        })
    }

    fn flight_phase(
        &self,
        altitude_ft: f64,
        cruise_altitude_ft: f64,
        destination_altitude_ft: f64,
        remaining_route_nm: f64,
        temperature_c: f64,
    ) -> Result<FlightPhase, TrajectoryPlannerError> {
        if altitude_ft > destination_altitude_ft + ALTITUDE_CAPTURE_FT {
            let descent =
                interpolate_vertical(&self.profile.descent, altitude_ft, temperature_c, "descent")?;
            let descent_minutes =
                (altitude_ft - destination_altitude_ft) / -descent.vertical_speed_fpm;
            let descent_distance_nm = descent.true_airspeed_kt * descent_minutes / 60.0;
            if remaining_route_nm <= descent_distance_nm {
                return Ok(FlightPhase::Descent {
                    target_altitude_ft: destination_altitude_ft,
                });
            }
        }
        if altitude_ft < cruise_altitude_ft - ALTITUDE_CAPTURE_FT {
            Ok(FlightPhase::Climb {
                target_altitude_ft: cruise_altitude_ft,
            })
        } else if altitude_ft > cruise_altitude_ft + ALTITUDE_CAPTURE_FT {
            Ok(FlightPhase::Descent {
                target_altitude_ft: cruise_altitude_ft,
            })
        } else {
            Ok(FlightPhase::Cruise)
        }
    }

    fn performance(
        &self,
        phase: FlightPhase,
        altitude_ft: f64,
        temperature_c: f64,
    ) -> Result<PhasePerformance, TrajectoryPlannerError> {
        match phase {
            FlightPhase::Climb { .. } => {
                interpolate_vertical(&self.profile.climb, altitude_ft, temperature_c, "climb")
            }
            FlightPhase::Cruise => interpolate_cruise(&self.profile.cruise, altitude_ft),
            FlightPhase::Descent { .. } => {
                interpolate_vertical(&self.profile.descent, altitude_ft, temperature_c, "descent")
            }
        }
    }
}

fn validate_profile(profile: &AircraftPerformanceProfile) -> Result<(), TrajectoryPlannerError> {
    validate_cruise_table(&profile.cruise)?;
    validate_vertical_table(&profile.climb, "climb", |rate| rate > 0.0)?;
    validate_vertical_table(&profile.descent, "descent", |rate| rate < 0.0)
}

fn validate_cruise_table(points: &[CruisePerformancePoint]) -> Result<(), TrajectoryPlannerError> {
    if points.is_empty() {
        return Err(invalid_table("cruise", "table is empty"));
    }
    validate_altitudes(
        "cruise",
        points.iter().map(|point| point.pressure_altitude_ft),
    )?;
    if points.iter().any(|point| {
        !point.true_airspeed_kt.is_finite()
            || point.true_airspeed_kt <= 0.0
            || !point.fuel_flow_gph.is_finite()
            || point.fuel_flow_gph <= 0.0
    }) {
        return Err(invalid_table(
            "cruise",
            "airspeed and fuel flow must be finite and positive",
        ));
    }
    Ok(())
}

fn validate_vertical_table(
    points: &[VerticalPerformancePoint],
    table: &'static str,
    valid_rate: impl Fn(f64) -> bool,
) -> Result<(), TrajectoryPlannerError> {
    if points.is_empty() {
        return Err(invalid_table(table, "table is empty"));
    }
    validate_altitudes(table, points.iter().map(|point| point.pressure_altitude_ft))?;
    let airspeed_basis = points[0].airspeed_basis;
    if points.iter().any(|point| {
        point.airspeed_basis != airspeed_basis
            || !point.airspeed_kt.is_finite()
            || point.airspeed_kt <= 0.0
            || !point.fuel_flow_gph.is_finite()
            || point.fuel_flow_gph <= 0.0
            || !point.vertical_speed_fpm.is_finite()
            || !valid_rate(point.vertical_speed_fpm)
    }) {
        return Err(invalid_table(
            table,
            "airspeed basis must be uniform, airspeed and fuel flow must be positive, and vertical speed has the wrong sign",
        ));
    }
    Ok(())
}

fn validate_altitudes(
    table: &'static str,
    altitudes: impl Iterator<Item = f64>,
) -> Result<(), TrajectoryPlannerError> {
    let mut previous = None;
    for altitude in altitudes {
        if !altitude.is_finite() || previous.is_some_and(|value| altitude <= value) {
            return Err(invalid_table(
                table,
                "altitudes must be finite and strictly increasing",
            ));
        }
        previous = Some(altitude);
    }
    Ok(())
}

fn invalid_table(table: &'static str, message: &str) -> TrajectoryPlannerError {
    TrajectoryPlannerError::InvalidPerformanceTable {
        table,
        message: message.to_string(),
    }
}

fn interpolate_cruise(
    points: &[CruisePerformancePoint],
    altitude_ft: f64,
) -> Result<PhasePerformance, TrajectoryPlannerError> {
    let (lower, upper, fraction) = interpolation_bounds(points, altitude_ft, "cruise", |point| {
        point.pressure_altitude_ft
    })?;
    Ok(PhasePerformance {
        true_airspeed_kt: lerp(lower.true_airspeed_kt, upper.true_airspeed_kt, fraction),
        fuel_flow_gph: lerp(lower.fuel_flow_gph, upper.fuel_flow_gph, fraction),
        vertical_speed_fpm: 0.0,
    })
}

fn interpolate_vertical(
    points: &[VerticalPerformancePoint],
    altitude_ft: f64,
    temperature_c: f64,
    phase: &'static str,
) -> Result<PhasePerformance, TrajectoryPlannerError> {
    let (lower, upper, fraction) = interpolation_bounds(points, altitude_ft, phase, |point| {
        point.pressure_altitude_ft
    })?;
    if lower.airspeed_basis != upper.airspeed_basis {
        return Err(invalid_table(
            phase,
            "airspeed basis changes within the table",
        ));
    }
    let scheduled_airspeed_kt = lerp(lower.airspeed_kt, upper.airspeed_kt, fraction);
    let true_airspeed_kt = match lower.airspeed_basis {
        PerformanceAirspeedBasis::Indicated => {
            indicated_to_true_airspeed(scheduled_airspeed_kt, altitude_ft, temperature_c)?
        }
        PerformanceAirspeedBasis::True => scheduled_airspeed_kt,
    };
    Ok(PhasePerformance {
        true_airspeed_kt,
        fuel_flow_gph: lerp(lower.fuel_flow_gph, upper.fuel_flow_gph, fraction),
        vertical_speed_fpm: lerp(lower.vertical_speed_fpm, upper.vertical_speed_fpm, fraction),
    })
}

fn indicated_to_true_airspeed(
    indicated_airspeed_kt: f64,
    pressure_altitude_ft: f64,
    temperature_c: f64,
) -> Result<f64, TrajectoryPlannerError> {
    // Pressure altitude supplies pressure ratio; sampled temperature supplies
    // temperature ratio. This low-Mach approximation treats IAS as CAS.
    let pressure_base = 1.0 - 6.875_35e-6 * pressure_altitude_ft;
    let temperature_ratio = (temperature_c + 273.15) / 288.15;
    let density_ratio = pressure_base.powf(5.255_879_7) / temperature_ratio;
    if !density_ratio.is_finite() || density_ratio <= 0.0 {
        return Err(TrajectoryPlannerError::AtmosphereUnavailable(
            "temperature and pressure altitude do not define positive air density".to_string(),
        ));
    }
    Ok(indicated_airspeed_kt / density_ratio.sqrt())
}

fn interpolation_bounds<'a, T>(
    points: &'a [T],
    altitude_ft: f64,
    phase: &'static str,
    altitude: impl Fn(&T) -> f64,
) -> Result<(&'a T, &'a T, f64), TrajectoryPlannerError> {
    let first = points
        .first()
        .expect("validated performance table is non-empty");
    let last = points
        .last()
        .expect("validated performance table is non-empty");
    if altitude_ft < altitude(first) || altitude_ft > altitude(last) {
        return Err(TrajectoryPlannerError::PerformanceOutOfRange { phase, altitude_ft });
    }
    for pair in points.windows(2) {
        let lower_altitude = altitude(&pair[0]);
        let upper_altitude = altitude(&pair[1]);
        if altitude_ft <= upper_altitude {
            let fraction = if upper_altitude == lower_altitude {
                0.0
            } else {
                (altitude_ft - lower_altitude) / (upper_altitude - lower_altitude)
            };
            return Ok((&pair[0], &pair[1], fraction));
        }
    }
    Ok((last, last, 0.0))
}

fn lerp(from: f64, to: f64, fraction: f64) -> f64 {
    from + (to - from) * fraction
}

fn ground_speed_along_course(
    true_airspeed_kt: f64,
    course_deg: f64,
    wind_east_kt: f64,
    wind_north_kt: f64,
) -> Result<f64, TrajectoryPlannerError> {
    let course_rad = course_deg.to_radians();
    let along_wind_kt = wind_east_kt * course_rad.sin() + wind_north_kt * course_rad.cos();
    let cross_wind_kt = wind_east_kt * course_rad.cos() - wind_north_kt * course_rad.sin();
    if cross_wind_kt.abs() >= true_airspeed_kt {
        return Err(TrajectoryPlannerError::WindExceedsAirspeed);
    }
    let airspeed_along_course_kt = (true_airspeed_kt.powi(2) - cross_wind_kt.powi(2)).sqrt();
    let ground_speed_kt = airspeed_along_course_kt + along_wind_kt;
    if ground_speed_kt <= 0.0 {
        return Err(TrajectoryPlannerError::NonPositiveGroundSpeed);
    }
    Ok(ground_speed_kt)
}

fn advance_altitude(
    altitude_ft: f64,
    vertical_speed_fpm: f64,
    elapsed_seconds: f64,
    phase: FlightPhase,
) -> f64 {
    let candidate = altitude_ft + vertical_speed_fpm * elapsed_seconds / 60.0;
    match phase {
        FlightPhase::Climb { target_altitude_ft } => candidate.min(target_altitude_ft),
        FlightPhase::Cruise => altitude_ft,
        FlightPhase::Descent { target_altitude_ft } => candidate.max(target_altitude_ft),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("RFC3339 test time")
            .with_timezone(&Utc)
    }

    #[test]
    fn departure_clock_parser_uses_next_local_occurrence_and_accepts_local_suffixes() {
        let now = utc("2026-08-05T19:00:00Z").timestamp_millis();
        let parsed = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::Time,
            "1400 lcl",
            now,
            TimeDisplayMode::Utc,
            chrono_tz::America::Los_Angeles,
        )
        .expect("local clock time");

        assert_eq!(parsed.basis, TimeDisplayMode::Local);
        assert_eq!(
            parsed.departure_time_epoch_ms,
            Some(utc("2026-08-05T21:00:00Z").timestamp_millis())
        );

        let tomorrow = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::Time,
            "1100 local",
            now,
            TimeDisplayMode::Utc,
            chrono_tz::America::Los_Angeles,
        )
        .expect("tomorrow's local clock time");
        assert_eq!(
            tomorrow.departure_time_epoch_ms,
            Some(utc("2026-08-06T18:00:00Z").timestamp_millis())
        );
    }

    #[test]
    fn departure_clock_parser_honors_z_suffix_and_current_basis() {
        let now = utc("2026-08-05T09:00:00Z").timestamp_millis();
        let explicit_z = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::Time,
            "1100Z",
            now,
            TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        )
        .expect("Zulu clock time");
        assert_eq!(explicit_z.basis, TimeDisplayMode::Utc);
        assert_eq!(
            explicit_z.departure_time_epoch_ms,
            Some(utc("2026-08-05T11:00:00Z").timestamp_millis())
        );

        let bare_z = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::Time,
            "0800",
            now,
            TimeDisplayMode::Utc,
            chrono_tz::America::Los_Angeles,
        )
        .expect("bare time in current Z basis");
        assert_eq!(
            bare_z.departure_time_epoch_ms,
            Some(utc("2026-08-06T08:00:00Z").timestamp_millis())
        );
    }

    #[test]
    fn departure_offset_parser_accepts_compound_durations_and_preserves_basis() {
        let now = utc("2026-08-05T19:15:30Z").timestamp_millis();
        let parsed = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::When,
            "1h 30m",
            now,
            TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        )
        .expect("relative departure");

        assert_eq!(parsed.basis, TimeDisplayMode::Local);
        assert_eq!(
            parsed.departure_time_epoch_ms,
            Some(utc("2026-08-05T20:45:30Z").timestamp_millis())
        );
        assert_eq!(
            parse_altitude_planner_departure_input(
                AltitudePlannerDepartureInputField::When,
                "now",
                now,
                TimeDisplayMode::Local,
                chrono_tz::America::Los_Angeles,
            )
            .expect("dynamic now")
            .departure_time_epoch_ms,
            None
        );

        let past = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::When,
            "-3",
            now,
            TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        )
        .expect("unitless negative hours");
        assert_eq!(
            past.departure_time_epoch_ms,
            Some(utc("2026-08-05T16:15:30Z").timestamp_millis())
        );

        let unicode_past = parse_altitude_planner_departure_input(
            AltitudePlannerDepartureInputField::When,
            "−1h 30m",
            now,
            TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        )
        .expect("Unicode-minus past departure");
        assert_eq!(
            unicode_past.departure_time_epoch_ms,
            Some(utc("2026-08-05T17:45:30Z").timestamp_millis())
        );
    }

    #[test]
    fn departure_editor_is_entirely_projected_by_core() {
        let now = utc("2026-08-05T19:00:00Z").timestamp_millis();
        let departure = utc("2026-08-05T22:15:00Z").timestamp_millis();
        let local = project_altitude_planner_ui(AltitudePlannerUiInput {
            now_epoch_ms: now,
            departure_time_epoch_ms: Some(departure),
            effective_departure_time_epoch_ms: departure,
            time_display_mode: TimeDisplayMode::Local,
            local_time_zone: chrono_tz::America::Los_Angeles,
            ..AltitudePlannerUiInput::default()
        });
        assert_eq!(local.departure.time_value, "1515");
        assert_eq!(local.departure.time_label, "");
        assert_eq!(local.departure.basis_label, "PDT");
        assert_eq!(local.departure.when_label, "=");
        assert_eq!(local.departure.when_value, "3h 15m");
        assert!(!local.departure.when_is_past);

        let zulu = project_altitude_planner_ui(AltitudePlannerUiInput {
            now_epoch_ms: now,
            departure_time_epoch_ms: Some(departure),
            effective_departure_time_epoch_ms: departure,
            time_display_mode: TimeDisplayMode::Utc,
            local_time_zone: chrono_tz::America::Los_Angeles,
            ..AltitudePlannerUiInput::default()
        });
        assert_eq!(zulu.departure.time_value, "2215");
        assert_eq!(zulu.departure.basis_label, "Z");
        assert_eq!(zulu.departure.when_value, "3h 15m");

        let past = project_altitude_planner_ui(AltitudePlannerUiInput {
            now_epoch_ms: now,
            departure_time_epoch_ms: Some(now - 1),
            effective_departure_time_epoch_ms: now - 1,
            ..AltitudePlannerUiInput::default()
        });
        assert!(past.departure.when_is_past);

        let exactly_now = project_altitude_planner_ui(AltitudePlannerUiInput {
            now_epoch_ms: now,
            departure_time_epoch_ms: Some(now),
            effective_departure_time_epoch_ms: now,
            ..AltitudePlannerUiInput::default()
        });
        assert!(!exactly_now.departure.when_is_past);

        let dynamic_now = project_altitude_planner_ui(AltitudePlannerUiInput {
            now_epoch_ms: now,
            departure_time_epoch_ms: None,
            effective_departure_time_epoch_ms: now,
            ..AltitudePlannerUiInput::default()
        });
        assert!(!dynamic_now.departure.when_is_past);
    }

    struct ConstantAtmosphere {
        wind_east_kt: f64,
        wind_north_kt: f64,
    }

    impl AtmosphereModel for ConstantAtmosphere {
        fn sample(
            &self,
            _position: LatLon,
            _pressure_altitude_ft: f64,
            _epoch_ms: i64,
        ) -> Result<AtmosphereSample, String> {
            Ok(AtmosphereSample {
                wind_east_kt: self.wind_east_kt,
                wind_north_kt: self.wind_north_kt,
                temperature_c: 15.0,
            })
        }
    }

    fn synthetic_profile() -> AircraftPerformanceProfile {
        AircraftPerformanceProfile {
            schema_version: 1,
            aircraft_model_id: "test-airplane".to_string(),
            profile_id: "test-cruise".to_string(),
            profile_version: "test-1".to_string(),
            aircraft_label: "TEST AIRPLANE".to_string(),
            profile_label: "TEST CRUISE".to_string(),
            source: "Synthetic test fixture; not operational data".to_string(),
            reference_weight_lb: Some(2_300.0),
            cruise: vec![
                CruisePerformancePoint {
                    pressure_altitude_ft: 0.0,
                    true_airspeed_kt: 120.0,
                    fuel_flow_gph: 10.0,
                },
                CruisePerformancePoint {
                    pressure_altitude_ft: 12_000.0,
                    true_airspeed_kt: 132.0,
                    fuel_flow_gph: 9.0,
                },
            ],
            climb: vec![
                VerticalPerformancePoint {
                    pressure_altitude_ft: 0.0,
                    airspeed_basis: PerformanceAirspeedBasis::Indicated,
                    airspeed_kt: 90.0,
                    fuel_flow_gph: 12.0,
                    vertical_speed_fpm: 800.0,
                },
                VerticalPerformancePoint {
                    pressure_altitude_ft: 12_000.0,
                    airspeed_basis: PerformanceAirspeedBasis::Indicated,
                    airspeed_kt: 100.0,
                    fuel_flow_gph: 11.0,
                    vertical_speed_fpm: 400.0,
                },
            ],
            descent: vec![
                VerticalPerformancePoint {
                    pressure_altitude_ft: 0.0,
                    airspeed_basis: PerformanceAirspeedBasis::True,
                    airspeed_kt: 125.0,
                    fuel_flow_gph: 7.0,
                    vertical_speed_fpm: -500.0,
                },
                VerticalPerformancePoint {
                    pressure_altitude_ft: 12_000.0,
                    airspeed_basis: PerformanceAirspeedBasis::True,
                    airspeed_kt: 135.0,
                    fuel_flow_gph: 6.0,
                    vertical_speed_fpm: -500.0,
                },
            ],
        }
    }

    fn eastbound_leg(degrees: f64) -> TrajectoryRouteLeg {
        TrajectoryRouteLeg {
            row_id: FlightPlanRowId("row-destination".to_string()),
            path: vec![
                LatLon { lat: 0.0, lon: 0.0 },
                LatLon {
                    lat: 0.0,
                    lon: degrees,
                },
            ],
        }
    }

    fn bundled_pa46_profile() -> AircraftPerformanceProfile {
        let definition: product_contracts::AircraftDefinition = serde_json::from_str(include_str!(
            "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/piper-pa46-310p.json"
        ))
        .expect("bundled PA46 definition");
        let hash = definition.content_hash().expect("definition hash");
        crate::performance_profile_from_definition(&hash, &definition, "economy-65")
            .expect("runtime PA46 profile")
    }

    #[test]
    fn interpolates_cruise_performance_without_extrapolating() {
        let profile = synthetic_profile();
        let midpoint = interpolate_cruise(&profile.cruise, 6_000.0).unwrap();

        assert_eq!(midpoint.true_airspeed_kt, 126.0);
        assert_eq!(midpoint.fuel_flow_gph, 9.5);
        assert!(matches!(
            interpolate_cruise(&profile.cruise, 13_000.0),
            Err(TrajectoryPlannerError::PerformanceOutOfRange {
                phase: "cruise",
                ..
            })
        ));
    }

    #[test]
    fn vertical_schedule_converts_indicated_speed_with_the_atmosphere() {
        let profile = bundled_pa46_profile();
        let points = &profile.climb;
        let sea_level = interpolate_vertical(points, 0.0, 15.0, "climb").unwrap();
        let ten_thousand = interpolate_vertical(points, 10_000.0, -5.0, "climb").unwrap();
        let twenty_thousand = interpolate_vertical(points, 20_000.0, -25.0, "climb").unwrap();

        assert!((sea_level.true_airspeed_kt - 130.0).abs() < 0.1);
        assert!((ten_thousand.true_airspeed_kt - 151.2).abs() < 0.2);
        assert!((twenty_thousand.true_airspeed_kt - 178.0).abs() < 0.2);
        assert_eq!(twenty_thousand.fuel_flow_gph, 36.0);
        assert_eq!(twenty_thousand.vertical_speed_fpm, 1_100.0);
    }

    #[test]
    fn warmer_air_increases_true_speed_for_a_vertical_ias_schedule() {
        let profile = bundled_pa46_profile();
        let points = &profile.climb;
        let isa = interpolate_vertical(points, 10_000.0, -5.0, "climb").unwrap();
        let warm = interpolate_vertical(points, 10_000.0, 15.0, "climb").unwrap();

        assert!(warm.true_airspeed_kt > isa.true_airspeed_kt);
    }

    #[test]
    fn vertical_tas_schedule_is_not_density_corrected() {
        let profile = bundled_pa46_profile();
        let cold = interpolate_vertical(&profile.descent, 12_000.0, -40.0, "descent").unwrap();
        let warm = interpolate_vertical(&profile.descent, 12_000.0, 20.0, "descent").unwrap();

        assert_eq!(cold.true_airspeed_kt, 185.0);
        assert_eq!(warm.true_airspeed_kt, 185.0);
    }

    #[test]
    fn predicts_level_no_wind_time_and_fuel() {
        let profile = synthetic_profile();
        let planner = TrajectoryPlanner::new(&profile, &NoWindIsaAtmosphere);
        let input = TrajectoryPlanInput {
            legs: vec![eastbound_leg(1.0)],
            start_pressure_altitude_ft: 0.0,
            cruise_pressure_altitude_ft: 0.0,
            destination_pressure_altitude_ft: 0.0,
            departure_epoch_ms: 0,
        };

        let prediction = planner.predict(&input).unwrap();

        assert_eq!(prediction.estimate_kind, FlightEstimateKind::Modeled);
        assert!((prediction.total_ete_seconds - 1_801.2).abs() < 2.0);
        assert!((prediction.total_fuel_gal - 5.003).abs() < 0.02);
        assert_eq!(prediction.legs[0].row_id.0, "row-destination");
        assert_eq!(format_trajectory_wind(&prediction), "·");
    }

    #[test]
    fn headwind_increases_time_and_fuel() {
        let profile = synthetic_profile();
        let no_wind = TrajectoryPlanner::new(&profile, &NoWindIsaAtmosphere);
        let headwind = ConstantAtmosphere {
            wind_east_kt: -20.0,
            wind_north_kt: 0.0,
        };
        let with_headwind = TrajectoryPlanner::new(&profile, &headwind);
        let input = TrajectoryPlanInput {
            legs: vec![eastbound_leg(1.0)],
            start_pressure_altitude_ft: 0.0,
            cruise_pressure_altitude_ft: 0.0,
            destination_pressure_altitude_ft: 0.0,
            departure_epoch_ms: 0,
        };

        let still_air = no_wind.predict(&input).unwrap();
        let windy = with_headwind.predict(&input).unwrap();

        assert!(windy.total_ete_seconds > still_air.total_ete_seconds);
        assert!(windy.total_fuel_gal > still_air.total_fuel_gal);
        assert!((windy.average_wind_east_kt + 20.0).abs() < 1.0e-9);
        assert!((windy.average_along_course_wind_kt + 20.0).abs() < 0.01);
        assert_eq!(format_trajectory_wind(&windy), "← −20");
    }

    #[test]
    fn wind_summary_arrow_points_toward_flow_and_signs_tailwind_positive() {
        let profile = synthetic_profile();
        let tailwind = ConstantAtmosphere {
            wind_east_kt: 8.0,
            wind_north_kt: 0.0,
        };
        let prediction = TrajectoryPlanner::new(&profile, &tailwind)
            .predict(&TrajectoryPlanInput {
                legs: vec![eastbound_leg(1.0)],
                start_pressure_altitude_ft: 0.0,
                cruise_pressure_altitude_ft: 0.0,
                destination_pressure_altitude_ft: 0.0,
                departure_epoch_ms: 0,
            })
            .unwrap();

        assert_eq!(format_trajectory_wind(&prediction), "→ +8");
    }

    #[test]
    fn integrates_climb_cruise_and_descent() {
        let profile = synthetic_profile();
        let planner = TrajectoryPlanner::new(&profile, &NoWindIsaAtmosphere);
        let input = TrajectoryPlanInput {
            legs: vec![eastbound_leg(4.0)],
            start_pressure_altitude_ft: 1_000.0,
            cruise_pressure_altitude_ft: 8_000.0,
            destination_pressure_altitude_ft: 1_000.0,
            departure_epoch_ms: 0,
        };

        let prediction = planner.predict(&input).unwrap();

        assert!(prediction.maximum_pressure_altitude_ft > 7_900.0);
        assert!((prediction.legs[0].ending_pressure_altitude_ft - 1_000.0).abs() < 100.0);
        assert!(prediction.total_fuel_gal > 0.0);
    }

    #[test]
    fn active_navigation_without_altitude_stays_basic_and_explains_why() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("TEST CRUISE".to_string()),
            cruise_altitude_ft: Some(8_000),
            navigation_active: true,
            ownship_altitude_available: false,
            plan_destination_altitude_available: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(view.estimate_kind, FlightEstimateKind::Basic);
        assert_eq!(view.unavailable_reasons.len(), 1);
        assert_eq!(
            view.unavailable_reasons[0].code,
            AltitudePlannerUnavailableReasonCode::OwnshipAltitudeUnavailable
        );
    }

    #[test]
    fn forecast_coverage_fallback_replaces_provenance_with_zulu_status() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("65% ECONOMY".to_string()),
            cruise_altitude_ft: Some(12_000),
            plan_origin_altitude_available: true,
            plan_destination_altitude_available: true,
            wind_model_label: "FORECAST".to_string(),
            wind_model_selected: true,
            time_display_mode: TimeDisplayMode::Utc,
            forecast: Some(AltitudePlannerForecastUiView {
                summary: "ordinary provenance".to_string(),
            }),
            wind_fallback: Some(AltitudePlannerWindFallback::ForecastCoverage {
                valid_from_epoch_ms: 0,
                valid_through_epoch_ms: 12 * 60 * 60 * 1_000,
                flight_start_epoch_ms: 0,
                flight_end_epoch_ms: (15 * 60 + 35) * 60 * 1_000,
            }),
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(
            view.forecast.expect("fallback status").summary,
            "Currently available forecast product valid until 1200Z, flight extends until 1535Z. Using NO-WIND model."
        );
        assert!(view.advisories.is_empty());
    }

    #[test]
    fn forecast_coverage_fallback_uses_local_display_basis() {
        let epoch_ms = DateTime::parse_from_rfc3339("2026-08-05T12:00:00Z")
            .expect("timestamp")
            .timestamp_millis();
        let pacific = "America/Los_Angeles".parse::<Tz>().expect("time zone");

        assert_eq!(
            format_planner_clock(epoch_ms, TimeDisplayMode::Local, pacific),
            "0500 PDT"
        );
    }

    #[test]
    fn complete_inputs_enable_modeled_provenance() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("TEST CRUISE".to_string()),
            cruise_altitude_ft: Some(8_000),
            navigation_active: true,
            ownship_altitude_available: true,
            plan_destination_altitude_available: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(view.estimate_kind, FlightEstimateKind::Modeled);
        assert!(view.unavailable_reasons.is_empty());
    }

    #[test]
    fn active_live_ground_speed_keeps_main_estimate_basic() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("65% ECONOMY".to_string()),
            cruise_altitude_ft: Some(12_000),
            navigation_active: true,
            ownship_altitude_available: true,
            plan_destination_altitude_available: true,
            live_ground_speed_estimate_active: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(view.estimate_kind, FlightEstimateKind::Basic);
        assert!(view.unavailable_reasons.is_empty());
        assert_eq!(
            view.estimate_summary.label,
            "Estimate basis:\nGS extrapolated"
        );
        assert_eq!(
            view.estimate_summary.estimate_kind,
            FlightEstimateKind::Basic
        );
    }

    #[test]
    fn modeled_summary_names_wind_model_and_selected_cruise_altitude() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("65% ECONOMY".to_string()),
            cruise_altitude_ft: Some(12_000),
            plan_origin_altitude_available: true,
            plan_destination_altitude_available: true,
            wind_model_label: "FORECAST".to_string(),
            wind_model_selected: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(
            view.estimate_summary.label,
            "Estimate basis:\nForecast winds\n12,000 cruise"
        );
        assert_eq!(
            view.estimate_summary.estimate_kind,
            FlightEstimateKind::Modeled
        );
    }

    #[test]
    fn planned_route_without_origin_elevation_stays_basic() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("TEST CRUISE".to_string()),
            cruise_altitude_ft: Some(8_000),
            plan_destination_altitude_available: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(view.estimate_kind, FlightEstimateKind::Basic);
        assert_eq!(view.unavailable_reasons.len(), 1);
        assert_eq!(
            view.unavailable_reasons[0].code,
            AltitudePlannerUnavailableReasonCode::PlanOriginAltitudeUnavailable
        );
    }

    #[test]
    fn planned_route_without_destination_elevation_stays_basic() {
        let view = project_altitude_planner_ui(AltitudePlannerUiInput {
            aircraft_profile_label: Some("TEST CRUISE".to_string()),
            cruise_altitude_ft: Some(8_000),
            plan_origin_altitude_available: true,
            ..AltitudePlannerUiInput::default()
        });

        assert_eq!(view.estimate_kind, FlightEstimateKind::Basic);
        assert_eq!(view.unavailable_reasons.len(), 1);
        assert_eq!(
            view.unavailable_reasons[0].code,
            AltitudePlannerUnavailableReasonCode::PlanDestinationAltitudeUnavailable
        );
    }
}
