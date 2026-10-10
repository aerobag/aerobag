// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use super::{
    weather::decode_weather, DecodeError, Reader, Result, Weather, WeatherKind, MAX_PRODUCT_BYTES,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Product {
    Attitude(Attitude),
    /// Units not independently established. Deliberately not a BARO input.
    PressureUnverified {
        raw: Option<f64>,
    },
    AttitudeSource {
        external: bool,
        raw: u8,
    },
    GpsSource {
        product_id: u16,
    },
    Nmea {
        sentence: String,
    },
    Traffic(Traffic),
    Weather(Weather),
    Uninterpreted {
        message_id: u32,
        length: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attitude {
    pub stage: u8,
    pub stage_percent: u8,
    pub status: u16,
    pub track_or_heading_degrees: Option<f64>,
    pub pitch_degrees: Option<f64>,
    pub roll_degrees: Option<f64>,
    pub lateral_acceleration_raw: i16,
    pub normal_acceleration_raw: i16,
    pub turn_rate_raw: i16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficOwnship {
    pub time_of_applicability_seconds: Option<f64>,
    pub utc: Option<chrono::DateTime<chrono::Utc>>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub ground_speed_knots: Option<f64>,
    pub true_track_degrees: Option<f64>,
    pub true_heading_degrees: Option<f64>,
    pub pressure_altitude_feet: Option<f64>,
    pub geometric_altitude_feet: Option<f64>,
    pub address: u32,
    pub callsign: String,
    pub flight_plan_id: String,
    pub airborne: bool,
    pub validity_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficTarget {
    pub receiver_track_id: u32,
    pub address: u32,
    pub address_qualifier_raw: u8,
    pub track_source_raw: u8,
    pub data_link_raw: u8,
    pub correlation_flags: u8,
    pub airborne: bool,
    pub state_age_seconds: Option<f64>,
    pub identity_age_seconds: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub ground_speed_knots: Option<f64>,
    pub pressure_altitude_feet: Option<f64>,
    pub geometric_altitude_feet: Option<f64>,
    pub true_direction_degrees: Option<f64>,
    pub true_direction_datum_raw: u8,
    pub vertical_velocity_feet_per_minute: Option<f64>,
    pub vertical_velocity_source_raw: u8,
    pub callsign: Option<String>,
    pub flight_plan_id: Option<String>,
    pub validity_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Traffic {
    pub ads_status: u8,
    pub tcas_installed: bool,
    pub tcas_mode: u8,
    pub age_seconds: Option<f64>,
    pub arrival_seconds: Option<f64>,
    pub airborne_status: u8,
    pub csa_status: u8,
    pub surface_status: u8,
    pub ownship: TrafficOwnship,
    pub relative_angle_datum_raw: u8,
    pub in_tisb_adsr_coverage: bool,
    pub targets: Vec<TrafficTarget>,
}

pub fn decode_product(message_id: u32, bytes: &[u8]) -> Result<Product> {
    if bytes.len() > MAX_PRODUCT_BYTES {
        return Err(DecodeError("product size limit"));
    }
    let mut input = Reader::new(bytes);
    match message_id {
        0x2100 | 0x2101 => {
            if bytes.len() != 16 {
                return Err(DecodeError("attitude length"));
            }
            let stage = input.u8()?;
            let stage_percent = input.u8()?;
            let status = input.u16()?;
            let track = input.i16()? as f64 * 180.0 / 32768.0;
            let pitch = input.i16()? as f64 * 180.0 / 32768.0;
            let roll = input.i16()? as f64 * 180.0 / 32768.0;
            Ok(Product::Attitude(Attitude {
                stage,
                stage_percent,
                status,
                track_or_heading_degrees: (status & 1 != 0).then_some(track),
                pitch_degrees: (status & 2 != 0).then_some(pitch),
                roll_degrees: (status & 4 != 0).then_some(roll),
                lateral_acceleration_raw: input.i16()?,
                normal_acceleration_raw: input.i16()?,
                turn_rate_raw: input.i16()?,
            }))
        }
        0x2200 => {
            if bytes.len() != 4 {
                return Err(DecodeError("pressure length"));
            }
            Ok(Product::PressureUnverified { raw: input.f32()? })
        }
        0x2110 => {
            if bytes.len() != 1 {
                return Err(DecodeError("attitude source length"));
            }
            Ok(Product::AttitudeSource {
                external: bytes[0] != 0,
                raw: bytes[0],
            })
        }
        0x2010 => {
            if bytes.len() != 2 {
                return Err(DecodeError("GPS source length"));
            }
            Ok(Product::GpsSource {
                product_id: input.u16()?,
            })
        }
        0x2001 | 0x2003 | 0x2005 if !bytes.is_empty() => {
            let sentence = std::str::from_utf8(bytes)
                .map_err(|_| DecodeError("NMEA encoding"))?
                .trim_matches(['\0', ' ', '\r', '\n']);
            let (body, check) = sentence
                .strip_prefix('$')
                .and_then(|value| value.split_once('*'))
                .ok_or(DecodeError("NMEA framing"))?;
            if check.len() != 2 || !body.is_ascii() || body.len() > 1024 {
                return Err(DecodeError("NMEA format"));
            }
            let expected =
                u8::from_str_radix(check, 16).map_err(|_| DecodeError("NMEA checksum encoding"))?;
            if body.bytes().fold(0u8, |sum, b| sum ^ b) != expected {
                return Err(DecodeError("NMEA checksum"));
            }
            Ok(Product::Nmea {
                sentence: sentence.to_string(),
            })
        }
        0x20000000 => decode_traffic(bytes).map(Product::Traffic),
        id if WeatherKind::from_message_id(id).is_some() => {
            decode_weather(WeatherKind::from_message_id(id).unwrap(), bytes).map(Product::Weather)
        }
        _ => Ok(Product::Uninterpreted {
            message_id,
            length: bytes.len(),
        }),
    }
}

fn position(lat: f64, lon: f64, valid: bool) -> (Option<f64>, Option<f64>) {
    let (lat, lon) = (lat.to_degrees(), lon.to_degrees());
    if valid && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        (Some(lat), Some(lon))
    } else {
        (None, None)
    }
}

fn valid(flags: u32, bit: u32, value: Option<f64>) -> Option<f64> {
    if flags & bit != 0 {
        value
    } else {
        None
    }
}

fn ownship(input: &mut Reader<'_>) -> Result<TrafficOwnship> {
    let time_of_applicability_seconds = input.f32()?;
    let month = input.u8()?;
    let day = input.u8()?;
    let year = input.u16()?;
    let hour = input.i16()?;
    let minute = input.u8()?;
    let second = input.u8()?;
    let utc = chrono::NaiveDate::from_ymd_opt(year.into(), month.into(), day.into())
        .and_then(|date| date.and_hms_opt(hour as u32, minute.into(), second.into()))
        .map(|date| date.and_utc());
    let lat = input.f64()?;
    let lon = input.f64()?;
    let mut values = [None; 9];
    for value in &mut values {
        *value = input.f32()?;
    }
    let address = input.u32()?;
    input.take(1)?;
    let callsign = input.text(8)?;
    let flight_plan_id = input.text(8)?;
    let airborne = input.u8()? != 0;
    input.take(2)?;
    let validity_flags = input.u32()?;
    let (latitude, longitude) = position(lat, lon, validity_flags & 1 != 0);
    Ok(TrafficOwnship {
        time_of_applicability_seconds,
        utc,
        latitude,
        longitude,
        ground_speed_knots: valid(validity_flags, 2, values[0]).filter(|v| *v >= 0.0),
        true_track_degrees: valid(validity_flags, 4, values[1]),
        true_heading_degrees: valid(validity_flags, 64, values[2]),
        pressure_altitude_feet: valid(validity_flags, 8, values[3]),
        geometric_altitude_feet: valid(validity_flags, 32, values[4]),
        address,
        callsign,
        flight_plan_id,
        airborne,
        validity_flags,
    })
}

fn target(input: &mut Reader<'_>) -> Result<TrafficTarget> {
    let receiver_track_id = input.u32()?;
    let address = input.u32()?;
    let track_source_raw = input.u8()?;
    let data_link_raw = input.u8()?;
    let address_qualifier_raw = input.u8()?;
    let airborne = input.u8()? != 0;
    let state_age_seconds = input.f32()?.filter(|age| *age >= 0.0);
    let identity_age_seconds = input.f32()?.filter(|age| *age >= 0.0);
    input.take(8)?;
    let lat = input.f64()?;
    let lon = input.f64()?;
    let mut values = [None; 16];
    for value in &mut values {
        *value = input.f32()?;
    }
    let meta = input.take(12)?;
    input.take(8)?;
    let callsign = input.text(8)?;
    let flight_plan_id = input.text(8)?;
    let validity_flags = input.u32()?;
    let (latitude, longitude) = position(lat, lon, validity_flags & 64 != 0);
    Ok(TrafficTarget {
        receiver_track_id,
        address,
        address_qualifier_raw,
        track_source_raw,
        data_link_raw,
        correlation_flags: meta[10],
        airborne,
        state_age_seconds,
        identity_age_seconds,
        latitude,
        longitude,
        ground_speed_knots: valid(validity_flags, 2, values[8]).filter(|v| *v >= 0.0),
        pressure_altitude_feet: valid(validity_flags, 2048, values[1]),
        geometric_altitude_feet: valid(validity_flags, 4096, values[2]),
        true_direction_degrees: values[5],
        true_direction_datum_raw: meta[1],
        vertical_velocity_feet_per_minute: values[11],
        vertical_velocity_source_raw: meta[3],
        callsign: (validity_flags & 512 != 0).then_some(callsign),
        flight_plan_id: (validity_flags & 1024 != 0).then_some(flight_plan_id),
        validity_flags,
    })
}

fn decode_traffic(bytes: &[u8]) -> Result<Traffic> {
    let mut input = Reader::new(bytes);
    if input.u16()? != 1 || input.u16()? != 0 {
        return Err(DecodeError("unsupported traffic version"));
    }
    let ads_status = input.u8()?;
    let tcas_installed = input.u8()? != 0;
    let tcas_mode = input.u8()?;
    input.take(1)?;
    let age_seconds = input.f32()?.filter(|age| *age >= 0.0);
    let arrival_seconds = input.f32()?;
    let airborne_status = input.u8()?;
    let csa_status = input.u8()?;
    let surface_status = input.u8()?;
    input.take(1)?;
    let ownship = ownship(&mut input)?;
    let relative_angle_datum_raw = input.u8()?;
    let in_tisb_adsr_coverage = input.u8()? != 0;
    input.take(2)?;
    let count = input.u32()? as usize;
    if count > 4096 || bytes.len() != 120 + count * 148 {
        return Err(DecodeError("traffic target count/length"));
    }
    let targets = (0..count)
        .map(|_| target(&mut input))
        .collect::<Result<Vec<_>>>()?;
    Ok(Traffic {
        ads_status,
        tcas_installed,
        tcas_mode,
        age_seconds,
        arrival_seconds,
        airborne_status,
        csa_status,
        surface_status,
        ownship,
        relative_angle_datum_raw,
        in_tisb_adsr_coverage,
        targets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_attitude_angles_are_unavailable_not_zero_attitude() {
        let Product::Attitude(sample) = decode_product(0x2100, &[0; 16]).unwrap() else {
            panic!()
        };
        assert_eq!(sample.roll_degrees, None);
        assert_eq!(sample.pitch_degrees, None);
        assert_eq!(sample.track_or_heading_degrees, None);
    }

    #[test]
    fn traffic_uses_validity_not_merely_finite_numbers() {
        let mut bytes = vec![0u8; 268];
        bytes[0] = 1;
        bytes[116..120].copy_from_slice(&1u32.to_le_bytes());
        let Product::Traffic(traffic) = decode_product(0x20000000, &bytes).unwrap() else {
            panic!()
        };
        assert_eq!(traffic.ownship.latitude, None);
        assert_eq!(traffic.ownship.pressure_altitude_feet, None);
        assert_eq!(traffic.targets[0].latitude, None);
        assert_eq!(traffic.targets[0].callsign, None);
        for length in 0..bytes.len() {
            assert!(decode_product(0x20000000, &bytes[..length]).is_err());
        }
        bytes.push(0);
        assert!(decode_product(0x20000000, &bytes).is_err());
    }

    #[test]
    fn unknown_message_is_inspectable_and_pressure_units_are_not_guessed() {
        assert!(matches!(
            decode_product(0xdeadbeef, &[1]).unwrap(),
            Product::Uninterpreted { .. }
        ));
        assert!(matches!(
            decode_product(0x2200, &1234f32.to_le_bytes()).unwrap(),
            Product::PressureUnverified { raw: Some(1234.0) }
        ));
    }
}
