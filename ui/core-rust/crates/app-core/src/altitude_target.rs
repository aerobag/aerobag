// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{barometer::Barometer, FlightDataCell, LatLon, OwnshipState};
use app_ui_contracts::session::{
    FlightDataAttention, FlightDataCellAction, FlightDataEditor, FlightDataEditorAction,
    GeographicAnnotationPoint, GeographicLineAnnotation,
};

pub(crate) const TARGET_CELL_ID: &str = "altitude_target";
const CAPTURE_BAND_FT: f64 = 100.0;
const BLINK_DURATION_MS: i64 = 6_000;
const BLINK_HALF_PERIOD_MS: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reference {
    Gps,
    Barometer,
}

impl Reference {
    fn label(self) -> &'static str {
        match self {
            Self::Gps => "GPS",
            Self::Barometer => "BARO",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Prediction {
    seconds: f64,
    distance_nm: f64,
    position: LatLon,
    track_deg: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AltitudeTarget {
    reference: Option<Reference>,
    altitude_ft: Option<f64>,
    editor: Option<FlightDataEditor>,
    prediction: Option<Prediction>,
    approach_started: Option<i64>,
    attention: Option<FlightDataAttention>,
    source_id: Option<crate::OwnshipSourceId>,
}

impl AltitudeTarget {
    fn reference(&self, barometer: &Barometer) -> Reference {
        self.reference.unwrap_or(if barometer.available() {
            Reference::Barometer
        } else {
            Reference::Gps
        })
    }

    pub fn open_editor(&mut self, barometer: &Barometer) {
        self.reference = Some(self.reference(barometer));
        self.editor = Some(FlightDataEditor {
            id: TARGET_CELL_ID.into(),
            title: Some("Target altitude".into()),
            label: "Target altitude".into(),
            unit: "ft".into(),
            input: self
                .altitude_ft
                .map(|value| format!("{value:.0}"))
                .unwrap_or_default(),
            input_revision: 0,
            input_correction: None,
            error: None,
            notice: String::new(),
            detail: None,
            warning: None,
            action_rows: vec![],
            dismiss_action_id: "close".into(),
            close_label: "CLOSE".into(),
        });
    }

    pub fn close_editor(&mut self) {
        self.editor = None;
    }

    pub fn set_input(&mut self, input: String) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.input = input;
        match editor.input.trim().parse::<f64>() {
            Ok(value)
                if value.is_finite()
                    && (-1_000.0..=60_000.0).contains(&value)
                    && value.fract() == 0.0 =>
            {
                if self.altitude_ft != Some(value) {
                    self.approach_started = None;
                }
                self.altitude_ft = Some(value);
                editor.error = None;
            }
            _ => editor.error = Some("Enter a whole altitude from -1000 to 60000 ft.".into()),
        }
    }

    pub fn action(&mut self, id: &str, barometer: &Barometer) {
        if self.editor.is_none() {
            return;
        }
        match id {
            "close" => self.close_editor(),
            "off" => {
                self.altitude_ft = None;
                self.approach_started = None;
                self.close_editor();
            }
            "gps" | "baro" => {
                if id == "baro" && !barometer.available() {
                    return;
                }
                self.reference = Some(if id == "gps" {
                    Reference::Gps
                } else {
                    Reference::Barometer
                });
                self.approach_started = None;
            }
            "increase" | "decrease" => {
                let value = (self.altitude_ft.unwrap_or(0.0)
                    + if id == "increase" { 100.0 } else { -100.0 })
                .clamp(-1_000.0, 60_000.0);
                self.set_input(format!("{value:.0}"));
                if let Some(editor) = self.editor.as_mut() {
                    editor.input_revision += 1;
                }
            }
            _ => {}
        }
    }

    pub fn editor(&self, barometer: &Barometer) -> Option<FlightDataEditor> {
        let mut editor = self.editor.clone()?;
        let reference = self.reference(barometer);
        editor.label = format!("{} target altitude", reference.label());
        editor.notice = match reference {
            Reference::Gps => "GPS MSL altitude prediction only. Use the aircraft altimeter for assigned altitudes and altitude restrictions.",
            Reference::Barometer => "Prediction uses this device's cabin-pressure altitude. Cross-check with the aircraft altimeter.",
        }.into();
        editor.detail = self.prediction.map(|prediction| {
            format!(
                "{:.1} min / {:.1} nm at current vertical speed",
                prediction.seconds / 60.0,
                prediction.distance_nm
            )
        });
        editor.warning = self
            .attention
            .as_ref()
            .map(|attention| attention.message.clone());
        editor.action_rows = vec![
            vec![
                editor_action("gps", "GPS", None, reference == Reference::Gps),
                editor_action(
                    "baro",
                    "BARO",
                    (!barometer.available())
                        .then_some("This device does not provide a barometric pressure sensor."),
                    reference == Reference::Barometer,
                ),
            ],
            vec![
                editor_action(
                    "decrease",
                    "-100",
                    (self.altitude_ft.unwrap_or(0.0) <= -1_000.0)
                        .then_some("Minimum target altitude is -1000 ft."),
                    false,
                ),
                editor_action(
                    "increase",
                    "+100",
                    (self.altitude_ft.unwrap_or(0.0) >= 60_000.0)
                        .then_some("Maximum target altitude is 60000 ft."),
                    false,
                ),
            ],
            vec![editor_action("off", "OFF", None, false)],
        ];
        Some(editor)
    }

    pub fn cell(&self, barometer: &Barometer) -> FlightDataCell {
        let mut cell = crate::flight_data::cell(
            TARGET_CELL_ID,
            &format!("TGT {}", self.reference(barometer).label()),
            Some(
                self.altitude_ft
                    .map(|altitude| format!("{altitude:.0}"))
                    .unwrap_or_else(|| "off".into()),
            ),
        );
        cell.action = Some(FlightDataCellAction {
            action_id: TARGET_CELL_ID.into(),
            accessibility_label: "Set target altitude".into(),
            symbol_id: None,
        });
        cell.attention = self.attention.clone();
        cell
    }

    pub fn refresh(&mut self, ownship: &OwnshipState, barometer: &Barometer, now: i64) -> bool {
        let previous = (
            self.prediction,
            self.approach_started,
            self.attention.clone(),
            self.source_id.clone(),
        );
        if self.source_id != ownship.resolved.active_source_id {
            self.approach_started = None;
            self.source_id = ownship.resolved.active_source_id.clone();
        }
        let reference = self.reference(barometer);
        let vertical = self.altitude_ft.and_then(|_| match reference {
            Reference::Gps => gps_vertical(ownship),
            Reference::Barometer => barometer
                .altitude_ft(now)
                .zip(barometer.vertical_speed_fpm(now)),
        });
        self.prediction = self
            .altitude_ft
            .zip(vertical)
            .and_then(|(target, (altitude, vs))| predict(target, altitude, vs, ownship));
        let seconds = (reference == Reference::Barometer)
            .then(|| self.prediction.map(|prediction| prediction.seconds))
            .flatten();
        self.update_approach(seconds, now);
        self.attention = self.approach_attention(now).or_else(|| (self.altitude_ft.is_some() && (vertical.is_none() || !ownship.render.draw_aircraft)).then(|| FlightDataAttention {
            message: format!("{} prediction unavailable: fresh altitude, position and a vertical trend are required.", reference.label()),
            highlighted: true,
        }));
        previous
            != (
                self.prediction,
                self.approach_started,
                self.attention.clone(),
                self.source_id.clone(),
            )
    }

    fn update_approach(&mut self, seconds: Option<f64>, now: i64) {
        let approaching = seconds.is_some_and(|seconds| {
            seconds > 0.0
                && seconds
                    <= if self.approach_started.is_some() {
                        75.0
                    } else {
                        60.0
                    }
        });
        if approaching {
            self.approach_started.get_or_insert(now);
        } else {
            self.approach_started = None;
        }
    }

    fn approach_attention(&self, now: i64) -> Option<FlightDataAttention> {
        self.approach_started.map(|started| {
            let elapsed = now.saturating_sub(started).max(0);
            FlightDataAttention {
                message: "Approaching target altitude. Cross-check aircraft altimeter.".into(),
                highlighted: elapsed >= BLINK_DURATION_MS
                    || (elapsed / BLINK_HALF_PERIOD_MS) % 2 == 0,
            }
        })
    }

    pub fn next_refresh(&self, now: i64) -> Option<i64> {
        let started = self.approach_started?;
        let elapsed = now.saturating_sub(started).max(0);
        (elapsed < BLINK_DURATION_MS)
            .then(|| started + (elapsed / BLINK_HALF_PERIOD_MS + 1) * BLINK_HALF_PERIOD_MS)
    }

    pub fn annotation(&self) -> Option<GeographicLineAnnotation> {
        let prediction = self.prediction?;
        let point = |bearing| {
            let position = crate::route_destination_point(
                prediction.position,
                bearing,
                prediction.distance_nm,
            );
            GeographicAnnotationPoint {
                lat: position.lat,
                lon: position.lon,
            }
        };
        Some(GeographicLineAnnotation {
            points: (0..=24)
                .map(|index| point(prediction.track_deg - 15.0 + index as f64 * 1.25))
                .collect(),
            label_position: point(prediction.track_deg),
            label_bearing_deg: prediction.track_deg,
            label: format!("{:.0} {}", self.altitude_ft?, self.reference?.label()),
        })
    }
}

fn editor_action(
    id: &str,
    label: &str,
    disabled_reason: Option<&str>,
    selected: bool,
) -> FlightDataEditorAction {
    FlightDataEditorAction {
        id: id.into(),
        label: label.into(),
        enabled: disabled_reason.is_none(),
        selected,
        secondary_label: None,
        symbol_feature: None,
        weather_badge: None,
        disabled_reason: disabled_reason.map(str::to_owned),
    }
}

fn gps_vertical(ownship: &OwnshipState) -> Option<(f64, f64)> {
    let kinematics = ownship.resolved.kinematics.as_ref()?;
    let source = ownship
        .sources
        .iter()
        .find(|source| Some(&source.source_id) == ownship.resolved.active_source_id.as_ref())?;
    let latest = source.recent_samples.last()?;
    if latest
        .vertical_accuracy_m
        .is_some_and(|accuracy| !accuracy.is_finite() || accuracy > 50.0)
    {
        return None;
    }
    // Derive from GPS MSL only: a supplied VSI may be barometric, and pressure
    // altitude must never fill a missing GPS sample in this prediction.
    let mut newer_time = latest.event_time_epoch_ms;
    let samples = source.recent_samples.iter().rev().take_while(|sample| {
        let contiguous = newer_time - sample.event_time_epoch_ms <= source.stale_after_ms;
        newer_time = sample.event_time_epoch_ms;
        contiguous
            && sample.altitude_msl_ft.is_some_and(f64::is_finite)
            && !sample
                .vertical_accuracy_m
                .is_some_and(|accuracy| !accuracy.is_finite() || accuracy > 50.0)
    });
    let vs = crate::ownship::estimate_vertical_speed_fpm(
        samples.filter_map(|sample| {
            Some((
                sample.event_time_epoch_ms,
                sample.altitude_msl_ft?,
                sample.vertical_accuracy_m,
            ))
        }),
        latest.event_time_epoch_ms,
    )?;
    Some((kinematics.altitude_msl_ft?, vs))
}

fn predict(target: f64, altitude: f64, vs: f64, ownship: &OwnshipState) -> Option<Prediction> {
    let render = &ownship.render;
    let delta = target - altitude;
    if !render.draw_aircraft
        || !delta.is_finite()
        || !vs.is_finite()
        || delta.abs() <= CAPTURE_BAND_FT
        || vs.abs() < 100.0
        || delta.signum() != vs.signum()
    {
        return None;
    }
    let position = render.position?;
    let track_deg = render.track_deg_true.filter(|value| value.is_finite())?;
    let speed = render
        .speed_kt
        .filter(|value| value.is_finite() && *value >= 5.0)?;
    if !(-90.0..=90.0).contains(&position.lat) || !(-180.0..=180.0).contains(&position.lon) {
        return None;
    }
    let seconds = delta / vs * 60.0;
    let distance_nm = speed * seconds / 3600.0;
    if seconds > 3600.0 || distance_nm > 200.0 {
        return None;
    }
    Some(Prediction {
        seconds,
        distance_nm,
        position,
        track_deg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ownship() -> OwnshipState {
        let mut state = OwnshipState::default();
        state.render.draw_aircraft = true;
        state.render.position = Some(LatLon {
            lat: 47.0,
            lon: -122.0,
        });
        state.render.track_deg_true = Some(90.0);
        state.render.orientation_deg = Some(30.0);
        state.render.speed_kt = Some(120.0);
        state
    }

    #[test]
    fn descent_predicts_eight_miles_along_track_not_heading() {
        let ownship = ownship();
        let prediction = predict(1500.0, 3500.0, -500.0, &ownship)
            .expect("valid descent must yield an intercept");
        assert_eq!(prediction.seconds, 240.0);
        assert_eq!(prediction.distance_nm, 8.0);
        let target = AltitudeTarget {
            reference: Some(Reference::Gps),
            altitude_ft: Some(1500.0),
            prediction: Some(prediction),
            ..Default::default()
        };
        let annotation = target.annotation().unwrap();
        assert_eq!(annotation.label_bearing_deg, 90.0);
        let center = LatLon {
            lat: annotation.label_position.lat,
            lon: annotation.label_position.lon,
        };
        assert!(
            (crate::geodesy::initial_course_deg(prediction.position, center) - 90.0).abs() < 0.001
        );
        for point in annotation.points {
            assert!(
                (crate::geodesy::great_circle_distance_nm(
                    prediction.position,
                    LatLon {
                        lat: point.lat,
                        lon: point.lon
                    }
                ) - 8.0)
                    .abs()
                    < 0.001
            );
        }
        assert_eq!(annotation.label, "1500 GPS");
    }

    #[test]
    fn climb_works_and_wrong_way_level_capture_and_invalid_inputs_do_not() {
        let mut ownship = ownship();
        assert_eq!(
            predict(5500.0, 3500.0, 500.0, &ownship)
                .unwrap()
                .distance_nm,
            8.0
        );
        for (target, altitude, vs) in [
            (1500.0, 3500.0, 500.0),
            (5500.0, 3500.0, -500.0),
            (1500.0, 3500.0, 0.0),
            (1500.0, 3500.0, -20.0),
            (1500.0, 1550.0, -500.0),
            (f64::NAN, 3500.0, -500.0),
        ] {
            assert!(predict(target, altitude, vs, &ownship).is_none());
        }
        ownship.render.draw_aircraft = false;
        assert!(predict(1500.0, 3500.0, -500.0, &ownship).is_none());
    }

    fn gps_sample(source: &str, time: i64, altitude: Option<f64>) -> crate::SituationSample {
        crate::SituationSample {
            source_id: crate::OwnshipSourceId(source.into()),
            source_kind: crate::OwnshipSourceKind::DeviceGps,
            event_time_epoch_ms: time,
            received_time_epoch_ms: time,
            position: ownship().render.position,
            horizontal_accuracy_m: Some(5.0),
            vertical_accuracy_m: Some(10.0),
            track_deg_true: Some(90.0),
            heading_deg_true: Some(30.0),
            ground_speed_kt: Some(120.0),
            altitude_msl_ft: altitude,
            pressure_altitude_ft: Some(6500.0),
            vertical_speed_fpm: Some(900.0),
        }
    }

    #[test]
    fn gps_uses_only_msl_history_not_native_pressure_vsi_or_pressure_altitude() {
        let mut ownship = OwnshipState::default();
        let now = 1_700_000_000_000;
        for second in 0..=20 {
            ownship = crate::ownship::push_sample(
                &ownship,
                gps_sample(
                    "gps",
                    now + second * 1000,
                    Some(3500.0 - second as f64 * 500.0 / 60.0),
                ),
            );
        }
        let mut target = AltitudeTarget {
            reference: Some(Reference::Gps),
            altitude_ft: Some(1500.0),
            ..Default::default()
        };
        let baro = Barometer::default();
        target.refresh(&ownship, &baro, now + 20_000);
        assert!(
            target.annotation().is_some(),
            "native VSI reports a climb, GPS correctly reports a descent"
        );
        assert!(
            target.approach_started.is_none(),
            "GPS predictions are not barometric altitude alerts"
        );
        ownship = crate::ownship::push_sample(&ownship, gps_sample("gps", now + 21_000, None));
        target.refresh(&ownship, &baro, now + 21_000);
        assert!(
            target.annotation().is_none(),
            "pressure altitude cannot fill missing GPS MSL"
        );
        ownship =
            crate::ownship::push_sample(&ownship, gps_sample("gps", now + 21_500, Some(2000.0)));
        target.refresh(&ownship, &baro, now + 21_500);
        assert!(
            target.annotation().is_none(),
            "missing MSL interrupts the GPS trend rather than creating an altitude jump"
        );
        ownship =
            crate::ownship::push_sample(&ownship, gps_sample("new", now + 22_000, Some(2000.0)));
        ownship = crate::ownship::select_source(
            &ownship,
            crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId("new".into()),
            },
        );
        target.refresh(&ownship, &baro, now + 22_000);
        assert!(
            target.annotation().is_none(),
            "a source switch cannot become a vertical-speed sample"
        );
    }

    #[test]
    fn barometric_reference_survives_sensor_loss_and_weather_is_not_required() {
        let now = 1_700_000_000_000;
        let mut baro = Barometer::default();
        for second in 0..=20 {
            let altitude = 2100.0 - second as f64 * 500.0 / 60.0;
            let pressure =
                29.92 * 33.863_886_666_7 * (1.0 - altitude * 0.3048 / 44330.0).powf(5.255);
            baro.apply(
                crate::FlightDataCommand::Observe {
                    available: true,
                    pressure_hpa: Some(pressure),
                    observed_epoch_ms: now + second * 1000,
                    received_epoch_ms: now + second * 1000,
                },
                now + second * 1000,
                None,
            );
        }
        let mut target = AltitudeTarget::default();
        target.open_editor(&baro);
        target.set_input("1500".into());
        target.refresh(&ownship(), &baro, now + 20_000);
        assert!(target.annotation().is_some());
        assert!(target.cell(&baro).attention.unwrap().highlighted);
        assert_eq!(target.next_refresh(now + 20_000), Some(now + 20_500));
        target.refresh(&ownship(), &baro, now + 20_500);
        assert!(!target.cell(&baro).attention.unwrap().highlighted);
        target.refresh(&ownship(), &baro, now + 25_000);
        assert!(
            target.annotation().is_none(),
            "silent barometer expires without another observation"
        );
        assert_eq!(target.cell(&baro).label, "TGT BARO");
        assert_eq!(target.cell(&baro).value.as_deref(), Some("1500"));
        baro.apply(
            crate::FlightDataCommand::Observe {
                available: false,
                pressure_hpa: None,
                observed_epoch_ms: now + 26_000,
                received_epoch_ms: now + 26_000,
            },
            now + 26_000,
            None,
        );
        target.refresh(&ownship(), &baro, now + 26_000);
        assert_eq!(
            target.cell(&baro).label,
            "TGT BARO",
            "no implicit GPS fallback"
        );
    }

    #[test]
    fn approach_cue_has_hysteresis_and_core_scheduled_finite_blinking() {
        let mut target = AltitudeTarget::default();
        target.update_approach(Some(61.0), 1000);
        assert!(target.approach_attention(1000).is_none());
        target.update_approach(Some(60.0), 2000);
        assert!(target.approach_attention(2000).unwrap().highlighted);
        assert_eq!(target.next_refresh(2000), Some(2500));
        assert!(!target.approach_attention(2500).unwrap().highlighted);
        target.update_approach(Some(62.0), 3000);
        assert_eq!(target.approach_started, Some(2000));
        assert!(target.approach_attention(8000).unwrap().highlighted);
        assert_eq!(target.next_refresh(8000), None);
        target.update_approach(Some(76.0), 9000);
        assert!(target.approach_attention(9000).is_none());
        target.update_approach(Some(59.0), 10000);
        target.update_approach(None, 11000);
        assert!(target.approach_attention(11000).is_none());
    }

    #[test]
    fn disabled_editor_actions_have_core_owned_explanations() {
        let mut baro = Barometer::default();
        let mut target = AltitudeTarget::default();
        target.open_editor(&baro);
        for (altitude, disabled_id, reason) in [
            (
                "1500",
                "baro",
                "This device does not provide a barometric pressure sensor.",
            ),
            ("-1000", "decrease", "Minimum target altitude is -1000 ft."),
            ("60000", "increase", "Maximum target altitude is 60000 ft."),
        ] {
            target.set_input(altitude.into());
            let editor = target.editor(&baro).unwrap();
            for action in editor.action_rows.iter().flatten() {
                assert_eq!(action.enabled, action.disabled_reason.is_none());
            }
            let action = editor
                .action_rows
                .iter()
                .flatten()
                .find(|action| action.id == disabled_id)
                .unwrap();
            assert!(!action.enabled);
            assert_eq!(action.disabled_reason.as_deref(), Some(reason));
        }
        baro.apply(
            crate::barometer::FlightDataCommand::Observe {
                available: true,
                pressure_hpa: Some(1013.25),
                observed_epoch_ms: 1000,
                received_epoch_ms: 1000,
            },
            1000,
            None,
        );
        let editor = target.editor(&baro).unwrap();
        let action = editor
            .action_rows
            .iter()
            .flatten()
            .find(|action| action.id == "baro")
            .unwrap();
        assert!(action.enabled);
        assert!(action.disabled_reason.is_none());
    }

    #[test]
    fn editor_validates_steps_and_preserves_target_on_close_but_off_disables_it() {
        let baro = Barometer::default();
        let mut target = AltitudeTarget::default();
        target.open_editor(&baro);
        target.set_input("1500".into());
        target.action("increase", &baro);
        assert_eq!(target.editor(&baro).unwrap().input, "1600");
        assert_eq!(target.editor(&baro).unwrap().input_revision, 1);
        for invalid in ["NaN", "inf", "60100", "-1100", "1500.5", "junk"] {
            target.set_input(invalid.into());
            assert!(target.editor(&baro).unwrap().error.is_some());
            assert_eq!(target.altitude_ft, Some(1600.0));
        }
        target.action("baro", &baro);
        assert_eq!(target.reference, Some(Reference::Gps));
        target.action("close", &baro);
        assert!(target.editor(&baro).is_none());
        assert_eq!(target.altitude_ft, Some(1600.0));
        target.open_editor(&baro);
        target.action("off", &baro);
        assert_eq!(target.altitude_ft, None);
    }
}
