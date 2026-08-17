// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Offset, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const TOGGLE_TIME_DISPLAY_MODE_ACTION_ID: &str = "toggle_time_display_mode";
const TIME_DISPLAY_REFRESH_INTERVAL_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeDisplayMode {
    #[default]
    Local,
    Utc,
}

impl TimeDisplayMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Local => Self::Utc,
            Self::Utc => Self::Local,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDayStyle {
    Colon,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatedTimeStyle {
    IsoMinute,
    MonthDayMinute,
    Friendly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelativeTimeStyle {
    Ago,
    Old,
    Until,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeOfDayDisplay {
    pub value: String,
    pub basis_label: String,
}

impl TimeOfDayDisplay {
    pub fn with_basis(&self) -> String {
        if self.basis_label == "Z" {
            format!("{}Z", self.value)
        } else {
            format!("{} {}", self.value, self.basis_label)
        }
    }
}

pub fn format_time_of_day(
    epoch_ms: i64,
    mode: TimeDisplayMode,
    local_time_zone: Tz,
    style: TimeOfDayStyle,
) -> TimeOfDayDisplay {
    let utc =
        DateTime::<Utc>::from_timestamp_millis(epoch_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let value_format = match style {
        TimeOfDayStyle::Colon => "%H:%M",
        TimeOfDayStyle::Compact => "%H%M",
    };
    match mode {
        TimeDisplayMode::Local => {
            let local = utc.with_timezone(&local_time_zone);
            TimeOfDayDisplay {
                value: local.format(value_format).to_string(),
                basis_label: local_time_zone_label(local),
            }
        }
        TimeDisplayMode::Utc => TimeOfDayDisplay {
            value: utc.format(value_format).to_string(),
            basis_label: "Z".to_string(),
        },
    }
}

pub fn next_time_display_refresh_epoch_ms(epoch_ms: i64) -> i64 {
    epoch_ms
        .div_euclid(TIME_DISPLAY_REFRESH_INTERVAL_MS)
        .saturating_add(1)
        .saturating_mul(TIME_DISPLAY_REFRESH_INTERVAL_MS)
}

pub(crate) fn format_relative_time(
    instant_epoch_ms: i64,
    now_epoch_ms: i64,
    style: RelativeTimeStyle,
    include_seconds: bool,
) -> String {
    let delta_ms = instant_epoch_ms.saturating_sub(now_epoch_ms);
    let magnitude = format_relative_duration(delta_ms.unsigned_abs(), include_seconds);
    match style {
        RelativeTimeStyle::Old => format!("{magnitude} old"),
        RelativeTimeStyle::Until if delta_ms >= 0 => format!("in {magnitude}"),
        RelativeTimeStyle::Ago if delta_ms > 0 => format!("in {magnitude}"),
        RelativeTimeStyle::Ago | RelativeTimeStyle::Until => format!("{magnitude} ago"),
    }
}

fn format_relative_duration(duration_ms: u64, include_seconds: bool) -> String {
    let seconds = duration_ms / 1_000;
    if include_seconds && seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = duration_ms / 60_000;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if !include_seconds && days >= 60 {
        let months = days / 30;
        if months < 24 {
            return format!("{months}mo");
        }
        return format!("{}y", days / 365);
    }
    format!("{days}d")
}

pub fn format_dated_time(
    epoch_ms: i64,
    mode: TimeDisplayMode,
    local_time_zone: Tz,
    style: DatedTimeStyle,
) -> String {
    let utc =
        DateTime::<Utc>::from_timestamp_millis(epoch_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let (local_format, utc_format) = match style {
        DatedTimeStyle::IsoMinute => ("%Y-%m-%d %H:%M", "%Y-%m-%d %H:%MZ"),
        DatedTimeStyle::MonthDayMinute => ("%m/%d %H:%M", "%m/%d %H:%MZ"),
        DatedTimeStyle::Friendly => ("%a %b %-d %-I:%M%P", "%a %b %-d %H:%MZ"),
    };
    match mode {
        TimeDisplayMode::Local => {
            let local = utc.with_timezone(&local_time_zone);
            format!(
                "{} {}",
                local.format(local_format),
                local_time_zone_label(local),
            )
        }
        TimeDisplayMode::Utc => utc.format(utc_format).to_string(),
    }
}

pub fn time_zone_label(epoch_ms: i64, time_zone: Tz) -> String {
    let utc =
        DateTime::<Utc>::from_timestamp_millis(epoch_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    local_time_zone_label(utc.with_timezone(&time_zone))
}

fn local_time_zone_label(local: DateTime<Tz>) -> String {
    let abbreviation = local.format("%Z").to_string();
    if !abbreviation.is_empty()
        && abbreviation
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return abbreviation;
    }
    utc_offset_label(local.offset().fix().local_minus_utc())
}

fn utc_offset_label(offset_seconds: i32) -> String {
    if offset_seconds == 0 {
        return "Z".to_string();
    }
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let total_minutes = offset_seconds.unsigned_abs() / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if minutes == 0 {
        format!("Z{sign}{hours}")
    } else {
        format!("Z{sign}{hours}:{minutes:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_same_instant_in_device_local_and_zulu_modes() {
        let epoch_ms = DateTime::parse_from_rfc3339("2026-08-13T02:34:00Z")
            .expect("instant")
            .timestamp_millis();

        assert_eq!(
            format_time_of_day(
                epoch_ms,
                TimeDisplayMode::Local,
                chrono_tz::America::Los_Angeles,
                TimeOfDayStyle::Colon,
            )
            .with_basis(),
            "19:34 PDT",
        );
        assert_eq!(
            format_time_of_day(
                epoch_ms,
                TimeDisplayMode::Utc,
                chrono_tz::America::Los_Angeles,
                TimeOfDayStyle::Compact,
            )
            .with_basis(),
            "0234Z",
        );
        assert_eq!(
            format_dated_time(
                epoch_ms,
                TimeDisplayMode::Local,
                chrono_tz::America::Los_Angeles,
                DatedTimeStyle::Friendly,
            ),
            "Wed Aug 12 7:34pm PDT",
        );
        assert_eq!(
            format_dated_time(
                epoch_ms,
                TimeDisplayMode::Utc,
                chrono_tz::America::Los_Angeles,
                DatedTimeStyle::MonthDayMinute,
            ),
            "08/13 02:34Z",
        );
    }

    #[test]
    fn unnamed_local_zones_fall_back_to_a_readable_utc_offset() {
        let epoch_ms = DateTime::parse_from_rfc3339("2026-08-13T02:34:00Z")
            .expect("instant")
            .timestamp_millis();

        assert_eq!(time_zone_label(epoch_ms, chrono_tz::Etc::GMTPlus3), "Z-3");
    }

    #[test]
    fn next_refresh_is_the_next_minute_boundary() {
        assert_eq!(next_time_display_refresh_epoch_ms(0), 60_000);
        assert_eq!(next_time_display_refresh_epoch_ms(60_000), 120_000);
        assert_eq!(next_time_display_refresh_epoch_ms(119_999), 120_000);
    }

    #[test]
    fn relative_time_policy_is_core_owned() {
        assert_eq!(
            format_relative_time(90_000, 0, RelativeTimeStyle::Until, false),
            "in 1m",
        );
        assert_eq!(
            format_relative_time(0, 90_000, RelativeTimeStyle::Ago, false),
            "1m ago",
        );
        assert_eq!(
            format_relative_time(0, 9_000, RelativeTimeStyle::Old, true),
            "9s old",
        );
    }
}
