// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{LatLon, MetarProductPayload};
pub use app_ui_contracts::session::{BarometerCommand, BarometerEditor};

const HPA_PER_INHG: f64 = 33.863_886_666_7;
const SAMPLE_LIFETIME_MS: i64 = 5_000;
const FILTER_TIME_CONSTANT_MS: f64 = 1_000.0;
const MAX_WEATHER_AGE_MS: i64 = 80 * 60 * 1_000;
const MAX_WEATHER_DISTANCE_NM: f64 = 100.0;
pub(crate) const BAROMETER_CELL_ID: &str = "barometer";

pub(crate) struct NearbyAltimeter {
    station: String,
    setting_inhg: f64,
    distance_nm: f64,
    observed_epoch_ms: i64,
}

impl NearbyAltimeter {
    pub fn expires_at(&self) -> i64 {
        self.observed_epoch_ms + MAX_WEATHER_AGE_MS + 1
    }
}

pub(crate) fn nearest_altimeter(
    position: Option<LatLon>,
    weather: Option<&MetarProductPayload>,
    now: i64,
) -> Option<NearbyAltimeter> {
    let position = position?;
    weather?
        .metars_by_station
        .values()
        .filter_map(|report| {
            if !(-90.0..=90.0).contains(&report.latitude)
                || !(-180.0..=180.0).contains(&report.longitude)
            {
                return None;
            }
            let distance_nm = crate::geodesy::great_circle_distance_nm(
                position,
                LatLon {
                    lat: report.latitude,
                    lon: report.longitude,
                },
            );
            if !distance_nm.is_finite() || distance_nm > MAX_WEATHER_DISTANCE_NM {
                return None;
            }
            // Report age, not feed download time. Unknown or future dates are not usable.
            let observed_epoch_ms =
                chrono::DateTime::parse_from_rfc3339(report.observed_at_utc.as_deref()?)
                    .ok()?
                    .timestamp_millis();
            if !(0..=MAX_WEATHER_AGE_MS).contains(&now.saturating_sub(observed_epoch_ms)) {
                return None;
            }
            Some(NearbyAltimeter {
                station: report.station_id.clone(),
                setting_inhg: metar_setting_inhg(&report.raw_text)?,
                distance_nm,
                observed_epoch_ms,
            })
        })
        .min_by(|a, b| {
            a.distance_nm
                .total_cmp(&b.distance_nm)
                .then_with(|| a.station.cmp(&b.station))
        })
}

fn metar_setting_inhg(text: &str) -> Option<f64> {
    // METAR body A#### is hundredths inHg; Q#### is hPa. SLP in remarks is
    // not an altimeter setting. https://aviationweather.gov/help/data/
    let mut setting = None;
    for token in text
        .split_ascii_whitespace()
        .take_while(|token| *token != "RMK")
    {
        let token = token.trim_end_matches('=');
        if token.len() != 5 || !matches!(token.as_bytes()[0], b'A' | b'Q') {
            continue;
        }
        if !token.as_bytes()[1..].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let number: f64 = token[1..].parse().ok()?;
        let value = if token.starts_with('A') {
            number / 100.0
        } else {
            number / HPA_PER_INHG
        };
        if !(25.0..=35.0).contains(&value) || setting.is_some() {
            return None;
        }
        setting = Some((value * 100.0).round() / 100.0);
    }
    setting
}

#[derive(Debug, Clone)]
pub struct BarometerReading {
    pub altitude_ft: Option<f64>,
    pub editor: Option<BarometerEditor>,
}

/// Device pressure is deliberately separate from ownship's aviation pressure altitude.
/// It must not become an input to terrain clearance, navigation, or traffic separation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Barometer {
    available: bool,
    sample: Option<(i64, f64)>,
    setting_inhg: f64,
    editor: Option<BarometerEditor>,
}

impl Default for Barometer {
    fn default() -> Self {
        Self {
            available: false,
            sample: None,
            setting_inhg: 29.92,
            editor: None,
        }
    }
}

