// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(super) struct Temp(pub std::path::PathBuf);
impl Temp {
    pub fn new() -> Self {
        let mut random = [0u8; 8];
        getrandom::getrandom(&mut random).unwrap();
        Self(std::env::temp_dir().join(format!(
            "aerobag-receiver-{:016x}",
            u64::from_le_bytes(random)
        )))
    }
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn radar() -> super::Radar {
    super::Radar {
        kind: super::WeatherKind::RegionalRadar,
        issue_time: super::PartialUtc {
            day: 10,
            hour: 12,
            minute: 0,
        },
        precipitation_time: super::PartialUtc {
            day: 10,
            hour: 11,
            minute: 58,
        },
        center_latitude: 47.0,
        center_longitude: -122.0,
        reference_latitude: 47.0,
        columns: 3,
        rows: 1,
        pixel_width_meters: 2000,
        pixel_height_meters: 2000,
        grid: vec![0, 24, 6],
    }
}

pub(crate) fn traffic(epoch_ms: i64) -> super::Traffic {
    super::Traffic {
        ads_status: 1,
        tcas_installed: false,
        tcas_mode: 0,
        age_seconds: Some(0.0),
        arrival_seconds: None,
        airborne_status: 0,
        csa_status: 0,
        surface_status: 0,
        relative_angle_datum_raw: 255,
        in_tisb_adsr_coverage: true,
        ownship: super::TrafficOwnship {
            time_of_applicability_seconds: None,
            utc: chrono::DateTime::from_timestamp_millis(epoch_ms),
            latitude: Some(47.45),
            longitude: Some(-122.31),
            ground_speed_knots: Some(120.0),
            true_track_degrees: Some(90.0),
            true_heading_degrees: None,
            pressure_altitude_feet: Some(2000.0),
            geometric_altitude_feet: Some(2400.0),
            address: 1,
            callsign: "TEST".into(),
            flight_plan_id: "".into(),
            airborne: true,
            validity_flags: 1 | 2 | 4 | 8 | 32,
        },
        targets: vec![super::TrafficTarget {
            receiver_track_id: 7,
            address: 2,
            address_qualifier_raw: 255,
            track_source_raw: 255,
            data_link_raw: 255,
            correlation_flags: 0,
            airborne: true,
            state_age_seconds: Some(1.0),
            identity_age_seconds: Some(2.0),
            latitude: Some(47.451),
            longitude: Some(-122.31),
            ground_speed_knots: Some(110.0),
            pressure_altitude_feet: Some(2500.0),
            geometric_altitude_feet: Some(2900.0),
            true_direction_degrees: Some(90.0),
            true_direction_datum_raw: 255,
            vertical_velocity_feet_per_minute: Some(100.0),
            vertical_velocity_source_raw: 255,
            callsign: Some("TARGET".into()),
            flight_plan_id: None,
            validity_flags: 2 | 64 | 512 | 2048 | 4096,
        }],
    }
}
