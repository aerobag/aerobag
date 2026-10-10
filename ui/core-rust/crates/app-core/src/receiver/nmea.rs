// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! GGA's height is explicitly MSL; traffic's geometric height is not.
//! Input has already passed the receiver product checksum/size validation.

use chrono::{DateTime, Duration, NaiveTime, Utc};

use crate::LatLon;

#[derive(Debug, Clone)]
pub(super) struct Gga {
    pub time: i64,
    pub fix: Option<(LatLon, Option<f64>)>,
}

pub(super) fn gga(sentence: &str, now: i64) -> Option<Gga> {
    let body = sentence.strip_prefix('$')?.split('*').next()?;
    let fields: Vec<_> = body.split(',').collect();
    if !matches!(fields.first()?, &"GPGGA" | &"GNGGA") || fields.len() < 11 {
        return None;
    }
    let time = NaiveTime::parse_from_str(fields[1], "%H%M%S%.f").ok()?;
    let now = DateTime::<Utc>::from_timestamp_millis(now)?;
    // GGA has no date. Choose the nearest UTC day, then enforce freshness at
    // ingestion. This handles midnight without borrowing a stale RMC date.
    let midnight = now.date_naive().and_time(time).and_utc();
    let time = [-1, 0, 1]
        .into_iter()
        .filter_map(|days| midnight.checked_add_signed(Duration::days(days)))
        .min_by_key(|candidate| (candidate.timestamp_millis() - now.timestamp_millis()).abs())?
        .timestamp_millis();
    // Estimated/manual/simulator fixes are not satellite fixes.
    let fix = if matches!(fields[6], "1" | "2" | "3" | "4" | "5") {
        coordinate(fields[2], fields[3], true)
            .zip(coordinate(fields[4], fields[5], false))
            .map(|(lat, lon)| {
                let altitude = (fields[10] == "M")
                    .then(|| fields[9].parse::<f64>().ok())
                    .flatten()
                    .filter(|value| value.is_finite() && (-1_000.0..=30_000.0).contains(value))
                    .map(|meters| meters / 0.3048);
                (LatLon { lat, lon }, altitude)
            })
    } else {
        None
    };
    Some(Gga { time, fix })
}

fn coordinate(value: &str, hemisphere: &str, latitude: bool) -> Option<f64> {
    let degrees_len = if latitude { 2 } else { 3 };
    if !value.is_ascii() || value.len() < degrees_len + 2 {
        return None;
    }
    let degrees: f64 = value[..degrees_len].parse().ok()?;
    let minutes: f64 = value[degrees_len..].parse().ok()?;
    let sign = match (latitude, hemisphere) {
        (true, "N") | (false, "E") => 1.0,
        (true, "S") | (false, "W") => -1.0,
        _ => return None,
    };
    let maximum = if latitude { 90.0 } else { 180.0 };
    let value = degrees + minutes / 60.0;
    (degrees >= 0.0 && (0.0..60.0).contains(&minutes) && value <= maximum).then_some(sign * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midnight_msl_units_and_invalid_fix_are_explicit() {
        let now = DateTime::parse_from_rfc3339("2026-10-10T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        let sentence = "$GPGGA,235959.00,4727.000,N,12218.600,W,1,10,1.0,609.6,M,-20,M,,*00";
        let report = gga(sentence, now).unwrap();
        assert_eq!(report.time, now - 1000);
        let (position, altitude) = report.fix.unwrap();
        assert!((position.lat - 47.45).abs() < 1e-9);
        assert!((position.lon + 122.31).abs() < 1e-9);
        assert_eq!(altitude, Some(2000.0));
        assert!(gga(&sentence.replace(",1,10,", ",0,10,"), now)
            .unwrap()
            .fix
            .is_none());
        assert_eq!(
            gga(&sentence.replace("609.6,M", "609.6,F"), now)
                .unwrap()
                .fix
                .unwrap()
                .1,
            None
        );
        assert!(gga(&sentence.replace("4727.000", "4760.000"), now)
            .unwrap()
            .fix
            .is_none());
        assert!(gga(&sentence.replace("235959.00", "bad"), now).is_none());
    }
}