impl Barometer {
    pub fn reading(&self, now: i64, nearest: Option<&NearbyAltimeter>) -> Option<BarometerReading> {
        self.available.then(|| BarometerReading {
            altitude_ft: self.altitude_ft(now),
            editor: self.editor().map(|mut editor| {
                editor.nearest_enabled = nearest.is_some();
                editor.nearest_detail = nearest.map(|report| {
                    format!(
                        "{}: {:.2} inHg, {}nm, {} min old",
                        report.station,
                        report.setting_inhg,
                        crate::flight_data::format_nm(report.distance_nm),
                        (now - report.observed_epoch_ms) / 60_000,
                    )
                });
                editor
            }),
        })
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn apply(
        &mut self,
        command: BarometerCommand,
        now: i64,
        nearest: Option<&NearbyAltimeter>,
    ) {
        match command {
            BarometerCommand::Observe {
                available,
                pressure_hpa,
                observed_epoch_ms,
                ..
            } => {
                self.available = available;
                if !available {
                    self.sample = None;
                    self.editor = None;
                    return;
                }
                let Some(pressure) =
                    pressure_hpa.filter(|p| p.is_finite() && (100.0..=1_200.0).contains(p))
                else {
                    self.sample = None;
                    return;
                };
                if observed_epoch_ms > now
                    || now.saturating_sub(observed_epoch_ms) >= SAMPLE_LIFETIME_MS
                {
                    return;
                }
                let filtered = match self.sample {
                    Some((previous_time, _)) if observed_epoch_ms <= previous_time => return,
                    Some((previous_time, previous_pressure))
                        if observed_epoch_ms - previous_time < SAMPLE_LIFETIME_MS =>
                    {
                        let weight = 1.0
                            - (-((observed_epoch_ms - previous_time) as f64)
                                / FILTER_TIME_CONSTANT_MS)
                                .exp();
                        previous_pressure + weight * (pressure - previous_pressure)
                    }
                    _ => pressure,
                };
                self.sample = Some((observed_epoch_ms, filtered));
            }
            BarometerCommand::SetSetting { input } => {
                let Some(editor) = self.editor.as_mut() else {
                    return;
                };
                editor.input = input;
                match editor.input.trim().parse::<f64>() {
                    Ok(setting) if setting.is_finite() && (25.0..=35.0).contains(&setting) => {
                        self.setting_inhg = (setting * 100.0).round() / 100.0;
                        editor.error = None;
                    }
                    _ => {
                        editor.error =
                            Some("Enter an altimeter setting from 25.00 to 35.00 inHg.".to_string())
                    }
                }
            }
            BarometerCommand::UseNearest => {
                if let (Some(editor), Some(report)) = (self.editor.as_mut(), nearest) {
                    self.setting_inhg = report.setting_inhg;
                    editor.input = format!("{:.2}", self.setting_inhg);
                    editor.input_revision += 1;
                    editor.error = None;
                }
            }
            BarometerCommand::CloseEditor => self.editor = None,
        }
    }

    pub fn open_editor(&mut self) {
        if self.available {
            self.editor = Some(BarometerEditor {
                title: "BARO".to_string(),
                label: "Altimeter inHg".to_string(),
                input: format!("{:.2}", self.setting_inhg),
                input_revision: 0,
                error: None,
                nearest_label: "NEAREST".to_string(),
                nearest_enabled: false,
                nearest_detail: None,
                close_label: "CLOSE".to_string(),
            });
        }
    }

    pub fn editor(&self) -> Option<BarometerEditor> {
        self.editor.clone()
    }

    pub fn altitude_ft(&self, now: i64) -> Option<f64> {
        let (timestamp, pressure) = self.sample?;
        if !self.available || now < timestamp || now.saturating_sub(timestamp) >= SAMPLE_LIFETIME_MS
        {
            return None;
        }
        // Standard-atmosphere altimetry with the manually selected sea-level pressure.
        // Android SensorManager.getAltitude documents the same pressure relationship:
        // https://developer.android.com/reference/android/hardware/SensorManager#getAltitude(float,%20float)
        Some(
            44_330.0 / 0.3048
                * (1.0 - (pressure / (self.setting_inhg * HPA_PER_INHG)).powf(1.0 / 5.255)),
        )
    }

    pub fn next_refresh(&self, now: i64) -> Option<i64> {
        self.sample
            .map(|(time, _)| time.saturating_add(SAMPLE_LIFETIME_MS))
            .filter(|deadline| *deadline > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather(reports: &[(&str, f64, i64, &str)], now: i64) -> MetarProductPayload {
        MetarProductPayload {
            schema_version: 3,
            version_label: "test".into(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: None,
            metars_by_station: reports
                .iter()
                .map(|(id, lon, age_ms, raw)| {
                    (
                        id.to_string(),
                        crate::MetarRecord {
                            station_id: id.to_string(),
                            longitude: *lon,
                            latitude: 0.0,
                            raw_text: raw.to_string(),
                            clouds: None,
                            flight_category: None,
                            observed_at_utc: Some(
                                chrono::DateTime::from_timestamp_millis(now - age_ms)
                                    .unwrap()
                                    .to_rfc3339(),
                            ),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn nearest_uses_closest_fresh_report_with_a_valid_setting_not_closest_symbol() {
        let now = 2_000_000_000_000;
        let payload = weather(
            &[
                ("OLD", 0.01, MAX_WEATHER_AGE_MS + 1, "A3100"),
                ("MISSING", 0.02, 0, "RMK SLP150 A3100"),
                ("BAD", 0.03, 0, "A////"),
                ("FUTURE", 0.04, -1, "A3100"),
                (
                    "CLOSE",
                    0.1,
                    MAX_WEATHER_AGE_MS,
                    "METAR CLOSE 010000Z A2997 RMK SLP150",
                ),
                ("FARTHER", 0.2, 0, "A3001"),
            ],
            now,
        );
        let position = Some(LatLon { lat: 0.0, lon: 0.0 });
        let nearest = nearest_altimeter(position, Some(&payload), now).unwrap();
        assert_eq!(nearest.station, "CLOSE");
        assert_eq!(nearest.setting_inhg, 29.97);
        assert_eq!(nearest.expires_at(), now + 1);
        assert_eq!(
            nearest_altimeter(position, Some(&payload), now + 1)
                .unwrap()
                .station,
            "FUTURE"
        );
        assert!(nearest_altimeter(None, Some(&payload), now).is_none());
        assert!(nearest_altimeter(position, None, now).is_none());
    }

    #[test]
    fn nearest_rejects_out_of_range_missing_dates_and_old_reports() {
        let now = 2_000_000_000_000;
        let position = LatLon { lat: 0.0, lon: 0.0 };
        for (distance, age, expected) in [
            (99.999, MAX_WEATHER_AGE_MS, true),
            (100.001, 0, false),
            (1.0, MAX_WEATHER_AGE_MS + 1, false),
        ] {
            let point = crate::ui_geometry::ui_project_ahead(position, 90.0, distance);
            let mut payload = weather(&[("TEST", point.lon, age, "A3000")], now);
            assert_eq!(
                nearest_altimeter(Some(position), Some(&payload), now).is_some(),
                expected
            );
            for date in [None, Some("bad timestamp".to_string())] {
                payload
                    .metars_by_station
                    .get_mut("TEST")
                    .unwrap()
                    .observed_at_utc = date;
                assert!(nearest_altimeter(Some(position), Some(&payload), now).is_none());
            }
        }
    }

    #[test]
    fn setting_parser_handles_inhg_and_hpa_but_not_remarks_or_invalid_pressure() {
        assert_eq!(
            metar_setting_inhg("METAR TEST A2997 RMK SLP152"),
            Some(29.97)
        );
        assert_eq!(metar_setting_inhg("METAR TEST Q1013="), Some(29.91));
        for text in [
            "RMK A2997",
            "SLP152",
            "A////",
            "A9999",
            "A299",
            "A29970",
            "Q0000",
            "A2997 A3010",
        ] {
            assert_eq!(metar_setting_inhg(text), None, "{text}");
        }
    }

    #[test]
    fn nearest_command_replaces_draft_and_disabled_command_cannot_change_setting() {
        let now = 2_000_000_000_000;
        let mut baro = Barometer::default();
        observe(&mut baro, Some(1002.64), now);
        baro.open_editor();
        baro.apply(
            BarometerCommand::SetSetting {
                input: "oops".into(),
            },
            now,
            None,
        );
        let payload = weather(&[("TEST", 0.0, 60_000, "A2997")], now);
        let nearest = nearest_altimeter(Some(LatLon { lat: 0.0, lon: 0.0 }), Some(&payload), now);
        let editor = baro.reading(now, nearest.as_ref()).unwrap().editor.unwrap();
        assert!(editor.nearest_enabled);
        assert!(editor.nearest_detail.unwrap().contains("TEST: 29.97 inHg"));
        baro.apply(BarometerCommand::UseNearest, now, nearest.as_ref());
        assert_eq!(baro.editor().unwrap().input, "29.97");
        assert_eq!(baro.editor().unwrap().input_revision, 1);
        assert!(baro.editor().unwrap().error.is_none());
        assert!((baro.altitude_ft(now).unwrap() - 336.0).abs() < 1.0);
        assert!(
            !baro
                .reading(now, None)
                .unwrap()
                .editor
                .unwrap()
                .nearest_enabled
        );
        let before = baro.clone();
        baro.apply(BarometerCommand::UseNearest, now, None);
        assert_eq!(baro, before);
    }

    fn observe(baro: &mut Barometer, pressure: Option<f64>, time: i64) {
        baro.apply(
            BarometerCommand::Observe {
                available: true,
                pressure_hpa: pressure,
                observed_epoch_ms: time,
                received_epoch_ms: time,
            },
            time,
            None,
        );
    }

    #[test]
    fn calibrated_pressure_reads_expected_altitude_without_any_gps_input() {
        let mut baro = Barometer::default();
        observe(&mut baro, Some(1002.64), 10_000);
        baro.open_editor();
        baro.apply(
            BarometerCommand::SetSetting {
                input: "29.97".into(),
            },
            10_000,
            None,
        );
        assert!((baro.altitude_ft(10_000).unwrap() - 336.0).abs() < 1.0);
        assert_eq!(baro.editor().unwrap().input, "29.97");
        // The reference pressure must read zero, including at non-standard settings.
        observe(&mut baro, Some(29.97 * HPA_PER_INHG), 20_000);
        assert!(baro.altitude_ft(20_000).unwrap().abs() < 0.001);
    }

    #[test]
    fn absent_invalid_or_stale_pressure_is_not_an_altitude_and_recovers() {
        let mut baro = Barometer::default();
        assert!(!baro.available());
        assert_eq!(baro.altitude_ft(10_000), None);
        observe(&mut baro, Some(900.0), 10_000);
        assert_eq!(baro.next_refresh(10_000), Some(15_000));
        assert!(baro.altitude_ft(14_999).is_some());
        assert_eq!(baro.altitude_ft(15_000), None);
        assert_eq!(baro.next_refresh(15_000), None);
        observe(&mut baro, Some(800.0), 20_000);
        let recovered = baro.altitude_ft(20_000).unwrap();
        assert!(recovered > 6_000.0);
        for pressure in [None, Some(f64::NAN), Some(0.0), Some(-1.0)] {
            observe(&mut baro, pressure, 21_000);
            assert_eq!(baro.altitude_ft(21_000), None);
        }
    }

    #[test]
    fn duplicate_and_delayed_samples_cannot_distort_filter_or_extend_freshness() {
        let mut baro = Barometer::default();
        observe(&mut baro, Some(900.0), 10_000);
        let original = baro.altitude_ft(10_000);
        observe(&mut baro, Some(800.0), 10_000);
        observe(&mut baro, Some(800.0), 9_000);
        assert_eq!(baro.altitude_ft(10_000), original);
        baro.apply(
            BarometerCommand::Observe {
                available: true,
                pressure_hpa: Some(700.0),
                observed_epoch_ms: 11_000,
                received_epoch_ms: 20_000,
            },
            20_000,
            None,
        );
        assert_eq!(baro.altitude_ft(20_000), None);
    }

    #[test]
    fn invalid_setting_keeps_last_calibration_and_error_until_corrected() {
        let mut baro = Barometer::default();
        observe(&mut baro, Some(900.0), 10_000);
        let original = baro.altitude_ft(10_000);
        baro.open_editor();
        for input in ["", "2997", "NaN", "-3", "100"] {
            baro.apply(
                BarometerCommand::SetSetting {
                    input: input.into(),
                },
                10_000,
                None,
            );
            assert!(baro.editor().unwrap().error.is_some());
            assert_eq!(baro.altitude_ft(10_000), original);
        }
        baro.apply(
            BarometerCommand::SetSetting {
                input: "30.12".into(),
            },
            10_000,
            None,
        );
        assert!(baro.editor().unwrap().error.is_none());
        assert!(baro.altitude_ft(10_000).unwrap() > original.unwrap());
    }
}
