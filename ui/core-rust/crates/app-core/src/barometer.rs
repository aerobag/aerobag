// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{LatLon, MetarProductPayload};
pub use app_ui_contracts::session::{FlightDataCommand, FlightDataEditor};

const HPA_PER_INHG: f64 = 33.863_886_666_7;
const SAMPLE_LIFETIME_MS: i64 = 5_000;
const FILTER_TIME_CONSTANT_MS: f64 = 1_000.0;
const MAX_WEATHER_AGE_MS: i64 = 80 * 60 * 1_000;
const MAX_WEATHER_DISTANCE_NM: f64 = 100.0;
const NO_NEARBY_ALTIMETER: &str =
    "No recent nearby METAR altimeter setting is available for the current position.";
pub(crate) const BAROMETER_CELL_ID: &str = "barometer";

pub(crate) struct NearbyAltimeter {
    pub station: String,
    pub position: LatLon,
    weather_badge: Option<crate::planning::FlightPlanWeatherBadgeUiView>,
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
                position: LatLon {
                    lat: report.latitude,
                    lon: report.longitude,
                },
                weather_badge: crate::map_overlay::weather_badge_for_metar(
                    report,
                    chrono::DateTime::from_timestamp_millis(now),
                ),
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
    pub editor: Option<FlightDataEditor>,
    pub warning: Option<String>,
}

/// Device pressure is deliberately separate from ownship's aviation pressure altitude.
/// It must not become an input to terrain clearance, navigation, or traffic separation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Barometer {
    available: bool,
    sample: Option<(i64, f64)>,
    setting_inhg: f64,
    editor: Option<FlightDataEditor>,
    pressure_history: std::collections::VecDeque<(i64, f64)>,
}

impl Default for Barometer {
    fn default() -> Self {
        Self {
            available: false,
            sample: None,
            setting_inhg: 29.92,
            editor: None,
            pressure_history: Default::default(),
        }
    }
}

impl Barometer {
    pub fn reading(&self, now: i64, nearest: Option<&NearbyAltimeter>) -> Option<BarometerReading> {
        self.available.then(|| BarometerReading {
            altitude_ft: self.altitude_ft(now),
            warning: self.setting_warning(now, nearest),
            editor: self.editor().map(|mut editor| {
                let action = &mut editor.action_rows[0][0];
                action.enabled = nearest.is_some();
                action.disabled_reason = nearest.is_none().then(|| NO_NEARBY_ALTIMETER.into());
                editor.warning = self.setting_warning(now, nearest);
                action.secondary_label = nearest.map(|report| {
                    format!(
                        "{} {}min old",
                        report.station,
                        (now - report.observed_epoch_ms) / 60_000
                    )
                });
                action.weather_badge = nearest.and_then(|report| report.weather_badge.clone());
                editor
            }),
        })
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn apply(
        &mut self,
        command: FlightDataCommand,
        now: i64,
        nearest: Option<&NearbyAltimeter>,
    ) {
        match command {
            FlightDataCommand::Observe {
                available,
                pressure_hpa,
                observed_epoch_ms,
                ..
            } => {
                self.available = available;
                if !available {
                    self.sample = None;
                    self.pressure_history.clear();
                    self.editor = None;
                    return;
                }
                let Some(pressure) =
                    pressure_hpa.filter(|p| p.is_finite() && (100.0..=1_200.0).contains(p))
                else {
                    self.sample = None;
                    self.pressure_history.clear();
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
                    _ => {
                        self.pressure_history.clear();
                        pressure
                    }
                };
                self.sample = Some((observed_epoch_ms, filtered));
                self.pressure_history
                    .push_back((observed_epoch_ms, pressure_altitude(pressure, 29.92)));
                while self
                    .pressure_history
                    .front()
                    .is_some_and(|(time, _)| observed_epoch_ms - time > 20_000)
                    || self.pressure_history.len() > 64
                {
                    self.pressure_history.pop_front();
                }
            }
            FlightDataCommand::SetInput { editor_id, input } if editor_id == BAROMETER_CELL_ID => {
                let Some(editor) = self.editor.as_mut() else {
                    return;
                };
                editor.input_correction = (input.len() == 4
                    && input.bytes().all(|c| c.is_ascii_digit()))
                .then(|| app_ui_contracts::session::FlightDataInputCorrection {
                    source: input.clone(),
                    start: 2,
                    end: 2,
                    text: ".".into(),
                });
                editor.input = if editor.input_correction.is_some() {
                    format!("{}.{}", &input[..2], &input[2..])
                } else {
                    input
                };
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
            FlightDataCommand::EditorAction {
                editor_id,
                action_id,
            } if editor_id == BAROMETER_CELL_ID => {
                if action_id == "close" {
                    self.editor = None;
                    return;
                }
                if action_id != "nearest" {
                    return;
                }
                if let (Some(editor), Some(report)) = (self.editor.as_mut(), nearest) {
                    self.setting_inhg = report.setting_inhg;
                    editor.input = format!("{:.2}", self.setting_inhg);
                    editor.input_revision += 1;
                    editor.input_correction = None;
                    editor.error = None;
                }
            }
            _ => {}
        }
    }

    pub fn open_editor(&mut self) {
        if self.available {
            self.editor = Some(FlightDataEditor {
                id: BAROMETER_CELL_ID.into(),
                title: None,
                label: "Altimeter setting".into(),
                unit: "inHg".into(),
                input: format!("{:.2}", self.setting_inhg),
                input_revision: 0,
                input_correction: None,
                error: None,
                notice: "BARO ALT from device is cabin alt. Cross-check.".into(),
                detail: None,
                warning: None,
                action_rows: vec![vec![app_ui_contracts::session::FlightDataEditorAction {
                    id: "nearest".into(),
                    label: "NEAREST".into(),
                    enabled: false,
                    selected: false,
                    secondary_label: None,
                    symbol_feature: None,
                    weather_badge: None,
                    disabled_reason: Some(NO_NEARBY_ALTIMETER.into()),
                }]],
                dismiss_action_id: "close".into(),
                close_label: "CLOSE".to_string(),
            });
        }
    }

    pub fn editor(&self) -> Option<FlightDataEditor> {
        self.editor.clone()
    }

    pub fn close_editor(&mut self) {
        self.editor = None;
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
        Some(pressure_altitude(pressure, self.setting_inhg))
    }

    pub fn vertical_speed_fpm(&self, now: i64) -> Option<f64> {
        self.altitude_ft(now)?;
        crate::ownship::estimate_vertical_speed_fpm(
            self.pressure_history
                .iter()
                .map(|&(time, altitude)| (time, altitude, Some(3.0))),
            self.sample?.0,
        )
    }

    fn setting_warning(&self, now: i64, nearest: Option<&NearbyAltimeter>) -> Option<String> {
        let report = nearest?;
        self.altitude_ft(now)?;
        let discrepancy = (self.setting_inhg * 100.0).round() as i32
            - (report.setting_inhg * 100.0).round() as i32;
        if discrepancy.abs() <= 10 {
            return None;
        }
        Some(format!(
            "Check baro setting:\n{} {:.2} {} min ago",
            report.station,
            report.setting_inhg,
            (now - report.observed_epoch_ms) / 60_000,
        ))
    }

    pub fn next_refresh(&self, now: i64) -> Option<i64> {
        self.sample
            .map(|(time, _)| time.saturating_add(SAMPLE_LIFETIME_MS))
            .filter(|deadline| *deadline > now)
    }
}

fn pressure_altitude(pressure: f64, setting_inhg: f64) -> f64 {
    44_330.0 / 0.3048 * (1.0 - (pressure / (setting_inhg * HPA_PER_INHG)).powf(1.0 / 5.255))
}

#[cfg(test)]
mod tests {
    #[test]
    fn four_digit_setting_entry_inserts_decimal_before_applying() {
        let now = 2_000_000_000_000;
        let mut baro = Barometer::default();
        observe(&mut baro, Some(1002.64), now);
        baro.open_editor();
        for input in ["3", "30", "300", "3006"] {
            baro.apply(
                FlightDataCommand::SetInput {
                    editor_id: "barometer".into(),
                    input: input.into(),
                },
                now,
                None,
            );
        }
        assert_eq!(baro.editor().unwrap().input, "30.06");
        assert!(baro.editor().unwrap().error.is_none());
        assert_eq!(
            baro.editor().unwrap().input_correction,
            Some(app_ui_contracts::session::FlightDataInputCorrection {
                source: "3006".into(),
                start: 2,
                end: 2,
                text: ".".into(),
            })
        );
        baro.close_editor();
        baro.open_editor();
        assert_eq!(baro.editor().unwrap().input, "30.06");
        assert!(baro.editor().unwrap().input_correction.is_none());
        for input in ["30.06", "29.", "2500", "3500"] {
            baro.apply(
                FlightDataCommand::SetInput {
                    editor_id: "barometer".into(),
                    input: input.into(),
                },
                now,
                None,
            );
            assert!(baro.editor().unwrap().error.is_none(), "{input}");
            if input.contains('.') {
                assert_eq!(baro.editor().unwrap().input, input);
                assert!(baro.editor().unwrap().input_correction.is_none());
            }
        }
    }

    use super::*;

    #[test]
    fn discrepant_setting_warns_on_cell_and_in_editor_without_changing_setting() {
        let now = 1_700_000_000_000;
        let mut baro = Barometer::default();
        baro.apply(
            FlightDataCommand::Observe {
                available: true,
                pressure_hpa: Some(1000.0),
                observed_epoch_ms: now,
                received_epoch_ms: now,
            },
            now,
            None,
        );
        let report = NearbyAltimeter {
            station: "KSEA".into(),
            position: LatLon {
                lat: 47.45,
                lon: -122.3,
            },
            weather_badge: None,
            setting_inhg: 30.12,
            distance_nm: 10.0,
            observed_epoch_ms: now - 60_000,
        };
        let warning = baro
            .reading(now, Some(&report))
            .unwrap()
            .warning
            .expect("BARO cell must explain a discrepant setting");
        assert_eq!(warning, "Check baro setting:\nKSEA 30.12 1 min ago");
        baro.open_editor();
        let reading = baro.reading(now, Some(&report)).unwrap();
        let editor = reading.editor.unwrap();
        assert_eq!(editor.warning.as_deref(), Some(warning.as_str()));
        assert_eq!(
            editor.notice,
            "BARO ALT from device is cabin alt. Cross-check."
        );
        assert!(editor.title.is_none());
        assert_eq!(editor.unit, "inHg");
        assert_eq!(editor.input, "29.92");
        assert!(baro.reading(now, None).unwrap().warning.is_none());
        assert!(
            baro.altitude_ft(now).is_some(),
            "weather loss cannot disable altitude"
        );
    }

    #[test]
    fn warning_threshold_is_strictly_more_than_ten_hundredths_and_setting_change_clears_it() {
        let now = 1_700_000_000_000;
        let mut baro = Barometer::default();
        baro.apply(
            FlightDataCommand::Observe {
                available: true,
                pressure_hpa: Some(1000.0),
                observed_epoch_ms: now,
                received_epoch_ms: now,
            },
            now,
            None,
        );
        for (setting, warned) in [(30.02, false), (29.82, false), (30.03, true), (29.81, true)] {
            let report = NearbyAltimeter {
                station: "KSEA".into(),
                position: LatLon {
                    lat: 47.45,
                    lon: -122.3,
                },
                weather_badge: None,
                setting_inhg: setting,
                distance_nm: 10.0,
                observed_epoch_ms: now,
            };
            assert_eq!(
                baro.reading(now, Some(&report)).unwrap().warning.is_some(),
                warned
            );
        }
        let report = NearbyAltimeter {
            station: "KSEA".into(),
            position: LatLon {
                lat: 47.45,
                lon: -122.3,
            },
            weather_badge: None,
            setting_inhg: 30.12,
            distance_nm: 10.0,
            observed_epoch_ms: now,
        };
        baro.open_editor();
        baro.apply(
            FlightDataCommand::EditorAction {
                editor_id: "barometer".into(),
                action_id: "nearest".into(),
            },
            now,
            Some(&report),
        );
        assert!(baro.reading(now, Some(&report)).unwrap().warning.is_none());
    }

    #[test]
    fn changing_kollsman_does_not_become_a_vertical_speed_and_gaps_reset_history() {
        let now = 1_700_000_000_000;
        let mut baro = Barometer::default();
        for second in 0..=20 {
            let time = now + second * 1000;
            baro.apply(
                FlightDataCommand::Observe {
                    available: true,
                    pressure_hpa: Some(1000.0),
                    observed_epoch_ms: time,
                    received_epoch_ms: time,
                },
                time,
                None,
            );
        }
        assert!(baro.vertical_speed_fpm(now + 20_000).unwrap().abs() < 0.01);
        baro.open_editor();
        baro.apply(
            FlightDataCommand::SetInput {
                editor_id: "barometer".into(),
                input: "30.42".into(),
            },
            now + 20_000,
            None,
        );
        assert!(baro.vertical_speed_fpm(now + 20_000).unwrap().abs() < 0.01);
        let time = now + 26_000;
        baro.apply(
            FlightDataCommand::Observe {
                available: true,
                pressure_hpa: Some(1010.0),
                observed_epoch_ms: time,
                received_epoch_ms: time,
            },
            time,
            None,
        );
        assert!(baro.vertical_speed_fpm(time).is_none());
    }

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
            FlightDataCommand::SetInput {
                editor_id: "barometer".into(),
                input: "oops".into(),
            },
            now,
            None,
        );
        let payload = weather(&[("TEST", 0.0, 60_000, "A2997")], now);
        let nearest = nearest_altimeter(Some(LatLon { lat: 0.0, lon: 0.0 }), Some(&payload), now);
        let editor = baro.reading(now, nearest.as_ref()).unwrap().editor.unwrap();
        assert!(editor.action_rows[0][0].enabled);
        assert!(editor.action_rows[0][0].disabled_reason.is_none());
        assert_eq!(
            editor.action_rows[0][0].secondary_label.as_deref(),
            Some("TEST 1min old")
        );
        assert!(editor.detail.is_none());
        baro.apply(
            FlightDataCommand::EditorAction {
                editor_id: "barometer".into(),
                action_id: "nearest".into(),
            },
            now,
            nearest.as_ref(),
        );
        assert_eq!(baro.editor().unwrap().input, "29.97");
        assert_eq!(baro.editor().unwrap().input_revision, 1);
        assert!(baro.editor().unwrap().error.is_none());
        assert!((baro.altitude_ft(now).unwrap() - 336.0).abs() < 1.0);
        let editor = baro.reading(now, None).unwrap().editor.unwrap();
        assert!(!editor.action_rows[0][0].enabled);
        assert_eq!(
            editor.action_rows[0][0].disabled_reason.as_deref(),
            Some(NO_NEARBY_ALTIMETER)
        );
        let before = baro.clone();
        baro.apply(
            FlightDataCommand::EditorAction {
                editor_id: "barometer".into(),
                action_id: "nearest".into(),
            },
            now,
            None,
        );
        assert_eq!(baro, before);
    }

    fn observe(baro: &mut Barometer, pressure: Option<f64>, time: i64) {
        baro.apply(
            FlightDataCommand::Observe {
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
            FlightDataCommand::SetInput {
                editor_id: "barometer".into(),
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
            FlightDataCommand::Observe {
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
        for input in ["", "2499", "3501", "NaN", "-3", "100", "30060"] {
            baro.apply(
                FlightDataCommand::SetInput {
                    editor_id: "barometer".into(),
                    input: input.into(),
                },
                10_000,
                None,
            );
            assert!(baro.editor().unwrap().error.is_some());
            assert_eq!(baro.altitude_ft(10_000), original);
        }
        baro.apply(
            FlightDataCommand::SetInput {
                editor_id: "barometer".into(),
                input: "30.12".into(),
            },
            10_000,
            None,
        );
        assert!(baro.editor().unwrap().error.is_none());
        assert!(baro.altitude_ft(10_000).unwrap() > original.unwrap());
    }
}
